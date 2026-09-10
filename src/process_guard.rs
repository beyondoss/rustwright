//! Track locally launched Chromium PIDs so abort, SIGTERM, and SIGINT can
//! still reap the process group after `Drop` is skipped (`panic = "abort"`).

use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const MAX_TRACKED: usize = 32;
const EMPTY_PID: u32 = 0;

static PIDS: [AtomicU32; MAX_TRACKED] = [const { AtomicU32::new(EMPTY_PID) }; MAX_TRACKED];
static INSTALLED: AtomicBool = AtomicBool::new(false);

pub(crate) fn register(child: &Child) {
    install();
    let pid = child.id();
    if pid == EMPTY_PID {
        return;
    }
    for slot in &PIDS {
        if slot.load(Ordering::SeqCst) == pid {
            return;
        }
    }
    for slot in &PIDS {
        if slot
            .compare_exchange(EMPTY_PID, pid, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return;
        }
    }
    eprintln!("rustwright: Chromium pid {pid} could not be tracked for abort/signal reaping");
}

pub(crate) fn unregister(pid: u32) {
    if pid == EMPTY_PID {
        return;
    }
    for slot in &PIDS {
        let _ = slot.compare_exchange(pid, EMPTY_PID, Ordering::SeqCst, Ordering::SeqCst);
    }
}

pub(crate) fn kill_and_unregister(child: &mut Child) {
    let pid = child.id();
    let _ = child.kill();
    let _ = child.wait();
    unregister(pid);
}

pub(crate) fn wait_and_unregister(child: &mut Child) -> std::io::Result<std::process::ExitStatus> {
    let pid = child.id();
    let status = child.wait();
    unregister(pid);
    status
}

fn install() {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }

    // Panic hooks run even for panics that `catch_unwind` recovers. Killing
    // Chromium here would defeat actor isolation in unwind builds. Abort
    // skips Drop, so the hook is only installed when that is the strategy.
    #[cfg(panic = "abort")]
    {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            kill_all_registered();
            previous(info);
        }));
    }

    #[cfg(unix)]
    install_unix_signal_handlers();
    #[cfg(windows)]
    install_windows_ctrl_handler();
}

fn kill_all_registered() {
    for slot in &PIDS {
        let pid = slot.swap(EMPTY_PID, Ordering::SeqCst);
        if pid != EMPTY_PID {
            kill_pid(pid);
        }
    }
}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    // SAFETY: kill(2) is async-signal-safe. `pid` is a previously spawned
    // Chromium leader; `-pid` targets the process group created at spawn.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
        libc::kill(-pid, libc::SIGKILL);
    }
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    const PROCESS_TERMINATE: u32 = 0x0001;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn TerminateProcess(handle: *mut std::ffi::c_void, exit_code: u32) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    // SAFETY: PROCESS_TERMINATE on a pid this process spawned; handles are
    // closed. Called from a panic hook or console handler, not a Unix signal.
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            let _ = TerminateProcess(handle, 1);
            let _ = CloseHandle(handle);
        }
    }
}

#[cfg(unix)]
fn install_unix_signal_handlers() {
    extern "C" fn handle_signal(sig: libc::c_int) {
        kill_all_registered();
        // SAFETY: restoring the default disposition and re-raising is
        // async-signal-safe and lets the process exit without running Drop.
        unsafe {
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }

    // SAFETY: handler only calls async-signal-safe functions (atomics, kill,
    // signal, raise). Installed once per process.
    unsafe {
        libc::signal(libc::SIGTERM, handle_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, handle_signal as *const () as libc::sighandler_t);
    }
}

#[cfg(windows)]
fn install_windows_ctrl_handler() {
    const CTRL_C_EVENT: u32 = 0;
    const CTRL_BREAK_EVENT: u32 = 1;
    const CTRL_CLOSE_EVENT: u32 = 2;
    const TRUE: i32 = 1;
    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }
    unsafe extern "system" fn handle_ctrl(control: u32) -> i32 {
        match control {
            CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT => {
                kill_all_registered();
                0
            }
            _ => 0,
        }
    }
    // SAFETY: handler only terminates tracked Chromium PIDs; returning 0
    // lets the default handler end this process.
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(handle_ctrl), TRUE);
    }
}
