/*!
 * a Text-To-Speech (TTS) library providing high-level interfaces to a variety of backends.
 * Currently supported backends are:
 * * Windows
 *   * Screen readers/SAPI via Tolk
 *   * WinRT
 * * Linux via [Speech Dispatcher](https://freebsoft.org/speechd)
 * * MacOS
 *   * AppKit on MacOS 10.13 and below
 *   * AVFoundation on MacOS 10.14 and, eventually, iDevices
 * * WebAssembly
 */

use std::boxed::Box;
#[cfg(target_os = "macos")]
use std::ffi::CStr;

#[cfg(target_os = "macos")]
use cocoa_foundation::base::id;
#[cfg(target_os = "macos")]
use libc::c_char;
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
use thiserror::Error;

mod backends;

pub enum Backends {
    #[cfg(target_os = "linux")]
    SpeechDispatcher,
    #[cfg(target_arch = "wasm32")]
    Web,
    #[cfg(windows)]
    Tolk,
    #[cfg(windows)]
    WinRT,
    #[cfg(target_os = "macos")]
    AppKit,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    AvFoundation,
}

pub struct Features {
    pub stop: bool,
    pub rate: bool,
    pub pitch: bool,
    pub volume: bool,
    pub is_speaking: bool,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Value not received")]
    NoneError,
    #[cfg(target_arch = "wasm32")]
    #[error("JavaScript error: [0])]")]
    JavaScriptError(wasm_bindgen::JsValue),
    #[cfg(windows)]
    #[error("WinRT error")]
    WinRT(winrt::Error),
    #[error("Unsupported feature")]
    UnsupportedFeature,
    #[error("Out of range")]
    OutOfRange,
}

pub trait Backend {
    fn supported_features(&self) -> Features;
    fn speak(&mut self, text: &str, interrupt: bool) -> Result<(), Error>;
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

pub struct TTS(Box<dyn Backend>);

unsafe impl std::marker::Send for TTS {}

unsafe impl std::marker::Sync for TTS {}

impl TTS {
    /**
     * Create a new `TTS` instance with the specified backend.
     */
    pub fn new(backend: Backends) -> Result<TTS, Error> {
        match backend {
            #[cfg(target_os = "linux")]
            Backends::SpeechDispatcher => Ok(TTS(Box::new(backends::SpeechDispatcher::new()))),
            #[cfg(target_arch = "wasm32")]
            Backends::Web => {
                let tts = backends::Web::new()?;
                Ok(TTS(Box::new(tts)))
            }
            #[cfg(windows)]
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
                let tts = backends::winrt::WinRT::new()?;
                Ok(TTS(Box::new(tts)))
            }
            #[cfg(target_os = "macos")]
            Backends::AppKit => Ok(TTS(Box::new(backends::AppKit::new()))),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Backends::AvFoundation => Ok(TTS(Box::new(backends::AvFoundation::new()))),
        }
    }

    pub fn default() -> Result<TTS, Error> {
        #[cfg(target_os = "linux")]
        let tts = TTS::new(Backends::SpeechDispatcher);
        #[cfg(windows)]
        let tts = if let Some(tts) = TTS::new(Backends::Tolk).ok() {
            Ok(tts)
        } else {
            TTS::new(Backends::WinRT)
        };
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
            let minor_version: i8 = version_parts[1].parse().unwrap();
            if minor_version >= 14 {
                TTS::new(Backends::AvFoundation)
            } else {
                TTS::new(Backends::AppKit)
            }
        };
        #[cfg(target_os = "ios")]
        let tts = TTS::new(Backends::AvFoundation);
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
    pub fn speak<S: Into<String>>(&mut self, text: S, interrupt: bool) -> Result<&Self, Error> {
        self.0.speak(text.into().as_str(), interrupt)?;
        Ok(self)
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
}
