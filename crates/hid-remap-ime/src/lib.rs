use hid_remap_core::config::ImeMethod;
use tracing::{debug, info};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::Ime::{ImmGetDefaultIMEWnd, IMC_SETOPENSTATUS};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONVERT,
    VK_IME_OFF, VK_IME_ON, VK_KANJI, VK_NONCONVERT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, SendMessageTimeoutW, GUITHREADINFO, SMTO_ABORTIFHUNG,
    WM_IME_CONTROL,
};

pub const IMC_GETOPENSTATUS: u32 = 0x0005;

/// Magic extra info value to tag synthetic inputs injected by this tool.
pub const EXTRA_INFO_MAGIC: usize = 0x43494D45; // 'CIME'

/// Retrieve the active foreground window or keyboard-focused window.
pub fn get_target_window() -> HWND {
    unsafe {
        let mut gti: GUITHREADINFO = std::mem::zeroed();
        gti.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(0, &mut gti) != 0 {
            if !gti.hwndFocus.is_null() {
                return gti.hwndFocus;
            }
            if !gti.hwndActive.is_null() {
                return gti.hwndActive;
            }
        }
        GetForegroundWindow()
    }
}

/// Query current IME open status for the given window handle.
/// Returns `Some(true)` if open (Japanese), `Some(false)` if closed (English/Direct), or `None` if indeterminate.
pub fn get_ime_status(hwnd: HWND) -> Option<bool> {
    if hwnd.is_null() {
        return None;
    }
    unsafe {
        let ime_wnd = ImmGetDefaultIMEWnd(hwnd);
        if ime_wnd.is_null() {
            return None;
        }

        let mut result: usize = 0;
        let success = SendMessageTimeoutW(
            ime_wnd,
            WM_IME_CONTROL,
            IMC_GETOPENSTATUS as usize,
            0,
            SMTO_ABORTIFHUNG,
            100, // 100ms timeout
            &mut result,
        );

        if success != 0 {
            Some(result != 0)
        } else {
            None
        }
    }
}

/// Set IME open status directly via WM_IME_CONTROL message.
pub fn set_ime_by_message(hwnd: HWND, on: bool) -> bool {
    if hwnd.is_null() {
        return false;
    }
    unsafe {
        let ime_wnd = ImmGetDefaultIMEWnd(hwnd);
        if ime_wnd.is_null() {
            return false;
        }

        let mut result: usize = 0;
        let success = SendMessageTimeoutW(
            ime_wnd,
            WM_IME_CONTROL,
            IMC_SETOPENSTATUS as usize,
            if on { 1 } else { 0 },
            SMTO_ABORTIFHUNG,
            100,
            &mut result,
        );

        success != 0
    }
}

fn create_key_input(vk: u16, scan: u16, key_up: bool) -> INPUT {
    let mut flags = 0;
    if key_up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: EXTRA_INFO_MAGIC,
            },
        },
    }
}

/// Send a simulated key press and release using SendInput.
pub fn send_key_stroke(vk: u16, scan: u16) {
    let mut inputs = [
        create_key_input(vk, scan, false),
        create_key_input(vk, scan, true),
    ];
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

/// Switch IME status according to specified method (FR-018, FR-019, FR-023).
pub fn switch_ime(on: bool, method: ImeMethod) {
    info!(
        "Switching IME to {} using method {:?}",
        if on { "ON" } else { "OFF" },
        method
    );

    match method {
        ImeMethod::Imm => {
            let hwnd = get_target_window();
            let current = get_ime_status(hwnd);
            debug!("Target HWND={:?}, Current IME={:?}", hwnd, current);

            // Avoid redundant switching (prevents unwanted '再変換' reconversion)
            if let Some(cur) = current {
                if cur == on {
                    debug!("IME already in desired state ({}), skipping", on);
                    return;
                }
            }

            let sent = set_ime_by_message(hwnd, on);
            let after = get_ime_status(hwnd);

            if let Some(status_after) = after {
                if status_after != on {
                    debug!("Message did not take effect, falling back to VK_KANJI toggle");
                    send_key_stroke(VK_KANJI, 0);
                }
            } else if !sent {
                debug!("Failed to deliver IMM message, falling back to VK_KANJI toggle");
                send_key_stroke(VK_KANJI, 0);
            }
        }
        ImeMethod::ImeKey => {
            let vk = if on { VK_IME_ON } else { VK_IME_OFF };
            send_key_stroke(vk, 0);
        }
        ImeMethod::ConvertKey => {
            let (vk, scan) = if on {
                (VK_CONVERT, 0x79)
            } else {
                (VK_NONCONVERT, 0x7B)
            };
            send_key_stroke(vk, scan);
        }
        ImeMethod::Hybrid => {
            let vk_ime = if on { VK_IME_ON } else { VK_IME_OFF };
            send_key_stroke(vk_ime, 0);
            let (vk_conv, scan) = if on {
                (VK_CONVERT, 0x79)
            } else {
                (VK_NONCONVERT, 0x7B)
            };
            send_key_stroke(vk_conv, scan);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_target_window() {
        let hwnd = get_target_window();
        println!("Target Window HWND: {:?}", hwnd);
        let status = get_ime_status(hwnd);
        println!("Current IME status: {:?}", status);
    }
}
