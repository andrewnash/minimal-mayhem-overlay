//! Background-app plumbing: tray icon and menu, one running copy, Start with Windows, and a log
//! file in place of the console.

use std::{cell::RefCell, os::windows::io::IntoRawHandle, path::PathBuf};
use windows::{
    Win32::{
        Foundation::*,
        System::{
            Console::{
                ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE,
                STD_OUTPUT_HANDLE, SetStdHandle,
            },
            LibraryLoader::GetModuleHandleW,
            Registry::{
                HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
                RegSetKeyValueW,
            },
            Threading::CreateMutexW,
        },
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{HSTRING, PCWSTR, Result, w},
};

/// Passed by the Start with Windows entry so a login launch starts quietly.
pub const STARTUP_FLAG: &str = "--startup";
const CLASS: PCWSTR = w!("MinimalMayhemTray");
const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("MinimalMayhemOverlay");
const CALLBACK: u32 = WM_APP + 1;
/// Posted by a second launch so the running copy can say where it is.
const ALREADY_RUNNING: u32 = WM_APP + 2;
const MENU_STARTUP: usize = 1;
const MENU_LOGS: usize = 2;
const MENU_QUIT: usize = 3;
static ICON: &[u8] = include_bytes!("../assets/icon.ico");

thread_local! {
    static ICON_DATA: RefCell<Option<NOTIFYICONDATAW>> = const { RefCell::new(None) };
    static TITLE: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    static TASKBAR_CREATED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// `%LOCALAPPDATA%\MinimalMayhemOverlay`, home of the log and saved scan samples.
pub fn data_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("MinimalMayhemOverlay"))
}

/// Prints to the terminal that launched us, for one-shot commands like `--check-ocr`.
pub fn use_parent_console() {
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

/// Without redirected output (a normal double-click or login launch), write stdout and stderr
/// to `overlay.log`, replaced on each launch.
pub fn redirect_logs() {
    let redirected = unsafe { GetStdHandle(STD_ERROR_HANDLE) }
        .is_ok_and(|handle| !handle.is_invalid() && !handle.0.is_null());
    let Some(dir) = data_dir().filter(|_| !redirected) else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let Ok(file) = std::fs::File::create(dir.join("overlay.log")) else {
        return;
    };
    let handle = HANDLE(file.into_raw_handle());
    unsafe {
        let _ = SetStdHandle(STD_OUTPUT_HANDLE, handle);
        let _ = SetStdHandle(STD_ERROR_HANDLE, handle);
    }
}

/// True when another copy is already running; that copy is asked to show a notice.
pub fn already_running() -> bool {
    unsafe {
        // Never closed: the mutex marks this process as the running copy until it exits.
        let mutex = CreateMutexW(None, false, w!("Local\\MinimalMayhemOverlay"));
        if mutex.is_err() || GetLastError() != ERROR_ALREADY_EXISTS {
            return false;
        }
        if let Ok(hwnd) = FindWindowW(CLASS, PCWSTR::null()) {
            let _ = PostMessageW(Some(hwnd), ALREADY_RUNNING, WPARAM(0), LPARAM(0));
        }
        true
    }
}

pub fn alert(text: &str) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            w!("Minimal Mayhem Overlay"),
            MB_OK | MB_ICONWARNING,
        );
    }
}

/// The notification-area icon. Dropping it removes the icon.
pub struct Tray;

impl Tray {
    pub fn new(title: &str) -> Result<Self> {
        unsafe {
            let instance = HINSTANCE(GetModuleHandleW(None)?.0);
            let descriptor = WNDCLASSW {
                lpfnWndProc: Some(tray_proc),
                hInstance: instance,
                lpszClassName: CLASS,
                ..Default::default()
            };
            if RegisterClassW(&descriptor) == 0 {
                return Err(windows::core::Error::from_thread());
            }
            // A hidden top-level window (not message-only) so Explorer's TaskbarCreated
            // broadcast reaches it after an Explorer restart.
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                CLASS,
                w!("Minimal Mayhem Overlay"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            )?;
            TASKBAR_CREATED.set(RegisterWindowMessageW(w!("TaskbarCreated")));
            TITLE.replace(title.encode_utf16().chain([0]).collect());
            let mut data = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: CALLBACK,
                hIcon: load_icon(GetSystemMetrics(SM_CXSMICON)).unwrap_or_default(),
                ..Default::default()
            };
            copy_text(&mut data.szTip, title);
            Shell_NotifyIconW(NIM_ADD, &data).ok()?;
            ICON_DATA.replace(Some(data));
            Ok(Self)
        }
    }

    pub fn notify(&self, title: &str, text: &str) {
        notify(title, text);
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        if let Some(data) = ICON_DATA.take() {
            let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
        }
    }
}

fn notify(title: &str, text: &str) {
    ICON_DATA.with_borrow(|data| {
        let Some(mut data) = *data else {
            return;
        };
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = NIIF_USER | NIIF_NOSOUND;
        copy_text(&mut data.szInfoTitle, title);
        copy_text(&mut data.szInfo, text);
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    });
}

fn copy_text<const N: usize>(target: &mut [u16; N], text: &str) {
    for (slot, unit) in target
        .iter_mut()
        .take(N - 1)
        .zip(text.encode_utf16().chain(std::iter::repeat(0)))
    {
        *slot = unit;
    }
}

/// Picks the bundled icon image closest to `size` (preferring larger) and makes an HICON of it.
fn load_icon(size: i32) -> Option<HICON> {
    let count = usize::from(u16::from_le_bytes([*ICON.get(4)?, *ICON.get(5)?]));
    let entry = (0..count)
        .filter_map(|index| ICON.get(6 + 16 * index..6 + 16 * (index + 1)))
        .min_by_key(|entry| {
            let width = if entry[0] == 0 {
                256
            } else {
                i32::from(entry[0])
            };
            if width >= size {
                width - size
            } else {
                1000 + size - width
            }
        })?;
    let length = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
    let offset = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
    let image = ICON.get(offset..offset + length)?;
    unsafe { CreateIconFromResourceEx(image, true, 0x0003_0000, size, size, LR_DEFAULTCOLOR) }.ok()
}

fn startup_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\" {STARTUP_FLAG}", exe.display()))
}

/// Whether the Windows Run entry launches this executable.
fn starts_with_windows() -> bool {
    let mut value = [0u16; 1024];
    let mut bytes = size_of_val(&value) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(value.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    let length = (bytes as usize / 2).saturating_sub(1).min(value.len());
    status == ERROR_SUCCESS
        && startup_command().is_some_and(|command| {
            String::from_utf16_lossy(&value[..length]).eq_ignore_ascii_case(&command)
        })
}

fn set_startup(enabled: bool) {
    let status = if enabled {
        let Some(command) = startup_command() else {
            return;
        };
        let value: Vec<u16> = command.encode_utf16().chain([0]).collect();
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ.0,
                Some(value.as_ptr().cast()),
                (value.len() * 2) as u32,
            )
        }
    } else {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE) }
    };
    if status == ERROR_SUCCESS {
        println!("Start with Windows {}", if enabled { "on" } else { "off" });
    } else {
        eprintln!("Could not change Start with Windows: {status:?}");
    }
}

unsafe fn show_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        TITLE.with_borrow(|title| {
            let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, PCWSTR(title.as_ptr()));
        });
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let startup = starts_with_windows();
        let checked = if startup { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(
            menu,
            MF_STRING | checked,
            MENU_STARTUP,
            w!("Start with Windows"),
        );
        let _ = AppendMenuW(menu, MF_STRING, MENU_LOGS, w!("Open log folder"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Quit"));
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        // Required so the menu closes when the user clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let choice = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
            point.x,
            point.y,
            None,
            hwnd,
            None,
        )
        .0 as usize;
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        match choice {
            MENU_STARTUP => set_startup(!startup),
            MENU_LOGS => {
                if let Some(dir) = data_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    ShellExecuteW(
                        None,
                        w!("open"),
                        &HSTRING::from(dir.as_os_str()),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    );
                }
            }
            MENU_QUIT => PostQuitMessage(0),
            _ => {}
        }
    }
}

unsafe extern "system" fn tray_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            CALLBACK => {
                if matches!((lparam.0 & 0xffff) as u32, WM_LBUTTONUP | WM_RBUTTONUP) {
                    show_menu(hwnd);
                }
                LRESULT(0)
            }
            ALREADY_RUNNING => {
                notify(
                    "Already running",
                    "Minimal Mayhem Overlay is in the notification area. Right-click its icon for options.",
                );
                LRESULT(0)
            }
            // The installer closes the running copy this way so the tray icon goes with it.
            WM_CLOSE => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ if msg != 0 && msg == TASKBAR_CREATED.get() => {
                ICON_DATA.with_borrow(|data| {
                    if let Some(data) = data {
                        let _ = Shell_NotifyIconW(NIM_ADD, data);
                    }
                });
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
