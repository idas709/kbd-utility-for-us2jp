use hid_remap_core::config::{AppConfig, LayoutMode};
use hid_remap_core::engine::{EngineAction, RemapEngine};
use hid_remap_core::ime::{CtrlTapTracker, TapResult};
use hid_remap_core::keyboard::{KeyDirection, KeyEvent, KeyFlags};
use hid_remap_ime::{switch_ime, EXTRA_INFO_MAGIC};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tracing::info;
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_EXTENDED, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

static HOOK_HANDLE: Mutex<isize> = Mutex::new(0);
static GLOBAL_ENABLED: AtomicBool = AtomicBool::new(true);
static ENGINE_STATE: Mutex<Option<EngineSharedState>> = Mutex::new(None);
static ACTIVITY_CALLBACK: Mutex<Option<Box<dyn Fn(String, String) + Send + Sync>>> = Mutex::new(None);

struct DeviceEngine {
    engine: RemapEngine,
    tap_tracker: CtrlTapTracker,
}

struct EngineSharedState {
    #[allow(dead_code)]
    device_engines: HashMap<String, DeviceEngine>,
    default_engine: DeviceEngine,
    config: AppConfig,
}

pub fn set_activity_callback<F>(cb: F)
where
    F: Fn(String, String) + Send + Sync + 'static,
{
    let mut guard = ACTIVITY_CALLBACK.lock().unwrap();
    *guard = Some(Box::new(cb));
}

fn create_engine_for_mode(mode: LayoutMode, ctrl_ime_enabled: bool, timeout_ms: u32) -> DeviceEngine {
    DeviceEngine {
        engine: RemapEngine::new(mode),
        tap_tracker: CtrlTapTracker::new(ctrl_ime_enabled, timeout_ms),
    }
}

pub fn start_standalone_engine(config: AppConfig) -> Result<(), &'static str> {
    GLOBAL_ENABLED.store(config.general.enabled, Ordering::Relaxed);
    update_engine_state(config);

    unsafe {
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            std::ptr::null_mut(),
            0,
        );
        if hook.is_null() {
            return Err("Failed to install standalone keyboard hook");
        }
        let mut handle = HOOK_HANDLE.lock().unwrap();
        *handle = hook as isize;
        info!("Standalone input engine hook installed.");
        Ok(())
    }
}

pub fn update_engine_config(config: AppConfig) {
    GLOBAL_ENABLED.store(config.general.enabled, Ordering::Relaxed);
    update_engine_state(config);
}

fn update_engine_state(config: AppConfig) {
    let mut device_engines = HashMap::new();
    let timeout = config.ime.tap_timeout_ms;

    // Default layout mode: Use the first real device's mode or UsToJis
    let default_mode = config
        .devices
        .iter()
        .find(|d| !d.instance_id.contains("EXAMPLE"))
        .map(|d| d.layout_mode)
        .unwrap_or(LayoutMode::UsToJis);

    let default_ctrl_ime = config
        .devices
        .iter()
        .find(|d| !d.instance_id.contains("EXAMPLE"))
        .map(|d| d.ctrl_ime_enabled)
        .unwrap_or(true);

    for dev in &config.devices {
        device_engines.insert(
            dev.instance_id.to_uppercase(),
            create_engine_for_mode(dev.layout_mode, dev.ctrl_ime_enabled, timeout),
        );
    }

    let default_engine = create_engine_for_mode(default_mode, default_ctrl_ime, timeout);

    let mut guard = ENGINE_STATE.lock().unwrap();
    *guard = Some(EngineSharedState {
        device_engines,
        default_engine,
        config,
    });
    info!("Engine updated. Default Mode: {:?}, Ctrl IME: {}", default_mode, default_ctrl_ime);
}

pub fn stop_standalone_engine() {
    unsafe {
        let mut handle = HOOK_HANDLE.lock().unwrap();
        let hook = *handle as HHOOK;
        if !hook.is_null() {
            UnhookWindowsHookEx(hook);
            *handle = 0;
            info!("Standalone input engine hook removed.");
        }
    }
}

fn send_synthetic_events(events: &[KeyEvent]) {
    if events.is_empty() {
        return;
    }

    let mut inputs: Vec<INPUT> = Vec::with_capacity(events.len());
    for evt in events {
        let mut flags = KEYEVENTF_SCANCODE;
        if evt.is_up() {
            flags |= KEYEVENTF_KEYUP;
        }
        if evt.flags.is_extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }

        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: evt.scan_code,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: EXTRA_INFO_MAGIC,
                },
            },
        });
    }

    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let kbd = &*(l_param as *const KBDLLHOOKSTRUCT);

        // Ignore synthetic inputs to prevent feedback loops
        if kbd.dwExtraInfo == EXTRA_INFO_MAGIC {
            return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
        }

        if GLOBAL_ENABLED.load(Ordering::Relaxed) {
            let msg = w_param as u32;
            let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

            if is_down || is_up {
                let is_extended = (kbd.flags & LLKHF_EXTENDED) != 0;
                let scan_code = kbd.scanCode as u16;

                let event = KeyEvent::new(
                    scan_code,
                    if is_down {
                        KeyDirection::Down
                    } else {
                        KeyDirection::Up
                    },
                    KeyFlags {
                        is_extended,
                        is_extended1: false,
                    },
                    kbd.time,
                );

                let mut trigger_ime: Option<(bool, hid_remap_core::config::ImeMethod)> = None;
                let mut remapped_events: Option<Vec<KeyEvent>> = None;
                let mut activity_info: Option<(String, String)> = None;

                if let Ok(mut guard) = ENGINE_STATE.lock() {
                    if let Some(state) = guard.as_mut() {
                        // Use default engine (or device-specific if identified)
                        let dev_engine = &mut state.default_engine;

                        // 1. Process Ctrl single-tap IME gesture
                        let tap_res = dev_engine.tap_tracker.process_event(&event);
                        match tap_res {
                            TapResult::LeftCtrlTap => {
                                trigger_ime = Some((false, state.config.ime.method));
                            }
                            TapResult::RightCtrlTap => {
                                trigger_ime = Some((true, state.config.ime.method));
                            }
                            TapResult::None => {}
                        }

                        // 2. Process Layout Remapping
                        let action = dev_engine.engine.process(event);
                        match action {
                            EngineAction::Emit(evts) => {
                                if evts.len() != 1 || evts[0] != event {
                                    if is_down {
                                        let out_desc: Vec<String> = evts.iter().map(|e| format!("0x{:02X}", e.scan_code)).collect();
                                        activity_info = Some((
                                            "All Active Keyboards".to_string(),
                                            format!("Code 0x{:02X} -> Remapped to [{}]", scan_code, out_desc.join(", ")),
                                        ));
                                    }
                                    remapped_events = Some(evts);
                                } else if is_down {
                                    activity_info = Some((
                                        "All Active Keyboards".to_string(),
                                        format!("Code 0x{:02X} -> Pass-through", scan_code),
                                    ));
                                }
                            }
                            EngineAction::Suppress => {
                                remapped_events = Some(Vec::new());
                            }
                        }
                    }
                }

                // Notify UI activity
                if let Some((dev_id, log_msg)) = activity_info {
                    if let Ok(cb_guard) = ACTIVITY_CALLBACK.lock() {
                        if let Some(cb) = cb_guard.as_ref() {
                            cb(dev_id, log_msg);
                        }
                    }
                }

                // If IME trigger was recognized, perform switch
                if let Some((on, method)) = trigger_ime {
                    info!(
                        "[Standalone Tap] Switching IME to {}",
                        if on { "ON" } else { "OFF" }
                    );
                    switch_ime(on, method);

                    if let Ok(cb_guard) = ACTIVITY_CALLBACK.lock() {
                        if let Some(cb) = cb_guard.as_ref() {
                            cb(
                                "All Active Keyboards".to_string(),
                                format!("Ctrl Tap Detected -> IME {}", if on { "ON" } else { "OFF" }),
                            );
                        }
                    }
                }

                // If the key was remapped, suppress original and inject transformed keys
                if let Some(evts) = remapped_events {
                    send_synthetic_events(&evts);
                    return 1; // Block original hardware event
                }
            }
        }
    }

    CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param)
}
