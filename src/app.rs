use eframe::egui;
use egui::*;
use image::DynamicImage;
use lru::LruCache;
use std::{path::PathBuf, process::exit};
use tracing::{info, instrument, warn};

/// The main application state structure holds all the data needed for the image viewer
pub struct FeriteApp {
    // Image handling components
    /// LRU cache helps manage memory by keeping only the most recently used images
    image_cache: LruCache<PathBuf, DynamicImage>,
    /// Current image being displayed, wrapped in Option since we might not have an image loaded
    current_image: Option<ImageData>,
    /// Path to the current image, useful for displaying filename and handling reloads
    current_path: Option<PathBuf>,

    // UI state components
    /// Zoom level affects how large the image appears (1.0 is actual size)
    zoom_level: f32,
    /// Tracks how far the user has dragged the image from its center position
    drag_offset: Vec2,
    /// Controls visibility of the performance monitoring window
    show_performance: bool,
}

/// Helper structure that keeps together the original image data and its GPU texture
/// The texture is optional because we create it lazily when first rendering
struct ImageData {
    texture: Option<egui::TextureHandle>,
    original: DynamicImage,
}

impl Default for FeriteApp {
    fn default() -> Self {
        Self {
            // Initialize cache with capacity for 5 images
            image_cache: LruCache::new(std::num::NonZeroUsize::new(5).unwrap()),
            current_image: None,
            current_path: None,
            zoom_level: 1.0,
            drag_offset: Vec2::ZERO,
            show_performance: false,
        }
    }
}

impl FeriteApp {
    /// Creates a new instance of the application
    #[instrument(skip(cc))]
    pub fn new(cc: &eframe::CreationContext<'_>, initial_image: Option<PathBuf>) -> Self {
        info!("Initializing Ferrite");

        // Set up custom fonts if needed
        let mut fonts = FontDefinitions::default();
        // Add custom fonts here if desired
        cc.egui_ctx.set_fonts(fonts);

        // Create the application instance
        let mut app = Self::default();

        // If an initial image was provided via command line, load it
        if let Some(path) = initial_image {
            info!("Loading initial image from command line: {:?}", path);
            if path.exists() {
                app.load_image(path);
            } else {
                warn!("Initial image path does not exist: {:?}", path);
            }
        }

        app
    }

    /// Handles loading a new image from a path
    /// The image is stored both in the cache and set as the current image
    #[instrument(skip(self, path))]
    fn load_image(&mut self, path: PathBuf) {
        info!("Loading image: {:?}", path);

        // Check if the image is already in our cache
        if let Some(img) = self.image_cache.get(&path) {
            info!("Image found in cache");
            self.current_image = Some(ImageData {
                texture: None, // Texture will be created on next frame
                original: img.clone(),
            });
            self.current_path = Some(path);
            return;
        }

        // If not in cache, load the new image from disk
        match image::open(&path) {
            Ok(img) => {
                info!("Image loaded successfully");
                self.image_cache.put(path.clone(), img.clone());
                self.current_image = Some(ImageData {
                    texture: None,
                    original: img,
                });
                self.current_path = Some(path);
                // Reset view parameters when loading a new image
                self.zoom_level = 1.0;
                self.drag_offset = Vec2::ZERO;
            }
            Err(e) => {
                warn!("Failed to load image: {}", e);
            }
        }
    }

    /// Handles files being dropped onto the application window
    fn handle_files_dropped(&mut self, _ctx: &egui::Context, files: Vec<PathBuf>) {
        if let Some(path) = files.first() {
            if let Some(extension) = path.extension() {
                // Check if the file has a supported image extension
                if matches!(
                    extension.to_str().map(|s| s.to_lowercase()),
                    Some(ext) if ["jpg", "jpeg", "png", "gif", "bmp"].contains(&ext.as_str())
                ) {
                    self.load_image(path.clone());
                }
            }
        }
    }

    fn render_image(&mut self, ui: &mut Ui) {
        // Handle zooming with Mouse Wheel
        let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);

        if scroll_delta != 0.0 {
            // Calculate zoom factor based on scroll direction
            // Using a smaller factor (0.001) makes zooming smoother
            let zoom_factor = 1.0 - (scroll_delta * 0.001);

            // Apply zoom to current level
            let new_zoom = self.zoom_level * zoom_factor;

            // Get pointer position before applying zoom
            if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
                let screen_rect = ui.clip_rect();
                let screen_center = screen_rect.center().to_vec2();
                let zoom_center = pointer_pos.to_vec2() - screen_center;

                // Update zoom level with clamping to prevent extreme zooms
                self.zoom_level = new_zoom.clamp(0.1, 10.0);

                // Update offset to maintain zoom center position
                self.drag_offset = zoom_center * (1.0 - zoom_factor) + self.drag_offset;
            } else {
                // If no pointer position, just update zoom level
                self.zoom_level = new_zoom.clamp(0.1, 10.0);
            }
        }

        if let Some(image_data) = &mut self.current_image {
            // Create or get the texture for rendering
            let texture: &egui::TextureHandle = match &image_data.texture {
                Some(texture) => texture,
                None => {
                    // Convert image data to a format egui can display
                    let size = [
                        image_data.original.width() as usize,
                        image_data.original.height() as usize,
                    ];
                    let image = image_data.original.to_rgba8();
                    let pixels = image.as_flat_samples();

                    // Create the GPU texture from our image data
                    image_data.texture = Some(ui.ctx().load_texture(
                        "current-image",
                        egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice()),
                        Default::default(),
                    ));
                    image_data.texture.as_ref().unwrap()
                }
            };

            // Calculate the desired display size based on zoom level
            let base_size = texture.size_vec2();
            let size = base_size * self.zoom_level;

            // Create a container for our image that allows for dragging
            egui::CentralPanel::default().show_inside(ui, |ui| {
                // Create a response area that we can use for dragging
                let response = ui.allocate_response(size, Sense::drag());

                // Handle dragging if the response area is being dragged
                if response.dragged() {
                    self.drag_offset += response.drag_delta();
                }

                // Calculate the position for our image based on the center and drag offset
                let rect = response.rect;
                let image_pos = rect.min + self.drag_offset;

                // Paint the image at the calculated position with the specified size
                ui.painter().image(
                    texture.id(),
                    Rect::from_min_size(image_pos, size),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            });
        }
    }
}

impl eframe::App for FeriteApp {
    #[instrument(skip(self, ctx, _frame))]
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle file drops from the operating system
        if !ctx.input(|i| i.raw.dropped_files.is_empty()) {
            let files: Vec<_> = ctx
                .input(|i| i.raw.dropped_files.clone())
                .into_iter()
                .filter_map(|f| f.path)
                .collect();
            self.handle_files_dropped(ctx, files);
        }

        // Main UI layout
        egui::CentralPanel::default().show(ctx, |ui| {
            // Top menu bar
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open...").clicked() {
                        // TODO: Implement file dialog
                    }
                    if ui.button("Toggle Performance").clicked() {
                        self.show_performance = !self.show_performance;
                    }
                });
            });

            // Image display
            self.render_image(ui);
        });

        // Performance monitoring window
        if self.show_performance {
            egui::Window::new("Performance").show(ctx, |ui| {
                ui.label(format!(
                    "Cache size: {}/{}",
                    self.image_cache.len(),
                    self.image_cache.cap()
                ));
                ui.label(format!("Zoom level: {:.2}x", self.zoom_level));
                if let Some(path) = &self.current_path {
                    ui.label(format!(
                        "Current image: {:?}",
                        path.file_name().unwrap_or_default()
                    ));
                }
            });
        }
    }
}
