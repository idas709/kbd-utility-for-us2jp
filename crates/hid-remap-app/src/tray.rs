use std::collections::HashMap;
use std::os::windows::ffi::OsStringExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tracing::info;
use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList, RegisterRawInputDevices,
    HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTDEVICELIST, RAWINPUTHEADER, RIDEV_INPUTSINK,
    RID_INPUT, RIDI_DEVICENAME, RIM_TYPEKEYBOARD,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW, PostMessageW, PostQuitMessage,
    RegisterClassExW, SetForegroundWindow, TrackPopupMenu, TranslateMessage, HICON, HMENU,
    IDI_APPLICATION, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, TPM_BOTTOMALIGN,
    TPM_RIGHTBUTTON, WM_COMMAND, WM_DESTROY, WM_INPUT, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NULL,
    WM_RBUTTONUP, WM_USER, WNDCLASSEXW,
};

static DEVICE_CACHE: Mutex<Option<HashMap<isize, String>>> = Mutex::new(None);

pub fn populate_device_cache() {
    let mut num_devices: u32 = 0;
    let struct_size = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
    unsafe {
        let res = GetRawInputDeviceList(std::ptr::null_mut(), &mut num_devices, struct_size);
        if res != u32::MAX && num_devices > 0 {
            let mut list: Vec<RAWINPUTDEVICELIST> = vec![std::mem::zeroed(); num_devices as usize];
            let actual = GetRawInputDeviceList(list.as_mut_ptr(), &mut num_devices, struct_size);
            if actual != u32::MAX {
                for dev in list.into_iter().take(actual as usize) {
                    if dev.dwType == RIM_TYPEKEYBOARD && !dev.hDevice.is_null() {
                        let _ = resolve_device_instance_id(dev.hDevice);
                    }
                }
            }
        }
    }
}

fn resolve_device_instance_id(h_device: HANDLE) -> Option<String> {
    let key = h_device as isize;
    if let Ok(guard) = DEVICE_CACHE.lock() {
        if let Some(cache) = guard.as_ref() {
            if let Some(id) = cache.get(&key) {
                return Some(id.clone());
            }
        }
    }

    let mut name_len: u32 = 0;
    unsafe {
        GetRawInputDeviceInfoW(h_device, RIDI_DEVICENAME, std::ptr::null_mut(), &mut name_len);
        if name_len > 0 {
            let mut buf: Vec<u16> = vec![0; name_len as usize];
            let copied = GetRawInputDeviceInfoW(
                h_device,
                RIDI_DEVICENAME,
                buf.as_mut_ptr() as *mut _,
                &mut name_len,
            );
            if copied > 0 && copied != u32::MAX {
                let len = if buf.last() == Some(&0) {
                    buf.len() - 1
                } else {
                    buf.len()
                };
                let path_os = std::ffi::OsString::from_wide(&buf[..len]);
                let dev_path = path_os.to_string_lossy().to_string();
                let instance_id = hid_remap_device::device_path_to_instance_id(&dev_path);

                if let Ok(mut guard) = DEVICE_CACHE.lock() {
                    let cache = guard.get_or_insert_with(HashMap::new);
                    cache.insert(key, instance_id.clone());
                }
                return Some(instance_id);
            }
        }
    }
    None
}

const WM_TRAY_CALLBACK: u32 = WM_USER + 100;
const WM_POWERBROADCAST: u32 = 0x0218;
const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;
const PBT_APMRESUMESUSPEND: usize = 0x0007;

const ID_OPEN: usize = 1001;
const ID_TOGGLE: usize = 1002;
const ID_STARTUP: usize = 1003;
const ID_EXIT: usize = 1004;

static GLOBAL_ENABLED: AtomicBool = AtomicBool::new(true);
static STARTUP_ENABLED: AtomicBool = AtomicBool::new(true);

type Callback = Box<dyn Fn() + Send + Sync + 'static>;

struct TrayCallbacks {
    on_open: Callback,
    on_toggle: Callback,
    on_toggle_startup: Callback,
    on_exit: Callback,
}

static CALLBACKS: Mutex<Option<TrayCallbacks>> = Mutex::new(None);

pub struct TrayManager {
    #[allow(dead_code)]
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn copy_to_wide_buf(dest: &mut [u16], src: &str) {
    let wide = src.encode_utf16().take(dest.len() - 1);
    let mut i = 0;
    for ch in wide {
        dest[i] = ch;
        i += 1;
    }
    if i < dest.len() {
        dest[i] = 0;
    }
}

pub fn update_tray_enabled(enabled: bool) {
    GLOBAL_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn update_tray_startup(enabled: bool) {
    STARTUP_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn start_tray_service<FOpen, FToggle, FStartup, FExit>(
    on_open: FOpen,
    on_toggle: FToggle,
    on_toggle_startup: FStartup,
    on_exit: FExit,
    initial_enabled: bool,
    initial_startup: bool,
) -> TrayManager
where
    FOpen: Fn() + Send + Sync + 'static,
    FToggle: Fn() + Send + Sync + 'static,
    FStartup: Fn() + Send + Sync + 'static,
    FExit: Fn() + Send + Sync + 'static,
{
    GLOBAL_ENABLED.store(initial_enabled, Ordering::Relaxed);
    STARTUP_ENABLED.store(initial_startup, Ordering::Relaxed);

    {
        let mut guard = CALLBACKS.lock().unwrap();
        *guard = Some(TrayCallbacks {
            on_open: Box::new(on_open),
            on_toggle: Box::new(on_toggle),
            on_toggle_startup: Box::new(on_toggle_startup),
            on_exit: Box::new(on_exit),
        });
    }

    let handle = std::thread::spawn(move || {
        run_tray_thread();
    });

    TrayManager {
        thread_handle: Some(handle),
    }
}

fn get_app_icon() -> HICON {
    unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());
        // WinRes embeds the primary icon as resource ID 1
        let icon = LoadIconW(hinstance, 1 as *const u16);
        if !icon.is_null() {
            icon
        } else {
            LoadIconW(std::ptr::null_mut(), IDI_APPLICATION)
        }
    }
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY_CALLBACK => {
            let event = l_param as u32;
            if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                if let Ok(guard) = CALLBACKS.lock() {
                    if let Some(cb) = guard.as_ref() {
                        (cb.on_open)();
                    }
                }
            } else if event == WM_RBUTTONUP {
                show_tray_menu(hwnd);
            }
            0
        }
        WM_COMMAND => {
            let cmd = (w_param & 0xFFFF) as usize;
            match cmd {
                ID_OPEN => {
                    if let Ok(guard) = CALLBACKS.lock() {
                        if let Some(cb) = guard.as_ref() {
                            (cb.on_open)();
                        }
                    }
                }
                ID_TOGGLE => {
                    if let Ok(guard) = CALLBACKS.lock() {
                        if let Some(cb) = guard.as_ref() {
                            (cb.on_toggle)();
                        }
                    }
                }
                ID_STARTUP => {
                    if let Ok(guard) = CALLBACKS.lock() {
                        if let Some(cb) = guard.as_ref() {
                            (cb.on_toggle_startup)();
                        }
                    }
                }
                ID_EXIT => {
                    if let Ok(guard) = CALLBACKS.lock() {
                        if let Some(cb) = guard.as_ref() {
                            (cb.on_exit)();
                        }
                    }
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_INPUT => {
            let hraw = l_param as HRAWINPUT;
            let mut buffer = [0u8; 128];
            let mut size = buffer.len() as u32;
            let header_size = std::mem::size_of::<RAWINPUTHEADER>() as u32;
            let res = GetRawInputData(
                hraw,
                RID_INPUT,
                buffer.as_mut_ptr() as *mut _,
                &mut size,
                header_size,
            );
            if res != u32::MAX && res > 0 {
                let raw = &*(buffer.as_ptr() as *const RAWINPUT);
                let h_device = raw.header.hDevice;
                if !h_device.is_null() {
                    if let Some(instance_id) = resolve_device_instance_id(h_device) {
                        crate::standalone_engine::set_last_typed_device(instance_id);
                    }
                }
            }
            DefWindowProcW(hwnd, msg, w_param, l_param)
        }
        WM_POWERBROADCAST => {
            let pbt = w_param as usize;
            if pbt == PBT_APMRESUMEAUTOMATIC || pbt == PBT_APMRESUMESUSPEND {
                info!("Power resume event detected ({:#x}). Re-anchoring keyboard hook...", pbt);
                crate::standalone_engine::refresh_hook();
            }
            1
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w_param, l_param),
    }
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let hmenu: HMENU = CreatePopupMenu();
    if hmenu.is_null() {
        return;
    }

    let wide_open = to_wide("⚙ 設定画面を開く (&O)");
    let is_enabled = GLOBAL_ENABLED.load(Ordering::Relaxed);
    let toggle_text = if is_enabled {
        "✔ リマップ有効 (動作中) (&R)"
    } else {
        "✖ リマップ一時停止中 (&R)"
    };
    let wide_toggle = to_wide(toggle_text);

    let is_startup = STARTUP_ENABLED.load(Ordering::Relaxed);
    let startup_text = if is_startup {
        "✔ スタートアップ起動 (有効) (&S)"
    } else {
        "スタートアップ起動 (無効) (&S)"
    };
    let wide_startup = to_wide(startup_text);
    let wide_exit = to_wide("❌ 終了 (&X)");

    let toggle_flags = if is_enabled {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING | MF_UNCHECKED
    };

    let startup_flags = if is_startup {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING | MF_UNCHECKED
    };

    AppendMenuW(hmenu, MF_STRING, ID_OPEN, wide_open.as_ptr());
    AppendMenuW(hmenu, toggle_flags, ID_TOGGLE, wide_toggle.as_ptr());
    AppendMenuW(hmenu, startup_flags, ID_STARTUP, wide_startup.as_ptr());
    AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
    AppendMenuW(hmenu, MF_STRING, ID_EXIT, wide_exit.as_ptr());

    let mut pt: POINT = std::mem::zeroed();
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    TrackPopupMenu(
        hmenu,
        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
        pt.x,
        pt.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(hmenu);
}

fn run_tray_thread() {
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);

        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name = to_wide("HIDRemapTrayWindowClass");

        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(tray_wnd_proc);
        wc.hInstance = hinstance;
        wc.lpszClassName = class_name.as_ptr();

        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null_mut(),
        );

        if hwnd.is_null() {
            info!("Failed to create tray helper window");
            return;
        }

        // Register for Raw Input keyboard events to identify physical devices
        let rid = RAWINPUTDEVICE {
            usUsagePage: 0x01, // Generic Desktop Controls
            usUsage: 0x06,     // Keyboard
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        };
        let reg_ok = RegisterRawInputDevices(
            &rid,
            1,
            std::mem::size_of::<RAWINPUTDEVICE>() as u32,
        );
        info!("Raw input sink registered on tray window: {}", reg_ok != 0);

        let icon = get_app_icon();

        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_CALLBACK;
        nid.hIcon = icon;
        copy_to_wide_buf(&mut nid.szTip, "HID Keyboard Layout Remapper");

        Shell_NotifyIconW(NIM_ADD, &nid);
        info!("System tray icon registered successfully (silent).");

        // Message pump
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Clean up tray icon on exit
        Shell_NotifyIconW(NIM_DELETE, &nid);
        info!("System tray icon deleted.");
    }
}
