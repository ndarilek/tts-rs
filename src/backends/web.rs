#[cfg(target_arch = "wasm32")]
use log::{info, trace};
use web_sys::SpeechSynthesisUtterance;

use crate::{Backend, Error, Features};

pub struct Web {
    rate: f32,
    pitch: f32,
    volume: f32,
}

impl Web {
    pub fn new() -> Result<Self, Error> {
        info!("Initializing Web backend");
        Ok(Web {
            rate: 1.,
            pitch: 1.,
            volume: 1.,
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
            is_speaking: true,
        }
    }

    fn speak(&mut self, text: &str, interrupt: bool) -> Result<(), Error> {
        trace!("speak({}, {})", text, interrupt);
        let utterance = SpeechSynthesisUtterance::new_with_text(text).unwrap();
        utterance.set_rate(self.rate);
        utterance.set_pitch(self.pitch);
        utterance.set_volume(self.volume);
        if interrupt {
            self.stop()?;
        }
        if let Some(window) = web_sys::window() {
            let speech_synthesis = window.speech_synthesis().unwrap();
            speech_synthesis.speak(&utterance);
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Error> {
        trace!("stop()");
        if let Some(window) = web_sys::window() {
            let speech_synthesis = window.speech_synthesis().unwrap();
            speech_synthesis.cancel();
        }
        Ok(())
    }

    fn min_rate(&self) -> f32 {
        0.1
    }

    fn max_rate(&self) -> f32 {
        10.
    }

    fn normal_rate(&self) -> f32 {
        1.
    }

    fn get_rate(&self) -> Result<f32, Error> {
        Ok(self.rate)
    }

    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        self.rate = rate;
        Ok(())
    }

    fn min_pitch(&self) -> f32 {
        0.
    }

    fn max_pitch(&self) -> f32 {
        2.
    }

    fn normal_pitch(&self) -> f32 {
        1.
    }

    fn get_pitch(&self) -> Result<f32, Error> {
        Ok(self.pitch)
    }

    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        self.pitch = pitch;
        Ok(())
    }

    fn min_volume(&self) -> f32 {
        0.
    }

    fn max_volume(&self) -> f32 {
        1.
    }

    fn normal_volume(&self) -> f32 {
        1.
    }

    fn get_volume(&self) -> Result<f32, Error> {
        Ok(self.volume)
    }

    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        self.volume = volume;
        Ok(())
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        trace!("is_speaking()");
        if let Some(window) = web_sys::window() {
            match window.speech_synthesis() {
                Ok(speech_synthesis) => Ok(speech_synthesis.speaking()),
                Err(e) => Err(Error::JavaScriptError(e)),
            }
        } else {
            Err(Error::NoneError)
        }
    }
}
