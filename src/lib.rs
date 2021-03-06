/*!
 * a Text-To-Speech (TTS) library providing high-level interfaces to a variety of backends.
 * Currently supported backends are:
 * * Windows
 *   * Screen readers/SAPI via Tolk (requires `tolk` Cargo feature)
 *   * WinRT
 * * Linux via [Speech Dispatcher](https://freebsoft.org/speechd)
 * * MacOS/iOS
 *   * AppKit on MacOS 10.13 and below
 *   * AVFoundation on MacOS 10.14 and above, and iOS
 * * Android
 * * WebAssembly
 */

use std::boxed::Box;
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::ffi::CStr;
use std::sync::Mutex;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use cocoa_foundation::base::id;
use dyn_clonable::*;
use lazy_static::lazy_static;
#[cfg(target_os = "macos")]
use libc::c_char;
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
use thiserror::Error;

mod backends;
#[cfg(feature = "ffi")]
pub mod ffi;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub enum Backends {
    #[cfg(target_os = "linux")]
    SpeechDispatcher,
    #[cfg(target_arch = "wasm32")]
    Web,
    #[cfg(all(windows, feature = "tolk"))]
    Tolk,
    #[cfg(windows)]
    WinRT,
    #[cfg(target_os = "macos")]
    AppKit,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    AvFoundation,
    #[cfg(target_os = "android")]
    Android,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BackendId {
    #[cfg(target_os = "linux")]
    SpeechDispatcher(u64),
    #[cfg(target_arch = "wasm32")]
    Web(u64),
    #[cfg(windows)]
    WinRT(u64),
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    AvFoundation(u64),
    #[cfg(target_os = "android")]
    Android(u64),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UtteranceId {
    #[cfg(target_os = "linux")]
    SpeechDispatcher(u64),
    #[cfg(target_arch = "wasm32")]
    Web(u64),
    #[cfg(windows)]
    WinRT(u64),
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    AvFoundation(id),
    #[cfg(target_os = "android")]
    Android(u64),
}

unsafe impl Send for UtteranceId {}

unsafe impl Sync for UtteranceId {}

#[repr(C)]
pub struct Features {
    pub stop: bool,
    pub rate: bool,
    pub pitch: bool,
    pub volume: bool,
    pub is_speaking: bool,
    pub utterance_callbacks: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            stop: false,
            rate: false,
            pitch: false,
            volume: false,
            is_speaking: false,
            utterance_callbacks: false,
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Value not received")]
    NoneError,
    #[error("Operation failed")]
    OperationFailed,
    #[cfg(target_arch = "wasm32")]
    #[error("JavaScript error: [0])]")]
    JavaScriptError(wasm_bindgen::JsValue),
    #[cfg(windows)]
    #[error("WinRT error")]
    WinRT(windows::Error),
    #[error("Unsupported feature")]
    UnsupportedFeature,
    #[error("Out of range")]
    OutOfRange,
    #[cfg(target_os = "android")]
    #[error("JNI error: [0])]")]
    JNI(#[from] jni::errors::Error),
}

#[clonable]
trait Backend: Clone {
    fn id(&self) -> Option<BackendId>;
    fn supported_features(&self) -> Features;
    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error>;
    fn stop(&mut self) -> Result<(), Error>;
    fn min_rate(&self) -> f32;
    fn max_rate(&self) -> f32;
    fn normal_rate(&self) -> f32;
    fn get_rate(&self) -> Result<f32, Error>;
    fn set_rate(&mut self, rate: f32) -> Result<(), Error>;
    fn min_pitch(&self) -> f32;
    fn max_pitch(&self) -> f32;
    fn normal_pitch(&self) -> f32;
    fn get_pitch(&self) -> Result<f32, Error>;
    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error>;
    fn min_volume(&self) -> f32;
    fn max_volume(&self) -> f32;
    fn normal_volume(&self) -> f32;
    fn get_volume(&self) -> Result<f32, Error>;
    fn set_volume(&mut self, volume: f32) -> Result<(), Error>;
    fn is_speaking(&self) -> Result<bool, Error>;
}

#[derive(Default)]
struct Callbacks {
    utterance_begin: Option<Box<dyn FnMut(UtteranceId)>>,
    utterance_end: Option<Box<dyn FnMut(UtteranceId)>>,
    utterance_stop: Option<Box<dyn FnMut(UtteranceId)>>,
}

unsafe impl Send for Callbacks {}

unsafe impl Sync for Callbacks {}

lazy_static! {
    static ref CALLBACKS: Mutex<HashMap<BackendId, Callbacks>> = {
        let m: HashMap<BackendId, Callbacks> = HashMap::new();
        Mutex::new(m)
    };
}

#[derive(Clone)]
pub struct TTS(Box<dyn Backend>);

unsafe impl Send for TTS {}

unsafe impl Sync for TTS {}

impl TTS {
    /**
     * Create a new `TTS` instance with the specified backend.
     */
    pub fn new(backend: Backends) -> Result<TTS, Error> {
        let backend = match backend {
            #[cfg(target_os = "linux")]
            Backends::SpeechDispatcher => Ok(TTS(Box::new(backends::SpeechDispatcher::new()))),
            #[cfg(target_arch = "wasm32")]
            Backends::Web => {
                let tts = backends::Web::new()?;
                Ok(TTS(Box::new(tts)))
            }
            #[cfg(all(windows, feature = "tolk"))]
            Backends::Tolk => {
                let tts = backends::Tolk::new();
                if let Some(tts) = tts {
                    Ok(TTS(Box::new(tts)))
                } else {
                    Err(Error::NoneError)
                }
            }
            #[cfg(windows)]
            Backends::WinRT => {
                let tts = backends::WinRT::new()?;
                Ok(TTS(Box::new(tts)))
            }
            #[cfg(target_os = "macos")]
            Backends::AppKit => Ok(TTS(Box::new(backends::AppKit::new()))),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Backends::AvFoundation => Ok(TTS(Box::new(backends::AvFoundation::new()))),
            #[cfg(target_os = "android")]
            Backends::Android => {
                let tts = backends::Android::new()?;
                Ok(TTS(Box::new(tts)))
            }
        };
        if let Ok(backend) = backend {
            if let Some(id) = backend.0.id() {
                let mut callbacks = CALLBACKS.lock().unwrap();
                callbacks.insert(id, Callbacks::default());
            }
            Ok(backend)
        } else {
            backend
        }
    }

    pub fn default() -> Result<TTS, Error> {
        #[cfg(target_os = "linux")]
        let tts = TTS::new(Backends::SpeechDispatcher);
        #[cfg(all(windows, feature = "tolk"))]
        let tts = if let Ok(tts) = TTS::new(Backends::Tolk) {
            Ok(tts)
        } else {
            TTS::new(Backends::WinRT)
        };
        #[cfg(all(windows, not(feature = "tolk")))]
        let tts = TTS::new(Backends::WinRT);
        #[cfg(target_arch = "wasm32")]
        let tts = TTS::new(Backends::Web);
        #[cfg(target_os = "macos")]
        let tts = unsafe {
            // Needed because the Rust NSProcessInfo structs report bogus values, and I don't want to pull in a full bindgen stack.
            let pi: id = msg_send![class!(NSProcessInfo), new];
            let version: id = msg_send![pi, operatingSystemVersionString];
            let str: *const c_char = msg_send![version, UTF8String];
            let str = CStr::from_ptr(str);
            let str = str.to_string_lossy();
            let version: Vec<&str> = str.split(" ").collect();
            let version = version[1];
            let version_parts: Vec<&str> = version.split(".").collect();
            let major_version: i8 = version_parts[0].parse().unwrap();
            let minor_version: i8 = version_parts[1].parse().unwrap();
            if major_version >= 11 || minor_version >= 14 {
                TTS::new(Backends::AvFoundation)
            } else {
                TTS::new(Backends::AppKit)
            }
        };
        #[cfg(target_os = "ios")]
        let tts = TTS::new(Backends::AvFoundation);
        #[cfg(target_os = "android")]
        let tts = TTS::new(Backends::Android);
        tts
    }

    /**
     * Returns the features supported by this TTS engine
     */
    pub fn supported_features(&self) -> Features {
        self.0.supported_features()
    }

    /**
     * Speaks the specified text, optionally interrupting current speech.
     */
    pub fn speak<S: Into<String>>(
        &mut self,
        text: S,
        interrupt: bool,
    ) -> Result<Option<UtteranceId>, Error> {
        self.0.speak(text.into().as_str(), interrupt)
    }

    /**
     * Stops current speech.
     */
    pub fn stop(&mut self) -> Result<&Self, Error> {
        let Features { stop, .. } = self.supported_features();
        if stop {
            self.0.stop()?;
            Ok(self)
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /**
     * Returns the minimum rate for this speech synthesizer.
     */
    pub fn min_rate(&self) -> f32 {
        self.0.min_rate()
    }

    /**
     * Returns the maximum rate for this speech synthesizer.
     */
    pub fn max_rate(&self) -> f32 {
        self.0.max_rate()
    }

    /**
     * Returns the normal rate for this speech synthesizer.
     */
    pub fn normal_rate(&self) -> f32 {
        self.0.normal_rate()
    }

    /**
     * Gets the current speech rate.
     */
    pub fn get_rate(&self) -> Result<f32, Error> {
        let Features { rate, .. } = self.supported_features();
        if rate {
            self.0.get_rate()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /**
     * Sets the desired speech rate.
     */
    pub fn set_rate(&mut self, rate: f32) -> Result<&Self, Error> {
        let Features {
            rate: rate_feature, ..
        } = self.supported_features();
        if rate_feature {
            if rate < self.0.min_rate() || rate > self.0.max_rate() {
                Err(Error::OutOfRange)
            } else {
                self.0.set_rate(rate)?;
                Ok(self)
            }
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /**
     * Returns the minimum pitch for this speech synthesizer.
     */
    pub fn min_pitch(&self) -> f32 {
        self.0.min_pitch()
    }

    /**
     * Returns the maximum pitch for this speech synthesizer.
     */
    pub fn max_pitch(&self) -> f32 {
        self.0.max_pitch()
    }

    /**
     * Returns the normal pitch for this speech synthesizer.
     */
    pub fn normal_pitch(&self) -> f32 {
        self.0.normal_pitch()
    }

    /**
     * Gets the current speech pitch.
     */
    pub fn get_pitch(&self) -> Result<f32, Error> {
        let Features { pitch, .. } = self.supported_features();
        if pitch {
            self.0.get_pitch()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /**
     * Sets the desired speech pitch.
     */
    pub fn set_pitch(&mut self, pitch: f32) -> Result<&Self, Error> {
        let Features {
            pitch: pitch_feature,
            ..
        } = self.supported_features();
        if pitch_feature {
            if pitch < self.0.min_pitch() || pitch > self.0.max_pitch() {
                Err(Error::OutOfRange)
            } else {
                self.0.set_pitch(pitch)?;
                Ok(self)
            }
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /**
     * Returns the minimum volume for this speech synthesizer.
     */
    pub fn min_volume(&self) -> f32 {
        self.0.min_volume()
    }

    /**
     * Returns the maximum volume for this speech synthesizer.
     */
    pub fn max_volume(&self) -> f32 {
        self.0.max_volume()
    }

    /**
     * Returns the normal volume for this speech synthesizer.
     */
    pub fn normal_volume(&self) -> f32 {
        self.0.normal_volume()
    }

    /**
     * Gets the current speech volume.
     */
    pub fn get_volume(&self) -> Result<f32, Error> {
        let Features { volume, .. } = self.supported_features();
        if volume {
            self.0.get_volume()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /**
     * Sets the desired speech volume.
     */
    pub fn set_volume(&mut self, volume: f32) -> Result<&Self, Error> {
        let Features {
            volume: volume_feature,
            ..
        } = self.supported_features();
        if volume_feature {
            if volume < self.0.min_volume() || volume > self.0.max_volume() {
                Err(Error::OutOfRange)
            } else {
                self.0.set_volume(volume)?;
                Ok(self)
            }
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /**
     * Returns whether this speech synthesizer is speaking.
     */
    pub fn is_speaking(&self) -> Result<bool, Error> {
        let Features { is_speaking, .. } = self.supported_features();
        if is_speaking {
            self.0.is_speaking()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /**
     * Called when this speech synthesizer begins speaking an utterance.
     */
    pub fn on_utterance_begin(
        &self,
        callback: Option<Box<dyn FnMut(UtteranceId)>>,
    ) -> Result<(), Error> {
        let Features {
            utterance_callbacks,
            ..
        } = self.supported_features();
        if utterance_callbacks {
            let mut callbacks = CALLBACKS.lock().unwrap();
            let id = self.0.id().unwrap();
            let mut callbacks = callbacks.get_mut(&id).unwrap();
            callbacks.utterance_begin = callback;
            Ok(())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /**
     * Called when this speech synthesizer finishes speaking an utterance.
     */
    pub fn on_utterance_end(
        &self,
        callback: Option<Box<dyn FnMut(UtteranceId)>>,
    ) -> Result<(), Error> {
        let Features {
            utterance_callbacks,
            ..
        } = self.supported_features();
        if utterance_callbacks {
            let mut callbacks = CALLBACKS.lock().unwrap();
            let id = self.0.id().unwrap();
            let mut callbacks = callbacks.get_mut(&id).unwrap();
            callbacks.utterance_end = callback;
            Ok(())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /**
     * Called when this speech synthesizer is stopped and still has utterances in its queue.
     */
    pub fn on_utterance_stop(
        &self,
        callback: Option<Box<dyn FnMut(UtteranceId)>>,
    ) -> Result<(), Error> {
        let Features {
            utterance_callbacks,
            ..
        } = self.supported_features();
        if utterance_callbacks {
            let mut callbacks = CALLBACKS.lock().unwrap();
            let id = self.0.id().unwrap();
            let mut callbacks = callbacks.get_mut(&id).unwrap();
            callbacks.utterance_stop = callback;
            Ok(())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }
}

impl Drop for TTS {
    fn drop(&mut self) {
        if let Some(id) = self.0.id() {
            let mut callbacks = CALLBACKS.lock().unwrap();
            callbacks.remove(&id);
        }
    }
}
