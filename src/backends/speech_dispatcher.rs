#[cfg(target_os = "linux")]
use log::{info, trace};
use speech_dispatcher::*;

use crate::{Backend, Error, Features};

pub struct SpeechDispatcher(Connection);

impl SpeechDispatcher {
    pub fn new() -> Self {
        info!("Initializing SpeechDispatcher backend");
        let connection = speech_dispatcher::Connection::open("tts", "tts", "tts", Mode::Single);
        SpeechDispatcher(connection)
    }
}

impl Backend for SpeechDispatcher {
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: true,
            pitch: true,
            volume: true,
            is_speaking: false,
        }
    }

    fn speak(&self, text: &str, interrupt: bool) -> Result<(), Error> {
        trace!("speak({}, {})", text, interrupt);
        if interrupt {
            self.stop()?;
        }
        let single_char = text.to_string().capacity() == 1;
        if single_char {
            self.0.set_punctuation(Punctuation::All);
        }
        self.0.say(Priority::Important, text);
        if single_char {
            self.0.set_punctuation(Punctuation::None);
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), Error> {
        trace!("stop()");
        self.0.cancel();
        Ok(())
    }

    fn min_rate(&self) -> f32 {
        -100.
    }

    fn max_rate(&self) -> f32 {
        100.
    }

    fn normal_rate(&self) -> f32 {
        0.
    }

    fn get_rate(&self) -> Result<f32, Error> {
        Ok(self.0.get_voice_rate() as f32)
    }

    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        self.0.set_voice_rate(rate as i32);
        Ok(())
    }

    fn min_pitch(&self) -> f32 {
        -100.
    }

    fn max_pitch(&self) -> f32 {
        100.
    }

    fn normal_pitch(&self) -> f32 {
        0.
    }

    fn get_pitch(&self) -> Result<f32, Error> {
        Ok(self.0.get_voice_pitch() as f32)
    }

    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        self.0.set_voice_pitch(pitch as i32);
        Ok(())
    }

    fn min_volume(&self) -> f32 {
        -100.
    }

    fn max_volume(&self) -> f32 {
        100.
    }

    fn normal_volume(&self) -> f32 {
        0.
    }

    fn get_volume(&self) -> Result<f32, Error> {
        Ok(self.0.get_volume() as f32)
    }

    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        self.0.set_volume(volume as i32);
        Ok(())
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        unimplemented!()
    }
}
