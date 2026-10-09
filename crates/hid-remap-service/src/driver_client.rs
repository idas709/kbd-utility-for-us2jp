use hid_remap_core::config::{AppConfig, LayoutMode};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::ptr;
use thiserror::Error;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

pub const HID_REMAP_USERMODE_PATH: &str = r"\\.\HidKeyboardRemapper";

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;

// IOCTL Definitions matching driver/public.h
const FILE_DEVICE_HID_REMAP: u32 = 0x8000;
const METHOD_BUFFERED: u32 = 0;
const FILE_ANY_ACCESS: u32 = 0;

const fn ctl_code(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | method
}

pub const IOCTL_HID_REMAP_GET_VERSION: u32 =
    ctl_code(FILE_DEVICE_HID_REMAP, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_HID_REMAP_SET_CONFIG: u32 =
    ctl_code(FILE_DEVICE_HID_REMAP, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_HID_REMAP_GET_IME_EVENT: u32 =
    ctl_code(FILE_DEVICE_HID_REMAP, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS);

pub const MAX_INSTANCE_ID_LEN: usize = 256;
pub const MAX_CONFIGURED_DEVICES: usize = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DriverDeviceEntry {
    pub instance_id: [u16; MAX_INSTANCE_ID_LEN],
    pub layout_mode: u32,
    pub ctrl_ime_enabled: u8,
    pub enabled: u8,
    pub reserved: [u8; 2],
}

impl Default for DriverDeviceEntry {
    fn default() -> Self {
        Self {
            instance_id: [0; MAX_INSTANCE_ID_LEN],
            layout_mode: 0,
            ctrl_ime_enabled: 1,
            enabled: 1,
            reserved: [0; 2],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DriverGlobalConfig {
    pub global_enabled: u8,
    pub reserved: [u8; 3],
    pub tap_timeout_ms: u32,
    pub device_count: u32,
    pub devices: [DriverDeviceEntry; MAX_CONFIGURED_DEVICES],
}

impl Default for DriverGlobalConfig {
    fn default() -> Self {
        Self {
            global_enabled: 1,
            reserved: [0; 3],
            tap_timeout_ms: 1000,
            device_count: 0,
            devices: [DriverDeviceEntry::default(); MAX_CONFIGURED_DEVICES],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DriverImeEventRaw {
    pub instance_id: [u16; MAX_INSTANCE_ID_LEN],
    pub ime_on: u8,
    pub reserved: [u8; 3],
    pub timestamp_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverImeEvent {
    pub instance_id: String,
    pub ime_on: bool,
    pub timestamp_ms: u32,
}

#[derive(Error, Debug)]
pub enum DriverError {
    #[error("Driver device not found or failed to open (code {0:#X})")]
    OpenFailed(u32),
    #[error("DeviceIoControl failed with win32 error code {0:#X}")]
    IoControlFailed(u32),
    #[error("Invalid driver response")]
    InvalidResponse,
}

pub struct DriverClient {
    handle: HANDLE,
}

unsafe impl Send for DriverClient {}

impl DriverClient {
    /// Attempt to connect to the KMDF driver's control device.
    pub fn connect() -> Result<Self, DriverError> {
        let wide_path: Vec<u16> = std::ffi::OsStr::new(HID_REMAP_USERMODE_PATH)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle = CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null_mut(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            );

            if handle == INVALID_HANDLE_VALUE {
                let err = GetLastError();
                return Err(DriverError::OpenFailed(err));
            }

            Ok(Self { handle })
        }
    }

    /// Query the driver version (major, minor).
    pub fn get_version(&self) -> Result<(u16, u16), DriverError> {
        let mut version_raw: u32 = 0;
        let mut bytes_returned: u32 = 0;

        unsafe {
            let success = DeviceIoControl(
                self.handle,
                IOCTL_HID_REMAP_GET_VERSION,
                ptr::null(),
                0,
                &mut version_raw as *mut _ as *mut c_void,
                std::mem::size_of::<u32>() as u32,
                &mut bytes_returned,
                ptr::null_mut(),
            );

            if success == 0 {
                return Err(DriverError::IoControlFailed(GetLastError()));
            }

            let major = (version_raw >> 16) as u16;
            let minor = (version_raw & 0xFFFF) as u16;
            Ok((major, minor))
        }
    }

    /// Push application configuration down to the kernel driver.
    pub fn push_config(&self, config: &AppConfig) -> Result<(), DriverError> {
        let mut driver_config = DriverGlobalConfig::default();
        driver_config.global_enabled = if config.general.enabled { 1 } else { 0 };
        driver_config.tap_timeout_ms = config.ime.tap_timeout_ms;

        let dev_count = config.devices.len().min(MAX_CONFIGURED_DEVICES);
        driver_config.device_count = dev_count as u32;

        for (i, dev) in config.devices.iter().take(dev_count).enumerate() {
            let mut entry = DriverDeviceEntry::default();
            entry.layout_mode = match dev.layout_mode {
                LayoutMode::Passthrough => 0,
                LayoutMode::UsToJis => 1,
                LayoutMode::JisToUs => 2,
            };
            entry.ctrl_ime_enabled = if dev.ctrl_ime_enabled { 1 } else { 0 };
            entry.enabled = 1;

            let wide_id: Vec<u16> = std::ffi::OsStr::new(&dev.instance_id)
                .encode_wide()
                .collect();
            let copy_len = wide_id.len().min(MAX_INSTANCE_ID_LEN - 1);
            entry.instance_id[..copy_len].copy_from_slice(&wide_id[..copy_len]);

            driver_config.devices[i] = entry;
        }

        let mut bytes_returned: u32 = 0;
        unsafe {
            let success = DeviceIoControl(
                self.handle,
                IOCTL_HID_REMAP_SET_CONFIG,
                &driver_config as *const _ as *const c_void,
                std::mem::size_of::<DriverGlobalConfig>() as u32,
                ptr::null_mut(),
                0,
                &mut bytes_returned,
                ptr::null_mut(),
            );

            if success == 0 {
                return Err(DriverError::IoControlFailed(GetLastError()));
            }

            Ok(())
        }
    }

    /// Poll for the next pending IME event queued by the driver.
    pub fn get_next_ime_event(&self) -> Result<Option<DriverImeEvent>, DriverError> {
        let mut raw_event: DriverImeEventRaw = unsafe { std::mem::zeroed() };
        let mut bytes_returned: u32 = 0;

        unsafe {
            let success = DeviceIoControl(
                self.handle,
                IOCTL_HID_REMAP_GET_IME_EVENT,
                ptr::null(),
                0,
                &mut raw_event as *mut _ as *mut c_void,
                std::mem::size_of::<DriverImeEventRaw>() as u32,
                &mut bytes_returned,
                ptr::null_mut(),
            );

            if success == 0 {
                let err = GetLastError();
                // STATUS_NO_MORE_ENTRIES translates to ERROR_NO_MORE_ITEMS (259)
                if err == 259 {
                    return Ok(None);
                }
                return Err(DriverError::IoControlFailed(err));
            }

            let len = raw_event
                .instance_id
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(MAX_INSTANCE_ID_LEN);
            let instance_id = String::from_utf16_lossy(&raw_event.instance_id[..len]);

            Ok(Some(DriverImeEvent {
                instance_id,
                ime_on: raw_event.ime_on != 0,
                timestamp_ms: raw_event.timestamp_ms,
            }))
        }
    }
}

impl Drop for DriverClient {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE && self.handle != ptr::null_mut() {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_struct_sizes() {
        // DriverDeviceEntry: 256*2 + 4 + 1 + 1 + 2 = 520 bytes
        assert_eq!(std::mem::size_of::<DriverDeviceEntry>(), 520);
        // DriverGlobalConfig: 1 + 3 + 4 + 4 + 16*520 = 8332 bytes
        assert_eq!(std::mem::size_of::<DriverGlobalConfig>(), 12 + 16 * 520);
        // DriverImeEventRaw: 512 + 1 + 3 + 4 = 520 bytes
        assert_eq!(std::mem::size_of::<DriverImeEventRaw>(), 520);
    }

    #[test]
    fn test_driver_connection_without_driver_installed() {
        let client = DriverClient::connect();
        assert!(client.is_err());
        println!("Expected failure on system without loaded driver: {:?}", client.err());
    }
}
