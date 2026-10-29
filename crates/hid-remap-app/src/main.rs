#![windows_subsystem = "windows"]

slint::include_modules!();

mod standalone_engine;
mod startup;
mod tray;

use hid_remap_core::config::{AppConfig, DeviceConfig, LayoutMode};
use hid_remap_device::{get_keyboard_devices, KeyboardDeviceInfo};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use standalone_engine::{
    reactivate_engine, refresh_hook, set_activity_callback, start_standalone_engine,
    stop_standalone_engine, update_engine_config,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
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

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn is_dir_writable(dir: &Path) -> bool {
    let test_file = dir.join(format!(".test_write_{}.tmp", std::process::id()));
    if let Ok(f) = std::fs::File::create(&test_file) {
        drop(f);
        let _ = std::fs::remove_file(&test_file);
        true
    } else {
        false
    }
}

fn is_file_or_dir_writable(file: &Path, dir: &Path) -> bool {
    if file.exists() {
        std::fs::OpenOptions::new().write(true).open(file).is_ok()
    } else {
        is_dir_writable(dir)
    }
}

fn get_config_file_path() -> PathBuf {
    // 1. If an explicit portable config.toml exists in exe directory and is writable, prioritize it
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_config = exe_dir.join("config.toml");
            if exe_config.exists() {
                if is_file_or_dir_writable(&exe_config, exe_dir) {
                    return exe_config;
                }
            } else if is_dir_writable(exe_dir) {
                return exe_config;
            }
        }
    }

    // 2. Per-user standard directory: %APPDATA%\HIDKeyboardLayoutRemapper\config.toml
    if let Ok(appdata) = std::env::var("APPDATA") {
        let app_dir = PathBuf::from(appdata).join("HIDKeyboardLayoutRemapper");
        let _ = std::fs::create_dir_all(&app_dir);
        let user_config = app_dir.join("config.toml");

        // If user config doesn't exist yet, copy from exe dir template if available
        if !user_config.exists() {
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(exe_dir) = exe_path.parent() {
                    let template = exe_dir.join("config.toml");
                    if template.exists() {
                        let _ = std::fs::copy(&template, &user_config);
                    }
                }
            }
        }
        return user_config;
    }

    // 3. Fallback
    PathBuf::from("config.toml")
}

fn main() -> Result<(), slint::PlatformError> {
    tracing_subscriber::fmt::init();

    // Ensure working directory is set to exe directory if possible
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let _ = std::env::set_current_dir(exe_dir);
        }
    }

    // Prevent multiple instances (single-instance mutex lock)
    let mutex_name = to_wide("Local\\HIDKeyboardLayoutRemapperSingleInstanceMutex");
    let h_mutex = unsafe {
        windows_sys::Win32::System::Threading::CreateMutexW(
            std::ptr::null(),
            1, // take initial ownership
            mutex_name.as_ptr(),
        )
    };

    if h_mutex.is_null()
        || unsafe { windows_sys::Win32::Foundation::GetLastError() }
            == windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS
    {
        info!("Another instance of HID Keyboard Remapper is already running. Signaling existing instance...");
        unsafe {
            let class_name = to_wide("HIDRemapTrayWindowClass");
            let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(
                class_name.as_ptr(),
                std::ptr::null(),
            );
            if !hwnd.is_null() {
                // Post ID_OPEN to the tray window to open/restore settings window
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                    hwnd,
                    windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                    1001,
                    0,
                );
            }
            if !h_mutex.is_null() {
                windows_sys::Win32::Foundation::CloseHandle(h_mutex);
            }
        }
        return Ok(());
    }

    info!("Starting HID Keyboard Remapper Standalone Application with Auto-Detection...");

    let config_path = Rc::new(get_config_file_path());
    info!("Using configuration file: {:?}", config_path);

    // Clean up sample / dummy configs and detect actual hardware
    let mut initial_config = AppConfig::load_from_file(&*config_path).unwrap_or_default();
    initial_config
        .devices
        .retain(|d| !d.instance_id.contains("EXAMPLE"));

    let actual_devices = get_keyboard_devices(&initial_config);
    let connected_ids: Vec<String> = actual_devices
        .iter()
        .filter(|d| d.is_connected)
        .map(|d| d.instance_id.clone())
        .collect();

    for dev in &actual_devices {
        if dev.is_connected && initial_config.find_device(&dev.instance_id).is_none() {
            let mut new_dev = DeviceConfig::new(&dev.instance_id, &dev.name);
            new_dev.layout_mode = LayoutMode::UsToJis; // Default to UsToJis for convenience
            new_dev.ctrl_ime_enabled = true;
            initial_config.upsert_device(new_dev);
        }
    }

    let connected_devs: Vec<&DeviceConfig> = initial_config
        .devices
        .iter()
        .filter(|d| connected_ids.iter().any(|cid| cid.eq_ignore_ascii_case(&d.instance_id)))
        .collect();

    if !connected_devs.is_empty() && connected_devs.iter().all(|d| d.layout_mode == LayoutMode::Passthrough) {
        initial_config.general.default_layout_mode = LayoutMode::Passthrough;
    } else if let Some(active_dev) = connected_devs.iter().find(|d| d.layout_mode != LayoutMode::Passthrough) {
        initial_config.general.default_layout_mode = active_dev.layout_mode;
    }
    let _ = initial_config.save_to_file(&*config_path);

    let config = Rc::new(RefCell::new(initial_config));

    // Start user-mode standalone input remapping engine
    if let Err(e) = start_standalone_engine(config.borrow().clone(), &connected_ids) {
        warn!("Failed to start standalone engine hook: {}", e);
    }

    let main_window = MainWindow::new()?;
    let _ = main_window.hide();

    // Populate initial device list
    let scanned_devices = get_keyboard_devices(&config.borrow());
    let items = build_device_items(&scanned_devices);
    let device_model = Rc::new(VecModel::from(items));
    let initial_enabled = config.borrow().general.enabled;
    let initial_startup = config.borrow().general.start_with_windows;
    main_window.set_devices(ModelRc::from(device_model.clone()));
    main_window.set_global_enabled(initial_enabled);
    main_window.set_start_with_windows(initial_startup);
    main_window.set_engine_status(SharedString::from(
        "Standalone Engine: Active (Zero Install & Hotplug Detection)",
    ));

    // Ensure registry matches config on startup
    if let Err(e) = startup::set_startup_enabled(initial_startup) {
        warn!("Failed to synchronize startup registry setting: {}", e);
    }

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
                *last_connected_ids.borrow_mut() = current_ids.clone();

                let items = build_device_items(&scanned);
                dev_model_pnp.set_vec(items);
                tray::populate_device_cache();
                update_engine_config(current_conf, &current_ids);
                refresh_hook();
            }
        },
    );

    // Toggle global enabled
    let window_weak_toggle = main_window.as_weak();
    let config_path_toggle = config_path.clone();
    main_window.on_toggle_global_enabled({
        let config = config.clone();
        move |enabled| {
            info!("Global remapping enabled changed: {}", enabled);
            config.borrow_mut().general.enabled = enabled;
            let _ = config.borrow().save_to_file(&*config_path_toggle);
            tray::update_tray_enabled(enabled);
            let current_conf = config.borrow().clone();
            let scanned = get_keyboard_devices(&current_conf);
            let current_ids: Vec<String> = scanned
                .iter()
                .filter(|d| d.is_connected)
                .map(|d| d.instance_id.clone())
                .collect();
            update_engine_config(current_conf, &current_ids);
            if let Some(win) = window_weak_toggle.upgrade() {
                win.set_engine_status(SharedString::from(if enabled {
                    "Standalone Engine: Active (リマップ動作中)"
                } else {
                    "Standalone Engine: Disabled (一時停止中)"
                }));
            }
        }
    });

    // Toggle start with windows
    let window_weak_startup = main_window.as_weak();
    let config_path_startup = config_path.clone();
    main_window.on_toggle_start_with_windows({
        let config = config.clone();
        move |enabled| {
            info!("Start with Windows setting changed: {}", enabled);
            config.borrow_mut().general.start_with_windows = enabled;
            let _ = config.borrow().save_to_file(&*config_path_startup);
            if let Err(e) = startup::set_startup_enabled(enabled) {
                warn!("Failed to update startup registry setting: {}", e);
            }
            tray::update_tray_startup(enabled);
            if let Some(win) = window_weak_startup.upgrade() {
                win.set_save_toast_text(SharedString::from(format!(
                    "スタートアップ設定更新: {}",
                    if enabled { "Windows ログイン時に自動起動 (有効)" } else { "自動起動を無効化" }
                )));
                win.set_show_save_toast(true);
            }
        }
    });

    // Change layout mode
    let window_weak_layout = main_window.as_weak();
    let config_path_layout = config_path.clone();
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

                let current_connected = get_keyboard_devices(&conf);
                let connected_ids: Vec<String> = current_connected
                    .iter()
                    .filter(|d| d.is_connected)
                    .map(|d| d.instance_id.clone())
                    .collect();

                let connected_devs: Vec<&DeviceConfig> = conf
                    .devices
                    .iter()
                    .filter(|d| connected_ids.iter().any(|cid| cid.eq_ignore_ascii_case(&d.instance_id)))
                    .collect();

                if !connected_devs.is_empty() && connected_devs.iter().all(|d| d.layout_mode == LayoutMode::Passthrough) {
                    conf.general.default_layout_mode = LayoutMode::Passthrough;
                } else if let Some(active_dev) = connected_devs.iter().find(|d| d.layout_mode != LayoutMode::Passthrough) {
                    conf.general.default_layout_mode = active_dev.layout_mode;
                }

                update_engine_config(conf.clone(), &connected_ids);
                let _ = conf.save_to_file(&*config_path_layout);
                info!("Updated layout mode for {}: {}", item.name, mode_str);

                if let Some(win) = window_weak_layout.upgrade() {
                    win.set_save_toast_text(SharedString::from(format!(
                        "モード変更完了: [{}] -> {}",
                        item.name, mode_str
                    )));
                    win.set_show_save_toast(true);
                    win.set_engine_status(SharedString::from(format!(
                        "Standalone Engine: Active [{:?} 有効]",
                        conf.general.default_layout_mode
                    )));
                }
            }
        }
    });

    // Toggle Ctrl IME
    let window_weak_ime = main_window.as_weak();
    let config_path_ime = config_path.clone();
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

                let current_connected = get_keyboard_devices(&conf);
                let connected_ids: Vec<String> = current_connected
                    .iter()
                    .filter(|d| d.is_connected)
                    .map(|d| d.instance_id.clone())
                    .collect();

                update_engine_config(conf.clone(), &connected_ids);
                let _ = conf.save_to_file(&*config_path_ime);
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
    let config_path_refresh = config_path.clone();
    main_window.on_refresh_devices({
        let config = config.clone();
        let dev_model = device_model.clone();
        move || {
            let conf = config.borrow_mut();
            let scanned = get_keyboard_devices(&conf);
            let new_items = build_device_items(&scanned);
            dev_model.set_vec(new_items);
            let connected_ids: Vec<String> = scanned.iter().filter(|d| d.is_connected).map(|d| d.instance_id.clone()).collect();
            tray::populate_device_cache();
            update_engine_config(conf.clone(), &connected_ids);
            let _ = conf.save_to_file(&*config_path_refresh);
            info!("Manually refreshed keyboard devices ({} found)", scanned.len());

            if let Some(win) = window_weak_refresh.upgrade() {
                win.set_save_toast_text(SharedString::from(format!(
                    "キーボード再スキャン完了: 接続デバイス {} 台を検出 (即時反映済)",
                    scanned.iter().filter(|d| d.is_connected).count()
                )));
                win.set_show_save_toast(true);
            }
        }
    });

    // Reactivate engine button
    let window_weak_reactivate = main_window.as_weak();
    let config_path_reactivate = config_path.clone();
    main_window.on_reactivate_engine({
        let config = config.clone();
        let dev_model = device_model.clone();
        move || {
            let mut conf = config.borrow_mut();
            conf.general.enabled = true;
            let _ = conf.save_to_file(&*config_path_reactivate);
            tray::update_tray_enabled(true);

            let scanned = get_keyboard_devices(&conf);
            let new_items = build_device_items(&scanned);
            dev_model.set_vec(new_items);
            let connected_ids: Vec<String> = scanned
                .iter()
                .filter(|d| d.is_connected)
                .map(|d| d.instance_id.clone())
                .collect();

            tray::populate_device_cache();
            reactivate_engine(conf.clone(), &connected_ids);
            info!("Manually reactivated input remapping and refreshed hook.");

            if let Some(win) = window_weak_reactivate.upgrade() {
                win.set_global_enabled(true);
                win.set_engine_status(SharedString::from(
                    "Standalone Engine: Active (リマップ & フック再有効化完了)",
                ));
                win.set_save_toast_text(SharedString::from(
                    "⚡ リマップとキーボードフックを再有効化しました (フック再接続完了)",
                ));
                win.set_show_save_toast(true);
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
    let window_weak_tray_reactivate = main_window.as_weak();
    let window_weak_tray_toggle = main_window.as_weak();
    let window_weak_tray_startup = main_window.as_weak();

    let tray_initial_enabled = config.borrow().general.enabled;
    let tray_initial_startup = config.borrow().general.start_with_windows;

    let _tray = tray::start_tray_service(
        // on_open (left click, tray menu '設定画面を開く', or 2nd instance activation)
        move || {
            let _ = window_weak_tray_open.upgrade_in_event_loop(|win| {
                let _ = win.show();
                let title = to_wide("HID Keyboard Layout Remapper (Standalone)");
                unsafe {
                    let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(
                        std::ptr::null(),
                        title.as_ptr(),
                    );
                    if !hwnd.is_null() {
                        windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                            hwnd,
                            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOW,
                        );
                        windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                            hwnd,
                            windows_sys::Win32::UI::WindowsAndMessaging::SW_RESTORE,
                        );
                        windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                    }
                }
            });
        },
        // on_reactivate (tray menu '⚡ リマップを再有効化')
        move || {
            let _ = window_weak_tray_reactivate.upgrade_in_event_loop(move |win| {
                win.invoke_reactivate_engine();
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
        // on_toggle_startup (tray menu 'スタートアップ起動')
        move || {
            let _ = window_weak_tray_startup.upgrade_in_event_loop(move |win| {
                let new_state = !win.get_start_with_windows();
                win.set_start_with_windows(new_state);
                win.invoke_toggle_start_with_windows(new_state);
            });
        },
        // on_exit (tray menu '終了')
        move || {
            info!("Exit selected from system tray. Terminating application...");
            stop_standalone_engine();
            let _ = slint::quit_event_loop();
        },
        tray_initial_enabled,
        tray_initial_startup,
    );
    tray::populate_device_cache();

    // Check launch arguments: start hidden in system tray unless --show is explicitly given
    let args: Vec<String> = std::env::args().collect();
    let start_shown = args.iter().any(|a| a == "--show" || a == "-s");

    if start_shown {
        let _ = main_window.show();
    } else {
        let _ = main_window.hide();
        // Also ensure window is hidden right after event loop startup
        let weak_init = main_window.as_weak();
        slint::Timer::single_shot(Duration::from_millis(15), move || {
            if let Some(win) = weak_init.upgrade() {
                let _ = win.hide();
                unsafe {
                    let title = to_wide("HID Keyboard Layout Remapper (Standalone)");
                    let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(
                        std::ptr::null(),
                        title.as_ptr(),
                    );
                    if !hwnd.is_null() {
                        windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                            hwnd,
                            windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE,
                        );
                    }
                }
                info!("Application started resident in system tray (window hidden).");
            }
        });
    }

    info!("Application resident in system tray. Starting persistent event loop...");
    let result = slint::run_event_loop_until_quit();
    stop_standalone_engine();
    unsafe {
        if !h_mutex.is_null() {
            windows_sys::Win32::Foundation::CloseHandle(h_mutex);
        }
    }
    result
}
