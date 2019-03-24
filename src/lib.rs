/*!
 * a Text-To-Speech (TTS) library providing high-level interfaces to a variety of backends.
 * Currently supported backends are:
 * *  [Speech Dispatcher](https://freebsoft.org/speechd) (Linux)
 * * WebAssembly
*/

use std::boxed::Box;
use std::convert;
use std::fmt;
use std::io;

use failure::Fail;

mod backends;

pub enum Backends {
    #[cfg(target_os = "linux")]
    SpeechDispatcher,
    #[cfg(target_arch = "wasm32")]
    Web,
}

#[derive(Debug, Fail)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.0)?;
        Ok(())
    }
}

impl convert::From<Error> for io::Error {
    fn from(e: Error) -> io::Error {
        io::Error::new(io::ErrorKind::Other, e.0)
    }
}

pub struct Features {
    stop: bool,
    rate: bool,
    pitch: bool,
    volume: bool,
}

pub trait Backend {
    fn supported_features(&self) -> Features;
    fn speak(&self, text: &str, interrupt: bool) -> Result<(), Error>;
    fn stop(&self) -> Result<(), Error>;
    fn get_rate(&self) -> Result<u8, Error>;
    fn set_rate(&mut self, rate: u8) -> Result<(), Error>;
    fn get_pitch(&self) -> Result<u8, Error>;
    fn set_pitch(&mut self, pitch: u8) -> Result<(), Error>;
    fn get_volume(&self) -> Result<u8, Error>;
    fn set_volume(&mut self, volume: u8) -> Result<(), Error>;
}

pub struct TTS(Box<Backend>);

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
        }
    }

    pub fn default() -> Result<TTS, Error> {
        #[cfg(target_os = "linux")]
        let tts = TTS::new(Backends::SpeechDispatcher);
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
            Err(Error("Feature not supported".to_string()))
        }
    }

    /**
     * Gets the current speech rate.
     */
    pub fn get_rate(&self) -> Result<u8, Error> {
        let Features { rate, .. } = self.supported_features();
        if rate {
            self.0.get_rate()
        } else {
            Err(Error("Feature not supported".to_string()))
        }
    }

    /**
     * Sets the desired speech rate.
     */
    pub fn set_rate(&mut self, rate: u8) -> Result<&Self, Error> {
        let Features { rate: rate_feature, .. } = self.supported_features();
        if rate_feature {
            self.0.set_rate(rate)?;
            Ok(self)
        } else {
            Err(Error("Unsupported feature".to_string()))
        }
    }

    /**
     * Gets the current speech pitch.
     */
    pub fn get_pitch(&self) -> Result<u8, Error> {
        let Features { pitch, .. } = self.supported_features();
        if pitch {
            self.0.get_pitch()
        } else {
            Err(Error("Feature not supported".to_string()))
        }
    }

    /**
     * Sets the desired speech pitch.
     */
    pub fn set_pitch(&mut self, pitch: u8) -> Result<&Self, Error> {
        let Features { pitch: pitch_feature, .. } = self.supported_features();
        if pitch_feature {
            self.0.set_pitch(pitch)?;
            Ok(self)
        } else {
            Err(Error("Unsupported feature".to_string()))
        }
    }

    /**
     * Gets the current speech volume.
     */
    pub fn get_volume(&self) -> Result<u8, Error> {
        let Features { volume, .. } = self.supported_features();
        if volume {
            self.0.get_volume()
        } else {
            Err(Error("Unsupported feature".to_string()))
        }
    }

    /**
     * Sets the desired speech volume.
     */
    pub fn set_volume(&mut self, volume: u8) -> Result<&Self, Error> {
        let Features { volume: volume_feature, .. } = self.supported_features();
        if volume_feature {
            self.0.set_volume(volume)?;
            Ok(self)
        } else {
            Err(Error("Unsupported feature".to_string()))
        }
    }
}
