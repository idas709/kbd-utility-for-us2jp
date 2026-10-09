slint::include_modules!();

mod standalone_engine;
mod tray;

use hid_remap_core::config::{AppConfig, DeviceConfig, LayoutMode};
use hid_remap_device::{get_keyboard_devices, KeyboardDeviceInfo};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use standalone_engine::{
    set_activity_callback, start_standalone_engine, stop_standalone_engine, update_engine_config,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use tracing::{info, warn};

fn to_layout_mode(s: &str) -> LayoutMode {
    match s {
        "us-to-jis" => LayoutMode::UsToJis,
        "jis-to-us" => LayoutMode::JisToUs,
        _ => LayoutMode::Passthrough,
    }
}

/// Convert devices list to UI items.
/// Only displays currently physically connected keyboards so unplugged devices disappear!
fn build_device_items(devices: &[KeyboardDeviceInfo]) -> Vec<DeviceItem> {
    devices
        .iter()
        .filter(|d| d.is_connected) // Filter: Show connected keyboards only!
        .map(|d| DeviceItem {
            name: SharedString::from(&d.name),
            instance_id: SharedString::from(&d.instance_id),
            is_connected: d.is_connected,
            layout_mode: SharedString::from(d.layout_mode.as_str()),
            ctrl_ime_enabled: d.ctrl_ime_enabled,
            is_active_typing: false,
        })
        .collect()
}

fn main() -> Result<(), slint::PlatformError> {
    tracing_subscriber::fmt::init();
    info!("Starting HID Keyboard Remapper Standalone Application with Auto-Detection...");

    // Clean up sample / dummy configs and detect actual hardware
    let mut initial_config = AppConfig::load_from_file("config.toml").unwrap_or_default();
    initial_config
        .devices
        .retain(|d| !d.instance_id.contains("EXAMPLE"));

    let actual_devices = get_keyboard_devices(&initial_config);
    for dev in &actual_devices {
        if dev.is_connected && initial_config.find_device(&dev.instance_id).is_none() {
            let mut new_dev = DeviceConfig::new(&dev.instance_id, &dev.name);
            new_dev.layout_mode = LayoutMode::UsToJis; // Default to UsToJis for convenience
            new_dev.ctrl_ime_enabled = true;
            initial_config.upsert_device(new_dev);
        }
    }
    if let Some(active_dev) = initial_config.devices.iter().find(|d| d.layout_mode != LayoutMode::Passthrough) {
        initial_config.general.default_layout_mode = active_dev.layout_mode;
    }
    let _ = initial_config.save_to_file("config.toml");

    let config = Rc::new(RefCell::new(initial_config));

    // Start user-mode standalone input remapping engine
    if let Err(e) = start_standalone_engine(config.borrow().clone()) {
        warn!("Failed to start standalone engine hook: {}", e);
    }

    let main_window = MainWindow::new()?;

    // Populate initial device list
    let scanned_devices = get_keyboard_devices(&config.borrow());
    let items = build_device_items(&scanned_devices);
    let device_model = Rc::new(VecModel::from(items));
    main_window.set_devices(ModelRc::from(device_model.clone()));
    main_window.set_global_enabled(config.borrow().general.enabled);
    main_window.set_engine_status(SharedString::from(
        "Standalone Engine: Active (Zero Install & Hotplug Detection)",
    ));

    // Setup live activity callback
    let weak_for_activity = main_window.as_weak();
    set_activity_callback(move |_dev_id, log_text| {
        let _ = weak_for_activity.upgrade_in_event_loop(move |window| {
            window.set_show_save_toast(false);
            window.set_live_input_log(SharedString::from(log_text));
        });
    });

    // PnP Hotplug auto-detection timer (checks connected devices list every 1 second)
    let pnp_timer = slint::Timer::default();
    let config_pnp = config.clone();
    let dev_model_pnp = device_model.clone();
    let last_connected_ids = Rc::new(RefCell::new(
        scanned_devices
            .iter()
            .filter(|d| d.is_connected)
            .map(|d| d.instance_id.clone())
            .collect::<Vec<String>>(),
    ));

    pnp_timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1000),
        move || {
            let current_conf = config_pnp.borrow().clone();
            let scanned = get_keyboard_devices(&current_conf);
            let current_ids: Vec<String> = scanned
                .iter()
                .filter(|d| d.is_connected)
                .map(|d| d.instance_id.clone())
                .collect();

            // Compare connected devices list with previous tick
            if current_ids != *last_connected_ids.borrow() {
                info!(
                    "PnP device change detected! Connected devices changed from {} to {}",
                    last_connected_ids.borrow().len(),
                    current_ids.len()
                );
                *last_connected_ids.borrow_mut() = current_ids;

                let items = build_device_items(&scanned);
                dev_model_pnp.set_vec(items);
            }
        },
    );

    // Toggle global enabled
    let window_weak_toggle = main_window.as_weak();
    main_window.on_toggle_global_enabled({
        let config = config.clone();
        move |enabled| {
            info!("Global remapping enabled changed: {}", enabled);
            config.borrow_mut().general.enabled = enabled;
            tray::update_tray_enabled(enabled);
            update_engine_config(config.borrow().clone());
            if let Some(win) = window_weak_toggle.upgrade() {
                win.set_engine_status(SharedString::from(if enabled {
                    "Standalone Engine: Active (リマップ動作中)"
                } else {
                    "Standalone Engine: Disabled (一時停止中)"
                }));
            }
        }
    });

    // Change layout mode
    let window_weak_layout = main_window.as_weak();
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
                conf.general.default_layout_mode = mode;
                update_engine_config(conf.clone());
                let _ = conf.save_to_file("config.toml");
                info!("Updated layout mode for {}: {}", item.name, mode_str);

                if let Some(win) = window_weak_layout.upgrade() {
                    win.set_save_toast_text(SharedString::from(format!(
                        "モード変更完了: [{}] -> {}",
                        item.name, mode_str
                    )));
                    win.set_show_save_toast(true);
                    win.set_engine_status(SharedString::from(format!(
                        "Standalone Engine: Active [{:?} 有効]",
                        mode
                    )));
                }
            }
        }
    });

    // Toggle Ctrl IME
    let window_weak_ime = main_window.as_weak();
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
                update_engine_config(conf.clone());
                let _ = conf.save_to_file("config.toml");
                info!("Updated Ctrl IME for {}: {}", item.name, enabled);

                if let Some(win) = window_weak_ime.upgrade() {
                    win.set_save_toast_text(SharedString::from(format!(
                        "Ctrl IME設定更新: [{}] -> {}",
                        item.name,
                        if enabled { "ON" } else { "OFF" }
                    )));
                    win.set_show_save_toast(true);
                }
            }
        }
    });

    // Refresh devices button
    let window_weak_refresh = main_window.as_weak();
    main_window.on_refresh_devices({
        let config = config.clone();
        let dev_model = device_model.clone();
        move || {
            let scanned = get_keyboard_devices(&config.borrow());
            let new_items = build_device_items(&scanned);
            dev_model.set_vec(new_items);
            info!("Manually refreshed keyboard devices ({} found)", scanned.len());

            if let Some(win) = window_weak_refresh.upgrade() {
                win.set_save_toast_text(SharedString::from(format!(
                    "キーボード再スキャン完了: 接続デバイス {} 台を検出",
                    scanned.iter().filter(|d| d.is_connected).count()
                )));
                win.set_show_save_toast(true);
            }
        }
    });

    // Save configuration button with visual toast feedback!
    let window_weak_save = main_window.as_weak();
    main_window.on_save_configuration({
        let config = config.clone();
        move || {
            let mut conf = config.borrow_mut();
            if let Some(active_dev) = conf.devices.iter().find(|d| d.layout_mode != LayoutMode::Passthrough) {
                conf.general.default_layout_mode = active_dev.layout_mode;
            }
            let save_res = conf.save_to_file("config.toml");
            update_engine_config(conf.clone());

            if let Some(window) = window_weak_save.upgrade() {
                match save_res {
                    Ok(_) => {
                        let active_count = conf.devices.iter().filter(|d| d.layout_mode != LayoutMode::Passthrough).count();
                        window.set_save_toast_text(SharedString::from(format!(
                            "設定を保存し即座に適用しました！（モード: {:?}, 対象キーボード: {}台）",
                            conf.general.default_layout_mode,
                            active_count
                        )));
                        window.set_show_save_toast(true);
                        window.set_engine_status(SharedString::from(format!(
                            "Standalone Engine: Active [{:?} 有効・設定保存済]",
                            conf.general.default_layout_mode
                        )));
                        info!("Configuration saved to config.toml successfully.");
                    }
                    Err(e) => {
                        window.set_save_toast_text(SharedString::from(format!("保存エラー: {}", e)));
                        window.set_show_save_toast(true);
                        warn!("Failed to save config: {}", e);
                    }
                }
            }
        }
    });

    // Intercept window close ('X' button) to keep running in system tray
    let window_weak_close = main_window.as_weak();
    main_window.window().on_close_requested(move || {
        if let Some(win) = window_weak_close.upgrade() {
            let _ = win.hide();
            info!("Window close ('X') requested -> minimized to system tray.");
        }
        slint::CloseRequestResponse::HideWindow
    });

    // Hide window button in UI
    let window_weak = main_window.as_weak();
    main_window.on_hide_window(move || {
        if let Some(win) = window_weak.upgrade() {
            let _ = win.hide();
            info!("GUI minimized to system tray. Input remapping and Ctrl IME remain active.");
        }
    });

    // Start System Tray service
    let window_weak_tray_open = main_window.as_weak();
    let window_weak_tray_toggle = main_window.as_weak();

    let _tray = tray::start_tray_service(
        // on_open (left click or tray menu '設定画面を開く')
        move || {
            let _ = window_weak_tray_open.upgrade_in_event_loop(|win| {
                let _ = win.show();
            });
        },
        // on_toggle (tray menu 'リマップ有効/無効')
        move || {
            let _ = window_weak_tray_toggle.upgrade_in_event_loop(move |win| {
                let new_state = !win.get_global_enabled();
                win.set_global_enabled(new_state);
                win.invoke_toggle_global_enabled(new_state);
            });
        },
        // on_exit (tray menu '終了')
        move || {
            info!("Exit selected from system tray. Terminating application...");
            stop_standalone_engine();
            let _ = slint::quit_event_loop();
        },
        config.borrow().general.enabled,
    );

    // Check launch arguments: if --show is passed, show settings window on start!
    let args: Vec<String> = std::env::args().collect();
    let start_shown = args.iter().any(|a| a == "--show" || a == "-s");

    if start_shown {
        main_window.show()?;
    }

    info!("Application resident in system tray. Starting persistent event loop...");
    let result = slint::run_event_loop_until_quit();
    stop_standalone_engine();
    result
}
