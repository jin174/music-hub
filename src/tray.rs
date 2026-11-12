use crate::app::App;
use std::sync::Arc;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE,
    REG_SZ,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, SetTimer, TrackPopupMenuEx, TranslateMessage, IDI_APPLICATION, MF_CHECKED,
    MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, TPM_BOTTOMALIGN,
    TPM_RIGHTALIGN, WM_COMMAND, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP,
    WM_TIMER, WM_USER, WNDCLASSW,
};

const WM_TRAYICON: u32 = WM_USER + 101;
const ID_TRAY_CONSOLE: usize = 1001;
const ID_TRAY_MINI: usize = 1002;
const ID_TRAY_RECORD: usize = 1003;
const ID_TRAY_SCROBBLE: usize = 1004;
const ID_TRAY_AUTOSTART: usize = 1005;
const ID_TRAY_EXIT: usize = 1006;

pub fn is_autostart_enabled() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            0,
            KEY_READ,
            &mut hkey,
        )
        .is_ok()
        {
            let mut buf = [0u8; 1024];
            let mut buf_len = buf.len() as u32;
            let res = RegQueryValueExW(
                hkey,
                w!("MusicHub"),
                None,
                None,
                Some(buf.as_mut_ptr()),
                Some(&mut buf_len),
            );
            let _ = RegCloseKey(hkey);
            return res.is_ok();
        }
    }
    false
}

pub fn set_autostart(enable: bool) {
    unsafe {
        let mut hkey = HKEY::default();
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
        .is_ok()
        {
            if enable {
                if let Ok(exe_path) = std::env::current_exe() {
                    let mut path_u16: Vec<u16> = exe_path
                        .to_string_lossy()
                        .encode_utf16()
                        .collect();
                    path_u16.push(0);
                    let bytes = std::slice::from_raw_parts(
                        path_u16.as_ptr() as *const u8,
                        path_u16.len() * 2,
                    );
                    let _ = RegSetValueExW(
                        hkey,
                        w!("MusicHub"),
                        0,
                        REG_SZ,
                        Some(bytes),
                    );
                }
            } else {
                let _ = RegDeleteValueW(hkey, w!("MusicHub"));
            }
            let _ = RegCloseKey(hkey);
        }
    }
}

pub fn spawn_tray_thread(app: Arc<App>) {
    std::thread::Builder::new()
        .name("system-tray".to_string())
        .spawn(move || {
            let class_name = w!("MusicHub_Tray_Class");
            let hinstance: HINSTANCE = unsafe { GetModuleHandleW(None).unwrap_or_default().into() };

            unsafe {
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(tray_wnd_proc),
                    hInstance: hinstance,
                    lpszClassName: class_name,
                    ..Default::default()
                };
                RegisterClassW(&wc);

                let hwnd = CreateWindowExW(
                    windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                    class_name,
                    w!("MusicHub Tray Window"),
                    windows::Win32::UI::WindowsAndMessaging::WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    hinstance,
                    None,
                )
                .expect("Failed to create tray hidden window");

                windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                    Arc::into_raw(app.clone()) as isize,
                );

                let icon = LoadIconW(hinstance, PCWSTR(1 as _)).unwrap_or_else(|_| {
                    LoadIconW(None, IDI_APPLICATION).unwrap()
                });

                let mut nid = NOTIFYICONDATAW {
                    cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                    hWnd: hwnd,
                    uID: 1,
                    uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                    uCallbackMessage: WM_TRAYICON,
                    hIcon: icon,
                    ..Default::default()
                };

                let tip_text = "Music Hub · 音乐伴侣\0";
                for (i, c) in tip_text.encode_utf16().enumerate().take(127) {
                    nid.szTip[i] = c;
                }

                let _ = Shell_NotifyIconW(NIM_ADD, &nid);

                // Update tray tooltip every 3s
                SetTimer(hwnd, 1, 3000, None);

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            }
        })
        .expect("Failed to spawn tray thread");
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
    ) as *const App;

    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let app = Arc::from_raw(ptr);
    let app_ref = app.clone();
    let _ = Arc::into_raw(app); // Keep ref count intact

    match msg {
        WM_TIMER => {
            // Update tooltip text
            let now_opt = app_ref.monitor.snapshot();

            let mode_str = if app_ref.cfg.is_scrobbling() {
                "打卡中"
            } else {
                "只记录"
            };
            let mut tip = format!("Music Hub · {}", mode_str);
            if let Some(n) = now_opt {
                tip.push_str(&format!("\n♪ {} - {}", n.title, n.artist));
            }
            tip.push('\0');

            let mut nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_TIP,
                ..Default::default()
            };
            for (i, c) in tip.encode_utf16().enumerate().take(127) {
                nid.szTip[i] = c;
            }
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
            LRESULT(0)
        }
        WM_TRAYICON => {
            let event = lparam.0 as u32;
            if event == WM_RBUTTONUP {
                show_tray_menu(hwnd, &app_ref);
            } else if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                app_ref.open_console_window(None);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let cmd_id = (wparam.0 & 0xffff) as usize;
            match cmd_id {
                ID_TRAY_CONSOLE => {
                    app_ref.open_console_window(None);
                }
                ID_TRAY_MINI => {
                    app_ref.open_mini_window_sync();
                }
                ID_TRAY_RECORD => {
                    app_ref.cfg.update(|d| d.mode = "record".to_string());
                }
                ID_TRAY_SCROBBLE => {
                    if !app_ref.cfg.is_authorized() {
                        app_ref.open_console_with_toast(
                            Some("settings"),
                            Some("请先完成 Last.fm 授权后再开启打卡模式".to_string()),
                        );
                    } else {
                        app_ref.cfg.update(|d| d.mode = "scrobble".to_string());
                        app_ref.scrobbler.kick();
                    }
                }
                ID_TRAY_AUTOSTART => {
                    let current = is_autostart_enabled();
                    set_autostart(!current);
                }
                ID_TRAY_EXIT => {
                    PostQuitMessage(0);
                    std::process::exit(0);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_tray_menu(hwnd: HWND, app: &App) {
    let mut pt = POINT::default();
    GetCursorPos(&mut pt).unwrap_or_default();

    let hmenu = CreatePopupMenu().expect("CreatePopupMenu failed");

    let is_scrobble = app.cfg.is_scrobbling();
    let is_record = !is_scrobble;
    let autostart = is_autostart_enabled();

    AppendMenuW(hmenu, MF_STRING, ID_TRAY_CONSOLE, w!("打开控制台")).unwrap();
    AppendMenuW(hmenu, MF_STRING, ID_TRAY_MINI, w!("打开迷你挂件")).unwrap();
    AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null()).unwrap();

    let record_flags = MF_STRING | if is_record { MF_CHECKED } else { MF_UNCHECKED };
    AppendMenuW(hmenu, record_flags, ID_TRAY_RECORD, w!("只记录（不打卡）")).unwrap();

    let scrobble_flags = MF_STRING | if is_scrobble { MF_CHECKED } else { MF_UNCHECKED };
    AppendMenuW(hmenu, scrobble_flags, ID_TRAY_SCROBBLE, w!("记录 + 打卡到 Last.fm")).unwrap();

    AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null()).unwrap();

    let autostart_flags = MF_STRING | if autostart { MF_CHECKED } else { MF_UNCHECKED };
    AppendMenuW(hmenu, autostart_flags, ID_TRAY_AUTOSTART, w!("开机自启")).unwrap();

    AppendMenuW(hmenu, MF_STRING, ID_TRAY_EXIT, w!("退出")).unwrap();

    // Make "打开控制台" the default bold item
    windows::Win32::UI::WindowsAndMessaging::SetMenuDefaultItem(hmenu, ID_TRAY_CONSOLE as u32, 0).unwrap_or_default();

    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenuEx(
        hmenu,
        (TPM_RIGHTALIGN | TPM_BOTTOMALIGN).0,
        pt.x,
        pt.y,
        hwnd,
        None,
    );
    DestroyMenu(hmenu).unwrap_or_default();
}
