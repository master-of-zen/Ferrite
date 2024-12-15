use eframe::Error;
use ferrite_cli::Args;
use ferrite_logging::{init, LogConfig};

mod app;

fn main() -> Result<(), Error> {
    let args = Args::parse();

    // Initialize logging
    init(LogConfig {
        level: args.get_log_level(),
        enable_tracy: true,
    });

    // Handle configuration
    let mut config = args.handle_config().unwrap_or_else(|e| {
        eprintln!(
            "Configuration error: {}. Run with --generate-config to create one.",
            e
        );
        std::process::exit(1);
    });
    args.apply_to_config(&mut config);

    // Start the application
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Ferrite",
        native_options,
        Box::new(move |cc| Box::new(app::FeriteApp::new(cc, args.image_path, config))),
    )
}
