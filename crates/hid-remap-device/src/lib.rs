use hid_remap_core::config::{AppConfig, LayoutMode};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::ptr;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_LOCAL_MACHINE, KEY_READ,
};
use windows_sys::Win32::UI::Input::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RIDI_DEVICENAME,
    RIM_TYPEKEYBOARD,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyboardDeviceInfo {
    /// HID Device Instance ID (e.g. `HID\VID_1234&PID_5678\...`)
    pub instance_id: String,
    /// Friendly Name or Product Name
    pub name: String,
    /// Vendor ID (if parseable)
    pub vid: Option<u16>,
    /// Product ID (if parseable)
    pub pid: Option<u16>,
    /// Container ID (GUID grouping multi-function peripherals)
    pub container_id: Option<String>,
    /// Whether the device is currently physically connected
    pub is_connected: bool,
    /// Whether this device has been registered in user configuration
    pub is_registered: bool,
    /// Current layout conversion mode
    pub layout_mode: LayoutMode,
    /// Whether Ctrl single-tap IME switching is enabled
    pub ctrl_ime_enabled: bool,
    /// OS Raw Input Device symbolic path (e.g. `\\?\HID#...`)
    pub raw_device_path: String,
}

/// Parse Vendor ID (VID) and Product ID (PID) from an instance ID or device path.
pub fn parse_vid_pid(path_or_id: &str) -> (Option<u16>, Option<u16>) {
    let upper = path_or_id.to_uppercase();
    let vid = upper.find("VID_").and_then(|idx| {
        let slice = &upper[idx + 4..];
        let end = slice.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(slice.len());
        u16::from_str_radix(&slice[..end.min(4)], 16).ok()
    });

    let pid = upper.find("PID_").and_then(|idx| {
        let slice = &upper[idx + 4..];
        let end = slice.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(slice.len());
        u16::from_str_radix(&slice[..end.min(4)], 16).ok()
    });

    (vid, pid)
}

/// Convert a Windows symbolic link path (e.g. `\\?\HID#VID_046D&PID_C31C#...#{GUID}`)
/// into a standard Device Instance ID (`HID\VID_046D&PID_C31C\...`).
pub fn device_path_to_instance_id(device_path: &str) -> String {
    let clean = device_path
        .strip_prefix(r"\\?\")
        .or_else(|| device_path.strip_prefix(r"\\.\"))
        .unwrap_or(device_path);

    // Strip class GUID at the end: #{884b96c3-5604-1104-9642-00a0c9a36419}
    let without_guid = if let Some(last_hash) = clean.rfind("#{") {
        &clean[..last_hash]
    } else {
        clean
    };

    without_guid.replace('#', r"\")
}

/// Query human-readable Device Description or FriendlyName from Windows Enum registry.
pub fn get_device_description_from_registry(instance_id: &str) -> Option<String> {
    let subkey = format!(r"SYSTEM\CurrentControlSet\Enum\{}", instance_id);
    let wide_key: Vec<u16> = std::ffi::OsStr::new(&subkey)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut hkey = std::mem::zeroed();
        let status = RegOpenKeyExW(HKEY_LOCAL_MACHINE, wide_key.as_ptr(), 0, KEY_READ, &mut hkey);
        if status != 0 {
            return None;
        }

        let read_string_val = |val_name: &str| -> Option<String> {
            let wide_val: Vec<u16> = std::ffi::OsStr::new(val_name)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let mut val_type = 0;
            let mut byte_len = 0;

            let query_res = RegQueryValueExW(
                hkey,
                wide_val.as_ptr(),
                ptr::null_mut(),
                &mut val_type,
                ptr::null_mut(),
                &mut byte_len,
            );

            if query_res == 0 && byte_len > 0 {
                let mut buf: Vec<u16> = vec![0; (byte_len / 2) as usize + 1];
                let query_val = RegQueryValueExW(
                    hkey,
                    wide_val.as_ptr(),
                    ptr::null_mut(),
                    &mut val_type,
                    buf.as_mut_ptr() as *mut _,
                    &mut byte_len,
                );

                if query_val == 0 {
                    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                    let s = String::from_utf16_lossy(&buf[..len]);
                    // Parse localized string format: "@keyboard.inf,%hid.keyboarddevice%;HID Keyboard Device"
                    if let Some(semicolon) = s.rfind(';') {
                        return Some(s[semicolon + 1..].trim().to_string());
                    }
                    return Some(s.trim().to_string());
                }
            }
            None
        };

        // Try FriendlyName first, then DeviceDesc
        let name = read_string_val("FriendlyName").or_else(|| read_string_val("DeviceDesc"));
        RegCloseKey(hkey);
        name
    }
}

/// Friendly default name generator from VID/PID, registry, and instance ID.
pub fn generate_device_name(instance_id: &str, vid: Option<u16>, pid: Option<u16>) -> String {
    // 1. Try registry description
    if let Some(desc) = get_device_description_from_registry(instance_id) {
        if !desc.is_empty() {
            if let (Some(v), Some(p)) = (vid, pid) {
                return format!("{} [VID:{:04X} PID:{:04X}]", desc, v, p);
            }
            if instance_id.starts_with("ACPI") {
                return format!("{} (Internal Laptop)", desc);
            }
            return desc;
        }
    }

    // 2. Fallback to VID/PID
    if let (Some(v), Some(p)) = (vid, pid) {
        format!("USB Keyboard [VID:{:04X} PID:{:04X}]", v, p)
    } else if instance_id.starts_with("ACPI") {
        "Internal PS/2 Keyboard".to_string()
    } else {
        format!("Keyboard ({})", instance_id)
    }
}

/// Enumerate all currently connected physical keyboard devices using Windows Raw Input API.
pub fn enumerate_raw_keyboards() -> Vec<(String, String)> {
    let mut num_devices: u32 = 0;
    let struct_size = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;

    unsafe {
        let res = GetRawInputDeviceList(ptr::null_mut(), &mut num_devices, struct_size);
        if res == u32::MAX || num_devices == 0 {
            return Vec::new();
        }

        let mut device_list: Vec<RAWINPUTDEVICELIST> = vec![std::mem::zeroed(); num_devices as usize];
        let actual = GetRawInputDeviceList(device_list.as_mut_ptr(), &mut num_devices, struct_size);
        if actual == u32::MAX {
            return Vec::new();
        }

        let mut results = Vec::new();

        for dev in device_list.into_iter().take(actual as usize) {
            if dev.dwType != RIM_TYPEKEYBOARD {
                continue;
            }

            // Get device path length
            let mut name_len: u32 = 0;
            GetRawInputDeviceInfoW(dev.hDevice, RIDI_DEVICENAME, ptr::null_mut(), &mut name_len);
            if name_len == 0 {
                continue;
            }

            let mut name_buf: Vec<u16> = vec![0; name_len as usize];
            let copied = GetRawInputDeviceInfoW(
                dev.hDevice,
                RIDI_DEVICENAME,
                name_buf.as_mut_ptr() as *mut _,
                &mut name_len,
            );

            if copied != u32::MAX && copied > 0 {
                let len = if name_buf.last() == Some(&0) {
                    name_buf.len() - 1
                } else {
                    name_buf.len()
                };
                let path_os = OsString::from_wide(&name_buf[..len]);
                let device_path = path_os.to_string_lossy().to_string();

                let instance_id = device_path_to_instance_id(&device_path);
                results.push((device_path, instance_id));
            }
        }

        results
    }
}

/// Enumerate keyboards and correlate with user configuration (FR-001, FR-003, FR-006).
pub fn get_keyboard_devices(config: &AppConfig) -> Vec<KeyboardDeviceInfo> {
    let raw_keyboards = enumerate_raw_keyboards();
    let mut devices = Vec::new();

    // Map connected physical devices
    for (raw_path, instance_id) in raw_keyboards {
        let (vid, pid) = parse_vid_pid(&instance_id);

        let registered_conf = config.find_device(&instance_id);
        let is_registered = registered_conf.is_some();

        let (name, layout_mode, ctrl_ime_enabled) = if let Some(conf) = registered_conf {
            // If the saved name was generic, update it with human-friendly name
            let display_name = if conf.name.starts_with("Keyboard (HID\\") || conf.name.starts_with("Keyboard (ACPI\\") {
                generate_device_name(&instance_id, vid, pid)
            } else {
                conf.name.clone()
            };
            (display_name, conf.layout_mode, conf.ctrl_ime_enabled)
        } else {
            (
                generate_device_name(&instance_id, vid, pid),
                LayoutMode::Passthrough,
                true,
            )
        };

        devices.push(KeyboardDeviceInfo {
            instance_id,
            name,
            vid,
            pid,
            container_id: None,
            is_connected: true,
            is_registered,
            layout_mode,
            ctrl_ime_enabled,
            raw_device_path: raw_path,
        });
    }

    // Also include registered devices from config that are not currently connected
    for registered in &config.devices {
        // Skip dummy sample devices if they don't look real
        if registered.instance_id.contains("EXAMPLE") {
            continue;
        }

        if !devices
            .iter()
            .any(|d| d.instance_id.eq_ignore_ascii_case(&registered.instance_id))
        {
            let (vid, pid) = parse_vid_pid(&registered.instance_id);
            devices.push(KeyboardDeviceInfo {
                instance_id: registered.instance_id.clone(),
                name: registered.name.clone(),
                vid,
                pid,
                container_id: None,
                is_connected: false,
                is_registered: true,
                layout_mode: registered.layout_mode,
                ctrl_ime_enabled: registered.ctrl_ime_enabled,
                raw_device_path: String::new(),
            });
        }
    }

    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vid_pid() {
        let path = r"\\?\HID#VID_046D&PID_C31C&MI_00#7&21B3F74&0&0000#{884B96C3-5604-1104-9642-00A0C9A36419}";
        let (vid, pid) = parse_vid_pid(path);
        assert_eq!(vid, Some(0x046D));
        assert_eq!(pid, Some(0xC31C));
    }

    #[test]
    fn test_device_path_to_instance_id() {
        let path = r"\\?\HID#VID_1234&PID_5678#7&21b3f74&0&0000#{884b96c3-5604-1104-9642-00a0c9a36419}";
        let instance_id = device_path_to_instance_id(path);
        assert_eq!(instance_id, r"HID\VID_1234&PID_5678\7&21b3f74&0&0000");
    }

    #[test]
    fn test_enumerate_returns_readable_names() {
        let config = AppConfig::default();
        let devices = get_keyboard_devices(&config);
        println!("Enumerated {} keyboards with readable names:", devices.len());
        for dev in &devices {
            println!("  Name: '{}' [{}]", dev.name, dev.instance_id);
        }
    }
}
