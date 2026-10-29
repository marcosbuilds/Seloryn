//! Process-level cancellation shared by the CLI and bounded job worker.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
mod platform {
    use std::io;

    const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;
    const SIG_ERR: usize = usize::MAX;

    unsafe extern "C" {
        fn signal(signal: i32, handler: Option<unsafe extern "C" fn(i32)>) -> usize;
    }

    unsafe extern "C" fn handler(_: i32) {
        super::REQUESTED.store(true, super::Ordering::Relaxed);
    }

    pub(super) fn install() -> io::Result<()> {
        let (int_result, term_result) = unsafe {
            (
                signal(SIGINT, Some(handler)),
                signal(SIGTERM, Some(handler)),
            )
        };
        if int_result == SIG_ERR || term_result == SIG_ERR {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(windows)]
mod platform {
    use std::io;

    const CTRL_C_EVENT: u32 = 0;
    const CTRL_BREAK_EVENT: u32 = 1;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(handler: Option<extern "system" fn(u32) -> i32>, add: i32) -> i32;
    }

    extern "system" fn handler(event: u32) -> i32 {
        if matches!(event, CTRL_C_EVENT | CTRL_BREAK_EVENT) {
            super::REQUESTED.store(true, super::Ordering::Relaxed);
            return 1;
        }
        0
    }

    pub(super) fn install() -> io::Result<()> {
        let installed = unsafe { SetConsoleCtrlHandler(Some(handler), 1) };
        if installed == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use std::io;

    pub(super) fn install() -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn install() -> io::Result<()> {
    REQUESTED.store(false, Ordering::Relaxed);
    platform::install()
}

pub(crate) fn requested() -> bool {
    REQUESTED.load(Ordering::Relaxed)
}
