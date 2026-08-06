//! * a Text-To-Speech (TTS) library providing high-level interfaces to a variety of backends.
//!  * Currently supported backends are:
//!  * * Windows
//!  *   * NVDA via the [NVDA Controller Client](https://github.com/nvaccess/nvda/tree/master/extras/controllerClient) (requires shipping `nvdaControllerClient.dll` with your application)
//!  *   * Screen readers/SAPI via Tolk (requires `tolk` Cargo feature)
//!  *   * `WinRT`
//!  * * Linux via [Speech Dispatcher](https://freebsoft.org/speechd)
//!  * * macOS/iOS/tvOS/watchOS/visionOS via `AVFoundation` (macOS 10.14 and above)
//!  * * Android (`minSdkVersion` 26 and above; see the README for its one setup requirement)
//!  * * WebAssembly

use std::{boxed::Box, fmt, sync::Arc};

#[cfg(any(windows, target_os = "android", target_vendor = "apple"))]
use std::io::Cursor;
#[cfg(windows)]
use std::string::FromUtf16Error;

use dyn_clonable::clonable;
pub use hound::{SampleFormat, WavSpec};
pub use oxilangtag::LanguageTag;
use parking_lot::{Mutex, RwLock};
#[cfg(target_os = "linux")]
use ssip_client_async::ClientError as SpeechDispatcherError;
use thiserror::Error;
use tracing::instrument;

mod backends;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Backends {
    #[cfg(target_os = "android")]
    Android,
    #[cfg(target_vendor = "apple")]
    AvFoundation,
    #[cfg(windows)]
    Nvda,
    #[cfg(target_os = "linux")]
    Orca,
    #[cfg(target_os = "linux")]
    SpeechDispatcher,
    #[cfg(all(windows, feature = "tolk"))]
    Tolk,
    #[cfg(target_arch = "wasm32")]
    Web,
    #[cfg(windows)]
    WinRt,
}

impl Backends {
    /// Returns this backend's human-readable name.
    #[instrument(level = "trace")]
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            #[cfg(target_os = "android")]
            Backends::Android => backends::Android::NAME,
            #[cfg(target_vendor = "apple")]
            Backends::AvFoundation => backends::AvFoundation::NAME,
            #[cfg(windows)]
            Backends::Nvda => backends::Nvda::NAME,
            #[cfg(target_os = "linux")]
            Backends::Orca => backends::Orca::NAME,
            #[cfg(target_os = "linux")]
            Backends::SpeechDispatcher => backends::SpeechDispatcher::NAME,
            #[cfg(all(windows, feature = "tolk"))]
            Backends::Tolk => backends::Tolk::NAME,
            #[cfg(target_arch = "wasm32")]
            Backends::Web => backends::Web::NAME,
            #[cfg(windows)]
            Backends::WinRt => backends::WinRt::NAME,
        }
    }

    /// Returns whether this backend can currently provide speech.
    ///
    /// This is a cheap probe: `true` means the backend is worth trying, not that
    /// [`Tts::new`] cannot fail.
    #[instrument(level = "debug", ret)]
    #[must_use]
    pub fn is_available(self) -> bool {
        match self {
            #[cfg(target_os = "android")]
            Backends::Android => backends::Android::is_available(),
            #[cfg(target_vendor = "apple")]
            Backends::AvFoundation => backends::AvFoundation::is_available(),
            #[cfg(windows)]
            Backends::Nvda => backends::Nvda::is_available(),
            #[cfg(target_os = "linux")]
            Backends::Orca => backends::Orca::is_available(),
            #[cfg(target_os = "linux")]
            Backends::SpeechDispatcher => backends::SpeechDispatcher::is_available(),
            #[cfg(all(windows, feature = "tolk"))]
            Backends::Tolk => backends::Tolk::is_available(),
            #[cfg(target_arch = "wasm32")]
            Backends::Web => backends::Web::is_available(),
            #[cfg(windows)]
            Backends::WinRt => backends::WinRt::is_available(),
        }
    }
}

impl fmt::Display for Backends {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
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
            UtteranceId::Android(id) => write!(f, "Android({id})"),
            #[cfg(target_os = "linux")]
            UtteranceId::SpeechDispatcher(id) => write!(f, "SpeechDispatcher({id})"),
            #[cfg(target_vendor = "apple")]
            UtteranceId::AvFoundation(id) => write!(f, "AvFoundation({id})"),
            #[cfg(target_arch = "wasm32")]
            UtteranceId::Web(id) => write!(f, "Web({id})"),
            #[cfg(windows)]
            UtteranceId::WinRt(id) => write!(f, "WinRt({id})"),
        }
    }
}

/// Speech synthesized to memory: a complete WAV file and its parsed format.
#[derive(Clone)]
pub struct SynthesizedAudio {
    spec: WavSpec,
    bytes: Vec<u8>,
}

impl SynthesizedAudio {
    /// Validates that `bytes` is a well-formed WAV file and captures its format.
    #[cfg(any(windows, target_os = "android", target_vendor = "apple"))]
    pub(crate) fn from_wav(bytes: Vec<u8>) -> Result<Self, Error> {
        let spec = hound::WavReader::new(Cursor::new(&bytes))?.spec();
        Ok(Self { spec, bytes })
    }

    /// Returns the audio format.
    #[must_use]
    pub fn spec(&self) -> WavSpec {
        self.spec
    }

    /// Returns the complete WAV file, header included.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Consumes this audio, returning the complete WAV file, header included.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl AsRef<[u8]> for SynthesizedAudio {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl From<SynthesizedAudio> for Vec<u8> {
    fn from(audio: SynthesizedAudio) -> Self {
        audio.bytes
    }
}

impl fmt::Debug for SynthesizedAudio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.debug_struct("SynthesizedAudio")
            .field("spec", &self.spec)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

// Independent capability flags, not modal state, so a bool-heavy struct is appropriate.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Features {
    pub is_speaking: bool,
    pub pause: bool,
    pub pitch: bool,
    pub rate: bool,
    pub stop: bool,
    pub synthesis: bool,
    pub utterance_callbacks: bool,
    pub voice: bool,
    pub get_voice: bool,
    pub volume: bool,
}

impl fmt::Display for Features {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{self:#?}")
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("backend unavailable: {0}")]
    BackendUnavailable(&'static str),
    #[error("{0} failed")]
    OperationFailed(&'static str),
    #[error("unexpected response from backend")]
    UnexpectedResponse,
    #[error("voice not found: {0}")]
    VoiceNotFound(String),
    #[cfg(target_arch = "wasm32")]
    #[error("JavaScript error: {0:?}")]
    JavaScriptError(wasm_bindgen::JsValue),
    #[cfg(target_os = "linux")]
    #[error("Speech Dispatcher error: {0}")]
    SpeechDispatcher(SpeechDispatcherError),
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    Zbus(#[from] zbus::Error),
    #[cfg(windows)]
    #[error(transparent)]
    WinRt(#[from] windows::core::Error),
    #[cfg(windows)]
    #[error(transparent)]
    UtfStringConversionFailed(#[from] FromUtf16Error),
    #[error(transparent)]
    Wav(#[from] hound::Error),
    #[error("Unsupported feature")]
    UnsupportedFeature,
    #[error("Out of range")]
    OutOfRange,
    #[cfg(target_os = "android")]
    #[error(transparent)]
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
pub(crate) trait Backend: Clone {
    fn supported_features(&self) -> Features;
    /// # Errors
    ///
    /// Returns an error if synthesis fails.
    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error>;
    /// # Errors
    ///
    /// Returns an error if the text cannot be synthesized to audio.
    fn synthesize(&mut self, text: &str) -> Result<SynthesizedAudio, Error>;
    /// # Errors
    ///
    /// Returns an error if speech cannot be stopped.
    fn stop(&mut self) -> Result<(), Error>;
    /// # Errors
    ///
    /// Returns an error if speech cannot be paused.
    fn pause(&mut self) -> Result<(), Error>;
    /// # Errors
    ///
    /// Returns an error if speech cannot be resumed.
    fn resume(&mut self) -> Result<(), Error>;
    /// # Errors
    ///
    /// Returns an error if paused state cannot be determined.
    fn is_paused(&self) -> Result<bool, Error>;
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
pub trait UtteranceCallback: FnMut(UtteranceId) + Send + 'static {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: FnMut(UtteranceId) + Send + 'static> UtteranceCallback for T {}
/// An utterance lifecycle callback.
#[cfg(target_arch = "wasm32")]
pub trait UtteranceCallback: FnMut(UtteranceId) + 'static {}
#[cfg(target_arch = "wasm32")]
impl<T: FnMut(UtteranceId) + 'static> UtteranceCallback for T {}

#[derive(Default)]
struct Callbacks {
    begin: Option<Box<dyn UtteranceCallback>>,
    end: Option<Box<dyn UtteranceCallback>>,
    stop: Option<Box<dyn UtteranceCallback>>,
    pause: Option<Box<dyn UtteranceCallback>>,
    resume: Option<Box<dyn UtteranceCallback>>,
    synthesis_begin: Option<Box<dyn UtteranceCallback>>,
    synthesis_complete: Option<Box<dyn UtteranceCallback>>,
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

    #[cfg(any(
        windows,
        target_os = "linux",
        target_vendor = "apple",
        target_arch = "wasm32"
    ))]
    #[instrument(level = "trace", skip(self))]
    fn utterance_pause(&mut self, utterance_id: UtteranceId) {
        if let Some(callback) = self.pause.as_mut() {
            callback(utterance_id);
        }
    }

    #[cfg(any(
        windows,
        target_os = "linux",
        target_vendor = "apple",
        target_arch = "wasm32"
    ))]
    #[instrument(level = "trace", skip(self))]
    fn utterance_resume(&mut self, utterance_id: UtteranceId) {
        if let Some(callback) = self.resume.as_mut() {
            callback(utterance_id);
        }
    }

    #[cfg(any(windows, target_os = "android", target_vendor = "apple"))]
    #[instrument(level = "trace", skip(self))]
    fn synthesis_begin(&mut self, utterance_id: UtteranceId) {
        if let Some(callback) = self.synthesis_begin.as_mut() {
            callback(utterance_id);
        }
    }

    #[cfg(any(windows, target_os = "android", target_vendor = "apple"))]
    #[instrument(level = "trace", skip(self))]
    fn synthesis_complete(&mut self, utterance_id: UtteranceId) {
        if let Some(callback) = self.synthesis_complete.as_mut() {
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
            .field("pause", &self.pause.is_some())
            .field("resume", &self.resume.is_some())
            .field("synthesis_begin", &self.synthesis_begin.is_some())
            .field("synthesis_complete", &self.synthesis_complete.is_some())
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
    backend_name: &'static str,
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
        let backend_name = backend.name();
        let backend: BoxedBackend = match backend {
            #[cfg(target_os = "linux")]
            Backends::Orca => Box::new(backends::Orca::new()?),
            #[cfg(target_os = "linux")]
            Backends::SpeechDispatcher => Box::new(backends::SpeechDispatcher::new(&callbacks)?),
            #[cfg(target_arch = "wasm32")]
            Backends::Web => Box::new(backends::Web::new(callbacks.clone())?),
            #[cfg(windows)]
            Backends::Nvda => Box::new(backends::Nvda::new()?),
            #[cfg(all(windows, feature = "tolk"))]
            Backends::Tolk => Box::new(
                backends::Tolk::new().ok_or(Error::BackendUnavailable("Tolk failed to load"))?,
            ),
            #[cfg(windows)]
            Backends::WinRt => Box::new(backends::WinRt::new(callbacks.clone())?),
            #[cfg(target_vendor = "apple")]
            Backends::AvFoundation => Box::new(backends::AvFoundation::new(callbacks.clone())?),
            #[cfg(target_os = "android")]
            Backends::Android => Box::new(backends::Android::new(callbacks.clone())?),
        };
        Ok(Tts {
            backend: Arc::new(RwLock::new(backend)),
            backend_name,
            callbacks,
        })
    }

    /// Returns the backends available on this platform, most preferred first, with screen
    /// readers ahead of general-purpose synthesizers. Index 0 is the platform default.
    #[instrument(level = "debug", ret)]
    #[must_use]
    pub fn backends() -> Vec<Backends> {
        let candidates: &[Backends] = &[
            #[cfg(windows)]
            Backends::Nvda,
            #[cfg(all(windows, feature = "tolk"))]
            Backends::Tolk,
            #[cfg(windows)]
            Backends::WinRt,
            #[cfg(target_os = "linux")]
            Backends::Orca,
            #[cfg(target_os = "linux")]
            Backends::SpeechDispatcher,
            #[cfg(target_vendor = "apple")]
            Backends::AvFoundation,
            #[cfg(target_os = "android")]
            Backends::Android,
            #[cfg(target_arch = "wasm32")]
            Backends::Web,
        ];
        candidates
            .iter()
            .copied()
            .filter(|backend| backend.is_available())
            .collect()
    }

    /// Create a new `TTS` instance with the default backend for the current platform.
    ///
    /// # Errors
    ///
    /// Returns an error if no backend is available or the backend fails to initialize.
    #[allow(clippy::should_implement_trait)]
    #[instrument(level = "info", err)]
    pub fn default() -> Result<Tts, Error> {
        let backend = *Tts::backends()
            .first()
            .ok_or(Error::BackendUnavailable("no backend for this platform"))?;
        Tts::new(backend)
    }

    /// Returns the name of the backend powering this instance.
    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        self.backend_name
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
    #[instrument(level = "debug", skip(self), err, ret)]
    pub fn speak(&self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        self.backend.write().speak(text, interrupt)
    }

    /// Synthesizes the specified text to audio, returned as a complete WAV file.
    ///
    /// Blocks until synthesis finishes, which can take noticeable time for long text; call it
    /// off any thread that must stay responsive.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot synthesize to audio, or
    /// another error if synthesis fails.
    #[instrument(level = "debug", skip(self), err, ret)]
    pub fn synthesize(&self, text: &str) -> Result<SynthesizedAudio, Error> {
        let Features { synthesis, .. } = self.supported_features();
        if synthesis {
            self.backend.write().synthesize(text)
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Stops current speech.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot stop speech, or another error
    /// if stopping fails.
    #[instrument(level = "debug", skip(self), err)]
    pub fn stop(&self) -> Result<(), Error> {
        let Features { stop, .. } = self.supported_features();
        if stop {
            self.backend.write().stop()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Pauses current speech, retaining it for [`resume`](Self::resume). Does nothing if no
    /// speech is active or queued.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot pause speech, or another
    /// error if pausing fails.
    #[instrument(level = "debug", skip(self), err)]
    pub fn pause(&self) -> Result<(), Error> {
        let Features { pause, .. } = self.supported_features();
        if pause {
            self.backend.write().pause()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Resumes previously paused speech.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot pause speech, or another
    /// error if resuming fails.
    #[instrument(level = "debug", skip(self), err)]
    pub fn resume(&self) -> Result<(), Error> {
        let Features { pause, .. } = self.supported_features();
        if pause {
            self.backend.write().resume()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Pauses current speech, or resumes it if already paused.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot pause speech, or another
    /// error if toggling fails.
    #[instrument(level = "debug", skip(self), err)]
    pub fn toggle_pause(&self) -> Result<(), Error> {
        let Features { pause, .. } = self.supported_features();
        if pause {
            let mut backend = self.backend.write();
            if backend.is_paused()? {
                backend.resume()
            } else {
                backend.pause()
            }
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns whether this speech synthesizer is paused.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot pause speech, or another
    /// error if reading the paused state fails.
    #[instrument(level = "trace", skip(self), err, ret)]
    pub fn is_paused(&self) -> Result<bool, Error> {
        let Features { pause, .. } = self.supported_features();
        if pause {
            self.backend.read().is_paused()
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the minimum rate for this speech synthesizer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its rate.
    #[instrument(level = "trace", skip(self), err, ret)]
    pub fn min_rate(&self) -> Result<f32, Error> {
        let Features { rate, .. } = self.supported_features();
        if rate {
            Ok(self.backend.read().min_rate())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the maximum rate for this speech synthesizer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its rate.
    #[instrument(level = "trace", skip(self), err, ret)]
    pub fn max_rate(&self) -> Result<f32, Error> {
        let Features { rate, .. } = self.supported_features();
        if rate {
            Ok(self.backend.read().max_rate())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the normal rate for this speech synthesizer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its rate.
    #[instrument(level = "trace", skip(self), err, ret)]
    pub fn normal_rate(&self) -> Result<f32, Error> {
        let Features { rate, .. } = self.supported_features();
        if rate {
            Ok(self.backend.read().normal_rate())
        } else {
            Err(Error::UnsupportedFeature)
        }
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
    #[instrument(level = "debug", skip(self), err)]
    pub fn set_rate(&self, rate: f32) -> Result<(), Error> {
        let Features {
            rate: rate_feature, ..
        } = self.supported_features();
        if rate_feature {
            let mut backend = self.backend.write();
            if rate < backend.min_rate() || rate > backend.max_rate() {
                Err(Error::OutOfRange)
            } else {
                backend.set_rate(rate)
            }
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the minimum pitch for this speech synthesizer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its pitch.
    #[instrument(level = "trace", skip(self), err, ret)]
    pub fn min_pitch(&self) -> Result<f32, Error> {
        let Features { pitch, .. } = self.supported_features();
        if pitch {
            Ok(self.backend.read().min_pitch())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the maximum pitch for this speech synthesizer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its pitch.
    #[instrument(level = "trace", skip(self), err, ret)]
    pub fn max_pitch(&self) -> Result<f32, Error> {
        let Features { pitch, .. } = self.supported_features();
        if pitch {
            Ok(self.backend.read().max_pitch())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the normal pitch for this speech synthesizer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its pitch.
    #[instrument(level = "trace", skip(self), err, ret)]
    pub fn normal_pitch(&self) -> Result<f32, Error> {
        let Features { pitch, .. } = self.supported_features();
        if pitch {
            Ok(self.backend.read().normal_pitch())
        } else {
            Err(Error::UnsupportedFeature)
        }
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
    #[instrument(level = "debug", skip(self), err)]
    pub fn set_pitch(&self, pitch: f32) -> Result<(), Error> {
        let Features {
            pitch: pitch_feature,
            ..
        } = self.supported_features();
        if pitch_feature {
            let mut backend = self.backend.write();
            if pitch < backend.min_pitch() || pitch > backend.max_pitch() {
                Err(Error::OutOfRange)
            } else {
                backend.set_pitch(pitch)
            }
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the minimum volume for this speech synthesizer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its volume.
    #[instrument(level = "trace", skip(self), err, ret)]
    pub fn min_volume(&self) -> Result<f32, Error> {
        let Features { volume, .. } = self.supported_features();
        if volume {
            Ok(self.backend.read().min_volume())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the maximum volume for this speech synthesizer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its volume.
    #[instrument(level = "trace", skip(self), err, ret)]
    pub fn max_volume(&self) -> Result<f32, Error> {
        let Features { volume, .. } = self.supported_features();
        if volume {
            Ok(self.backend.read().max_volume())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Returns the normal volume for this speech synthesizer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot change its volume.
    #[instrument(level = "trace", skip(self), err, ret)]
    pub fn normal_volume(&self) -> Result<f32, Error> {
        let Features { volume, .. } = self.supported_features();
        if volume {
            Ok(self.backend.read().normal_volume())
        } else {
            Err(Error::UnsupportedFeature)
        }
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
    #[instrument(level = "debug", skip(self), err)]
    pub fn set_volume(&self, volume: f32) -> Result<(), Error> {
        let Features {
            volume: volume_feature,
            ..
        } = self.supported_features();
        if volume_feature {
            let mut backend = self.backend.write();
            if volume < backend.min_volume() || volume > backend.max_volume() {
                Err(Error::OutOfRange)
            } else {
                backend.set_volume(volume)
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
    pub fn set_voice(&self, voice: &Voice) -> Result<(), Error> {
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
    #[instrument(level = "debug", skip(self, callback), err)]
    pub fn on_utterance_begin(&self, callback: impl UtteranceCallback) -> Result<(), Error> {
        let Features {
            utterance_callbacks,
            ..
        } = self.supported_features();
        if utterance_callbacks {
            self.callbacks.lock().begin = Some(Box::new(callback));
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
    #[instrument(level = "debug", skip(self, callback), err)]
    pub fn on_utterance_end(&self, callback: impl UtteranceCallback) -> Result<(), Error> {
        let Features {
            utterance_callbacks,
            ..
        } = self.supported_features();
        if utterance_callbacks {
            self.callbacks.lock().end = Some(Box::new(callback));
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
    #[instrument(level = "debug", skip(self, callback), err)]
    pub fn on_utterance_stop(&self, callback: impl UtteranceCallback) -> Result<(), Error> {
        let Features {
            utterance_callbacks,
            ..
        } = self.supported_features();
        if utterance_callbacks {
            self.callbacks.lock().stop = Some(Box::new(callback));
            Ok(())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Called when this speech synthesizer pauses an utterance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot pause speech.
    #[instrument(level = "debug", skip(self, callback), err)]
    pub fn on_utterance_pause(&self, callback: impl UtteranceCallback) -> Result<(), Error> {
        let Features { pause, .. } = self.supported_features();
        if pause {
            self.callbacks.lock().pause = Some(Box::new(callback));
            Ok(())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Called when this speech synthesizer resumes a previously paused utterance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend cannot pause speech.
    #[instrument(level = "debug", skip(self, callback), err)]
    pub fn on_utterance_resume(&self, callback: impl UtteranceCallback) -> Result<(), Error> {
        let Features { pause, .. } = self.supported_features();
        if pause {
            self.callbacks.lock().resume = Some(Box::new(callback));
            Ok(())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Clears all registered utterance callbacks.
    #[instrument(level = "debug", skip(self))]
    pub fn clear_utterance_callbacks(&self) {
        let mut callbacks = self.callbacks.lock();
        callbacks.begin = None;
        callbacks.end = None;
        callbacks.stop = None;
        callbacks.pause = None;
        callbacks.resume = None;
    }

    /// Called when this speech synthesizer begins synthesizing an utterance to audio.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend does not support synthesis.
    #[instrument(level = "debug", skip(self, callback), err)]
    pub fn on_synthesis_begin(&self, callback: impl UtteranceCallback) -> Result<(), Error> {
        let Features { synthesis, .. } = self.supported_features();
        if synthesis {
            self.callbacks.lock().synthesis_begin = Some(Box::new(callback));
            Ok(())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Called when this speech synthesizer finishes synthesizing an utterance to audio.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFeature`] if the backend does not support synthesis.
    #[instrument(level = "debug", skip(self, callback), err)]
    pub fn on_synthesis_complete(&self, callback: impl UtteranceCallback) -> Result<(), Error> {
        let Features { synthesis, .. } = self.supported_features();
        if synthesis {
            self.callbacks.lock().synthesis_complete = Some(Box::new(callback));
            Ok(())
        } else {
            Err(Error::UnsupportedFeature)
        }
    }

    /// Clears all registered synthesis callbacks.
    #[instrument(level = "debug", skip(self))]
    pub fn clear_synthesis_callbacks(&self) {
        let mut callbacks = self.callbacks.lock();
        callbacks.synthesis_begin = None;
        callbacks.synthesis_complete = None;
    }

    /// Returns `true` if a screen reader is available to provide speech.
    #[instrument(level = "debug", ret)]
    #[must_use]
    pub fn screen_reader_available() -> bool {
        #[cfg(windows)]
        {
            if backends::Nvda::is_available() {
                return true;
            }
            #[cfg(feature = "tolk")]
            if backends::Tolk::is_available() {
                return true;
            }
            false
        }
        #[cfg(target_os = "linux")]
        {
            backends::a11y_screen_reader_enabled()
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            false
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
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
    pub fn id(&self) -> &str {
        &self.id
    }

    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn gender(&self) -> Option<Gender> {
        self.gender
    }

    #[instrument(level = "trace", skip(self))]
    #[must_use]
    pub fn language(&self) -> &LanguageTag<String> {
        &self.language
    }
}
