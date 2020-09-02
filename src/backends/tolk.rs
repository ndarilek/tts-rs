#[cfg(windows)]
use log::{info, trace};
use tolk::Tolk as TolkPtr;

use crate::{Backend, Error, Features};

pub struct Tolk(TolkPtr);

impl Tolk {
    pub fn new() -> Option<Self> {
        info!("Initializing Tolk backend");
        let tolk = TolkPtr::new();
        if tolk.detect_screen_reader().is_some() {
            Some(Tolk(tolk))
        } else {
            None
        }
    }
}

impl Backend for Tolk {
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            ..Default::default()
        }
    }

    fn speak(&mut self, text: &str, interrupt: bool) -> Result<(), Error> {
        trace!("speak({}, {})", text, interrupt);
        const BUFFER_LENGTH: usize = 300;
        if text.len() <= BUFFER_LENGTH {
            self.0.speak(text, interrupt);
        } else {
            if interrupt {
                self.stop()?;
            }
            let tokens = text.split_whitespace();
            let mut buffer = String::new();
            for token in tokens {
                if buffer.len() + token.len() > BUFFER_LENGTH {
                    self.0.speak(buffer, false);
                    buffer = String::new();
                } else {
                    buffer.push_str(token);
                    buffer.push(' ');
                }
            }
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Error> {
        trace!("stop()");
        self.0.silence();
        Ok(())
    }

    fn min_rate(&self) -> f32 {
        unimplemented!()
    }

    fn max_rate(&self) -> f32 {
        unimplemented!()
    }

    fn normal_rate(&self) -> f32 {
        unimplemented!()
    }

    fn get_rate(&self) -> Result<f32, Error> {
        unimplemented!();
    }

    fn set_rate(&mut self, _rate: f32) -> Result<(), Error> {
        unimplemented!();
    }

    fn min_pitch(&self) -> f32 {
        unimplemented!()
    }

    fn max_pitch(&self) -> f32 {
        unimplemented!()
    }

    fn normal_pitch(&self) -> f32 {
        unimplemented!()
    }

    fn get_pitch(&self) -> Result<f32, Error> {
        unimplemented!();
    }

    fn set_pitch(&mut self, _pitch: f32) -> Result<(), Error> {
        unimplemented!();
    }

    fn min_volume(&self) -> f32 {
        unimplemented!()
    }

    fn max_volume(&self) -> f32 {
        unimplemented!()
    }

    fn normal_volume(&self) -> f32 {
        unimplemented!()
    }

    fn get_volume(&self) -> Result<f32, Error> {
        unimplemented!();
    }

    fn set_volume(&mut self, _volume: f32) -> Result<(), Error> {
        unimplemented!();
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        unimplemented!()
    }
}
