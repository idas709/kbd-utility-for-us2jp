use hid_remap_core::config::ImeMethod;
use hid_remap_core::ime::{CtrlTapTracker, TapResult};
use hid_remap_core::keyboard::{KeyEvent, KeyFlags};
use hid_remap_ime::{switch_ime, EXTRA_INFO_MAGIC};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tracing::info;
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_EXTENDED, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

static HOOK_HANDLE: Mutex<isize> = Mutex::new(0);
static ENABLED: AtomicBool = AtomicBool::new(true);
static TRACKER: Mutex<Option<(CtrlTapTracker, ImeMethod)>> = Mutex::new(None);

pub fn start_hook(timeout_ms: u32, method: ImeMethod) -> Result<(), &'static str> {
    {
        let mut tracker_guard = TRACKER.lock().unwrap();
        *tracker_guard = Some((CtrlTapTracker::new(true, timeout_ms), method));
    }

    unsafe {
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            std::ptr::null_mut(),
            0,
        );
        if hook.is_null() {
            return Err("Failed to install low-level keyboard hook");
        }
        let mut handle = HOOK_HANDLE.lock().unwrap();
        *handle = hook as isize;
        info!("Low-level keyboard hook installed successfully.");
        Ok(())
    }
}

pub fn stop_hook() {
    unsafe {
        let mut handle = HOOK_HANDLE.lock().unwrap();
        let hook = *handle as HHOOK;
        if !hook.is_null() {
            UnhookWindowsHookEx(hook);
            *handle = 0;
            info!("Low-level keyboard hook removed.");
        }
    }
}

unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 && ENABLED.load(Ordering::Relaxed) {
        let kbd = &*(l_param as *const KBDLLHOOKSTRUCT);

        // Ignore synthetic inputs from ourselves
        if kbd.dwExtraInfo == EXTRA_INFO_MAGIC {
            return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
        }

        let msg = w_param as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        let is_extended = (kbd.flags & LLKHF_EXTENDED) != 0;
        let scan_code = kbd.scanCode as u16;

        let flags = KeyFlags {
            is_extended,
            is_extended1: false,
        };

        let mut trigger: Option<(bool, ImeMethod)> = None;

        if let Ok(mut guard) = TRACKER.lock() {
            if let Some((tracker, method)) = guard.as_mut() {
                let event = if is_down {
                    KeyEvent::new(
                        scan_code,
                        hid_remap_core::keyboard::KeyDirection::Down,
                        flags,
                        kbd.time,
                    )
                } else if is_up {
                    KeyEvent::new(
                        scan_code,
                        hid_remap_core::keyboard::KeyDirection::Up,
                        flags,
                        kbd.time,
                    )
                } else {
                    return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
                };

                let res = tracker.process_event(&event);
                match res {
                    TapResult::LeftCtrlTap => {
                        trigger = Some((false, *method));
                    }
                    TapResult::RightCtrlTap => {
                        trigger = Some((true, *method));
                    }
                    TapResult::None => {}
                }
            }
        }

        if let Some((on, method)) = trigger {
            info!(
                "[User Session Hook] Single Ctrl Tap detected: Switching IME to {}",
                if on { "ON" } else { "OFF" }
            );
            switch_ime(on, method);
        }
    }

    CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param)
}
