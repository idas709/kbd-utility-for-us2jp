use hid_remap_core::config::{AppConfig, LayoutMode};
use hid_remap_core::engine::{EngineAction, RemapEngine};
use hid_remap_core::ime::{CtrlTapTracker, TapResult};
use hid_remap_core::keyboard::{KeyDirection, KeyEvent, KeyFlags};
use hid_remap_ime::{switch_ime, EXTRA_INFO_MAGIC};
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
static ENGINE_STATE: Mutex<Option<EngineSharedState>> = Mutex::new(None);
static GLOBAL_ENABLED: AtomicBool = AtomicBool::new(true);

struct EngineSharedState {
    engine: RemapEngine,
    tap_tracker: CtrlTapTracker,
    config: AppConfig,
}

pub fn start_standalone_engine(config: AppConfig) -> Result<(), &'static str> {
    let mode = config
        .devices
        .first()
        .map(|d| d.layout_mode)
        .unwrap_or(LayoutMode::UsToJis);

    let ctrl_ime_enabled = config
        .devices
        .first()
        .map(|d| d.ctrl_ime_enabled)
        .unwrap_or(true);

    GLOBAL_ENABLED.store(config.general.enabled, Ordering::Relaxed);

    {
        let mut guard = ENGINE_STATE.lock().unwrap();
        *guard = Some(EngineSharedState {
            engine: RemapEngine::new(mode),
            tap_tracker: CtrlTapTracker::new(ctrl_ime_enabled, config.ime.tap_timeout_ms),
            config,
        });
    }

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

    if let Ok(mut guard) = ENGINE_STATE.lock() {
        if let Some(state) = guard.as_mut() {
            let mode = config
                .devices
                .first()
                .map(|d| d.layout_mode)
                .unwrap_or(LayoutMode::UsToJis);

            let ctrl_ime_enabled = config
                .devices
                .first()
                .map(|d| d.ctrl_ime_enabled)
                .unwrap_or(true);

            state.engine.layout_mode = mode;
            state.tap_tracker.enabled = ctrl_ime_enabled;
            state.tap_tracker.timeout_ms = config.ime.tap_timeout_ms;
            state.config = config;
            info!("Standalone input engine updated with new config (Mode: {:?}, Ctrl IME: {})", mode, ctrl_ime_enabled);
        }
    }
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

        // Ignore our own synthetic inputs to prevent feedback loops
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

                if let Ok(mut guard) = ENGINE_STATE.lock() {
                    if let Some(state) = guard.as_mut() {
                        // 1. Process Ctrl single-tap IME gesture
                        let tap_res = state.tap_tracker.process_event(&event);
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
                        let action = state.engine.process(event);
                        match action {
                            EngineAction::Emit(evts) => {
                                // If the engine emitted something different from the original key, block original and inject
                                if evts.len() != 1 || evts[0] != event {
                                    remapped_events = Some(evts);
                                }
                            }
                            EngineAction::Suppress => {
                                remapped_events = Some(Vec::new());
                            }
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

