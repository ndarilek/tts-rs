#[cfg(target_os = "linux")]

use std::u8;

use log::{info, trace};
use speech_dispatcher::*;

use crate::{Backend, Error};

pub struct SpeechDispatcher(Connection);

impl SpeechDispatcher {
    pub fn new() -> impl Backend {
        info!("Initializing SpeechDispatcher backend");
        let connection = speech_dispatcher::Connection::open("tts", "tts", "tts", Mode::Single);
        SpeechDispatcher(connection)
    }
}

fn u8_to_i32(v: u8) -> i32 {
    let ratio: f32 = v as f32/u8::MAX as f32;
    (ratio * 200. - 100.) as i32
}

fn i32_to_u8(v: i32) -> u8 {
    let v = v as f32;
    let ratio: f32 = (v + 100.) / 200.;
    let v = ratio * u8::MAX as f32;
    v as u8
}

impl Backend for SpeechDispatcher {
    fn speak(&self, text: &str, interrupt: bool) -> Result<(), Error> {
        trace!("speak({}, {})", text, interrupt);
        if interrupt {
            self.stop()?;
        }
        self.0.say(Priority::Important, text);
        Ok(())
    }

    fn stop(&self) -> Result<(), Error> {
        trace!("stop()");
        self.0.cancel();
        Ok(())
    }

    fn get_rate(&self) -> Result<u8, Error> {
        Ok(i32_to_u8(self.0.get_voice_rate()))
    }

    fn set_rate(&mut self, rate: u8) -> Result<(), Error> {
        self.0.set_voice_rate(u8_to_i32(rate));
        Ok(())
    }

    fn get_pitch(&self) -> Result<u8, Error> {
        Ok(i32_to_u8(self.0.get_voice_pitch()))
    }

    fn set_pitch(&mut self, pitch: u8) -> Result<(), Error> {
        self.0.set_voice_pitch(u8_to_i32(pitch));
        Ok(())
    }

    fn get_volume(&self) -> Result<u8, Error> {
        Ok(i32_to_u8(self.0.get_volume()))
    }

    fn set_volume(&mut self, volume: u8) -> Result<(), Error> {
        self.0.set_volume(u8_to_i32(volume));
        Ok(())
    }
}
