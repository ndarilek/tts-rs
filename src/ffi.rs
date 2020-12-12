use libc::c_char;
use std::{
    cell::RefCell,
    ffi::{CString, NulError},
    ptr,
};

use crate::{Backends, Features, TTS};

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

/// Creates a new TTS object with the specified backend and returns a pointer to it.
/// If an error occured, a null pointer is returned,
/// Call `tts_get_error()` for more information about the specific error.
#[no_mangle]
pub extern "C" fn tts_new(backend: Backends) -> *mut TTS {
    match TTS::new(backend) {
        Ok(tts) => Box::into_raw(Box::new(tts)),
        Err(e) => {
            set_last_error(e.to_string()).unwrap();
            ptr::null_mut()
        }
    }
}

/// Creates a new TTS object with the default backend and returns a pointer to it.
/// If an error occured, a null pointer is returned,
/// Call `tts_get_error()` for more information about the specific error.
#[no_mangle]
pub extern "C" fn tts_default() -> *mut TTS {
    match TTS::default() {
        Ok(tts) => Box::into_raw(Box::new(tts)),
        Err(e) => {
            set_last_error(e.to_string()).unwrap();
            ptr::null_mut()
        }
    }
}

/// Frees the memory associated with a TTS object.
/// If `tts` is a null pointer, this function does nothing.
#[no_mangle]
pub extern "C" fn tts_free(tts: *mut TTS) {
    if tts.is_null() {
        return;
    }
    unsafe {
        Box::from_raw(tts); // Goes out of scope and is dropped
    }
}

/// Returns the features supported by this TTS engine.
/// tts must be a valid pointer to a TTS object.
#[no_mangle]
pub extern "C" fn tts_supported_features(tts: *mut TTS) -> Features {
    unsafe { tts.as_ref().unwrap().supported_features() }
}
