use std::path::PathBuf;
use tracing::{info, warn};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ,
};

const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_REG_NAME: &str = "HIDKeyboardLayoutRemapper";

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn get_executable_path() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| std::fs::canonicalize(&p).ok().or(Some(p)))
}

#[allow(dead_code)]
pub fn is_startup_enabled() -> bool {
    let subkey_wide = to_wide(RUN_SUBKEY);
    let name_wide = to_wide(APP_REG_NAME);

    unsafe {
        let mut hkey = std::mem::zeroed();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey_wide.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        );
        if status != 0 {
            return false;
        }

        let mut val_type = 0;
        let mut byte_len = 0;
        let query_status = RegQueryValueExW(
            hkey,
            name_wide.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            std::ptr::null_mut(),
            &mut byte_len,
        );

        RegCloseKey(hkey);
        query_status == 0 && byte_len > 0
    }
}

pub fn set_startup_enabled(enabled: bool) -> Result<(), String> {
    let subkey_wide = to_wide(RUN_SUBKEY);
    let name_wide = to_wide(APP_REG_NAME);

    unsafe {
        let mut hkey = std::mem::zeroed();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey_wide.as_ptr(),
            0,
            KEY_READ | KEY_WRITE,
            &mut hkey,
        );
        if status != 0 {
            return Err(format!("Failed to open registry Run key: error {}", status));
        }

        let result = if enabled {
            if let Some(exe_path) = get_executable_path() {
                // Strip verbatim prefix \\?\ if present from canonicalize
                let path_str = exe_path.to_string_lossy();
                let clean_path = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
                let cmd = format!("\"{}\"", clean_path);
                let cmd_wide = to_wide(&cmd);
                let byte_len = (cmd_wide.len() * 2) as u32;

                let set_status = RegSetValueExW(
                    hkey,
                    name_wide.as_ptr(),
                    0,
                    REG_SZ,
                    cmd_wide.as_ptr() as *const u8,
                    byte_len,
                );
                if set_status == 0 {
                    info!("Startup registry entry registered: {}", cmd);
                    Ok(())
                } else {
                    let err = format!("Failed to set startup registry entry: error {}", set_status);
                    warn!("{}", err);
                    Err(err)
                }
            } else {
                let err = "Failed to resolve current executable path".to_string();
                warn!("{}", err);
                Err(err)
            }
        } else {
            let del_status = RegDeleteValueW(hkey, name_wide.as_ptr());
            if del_status == 0 || del_status == 2 {
                info!("Startup registry entry removed.");
                Ok(())
            } else {
                let err = format!("Failed to delete startup registry entry: error {}", del_status);
                warn!("{}", err);
                Err(err)
            }
        };

        RegCloseKey(hkey);
        result
    }
}
