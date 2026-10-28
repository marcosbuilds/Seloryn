#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("Seloryn Setup is currently implemented for Windows x64 only.");
    std::process::exit(2);
}

#[cfg(windows)]
#[allow(dead_code)]
#[path = "distribution.rs"]
mod distribution;

#[cfg(windows)]
#[allow(dead_code)]
#[path = "json.rs"]
mod json;

#[cfg(windows)]
mod windows_setup {
    #![allow(dead_code, unsafe_op_in_unsafe_fn)]

    use super::distribution;

    include!(concat!(env!("OUT_DIR"), "/installer_payload.rs"));

    use std::env;
    use std::ffi::{OsString, c_void};
    use std::path::{Path, PathBuf};
    use std::ptr::{null, null_mut};
    use std::thread;

    use windows_sys::Win32::Foundation::{
        COLORREF, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        AddFontMemResourceEx, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt,
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection, CreateFontW,
        CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER,
        DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, Ellipse, EndPaint,
        FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, FillRect, HBITMAP, HBRUSH, HDC, HFONT, InvalidateRect,
        LineTo, MoveToEx, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, Polygon, RoundRect, SRCCOPY,
        SelectObject, SetBkMode, SetTextColor,
    };
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, RegCloseKey,
        RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows_sys::Win32::UI::Controls::{
        BST_CHECKED, DRAWITEMSTRUCT, EM_REPLACESEL, EM_SCROLLCARET, EM_SETMARGINS, EM_SETSEL,
        InitCommonControls, PBM_SETPOS, PBM_SETRANGE32, PBS_SMOOTH,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows_sys::Win32::UI::Shell::{
        BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW, SHBrowseForFolderW,
        SHGetPathFromIDListW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRectEx, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_OWNERDRAW, CW_USEDEFAULT,
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL,
        ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY, GWLP_USERDATA, GetClientRect, GetCursorPos,
        GetMessageW, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
        HWND_BROADCAST, IsWindowVisible, KillTimer, MB_ICONERROR, MB_OK, MessageBoxW, PostMessageW,
        PostQuitMessage, RegisterClassW, SendMessageW, SetTimer, SetWindowLongPtrW, SetWindowTextW,
        ShowWindow, TranslateMessage, WM_APP, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLORBTN,
        WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DRAWITEM, WM_ERASEBKGND,
        WM_GETMINMAXINFO, WM_PAINT, WM_SETFONT, WM_TIMER, WNDCLASSW, WS_BORDER, WS_CAPTION,
        WS_CHILD, WS_CLIPCHILDREN, WS_EX_CLIENTEDGE, WS_MINIMIZEBOX, WS_SYSMENU, WS_TABSTOP,
        WS_VISIBLE, WS_VSCROLL,
    };

    const RUNTIME_VERSION: &str = "0.12.0";
    const LANGUAGE_VERSION: &str = "0.12";
    const CLIENT_WIDTH: i32 = 880;
    const CLIENT_HEIGHT: i32 = 700;
    const CONTENT_LEFT: i32 = 32;
    const CONTENT_RIGHT: i32 = CLIENT_WIDTH - 32;
    const CONTENT_WIDTH: i32 = CONTENT_RIGHT - CONTENT_LEFT;
    const PACKAGE_IMAGE_WIDTH: i32 = 680;
    const PACKAGE_IMAGE_HEIGHT: i32 = 360;
    const WM_INSTALL_EVENT: u32 = WM_APP + 41;

    const ID_PATH: usize = 100;
    const ID_BROWSE: usize = 101;
    const ID_ADD_PATH: usize = 102;
    const ID_DEFAULT: usize = 103;
    const ID_BACK: usize = 104;
    const ID_ACTION: usize = 105;
    const ID_CANCEL: usize = 106;
    const ID_LOG: usize = 107;
    const ID_PROGRESS: usize = 108;
    const ID_STEP_ONE: usize = 109;
    const ID_STEP_TWO: usize = 110;
    const ID_STEP_THREE: usize = 111;
    const ID_STEP_FOUR: usize = 112;
    const ID_PAGE_TITLE: usize = 113;
    const ID_PAGE_BODY: usize = 114;
    const ID_STATUS: usize = 115;
    const ID_CLEANUP: usize = 117;
    const ID_PATH_LABEL: usize = 119;
    const HOVER_TIMER: usize = 1;

    const BG: COLORREF = rgb(255, 255, 255);
    const SURFACE: COLORREF = rgb(255, 255, 255);
    const SURFACE_RAISED: COLORREF = rgb(245, 247, 246);
    const MINT: COLORREF = rgb(20, 148, 116);
    const MINT_SOFT: COLORREF = rgb(41, 175, 143);
    const TEAL: COLORREF = rgb(20, 112, 91);
    const TEAL_DARK: COLORREF = rgb(205, 214, 209);
    const TEXT_PRIMARY: COLORREF = rgb(18, 22, 20);
    const TEXT_SECONDARY: COLORREF = rgb(78, 86, 81);
    const TEXT_MUTED: COLORREF = rgb(137, 145, 140);
    const CTA: COLORREF = rgb(218, 240, 230);
    const CTA_HOVER: COLORREF = rgb(190, 228, 209);
    const ERROR: COLORREF = rgb(239, 133, 133);

    const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF {
        red | (green << 8) | (blue << 16)
    }

    fn mix_color(start: COLORREF, end: COLORREF, amount: u8) -> COLORREF {
        let amount = u32::from(amount);
        let channel = |from: u32, to: u32| from + (to.saturating_sub(from) * amount / 255);
        rgb(
            channel(start & 0xff, end & 0xff),
            channel((start >> 8) & 0xff, (end >> 8) & 0xff),
            channel((start >> 16) & 0xff, (end >> 16) & 0xff),
        )
    }

    enum InstallEvent {
        Progress(u32, String),
        Complete(Result<String, String>),
    }

    struct Controls {
        path: HWND,
        browse: HWND,
        add_path: HWND,
        make_default: HWND,
        clean_previous: HWND,
        back: HWND,
        action: HWND,
        cancel: HWND,
        log: HWND,
        progress: HWND,
        page_title: HWND,
        page_body: HWND,
        path_label: HWND,
        status: HWND,
        step_one: HWND,
        step_two: HWND,
        step_three: HWND,
        step_four: HWND,
        package_bitmap: HBITMAP,
        background_brush: HBRUSH,
        surface_brush: HBRUSH,
        input_brush: HBRUSH,
        font_resource: HANDLE,
        display_font: HFONT,
        body_font: HFONT,
        body_bold_font: HFONT,
        mono_font: HFONT,
        phase: u8,
        installing: bool,
        progress_value: u32,
        hover_amount: u8,
        hovered_button: HWND,
    }

    pub fn main() {
        let arguments: Vec<OsString> = env::args_os().skip(1).collect();
        if arguments.first().is_some_and(|value| value == "--headless") {
            let add_path = arguments.iter().any(|value| value == "--add-path");
            let make_default = arguments.iter().any(|value| value == "--make-default");
            let root = arguments
                .windows(2)
                .find(|pair| pair[0] == "--install-root")
                .map(|pair| PathBuf::from(&pair[1]))
                .unwrap_or_else(default_root);
            match install(&root, add_path, make_default) {
                Ok(message) => println!("{message}"),
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
            }
            return;
        }
        if let Err(error) = run_window() {
            let title = wide("Seloryn Setup");
            let message = wide(&error);
            unsafe {
                MessageBoxW(
                    null_mut(),
                    message.as_ptr(),
                    title.as_ptr(),
                    MB_OK | MB_ICONERROR,
                );
            }
            std::process::exit(1);
        }
    }

    fn default_root() -> PathBuf {
        distribution::root_dir().unwrap_or_else(|_| PathBuf::from("Seloryn"))
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn window_style() -> u32 {
        WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN
    }

    unsafe fn window_outer_size(style: u32) -> (i32, i32) {
        let mut frame = RECT {
            left: 0,
            top: 0,
            right: CLIENT_WIDTH,
            bottom: CLIENT_HEIGHT,
        };
        AdjustWindowRectEx(&mut frame, style, 0, 0);
        (frame.right - frame.left, frame.bottom - frame.top)
    }

    fn run_window() -> Result<(), String> {
        unsafe {
            InitCommonControls();
            let instance = GetModuleHandleW(null());
            if instance.is_null() {
                return Err("could not obtain the setup module handle".to_owned());
            }
            let class_name = wide("SelorynSetupWindow");
            let window_name = wide("Installer");
            let class = WNDCLASSW {
                style: 3,
                lpfnWndProc: Some(window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: null_mut(),
                hCursor: null_mut(),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class_name.as_ptr(),
            };
            if RegisterClassW(&class) == 0 {
                return Err("could not register the setup window".to_owned());
            }
            let style = window_style();
            let (window_width, window_height) = window_outer_size(style);
            let window = CreateWindowExW(
                0,
                class_name.as_ptr(),
                window_name.as_ptr(),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                window_width,
                window_height,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            if window.is_null() {
                return Err("could not create the setup window".to_owned());
            }
            ShowWindow(window, 1);
            let mut message = std::mem::zeroed();
            while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_CREATE => {
                create_controls(hwnd);
                0
            }
            WM_GETMINMAXINFO => {
                let info =
                    &mut *(lparam as *mut windows_sys::Win32::UI::WindowsAndMessaging::MINMAXINFO);
                let (window_width, window_height) = window_outer_size(window_style());
                info.ptMinTrackSize.x = window_width;
                info.ptMinTrackSize.y = window_height;
                info.ptMaxTrackSize.x = window_width;
                info.ptMaxTrackSize.y = window_height;
                0
            }
            WM_COMMAND => handle_command(hwnd, wparam),
            WM_INSTALL_EVENT => handle_install_event(hwnd, lparam),
            WM_DRAWITEM => handle_draw_item(hwnd, lparam),
            WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => {
                handle_control_color(hwnd, wparam, lparam)
            }
            WM_TIMER => {
                update_button_hover(hwnd);
                0
            }
            WM_ERASEBKGND => 1,
            WM_PAINT => paint(hwnd),
            WM_CLOSE => {
                let pointer = controls(hwnd);
                if !pointer.is_null() && (*pointer).installing {
                    append_log(
                        (*pointer).log,
                        "Installation is still running; waiting for a safe completion point.",
                    );
                    return 0;
                }
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                let pointer = controls(hwnd);
                if !pointer.is_null() {
                    KillTimer(hwnd, HOVER_TIMER);
                    destroy_controls(Box::from_raw(pointer));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    unsafe fn create_controls(hwnd: HWND) {
        let instance = GetModuleHandleW(null());
        let background_brush = CreateSolidBrush(BG);
        let surface_brush = CreateSolidBrush(SURFACE);
        let input_brush = CreateSolidBrush(SURFACE_RAISED);
        let (font_resource, display_face) = load_private_font();
        let display_font = create_font(-32, FW_NORMAL, display_face);
        let body_font = create_font(-15, FW_NORMAL, "Segoe UI");
        let body_bold_font = create_font(-15, FW_SEMIBOLD, "Segoe UI");
        let mono_font = create_font(-14, FW_NORMAL, "Cascadia Mono");

        let step_one = label(hwnd, "WELCOME", 50, 104, 120, 24, ID_STEP_ONE);
        let step_two = label(hwnd, "LOCATION", 270, 104, 120, 24, ID_STEP_TWO);
        let step_three = label(hwnd, "INSTALL", 490, 104, 120, 24, ID_STEP_THREE);
        let step_four = label(hwnd, "DONE", 710, 104, 120, 24, ID_STEP_FOUR);
        let page_title = label(
            hwnd,
            "WELCOME TO SELORYN",
            CONTENT_LEFT,
            154,
            CONTENT_WIDTH,
            42,
            ID_PAGE_TITLE,
        );
        let page_body = label(
            hwnd,
            "A native per-user setup for the verified 0.12.0 runtime.",
            CONTENT_LEFT + 2,
            204,
            CONTENT_WIDTH - 4,
            30,
            ID_PAGE_BODY,
        );
        let path_label = label(
            hwnd,
            "INSTALLATION DIRECTORY",
            CONTENT_LEFT + 2,
            250,
            CONTENT_WIDTH - 4,
            22,
            ID_PATH_LABEL,
        );
        let path = edit(
            hwnd,
            &default_root().to_string_lossy(),
            CONTENT_LEFT + 2,
            284,
            620,
            40,
            ID_PATH,
        );
        let browse = button(hwnd, "Browse", 666, 284, 182, 40, ID_BROWSE);
        let add_path = checkbox(
            hwnd,
            "Add Seloryn to the current user's PATH",
            CONTENT_LEFT + 2,
            344,
            ID_ADD_PATH,
        );
        let make_default = checkbox(
            hwnd,
            "Make 0.12.0 the default version",
            CONTENT_LEFT + 2,
            378,
            ID_DEFAULT,
        );
        let clean_previous = checkbox(
            hwnd,
            "Remove previous Seloryn installations",
            CONTENT_LEFT + 2,
            412,
            ID_CLEANUP,
        );
        SendMessageW(add_path, BM_SETCHECK, BST_CHECKED as _, 0);
        SendMessageW(make_default, BM_SETCHECK, BST_CHECKED as _, 0);
        let log = log_edit(hwnd, CONTENT_LEFT + 2, 266, CONTENT_WIDTH - 4, 250, ID_LOG);
        let progress = CreateWindowExW(
            0,
            wide("msctls_progress32").as_ptr(),
            null(),
            WS_CHILD | PBS_SMOOTH,
            CONTENT_LEFT,
            552,
            CONTENT_WIDTH,
            10,
            hwnd,
            ID_PROGRESS as _,
            instance,
            null(),
        );
        SendMessageW(progress, PBM_SETRANGE32, 0, 100);
        let status = label(
            hwnd,
            "Ready to begin.",
            CONTENT_LEFT,
            616,
            CONTENT_WIDTH,
            24,
            ID_STATUS,
        );
        let back = button(hwnd, "Back", 540, 646, 112, 40, ID_BACK);
        let action = button(hwnd, "Continue", 668, 646, 180, 40, ID_ACTION);
        let cancel = button(hwnd, "Cancel", CONTENT_LEFT, 646, 112, 40, ID_CANCEL);

        for (control, font) in [
            (step_one, body_bold_font),
            (step_two, body_bold_font),
            (step_three, body_bold_font),
            (step_four, body_bold_font),
            (page_title, display_font),
            (page_body, body_font),
            (path_label, body_font),
            (path, body_font),
            (browse, body_font),
            (add_path, body_font),
            (make_default, body_font),
            (clean_previous, body_font),
            (log, mono_font),
            (status, body_font),
            (back, body_font),
            (action, body_font),
            (cancel, body_font),
        ] {
            SendMessageW(control, WM_SETFONT, font as _, 1);
        }
        let controls = Box::new(Controls {
            path,
            browse,
            add_path,
            make_default,
            clean_previous,
            back,
            action,
            cancel,
            log,
            progress,
            page_title,
            page_body,
            path_label,
            status,
            step_one,
            step_two,
            step_three,
            step_four,
            package_bitmap: load_package_bitmap(),
            background_brush,
            surface_brush,
            input_brush,
            font_resource,
            display_font,
            body_font,
            body_bold_font,
            mono_font,
            phase: 1,
            installing: false,
            progress_value: 0,
            hover_amount: 0,
            hovered_button: null_mut(),
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(controls) as isize);
        SetTimer(hwnd, HOVER_TIMER, 50, None);
        show_phase(hwnd, 1);
    }

    unsafe fn create_font(height: i32, weight: u32, face: &str) -> HFONT {
        let name = wide(face);
        CreateFontW(
            height,
            0,
            0,
            0,
            weight as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            name.as_ptr(),
        )
    }

    unsafe fn load_private_font() -> (HANDLE, &'static str) {
        if DISPLAY_FONT_PAYLOAD.is_empty() {
            return (null_mut(), "Segoe UI Semibold");
        }
        let font_count = 0_u32;
        let token = AddFontMemResourceEx(
            DISPLAY_FONT_PAYLOAD.as_ptr().cast::<c_void>(),
            DISPLAY_FONT_PAYLOAD.len() as u32,
            null(),
            &font_count,
        );
        if token.is_null() {
            (null_mut(), "Segoe UI Semibold")
        } else {
            (token, "Seloryn Signature")
        }
    }

    unsafe fn load_package_bitmap() -> HBITMAP {
        let data = PACKAGE_IMAGE_PAYLOAD;
        if data.len() < 54 || data[0] != b'B' || data[1] != b'M' {
            return null_mut();
        }
        let read_u16 = |offset: usize| u16::from_le_bytes([data[offset], data[offset + 1]]);
        let read_u32 = |offset: usize| {
            u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ])
        };
        let pixel_offset = read_u32(10) as usize;
        let width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
        let source_height = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
        let bit_count = read_u16(28);
        if width <= 0 || source_height == 0 || (bit_count != 24 && bit_count != 32) {
            return null_mut();
        }
        let height = source_height.unsigned_abs() as usize;
        let width = width as usize;
        let source_stride = (width * usize::from(bit_count)).div_ceil(32) * 4;
        if pixel_offset
            .checked_add(source_stride.saturating_mul(height))
            .is_none_or(|end| end > data.len())
        {
            return null_mut();
        }

        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits: *mut c_void = null_mut();
        let bitmap = CreateDIBSection(null_mut(), &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bitmap.is_null() || bits.is_null() {
            return bitmap;
        }

        let source_bytes = usize::from(bit_count / 8);
        let destination_stride = width * 4;
        for row in 0..height {
            let source_row = if source_height > 0 {
                height - row - 1
            } else {
                row
            };
            let source = data.as_ptr().add(pixel_offset + source_row * source_stride);
            let destination = (bits as *mut u8).add(row * destination_stride);
            for column in 0..width {
                let source_pixel = source.add(column * source_bytes);
                let destination_pixel = destination.add(column * 4);
                *destination_pixel = *source_pixel;
                *destination_pixel.add(1) = *source_pixel.add(1);
                *destination_pixel.add(2) = *source_pixel.add(2);
                *destination_pixel.add(3) = 0xff;
            }
        }
        bitmap
    }

    unsafe fn label(
        parent: HWND,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        id: usize,
    ) -> HWND {
        CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE,
            x,
            y,
            width,
            height,
            parent,
            id as _,
            GetModuleHandleW(null()),
            null(),
        )
    }

    unsafe fn edit(
        parent: HWND,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        id: usize,
    ) -> HWND {
        CreateWindowExW(
            WS_EX_CLIENTEDGE,
            wide("EDIT").as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32 | WS_BORDER,
            x,
            y,
            width,
            height,
            parent,
            id as _,
            GetModuleHandleW(null()),
            null(),
        )
    }

    unsafe fn log_edit(parent: HWND, x: i32, y: i32, width: i32, height: i32, id: usize) -> HWND {
        let control = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            wide("EDIT").as_ptr(),
            null(),
            WS_CHILD
                | ES_MULTILINE as u32
                | ES_AUTOVSCROLL as u32
                | ES_READONLY as u32
                | WS_VSCROLL,
            x,
            y,
            width,
            height,
            parent,
            id as _,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(control, EM_SETMARGINS, 0, ((12_u32 << 16) | 12) as LPARAM);
        control
    }

    unsafe fn checkbox(parent: HWND, text: &str, x: i32, y: i32, id: usize) -> HWND {
        CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX as u32,
            x,
            y,
            430,
            28,
            parent,
            id as _,
            GetModuleHandleW(null()),
            null(),
        )
    }

    unsafe fn button(
        parent: HWND,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        id: usize,
    ) -> HWND {
        CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            x,
            y,
            width,
            height,
            parent,
            id as _,
            GetModuleHandleW(null()),
            null(),
        )
    }

    unsafe fn handle_command(hwnd: HWND, wparam: WPARAM) -> LRESULT {
        let id = wparam & 0xffff;
        let pointer = controls(hwnd);
        if pointer.is_null() {
            return 0;
        }
        if id == ID_CANCEL {
            if !(*pointer).installing {
                DestroyWindow(hwnd);
            }
            return 0;
        }
        if id == ID_BROWSE && (*pointer).phase == 2 {
            if let Some(path) = browse_for_folder(hwnd) {
                SetWindowTextW((*pointer).path, wide(&path).as_ptr());
            }
            return 0;
        }
        if id == ID_BACK && (*pointer).phase == 2 && !(*pointer).installing {
            (*pointer).phase = 1;
            show_phase(hwnd, 1);
            navigation_sound();
            return 0;
        }
        if id == ID_ACTION {
            match (*pointer).phase {
                1 => {
                    (*pointer).phase = 2;
                    show_phase(hwnd, 2);
                    navigation_sound();
                }
                2 => start_install(hwnd),
                3 => start_install(hwnd),
                4 => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            return 0;
        }
        0
    }

    unsafe fn navigation_sound() {
        // Use the user's normal Windows notification sound so no extra audio asset is needed.
        MessageBeep(0x0000_0040);
    }

    unsafe fn start_install(hwnd: HWND) {
        let pointer = controls(hwnd);
        if pointer.is_null() || (*pointer).installing {
            return;
        }
        let root = PathBuf::from(window_text((*pointer).path));
        if root.as_os_str().is_empty() || !root.is_absolute() {
            set_status(
                (*pointer).status,
                "Choose an absolute installation directory.",
            );
            return;
        }
        (*pointer).phase = 3;
        (*pointer).installing = true;
        show_phase(hwnd, 3);
        let add_path = is_checked((*pointer).add_path);
        let make_default = is_checked((*pointer).make_default);
        let clean_previous = is_checked((*pointer).clean_previous);
        let hwnd_raw = hwnd as isize;
        thread::spawn(move || {
            let result = install_with_progress(
                &root,
                add_path,
                make_default,
                clean_previous,
                |progress, message| {
                    post_install_event(
                        hwnd_raw,
                        InstallEvent::Progress(progress, message.to_owned()),
                    );
                },
            );
            post_install_event(hwnd_raw, InstallEvent::Complete(result));
        });
    }

    unsafe fn handle_install_event(hwnd: HWND, lparam: LPARAM) -> LRESULT {
        let event = Box::from_raw(lparam as *mut InstallEvent);
        let pointer = controls(hwnd);
        if pointer.is_null() {
            return 0;
        }
        match *event {
            InstallEvent::Progress(progress, message) => {
                (*pointer).progress_value = progress;
                SendMessageW((*pointer).progress, PBM_SETPOS, progress as _, 0);
                append_log((*pointer).log, &message);
                set_status((*pointer).status, &message);
                let progress_rect = RECT {
                    left: CONTENT_LEFT,
                    top: 548,
                    right: CONTENT_RIGHT,
                    bottom: 568,
                };
                InvalidateRect(hwnd, &progress_rect, 0);
            }
            InstallEvent::Complete(result) => {
                (*pointer).installing = false;
                EnableWindow((*pointer).back, 1);
                EnableWindow((*pointer).cancel, 1);
                match result {
                    Ok(message) => {
                        (*pointer).progress_value = 100;
                        SendMessageW((*pointer).progress, PBM_SETPOS, 100, 0);
                        append_log((*pointer).log, &message);
                        let root = PathBuf::from(window_text((*pointer).path));
                        match launch_terminal(&root) {
                            Ok(()) => {
                                DestroyWindow(hwnd);
                            }
                            Err(error) => {
                                append_log(
                                    (*pointer).log,
                                    &format!("Terminal handoff failed: {error}"),
                                );
                                set_status((*pointer).status, &error);
                                (*pointer).phase = 4;
                                show_phase(hwnd, 4);
                            }
                        }
                    }
                    Err(error) => {
                        append_log((*pointer).log, &format!("ERROR  {error}"));
                        set_status((*pointer).status, &error);
                        SetWindowTextW((*pointer).action, wide("Retry").as_ptr());
                        EnableWindow((*pointer).action, 1);
                    }
                }
            }
        }
        0
    }

    unsafe fn post_install_event(hwnd_raw: isize, event: InstallEvent) {
        let pointer = Box::into_raw(Box::new(event));
        if PostMessageW(hwnd_raw as HWND, WM_INSTALL_EVENT, 0, pointer as LPARAM) == 0 {
            drop(Box::from_raw(pointer));
        }
    }

    unsafe fn show_phase(hwnd: HWND, phase: u8) {
        let pointer = controls(hwnd);
        if pointer.is_null() {
            return;
        }
        let choose = phase == 2;
        let installing = phase == 3 || phase == 4;
        for (control, visible) in [
            ((*pointer).path, choose),
            ((*pointer).browse, choose),
            ((*pointer).path_label, choose),
            ((*pointer).add_path, choose),
            ((*pointer).make_default, choose),
            ((*pointer).clean_previous, choose),
            ((*pointer).log, installing),
            ((*pointer).progress, false),
        ] {
            ShowWindow(control, if visible { 1 } else { 0 });
        }
        ShowWindow(
            (*pointer).back,
            if phase == 2 && !(*pointer).installing {
                1
            } else {
                0
            },
        );
        ShowWindow((*pointer).cancel, if phase < 4 { 1 } else { 0 });
        SetWindowTextW(
            (*pointer).page_title,
            wide(match phase {
                1 => "WELCOME TO SELORYN",
                2 => "INSTALLATION LOCATION",
                3 => "INSTALLING SELORYN",
                4 => "SELORYN IS READY",
                _ => "Seloryn Setup",
            })
            .as_ptr(),
        );
        SetWindowTextW(
            (*pointer).page_body,
            wide(match phase {
                1 => "A quiet, per-user setup for the verified Seloryn 0.12.0 runtime.",
                2 => "Choose where Seloryn lives and how this version should behave.",
                3 => "Every line below maps to a real installation operation.",
                4 => "The runtime and launcher were verified after installation.",
                _ => "",
            })
            .as_ptr(),
        );
        SetWindowTextW(
            (*pointer).action,
            wide(match phase {
                1 => "Continue",
                2 => "Install",
                3 => "Installing...",
                4 => "Close",
                _ => "Continue",
            })
            .as_ptr(),
        );
        if phase == 3 {
            (*pointer).progress_value = 0;
        }
        EnableWindow((*pointer).action, if phase == 3 { 0 } else { 1 });
        InvalidateRect(hwnd, null(), 1);
    }

    unsafe fn handle_control_color(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let pointer = controls(hwnd);
        if pointer.is_null() {
            return DefWindowProcW(hwnd, WM_CTLCOLORSTATIC, wparam, lparam);
        }
        let hdc = wparam as HDC;
        SetBkMode(hdc, 1);
        let control = lparam as HWND;
        let step = if control == (*pointer).step_one {
            Some(1)
        } else if control == (*pointer).step_two {
            Some(2)
        } else if control == (*pointer).step_three {
            Some(3)
        } else if control == (*pointer).step_four {
            Some(4)
        } else {
            None
        };
        let color = if control == (*pointer).page_title {
            TEXT_PRIMARY
        } else if control == (*pointer).path_label {
            MINT_SOFT
        } else if control == (*pointer).page_body || control == (*pointer).status {
            TEXT_SECONDARY
        } else if step.is_some_and(|value| value == (*pointer).phase) {
            MINT
        } else if step.is_some_and(|value| value < (*pointer).phase) {
            MINT_SOFT
        } else {
            TEXT_PRIMARY
        };
        SetTextColor(hdc, color);
        if control == (*pointer).path || control == (*pointer).log {
            (*pointer).input_brush as LRESULT
        } else {
            (*pointer).background_brush as LRESULT
        }
    }

    unsafe fn update_button_hover(hwnd: HWND) {
        let pointer = controls(hwnd);
        if pointer.is_null() {
            return;
        }
        let mut point = POINT::default();
        if GetCursorPos(&mut point) == 0 {
            return;
        }
        let mut hovered = null_mut();
        for button in [
            (*pointer).browse,
            (*pointer).back,
            (*pointer).action,
            (*pointer).cancel,
        ] {
            if button.is_null() || IsWindowVisible(button) == 0 {
                continue;
            }
            let mut rect = RECT::default();
            if GetWindowRect(button, &mut rect) != 0
                && point.x >= rect.left
                && point.x < rect.right
                && point.y >= rect.top
                && point.y < rect.bottom
            {
                hovered = button;
                break;
            }
        }
        if hovered != (*pointer).hovered_button {
            let old = (*pointer).hovered_button;
            (*pointer).hovered_button = hovered;
            if !old.is_null() {
                InvalidateRect(old, null(), 0);
            }
            if !hovered.is_null() {
                InvalidateRect(hovered, null(), 0);
            }
        }
        let target = if hovered.is_null() { 0 } else { 255 };
        let current = (*pointer).hover_amount;
        let next = if current < target {
            current.saturating_add(32).min(target)
        } else {
            current.saturating_sub(32).max(target)
        };
        if next != current {
            (*pointer).hover_amount = next;
            if !(*pointer).hovered_button.is_null() {
                InvalidateRect((*pointer).hovered_button, null(), 0);
            }
        }
    }

    unsafe fn handle_draw_item(hwnd: HWND, lparam: LPARAM) -> LRESULT {
        let item = &*(lparam as *const DRAWITEMSTRUCT);
        let pointer = controls(hwnd);
        let hovered = !pointer.is_null() && item.hwndItem == (*pointer).hovered_button;
        let selected = item.itemState & 1 != 0;
        let disabled = item.itemState & 0x0004 != 0;
        let focused = item.itemState & 0x0010 != 0;
        let primary = item.CtlID == ID_ACTION as u32;
        let hover_amount = if hovered && !pointer.is_null() {
            (*pointer).hover_amount
        } else {
            0
        };
        let fill = if disabled {
            SURFACE
        } else if primary {
            if selected {
                CTA_HOVER
            } else {
                mix_color(CTA, CTA_HOVER, hover_amount)
            }
        } else {
            SURFACE_RAISED
        };
        let fill_brush = CreateSolidBrush(fill);
        let old_brush = SelectObject(item.hDC, fill_brush as _);
        let border = CreatePen(
            PS_SOLID,
            if focused { 2 } else { 1 },
            if disabled {
                TEAL_DARK
            } else if selected || hovered || focused {
                TEXT_PRIMARY
            } else {
                TEAL_DARK
            },
        );
        let old = SelectObject(item.hDC, border as _);
        RoundRect(
            item.hDC,
            item.rcItem.left,
            item.rcItem.top,
            item.rcItem.right,
            item.rcItem.bottom,
            14,
            14,
        );
        SelectObject(item.hDC, old);
        DeleteObject(border as _);
        SelectObject(item.hDC, old_brush);
        DeleteObject(fill_brush as _);
        SetBkMode(item.hDC, 1);
        SetTextColor(
            item.hDC,
            if disabled {
                TEXT_SECONDARY
            } else {
                TEXT_PRIMARY
            },
        );
        if item.CtlID as usize == ID_BROWSE {
            draw_folder_icon(item.hDC, item.rcItem.left + 10, item.rcItem.top + 9);
        }
        let text = wide(&window_text(item.hwndItem));
        let mut rect = item.rcItem;
        if item.CtlID as usize == ID_BROWSE {
            rect.left += 24;
        }
        if primary && !disabled && !pointer.is_null() && (*pointer).phase < 4 {
            let circle_brush = CreateSolidBrush(TEXT_PRIMARY);
            let arrow_pen = CreatePen(PS_SOLID, 2, BG);
            let old_circle_brush = SelectObject(item.hDC, circle_brush as _);
            let old_arrow_pen = SelectObject(item.hDC, arrow_pen as _);
            let center_x = item.rcItem.right - 20;
            let center_y = (item.rcItem.top + item.rcItem.bottom) / 2;
            Ellipse(
                item.hDC,
                center_x - 10,
                center_y - 10,
                center_x + 10,
                center_y + 10,
            );
            MoveToEx(item.hDC, center_x - 5, center_y, null_mut());
            LineTo(item.hDC, center_x + 5, center_y);
            MoveToEx(item.hDC, center_x + 1, center_y - 4, null_mut());
            LineTo(item.hDC, center_x + 5, center_y);
            LineTo(item.hDC, center_x + 1, center_y + 4);
            SelectObject(item.hDC, old_arrow_pen);
            SelectObject(item.hDC, old_circle_brush);
            DeleteObject(arrow_pen as _);
            DeleteObject(circle_brush as _);
            rect.right -= 32;
        }
        DrawTextW(
            item.hDC,
            text.as_ptr(),
            -1,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        1
    }

    unsafe fn paint(hwnd: HWND) -> LRESULT {
        let mut paint = std::mem::zeroed::<PAINTSTRUCT>();
        let hdc = BeginPaint(hwnd, &mut paint);
        let mut client = RECT::default();
        GetClientRect(hwnd, &mut client);
        let brush = CreateSolidBrush(BG);
        FillRect(hdc, &client, brush);
        DeleteObject(brush as _);
        let pointer = controls(hwnd);
        if !pointer.is_null() {
            draw_stepper(hdc, pointer);
            match (*pointer).phase {
                1 => draw_package_image(hdc, pointer),
                2 => draw_surface_card(hdc, 24, 240, CONTENT_RIGHT + 8, 456),
                3 | 4 => draw_surface_card(hdc, 24, 240, CONTENT_RIGHT + 8, 532),
                _ => {}
            }
            if (*pointer).phase == 3 || (*pointer).phase == 4 {
                draw_progress(
                    hdc,
                    CONTENT_LEFT,
                    552,
                    CONTENT_WIDTH,
                    8,
                    (*pointer).progress_value,
                );
            }
        }
        EndPaint(hwnd, &paint);
        0
    }

    unsafe fn draw_surface_card(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32) {
        let brush = CreateSolidBrush(SURFACE);
        let pen = CreatePen(PS_SOLID, 1, TEAL_DARK);
        let old_brush = SelectObject(hdc, brush as _);
        let old_pen = SelectObject(hdc, pen as _);
        RoundRect(hdc, left, top, right, bottom, 12, 12);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        DeleteObject(pen as _);
        DeleteObject(brush as _);
    }

    unsafe fn draw_stepper(hdc: HDC, pointer: *mut Controls) {
        let centers = [110, 330, 550, 770];
        let y = 64;
        let base_pen = CreatePen(PS_SOLID, 1, TEAL_DARK);
        let old_pen = SelectObject(hdc, base_pen as _);
        MoveToEx(hdc, centers[0], y, null_mut());
        LineTo(hdc, centers[3], y);
        SelectObject(hdc, old_pen);
        DeleteObject(base_pen as _);

        let active = (*pointer).phase.min(4) as usize;
        if active > 1 {
            let progress_pen = CreatePen(PS_SOLID, 2, MINT);
            let old_progress = SelectObject(hdc, progress_pen as _);
            MoveToEx(hdc, centers[0], y, null_mut());
            LineTo(hdc, centers[active - 1], y);
            SelectObject(hdc, old_progress);
            DeleteObject(progress_pen as _);
        }

        let old_font = SelectObject(hdc, (*pointer).body_bold_font as _);
        SetBkMode(hdc, 1);
        for (index, center) in centers.iter().enumerate() {
            let current = index + 1 == active;
            let completed = index + 1 < active;
            let color = if current {
                MINT
            } else if completed {
                MINT_SOFT
            } else {
                TEXT_MUTED
            };
            let pen = CreatePen(PS_SOLID, if current { 2 } else { 1 }, color);
            let brush = CreateSolidBrush(BG);
            let old_circle_pen = SelectObject(hdc, pen as _);
            let old_circle_brush = SelectObject(hdc, brush as _);
            Ellipse(hdc, center - 18, y - 18, center + 18, y + 18);
            SelectObject(hdc, old_circle_brush);
            SelectObject(hdc, old_circle_pen);
            DeleteObject(brush as _);
            DeleteObject(pen as _);

            SetTextColor(hdc, color);
            let mut number = RECT {
                left: center - 17,
                top: y - 13,
                right: center + 17,
                bottom: y + 13,
            };
            DrawTextW(
                hdc,
                wide(&format!("{:02}", index + 1)).as_ptr(),
                -1,
                &mut number,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
        SelectObject(hdc, old_font);
    }

    unsafe fn draw_package_image(hdc: HDC, pointer: *mut Controls) {
        if (*pointer).package_bitmap.is_null() {
            draw_surface_card(
                hdc,
                96,
                252,
                96 + PACKAGE_IMAGE_WIDTH,
                252 + PACKAGE_IMAGE_HEIGHT,
            );
            return;
        }
        let memory = CreateCompatibleDC(hdc);
        if memory.is_null() {
            return;
        }
        let previous = SelectObject(memory, (*pointer).package_bitmap as _);
        BitBlt(
            hdc,
            (CLIENT_WIDTH - PACKAGE_IMAGE_WIDTH) / 2,
            244,
            PACKAGE_IMAGE_WIDTH,
            PACKAGE_IMAGE_HEIGHT,
            memory,
            0,
            0,
            SRCCOPY,
        );
        SelectObject(memory, previous);
        DeleteDC(memory);
    }

    unsafe fn draw_package_box_icon(hdc: HDC, x: i32, y: i32, color: COLORREF) {
        let pen = CreatePen(PS_SOLID, 1, color);
        let old = SelectObject(hdc, pen as _);
        MoveToEx(hdc, x, y + 6, null_mut());
        LineTo(hdc, x + 12, y);
        LineTo(hdc, x + 24, y + 6);
        LineTo(hdc, x + 12, y + 12);
        LineTo(hdc, x, y + 6);
        MoveToEx(hdc, x + 12, y + 12, null_mut());
        LineTo(hdc, x + 12, y + 24);
        MoveToEx(hdc, x, y + 6, null_mut());
        LineTo(hdc, x, y + 18);
        LineTo(hdc, x + 12, y + 24);
        LineTo(hdc, x + 24, y + 18);
        LineTo(hdc, x + 24, y + 6);
        SelectObject(hdc, old);
        DeleteObject(pen as _);
    }

    unsafe fn draw_package_gear_icon(hdc: HDC, x: i32, y: i32, color: COLORREF) {
        let pen = CreatePen(PS_SOLID, 1, color);
        let brush = CreateSolidBrush(SURFACE);
        let old_pen = SelectObject(hdc, pen as _);
        let old_brush = SelectObject(hdc, brush as _);
        Ellipse(hdc, x + 4, y + 4, x + 20, y + 20);
        Ellipse(hdc, x + 9, y + 9, x + 15, y + 15);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(brush as _);
        DeleteObject(pen as _);
        let spoke = CreatePen(PS_SOLID, 2, color);
        let old_spoke = SelectObject(hdc, spoke as _);
        MoveToEx(hdc, x + 12, y, null_mut());
        LineTo(hdc, x + 12, y + 4);
        MoveToEx(hdc, x + 12, y + 20, null_mut());
        LineTo(hdc, x + 12, y + 24);
        MoveToEx(hdc, x, y + 12, null_mut());
        LineTo(hdc, x + 4, y + 12);
        MoveToEx(hdc, x + 20, y + 12, null_mut());
        LineTo(hdc, x + 24, y + 12);
        SelectObject(hdc, old_spoke);
        DeleteObject(spoke as _);
    }

    unsafe fn draw_package_code_icon(hdc: HDC, x: i32, y: i32, color: COLORREF) {
        let pen = CreatePen(PS_SOLID, 2, color);
        let old = SelectObject(hdc, pen as _);
        MoveToEx(hdc, x + 8, y + 2, null_mut());
        LineTo(hdc, x + 2, y + 12);
        LineTo(hdc, x + 8, y + 22);
        MoveToEx(hdc, x + 16, y + 2, null_mut());
        LineTo(hdc, x + 22, y + 12);
        LineTo(hdc, x + 16, y + 22);
        MoveToEx(hdc, x + 12, y + 2, null_mut());
        LineTo(hdc, x + 10, y + 22);
        SelectObject(hdc, old);
        DeleteObject(pen as _);
    }

    unsafe fn draw_package_windows_icon(hdc: HDC, x: i32, y: i32, color: COLORREF) {
        let brush = CreateSolidBrush(color);
        let old = SelectObject(hdc, brush as _);
        for (left, top, right, bottom) in [
            (x, y, x + 10, y + 10),
            (x + 13, y, x + 23, y + 10),
            (x, y + 13, x + 10, y + 23),
            (x + 13, y + 13, x + 23, y + 23),
        ] {
            FillRect(
                hdc,
                &RECT {
                    left,
                    top,
                    right,
                    bottom,
                },
                brush,
            );
        }
        SelectObject(hdc, old);
        DeleteObject(brush as _);
    }

    unsafe fn draw_package_user_icon(hdc: HDC, x: i32, y: i32, color: COLORREF) {
        let pen = CreatePen(PS_SOLID, 1, color);
        let brush = CreateSolidBrush(SURFACE);
        let old_pen = SelectObject(hdc, pen as _);
        let old_brush = SelectObject(hdc, brush as _);
        Ellipse(hdc, x + 8, y, x + 16, y + 8);
        RoundRect(hdc, x + 3, y + 12, x + 21, y + 24, 6, 6);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(brush as _);
        DeleteObject(pen as _);
    }

    unsafe fn draw_package_shield_icon(hdc: HDC, x: i32, y: i32, color: COLORREF) {
        let pen = CreatePen(PS_SOLID, 1, color);
        let old = SelectObject(hdc, pen as _);
        let points = [
            POINT { x: x + 12, y },
            POINT {
                x: x + 23,
                y: y + 4,
            },
            POINT {
                x: x + 20,
                y: y + 16,
            },
            POINT {
                x: x + 12,
                y: y + 24,
            },
            POINT {
                x: x + 4,
                y: y + 16,
            },
            POINT { x: x + 1, y: y + 4 },
        ];
        Polygon(hdc, points.as_ptr(), points.len() as i32);
        MoveToEx(hdc, x + 7, y + 12, null_mut());
        LineTo(hdc, x + 11, y + 16);
        LineTo(hdc, x + 18, y + 8);
        SelectObject(hdc, old);
        DeleteObject(pen as _);
    }

    unsafe fn draw_package_list_icon(hdc: HDC, x: i32, y: i32, color: COLORREF) {
        let pen = CreatePen(PS_SOLID, 1, color);
        let brush = CreateSolidBrush(color);
        let old_pen = SelectObject(hdc, pen as _);
        let old_brush = SelectObject(hdc, brush as _);
        for row in [3, 11, 19] {
            Ellipse(hdc, x, y + row, x + 4, y + row + 4);
            MoveToEx(hdc, x + 9, y + row + 2, null_mut());
            LineTo(hdc, x + 23, y + row + 2);
        }
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(brush as _);
        DeleteObject(pen as _);
    }

    unsafe fn draw_progress(hdc: HDC, x: i32, y: i32, width: i32, height: i32, value: u32) {
        let track = CreateSolidBrush(SURFACE_RAISED);
        let track_rect = RECT {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        };
        FillRect(hdc, &track_rect, track);
        DeleteObject(track as _);
        let fill_width = ((width - 2).max(0) as u32 * value.min(100) / 100) as i32;
        if fill_width > 0 {
            let fill = CreateSolidBrush(MINT);
            let fill_rect = RECT {
                left: x + 1,
                top: y + 1,
                right: x + 1 + fill_width,
                bottom: y + height - 1,
            };
            FillRect(hdc, &fill_rect, fill);
            DeleteObject(fill as _);
        }
    }

    unsafe fn draw_folder_icon(hdc: HDC, x: i32, y: i32) {
        let brush = CreateSolidBrush(SURFACE);
        let pen = CreatePen(PS_SOLID, 1, MINT_SOFT);
        let old_brush = SelectObject(hdc, brush as _);
        let old_pen = SelectObject(hdc, pen as _);
        let points = [
            POINT { x, y: y + 5 },
            POINT { x: x + 8, y: y + 5 },
            POINT {
                x: x + 11,
                y: y + 2,
            },
            POINT {
                x: x + 21,
                y: y + 2,
            },
            POINT {
                x: x + 23,
                y: y + 6,
            },
            POINT {
                x: x + 23,
                y: y + 20,
            },
            POINT { x, y: y + 20 },
        ];
        Polygon(hdc, points.as_ptr(), points.len() as i32);
        MoveToEx(hdc, x + 1, y + 9, null_mut());
        LineTo(hdc, x + 22, y + 9);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(brush as _);
        DeleteObject(pen as _);
    }

    unsafe fn browse_for_folder(hwnd: HWND) -> Option<String> {
        let mut display = [0_u16; 260];
        let title = wide("Select the Seloryn installation directory");
        let info = BROWSEINFOW {
            hwndOwner: hwnd,
            pidlRoot: null_mut(),
            pszDisplayName: display.as_mut_ptr(),
            lpszTitle: title.as_ptr(),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            lpfn: None,
            lParam: 0,
            iImage: 0,
        };
        let pidl = SHBrowseForFolderW(&info);
        if pidl.is_null() {
            return None;
        }
        let mut path = [0_u16; 32768];
        let result = if SHGetPathFromIDListW(pidl, path.as_mut_ptr()) != 0 {
            Some(String::from_utf16_lossy(
                &path[..path
                    .iter()
                    .position(|value| *value == 0)
                    .unwrap_or(path.len())],
            ))
        } else {
            None
        };
        CoTaskMemFree(pidl.cast());
        result
    }

    unsafe fn controls(hwnd: HWND) -> *mut Controls {
        GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Controls
    }

    unsafe fn destroy_controls(controls: Box<Controls>) {
        for font in [
            controls.display_font,
            controls.body_font,
            controls.body_bold_font,
            controls.mono_font,
        ] {
            if !font.is_null() {
                DeleteObject(font as _);
            }
        }
        if !controls.font_resource.is_null() {
            windows_sys::Win32::Graphics::Gdi::RemoveFontMemResourceEx(controls.font_resource);
        }
        if !controls.package_bitmap.is_null() {
            DeleteObject(controls.package_bitmap as _);
        }
        DeleteObject(controls.background_brush as _);
        DeleteObject(controls.surface_brush as _);
        DeleteObject(controls.input_brush as _);
    }

    unsafe fn window_text(hwnd: HWND) -> String {
        let length = GetWindowTextLengthW(hwnd);
        let mut buffer = vec![0_u16; length as usize + 1];
        let read = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        String::from_utf16_lossy(&buffer[..read as usize])
    }

    unsafe fn is_checked(hwnd: HWND) -> bool {
        SendMessageW(hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED as isize
    }

    unsafe fn append_log(hwnd: HWND, message: &str) {
        let mut text = message.to_owned();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        SendMessageW(hwnd, EM_SETSEL, usize::MAX, usize::MAX as _);
        SendMessageW(hwnd, EM_REPLACESEL, 0, wide(&text).as_ptr() as _);
        SendMessageW(hwnd, EM_SCROLLCARET, 0, 0);
    }

    unsafe fn set_status(hwnd: HWND, message: &str) {
        SetWindowTextW(hwnd, wide(message).as_ptr());
    }

    fn install(root: &Path, add_path: bool, make_default: bool) -> Result<String, String> {
        install_with_progress(root, add_path, make_default, false, |_, _| {})
    }

    fn install_with_progress<F>(
        root: &Path,
        add_path: bool,
        make_default: bool,
        clean_previous: bool,
        mut report: F,
    ) -> Result<String, String>
    where
        F: FnMut(u32, &str),
    {
        if RUNTIME_PAYLOAD.is_empty() || LAUNCHER_PAYLOAD.is_empty() {
            return Err("This setup was built without embedded runtime payloads.".to_owned());
        }
        report(5, "Validating embedded runtime and launcher payloads");
        if clean_previous {
            report(12, "Cleaning previous Seloryn installations");
            let message = cleanup_previous(root)?;
            report(18, &message);
        } else {
            report(12, "Keeping previous installations (cleanup skipped)");
        }
        let installed = distribution::install_runtime(
            root,
            RUNTIME_VERSION,
            LANGUAGE_VERSION,
            RUNTIME_PAYLOAD,
            LAUNCHER_PAYLOAD,
            make_default,
        )
        .map_err(|error| error.to_string())?;
        report(72, "Runtime, launcher and manifest written atomically");
        let output = std::process::Command::new(&installed.runtime_path)
            .arg("--version")
            .output()
            .map_err(|error| format!("SEL-I002 failed to validate installed runtime: {error}"))?;
        if !output.status.success()
            || !String::from_utf8_lossy(&output.stdout).starts_with("seloryn 0.12 package=0.12.0 ")
        {
            return Err(
                "SEL-I003 installed runtime did not report the expected version".to_owned(),
            );
        }
        report(86, "Post-install runtime verification passed");
        if add_path {
            report(92, "Updating the current user's PATH");
            add_user_path(&root.join("bin"))?;
        }
        report(100, "Installation complete");
        Ok(format!(
            "Installed Seloryn {RUNTIME_VERSION}. SHA-256 {}. Restart shells to load PATH changes.",
            installed.sha256
        ))
    }

    fn cleanup_previous(root: &Path) -> Result<String, String> {
        let mut state = distribution::read_state(root).map_err(|error| error.to_string())?;
        let previous: Vec<String> = state
            .versions
            .iter()
            .filter(|version| version.as_str() != RUNTIME_VERSION)
            .cloned()
            .collect();
        if previous.is_empty() {
            return Ok("No previous Seloryn installations found".to_owned());
        }
        if state
            .default_version
            .as_ref()
            .is_some_and(|default| previous.iter().any(|version| version == default))
        {
            state.default_version = None;
            distribution::write_state(root, &state).map_err(|error| error.to_string())?;
        }
        for version in &previous {
            distribution::uninstall(root, version).map_err(|error| error.to_string())?;
        }
        Ok(format!(
            "Removed {} previous Seloryn installation(s); user projects were preserved",
            previous.len()
        ))
    }

    fn launch_terminal(root: &Path) -> Result<(), String> {
        let mut path = OsString::from(root.join("bin"));
        if let Some(existing) = env::var_os("PATH") {
            path.push(";");
            path.push(existing);
        }
        std::process::Command::new("cmd.exe")
            .args(["/K", "seloryn --version"])
            .env("PATH", path)
            .env("SELORYN_HOME", root)
            .current_dir(root)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("SEL-I007 could not open the verification terminal: {error}"))
    }

    fn add_user_path(entry: &Path) -> Result<(), String> {
        let entry = entry.to_string_lossy().to_string();
        unsafe {
            let key_name = wide("Environment");
            let path_name = wide("Path");
            let mut key: HKEY = null_mut();
            let status = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                key_name.as_ptr(),
                0,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                &mut key,
            );
            if status != 0 {
                return Err(format!(
                    "SEL-I004 could not open the user environment registry key ({status})"
                ));
            }
            let result = (|| {
                let existing = registry_path_value(key, path_name.as_ptr())?;
                let mut entries: Vec<String> = existing
                    .split(';')
                    .filter(|item| !item.trim().is_empty())
                    .map(str::to_owned)
                    .collect();
                if !entries.iter().any(|item| {
                    item.trim_end_matches('\\')
                        .eq_ignore_ascii_case(entry.trim_end_matches('\\'))
                }) {
                    entries.push(entry);
                }
                let joined = entries.join(";");
                let encoded = wide(&joined);
                let bytes = (encoded.len() * 2) as u32;
                let status = RegSetValueExW(
                    key,
                    path_name.as_ptr(),
                    0,
                    REG_EXPAND_SZ,
                    encoded.as_ptr().cast(),
                    bytes,
                );
                if status != 0 {
                    return Err(format!(
                        "SEL-I005 could not update the user PATH ({status})"
                    ));
                }
                let environment = wide("Environment");
                windows_sys::Win32::UI::WindowsAndMessaging::SendMessageTimeoutW(
                    HWND_BROADCAST,
                    windows_sys::Win32::UI::WindowsAndMessaging::WM_SETTINGCHANGE,
                    0,
                    environment.as_ptr() as _,
                    windows_sys::Win32::UI::WindowsAndMessaging::SMTO_ABORTIFHUNG,
                    5000,
                    null_mut(),
                );
                Ok(())
            })();
            RegCloseKey(key);
            result
        }
    }

    unsafe fn registry_path_value(key: HKEY, name: *const u16) -> Result<String, String> {
        let mut kind = 0_u32;
        let mut bytes = 0_u32;
        let status = RegQueryValueExW(key, name, null_mut(), &mut kind, null_mut(), &mut bytes);
        if status != 0 && status != 2 {
            return Err(format!("SEL-I006 could not read the user PATH ({status})"));
        }
        if status == 2 || bytes == 0 {
            return Ok(String::new());
        }
        let mut buffer = vec![0_u16; bytes as usize / 2 + 1];
        let mut length = bytes;
        let status = RegQueryValueExW(
            key,
            name,
            null_mut(),
            &mut kind,
            buffer.as_mut_ptr().cast(),
            &mut length,
        );
        if status != 0 {
            return Err(format!("SEL-I006 could not read the user PATH ({status})"));
        }
        let length = (length as usize / 2).min(buffer.len());
        Ok(String::from_utf16_lossy(&buffer[..length])
            .trim_end_matches('\0')
            .to_owned())
    }
}

#[cfg(windows)]
fn main() {
    windows_setup::main();
}
