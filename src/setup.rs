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
    use std::io::Write;
    use std::iter::Peekable;
    use std::path::{Path, PathBuf};
    use std::ptr::{null, null_mut};
    use std::str::Chars;
    use std::thread;

    use windows_sys::Win32::Foundation::{
        COLORREF, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        AddFontMemResourceEx, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, CLEARTYPE_QUALITY,
        CLIP_DEFAULT_PRECIS, CreateDIBSection, CreateFontW, CreatePen, CreateSolidBrush,
        DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER, DT_LEFT, DT_RIGHT,
        DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, DeleteObject, DrawTextW, Ellipse, EndPaint,
        FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, FillRect, HBITMAP, HBRUSH, HDC, HFONT, InvalidateRect,
        LineTo, MoveToEx, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, Polygon, RoundRect,
        SelectObject, SetBkMode, SetTextColor,
    };
    use windows_sys::Win32::Graphics::GdiPlus::{
        FillModeAlternate, GdipAddPathArc, GdipAddPathBezier, GdipAddPathLine, GdipClosePathFigure,
        GdipCreateFromHDC, GdipCreatePath, GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush,
        GdipDeleteGraphics, GdipDeletePath, GdipDeletePen, GdipDrawEllipseI, GdipDrawLineI,
        GdipDrawPath, GdipFillEllipseI, GdipFillPath, GdipSetPixelOffsetMode, GdipSetSmoothingMode,
        GdipStartPathFigure, GdiplusShutdown, GdiplusStartup, GdiplusStartupInput, GpBrush,
        GpGraphics, GpPath, GpPen, GpSolidFill, Ok as GdipOk, PixelOffsetModeHalf,
        SmoothingModeAntiAlias8x8, UnitPixel,
    };
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, RegCloseKey,
        RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows_sys::Win32::UI::Controls::{
        DRAWITEMSTRUCT, EM_REPLACESEL, EM_SCROLLCARET, EM_SETMARGINS, EM_SETSEL,
        InitCommonControls, PBM_SETPOS, PBM_SETRANGE32, PBS_SMOOTH,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows_sys::Win32::UI::Shell::{
        BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW, SHBrowseForFolderW,
        SHGetPathFromIDListW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRectEx, BS_AUTOCHECKBOX, BS_OWNERDRAW, CW_USEDEFAULT, CreateWindowExW,
        DefWindowProcW, DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL, ES_AUTOVSCROLL,
        ES_MULTILINE, ES_READONLY, GWLP_USERDATA, GetClientRect, GetCursorPos, GetMessageW,
        GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, HWND_BROADCAST,
        IsWindowVisible, KillTimer, MB_ICONERROR, MB_OK, MessageBoxW, PostMessageW,
        PostQuitMessage, RegisterClassW, SendMessageW, SetTimer, SetWindowLongPtrW, SetWindowTextW,
        ShowWindow, TranslateMessage, WM_APP, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLORBTN,
        WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DRAWITEM, WM_ERASEBKGND,
        WM_GETMINMAXINFO, WM_PAINT, WM_SETFONT, WM_TIMER, WNDCLASSW, WS_CAPTION, WS_CHILD,
        WS_CLIPCHILDREN, WS_MINIMIZEBOX, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
    };

    const RUNTIME_VERSION: &str = "0.12.0";
    const LANGUAGE_VERSION: &str = "0.12";
    const CLIENT_WIDTH: i32 = 820;
    const CLIENT_HEIGHT: i32 = 588;
    const CONTENT_LEFT: i32 = 28;
    const CONTENT_RIGHT: i32 = CLIENT_WIDTH - 28;
    const CONTENT_WIDTH: i32 = CONTENT_RIGHT - CONTENT_LEFT;
    const CARD_TOP: i32 = 196;
    const CARD_BOTTOM: i32 = 456;
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

    const BG: COLORREF = rgb(0, 0, 0);
    const SURFACE: COLORREF = rgb(9, 18, 15);
    const SURFACE_RAISED: COLORREF = rgb(14, 30, 24);
    const MINT: COLORREF = rgb(43, 209, 160);
    const MINT_SOFT: COLORREF = rgb(91, 231, 190);
    const TEAL: COLORREF = rgb(56, 211, 166);
    const TEAL_DARK: COLORREF = rgb(27, 61, 49);
    const TEXT_PRIMARY: COLORREF = rgb(245, 255, 250);
    const TEXT_SECONDARY: COLORREF = rgb(174, 201, 190);
    const TEXT_MUTED: COLORREF = rgb(99, 128, 116);
    const CTA: COLORREF = rgb(43, 209, 160);
    const CTA_HOVER: COLORREF = rgb(91, 231, 190);
    const ERROR: COLORREF = rgb(255, 104, 124);
    const WHITE: COLORREF = rgb(255, 255, 255);
    const BRAND_SYMBOL_SVG: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/brand/seloryn-symbol.svg"
    ));

    const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF {
        red | (green << 8) | (blue << 16)
    }

    fn mix_color(start: COLORREF, end: COLORREF, amount: u8) -> COLORREF {
        let amount = u32::from(amount);
        let channel = |from: u32, to: u32| {
            if to >= from {
                from + ((to - from) * amount / 255)
            } else {
                from - ((from - to) * amount / 255)
            }
        };
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
        display_font_resource: HANDLE,
        body_font_resource: HANDLE,
        display_font: HFONT,
        body_font: HFONT,
        body_large_font: HFONT,
        body_bold_font: HFONT,
        mono_font: HFONT,
        phase: u8,
        installing: bool,
        progress_value: u32,
        hover_amount: u8,
        hovered_button: HWND,
        add_path_checked: bool,
        make_default_checked: bool,
        clean_previous_checked: bool,
    }

    pub fn main() {
        let arguments: Vec<OsString> = env::args_os().skip(1).collect();
        if arguments
            .first()
            .is_some_and(|value| value == "--headless" || value == "--headless-progress")
        {
            let progress_output = arguments
                .first()
                .is_some_and(|value| value == "--headless-progress");
            let add_path = arguments.iter().any(|value| value == "--add-path");
            let make_default = arguments.iter().any(|value| value == "--make-default");
            let clean_previous = arguments.iter().any(|value| value == "--clean-previous");
            let root = arguments
                .windows(2)
                .find(|pair| pair[0] == "--install-root")
                .map(|pair| PathBuf::from(&pair[1]))
                .unwrap_or_else(default_root);
            let result = if progress_output {
                install_with_progress(
                    &root,
                    add_path,
                    make_default,
                    clean_previous,
                    |progress, message| {
                        println!("PROGRESS\t{progress}\t{}", message.replace('\t', " "));
                        let _ = std::io::stdout().flush();
                    },
                )
            } else {
                install_with_progress(&root, add_path, make_default, clean_previous, |_, _| {})
            };
            match result {
                Ok(message) => {
                    if progress_output {
                        println!("COMPLETE\tOK\t{message}");
                        let _ = std::io::stdout().flush();
                    } else {
                        println!("{message}");
                    }
                }
                Err(error) => {
                    if progress_output {
                        println!("COMPLETE\tERROR\t{}", error.replace('\t', " "));
                        let _ = std::io::stdout().flush();
                    }
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
            let mut gdiplus_token = 0_usize;
            let startup = GdiplusStartupInput {
                GdiplusVersion: 1,
                ..std::mem::zeroed()
            };
            if GdiplusStartup(&mut gdiplus_token, &startup, null_mut()) != GdipOk {
                return Err("could not initialize the setup renderer".to_owned());
            }
            let instance = GetModuleHandleW(null());
            if instance.is_null() {
                GdiplusShutdown(gdiplus_token);
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
                GdiplusShutdown(gdiplus_token);
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
                GdiplusShutdown(gdiplus_token);
                return Err("could not create the setup window".to_owned());
            }
            ShowWindow(window, 1);
            let mut message = std::mem::zeroed();
            while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            GdiplusShutdown(gdiplus_token);
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
        let (display_font_resource, display_face) =
            load_private_font(DISPLAY_FONT_PAYLOAD, "Segoe UI Semibold");
        let (body_font_resource, body_face) = load_private_font(BODY_FONT_PAYLOAD, "Segoe UI");
        let display_font = create_font(-30, FW_NORMAL, display_face);
        let body_font = create_font(-14, FW_NORMAL, body_face);
        let body_large_font = create_font(-22, FW_SEMIBOLD, body_face);
        let body_bold_font = create_font(-14, FW_SEMIBOLD, body_face);
        let mono_font = create_font(-13, FW_NORMAL, "Cascadia Mono");

        let step_one = label(hwnd, "WELCOME", 180, 78, 112, 24, ID_STEP_ONE);
        let step_two = label(hwnd, "LOCATION", 352, 78, 112, 24, ID_STEP_TWO);
        let step_three = label(hwnd, "INSTALL", 524, 78, 112, 24, ID_STEP_THREE);
        let step_four = label(hwnd, "DONE", 696, 78, 112, 24, ID_STEP_FOUR);
        let page_title = label(
            hwnd,
            "WELCOME TO SELORYN",
            CONTENT_LEFT,
            126,
            CONTENT_WIDTH,
            38,
            ID_PAGE_TITLE,
        );
        let page_body = label(
            hwnd,
            "A native per-user setup for the verified 0.12.0 runtime.",
            CONTENT_LEFT + 2,
            166,
            CONTENT_WIDTH - 4,
            24,
            ID_PAGE_BODY,
        );
        let path_label = label(
            hwnd,
            "INSTALLATION DIRECTORY",
            CONTENT_LEFT + 4,
            218,
            CONTENT_WIDTH - 4,
            20,
            ID_PATH_LABEL,
        );
        let path = edit(
            hwnd,
            &default_root().to_string_lossy(),
            CONTENT_LEFT + 4,
            246,
            588,
            38,
            ID_PATH,
        );
        let browse = button(hwnd, "Browse", 632, 246, 160, 38, ID_BROWSE);
        let add_path = checkbox(
            hwnd,
            "Add Seloryn to the current user's PATH",
            CONTENT_LEFT + 4,
            302,
            ID_ADD_PATH,
        );
        let make_default = checkbox(
            hwnd,
            "Make 0.12.0 the default version",
            CONTENT_LEFT + 4,
            336,
            ID_DEFAULT,
        );
        let clean_previous = checkbox(
            hwnd,
            "Remove previous Seloryn installations",
            CONTENT_LEFT + 4,
            370,
            ID_CLEANUP,
        );
        let log = log_edit(hwnd, CONTENT_LEFT + 4, 218, CONTENT_WIDTH - 8, 226, ID_LOG);
        let progress = CreateWindowExW(
            0,
            wide("msctls_progress32").as_ptr(),
            null(),
            WS_CHILD | PBS_SMOOTH,
            CONTENT_LEFT,
            456,
            CONTENT_WIDTH,
            8,
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
            478,
            CONTENT_WIDTH - 76,
            24,
            ID_STATUS,
        );
        let back = button(hwnd, "Back", 514, 520, 108, 40, ID_BACK);
        let action = button(hwnd, "Continue", 634, 520, 158, 40, ID_ACTION);
        let cancel = button(hwnd, "Cancel", CONTENT_LEFT, 520, 108, 40, ID_CANCEL);

        for (control, font) in [
            (step_one, body_bold_font),
            (step_two, body_bold_font),
            (step_three, body_bold_font),
            (step_four, body_bold_font),
            (page_title, body_large_font),
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
            display_font_resource,
            body_font_resource,
            display_font,
            body_font,
            body_large_font,
            body_bold_font,
            mono_font,
            phase: 1,
            installing: false,
            progress_value: 0,
            hover_amount: 0,
            hovered_button: null_mut(),
            add_path_checked: true,
            make_default_checked: true,
            clean_previous_checked: false,
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

    unsafe fn load_private_font(payload: &[u8], fallback: &'static str) -> (HANDLE, &'static str) {
        if payload.is_empty() {
            return (null_mut(), fallback);
        }
        let font_count = 0_u32;
        let token = AddFontMemResourceEx(
            payload.as_ptr().cast::<c_void>(),
            payload.len() as u32,
            null(),
            &font_count,
        );
        if token.is_null() {
            (null_mut(), fallback)
        } else {
            (
                token,
                if fallback == "Segoe UI Semibold" {
                    "Seloryn Strand Display"
                } else {
                    "Sora"
                },
            )
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
        let control = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
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

    unsafe fn log_edit(parent: HWND, x: i32, y: i32, width: i32, height: i32, id: usize) -> HWND {
        let control = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            null(),
            WS_CHILD
                | WS_TABSTOP
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
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX as u32 | BS_OWNERDRAW as u32,
            x,
            y,
            CONTENT_WIDTH - 8,
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
        if id == ID_ADD_PATH || id == ID_DEFAULT || id == ID_CLEANUP {
            match id {
                ID_ADD_PATH => (*pointer).add_path_checked = !(*pointer).add_path_checked,
                ID_DEFAULT => (*pointer).make_default_checked = !(*pointer).make_default_checked,
                ID_CLEANUP => {
                    (*pointer).clean_previous_checked = !(*pointer).clean_previous_checked
                }
                _ => {}
            }
            InvalidateRect(hwnd, null(), 0);
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
                            SetWindowTextW((*pointer).action, wide("Close").as_ptr());
                        }
                    }
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
        let add_path = (*pointer).add_path_checked;
        let make_default = (*pointer).make_default_checked;
        let clean_previous = (*pointer).clean_previous_checked;
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
                    top: 452,
                    right: CONTENT_RIGHT,
                    bottom: 470,
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
                        (*pointer).phase = 4;
                        show_phase(hwnd, 4);
                        set_status(
                            (*pointer).status,
                            "Installation verified. Open a terminal when you are ready.",
                        );
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
                3 => "Each update below maps to a real installation operation.",
                4 => "The runtime and launcher were verified successfully.",
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
                4 => "Open terminal",
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
            (*pointer).add_path,
            (*pointer).make_default,
            (*pointer).clean_previous,
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
        let checkbox = matches!(item.CtlID as usize, ID_ADD_PATH | ID_DEFAULT | ID_CLEANUP);
        if checkbox {
            let checked = match item.CtlID as usize {
                ID_ADD_PATH => !pointer.is_null() && (*pointer).add_path_checked,
                ID_DEFAULT => !pointer.is_null() && (*pointer).make_default_checked,
                ID_CLEANUP => !pointer.is_null() && (*pointer).clean_previous_checked,
                _ => false,
            };
            draw_round_rect(
                item.hDC,
                item.rcItem.left,
                item.rcItem.top + 1,
                item.rcItem.right,
                item.rcItem.bottom - 1,
                8,
                if hovered { SURFACE_RAISED } else { SURFACE },
                SURFACE,
                0.0,
            );
            let box_left = item.rcItem.left + 3;
            let box_top = item.rcItem.top + (item.rcItem.bottom - item.rcItem.top - 18) / 2;
            draw_round_rect(
                item.hDC,
                box_left,
                box_top,
                box_left + 18,
                box_top + 18,
                5,
                if checked { MINT } else { SURFACE },
                if checked { MINT } else { TEAL_DARK },
                if focused { 2.0 } else { 1.0 },
            );
            if checked {
                draw_line(
                    item.hDC,
                    box_left + 5,
                    box_top + 9,
                    box_left + 8,
                    box_top + 12,
                    WHITE,
                    1.8,
                );
                draw_line(
                    item.hDC,
                    box_left + 8,
                    box_top + 12,
                    box_left + 14,
                    box_top + 5,
                    WHITE,
                    1.8,
                );
            }
            let text = wide(&window_text(item.hwndItem));
            let mut rect = item.rcItem;
            rect.left += 30;
            SetBkMode(item.hDC, 1);
            SetTextColor(item.hDC, if disabled { TEXT_MUTED } else { TEXT_PRIMARY });
            DrawTextW(
                item.hDC,
                text.as_ptr(),
                -1,
                &mut rect,
                DT_VCENTER | DT_SINGLELINE,
            );
            return 1;
        }
        let primary = item.CtlID == ID_ACTION as u32;
        let hover_amount = if hovered && !pointer.is_null() {
            pointer.as_ref().unwrap().hover_amount
        } else {
            0
        };
        let fill = if disabled {
            SURFACE_RAISED
        } else if primary {
            if selected {
                CTA_HOVER
            } else {
                mix_color(CTA, CTA_HOVER, hover_amount)
            }
        } else {
            SURFACE_RAISED
        };
        draw_round_rect(
            item.hDC,
            item.rcItem.left,
            item.rcItem.top,
            item.rcItem.right,
            item.rcItem.bottom,
            14,
            fill,
            if disabled {
                TEAL_DARK
            } else if selected || hovered || focused {
                MINT
            } else {
                TEAL_DARK
            },
            if focused { 2.0 } else { 1.0 },
        );
        SetBkMode(item.hDC, 1);
        SetTextColor(
            item.hDC,
            if disabled {
                TEXT_SECONDARY
            } else if primary {
                BG
            } else {
                TEXT_PRIMARY
            },
        );
        if item.CtlID as usize == ID_BROWSE {
            draw_folder_icon(item.hDC, item.rcItem.left + 12, item.rcItem.top + 7);
        }
        let text = wide(&window_text(item.hwndItem));
        let mut rect = item.rcItem;
        if item.CtlID as usize == ID_BROWSE {
            rect.left += 26;
        }
        if primary && !disabled && !pointer.is_null() && (*pointer).phase < 4 {
            let center_x = item.rcItem.right - 20;
            let center_y = (item.rcItem.top + item.rcItem.bottom) / 2;
            fill_circle(item.hDC, center_x, center_y, 10, TEXT_PRIMARY);
            draw_line(
                item.hDC,
                center_x - 5,
                center_y,
                center_x + 5,
                center_y,
                BG,
                1.6,
            );
            draw_line(
                item.hDC,
                center_x + 1,
                center_y - 4,
                center_x + 5,
                center_y,
                BG,
                1.6,
            );
            draw_line(
                item.hDC,
                center_x + 5,
                center_y,
                center_x + 1,
                center_y + 4,
                BG,
                1.6,
            );
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

    fn argb(color: COLORREF) -> u32 {
        0xff00_0000 | ((color & 0xff) << 16) | (color & 0xff00) | ((color >> 16) & 0xff)
    }

    unsafe fn begin_gdip(hdc: HDC) -> *mut GpGraphics {
        let mut graphics = null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != GdipOk || graphics.is_null() {
            return null_mut();
        }
        GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias8x8);
        GdipSetPixelOffsetMode(graphics, PixelOffsetModeHalf);
        graphics
    }

    unsafe fn make_brush(color: COLORREF) -> *mut GpBrush {
        let mut brush: *mut GpSolidFill = null_mut();
        if GdipCreateSolidFill(argb(color), &mut brush) != GdipOk {
            null_mut()
        } else {
            brush.cast()
        }
    }

    unsafe fn make_pen(color: COLORREF, width: f32) -> *mut GpPen {
        let mut pen = null_mut();
        if GdipCreatePen1(argb(color), width, UnitPixel, &mut pen) != GdipOk {
            null_mut()
        } else {
            pen
        }
    }

    unsafe fn rounded_path(
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        radius: i32,
    ) -> *mut GpPath {
        let radius = radius
            .min((right - left) / 2)
            .min((bottom - top) / 2)
            .max(1);
        let mut path = null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path) != GdipOk || path.is_null() {
            return null_mut();
        }
        let l = left as f32;
        let t = top as f32;
        let r = right as f32;
        let b = bottom as f32;
        let radius = radius as f32;
        let diameter = radius * 2.0;
        GdipAddPathLine(path, l + radius, t, r - radius, t);
        GdipAddPathArc(path, r - diameter, t, diameter, diameter, 270.0, 90.0);
        GdipAddPathLine(path, r, t + radius, r, b - radius);
        GdipAddPathArc(
            path,
            r - diameter,
            b - diameter,
            diameter,
            diameter,
            0.0,
            90.0,
        );
        GdipAddPathLine(path, r - radius, b, l + radius, b);
        GdipAddPathArc(path, l, b - diameter, diameter, diameter, 90.0, 90.0);
        GdipAddPathLine(path, l, b - radius, l, t + radius);
        GdipAddPathArc(path, l, t, diameter, diameter, 180.0, 90.0);
        GdipClosePathFigure(path);
        path
    }

    unsafe fn draw_round_rect(
        hdc: HDC,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        radius: i32,
        fill: COLORREF,
        stroke: COLORREF,
        stroke_width: f32,
    ) {
        let graphics = begin_gdip(hdc);
        if graphics.is_null() {
            return;
        }
        let path = rounded_path(left, top, right, bottom, radius);
        if !path.is_null() {
            let brush = make_brush(fill);
            if !brush.is_null() {
                GdipFillPath(graphics, brush, path);
                GdipDeleteBrush(brush);
            }
            if stroke_width > 0.0 {
                let pen = make_pen(stroke, stroke_width);
                if !pen.is_null() {
                    GdipDrawPath(graphics, pen, path);
                    GdipDeletePen(pen);
                }
            }
            GdipDeletePath(path);
        }
        GdipDeleteGraphics(graphics);
    }

    unsafe fn draw_line(hdc: HDC, x1: i32, y1: i32, x2: i32, y2: i32, color: COLORREF, width: f32) {
        let graphics = begin_gdip(hdc);
        if graphics.is_null() {
            return;
        }
        let pen = make_pen(color, width);
        if !pen.is_null() {
            GdipDrawLineI(graphics, pen, x1, y1, x2, y2);
            GdipDeletePen(pen);
        }
        GdipDeleteGraphics(graphics);
    }

    unsafe fn fill_circle(hdc: HDC, x: i32, y: i32, radius: i32, color: COLORREF) {
        let graphics = begin_gdip(hdc);
        if graphics.is_null() {
            return;
        }
        let brush = make_brush(color);
        if !brush.is_null() {
            GdipFillEllipseI(
                graphics,
                brush,
                x - radius,
                y - radius,
                radius * 2,
                radius * 2,
            );
            GdipDeleteBrush(brush);
        }
        GdipDeleteGraphics(graphics);
    }

    unsafe fn draw_circle(
        hdc: HDC,
        x: i32,
        y: i32,
        radius: i32,
        fill: COLORREF,
        stroke: COLORREF,
        width: f32,
    ) {
        let graphics = begin_gdip(hdc);
        if graphics.is_null() {
            return;
        }
        let brush = make_brush(fill);
        if !brush.is_null() {
            GdipFillEllipseI(
                graphics,
                brush,
                x - radius,
                y - radius,
                radius * 2,
                radius * 2,
            );
            GdipDeleteBrush(brush);
        }
        let pen = make_pen(stroke, width);
        if !pen.is_null() {
            GdipDrawEllipseI(
                graphics,
                pen,
                x - radius,
                y - radius,
                radius * 2,
                radius * 2,
            );
            GdipDeletePen(pen);
        }
        GdipDeleteGraphics(graphics);
    }

    unsafe fn draw_text_line(
        hdc: HDC,
        font: HFONT,
        text: &str,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        color: COLORREF,
        flags: u32,
    ) {
        let old_font = SelectObject(hdc, font as _);
        SetBkMode(hdc, 1);
        SetTextColor(hdc, color);
        let mut rect = RECT {
            left,
            top,
            right,
            bottom,
        };
        let value = wide(text);
        DrawTextW(hdc, value.as_ptr(), -1, &mut rect, flags);
        SelectObject(hdc, old_font);
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
                1 => draw_welcome_visual(hdc, pointer),
                2 => draw_surface_card(hdc, CONTENT_LEFT, CARD_TOP, CONTENT_RIGHT, CARD_BOTTOM),
                3 | 4 => draw_surface_card(hdc, CONTENT_LEFT, CARD_TOP, CONTENT_RIGHT, 452),
                _ => {}
            }
            if (*pointer).phase == 3 || (*pointer).phase == 4 {
                draw_progress(
                    hdc,
                    CONTENT_LEFT,
                    456,
                    CONTENT_WIDTH,
                    8,
                    (*pointer).progress_value,
                );
                draw_text_line(
                    hdc,
                    (*pointer).body_bold_font,
                    &format!("{}%", (*pointer).progress_value),
                    CONTENT_RIGHT - 64,
                    474,
                    CONTENT_RIGHT,
                    496,
                    MINT_SOFT,
                    DT_RIGHT | DT_SINGLELINE,
                );
            }
        }
        EndPaint(hwnd, &paint);
        0
    }

    unsafe fn draw_surface_card(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32) {
        draw_round_rect(hdc, left, top, right, bottom, 14, SURFACE, TEAL_DARK, 1.0);
    }

    unsafe fn draw_stepper(hdc: HDC, pointer: *mut Controls) {
        let centers = [236, 408, 580, 752];
        let y = 44;
        draw_line(hdc, centers[0], y, centers[3], y, TEAL_DARK, 1.0);
        let active = (*pointer).phase.min(4) as usize;
        if active > 1 {
            draw_line(hdc, centers[0], y, centers[active - 1], y, MINT, 2.0);
        }
        draw_brand_mark(hdc, 22, 24, 30, 30);
        draw_text_line(
            hdc,
            (*pointer).body_bold_font,
            "SELORYN",
            58,
            27,
            198,
            46,
            TEAL,
            DT_LEFT | DT_SINGLELINE,
        );
        for (index, center) in centers.iter().enumerate() {
            let current = index + 1 == active;
            let completed = index + 1 < active;
            let fill = if current || completed { MINT } else { BG };
            let stroke = if current || completed {
                MINT
            } else {
                TEAL_DARK
            };
            let number_color = if current || completed {
                WHITE
            } else {
                TEXT_MUTED
            };
            draw_circle(
                hdc,
                *center,
                y,
                16,
                fill,
                stroke,
                if current { 2.0 } else { 1.0 },
            );
            draw_text_line(
                hdc,
                (*pointer).body_bold_font,
                &format!("{:02}", index + 1),
                center - 14,
                y - 9,
                center + 14,
                y + 9,
                number_color,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }

    fn next_svg_number(chars: &mut Peekable<Chars<'_>>) -> Option<f32> {
        while chars
            .peek()
            .is_some_and(|value| value.is_whitespace() || *value == ',')
        {
            chars.next();
        }
        let mut token = String::new();
        while chars.peek().is_some_and(|value| {
            value.is_ascii_digit() || matches!(*value, '.' | '-' | '+' | 'e' | 'E')
        }) {
            token.push(chars.next().unwrap());
        }
        token.parse().ok()
    }

    unsafe fn draw_svg_path(
        graphics: *mut GpGraphics,
        d: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        color: COLORREF,
    ) {
        let scale = (width as f32 / 91.0).min(height as f32 / 88.0);
        let origin_x = x as f32 - 35.0 * scale;
        let origin_y = y as f32 - 37.0 * scale;
        let map = |value: f32, offset: f32| value * scale + offset;
        let mut chars = d.chars().peekable();
        let mut command = 'M';
        let mut current = (0.0_f32, 0.0_f32);
        let mut path = null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path) != GdipOk || path.is_null() {
            return;
        }
        GdipStartPathFigure(path);
        while chars.peek().is_some() {
            while chars
                .peek()
                .is_some_and(|value| value.is_whitespace() || *value == ',')
            {
                chars.next();
            }
            if let Some(value) = chars
                .peek()
                .copied()
                .filter(|value| value.is_ascii_alphabetic())
            {
                command = value;
                chars.next();
                continue;
            }
            let count = match command {
                'M' | 'm' => 2,
                'H' | 'h' | 'V' | 'v' => 1,
                'C' | 'c' => 6,
                _ => break,
            };
            let mut values = [0.0_f32; 6];
            for value in values.iter_mut().take(count) {
                let Some(number) = next_svg_number(&mut chars) else {
                    break;
                };
                *value = number;
            }
            let point = |px: f32, py: f32| (map(px, origin_x), map(py, origin_y));
            match command {
                'M' => {
                    current = (values[0], values[1]);
                    GdipAddPathLine(
                        path,
                        map(current.0, origin_x),
                        map(current.1, origin_y),
                        map(current.0, origin_x),
                        map(current.1, origin_y),
                    );
                }
                'H' => {
                    let next = (values[0], current.1);
                    GdipAddPathLine(
                        path,
                        map(current.0, origin_x),
                        map(current.1, origin_y),
                        map(next.0, origin_x),
                        map(next.1, origin_y),
                    );
                    current = next;
                }
                'V' => {
                    let next = (current.0, values[0]);
                    GdipAddPathLine(
                        path,
                        map(current.0, origin_x),
                        map(current.1, origin_y),
                        map(next.0, origin_x),
                        map(next.1, origin_y),
                    );
                    current = next;
                }
                'C' => {
                    let end = point(values[4], values[5]);
                    GdipAddPathBezier(
                        path,
                        map(current.0, origin_x),
                        map(current.1, origin_y),
                        map(values[0], origin_x),
                        map(values[1], origin_y),
                        map(values[2], origin_x),
                        map(values[3], origin_y),
                        end.0,
                        end.1,
                    );
                    current = (values[4], values[5]);
                }
                _ => {}
            }
        }
        let pen = make_pen(color, (20.0 * scale).max(2.0));
        if !pen.is_null() {
            GdipDrawPath(graphics, pen, path);
            GdipDeletePen(pen);
        }
        GdipDeletePath(path);
    }

    unsafe fn draw_brand_mark(hdc: HDC, x: i32, y: i32, width: i32, height: i32) {
        let graphics = begin_gdip(hdc);
        if graphics.is_null() {
            return;
        }
        for (index, path) in BRAND_SYMBOL_SVG
            .split("d=\"")
            .skip(1)
            .filter_map(|value| value.split('\"').next())
            .filter(|value| value.trim_start().starts_with('M'))
            .take(2)
            .enumerate()
        {
            draw_svg_path(
                graphics,
                path,
                x,
                y,
                width,
                height,
                if index == 0 { MINT } else { TEAL },
            );
        }
        GdipDeleteGraphics(graphics);
    }

    unsafe fn draw_welcome_visual(hdc: HDC, pointer: *mut Controls) {
        draw_surface_card(hdc, CONTENT_LEFT, CARD_TOP, CONTENT_RIGHT, CARD_BOTTOM);
        draw_line(hdc, 448, 222, 448, 426, TEAL_DARK, 1.0);
        draw_text_line(
            hdc,
            (*pointer).body_bold_font,
            "READY FOR YOUR MACHINE",
            52,
            222,
            414,
            242,
            MINT,
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text_line(
            hdc,
            (*pointer).body_large_font,
            "A clean runtime.\nKept local.",
            52,
            248,
            410,
            320,
            TEXT_PRIMARY,
            DT_LEFT | DT_WORDBREAK,
        );
        draw_text_line(
            hdc,
            (*pointer).body_font,
            "Seloryn installs for this Windows user and keeps\nprojects outside the runtime untouched.",
            52,
            328,
            414,
            380,
            TEXT_SECONDARY,
            DT_LEFT | DT_WORDBREAK,
        );
        draw_round_rect(
            hdc,
            52,
            394,
            154,
            426,
            10,
            SURFACE_RAISED,
            SURFACE_RAISED,
            0.0,
        );
        draw_round_rect(
            hdc,
            216,
            394,
            330,
            426,
            10,
            SURFACE_RAISED,
            SURFACE_RAISED,
            0.0,
        );
        draw_text_line(
            hdc,
            (*pointer).body_bold_font,
            "PER-USER",
            68,
            401,
            142,
            419,
            TEAL,
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text_line(
            hdc,
            (*pointer).body_bold_font,
            "VERIFIED",
            232,
            401,
            318,
            419,
            TEAL,
            DT_LEFT | DT_SINGLELINE,
        );

        draw_round_rect(hdc, 478, 222, 768, 426, 12, SURFACE_RAISED, TEAL_DARK, 1.0);
        draw_brand_mark(hdc, 510, 240, 28, 32);
        draw_text_line(
            hdc,
            (*pointer).body_bold_font,
            "SELORYN RUNTIME",
            544,
            238,
            744,
            258,
            TEAL,
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text_line(
            hdc,
            (*pointer).display_font,
            "0.12.0",
            510,
            278,
            744,
            320,
            TEXT_PRIMARY,
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text_line(
            hdc,
            (*pointer).body_font,
            "Windows x64  ·  verified preview",
            512,
            332,
            748,
            352,
            TEXT_SECONDARY,
            DT_LEFT | DT_SINGLELINE,
        );
        draw_line(hdc, 512, 376, 734, 376, TEAL_DARK, 1.0);
        draw_circle(hdc, 524, 398, 7, MINT, MINT, 1.0);
        draw_text_line(
            hdc,
            (*pointer).body_font,
            "4 guided stages",
            544,
            388,
            720,
            408,
            TEXT_SECONDARY,
            DT_LEFT | DT_SINGLELINE,
        );
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
        draw_round_rect(
            hdc,
            x,
            y,
            x + width,
            y + height,
            height / 2,
            SURFACE_RAISED,
            SURFACE_RAISED,
            0.0,
        );
        let fill_width = (width as u32 * value.min(100) / 100) as i32;
        if fill_width > 0 {
            draw_round_rect(
                hdc,
                x,
                y,
                x + fill_width.max(height),
                y + height,
                height / 2,
                MINT,
                MINT,
                0.0,
            );
        }
    }

    unsafe fn draw_folder_icon(hdc: HDC, x: i32, y: i32) {
        draw_round_rect(hdc, x, y + 3, x + 24, y + 21, 4, SURFACE, MINT_SOFT, 1.4);
        draw_line(hdc, x + 2, y + 9, x + 22, y + 9, MINT, 1.4);
        draw_line(hdc, x + 3, y + 3, x + 9, y + 3, MINT_SOFT, 1.4);
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
            controls.body_large_font,
            controls.body_bold_font,
            controls.mono_font,
        ] {
            if !font.is_null() {
                DeleteObject(font as _);
            }
        }
        for font_resource in [controls.display_font_resource, controls.body_font_resource] {
            if !font_resource.is_null() {
                windows_sys::Win32::Graphics::Gdi::RemoveFontMemResourceEx(font_resource);
            }
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
