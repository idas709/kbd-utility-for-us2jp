mod hook_fallback;

use hid_remap_core::config::AppConfig;
use hid_remap_ime::switch_ime;
use hid_remap_service::DriverClient;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tracing::{error, info, warn};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting HID Keyboard Layout Remapper User Session Agent...");

    let config = AppConfig::load_from_file("config.toml").unwrap_or_default();
    info!(
        "Configuration: enabled={}, layout={}, IME method={:?}, tap_timeout={}ms",
        config.general.enabled,
        config.general.windows_layout,
        config.ime.method,
        config.ime.tap_timeout_ms
    );

    let running = Arc::new(AtomicBool::new(true));

    // Attempt to connect to the KMDF driver
    match DriverClient::connect() {
        Ok(driver) => {
            info!("Successfully connected to KMDF filter driver!");
            if let Ok((major, minor)) = driver.get_version() {
                info!("Driver version: {}.{}", major, minor);
            }

            if let Err(e) = driver.push_config(&config) {
                warn!("Failed to push config to driver: {}", e);
            }

            // Spawn background thread to consume IME events from driver
            let running_clone = running.clone();
            let ime_method = config.ime.method;
            thread::spawn(move || {
                info!("Driver event listener thread started.");
                while running_clone.load(Ordering::Relaxed) {
                    match driver.get_next_ime_event() {
                        Ok(Some(evt)) => {
                            info!(
                                "[Driver Event] IME Tap on device '{}': Set IME to {}",
                                evt.instance_id,
                                if evt.ime_on { "ON" } else { "OFF" }
                            );
                            switch_ime(evt.ime_on, ime_method);
                        }
                        Ok(None) => {
                            // Queue empty, brief sleep
                            thread::sleep(Duration::from_millis(15));
                        }
                        Err(e) => {
                            warn!("Driver communication error: {}", e);
                            thread::sleep(Duration::from_millis(500));
                        }
                    }
                }
            });
        }
        Err(e) => {
            info!(
                "KMDF filter driver not loaded ({}), activating user-mode fallback hook...",
                e
            );
            if let Err(hook_err) = hook_fallback::start_hook(config.ime.tap_timeout_ms, config.ime.method) {
                error!("Failed to start hook fallback: {}", hook_err);
            }
        }
    }

    info!("User Session Agent is running. Processing Windows messages...");

    // Windows Message Loop
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    running.store(false, Ordering::Relaxed);
    hook_fallback::stop_hook();
    info!("User Session Agent stopped.");
}
