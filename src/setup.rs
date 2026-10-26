#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("Seloryn Setup is currently implemented for Windows x64 only.");
    std::process::exit(2);
}

#[cfg(windows)]
mod windows_setup {
    #![allow(dead_code, unsafe_op_in_unsafe_fn)]

    #[path = "../distribution.rs"]
    mod distribution;
    #[path = "../json.rs"]
    mod json;

    include!(concat!(env!("OUT_DIR"), "/installer_payload.rs"));

    use std::env;
    use std::ffi::{OsString, c_void};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::ptr::{null, null_mut};
    use std::thread;

    use windows_sys::Win32::Foundation::{
        COLORREF, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        AddFontMemResourceEx, BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW,
        CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_SINGLELINE,
        DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD,
        FillRect, HBITMAP, HBRUSH, HDC, HFONT, InvalidateRect, LineTo, MoveToEx,
        OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, Polygon, RoundRect, SelectObject, SetBkMode,
        SetTextColor,
    };
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, RegCloseKey,
        RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows_sys::Win32::UI::Controls::{
        BST_CHECKED, DRAWITEMSTRUCT, EM_REPLACESEL, EM_SETSEL, InitCommonControls, PBM_SETPOS,
        PBM_SETRANGE32, PBS_SMOOTH,
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
        HWND_BROADCAST, IMAGE_BITMAP, IsWindowVisible, KillTimer, LR_CREATEDIBSECTION,
        LR_LOADFROMFILE, MB_ICONERROR, MB_OK, MessageBoxW, PostMessageW, PostQuitMessage,
        RegisterClassW, SendMessageW, SetTimer, SetWindowLongPtrW, SetWindowTextW, ShowWindow,
        TranslateMessage, WM_APP, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLORBTN, WM_CTLCOLOREDIT,
        WM_CTLCOLORSTATIC, WM_DESTROY, WM_DRAWITEM, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_PAINT,
        WM_SETFONT, WM_TIMER, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN,
        WS_EX_CLIENTEDGE, WS_MINIMIZEBOX, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
    };

    const SETUP_VERSION: &str = env!("CARGO_PKG_VERSION");
    const RUNTIME_VERSION: &str = "0.12.0";
    const LANGUAGE_VERSION: &str = "0.12";
    const CLIENT_WIDTH: i32 = 880;
    const CLIENT_HEIGHT: i32 = 560;
    const PANEL_WIDTH: i32 = 280;
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
    const ID_RUNTIME_CARD: usize = 116;
    const ID_CLEANUP: usize = 117;
    const ID_BRAND: usize = 118;
    const ID_PATH_LABEL: usize = 119;
    const SS_BITMAP_STYLE: u32 = 0x000E;
    const HOVER_TIMER: usize = 1;

    const BG: COLORREF = rgb(11, 26, 23);
    const SURFACE: COLORREF = rgb(16, 40, 32);
    const SURFACE_RAISED: COLORREF = rgb(23, 58, 48);
    const MINT: COLORREF = rgb(49, 208, 170);
    const MINT_SOFT: COLORREF = rgb(107, 231, 198);
    const TEAL: COLORREF = rgb(35, 88, 74);
    const TEXT_PRIMARY: COLORREF = rgb(243, 248, 245);
    const TEXT_SECONDARY: COLORREF = rgb(185, 205, 198);
    const ERROR: COLORREF = rgb(239, 133, 133);

    const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF {
        red | (green << 8) | (blue << 16)
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
        runtime_card: HWND,
        path_label: HWND,
        status: HWND,
        step_one: HWND,
        step_two: HWND,
        step_three: HWND,
        step_four: HWND,
        panel: HWND,
        bitmap: HBITMAP,
        background_brush: HBRUSH,
        surface_brush: HBRUSH,
        font_resource: HANDLE,
        display_font: HFONT,
        body_font: HFONT,
        body_bold_font: HFONT,
        mono_font: HFONT,
        phase: u8,
        installing: bool,
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
            let window_name = wide("Seloryn Setup — 0.12.0");
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
        let surface_brush = CreateSolidBrush(SURFACE_RAISED);
        let (font_resource, display_face) = load_private_font();
        let display_font = create_font(-30, FW_SEMIBOLD, display_face);
        let body_font = create_font(-15, FW_NORMAL, "Segoe UI");
        let body_bold_font = create_font(-15, FW_SEMIBOLD, "Segoe UI");
        let mono_font = create_font(-14, FW_NORMAL, "Cascadia Mono");

        let panel = CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            null(),
            WS_CHILD | WS_VISIBLE | SS_BITMAP_STYLE,
            CLIENT_WIDTH - PANEL_WIDTH,
            0,
            PANEL_WIDTH,
            CLIENT_HEIGHT,
            hwnd,
            null_mut(),
            instance,
            null(),
        );
        let bitmap = load_panel_bitmap(panel);
        let brand = label(
            hwnd,
            &format!("SELORYN  /  SETUP  /  {SETUP_VERSION}"),
            32,
            24,
            536,
            30,
            ID_BRAND,
        );
        let step_one = label(hwnd, "01  WELCOME", 32, 70, 124, 24, ID_STEP_ONE);
        let step_two = label(hwnd, "02  LOCATION", 168, 70, 136, 24, ID_STEP_TWO);
        let step_three = label(hwnd, "03  INSTALL", 316, 70, 124, 24, ID_STEP_THREE);
        let step_four = label(hwnd, "04  DONE", 452, 70, 116, 24, ID_STEP_FOUR);
        let page_title = label(hwnd, "INSTALL SELORYN", 32, 124, 536, 38, ID_PAGE_TITLE);
        let page_body = label(
            hwnd,
            "A native per-user setup for the verified 0.12.0 runtime.",
            34,
            170,
            536,
            36,
            ID_PAGE_BODY,
        );
        let runtime_card = label(
            hwnd,
            "INSTALL RUNTIME  0.12.0\r\nCREATE SELORYN LAUNCHER\r\nVERIFY  seloryn --version",
            34,
            226,
            536,
            112,
            ID_RUNTIME_CARD,
        );
        let path_label = label(
            hwnd,
            "INSTALLATION DIRECTORY",
            34,
            230,
            536,
            22,
            ID_PATH_LABEL,
        );
        let path = edit(
            hwnd,
            &default_root().to_string_lossy(),
            34,
            258,
            430,
            34,
            ID_PATH,
        );
        let browse = button(hwnd, "Browse", 476, 258, 92, 34, ID_BROWSE);
        let add_path = checkbox(
            hwnd,
            "Add Seloryn to the current user's PATH",
            34,
            308,
            ID_ADD_PATH,
        );
        let make_default = checkbox(hwnd, "Make 0.12.0 the default version", 34, 342, ID_DEFAULT);
        let clean_previous = checkbox(
            hwnd,
            "Remove previous Seloryn installations",
            34,
            376,
            ID_CLEANUP,
        );
        SendMessageW(add_path, BM_SETCHECK, BST_CHECKED as _, 0);
        SendMessageW(make_default, BM_SETCHECK, BST_CHECKED as _, 0);
        let log = log_edit(hwnd, 32, 218, 536, 208, ID_LOG);
        let progress = CreateWindowExW(
            0,
            wide("msctls_progress32").as_ptr(),
            null(),
            WS_CHILD | PBS_SMOOTH,
            32,
            438,
            536,
            10,
            hwnd,
            ID_PROGRESS as _,
            instance,
            null(),
        );
        SendMessageW(progress, PBM_SETRANGE32, 0, 100);
        let status = label(hwnd, "Ready to begin.", 32, 462, 536, 22, ID_STATUS);
        let back = button(hwnd, "Back", 332, 508, 96, 36, ID_BACK);
        let action = button(hwnd, "Continue", 436, 508, 132, 36, ID_ACTION);
        let cancel = button(hwnd, "Cancel", 32, 508, 96, 36, ID_CANCEL);

        for (control, font) in [
            (brand, display_font),
            (step_one, body_bold_font),
            (step_two, body_bold_font),
            (step_three, body_bold_font),
            (step_four, body_bold_font),
            (page_title, display_font),
            (page_body, body_font),
            (runtime_card, body_font),
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
            runtime_card,
            path_label,
            status,
            step_one,
            step_two,
            step_three,
            step_four,
            panel,
            bitmap,
            background_brush,
            surface_brush,
            font_resource,
            display_font,
            body_font,
            body_bold_font,
            mono_font,
            phase: 1,
            installing: false,
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
            (token, "Seloryn Strand")
        }
    }

    unsafe fn load_panel_bitmap(panel: HWND) -> HBITMAP {
        if PANEL_PAYLOAD.is_empty() {
            return null_mut();
        }
        let path = env::temp_dir().join(format!(
            "seloryn-installer-panel-{}.bmp",
            std::process::id()
        ));
        if fs::write(&path, PANEL_PAYLOAD).is_err() {
            return null_mut();
        }
        let name = wide(&path.to_string_lossy());
        let bitmap = windows_sys::Win32::UI::WindowsAndMessaging::LoadImageW(
            null_mut(),
            name.as_ptr(),
            IMAGE_BITMAP,
            PANEL_WIDTH,
            CLIENT_HEIGHT,
            LR_LOADFROMFILE | LR_CREATEDIBSECTION,
        ) as HBITMAP;
        let _ = fs::remove_file(path);
        if !bitmap.is_null() {
            SendMessageW(panel, 0x0172, IMAGE_BITMAP as _, bitmap as _);
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
        CreateWindowExW(
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
        )
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
            return 0;
        }
        if id == ID_ACTION {
            match (*pointer).phase {
                1 => {
                    (*pointer).phase = 2;
                    show_phase(hwnd, 2);
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
                SendMessageW((*pointer).progress, PBM_SETPOS, progress as _, 0);
                append_log((*pointer).log, &message);
                set_status((*pointer).status, &message);
            }
            InstallEvent::Complete(result) => {
                (*pointer).installing = false;
                EnableWindow((*pointer).back, 1);
                EnableWindow((*pointer).cancel, 1);
                match result {
                    Ok(message) => {
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
        let welcome = phase == 1;
        for (control, visible) in [
            ((*pointer).path, choose),
            ((*pointer).browse, choose),
            ((*pointer).path_label, choose),
            ((*pointer).add_path, choose),
            ((*pointer).make_default, choose),
            ((*pointer).clean_previous, choose),
            ((*pointer).runtime_card, welcome),
            ((*pointer).log, installing),
            ((*pointer).progress, installing),
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
                1 => "A native, per-user installation for the Seloryn 0.12.0 runtime.",
                2 => "Choose a location, then decide how to handle older Seloryn versions.",
                3 => "The setup is working in the background. Every message below maps to a real operation.",
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
        } else if control == (*pointer).runtime_card
            || control == (*pointer).page_body
            || control == (*pointer).path_label
            || control == (*pointer).status
        {
            TEXT_SECONDARY
        } else if step.is_some_and(|value| value == (*pointer).phase) {
            MINT
        } else if step.is_some_and(|value| value < (*pointer).phase) {
            MINT_SOFT
        } else {
            TEXT_PRIMARY
        };
        SetTextColor(hdc, color);
        if control == (*pointer).path
            || control == (*pointer).log
            || control == (*pointer).runtime_card
        {
            (*pointer).surface_brush as LRESULT
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
    }

    unsafe fn handle_draw_item(hwnd: HWND, lparam: LPARAM) -> LRESULT {
        let item = &*(lparam as *const DRAWITEMSTRUCT);
        let pointer = controls(hwnd);
        let hovered = !pointer.is_null() && item.hwndItem == (*pointer).hovered_button;
        let selected = item.itemState & 1 != 0;
        let disabled = item.itemState & 0x0004 != 0;
        let focused = item.itemState & 0x0010 != 0;
        let primary = item.CtlID == ID_ACTION as u32;
        let fill = if disabled {
            SURFACE
        } else if primary {
            if selected {
                TEAL
            } else if hovered {
                MINT_SOFT
            } else {
                MINT
            }
        } else if selected {
            MINT
        } else if hovered {
            TEAL
        } else {
            SURFACE_RAISED
        };
        let fill_brush = CreateSolidBrush(fill);
        let old_brush = SelectObject(item.hDC, fill_brush as _);
        FillRect(item.hDC, &item.rcItem, fill_brush);
        let border = CreatePen(
            PS_SOLID,
            if focused { 2 } else { 1 },
            if disabled {
                TEAL
            } else if selected || hovered || focused {
                MINT_SOFT
            } else {
                TEAL
            },
        );
        let old = SelectObject(item.hDC, border as _);
        RoundRect(
            item.hDC,
            item.rcItem.left,
            item.rcItem.top,
            item.rcItem.right,
            item.rcItem.bottom,
            8,
            8,
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
            } else if primary {
                BG
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
        let panel_left = client.right - PANEL_WIDTH;
        let pointer = controls(hwnd);
        if !pointer.is_null() && (*pointer).phase == 1 {
            draw_surface_card(hdc, 28, 220, panel_left - 28, 348);
        }
        let divider = CreatePen(PS_SOLID, 1, SURFACE_RAISED);
        let old = SelectObject(hdc, divider as _);
        MoveToEx(hdc, panel_left, 0, null_mut());
        LineTo(hdc, panel_left, client.bottom);
        MoveToEx(hdc, 40, 116, null_mut());
        LineTo(hdc, panel_left - 26, 116);
        SelectObject(hdc, old);
        DeleteObject(divider as _);
        if !pointer.is_null() && (*pointer).bitmap.is_null() {
            draw_fallback_panel(hdc, &client);
        }
        EndPaint(hwnd, &paint);
        0
    }

    unsafe fn draw_surface_card(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32) {
        let brush = CreateSolidBrush(SURFACE);
        let pen = CreatePen(PS_SOLID, 1, TEAL);
        let old_brush = SelectObject(hdc, brush as _);
        let old_pen = SelectObject(hdc, pen as _);
        RoundRect(hdc, left, top, right, bottom, 12, 12);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        DeleteObject(pen as _);
        DeleteObject(brush as _);
    }

    unsafe fn draw_folder_icon(hdc: HDC, x: i32, y: i32) {
        let brush = CreateSolidBrush(SURFACE);
        let pen = CreatePen(PS_SOLID, 1, MINT_SOFT);
        let old_brush = SelectObject(hdc, brush as _);
        let old_pen = SelectObject(hdc, pen as _);
        RoundRect(hdc, x, y, x + 23, y + 20, 4, 4);
        MoveToEx(hdc, x + 2, y + 7, null_mut());
        LineTo(hdc, x + 21, y + 7);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(brush as _);
        DeleteObject(pen as _);
    }

    unsafe fn draw_fallback_panel(hdc: HDC, client: &RECT) {
        let panel_brush = CreateSolidBrush(SURFACE);
        let old = SelectObject(hdc, panel_brush as _);
        let panel_left = client.right - PANEL_WIDTH;
        let rect = RECT {
            left: panel_left,
            top: 0,
            right: client.right,
            bottom: client.bottom,
        };
        FillRect(hdc, &rect, panel_brush);
        SelectObject(hdc, old);
        DeleteObject(panel_brush as _);
        let pen = CreatePen(PS_SOLID, 1, TEAL);
        let old_pen = SelectObject(hdc, pen as _);
        for offset in [44, 146, 256] {
            MoveToEx(hdc, panel_left + offset, 0, null_mut());
            LineTo(hdc, client.right - 24, client.bottom);
        }
        let points = [
            windows_sys::Win32::Foundation::POINT {
                x: panel_left + 70,
                y: client.bottom - 170,
            },
            windows_sys::Win32::Foundation::POINT {
                x: panel_left + 160,
                y: client.bottom - 220,
            },
            windows_sys::Win32::Foundation::POINT {
                x: client.right - 20,
                y: client.bottom - 170,
            },
            windows_sys::Win32::Foundation::POINT {
                x: panel_left + 160,
                y: client.bottom - 120,
            },
        ];
        Polygon(hdc, points.as_ptr(), points.len() as i32);
        SelectObject(hdc, old_pen);
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
        if !controls.bitmap.is_null() {
            DeleteObject(controls.bitmap as _);
        }
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
        DeleteObject(controls.background_brush as _);
        DeleteObject(controls.surface_brush as _);
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
