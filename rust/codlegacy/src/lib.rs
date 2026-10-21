//! CoD Legacy's Rust core, linked into CodLegacy.exe as a static library.
//!
//! Every export is `extern "C"` with plain C types, declared in
//! `src/client/rust/codlegacy.h`. Strings handed out are static and
//! NUL-terminated; strings handed in are borrowed for the call only.

use std::ffi::{c_char, CStr};

/// The library's name and version, for the client's log and the UI.
#[no_mangle]
pub extern "C" fn codlegacy_version() -> *const c_char {
    static VERSION: &CStr = c"CoD Legacy Rust core 0.1.0";
    VERSION.as_ptr()
}

/// Black Ops II's sprint speed scale (player_sprintSpeedScale, 1.5): the
/// first gameplay value the client will take from the Rust side.
#[no_mangle]
pub extern "C" fn codlegacy_bo2_sprint_speed_scale() -> f32 {
    1.5
}

/// Writes `a + b` (saturating) to `*out` and returns true; false on a null
/// `out` (a check that values cross the boundary both ways).
#[no_mangle]
pub extern "C" fn codlegacy_add(a: i32, b: i32, out: *mut i32) -> bool {
    if out.is_null() {
        return false;
    }
    // SAFETY: non-null, and the caller passes a valid, aligned i32.
    unsafe { *out = a.saturating_add(b) };
    true
}
