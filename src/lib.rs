/*!
 * a Text-To-Speech (TTS) library providing high-level interfaces to a variety of backends.
 * Currently supported backends are [Speech Dispatcher](https://freebsoft.org/speechd) (Linux).
*/

use std::boxed::Box;

mod backends;

pub enum Backends {
    #[cfg(target_os = "linux")]
    SpeechDispatcher,
}

trait Backend {
    fn speak(&self, text: &str, interrupt: bool);
    fn stop(&self);
    fn get_rate(&self) -> u8;
    fn set_rate(&self, rate: u8);
    fn get_pitch(&self) -> u8;
    fn set_pitch(&self, pitch: u8);
    fn get_volume(&self) -> u8;
    fn set_volume(&self, volume: u8);
}

pub struct TTS(Box<Backend>);

impl TTS {

    /**
     * Create a new `TTS` instance with the specified backend.
    */
    pub fn new(backend: Backends) -> TTS {
        match backend {
            #[cfg(target_os = "linux")]
            Backends::SpeechDispatcher => TTS(Box::new(backends::SpeechDispatcher::new())),
        }
    }

    /**
     * Speaks the specified text, optionally interrupting current speech.
    */
    pub fn speak<S: Into<String>>(&self, text: S, interrupt: bool) -> &Self {
        self.0.speak(text.into().as_str(), interrupt);
        self
    }

    /**
     * Stops current speech.
    */
    pub fn stop(&self) -> &Self {
        self.0.stop();
        self
    }

    /**
     * Gets the current speech rate.
    */
    pub fn get_rate(&self) -> u8 {
        self.0.get_rate()
    }

    /**
     * Sets the desired speech rate.
    */
    pub fn set_rate(&self, rate: u8) -> &Self {
        self.0.set_rate(rate);
        self
    }

    /**
     * Gets the current speech pitch.
    */
    pub fn get_pitch(&self) -> u8 {
        self.0.get_pitch()
    }

    /**
     * Sets the desired speech pitch.
    */
    pub fn set_pitch(&self, pitch: u8) -> &Self {
        self.0.set_pitch(pitch);
        self
    }

    /**
     * Gets the current speech volume.
    */
    pub fn get_volume(&self) -> u8 {
        self.0.get_volume()
    }

    /**
     * Sets the desired speech volume.
    */
    pub fn set_volume(&self, volume: u8) -> &Self {
        self.0.set_volume(volume);
        self
    }
}

impl Default for TTS {
    fn default() -> TTS {
        #[cfg(target_os = "linux")]
        TTS::new(Backends::SpeechDispatcher)
    }
}
