#[cfg(windows)]
use log::{info, trace};
use tolk::Tolk as TolkPtr;

use crate::{Backend, Error, Features};

pub struct Tolk(TolkPtr);

impl Tolk {
    pub fn new() -> Self {
        info!("Initializing Tolk backend");
        let tolk = TolkPtr::new();
        tolk.try_sapi(true);
        Tolk(tolk)
    }
}

impl Backend for Tolk {
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: false,
            pitch: false,
            volume: false,
            is_speaking: false,
        }
    }

    fn speak(&self, text: &str, interrupt: bool) -> Result<(), Error> {
        trace!("speak({}, {})", text, interrupt);
        self.0.speak(text, interrupt);
        Ok(())
    }

    fn stop(&self) -> Result<(), Error> {
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
