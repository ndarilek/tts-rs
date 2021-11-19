#[cfg(target_os = "linux")]
use std::{collections::HashMap, sync::Mutex};

use lazy_static::*;
use log::{info, trace};
use speech_dispatcher::*;

use crate::{Backend, BackendId, Error, Features, UtteranceId, CALLBACKS};

#[derive(Clone, Debug)]
pub(crate) struct SpeechDispatcher(Connection);

lazy_static! {
    static ref SPEAKING: Mutex<HashMap<u64, bool>> = {
        let m: HashMap<u64, bool> = HashMap::new();
        Mutex::new(m)
    };
}

impl SpeechDispatcher {
    pub(crate) fn new() -> std::result::Result<Self, Error> {
        info!("Initializing SpeechDispatcher backend");
        let connection = speech_dispatcher::Connection::open("tts", "tts", "tts", Mode::Threaded)?;
        let sd = SpeechDispatcher(connection);
        let mut speaking = SPEAKING.lock().unwrap();
        speaking.insert(sd.0.client_id(), false);
        sd.0.on_begin(Some(Box::new(|msg_id, client_id| {
            let mut speaking = SPEAKING.lock().unwrap();
            speaking.insert(client_id, true);
            let mut callbacks = CALLBACKS.lock().unwrap();
            let backend_id = BackendId::SpeechDispatcher(client_id);
            let cb = callbacks.get_mut(&backend_id).unwrap();
            let utterance_id = UtteranceId::SpeechDispatcher(msg_id);
            if let Some(f) = cb.utterance_begin.as_mut() {
                f(utterance_id);
            }
        })));
        sd.0.on_end(Some(Box::new(|msg_id, client_id| {
            let mut speaking = SPEAKING.lock().unwrap();
            speaking.insert(client_id, false);
            let mut callbacks = CALLBACKS.lock().unwrap();
            let backend_id = BackendId::SpeechDispatcher(client_id);
            let cb = callbacks.get_mut(&backend_id).unwrap();
            let utterance_id = UtteranceId::SpeechDispatcher(msg_id);
            if let Some(f) = cb.utterance_end.as_mut() {
                f(utterance_id);
            }
        })));
        sd.0.on_cancel(Some(Box::new(|msg_id, client_id| {
            let mut speaking = SPEAKING.lock().unwrap();
            speaking.insert(client_id, false);
            let mut callbacks = CALLBACKS.lock().unwrap();
            let backend_id = BackendId::SpeechDispatcher(client_id);
            let cb = callbacks.get_mut(&backend_id).unwrap();
            let utterance_id = UtteranceId::SpeechDispatcher(msg_id);
            if let Some(f) = cb.utterance_stop.as_mut() {
                f(utterance_id);
            }
        })));
        sd.0.on_pause(Some(Box::new(|_msg_id, client_id| {
            let mut speaking = SPEAKING.lock().unwrap();
            speaking.insert(client_id, false);
        })));
        sd.0.on_resume(Some(Box::new(|_msg_id, client_id| {
            let mut speaking = SPEAKING.lock().unwrap();
            speaking.insert(client_id, true);
        })));
        Ok(sd)
    }
}

impl Backend for SpeechDispatcher {
    fn id(&self) -> Option<BackendId> {
        Some(BackendId::SpeechDispatcher(self.0.client_id()))
    }

    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: true,
            pitch: true,
            volume: true,
            is_speaking: true,
            utterance_callbacks: true,
        }
    }

    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        trace!("speak({}, {})", text, interrupt);
        if interrupt {
            self.stop()?;
        }
        let single_char = text.to_string().capacity() == 1;
        if single_char {
            self.0.set_punctuation(Punctuation::All);
        }
        let id = self.0.say(Priority::Important, text);
        if single_char {
            self.0.set_punctuation(Punctuation::None);
        }
        if let Some(id) = id {
            Ok(Some(UtteranceId::SpeechDispatcher(id)))
        } else {
            Err(Error::NoneError)
        }
    }

    fn stop(&mut self) -> Result<(), Error> {
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
        100.
    }

    fn get_volume(&self) -> Result<f32, Error> {
        Ok(self.0.get_volume() as f32)
    }

    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        self.0.set_volume(volume as i32);
        Ok(())
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        let speaking = SPEAKING.lock().unwrap();
        let is_speaking = speaking.get(&self.0.client_id()).unwrap();
        Ok(*is_speaking)
    }
}

impl Drop for SpeechDispatcher {
    fn drop(&mut self) {
        let mut speaking = SPEAKING.lock().unwrap();
        speaking.remove(&self.0.client_id());
    }
}
