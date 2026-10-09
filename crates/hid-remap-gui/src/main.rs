slint::include_modules!();

use hid_remap_core::config::{AppConfig, DeviceConfig, LayoutMode};
use hid_remap_device::{get_keyboard_devices, KeyboardDeviceInfo};
use hid_remap_service::DriverClient;
use slint::{Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use tracing::{info, warn};

fn to_layout_mode(s: &str) -> LayoutMode {
    match s {
        "us-to-jis" => LayoutMode::UsToJis,
        "jis-to-us" => LayoutMode::JisToUs,
        _ => LayoutMode::Passthrough,
    }
}

fn build_device_items(devices: &[KeyboardDeviceInfo]) -> Vec<DeviceItem> {
    devices
        .iter()
        .map(|d| DeviceItem {
            name: SharedString::from(&d.name),
            instance_id: SharedString::from(&d.instance_id),
            is_connected: d.is_connected,
            layout_mode: SharedString::from(d.layout_mode.as_str()),
            ctrl_ime_enabled: d.ctrl_ime_enabled,
        })
        .collect()
}

fn main() -> Result<(), slint::PlatformError> {
    tracing_subscriber::fmt::init();
    info!("Starting HID Keyboard Remapper GUI...");

    let main_window = MainWindow::new()?;

    let config = Rc::new(RefCell::new(
        AppConfig::load_from_file("config.toml").unwrap_or_default(),
    ));

    // Check driver connection status
    let driver_status = match DriverClient::connect() {
        Ok(driver) => {
            if let Ok((maj, min)) = driver.get_version() {
                format!("Driver: Connected (v{}.{})", maj, min)
            } else {
                "Driver: Connected".to_string()
            }
        }
        Err(_) => "Driver: Not Loaded (User Fallback Mode)".to_string(),
    };
    main_window.set_driver_status(SharedString::from(driver_status));

    // Populate initial device list
    let devices = get_keyboard_devices(&config.borrow());
    let items = build_device_items(&devices);
    let device_model = Rc::new(VecModel::from(items));
    main_window.set_devices(ModelRc::from(device_model.clone()));
    main_window.set_global_enabled(config.borrow().general.enabled);

    // Callbacks

    // Toggle global enabled
    main_window.on_toggle_global_enabled({
        let config = config.clone();
        move |enabled| {
            info!("Global remapping enabled changed: {}", enabled);
            config.borrow_mut().general.enabled = enabled;
        }
    });

    // Change layout mode
    main_window.on_change_device_layout({
        let config = config.clone();
        let dev_model = device_model.clone();
        move |idx, mode_str| {
            let idx = idx as usize;
            if let Some(mut item) = dev_model.row_data(idx) {
                item.layout_mode = mode_str.clone();
                dev_model.set_row_data(idx, item.clone());

                let mode = to_layout_mode(&mode_str);
                let mut conf = config.borrow_mut();
                if let Some(dev) = conf.find_device_mut(&item.instance_id) {
                    dev.layout_mode = mode;
                } else {
                    let mut new_dev = DeviceConfig::new(&item.instance_id, &item.name);
                    new_dev.layout_mode = mode;
                    new_dev.ctrl_ime_enabled = item.ctrl_ime_enabled;
                    conf.upsert_device(new_dev);
                }
                info!("Updated layout for {}: {}", item.name, mode_str);
            }
        }
    });

    // Toggle Ctrl IME
    main_window.on_toggle_device_ctrl_ime({
        let config = config.clone();
        let dev_model = device_model.clone();
        move |idx, enabled| {
            let idx = idx as usize;
            if let Some(mut item) = dev_model.row_data(idx) {
                item.ctrl_ime_enabled = enabled;
                dev_model.set_row_data(idx, item.clone());

                let mut conf = config.borrow_mut();
                if let Some(dev) = conf.find_device_mut(&item.instance_id) {
                    dev.ctrl_ime_enabled = enabled;
                } else {
                    let mut new_dev = DeviceConfig::new(&item.instance_id, &item.name);
                    new_dev.layout_mode = to_layout_mode(&item.layout_mode);
                    new_dev.ctrl_ime_enabled = enabled;
                    conf.upsert_device(new_dev);
                }
                info!("Updated Ctrl IME for {}: {}", item.name, enabled);
            }
        }
    });

    // Refresh devices
    main_window.on_refresh_devices({
        let config = config.clone();
        let dev_model = device_model.clone();
        move || {
            let scanned = get_keyboard_devices(&config.borrow());
            let new_items = build_device_items(&scanned);
            dev_model.set_vec(new_items);
            info!("Refreshed connected keyboard devices ({} found)", scanned.len());
        }
    });

    // Save configuration
    main_window.on_save_configuration({
        let config = config.clone();
        move || {
            let conf = config.borrow();
            if let Err(e) = conf.save_to_file("config.toml") {
                warn!("Failed to save config: {}", e);
            } else {
                info!("Configuration saved to config.toml successfully.");
                if let Ok(driver) = DriverClient::connect() {
                    let _ = driver.push_config(&conf);
                    info!("Pushed updated configuration to KMDF driver.");
                }
            }
        }
    });

    main_window.run()
}

