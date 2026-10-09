use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW, PostMessageW, PostQuitMessage,
    RegisterClassExW, SetForegroundWindow, TrackPopupMenu, TranslateMessage, HICON, HMENU,
    IDI_APPLICATION, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, TPM_BOTTOMALIGN,
    TPM_RIGHTBUTTON, WM_COMMAND, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NULL,
    WM_RBUTTONUP, WM_USER, WNDCLASSEXW,
};

const WM_TRAY_CALLBACK: u32 = WM_USER + 100;
const ID_OPEN: usize = 1001;
const ID_TOGGLE: usize = 1002;
const ID_EXIT: usize = 1003;

static GLOBAL_ENABLED: AtomicBool = AtomicBool::new(true);

use std::sync::Mutex;

type Callback = Box<dyn Fn() + Send + Sync + 'static>;

struct TrayCallbacks {
    on_open: Callback,
    on_toggle: Callback,
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

pub fn start_tray_service<FOpen, FToggle, FExit>(
    on_open: FOpen,
    on_toggle: FToggle,
    on_exit: FExit,
    initial_enabled: bool,
) -> TrayManager
where
    FOpen: Fn() + Send + Sync + 'static,
    FToggle: Fn() + Send + Sync + 'static,
    FExit: Fn() + Send + Sync + 'static,
{
    GLOBAL_ENABLED.store(initial_enabled, Ordering::Relaxed);

    {
        let mut guard = CALLBACKS.lock().unwrap();
        *guard = Some(TrayCallbacks {
            on_open: Box::new(on_open),
            on_toggle: Box::new(on_toggle),
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
    let wide_exit = to_wide("❌ 終了 (&X)");

    let toggle_flags = if is_enabled {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING | MF_UNCHECKED
    };

    AppendMenuW(hmenu, MF_STRING, ID_OPEN, wide_open.as_ptr());
    AppendMenuW(hmenu, toggle_flags, ID_TOGGLE, wide_toggle.as_ptr());
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

        let icon = get_app_icon();

        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_INFO;
        nid.uCallbackMessage = WM_TRAY_CALLBACK;
        nid.hIcon = icon;
        copy_to_wide_buf(&mut nid.szTip, "HID Keyboard Layout Remapper");
        copy_to_wide_buf(&mut nid.szInfo, "タスクトレイに常駐しました。クリックで設定を開けます。");
        copy_to_wide_buf(&mut nid.szInfoTitle, "HID Keyboard Layout Remapper");
        nid.dwInfoFlags = NIIF_INFO;

        Shell_NotifyIconW(NIM_ADD, &nid);
        info!("System tray icon registered successfully.");

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
