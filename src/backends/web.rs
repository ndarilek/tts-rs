#[cfg(target_arch = "wasm32")]
use std::u8;

use log::{info, trace};
use web_sys::SpeechSynthesisUtterance;

use crate::{Backend, Error};

pub struct Web {
    rate: u8,
    pitch: u8,
    volume: u8,
}

impl Web {
    pub fn new() -> Result<impl Backend, Error> {
        info!("Initializing Web backend");
        Ok(Web {
            rate: 25,
            pitch: 127,
            volume: u8::MAX,
        })
    }
}

impl Backend for Web {
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: true,
            pitch: true,
            volume: true,
        }
    }

    fn speak(&self, text: &str, interrupt: bool) -> Result<(), Error> {
        trace!("speak({}, {})", text, interrupt);
        let utterance = SpeechSynthesisUtterance::new_with_text(text).unwrap();
        let mut rate: f32 = self.rate as f32;
        rate = rate / u8::MAX as f32 * 10.;
        utterance.set_rate(rate);
        let mut pitch: f32 = self.pitch as f32;
        pitch = pitch / u8::MAX as f32 * 2.;
        utterance.set_pitch(pitch);
        let mut volume: f32 = self.volume as f32;
        volume = volume / u8::MAX as f32 * 1.;
        utterance.set_volume(volume);
        if interrupt {
            self.stop()?;
        }
        if let Some(window) = web_sys::window() {
            let speech_synthesis = window.speech_synthesis().unwrap();
            speech_synthesis.speak(&utterance);
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), Error> {
        trace!("stop()");
        if let Some(window) = web_sys::window() {
            let speech_synthesis = window.speech_synthesis().unwrap();
            speech_synthesis.cancel();
        }
        Ok(())
    }

    fn get_rate(&self) -> Result<u8, Error> {
        Ok(self.rate)
    }

    fn set_rate(&mut self, rate: u8) -> Result<(), Error> {
        self.rate = rate;
        Ok(())
    }

    fn get_pitch(&self) -> Result<u8, Error> {
        Ok(self.pitch)
    }

    fn set_pitch(&mut self, pitch: u8) -> Result<(), Error> {
        self.pitch = pitch;
        Ok(())
    }

    fn get_volume(&self) -> Result<u8, Error> {
        Ok(self.volume)
    }

    fn set_volume(&mut self, volume: u8) -> Result<(), Error> {
        self.volume = volume;
        Ok(())
    }
}
