/*!
 * a Text-To-Speech (TTS) library providing high-level interfaces to a variety of backends.
 * Currently supported backends are:
 * * [Speech Dispatcher](https://freebsoft.org/speechd) (Linux)
 * * Windows screen readers and SAPI via [Tolk](https://github.com/dkager/tolk/)
 * * WebAssembly
*/

use std::boxed::Box;

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
}

pub struct Features {
    pub stop: bool,
    pub rate: bool,
    pub pitch: bool,
    pub volume: bool,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
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
    fn speak(&self, text: &str, interrupt: bool) -> Result<(), Error>;
    fn stop(&self) -> Result<(), Error>;
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
                Ok(TTS(Box::new(tts)))
            }
            #[cfg(windows)]
            Backends::WinRT => {
                let tts = backends::winrt::WinRT::new()?;
                Ok(TTS(Box::new(tts)))
            }
        }
    }

    pub fn default() -> Result<TTS, Error> {
        #[cfg(target_os = "linux")]
        let tts = TTS::new(Backends::SpeechDispatcher);
        #[cfg(windows)]
        let tts = {
            let tolk = tolk::Tolk::new();
            if tolk.detect_screen_reader().is_some() {
                TTS::new(Backends::Tolk)
            } else {
                TTS::new(Backends::WinRT)
            }
        };
        #[cfg(target_arch = "wasm32")]
        let tts = TTS::new(Backends::Web);
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
    pub fn speak<S: Into<String>>(&self, text: S, interrupt: bool) -> Result<&Self, Error> {
        self.0.speak(text.into().as_str(), interrupt)?;
        Ok(self)
    }

    /**
     * Stops current speech.
     */
    pub fn stop(&self) -> Result<&Self, Error> {
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
}
