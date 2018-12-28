#[cfg(target_os = "linux")]

use std::u8;

use log::{info, trace};
use speech_dispatcher::*;

use crate::Backend;

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
    fn speak(&self, text: &str, interrupt: bool) {
        trace!("speak({}, {})", text, interrupt);
        if interrupt {
            self.0.cancel();
        }
        self.0.say(Priority::Important, text);
    }

    fn stop(&self) {
        trace!("stop()");
        self.0.cancel();
    }

    fn get_rate(&self) -> u8 {
        i32_to_u8(self.0.get_voice_rate())
    }

    fn set_rate(&self, rate: u8) {
        self.0.set_voice_rate(u8_to_i32(rate));
    }

    fn get_pitch(&self) -> u8 {
        i32_to_u8(self.0.get_voice_pitch())
    }

    fn set_pitch(&self, pitch: u8) {
        self.0.set_voice_pitch(u8_to_i32(pitch));
    }

    fn get_volume(&self) -> u8 {
        i32_to_u8(self.0.get_volume())
    }

    fn set_volume(&self, volume: u8) {
        self.0.set_volume(u8_to_i32(volume));
    }
}
