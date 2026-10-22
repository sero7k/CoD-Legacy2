//! The engine as the Rust side sees it: a small table of callbacks the client
//! hands over once at start-up ([`codlegacy_init`]). Everything the modules
//! need from the game (print a line, run a console command, read a dvar) goes
//! through here, so they stay plain Rust and can be tested without the game.

use std::ffi::{c_char, CStr, CString};
use std::sync::Mutex;

/// Callbacks filled in by the client. Strings handed in are borrowed for the
/// call only.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostApi {
    /// Prints one line to the game's console and log.
    pub log: extern "C" fn(line: *const c_char),
    /// Queues a console command (no trailing newline needed).
    pub exec: extern "C" fn(command: *const c_char),
    /// Copies a dvar's value as text into `out` (`cap` bytes, always
    /// NUL-terminated). False when the dvar does not exist.
    pub dvar_string: extern "C" fn(name: *const c_char, out: *mut c_char, cap: usize) -> bool,
}

static HOST: Mutex<Option<HostApi>> = Mutex::new(None);

fn host() -> Option<HostApi> {
    *HOST.lock().unwrap_or_else(|e| e.into_inner())
}

/// Hands the engine callbacks to the Rust core. Returns false for a null table.
#[no_mangle]
pub extern "C" fn codlegacy_init(api: *const HostApi) -> bool {
    // SAFETY: a non-null `api` points to a table that is valid for the call;
    // it is copied, not kept.
    let Some(api) = (unsafe { api.as_ref() }) else { return false };
    *HOST.lock().unwrap_or_else(|e| e.into_inner()) = Some(*api);
    true
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', " ")).unwrap_or_default()
}

/// Prints a line (prefixed with the module's name) to the game's console.
pub fn log(module: &str, line: &str) {
    if let Some(h) = host() {
        let text = cstring(&format!("[{module}] {line}\n"));
        (h.log)(text.as_ptr());
    }
}

/// Queues a console command.
pub fn exec(command: &str) {
    if let Some(h) = host() {
        let c = cstring(command);
        (h.exec)(c.as_ptr());
    }
}

/// A dvar's value as text; `None` when the dvar does not exist or there is no
/// engine.
pub fn dvar(name: &str) -> Option<String> {
    let h = host()?;
    let n = cstring(name);
    let mut buf = [0 as c_char; 256];
    if !(h.dvar_string)(n.as_ptr(), buf.as_mut_ptr(), buf.len()) {
        return None;
    }
    buf[buf.len() - 1] = 0;
    // SAFETY: `buf` is NUL-terminated (forced above) and lives for the read.
    Some(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as M;

    static LOG: M<Vec<String>> = M::new(Vec::new());
    static SERIAL: M<()> = M::new(());

    extern "C" fn fake_log(line: *const c_char) {
        LOG.lock().unwrap().push(unsafe { CStr::from_ptr(line) }.to_string_lossy().into_owned());
    }
    extern "C" fn fake_exec(_: *const c_char) {}
    extern "C" fn fake_dvar(name: *const c_char, out: *mut c_char, cap: usize) -> bool {
        let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned();
        if name != "name" {
            return false;
        }
        let v = b"sero\0";
        assert!(v.len() <= cap);
        unsafe { std::ptr::copy_nonoverlapping(v.as_ptr().cast(), out, v.len()) };
        true
    }

    #[test]
    fn calls_go_through_the_table_and_a_null_table_is_refused() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!codlegacy_init(std::ptr::null()));
        let api = HostApi { log: fake_log, exec: fake_exec, dvar_string: fake_dvar };
        assert!(codlegacy_init(&api));
        log("music", "hello");
        assert_eq!(LOG.lock().unwrap().last().map(String::as_str), Some("[music] hello\n"));
        assert_eq!(dvar("name").as_deref(), Some("sero"));
        assert_eq!(dvar("nothing"), None);
    }
}
