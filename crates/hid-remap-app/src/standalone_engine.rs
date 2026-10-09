use hid_remap_core::config::{AppConfig, LayoutMode};
use hid_remap_core::engine::{EngineAction, RemapEngine};
use hid_remap_core::ime::{CtrlTapTracker, TapResult};
use hid_remap_core::keyboard::scancodes::*;
use hid_remap_core::keyboard::{KeyDirection, KeyEvent, KeyFlags};
use hid_remap_ime::{switch_ime, EXTRA_INFO_MAGIC};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tracing::info;
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, VK_CONTROL, VK_LCONTROL, VK_RCONTROL,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_EXTENDED, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

const MAPVK_VK_TO_VSC: u32 = 0;
const MAPVK_VSC_TO_VK_EX: u32 = 3;

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

    // Determine active layout mode:
    // If any device is explicitly configured to UsToJis or JisToUs, use that mode!
    // Otherwise, fallback to config.general.default_layout_mode.
    let active_mode = if let Some(dev) = config.devices.iter().find(|d| d.layout_mode != LayoutMode::Passthrough) {
        dev.layout_mode
    } else {
        config.general.default_layout_mode
    };

    let default_ctrl_ime = config
        .devices
        .iter()
        .any(|d| d.ctrl_ime_enabled) || true;

    for dev in &config.devices {
        device_engines.insert(
            dev.instance_id.to_uppercase(),
            create_engine_for_mode(dev.layout_mode, dev.ctrl_ime_enabled, timeout),
        );
    }

    let default_engine = create_engine_for_mode(active_mode, default_ctrl_ime, timeout);

    let mut guard = ENGINE_STATE.lock().unwrap();
    *guard = Some(EngineSharedState {
        device_engines,
        default_engine,
        config,
    });
    info!("Engine updated. Active Layout Mode: {:?}, Ctrl IME: {}", active_mode, default_ctrl_ime);
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

fn friendly_key_name(scan_code: u16, is_shift: bool) -> &'static str {
    match (scan_code, is_shift) {
        (KEY_2, true) => "Shift + 2 (@)",
        (KEY_2, false) => "2",
        (KEY_6, true) => "Shift + 6 (^)",
        (KEY_6, false) => "6",
        (KEY_7, true) => "Shift + 7 (&)",
        (KEY_7, false) => "7",
        (KEY_8, true) => "Shift + 8 (*)",
        (KEY_8, false) => "8",
        (KEY_9, true) => "Shift + 9 ( ( )",
        (KEY_9, false) => "9",
        (KEY_0, true) => "Shift + 0 ( ) )",
        (KEY_0, false) => "0",
        (MINUS, true) => "Shift + - (_)",
        (MINUS, false) => "-",
        (EQUALS, true) => "Shift + = (+)",
        (EQUALS, false) => "= (Equals)",
        (LBRACKET, true) => "Shift + [ ({)",
        (LBRACKET, false) => "[ (Left Bracket)",
        (RBRACKET, true) => "Shift + ] (})",
        (RBRACKET, false) => "] (Right Bracket)",
        (SEMICOLON, true) => "Shift + ; (:)",
        (SEMICOLON, false) => "; (Semicolon)",
        (APOSTROPHE, true) => "Shift + ' (\")",
        (APOSTROPHE, false) => "' (Apostrophe)",
        (GRAVE, true) => "Shift + ` (~)",
        (GRAVE, false) => "` (Backtick)",
        (BACKSLASH, true) => "Shift + \\ (|)",
        (BACKSLASH, false) => "\\ (Backslash)",
        (LSHIFT, _) => "Left Shift",
        (RSHIFT, _) => "Right Shift",
        (LCTRL, _) => "Left Ctrl",
        (LALT, _) => "Left Alt",
        (SPACE, _) => "Space",
        (ENTER, _) => "Enter",
        _ => "Key",
    }
}

fn friendly_scancode_desc(scan_code: u16) -> String {
    match scan_code {
        LBRACKET => "JIS '@' (0x1A)".to_string(),
        EQUALS => "JIS '^' (0x0D)".to_string(),
        KEY_6 => "JIS '&' (0x07)".to_string(),
        APOSTROPHE => "JIS '*' (0x28)".to_string(),
        KEY_8 => "JIS '(' (0x09)".to_string(),
        KEY_9 => "JIS ')' (0x0A)".to_string(),
        JIS_RO => "JIS '_' (0x73 ろ)".to_string(),
        MINUS => "JIS '=' (0x0C)".to_string(),
        SEMICOLON => "JIS '+' (0x27)".to_string(),
        RBRACKET => "JIS '[' (0x1B)".to_string(),
        BACKSLASH => "JIS ']' (0x2B)".to_string(),
        JIS_YEN => "JIS '¥ / |' (0x7D)".to_string(),
        KEY_7 => "JIS ''' (0x08)".to_string(),
        KEY_2 => "JIS '\"' (0x03)".to_string(),
        LSHIFT => "Shift".to_string(),
        _ => format!("0x{:02X}", scan_code),
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

        // Map scan code to Virtual Key so applications that inspect vkCode also process correctly
        let vk = unsafe { MapVirtualKeyW(evt.scan_code as u32, MAPVK_VSC_TO_VK_EX) as u16 };

        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
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

                // Ensure scanCode is valid using MapVirtualKeyW fallback
                let mut scan_code = (kbd.scanCode & 0xFF) as u16;
                if scan_code == 0 {
                    scan_code = (MapVirtualKeyW(kbd.vkCode, MAPVK_VK_TO_VSC) & 0xFF) as u16;
                }

                // Handle Left/Right Ctrl identification explicitly
                let is_left_ctrl = kbd.vkCode == VK_LCONTROL as u32 || (kbd.vkCode == VK_CONTROL as u32 && !is_extended);
                let is_right_ctrl = kbd.vkCode == VK_RCONTROL as u32 || (kbd.vkCode == VK_CONTROL as u32 && is_extended);

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
                        let dev_engine = &mut state.default_engine;

                        // 1. Process Ctrl single-tap IME gesture
                        if is_left_ctrl || is_right_ctrl || scan_code == 0x1D {
                            let tap_event = KeyEvent::new(
                                0x1D,
                                event.direction,
                                KeyFlags {
                                    is_extended: is_right_ctrl,
                                    is_extended1: false,
                                },
                                kbd.time,
                            );

                            let tap_res = dev_engine.tap_tracker.process_event(&tap_event);
                            match tap_res {
                                TapResult::LeftCtrlTap => {
                                    trigger_ime = Some((false, state.config.ime.method));
                                }
                                TapResult::RightCtrlTap => {
                                    trigger_ime = Some((true, state.config.ime.method));
                                }
                                TapResult::None => {}
                            }
                        } else {
                            // Any other key resets / cancels tap
                            dev_engine.tap_tracker.process_event(&event);
                        }

                        // 2. Process Layout Remapping
                        let is_shift_active = dev_engine.engine.modifiers.is_shift();
                        let key_desc = friendly_key_name(scan_code, is_shift_active);

                        let action = dev_engine.engine.process(event);
                        match action {
                            EngineAction::Emit(evts) => {
                                if evts.len() != 1 || evts[0] != event {
                                    if is_down {
                                        let out_desc: Vec<String> = evts
                                            .iter()
                                            .filter(|e| e.is_down() && e.scan_code != LSHIFT)
                                            .map(|e| friendly_scancode_desc(e.scan_code))
                                            .collect();
                                        activity_info = Some((
                                            "Keyboard Input".to_string(),
                                            format!("✔ REMAPPED: {} -> Output: [{}]", key_desc, out_desc.join(", ")),
                                        ));
                                    }
                                    remapped_events = Some(evts);
                                } else if is_down {
                                    // Passthrough notification
                                    let char_hint = if kbd.vkCode >= 0x41 && kbd.vkCode <= 0x5A {
                                        format!("'{}'", (kbd.vkCode as u8) as char)
                                    } else if kbd.vkCode >= 0x30 && kbd.vkCode <= 0x39 && !is_shift_active {
                                        format!("'{}'", (kbd.vkCode as u8) as char)
                                    } else {
                                        key_desc.to_string()
                                    };

                                    activity_info = Some((
                                        "Keyboard Input".to_string(),
                                        format!("Passthrough: {} (US & JIS share same key)", char_hint),
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
                                "Ctrl Tap Detected".to_string(),
                                format!("✔ IME SWITCHED: Set IME to {}", if on { "ON (Japanese)" } else { "OFF (English)" }),
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
