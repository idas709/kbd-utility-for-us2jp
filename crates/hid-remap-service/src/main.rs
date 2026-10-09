use hid_remap_core::config::AppConfig;
use hid_remap_service::DriverClient;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tracing::{info, warn};

fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting HID Keyboard Remapper Windows Service daemon...");

    let running = Arc::new(AtomicBool::new(true));

    let mut last_modified = std::fs::metadata("config.toml")
        .and_then(|m| m.modified())
        .ok();

    // Initial load and driver push
    if let Ok(config) = AppConfig::load_from_file("config.toml") {
        info!("Initial configuration loaded.");
        if let Ok(driver) = DriverClient::connect() {
            if let Err(e) = driver.push_config(&config) {
                warn!("Failed to push initial config to driver: {}", e);
            } else {
                info!("Pushed initial configuration to KMDF driver.");
            }
        } else {
            info!("Driver not available, waiting for driver or agent...");
        }
    }

    info!("Service running. Monitoring configuration and devices...");

    // Service loop
    while running.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_secs(2));

        // Detect config changes and reload
        let current_modified = std::fs::metadata("config.toml")
            .and_then(|m| m.modified())
            .ok();

        if current_modified.is_some() && current_modified != last_modified {
            last_modified = current_modified;
            info!("Detected change in config.toml, reloading...");
            match AppConfig::load_from_file("config.toml") {
                Ok(new_config) => {
                    info!("Successfully reloaded configuration.");
                    if let Ok(driver) = DriverClient::connect() {
                        let _ = driver.push_config(&new_config);
                        info!("Pushed updated configuration to KMDF driver.");
                    }
                }
                Err(e) => {
                    warn!("Invalid configuration syntax in config.toml: {}", e);
                }
            }
        }
    }

    info!("HID Keyboard Remapper Windows Service stopped.");
}
