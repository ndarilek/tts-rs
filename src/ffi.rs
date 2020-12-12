use libc::c_char;
use std::{
    cell::RefCell,
    ffi::{CString, NulError},
    ptr,
};

thread_local! {
    /// Stores the last reported error, so it can be retrieved at will from C
    static LAST_ERROR: RefCell<Option<CString>> = RefCell::new(None);
}

fn set_last_error<E: Into<Vec<u8>>>(err: E) -> Result<(), NulError> {
    LAST_ERROR.with(|last| {
        *last.borrow_mut() = Some(CString::new(err)?);
        Ok(())
    })
}

/// Get the last reported error as a const C string (const char*)
/// This string will be valid until at least the next call to `tts_get_error`.
/// It is never called internally by the library.
#[no_mangle]
pub extern "C" fn tts_get_error() -> *const c_char {
    LAST_ERROR.with(|err| match &*err.borrow() {
        Some(e) => e.as_ptr(),
        None => ptr::null(),
    })
}

/// Deallocate the last reported error (if any).
#[no_mangle]
pub extern "C" fn tts_clear_error() {
    LAST_ERROR.with(|err| {
        *err.borrow_mut() = None;
    });
}
