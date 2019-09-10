#[cfg(windows)]
use log::{info, trace};
use tolk::Tolk as TolkPtr;

use crate::{Backend, Error, Features};

pub struct Tolk(TolkPtr);

impl Tolk {
    pub fn new() -> impl Backend {
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

    fn get_rate(&self) -> Result<u8, Error> {
        unimplemented!();
    }

    fn set_rate(&mut self, _rate: u8) -> Result<(), Error> {
        unimplemented!();
    }

    fn get_pitch(&self) -> Result<u8, Error> {
        unimplemented!();
    }

    fn set_pitch(&mut self, _pitch: u8) -> Result<(), Error> {
        unimplemented!();
    }

    fn get_volume(&self) -> Result<u8, Error> {
        unimplemented!();
    }

    fn set_volume(&mut self, _volume: u8) -> Result<(), Error> {
        unimplemented!();
    }
}
