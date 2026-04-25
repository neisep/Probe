mod app;
mod http_format;
mod oauth;
mod openapi;
mod persistence;
mod runtime;
mod state;
mod ui;

use std::error::Error;

use eframe::egui;
use tracing_subscriber::EnvFilter;

fn main() {
    if let Err(error) = run() {
        eprintln!("probe failed to start: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    init_tracing();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Probe")
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Probe",
        options,
        Box::new(|_creation_context| Ok(Box::new(app::ProbeApp::new()))),
    )?;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
