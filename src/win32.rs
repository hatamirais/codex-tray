use std::{ffi::c_void, mem::size_of};

use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
                Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CREATESTRUCTW, CW_USEDEFAULT, CreatePopupMenu, CreateWindowExW,
                DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
                GetCursorPos, GetMessageW, GetWindowLongPtrW, HMENU, IDC_ARROW, IDI_APPLICATION,
                IsWindow, LoadCursorW, LoadIconW, MF_DISABLED, MF_SEPARATOR, MF_STRING, MSG,
                PostMessageW, PostQuitMessage, RegisterClassExW, SetForegroundWindow,
                SetWindowLongPtrW, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, TrackPopupMenu,
                TranslateMessage, UnregisterClassW, WM_APP, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP,
                WM_NCCREATE, WM_NULL, WM_RBUTTONUP, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
            },
        },
    },
    core::{Error, PCWSTR, Result, w},
};

use crate::{
    tray::{self, ICON_ID, MENU_QUIT, MENU_REFRESH},
    usage::CodexUsage,
};

const TRAY_CALLBACK: u32 = WM_APP + 1;
const CLASS_NAME: PCWSTR = w!("CodexTrayMessageWindow");

type RefreshUsage = fn() -> CodexUsage;

pub fn run(initial_usage: CodexUsage, refresh_usage: RefreshUsage) -> Result<()> {
    // All Win32 calls and raw-pointer access are confined to this module.
    unsafe { run_native(initial_usage, refresh_usage) }
}

struct AppState {
    hwnd: HWND,
    menu: HMENU,
    tray_added: bool,
    refresh_usage: RefreshUsage,
}

impl AppState {
    unsafe fn add_tray_icon(&mut self, usage: &CodexUsage) -> Result<()> {
        let icon = unsafe { LoadIconW(None, IDI_APPLICATION)? };
        let mut data = notify_icon_data(self.hwnd, usage);
        data.uFlags |= NIF_ICON;
        data.hIcon = icon;

        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
            self.tray_added = true;
            Ok(())
        } else {
            Err(Error::from_thread())
        }
    }

    unsafe fn update_tooltip(&self, usage: &CodexUsage) -> Result<()> {
        let data = notify_icon_data(self.hwnd, usage);
        if unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) }.as_bool() {
            Ok(())
        } else {
            Err(Error::from_thread())
        }
    }

    unsafe fn remove_tray_icon(&mut self) {
        if !self.tray_added {
            return;
        }

        let data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: ICON_ID,
            ..Default::default()
        };
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
        self.tray_added = false;
    }

    unsafe fn show_menu(&self) {
        let mut cursor = POINT::default();
        if unsafe { GetCursorPos(&mut cursor) }.is_err() {
            return;
        }

        // This foreground-window handshake is required so Windows dismisses the popup correctly.
        let _ = unsafe { SetForegroundWindow(self.hwnd) };
        unsafe {
            let _ = TrackPopupMenu(
                self.menu,
                TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
                cursor.x,
                cursor.y,
                None,
                self.hwnd,
                None,
            );
        };
        let _ = unsafe { PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0)) };
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        unsafe {
            self.remove_tray_icon();
            let _ = DestroyMenu(self.menu);
        }
    }
}

struct WindowRegistration {
    instance: HINSTANCE,
}

impl Drop for WindowRegistration {
    fn drop(&mut self) {
        unsafe {
            let _ = UnregisterClassW(CLASS_NAME, Some(self.instance));
        }
    }
}

struct WindowHandle(HWND);

impl Drop for WindowHandle {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(Some(self.0)).as_bool() {
                let _ = DestroyWindow(self.0);
            }
        }
    }
}

unsafe fn run_native(initial_usage: CodexUsage, refresh_usage: RefreshUsage) -> Result<()> {
    let module = unsafe { GetModuleHandleW(None)? };
    let instance = HINSTANCE(module.0);
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };

    if unsafe { RegisterClassExW(&class) } == 0 {
        return Err(Error::from_thread());
    }
    let _registration = WindowRegistration { instance };

    let menu = unsafe { create_menu()? };
    let mut state = Box::new(AppState {
        hwnd: HWND::default(),
        menu,
        tray_added: false,
        refresh_usage,
    });

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            CLASS_NAME,
            w!("Codex Tray"),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            None,
            None,
            Some(instance),
            Some((&raw mut *state).cast::<c_void>()),
        )?
    };
    state.hwnd = hwnd;
    let _window = WindowHandle(hwnd);
    unsafe { state.add_tray_icon(&initial_usage)? };

    let mut message = MSG::default();
    loop {
        let status = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if status.0 == -1 {
            return Err(Error::from_hresult(unsafe { GetLastError() }.to_hresult()));
        }
        if !status.as_bool() {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    Ok(())
}

unsafe fn create_menu() -> Result<HMENU> {
    let menu = unsafe { CreatePopupMenu()? };

    let result = (|| unsafe {
        AppendMenuW(menu, MF_STRING | MF_DISABLED, 0, w!("Codex Usage"))?;
        AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
        AppendMenuW(menu, MF_STRING, MENU_REFRESH, w!("Refresh"))?;
        AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Quit"))?;
        Ok(())
    })();

    if let Err(error) = result {
        unsafe { DestroyMenu(menu) }.ok();
        return Err(error);
    }

    Ok(menu)
}

fn notify_icon_data(hwnd: HWND, usage: &CodexUsage) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        uFlags: NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: TRAY_CALLBACK,
        ..Default::default()
    };
    write_wide(&mut data.szTip, &tray::tooltip(usage));
    data
}

fn write_wide<const N: usize>(destination: &mut [u16; N], value: &str) {
    destination.fill(0);
    for (slot, code_unit) in destination
        .iter_mut()
        .take(N.saturating_sub(1))
        .zip(value.encode_utf16())
    {
        *slot = code_unit;
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam.0 as *const CREATESTRUCTW;
        if create.is_null() {
            return LRESULT(0);
        }
        let state = unsafe { (*create).lpCreateParams.cast::<AppState>() };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
        return LRESULT(1);
    }

    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut AppState;

    match message {
        TRAY_CALLBACK if !state.is_null() => {
            let event = lparam.0 as u32;
            if event == WM_LBUTTONUP || event == WM_RBUTTONUP {
                unsafe { (*state).show_menu() };
            }
            LRESULT(0)
        }
        WM_COMMAND if !state.is_null() => {
            match wparam.0 & 0xffff {
                MENU_REFRESH => {
                    let usage = unsafe { ((*state).refresh_usage)() };
                    let _ = unsafe { (*state).update_tooltip(&usage) };
                }
                MENU_QUIT => {
                    let _ = unsafe { DestroyWindow(hwnd) };
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if !state.is_null() {
                unsafe { (*state).remove_tray_icon() };
            }
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}
