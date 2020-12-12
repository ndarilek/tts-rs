use libc::c_char;
use std::{
    cell::RefCell,
    ffi::{CStr, CString, NulError},
    ptr,
};

use crate::{Backends, Features, UtteranceId, TTS};

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

/// Get the last reported error as a const C string.
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

/// Create a new `TTS` instance with the specified backend.
/// If an error occures, returns a null pointer,
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

/// Create a new TTS object with the default backend.
/// If an error occures, returns a null pointer,
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

/// Free the memory associated with a TTS object.
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
/// `tts` must be a valid pointer to a TTS object.
#[no_mangle]
pub extern "C" fn tts_supported_features(tts: *mut TTS) -> Features {
    unsafe { tts.as_ref().unwrap().supported_features() }
}

/// Speaks the specified text, optionally interrupting current speech.
/// If `utterance` is not NULL, , fills it with a pointer to the returned UtteranceId (or NULL if
/// the backend doesn't provide one).
/// Returns true on success, false on error or if `tts` is NULL.
#[no_mangle]
pub extern "C" fn tts_speak(
    tts: *mut TTS,
    text: *const c_char,
    interrupt: bool,
    utterance: *mut *mut UtteranceId,
) -> bool {
    if tts.is_null() {
        return true;
    }
    unsafe {
        let text = CStr::from_ptr(text).to_string_lossy().into_owned();
        match tts.as_mut().unwrap().speak(text, interrupt) {
            Ok(u) => {
                if !utterance.is_null() {
                    *utterance = match u {
                        Some(u) => Box::into_raw(Box::new(u)),
                        None => ptr::null_mut(),
                    };
                }
                return true;
            }
            Err(e) => {
                set_last_error(e.to_string()).unwrap();
                return false;
            }
        }
    }
}

/// Free the memory associated with an `UtteranceId`.
/// Does nothing if `utterance` is NULL.
#[no_mangle]
pub extern "C" fn tts_free_utterance(utterance: *mut UtteranceId) {
    if utterance.is_null() {
        return;
    }
    unsafe {
        Box::from_raw(utterance);
    }
}
