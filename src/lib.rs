//! * a Text-To-Speech (TTS) library providing high-level interfaces to a variety of backends.
//!  * Currently supported backends are:
//!  * * Windows
//!  *   * Screen readers/SAPI via Tolk (requires `tolk` Cargo feature)
//!  *   * `WinRT`
//!  * * Linux via [Speech Dispatcher](https://freebsoft.org/speechd)
//!  * * macOS/iOS/tvOS/watchOS/visionOS via `AVFoundation` (macOS 10.14 and above)
//!  * * Android
//!  * * WebAssembly

use std::{boxed::Box, fmt, sync::Arc};

#[cfg(windows)]
use std::string::FromUtf16Error;

use dyn_clonable::clonable;
pub use oxilangtag::LanguageTag;
use parking_lot::{Mutex, RwLock};
#[cfg(target_os = "linux")]
use ssip_client_async::ClientError as SpeechDispatcherError;
use thiserror::Error;
#[cfg(all(windows, feature = "tolk"))]
use tolk::Tolk;
use tracing::{Span, field::Empty, instrument};

mod backends;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Backends {
    #[cfg(target_os = "android")]
    Android,
    #[cfg(target_vendor = "apple")]
    AvFoundation,
    #[cfg(target_os = "linux")]
    SpeechDispatcher,
    #[cfg(all(windows, feature = "tolk"))]
    Tolk,
    #[cfg(target_arch = "wasm32")]
    Web,
    #[cfg(windows)]
    WinRt,
}

impl fmt::Display for Backends {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            #[cfg(target_os = "android")]
            Backends::Android => writeln!(f, "Android"),
            #[cfg(target_vendor = "apple")]
            Backends::AvFoundation => writeln!(f, "AVFoundation"),
            #[cfg(target_os = "linux")]
            Backends::SpeechDispatcher => writeln!(f, "Speech Dispatcher"),
            #[cfg(all(windows, feature = "tolk"))]
            Backends::Tolk => writeln!(f, "Tolk"),
            #[cfg(target_arch = "wasm32")]
            Backends::Web => writeln!(f, "Web"),
            #[cfg(windows)]
            Backends::WinRt => writeln!(f, "Windows Runtime"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BackendId {
    #[cfg(target_os = "android")]
    Android(u64),
    #[cfg(target_vendor = "apple")]
    AvFoundation(u64),
    #[cfg(target_os = "linux")]
    SpeechDispatcher(usize),
    #[cfg(target_arch = "wasm32")]
    Web(u64),
    #[cfg(windows)]
    WinRt(u64),
}

impl fmt::Display for BackendId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            #[cfg(target_os = "android")]
            BackendId::Android(id) => writeln!(f, "Android({id})"),
            #[cfg(target_vendor = "apple")]
            BackendId::AvFoundation(id) => writeln!(f, "AvFoundation({id})"),
            #[cfg(target_os = "linux")]
            BackendId::SpeechDispatcher(id) => writeln!(f, "SpeechDispatcher({id})"),
            #[cfg(target_arch = "wasm32")]
            BackendId::Web(id) => writeln!(f, "Web({id})"),
            #[cfg(windows)]
            BackendId::WinRt(id) => writeln!(f, "WinRT({id})"),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum UtteranceId {
    #[cfg(target_os = "android")]
    Android(u64),
    #[cfg(target_vendor = "apple")]
    AvFoundation(usize),
    #[cfg(target_os = "linux")]
    SpeechDispatcher(u64),
    #[cfg(target_arch = "wasm32")]
    Web(u64),
    #[cfg(windows)]
    WinRt(u64),
}

impl fmt::Display for UtteranceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            #[cfg(target_os = "android")]
            UtteranceId::Android(id) => writeln!(f, "Android({id})"),
            #[cfg(target_os = "linux")]
            UtteranceId::SpeechDispatcher(id) => writeln!(f, "SpeechDispatcher({id})"),
            #[cfg(target_vendor = "apple")]
            UtteranceId::AvFoundation(id) => writeln!(f, "AvFoundation({id})"),
            #[cfg(target_arch = "wasm32")]
            UtteranceId::Web(id) => writeln!(f, "Web({id})"),
            #[cfg(windows)]
            UtteranceId::WinRt(id) => writeln!(f, "WinRt({id})"),
        }
    }
}

// Independent capability flags, not modal state, so a bool-heavy struct is appropriate.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Features {
    pub is_speaking: bool,
    pub pitch: bool,
    pub rate: bool,
    pub stop: bool,
    pub utterance_callbacks: bool,
    pub voice: bool,
    pub get_voice: bool,
    pub volume: bool,
}

impl fmt::Display for Features {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        writeln!(f, "{self:#?}")
    }
}

impl Features {
    #[instrument(level = "trace")]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Value not received")]
    NoneError,
    #[error("Operation failed")]
    OperationFailed,
    #[cfg(target_arch = "wasm32")]
    #[error("JavaScript error: [0]")]
    JavaScriptError(wasm_bindgen::JsValue),
    #[cfg(target_os = "linux")]
    #[error("Speech Dispatcher error: {0}")]
    SpeechDispatcher(SpeechDispatcherError),
    #[cfg(windows)]
    #[error("WinRT error")]
    WinRt(windows::core::Error),
    #[cfg(windows)]
    #[error("UTF string conversion failed")]
    UtfStringConversionFailed(#[from] FromUtf16Error),
    #[error("Unsupported feature")]
    UnsupportedFeature,
    #[error("Out of range")]
    OutOfRange,
    #[cfg(target_os = "android")]
    #[error("JNI error: [0])]")]
    JNI(#[from] jni::errors::Error),
}

// `ClientError` lacks `std::error::Error`, ruling out `#[from]`.
#[cfg(target_os = "linux")]
impl From<SpeechDispatcherError> for Error {
    fn from(error: SpeechDispatcherError) -> Self {
        Self::SpeechDispatcher(error)
    }
}

#[clonable]
pub trait Backend: Clone {
    fn id(&self) -> Option<BackendId>;
    fn supported_features(&self) -> Features;
    /// # Errors
    ///
    /// Returns an error if synthesis fails.
    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error>;
    /// # Errors
    ///
    /// Returns an error if speech cannot be stopped.
    fn stop(&mut self) -> Result<(), Error>;
    fn min_rate(&self) -> f32;
    fn max_rate(&self) -> f32;
    fn normal_rate(&self) -> f32;
    /// # Errors
    ///
    /// Returns an error if the rate cannot be read.
    fn get_rate(&self) -> Result<f32, Error>;
    /// # Errors
    ///
    /// Returns an error if the rate cannot be set.
    fn set_rate(&mut self, rate: f32) -> Result<(), Error>;
    fn min_pitch(&self) -> f32;
    fn max_pitch(&self) -> f32;
    fn normal_pitch(&self) -> f32;
    /// # Errors
    ///
    /// Returns an error if the pitch cannot be read.
    fn get_pitch(&self) -> Result<f32, Error>;
    /// # Errors
    ///
    /// Returns an error if the pitch cannot be set.
    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error>;
    fn min_volume(&self) -> f32;
    fn max_volume(&self) -> f32;
    fn normal_volume(&self) -> f32;
    /// # Errors
    ///
    /// Returns an error if the volume cannot be read.
    fn get_volume(&self) -> Result<f32, Error>;
    /// # Errors
    ///
    /// Returns an error if the volume cannot be set.
    fn set_volume(&mut self, volume: f32) -> Result<(), Error>;
    /// # Errors
    ///
    /// Returns an error if speaking state cannot be determined.
    fn is_speaking(&self) -> Result<bool, Error>;
    /// # Errors
    ///
    /// Returns an error if the voice list cannot be retrieved.
    fn voices(&self) -> Result<Vec<Voice>, Error>;
    /// # Errors
    ///
    /// Returns an error if the current voice cannot be determined.
    fn voice(&self) -> Result<Option<Voice>, Error>;
    /// # Errors
    ///
    /// Returns an error if the voice cannot be set.
    fn set_voice(&mut self, voice: &Voice) -> Result<(), Error>;
}

/// An utterance lifecycle callback. Backends invoke these from their own event threads, so
/// callbacks must be [`Send`] everywhere except single-threaded wasm.
#[cfg(not(target_arch = "wasm32"))]
pub type UtteranceCallback = Box<dyn FnMut(UtteranceId) + Send>;
/// An utterance lifecycle callback.
#[cfg(target_arch = "wasm32")]
pub type UtteranceCallback = Box<dyn FnMut(UtteranceId)>;

#[derive(Default)]
struct Callbacks {
    begin: Option<UtteranceCallback>,
    end: Option<UtteranceCallback>,
    stop: Option<UtteranceCallback>,
}

impl Callbacks {
    #[instrument(level = "trace", skip(self))]
    fn utterance_begin(&mut self, utterance_id: UtteranceId) {
        if let Some(callback) = self.begin.as_mut() {
            callback(utterance_id);
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn utterance_end(&mut self, utterance_id: UtteranceId) {
        if let Some(callback) = self.end.as_mut() {
            callback(utterance_id);
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn utterance_stop(&mut self, utterance_id: UtteranceId) {
        if let Some(callback) = self.stop.as_mut() {
            callback(utterance_id);
        }
    }
}

impl fmt::Debug for Callbacks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.debug_struct("Callbacks")
            .field("begin", &self.begin.is_some())
            .field("end", &self.end.is_some())
            .field("stop", &self.stop.is_some())
            .finish()
    }
}

/// Backends run on arbitrary threads, so they must be genuinely thread-safe everywhere except
/// single-threaded wasm, where JS values can never be [`Send`].
#[cfg(not(target_arch = "wasm32"))]
type BoxedBackend = Box<dyn Backend + Send + Sync>;
#[cfg(target_arch = "wasm32")]
type BoxedBackend = Box<dyn Backend>;

#[derive(Clone)]
pub struct Tts {
    backend: Arc<RwLock<BoxedBackend>>,
    callbacks: Arc<Mutex<Callbacks>>,
}

impl Tts {
    /// Create a new `TTS` instance with the specified backend.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend fails to initialize.
    // Wasm is single-threaded, so backends and callbacks there are not Send; the Arc-based
    // plumbing is shared with the threaded targets.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    #[instrument(level = "info", err)]
    pub fn new(backend: Backends) -> Result<Tts, Error> {
        let callbacks = Arc::new(Mutex::new(Callbacks::default()));
        let backend: BoxedBackend = match backend {
            #[cfg(target_os = "linux")]
            Backends::SpeechDispatcher => Box::new(backends::SpeechDispatcher::new(&callbacks)?),
            #[cfg(target_arch = "wasm32")]
            Backends::Web => Box::new(backends::Web::new(callbacks.clone())?),
            #[cfg(all(windows, feature = "tolk"))]
            Backends::Tolk => Box::new(backends::Tolk::new().ok_or(Error::NoneError)?),
            #[cfg(windows)]
            Backends::WinRt => Box::new(backends::WinRt::new(callbacks.clone())?),
            #[cfg(target_vendor = "apple")]
            Backends::AvFoundation => Box::new(backends::AvFoundation::new(callbacks.clone())?),
            #[cfg(target_os = "android")]
            Backends::Android => Box::new(backends::Android::new(callbacks.clone())?),
        };
        Ok(Tts {
            backend: Arc::new(RwLock::new(backend)),
            callbacks,
        })
    }

    /// Create a new `TTS` instance with the default backend for the current platform.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend fails to initialize.
    #[allow(clippy::should_implement_trait)]
    #[instrument(level = "info", err)]
    pub fn default() -> Result<Tts, Error> {
        #[cfg(target_os = "linux")]
        let tts = Tts::new(Backends::SpeechDispatcher);
        #[cfg(all(windows, feature = "tolk"))]
        let tts = if let Ok(tts) = Tts::new(Backends::Tolk) {
            Ok(tts)
        } else {
            Tts::new(Backends::WinRt)
        };
        #[cfg(all(windows, not(feature = "tolk")))]
        let tts = Tts::new(Backends::WinRt);
        #[cfg(target_arch = "wasm32")]
        let tts = Tts::new(Backends::Web);
        #[cfg(target_vendor = "apple")]
        let tts = Tts::new(Backends::AvFoundation);
        #[cfg(target_os = "android")]
        let tts = Tts::new(Backends::Android);
        tts
    }

    /// Returns the features supported by this TTS engine
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn supported_features(&self) -> Features {
        self.backend.read().supported_features()
    }

    /// Speaks the specified text, optionally interrupting current speech.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend fails to synthesize the text.
    #[instrument(level = "debug", skip(self, text), fields(text = Empty), err, ret)]
    pub fn speak<S: Into<String>>(
        &mut self,
        text: S,
        interrupt: bool,
    ) -> Result<Option<UtteranceId>, Error> {
        let text = text.into();
        Span::current().record("text", text.as_str());
        self.backend.write().speak(&text, interrupt)
    }

    /// Stops current speech.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot stop speech, or another error
    /// if stopping fails.
    #[instrument(level = "debug", skip(self))]
    pub fn stop(&mut self) -> Result<&Self, Error> {
        let Features { stop, .. } = self.supported_features();
        if stop {
            self.backend.write().stop()?;
            Ok(self)
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the minimum rate for this speech synthesizer.
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn min_rate(&self) -> f32 {
        self.backend.read().min_rate()
    }

    /// Returns the maximum rate for this speech synthesizer.
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn max_rate(&self) -> f32 {
        self.backend.read().max_rate()
    }

    /// Returns the normal rate for this speech synthesizer.
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn normal_rate(&self) -> f32 {
        self.backend.read().normal_rate()
    }

    /// Gets the current speech rate.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot report its rate, or another
    /// error if reading it fails.
    #[instrument(level = "debug", skip(self), err, ret)]
    pub fn get_rate(&self) -> Result<f32, Error> {
        let Features { rate, .. } = self.supported_features();
        if rate {
            self.backend.read().get_rate()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Sets the desired speech rate.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its rate,
    /// [`Error::OutOfRange`] if the rate is outside the backend's supported range, or another
    /// error if setting it fails.
    #[instrument(level = "debug", skip(self))]
    pub fn set_rate(&mut self, rate: f32) -> Result<&Self, Error> {
        let Features {
            rate: rate_feature, ..
        } = self.supported_features();
        if rate_feature {
            let mut backend = self.backend.write();
            if rate < backend.min_rate() || rate > backend.max_rate() {
                Err(Error::OutOfRange)
            } else {
                backend.set_rate(rate)?;
                Ok(self)
            }
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the minimum pitch for this speech synthesizer.
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn min_pitch(&self) -> f32 {
        self.backend.read().min_pitch()
    }

    /// Returns the maximum pitch for this speech synthesizer.
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn max_pitch(&self) -> f32 {
        self.backend.read().max_pitch()
    }

    /// Returns the normal pitch for this speech synthesizer.
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn normal_pitch(&self) -> f32 {
        self.backend.read().normal_pitch()
    }

    /// Gets the current speech pitch.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot report its pitch, or another
    /// error if reading it fails.
    #[instrument(level = "debug", skip(self), err, ret)]
    pub fn get_pitch(&self) -> Result<f32, Error> {
        let Features { pitch, .. } = self.supported_features();
        if pitch {
            self.backend.read().get_pitch()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Sets the desired speech pitch.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its pitch,
    /// [`Error::OutOfRange`] if the pitch is outside the backend's supported range, or another
    /// error if setting it fails.
    #[instrument(level = "debug", skip(self))]
    pub fn set_pitch(&mut self, pitch: f32) -> Result<&Self, Error> {
        let Features {
            pitch: pitch_feature,
            ..
        } = self.supported_features();
        if pitch_feature {
            let mut backend = self.backend.write();
            if pitch < backend.min_pitch() || pitch > backend.max_pitch() {
                Err(Error::OutOfRange)
            } else {
                backend.set_pitch(pitch)?;
                Ok(self)
            }
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the minimum volume for this speech synthesizer.
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn min_volume(&self) -> f32 {
        self.backend.read().min_volume()
    }

    /// Returns the maximum volume for this speech synthesizer.
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn max_volume(&self) -> f32 {
        self.backend.read().max_volume()
    }

    /// Returns the normal volume for this speech synthesizer.
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn normal_volume(&self) -> f32 {
        self.backend.read().normal_volume()
    }

    /// Gets the current speech volume.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot report its volume, or another
    /// error if reading it fails.
    #[instrument(level = "debug", skip(self), err, ret)]
    pub fn get_volume(&self) -> Result<f32, Error> {
        let Features { volume, .. } = self.supported_features();
        if volume {
            self.backend.read().get_volume()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Sets the desired speech volume.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its volume,
    /// [`Error::OutOfRange`] if the volume is outside the backend's supported range, or another
    /// error if setting it fails.
    #[instrument(level = "debug", skip(self))]
    pub fn set_volume(&mut self, volume: f32) -> Result<&Self, Error> {
        let Features {
            volume: volume_feature,
            ..
        } = self.supported_features();
        if volume_feature {
            let mut backend = self.backend.write();
            if volume < backend.min_volume() || volume > backend.max_volume() {
                Err(Error::OutOfRange)
            } else {
                backend.set_volume(volume)?;
                Ok(self)
            }
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns whether this speech synthesizer is speaking.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot report its speaking state, or
    /// another error if reading it fails.
    #[instrument(level = "trace", skip(self), err, ret)]
    pub fn is_speaking(&self) -> Result<bool, Error> {
        let Features { is_speaking, .. } = self.supported_features();
        if is_speaking {
            self.backend.read().is_speaking()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns list of available voices.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot list voices, or another error
    /// if retrieving them fails.
    #[instrument(level = "debug", skip(self), err)]
    pub fn voices(&self) -> Result<Vec<Voice>, Error> {
        let Features { voice, .. } = self.supported_features();
        if voice {
            self.backend.read().voices()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Return the current speaking voice.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot report its current voice, or
    /// another error if reading it fails.
    #[instrument(level = "debug", skip(self), err, ret)]
    pub fn voice(&self) -> Result<Option<Voice>, Error> {
        let Features { get_voice, .. } = self.supported_features();
        if get_voice {
            self.backend.read().voice()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Set speaking voice.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change voices, or another
    /// error if setting the voice fails.
    #[instrument(level = "debug", skip(self), err)]
    pub fn set_voice(&mut self, voice: &Voice) -> Result<(), Error> {
        let Features {
            voice: voice_feature,
            ..
        } = self.supported_features();
        if voice_feature {
            self.backend.write().set_voice(voice)
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Called when this speech synthesizer begins speaking an utterance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend does not support utterance callbacks.
    #[instrument(
        level = "debug",
        skip(self, callback),
        fields(registered = callback.is_some()),
        err
    )]
    pub fn on_utterance_begin(&self, callback: Option<UtteranceCallback>) -> Result<(), Error> {
        let Features {
            utterance_callbacks,
            ..
        } = self.supported_features();
        if utterance_callbacks {
            self.callbacks.lock().begin = callback;
            Ok(())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Called when this speech synthesizer finishes speaking an utterance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend does not support utterance callbacks.
    #[instrument(
        level = "debug",
        skip(self, callback),
        fields(registered = callback.is_some()),
        err
    )]
    pub fn on_utterance_end(&self, callback: Option<UtteranceCallback>) -> Result<(), Error> {
        let Features {
            utterance_callbacks,
            ..
        } = self.supported_features();
        if utterance_callbacks {
            self.callbacks.lock().end = callback;
            Ok(())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Called when this speech synthesizer is stopped and still has utterances in its queue.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend does not support utterance callbacks.
    #[instrument(
        level = "debug",
        skip(self, callback),
        fields(registered = callback.is_some()),
        err
    )]
    pub fn on_utterance_stop(&self, callback: Option<UtteranceCallback>) -> Result<(), Error> {
        let Features {
            utterance_callbacks,
            ..
        } = self.supported_features();
        if utterance_callbacks {
            self.callbacks.lock().stop = callback;
            Ok(())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /*
     * Returns `true` if a screen reader is available to provide speech.
     */
    #[allow(unreachable_code)]
    #[instrument(level = "debug", ret)]
    #[must_use]
    pub fn screen_reader_available() -> bool {
        #[cfg(target_os = "windows")]
        {
            #[cfg(feature = "tolk")]
            {
                let tolk = Tolk::new();
                return tolk.detect_screen_reader().is_some();
            }
            #[cfg(not(feature = "tolk"))]
            return false;
        }
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Gender {
    Male,
    Female,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Voice {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) gender: Option<Gender>,
    pub(crate) language: LanguageTag<String>,
}

impl Voice {
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn id(&self) -> String {
        self.id.clone()
    }

    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn gender(&self) -> Option<Gender> {
        self.gender
    }

    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn language(&self) -> LanguageTag<String> {
        self.language.clone()
    }
}
