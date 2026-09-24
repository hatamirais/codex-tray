use std::{ffi::c_void, mem::size_of, time::SystemTime};

use windows::{
    Win32::{
        Foundation::{
            COLORREF, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
        },
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, CLEARTYPE_QUALITY,
            CLIP_DEFAULT_PRECIS, CreateBitmap, CreateDIBSection, CreateFontW, CreatePen,
            CreateRoundRectRgn, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS,
            DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER,
            DeleteObject, DrawTextW, Ellipse, EndPaint, FF_DONTCARE, FW_MEDIUM, FW_NORMAL,
            FillRect, GetMonitorInfoW, HDC, HFONT, LineTo, MONITOR_DEFAULTTONEAREST, MONITORINFO,
            MonitorFromPoint, MoveToEx, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, PtInRect,
            RoundRect, SelectObject, SetBkMode, SetTextColor, SetWindowRgn, TRANSPARENT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
                Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CREATESTRUCTW, CW_USEDEFAULT, CreateIconIndirect, CreatePopupMenu,
                CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DestroyWindow,
                DispatchMessageW, GWLP_USERDATA, GetCursorPos, GetMessageW, GetWindowLongPtrW,
                GetWindowRect, HICON, HMENU, HWND_TOPMOST, ICONINFO, IDC_ARROW, IsWindow,
                IsWindowVisible, KillTimer, LWA_ALPHA, LoadCursorW, MF_DISABLED, MF_SEPARATOR,
                MF_STRING, MSG, PostMessageW, PostQuitMessage, RegisterClassExW, SW_HIDE,
                SWP_NOACTIVATE, SWP_SHOWWINDOW, SetForegroundWindow, SetLayeredWindowAttributes,
                SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TPM_BOTTOMALIGN,
                TPM_LEFTALIGN, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, UnregisterClassW,
                WM_APP, WM_COMMAND, WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONUP, WM_MOUSEMOVE,
                WM_NCCREATE, WM_NULL, WM_PAINT, WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED,
                WS_EX_TOOLWINDOW, WS_OVERLAPPED, WS_POPUP,
            },
        },
    },
    core::{Error, PCWSTR, Result, w},
};

use crate::{
    icon,
    tray::{self, ICON_ID, MENU_QUIT, MENU_REFRESH},
    usage::CodexUsage,
};

const TRAY_CALLBACK: u32 = WM_APP + 1;
const MESSAGE_CLASS_NAME: PCWSTR = w!("CodexTrayMessageWindow");
const POPUP_CLASS_NAME: PCWSTR = w!("CodexTrayUsagePopup");
const POPUP_WIDTH: i32 = 280;
const POPUP_HEIGHT: i32 = 160;
const POPUP_RADIUS: i32 = 6;
const HOVER_TIMER_ID: usize = 1;
const HOVER_TIMER_INTERVAL_MS: u32 = 120;

const BACKGROUND: COLORREF = rgb(0x1c, 0x1c, 0x1e);
const BORDER: COLORREF = rgb(0x38, 0x38, 0x3a);
const HEADER_TEXT: COLORREF = rgb(0xe5, 0xe5, 0xe7);
const LABEL_TEXT: COLORREF = rgb(0xa1, 0xa1, 0xa6);
const FOOTER_TEXT: COLORREF = rgb(0x8e, 0x8e, 0x93);
const TRACK: COLORREF = rgb(0x33, 0x33, 0x35);
const DIVIDER: COLORREF = rgb(0x32, 0x32, 0x34);
type RefreshUsage = fn() -> CodexUsage;

pub fn run(initial_usage: CodexUsage, refresh_usage: RefreshUsage) -> Result<()> {
    // All Win32 calls and raw-pointer access are confined to this module.
    unsafe { run_native(initial_usage, refresh_usage) }
}

struct Fonts {
    header: HFONT,
    label: HFONT,
    mono: HFONT,
    footer: HFONT,
}

impl Fonts {
    unsafe fn create() -> Result<Self> {
        let header = unsafe { create_font(13, FW_MEDIUM.0 as i32, w!("Segoe UI Variable"))? };
        let label = unsafe { create_font(12, FW_NORMAL.0 as i32, w!("Segoe UI Variable"))? };
        let mono = unsafe { create_font(12, FW_MEDIUM.0 as i32, w!("Cascadia Mono"))? };
        let footer = unsafe { create_font(11, FW_NORMAL.0 as i32, w!("Segoe UI Variable"))? };
        Ok(Self {
            header,
            label,
            mono,
            footer,
        })
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.header.into());
            let _ = DeleteObject(self.label.into());
            let _ = DeleteObject(self.mono.into());
            let _ = DeleteObject(self.footer.into());
        }
    }
}

struct AppState {
    hwnd: HWND,
    popup: HWND,
    menu: HMENU,
    tray_added: bool,
    tray_icon: Option<HICON>,
    refresh_usage: RefreshUsage,
    usage: CodexUsage,
    fonts: Fonts,
    tray_hover_bounds: RECT,
}

impl AppState {
    unsafe fn add_tray_icon(&mut self) -> Result<()> {
        let icon = unsafe { create_tray_icon(&self.usage)? };
        let mut data = notify_icon_data(self.hwnd);
        data.uFlags |= NIF_ICON;
        data.hIcon = icon;

        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
            self.tray_added = true;
            self.tray_icon = Some(icon);
            Ok(())
        } else {
            let _ = unsafe { DestroyIcon(icon) };
            Err(Error::from_thread())
        }
    }

    unsafe fn update_tray_icon(&mut self) -> Result<()> {
        let icon = unsafe { create_tray_icon(&self.usage)? };
        let mut data = notify_icon_data(self.hwnd);
        data.uFlags |= NIF_ICON;
        data.hIcon = icon;

        if unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) }.as_bool() {
            if let Some(previous) = self.tray_icon.replace(icon) {
                let _ = unsafe { DestroyIcon(previous) };
            }
            Ok(())
        } else {
            let _ = unsafe { DestroyIcon(icon) };
            Err(Error::from_thread())
        }
    }

    unsafe fn refresh(&mut self) {
        self.usage = (self.refresh_usage)();
        let _ = unsafe { self.update_tray_icon() };
        let _ =
            unsafe { windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.popup), None, false) };
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

    unsafe fn show_popup(&mut self) {
        let mut cursor = POINT::default();
        if unsafe { GetCursorPos(&mut cursor) }.is_err() {
            return;
        }
        self.tray_hover_bounds = RECT {
            left: cursor.x - 20,
            top: cursor.y - 20,
            right: cursor.x + 20,
            bottom: cursor.y + 20,
        };
        if unsafe { IsWindowVisible(self.popup) }.as_bool() {
            return;
        }

        let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
        let mut monitor_info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let has_monitor = unsafe { GetMonitorInfoW(monitor, &mut monitor_info) }.as_bool();
        let mut x = cursor.x - POPUP_WIDTH;
        let mut y = cursor.y - POPUP_HEIGHT - 8;
        if has_monitor {
            x = x.clamp(
                monitor_info.rcWork.left,
                monitor_info.rcWork.right - POPUP_WIDTH,
            );
            y = y.clamp(
                monitor_info.rcWork.top,
                monitor_info.rcWork.bottom - POPUP_HEIGHT,
            );
        }

        let _ = unsafe {
            SetWindowPos(
                self.popup,
                Some(HWND_TOPMOST),
                x,
                y,
                POPUP_WIDTH,
                POPUP_HEIGHT,
                SWP_SHOWWINDOW | SWP_NOACTIVATE,
            )
        };
        unsafe {
            SetTimer(
                Some(self.hwnd),
                HOVER_TIMER_ID,
                HOVER_TIMER_INTERVAL_MS,
                None,
            )
        };
    }

    unsafe fn hide_popup_if_pointer_left(&self) {
        let mut cursor = POINT::default();
        if unsafe { GetCursorPos(&mut cursor) }.is_err() {
            return;
        }

        let over_icon = unsafe { PtInRect(&self.tray_hover_bounds, cursor) }.as_bool();

        let mut popup_bounds = RECT::default();
        let over_popup = unsafe { GetWindowRect(self.popup, &mut popup_bounds) }
            .map(|()| {
                popup_bounds.left -= 8;
                popup_bounds.top -= 8;
                popup_bounds.right += 8;
                popup_bounds.bottom += 8;
                unsafe { PtInRect(&popup_bounds, cursor) }.as_bool()
            })
            .unwrap_or(false);

        if !over_icon && !over_popup {
            let _ = unsafe { ShowWindow(self.popup, SW_HIDE) };
            let _ = unsafe { KillTimer(Some(self.hwnd), HOVER_TIMER_ID) };
        }
    }

    unsafe fn show_menu(&self) {
        let _ = unsafe { ShowWindow(self.popup, SW_HIDE) };
        let _ = unsafe { KillTimer(Some(self.hwnd), HOVER_TIMER_ID) };
        let mut cursor = POINT::default();
        if unsafe { GetCursorPos(&mut cursor) }.is_err() {
            return;
        }

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
            if let Some(icon) = self.tray_icon.take() {
                let _ = DestroyIcon(icon);
            }
            let _ = DestroyMenu(self.menu);
        }
    }
}

struct WindowRegistration {
    instance: HINSTANCE,
    class_name: PCWSTR,
}

impl Drop for WindowRegistration {
    fn drop(&mut self) {
        unsafe {
            let _ = UnregisterClassW(self.class_name, Some(self.instance));
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
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW)? };

    unsafe { register_class(instance, cursor, MESSAGE_CLASS_NAME, window_proc)? };
    let _message_registration = WindowRegistration {
        instance,
        class_name: MESSAGE_CLASS_NAME,
    };
    unsafe { register_class(instance, cursor, POPUP_CLASS_NAME, popup_proc)? };
    let _popup_registration = WindowRegistration {
        instance,
        class_name: POPUP_CLASS_NAME,
    };

    let menu = unsafe { create_menu()? };
    let fonts = unsafe { Fonts::create()? };
    let mut state = Box::new(AppState {
        hwnd: HWND::default(),
        popup: HWND::default(),
        menu,
        tray_added: false,
        tray_icon: None,
        refresh_usage,
        usage: initial_usage,
        fonts,
        tray_hover_bounds: RECT::default(),
    });

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            MESSAGE_CLASS_NAME,
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

    let popup = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            POPUP_CLASS_NAME,
            w!("Codex Usage"),
            WS_POPUP,
            0,
            0,
            POPUP_WIDTH,
            POPUP_HEIGHT,
            Some(hwnd),
            None,
            Some(instance),
            Some((&raw mut *state).cast::<c_void>()),
        )?
    };
    state.popup = popup;
    let _popup_window = WindowHandle(popup);
    unsafe {
        SetLayeredWindowAttributes(popup, COLORREF(0), 255, LWA_ALPHA)?;
        let region = CreateRoundRectRgn(
            0,
            0,
            POPUP_WIDTH + 1,
            POPUP_HEIGHT + 1,
            POPUP_RADIUS * 2,
            POPUP_RADIUS * 2,
        );
        if region.0.is_null() || SetWindowRgn(popup, Some(region), true) == 0 {
            return Err(Error::from_thread());
        }
        state.add_tray_icon()?;
    }

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

unsafe fn create_tray_icon(usage: &CodexUsage) -> Result<HICON> {
    const SIZE: u32 = 32;
    let pixmap = icon::render(usage, SIZE).ok_or_else(Error::from_thread)?;
    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: SIZE as i32,
            biHeight: -(SIZE as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bitmap_bits = std::ptr::null_mut();
    let color_bitmap = unsafe {
        CreateDIBSection(
            None,
            &bitmap_info,
            DIB_RGB_COLORS,
            &mut bitmap_bits,
            None,
            0,
        )?
    };
    let mask_bits = vec![0_u8; (SIZE * SIZE / 8) as usize];
    let mask_bitmap = unsafe {
        CreateBitmap(
            SIZE as i32,
            SIZE as i32,
            1,
            1,
            Some(mask_bits.as_ptr().cast()),
        )
    };
    if mask_bitmap.0.is_null() {
        let _ = unsafe { DeleteObject(color_bitmap.into()) };
        return Err(Error::from_thread());
    }

    let (source_pixels, source_remainder) = pixmap.data().as_chunks::<4>();
    let bitmap_data = unsafe {
        std::slice::from_raw_parts_mut(bitmap_bits.cast::<u8>(), (SIZE * SIZE * 4) as usize)
    };
    let (destination_pixels, destination_remainder) = bitmap_data.as_chunks_mut::<4>();
    debug_assert!(source_remainder.is_empty() && destination_remainder.is_empty());
    for (source, destination) in source_pixels.iter().zip(destination_pixels) {
        destination.copy_from_slice(&[source[2], source[1], source[0], source[3]]);
    }

    let icon_info = ICONINFO {
        fIcon: true.into(),
        hbmMask: mask_bitmap,
        hbmColor: color_bitmap,
        ..Default::default()
    };
    let result = unsafe { CreateIconIndirect(&icon_info) };
    let _ = unsafe { DeleteObject(color_bitmap.into()) };
    let _ = unsafe { DeleteObject(mask_bitmap.into()) };
    result
}

unsafe fn register_class(
    instance: HINSTANCE,
    cursor: windows::Win32::UI::WindowsAndMessaging::HCURSOR,
    class_name: PCWSTR,
    procedure: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
) -> Result<()> {
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(procedure),
        hInstance: instance,
        hCursor: cursor,
        lpszClassName: class_name,
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&class) } == 0 {
        Err(Error::from_thread())
    } else {
        Ok(())
    }
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

fn notify_icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        uFlags: NIF_MESSAGE,
        uCallbackMessage: TRAY_CALLBACK,
        ..Default::default()
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        return unsafe { store_state(hwnd, lparam) };
    }

    let state = unsafe { state_from(hwnd) };
    match message {
        TRAY_CALLBACK if !state.is_null() => {
            match lparam.0 as u32 {
                WM_MOUSEMOVE | WM_LBUTTONUP => unsafe { (*state).show_popup() },
                WM_RBUTTONUP => unsafe { (*state).show_menu() },
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND if !state.is_null() => {
            match wparam.0 & 0xffff {
                MENU_REFRESH => unsafe { (*state).refresh() },
                MENU_QUIT => {
                    let _ = unsafe { DestroyWindow(hwnd) };
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_TIMER if !state.is_null() && wparam.0 == HOVER_TIMER_ID => {
            unsafe { (*state).hide_popup_if_pointer_left() };
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

unsafe extern "system" fn popup_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        return unsafe { store_state(hwnd, lparam) };
    }

    let state = unsafe { state_from(hwnd) };
    match message {
        WM_PAINT if !state.is_null() => {
            unsafe { paint_popup(hwnd, &*state) };
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

unsafe fn store_state(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let create = lparam.0 as *const CREATESTRUCTW;
    if create.is_null() {
        return LRESULT(0);
    }
    let state = unsafe { (*create).lpCreateParams.cast::<AppState>() };
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
    LRESULT(1)
}

unsafe fn state_from(hwnd: HWND) -> *mut AppState {
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState }
}

unsafe fn paint_popup(hwnd: HWND, state: &AppState) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
    if hdc.0.is_null() {
        return;
    }

    unsafe {
        fill(
            hdc,
            RECT {
                left: 0,
                top: 0,
                right: POPUP_WIDTH,
                bottom: POPUP_HEIGHT,
            },
            BACKGROUND,
        );
        outline_round_rect(hdc, 0, 0, POPUP_WIDTH, POPUP_HEIGHT, POPUP_RADIUS, BORDER);
        SetBkMode(hdc, TRANSPARENT);

        draw_codex_icon(hdc, 16, 16);
        draw_text(
            hdc,
            state.fonts.header,
            "Codex",
            RECT {
                left: 38,
                top: 12,
                right: 264,
                bottom: 32,
            },
            HEADER_TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );

        draw_metric(
            hdc,
            &state.fonts,
            "Session",
            state.usage.session_remaining_percent,
            40,
        );
        draw_metric(
            hdc,
            &state.fonts,
            "Weekly",
            state.usage.weekly_remaining_percent,
            74,
        );

        fill(
            hdc,
            RECT {
                left: 16,
                top: 109,
                right: 264,
                bottom: 110,
            },
            DIVIDER,
        );
        let now = SystemTime::now();
        let session_reset = reset_line("Session resets", state.usage.session_reset_at, now);
        let weekly_reset = reset_line("Weekly resets", state.usage.weekly_reset_at, now);
        draw_clock_icon(hdc, 17, 119);
        draw_text(
            hdc,
            state.fonts.footer,
            &session_reset,
            RECT {
                left: 36,
                top: 115,
                right: 264,
                bottom: 133,
            },
            FOOTER_TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        draw_calendar_icon(hdc, 17, 139);
        draw_text(
            hdc,
            state.fonts.footer,
            &weekly_reset,
            RECT {
                left: 36,
                top: 135,
                right: 264,
                bottom: 153,
            },
            FOOTER_TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        let _ = EndPaint(hwnd, &paint);
    }
}

unsafe fn draw_metric(hdc: HDC, fonts: &Fonts, label: &str, value: Option<f32>, top: i32) {
    let percent = value.map(tray::rounded_percent);
    let color = percent.map_or(LABEL_TEXT, |percent| colorref(tray::usage_color(percent)));
    unsafe {
        draw_text(
            hdc,
            fonts.label,
            label,
            RECT {
                left: 16,
                top,
                right: 190,
                bottom: top + 16,
            },
            LABEL_TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        draw_text(
            hdc,
            fonts.mono,
            &percent.map_or_else(|| "--".to_owned(), |value| format!("{value}%")),
            RECT {
                left: 190,
                top,
                right: 264,
                bottom: top + 16,
            },
            color,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        draw_round_rect(hdc, 16, top + 20, 264, top + 25, 3, TRACK);
        if let Some(percent) = percent {
            let width = 248 * i32::from(percent) / 100;
            if width > 0 {
                draw_round_rect(hdc, 16, top + 20, 16 + width, top + 25, 3, color);
            }
        }
    }
}

fn reset_line(label: &str, reset_at: Option<SystemTime>, now: SystemTime) -> String {
    match reset_at {
        Some(reset_at) => format!("{label} in {}", tray::format_reset_duration(reset_at, now)),
        None => format!("{label} unavailable"),
    }
}

unsafe fn create_font(height: i32, weight: i32, face: PCWSTR) -> Result<HFONT> {
    let font = unsafe {
        CreateFontW(
            -height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            u32::from(DEFAULT_PITCH.0 | FF_DONTCARE.0),
            face,
        )
    };
    if font.0.is_null() {
        Err(Error::from_thread())
    } else {
        Ok(font)
    }
}

unsafe fn draw_text(
    hdc: HDC,
    font: HFONT,
    text: &str,
    mut bounds: RECT,
    color: COLORREF,
    format: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT,
) {
    let previous = unsafe { SelectObject(hdc, font.into()) };
    unsafe { SetTextColor(hdc, color) };
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        DrawTextW(hdc, &mut wide, &mut bounds, format);
        SelectObject(hdc, previous);
    }
}

unsafe fn fill(hdc: HDC, bounds: RECT, color: COLORREF) {
    let brush = unsafe { CreateSolidBrush(color) };
    if !brush.0.is_null() {
        unsafe {
            FillRect(hdc, &bounds, brush);
            let _ = DeleteObject(brush.into());
        }
    }
}

unsafe fn draw_round_rect(
    hdc: HDC,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    radius: i32,
    color: COLORREF,
) {
    let brush = unsafe { CreateSolidBrush(color) };
    let pen = unsafe { CreatePen(PS_SOLID, 1, color) };
    if brush.0.is_null() || pen.0.is_null() {
        if !brush.0.is_null() {
            let _ = unsafe { DeleteObject(brush.into()) };
        }
        if !pen.0.is_null() {
            let _ = unsafe { DeleteObject(pen.into()) };
        }
        return;
    }
    let old_brush = unsafe { SelectObject(hdc, brush.into()) };
    let old_pen = unsafe { SelectObject(hdc, pen.into()) };
    unsafe {
        let _ = RoundRect(hdc, left, top, right, bottom, radius * 2, radius * 2);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
    }
}

unsafe fn outline_round_rect(
    hdc: HDC,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    radius: i32,
    color: COLORREF,
) {
    let brush = unsafe { CreateSolidBrush(BACKGROUND) };
    let pen = unsafe { CreatePen(PS_SOLID, 1, color) };
    if brush.0.is_null() || pen.0.is_null() {
        if !brush.0.is_null() {
            let _ = unsafe { DeleteObject(brush.into()) };
        }
        if !pen.0.is_null() {
            let _ = unsafe { DeleteObject(pen.into()) };
        }
        return;
    }
    let old_brush = unsafe { SelectObject(hdc, brush.into()) };
    let old_pen = unsafe { SelectObject(hdc, pen.into()) };
    unsafe {
        let _ = RoundRect(
            hdc,
            left,
            top,
            right - 1,
            bottom - 1,
            radius * 2,
            radius * 2,
        );
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
    }
}

unsafe fn draw_codex_icon(hdc: HDC, x: i32, y: i32) {
    let brush = unsafe { CreateSolidBrush(BACKGROUND) };
    let pen = unsafe { CreatePen(PS_SOLID, 1, HEADER_TEXT) };
    if brush.0.is_null() || pen.0.is_null() {
        if !brush.0.is_null() {
            let _ = unsafe { DeleteObject(brush.into()) };
        }
        if !pen.0.is_null() {
            let _ = unsafe { DeleteObject(pen.into()) };
        }
        return;
    }
    let old_brush = unsafe { SelectObject(hdc, brush.into()) };
    let old_pen = unsafe { SelectObject(hdc, pen.into()) };
    unsafe {
        let _ = Ellipse(hdc, x, y, x + 14, y + 14);
        let _ = MoveToEx(hdc, x + 4, y + 7, None);
        let _ = LineTo(hdc, x + 10, y + 7);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
    }
}

unsafe fn draw_clock_icon(hdc: HDC, x: i32, y: i32) {
    let brush = unsafe { CreateSolidBrush(BACKGROUND) };
    let pen = unsafe { CreatePen(PS_SOLID, 1, FOOTER_TEXT) };
    if brush.0.is_null() || pen.0.is_null() {
        if !brush.0.is_null() {
            let _ = unsafe { DeleteObject(brush.into()) };
        }
        if !pen.0.is_null() {
            let _ = unsafe { DeleteObject(pen.into()) };
        }
        return;
    }
    let old_brush = unsafe { SelectObject(hdc, brush.into()) };
    let old_pen = unsafe { SelectObject(hdc, pen.into()) };
    unsafe {
        let _ = Ellipse(hdc, x, y, x + 12, y + 12);
        let _ = MoveToEx(hdc, x + 6, y + 3, None);
        let _ = LineTo(hdc, x + 6, y + 7);
        let _ = LineTo(hdc, x + 9, y + 8);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
    }
}

unsafe fn draw_calendar_icon(hdc: HDC, x: i32, y: i32) {
    let pen = unsafe { CreatePen(PS_SOLID, 1, FOOTER_TEXT) };
    if pen.0.is_null() {
        return;
    }
    let old_pen = unsafe { SelectObject(hdc, pen.into()) };
    unsafe {
        let _ = MoveToEx(hdc, x, y + 2, None);
        let _ = LineTo(hdc, x + 12, y + 2);
        let _ = LineTo(hdc, x + 12, y + 12);
        let _ = LineTo(hdc, x, y + 12);
        let _ = LineTo(hdc, x, y + 2);
        let _ = MoveToEx(hdc, x, y + 5, None);
        let _ = LineTo(hdc, x + 12, y + 5);
        let _ = MoveToEx(hdc, x + 3, y, None);
        let _ = LineTo(hdc, x + 3, y + 4);
        let _ = MoveToEx(hdc, x + 9, y, None);
        let _ = LineTo(hdc, x + 9, y + 4);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(pen.into());
    }
}

const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    COLORREF(red as u32 | ((green as u32) << 8) | ((blue as u32) << 16))
}

const fn colorref([red, green, blue]: [u8; 3]) -> COLORREF {
    rgb(red, green, blue)
}
