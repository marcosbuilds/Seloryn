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
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, RegCloseKey,
        RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows_sys::Win32::UI::Controls::BST_CHECKED;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, CW_USEDEFAULT,
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL,
        GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW,
        HWND_BROADCAST, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MessageBoxW, PostQuitMessage,
        RegisterClassW, SMTO_ABORTIFHUNG, SW_SHOW, SendMessageTimeoutW, SendMessageW,
        SetWindowLongPtrW, SetWindowTextW, ShowWindow, TranslateMessage, WM_CLOSE, WM_COMMAND,
        WM_CREATE, WM_DESTROY, WM_SETTINGCHANGE, WNDCLASSW, WS_BORDER, WS_CHILD, WS_EX_CLIENTEDGE,
        WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE,
    };

    const SETUP_VERSION: &str = env!("CARGO_PKG_VERSION");
    const RUNTIME_VERSION: &str = "0.12.0";
    const LANGUAGE_VERSION: &str = "0.12";
    const ID_PATH: usize = 100;
    const ID_ADD_PATH: usize = 101;
    const ID_DEFAULT: usize = 102;
    const ID_INSTALL: usize = 103;
    const ID_CANCEL: usize = 104;
    const ID_STATUS: usize = 105;

    struct Controls {
        path: HWND,
        add_path: HWND,
        make_default: HWND,
        status: HWND,
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

    fn run_window() -> Result<(), String> {
        unsafe {
            let instance = GetModuleHandleW(null());
            if instance.is_null() {
                return Err("could not obtain the setup module handle".to_owned());
            }
            let class_name = wide("SelorynSetupWindow");
            let window_name = wide("Seloryn Setup — 0.12.0");
            let class = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: null_mut(),
                hCursor: null_mut(),
                hbrBackground: (5 + 1) as _,
                lpszMenuName: null(),
                lpszClassName: class_name.as_ptr(),
            };
            if RegisterClassW(&class) == 0 {
                return Err("could not register the setup window".to_owned());
            }
            let window = CreateWindowExW(
                0,
                class_name.as_ptr(),
                window_name.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                560,
                370,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            if window.is_null() {
                return Err("could not create the setup window".to_owned());
            }
            ShowWindow(window, SW_SHOW);
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
                let instance = GetModuleHandleW(null());
                let static_class = wide("STATIC");
                let edit_class = wide("EDIT");
                let button_class = wide("BUTTON");
                let heading = wide(&format!("Seloryn Setup {SETUP_VERSION}"));
                let description = wide("Native per-user installation — isolated runtime version");
                let path_label = wide("Installation directory:");
                let default_path = wide(&default_root().to_string_lossy());
                let add_path_text = wide("Add Seloryn bin to the user PATH");
                let default_text = wide("Make 0.12.0 the default version");
                let install_text = wide("Install");
                let cancel_text = wide("Cancel");
                let status_text = wide("Ready to verify the embedded runtime payload.");
                CreateWindowExW(
                    0,
                    static_class.as_ptr(),
                    heading.as_ptr(),
                    WS_CHILD | WS_VISIBLE,
                    24,
                    24,
                    480,
                    28,
                    hwnd,
                    null_mut(),
                    instance,
                    null(),
                );
                CreateWindowExW(
                    0,
                    static_class.as_ptr(),
                    description.as_ptr(),
                    WS_CHILD | WS_VISIBLE,
                    24,
                    56,
                    480,
                    24,
                    hwnd,
                    null_mut(),
                    instance,
                    null(),
                );
                CreateWindowExW(
                    0,
                    static_class.as_ptr(),
                    path_label.as_ptr(),
                    WS_CHILD | WS_VISIBLE,
                    24,
                    96,
                    480,
                    24,
                    hwnd,
                    null_mut(),
                    instance,
                    null(),
                );
                let path = CreateWindowExW(
                    WS_EX_CLIENTEDGE,
                    edit_class.as_ptr(),
                    default_path.as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32 | WS_BORDER,
                    24,
                    120,
                    480,
                    28,
                    hwnd,
                    ID_PATH as _,
                    instance,
                    null(),
                );
                let add_path = CreateWindowExW(
                    0,
                    button_class.as_ptr(),
                    add_path_text.as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                    24,
                    160,
                    300,
                    24,
                    hwnd,
                    ID_ADD_PATH as _,
                    instance,
                    null(),
                );
                SendMessageW(add_path, BM_SETCHECK, BST_CHECKED as _, 0);
                let make_default = CreateWindowExW(
                    0,
                    button_class.as_ptr(),
                    default_text.as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                    24,
                    190,
                    300,
                    24,
                    hwnd,
                    ID_DEFAULT as _,
                    instance,
                    null(),
                );
                SendMessageW(make_default, BM_SETCHECK, BST_CHECKED as _, 0);
                let _install = CreateWindowExW(
                    0,
                    button_class.as_ptr(),
                    install_text.as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
                    300,
                    250,
                    100,
                    32,
                    hwnd,
                    ID_INSTALL as _,
                    instance,
                    null(),
                );
                let cancel = CreateWindowExW(
                    0,
                    button_class.as_ptr(),
                    cancel_text.as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                    410,
                    250,
                    94,
                    32,
                    hwnd,
                    ID_CANCEL as _,
                    instance,
                    null(),
                );
                let status = CreateWindowExW(
                    0,
                    static_class.as_ptr(),
                    status_text.as_ptr(),
                    WS_CHILD | WS_VISIBLE,
                    24,
                    300,
                    480,
                    24,
                    hwnd,
                    ID_STATUS as _,
                    instance,
                    null(),
                );
                let controls = Box::new(Controls {
                    path,
                    add_path,
                    make_default,
                    status,
                });
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(controls) as isize);
                let _ = cancel;
                0
            }
            WM_COMMAND => {
                let command_id = wparam & 0xffff;
                if command_id == ID_CANCEL {
                    DestroyWindow(hwnd);
                    return 0;
                }
                if command_id == ID_INSTALL {
                    let controls = controls(hwnd);
                    if !controls.is_null() {
                        let controls = &*controls;
                        let root = PathBuf::from(window_text(controls.path));
                        set_status(
                            controls.status,
                            "Validating payload and preparing an isolated version directory...",
                        );
                        match install(
                            &root,
                            is_checked(controls.add_path),
                            is_checked(controls.make_default),
                        ) {
                            Ok(message) => {
                                set_status(controls.status, &message);
                                let title = wide("Seloryn Setup");
                                let text = wide(&message);
                                MessageBoxW(
                                    hwnd,
                                    text.as_ptr(),
                                    title.as_ptr(),
                                    MB_OK | MB_ICONINFORMATION,
                                );
                            }
                            Err(error) => set_status(controls.status, &error),
                        }
                    }
                    return 0;
                }
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                let pointer = controls(hwnd);
                if !pointer.is_null() {
                    drop(Box::from_raw(pointer));
                }
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    unsafe fn controls(hwnd: HWND) -> *mut Controls {
        GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Controls
    }

    unsafe fn set_status(hwnd: HWND, value: &str) {
        let text = wide(value);
        SetWindowTextW(hwnd, text.as_ptr());
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

    fn install(root: &Path, add_path: bool, make_default: bool) -> Result<String, String> {
        if RUNTIME_PAYLOAD.is_empty() || LAUNCHER_PAYLOAD.is_empty() {
            return Err("This setup was built without embedded runtime payloads.".to_owned());
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
        if add_path {
            add_user_path(&root.join("bin"))?;
        }
        Ok(format!(
            "Installed Seloryn {RUNTIME_VERSION}. SHA-256 {}. Restart shells to load PATH changes.",
            installed.sha256
        ))
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
                SendMessageTimeoutW(
                    HWND_BROADCAST,
                    WM_SETTINGCHANGE,
                    0,
                    environment.as_ptr() as _,
                    SMTO_ABORTIFHUNG,
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
