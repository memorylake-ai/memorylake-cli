//! Terminal state helpers for interactive prompts.
//!
//! `dialoguer` / `console` may hide the cursor and disable ECHO. Ctrl-C must
//! restore those settings or the user's shell is left unusable.

use std::io::Write;
use std::sync::Once;

use anyhow::{Result, bail};
use dialoguer::console::Term;

/// Prepare stdin for interactive prompts and install Ctrl-C restoration.
///
/// Call before any `dialoguer` interaction. Safe to call repeatedly.
pub fn prepare() -> Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        bail!("interactive prompts require a TTY; pass flags for non-interactive use");
    }
    // Capture cooked/echo settings before dialoguer disables ECHO.
    termios::snapshot();
    install_ctrlc_restore();
    Ok(())
}

/// RAII guard that restores termios and the cursor when dropped.
///
/// Hold this across any `dialoguer` call so normal returns and errors clean up.
/// SIGINT is handled separately (see [`install_ctrlc_restore`]) because
/// `process::exit` skips `Drop`.
#[derive(Debug, Default)]
pub struct TerminalGuard;

impl TerminalGuard {
    /// Create a guard. Call [`prepare`] first so termios is snapshotted.
    pub fn new() -> Self {
        Self
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore(false);
    }
}

/// Map a `dialoguer` error, restoring the terminal when the user cancelled.
pub fn map_prompt_error(err: dialoguer::Error) -> anyhow::Error {
    let cancelled = is_interrupt(&err);
    restore(cancelled);
    if cancelled {
        anyhow::anyhow!("cancelled")
    } else {
        err.into()
    }
}

fn is_interrupt(err: &dialoguer::Error) -> bool {
    match err {
        dialoguer::Error::IO(io_err) => io_err.kind() == std::io::ErrorKind::Interrupted,
    }
}

fn install_ctrlc_restore() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = ctrlc::set_handler(|| {
            restore(true);
            // 130 = 128 + SIGINT, conventional shell exit status.
            std::process::exit(130);
        });
    });
}

fn restore(after_cancel: bool) {
    termios::restore();
    let _ = Term::stderr().show_cursor();
    let _ = Term::stdout().show_cursor();
    if after_cancel {
        // Shell prompt must start on a fresh line; Enter may not have echoed.
        let mut stderr = Term::stderr();
        let _ = writeln!(stderr);
        let _ = stderr.flush();
    }
}

#[cfg(unix)]
mod termios {
    use std::mem::MaybeUninit;
    use std::sync::Mutex;

    static SAVED: Mutex<Option<libc::termios>> = Mutex::new(None);

    pub fn snapshot() {
        let mut guard = match SAVED.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        if guard.is_some() {
            return;
        }
        let mut termios = MaybeUninit::uninit();
        let rc = unsafe { libc::tcgetattr(libc::STDIN_FILENO, termios.as_mut_ptr()) };
        if rc == 0 {
            *guard = Some(unsafe { termios.assume_init() });
        }
    }

    pub fn restore() {
        let guard = match SAVED.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        if let Some(termios) = guard.as_ref() {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, termios);
            }
        }
    }
}

#[cfg(not(unix))]
mod termios {
    pub fn snapshot() {}
    pub fn restore() {}
}
