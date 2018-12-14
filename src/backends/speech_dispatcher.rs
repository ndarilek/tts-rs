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

impl Backend for SpeechDispatcher {
    fn speak(&self, text: &str, interrupt: bool) {
        trace!("speak({}, {})", text, interrupt);
        if interrupt {
            self.0.cancel();
        }
        self.0.say(Priority::Important, text);
    }

    fn get_rate(&self) -> u8 {
        let rate = self.0.get_voice_rate() as f32;
        trace!("get_voice_rate() = {}", rate);
        let ratio: f32 = (rate + 100.) / 200.;
        trace!("ratio = {}", ratio);
        let rate = ratio * u8::MAX as f32;
        trace!("rate = {}", rate);
        rate as u8
    }

    fn set_rate(&self, rate: u8) {
        trace!("set_rate({})", rate);
        let ratio: f32 = rate as f32/u8::MAX as f32;
        let rate = ratio * 200. - 100.;
        self.0.set_voice_rate(rate as i32);
    }
}
