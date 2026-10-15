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
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, VK_CONTROL, VK_LCONTROL,
    VK_LWIN, VK_MENU, VK_RCONTROL, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_USER,
};

const MAPVK_VK_TO_VSC: u32 = 0;
const MAPVK_VSC_TO_VK_EX: u32 = 3;

static HOOK_HANDLE: Mutex<isize> = Mutex::new(0);
static HOOK_THREAD_ID: Mutex<u32> = Mutex::new(0);
static GLOBAL_ENABLED: AtomicBool = AtomicBool::new(true);
static ENGINE_STATE: Mutex<Option<EngineSharedState>> = Mutex::new(None);
static ACTIVITY_CALLBACK: Mutex<Option<Box<dyn Fn(String, String) + Send + Sync>>> = Mutex::new(None);
static LAST_TYPED_DEVICE: Mutex<Option<String>> = Mutex::new(None);

pub const WM_USER_REFRESH_HOOK: u32 = WM_USER + 200;

pub fn set_last_typed_device(instance_id: String) {
    if let Ok(mut guard) = LAST_TYPED_DEVICE.lock() {
        *guard = Some(instance_id);
    }
}

pub fn get_last_typed_device() -> Option<String> {
    LAST_TYPED_DEVICE.lock().ok().and_then(|g| g.clone())
}

struct DeviceEngine {
    engine: RemapEngine,
    tap_tracker: CtrlTapTracker,
}

struct EngineSharedState {
    device_engines: HashMap<String, DeviceEngine>,
    default_engine: DeviceEngine,
    key_owners: HashMap<u16, String>,
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

pub fn start_standalone_engine(config: AppConfig, connected_ids: &[String]) -> Result<(), &'static str> {
    GLOBAL_ENABLED.store(config.general.enabled, Ordering::Relaxed);
    update_engine_state(config, connected_ids);

    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::Builder::new()
        .name("hid-remap-hook-worker".to_string())
        .spawn(move || {
            unsafe {
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);

                let tid = GetCurrentThreadId();
                if let Ok(mut guard) = HOOK_THREAD_ID.lock() {
                    *guard = tid;
                }

                let hook = SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(low_level_keyboard_proc),
                    std::ptr::null_mut(),
                    0,
                );

                if hook.is_null() {
                    let _ = tx.send(Err("Failed to install standalone keyboard hook"));
                    return;
                }

                if let Ok(mut handle) = HOOK_HANDLE.lock() {
                    *handle = hook as isize;
                }
                info!("Standalone input engine hook installed on dedicated worker thread (TID: {}).", tid);
                let _ = tx.send(Ok(()));

                // Dedicated, ultra-responsive message pump isolated from Slint GUI & PnP I/O
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    if msg.message == WM_USER_REFRESH_HOOK {
                        let mut handle = HOOK_HANDLE.lock().unwrap();
                        let old_hook = *handle as HHOOK;
                        if !old_hook.is_null() {
                            UnhookWindowsHookEx(old_hook);
                            *handle = 0;
                        }
                        let new_hook = SetWindowsHookExW(
                            WH_KEYBOARD_LL,
                            Some(low_level_keyboard_proc),
                            std::ptr::null_mut(),
                            0,
                        );
                        *handle = new_hook as isize;
                        info!("Standalone hook refreshed on worker thread: valid={}", !new_hook.is_null());
                    }
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                // Cleanup on thread exit
                let mut handle = HOOK_HANDLE.lock().unwrap();
                let cur_hook = *handle as HHOOK;
                if !cur_hook.is_null() {
                    UnhookWindowsHookEx(cur_hook);
                    *handle = 0;
                    info!("Standalone hook uninstalled on thread termination.");
                }
            }
        })
        .map_err(|_| "Failed to spawn hook worker thread")?;

    match rx.recv_timeout(std::time::Duration::from_secs(3)) {
        Ok(res) => res,
        Err(_) => Err("Timeout waiting for hook worker thread to initialize"),
    }
}

pub fn refresh_hook() {
    if let Ok(guard) = HOOK_THREAD_ID.lock() {
        let tid = *guard;
        if tid != 0 {
            unsafe {
                PostThreadMessageW(tid, WM_USER_REFRESH_HOOK, 0, 0);
            }
        }
    }
}

pub fn reset_engine_states() {
    if let Ok(mut guard) = ENGINE_STATE.lock() {
        if let Some(state) = guard.as_mut() {
            let mut all_cleanups = Vec::new();
            for de in state.device_engines.values_mut() {
                all_cleanups.extend(de.engine.reset());
            }
            all_cleanups.extend(state.default_engine.engine.reset());
            state.key_owners.clear();
            if !all_cleanups.is_empty() {
                send_synthetic_events(&all_cleanups);
            }
        }
    }
}

pub fn reactivate_engine(config: AppConfig, connected_ids: &[String]) {
    GLOBAL_ENABLED.store(true, Ordering::Relaxed);
    reset_engine_states();
    update_engine_state(config, connected_ids);
    refresh_hook();
    info!("Engine reactivated and hook refreshed.");
}

pub fn update_engine_config(config: AppConfig, connected_ids: &[String]) {
    GLOBAL_ENABLED.store(config.general.enabled, Ordering::Relaxed);
    update_engine_state(config, connected_ids);
}

fn is_device_id_match(id1: &str, id2: &str) -> bool {
    if id1.eq_ignore_ascii_case(id2) {
        return true;
    }
    let norm1 = id1.replace('/', "\\").to_uppercase();
    let norm2 = id2.replace('/', "\\").to_uppercase();
    if norm1 == norm2 || norm1.contains(&norm2) || norm2.contains(&norm1) {
        return true;
    }
    if let (Some(vid1), Some(pid1)) = hid_remap_device::parse_vid_pid(id1) {
        if let (Some(vid2), Some(pid2)) = hid_remap_device::parse_vid_pid(id2) {
            return vid1 == vid2 && pid1 == pid2;
        }
    }
    false
}

fn update_engine_state(config: AppConfig, connected_ids: &[String]) {
    let mut device_engines = HashMap::new();
    let timeout = config.ime.tap_timeout_ms;

    let (active_mode, default_ctrl_ime, connected_count) = {
        let connected_devs: Vec<&hid_remap_core::config::DeviceConfig> = if !connected_ids.is_empty() {
            config
                .devices
                .iter()
                .filter(|d| connected_ids.iter().any(|cid| is_device_id_match(cid, &d.instance_id)))
                .collect()
        } else {
            config.devices.iter().collect()
        };

        let all_connected_passthrough = !connected_devs.is_empty()
            && connected_devs.iter().all(|d| d.layout_mode == LayoutMode::Passthrough);

        let active_mode = if all_connected_passthrough {
            LayoutMode::Passthrough
        } else if let Some(dev) = connected_devs.iter().find(|d| d.layout_mode != LayoutMode::Passthrough) {
            dev.layout_mode
        } else if let Some(dev) = config.devices.iter().find(|d| d.layout_mode != LayoutMode::Passthrough) {
            dev.layout_mode
        } else {
            config.general.default_layout_mode
        };

        let default_ctrl_ime = if !connected_devs.is_empty() {
            connected_devs.iter().any(|d| d.ctrl_ime_enabled)
        } else {
            config.devices.iter().any(|d| d.ctrl_ime_enabled)
        };

        (active_mode, default_ctrl_ime, connected_devs.len())
    };

    for dev in &config.devices {
        device_engines.insert(
            dev.instance_id.to_uppercase(),
            create_engine_for_mode(dev.layout_mode, dev.ctrl_ime_enabled, timeout),
        );
    }

    let default_engine = create_engine_for_mode(active_mode, default_ctrl_ime, timeout);

    let mut guard = ENGINE_STATE.lock().unwrap();
    let existing_owners = guard.as_mut().map(|s| std::mem::take(&mut s.key_owners)).unwrap_or_default();
    *guard = Some(EngineSharedState {
        device_engines,
        default_engine,
        key_owners: existing_owners,
        config,
    });
    info!(
        "Engine updated. Connected Devs: {}, Active Mode: {:?}, Ctrl IME: {}",
        connected_count,
        active_mode,
        default_ctrl_ime
    );
}

pub fn stop_standalone_engine() {
    if let Ok(guard) = HOOK_THREAD_ID.lock() {
        let tid = *guard;
        if tid != 0 {
            unsafe {
                PostThreadMessageW(tid, WM_QUIT, 0, 0);
            }
        }
    }
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
        (CAPSLOCK, _) => "CapsLock",
        _ => "Key",
    }
}

fn friendly_scancode_desc(scan_code: u16) -> String {
    match scan_code {
        CAPSLOCK => "CapsLock (Toggle)".to_string(),
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

#[inline]
fn sync_engine_modifiers_with_hardware(modifiers: &mut hid_remap_core::keyboard::ModifierState) {
    unsafe {
        let is_ctrl = (GetAsyncKeyState(VK_CONTROL as i32) as u16 & 0x8000) != 0;
        let is_alt = (GetAsyncKeyState(VK_MENU as i32) as u16 & 0x8000) != 0;
        let is_lwin = (GetAsyncKeyState(VK_LWIN as i32) as u16 & 0x8000) != 0;
        let is_rwin = (GetAsyncKeyState(VK_RWIN as i32) as u16 & 0x8000) != 0;
        let is_shift = (GetAsyncKeyState(VK_SHIFT as i32) as u16 & 0x8000) != 0;

        // If OS physical state says modifier is NOT pressed, clear it immediately
        if !is_ctrl {
            modifiers.lctrl = false;
            modifiers.rctrl = false;
        }
        if !is_alt {
            modifiers.lalt = false;
            modifiers.ralt = false;
        }
        if !is_lwin {
            modifiers.lwin = false;
        }
        if !is_rwin {
            modifiers.rwin = false;
        }
        if !is_shift {
            modifiers.lshift = false;
            modifiers.rshift = false;
        } else if !modifiers.is_shift() {
            // Hardware Shift is physically pressed, but engine hadn't recorded it yet
            let is_rshift = (GetAsyncKeyState(VK_RSHIFT as i32) as u16 & 0x8000) != 0;
            if is_rshift {
                modifiers.rshift = true;
            } else {
                modifiers.lshift = true;
            }
        }
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
                        let active_id = if is_down {
                            let cur_id = get_last_typed_device();
                            if let Some(ref id) = cur_id {
                                state.key_owners.insert(scan_code, id.clone());
                            } else {
                                state.key_owners.remove(&scan_code);
                            }
                            cur_id
                        } else {
                            state.key_owners.remove(&scan_code).or_else(get_last_typed_device)
                        };

                        let find_engine_key = |id: &str, engines: &HashMap<String, DeviceEngine>| -> Option<String> {
                            let upper = id.to_uppercase();
                            if engines.contains_key(&upper) {
                                return Some(upper);
                            }
                            if let Some(key) = engines.keys().find(|k| is_device_id_match(k, id)) {
                                return Some(key.clone());
                            }
                            None
                        };

                        let target_engine_key: Option<String> = if is_up {
                            if let Some(ref id) = active_id {
                                let resolved = find_engine_key(id, &state.device_engines).unwrap_or_else(|| id.to_uppercase());
                                if state.device_engines.get(&resolved).map_or(false, |de| de.engine.has_active_key(scan_code)) {
                                    Some(resolved)
                                } else if state.default_engine.engine.has_active_key(scan_code) {
                                    None
                                } else if let Some((other_key, _)) = state.device_engines.iter().find(|(_, de)| de.engine.has_active_key(scan_code)) {
                                    Some(other_key.clone())
                                } else {
                                    Some(resolved)
                                }
                            } else if state.default_engine.engine.has_active_key(scan_code) {
                                None
                            } else if let Some((other_key, _)) = state.device_engines.iter().find(|(_, de)| de.engine.has_active_key(scan_code)) {
                                Some(other_key.clone())
                            } else {
                                None
                            }
                        } else {
                            active_id.as_deref().and_then(|id| find_engine_key(id, &state.device_engines))
                                .or_else(|| active_id.as_ref().map(|id| id.to_uppercase()))
                        };

                        let (dev_engine, dev_label) = if let Some(ref upper) = target_engine_key {
                            if let Some(engine) = state.device_engines.get_mut(upper) {
                                (engine, upper.as_str())
                            } else {
                                (&mut state.default_engine, "Default")
                            }
                        } else {
                            (&mut state.default_engine, "Default")
                        };

                        // Synchronize modifier states with actual physical hardware before processing!
                        sync_engine_modifiers_with_hardware(&mut dev_engine.engine.modifiers);

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
                                            dev_label.to_string(),
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
                                        dev_label.to_string(),
                                        if dev_engine.engine.layout_mode == LayoutMode::Passthrough {
                                            format!("Passthrough: {} (Mode: Passthrough)", char_hint)
                                        } else {
                                            format!("Passthrough: {} (US & JIS share same key)", char_hint)
                                        },
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

                // If a physical shift key was released, unconditionally clear any synthetic Shift state in Windows!
                if (scan_code == LSHIFT || scan_code == RSHIFT) && is_up {
                    send_synthetic_events(&[
                        KeyEvent::up(LSHIFT, KeyFlags::NORMAL),
                        KeyEvent::up(RSHIFT, KeyFlags::NORMAL),
                    ]);
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
