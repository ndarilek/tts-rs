use libc::{c_char, c_float};
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
pub unsafe extern "C" fn tts_free(tts: *mut TTS) {
    if tts.is_null() {
        return;
    }
    Box::from_raw(tts); // Goes out of scope and is dropped
}

/// Returns the features supported by this TTS engine.
/// `tts` must be a valid pointer to a TTS object.
#[no_mangle]
pub unsafe extern "C" fn tts_supported_features(tts: *mut TTS) -> Features {
    tts.as_ref().unwrap().supported_features()
}

/// Speaks the specified text, optionally interrupting current speech.
/// If `utterance` is not NULL, , fills it with a pointer to the returned UtteranceId (or NULL if
/// the backend doesn't provide one).
/// Returns true on success, false on error or if `tts` is NULL.
#[no_mangle]
pub unsafe extern "C" fn tts_speak(
    tts: *mut TTS,
    text: *const c_char,
    interrupt: bool,
    utterance: *mut *mut UtteranceId,
) -> bool {
    if tts.is_null() {
        return true;
    }
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

/// Free the memory associated with an `UtteranceId`.
/// Does nothing if `utterance` is NULL.
#[no_mangle]
pub unsafe extern "C" fn tts_free_utterance(utterance: *mut UtteranceId) {
    if utterance.is_null() {
        return;
    }
    Box::from_raw(utterance);
}
/// Stops current speech.
/// Returns true on success, false on error or if `tts` is NULL.
#[no_mangle]
pub unsafe extern "C" fn tts_stop(tts: *mut TTS) -> bool {
    if tts.is_null() {
        return false;
    }
    match tts.as_mut().unwrap().stop() {
        Ok(_) => true,
        Err(e) => {
            set_last_error(e.to_string()).unwrap();
            false
        }
    }
}

/// Returns the minimum rate for this speech synthesizer.
/// `tts` must be a valid pointer to a TTS object.
#[no_mangle]
pub unsafe extern "C" fn tts_min_rate(tts: *mut TTS) -> c_float {
    tts.as_ref().unwrap().min_rate()
}

/// Returns the maximum rate for this speech synthesizer.
/// `tts` must be a valid pointer to a TTS object.
#[no_mangle]
pub unsafe extern "C" fn tts_max_rate(tts: *mut TTS) -> c_float {
    tts.as_ref().unwrap().max_rate()
}

/// Returns the normal rate for this speech synthesizer.
/// `tts` must be a valid pointer to a TTS object.
#[no_mangle]
pub unsafe extern "C" fn tts_normal_rate(tts: *mut TTS) -> c_float {
    tts.as_ref().unwrap().normal_rate()
}

/// Gets the current speech rate.
/// Returns true on success, false on error (likely that the backend doesn't support rate changes)
/// or if `tts` is NULL.
/// Does nothing if `rate` is NULL.
#[no_mangle]
pub unsafe extern "C" fn tts_get_rate(tts: *mut TTS, rate: *mut c_float) -> bool {
    if tts.is_null() {
        return false;
    }
    match tts.as_ref().unwrap().get_rate() {
        Ok(r) => {
            if !rate.is_null() {
                *rate = r;
            }
            true
        }
        Err(e) => {
            set_last_error(e.to_string()).unwrap();
            false
        }
    }
}

/// Sets the desired speech rate.
/// Returns true on success, false on error (likely that the backend doesn't support rate changes)
/// or if `tts` is NULL.
#[no_mangle]
pub unsafe extern "C" fn tts_set_rate(tts: *mut TTS, rate: c_float) -> bool {
    if tts.is_null() {
        return false;
    }
    match tts.as_mut().unwrap().set_rate(rate) {
        Ok(_) => true,
        Err(e) => {
            set_last_error(e.to_string()).unwrap();
            false
        }
    }
}

/// Returns the minimum pitch for this speech synthesizer.
/// `tts` must be a valid pointer to a TTS object.
#[no_mangle]
pub unsafe extern "C" fn tts_min_pitch(tts: *mut TTS) -> c_float {
    tts.as_ref().unwrap().min_pitch()
}

/// Returns the maximum pitch for this speech synthesizer.
/// `tts` must be a valid pointer to a TTS object.
#[no_mangle]
pub unsafe extern "C" fn tts_max_pitch(tts: *mut TTS) -> c_float {
    tts.as_ref().unwrap().max_pitch()
}

/// Returns the normal pitch for this speech synthesizer.
/// `tts` must be a valid pointer to a TTS object.
#[no_mangle]
pub unsafe extern "C" fn tts_normal_pitch(tts: *mut TTS) -> c_float {
    tts.as_ref().unwrap().normal_pitch()
}

/// Gets the current speech pitch.
/// Returns true on success, false on error (likely that the backend doesn't support pitch changes)
/// or if `tts` is NULL.
/// Does nothing if `pitch` is NULL.
#[no_mangle]
pub unsafe extern "C" fn tts_get_pitch(tts: *mut TTS, pitch: *mut c_float) -> bool {
    if tts.is_null() {
        return false;
    }
    match tts.as_ref().unwrap().get_pitch() {
        Ok(r) => {
            if !pitch.is_null() {
                *pitch = r;
            }
            true
        }
        Err(e) => {
            set_last_error(e.to_string()).unwrap();
            false
        }
    }
}

/// Sets the desired speech pitch.
/// Returns true on success, false on error (likely that the backend doesn't support pitch changes)
/// or if `tts` is NULL.
#[no_mangle]
pub unsafe extern "C" fn tts_set_pitch(tts: *mut TTS, pitch: c_float) -> bool {
    if tts.is_null() {
        return false;
    }
    match tts.as_mut().unwrap().set_pitch(pitch) {
        Ok(_) => true,
        Err(e) => {
            set_last_error(e.to_string()).unwrap();
            false
        }
    }
}

/// Returns the minimum volume for this speech synthesizer.
/// `tts` must be a valid pointer to a TTS object.
#[no_mangle]
pub unsafe extern "C" fn tts_min_volume(tts: *mut TTS) -> c_float {
    tts.as_ref().unwrap().min_volume()
}

/// Returns the maximum volume for this speech synthesizer.
/// `tts` must be a valid pointer to a TTS object.
#[no_mangle]
pub unsafe extern "C" fn tts_max_volume(tts: *mut TTS) -> c_float {
    tts.as_ref().unwrap().max_volume()
}

/// Returns the normal volume for this speech synthesizer.
/// `tts` must be a valid pointer to a TTS object.
#[no_mangle]
pub unsafe extern "C" fn tts_normal_volume(tts: *mut TTS) -> c_float {
    tts.as_ref().unwrap().normal_volume()
}

/// Gets the current speech volume.
/// Returns true on success, false on error (likely that the backend doesn't support volume changes)
/// or if `tts` is NULL.
/// Does nothing if `volume` is NULL.
#[no_mangle]
pub unsafe extern "C" fn tts_get_volume(tts: *mut TTS, volume: *mut c_float) -> bool {
    if tts.is_null() {
        return false;
    }
    match tts.as_ref().unwrap().get_volume() {
        Ok(r) => {
            if !volume.is_null() {
                *volume = r;
            }
            true
        }
        Err(e) => {
            set_last_error(e.to_string()).unwrap();
            false
        }
    }
}

/// Sets the desired speech volume.
/// Returns true on success, false on error (likely that the backend doesn't support volume changes)
/// or if `tts` is NULL.
#[no_mangle]
pub unsafe extern "C" fn tts_set_volume(tts: *mut TTS, volume: c_float) -> bool {
    if tts.is_null() {
        return false;
    }
    match tts.as_mut().unwrap().set_volume(volume) {
        Ok(_) => true,
        Err(e) => {
            set_last_error(e.to_string()).unwrap();
            false
        }
    }
}
