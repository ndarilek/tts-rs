use std::boxed::Box;

mod backends;

pub enum Backends {
    #[cfg(target_os = "linux")]
    SpeechDispatcher,
}

trait Backend {
    fn speak(&self, text: &str, interrupt: bool);
    fn get_rate(&self) -> u8;
    fn set_rate(&self, rate: u8);
}

pub struct TTS(Box<Backend>);

impl TTS {
    pub fn new(backend: Backends) -> TTS {
        match backend {
            #[cfg(target_os = "linux")]
            Backends::SpeechDispatcher => TTS(Box::new(backends::SpeechDispatcher::new())),
        }
    }

    pub fn speak<S: Into<String>>(&self, text: S, interrupt: bool) -> &Self {
        self.0.speak(text.into().as_str(), interrupt);
        self
    }

    pub fn get_rate(&self) -> u8 {
        self.0.get_rate()
    }

    pub fn set_rate(&self, rate: u8) -> &Self {
        self.0.set_rate(rate);
        self
    }
}

impl Default for TTS {
    fn default() -> TTS {
        #[cfg(target_os = "linux")]
        TTS::new(Backends::SpeechDispatcher)
    }
}
