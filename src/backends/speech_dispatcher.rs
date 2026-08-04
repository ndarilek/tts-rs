#[cfg(target_os = "linux")]
use std::sync::Arc;

use log::{info, trace};
use oxilangtag::LanguageTag;
use parking_lot::Mutex;
use speech_dispatcher::{Connection, Mode, Priority, Punctuation};

use crate::{Backend, BackendId, Callbacks, Error, Features, UtteranceId, Voice};

#[derive(Clone, Debug)]
pub(crate) struct SpeechDispatcher {
    connection: Connection,
    speaking: Arc<Mutex<bool>>,
}

impl SpeechDispatcher {
    pub(crate) fn new(callbacks: &Arc<Mutex<Callbacks>>) -> std::result::Result<Self, Error> {
        info!("Initializing SpeechDispatcher backend");
        let connection = Connection::open("tts", "tts", "tts", Mode::Threaded)?;
        let sd = SpeechDispatcher {
            connection,
            speaking: Arc::new(Mutex::new(false)),
        };
        sd.connection.on_begin(Some(Box::new({
            let speaking = sd.speaking.clone();
            let callbacks = callbacks.clone();
            move |msg_id, _client_id| {
                *speaking.lock() = true;
                callbacks
                    .lock()
                    .utterance_begin(UtteranceId::SpeechDispatcher(msg_id as u64));
            }
        })));
        sd.connection.on_end(Some(Box::new({
            let speaking = sd.speaking.clone();
            let callbacks = callbacks.clone();
            move |msg_id, _client_id| {
                *speaking.lock() = false;
                callbacks
                    .lock()
                    .utterance_end(UtteranceId::SpeechDispatcher(msg_id as u64));
            }
        })));
        sd.connection.on_cancel(Some(Box::new({
            let speaking = sd.speaking.clone();
            let callbacks = callbacks.clone();
            move |msg_id, _client_id| {
                *speaking.lock() = false;
                callbacks
                    .lock()
                    .utterance_stop(UtteranceId::SpeechDispatcher(msg_id as u64));
            }
        })));
        sd.connection.on_pause(Some(Box::new({
            let speaking = sd.speaking.clone();
            move |_msg_id, _client_id| {
                *speaking.lock() = false;
            }
        })));
        sd.connection.on_resume(Some(Box::new({
            let speaking = sd.speaking.clone();
            move |_msg_id, _client_id| {
                *speaking.lock() = true;
            }
        })));
        Ok(sd)
    }
}

impl Backend for SpeechDispatcher {
    fn id(&self) -> Option<BackendId> {
        Some(BackendId::SpeechDispatcher(self.connection.client_id()))
    }

    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: true,
            pitch: true,
            volume: true,
            is_speaking: true,
            voice: true,
            get_voice: false,
            utterance_callbacks: true,
        }
    }

    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        trace!("speak({text}, {interrupt})");
        if interrupt {
            self.stop()?;
        }
        let single_char = text.to_string().capacity() == 1;
        if single_char {
            self.connection.set_punctuation(Punctuation::All)?;
        }
        let id = self.connection.say(Priority::Important, text);
        if single_char {
            self.connection.set_punctuation(Punctuation::None)?;
        }
        if let Some(id) = id {
            Ok(Some(UtteranceId::SpeechDispatcher(id)))
        } else {
            Err(Error::NoneError)
        }
    }

    fn stop(&mut self) -> Result<(), Error> {
        trace!("stop()");
        self.connection.cancel()?;
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

    // Rates are in [-100, 100], well within both types' exact ranges.
    #[allow(clippy::cast_precision_loss)]
    fn get_rate(&self) -> Result<f32, Error> {
        Ok(self.connection.get_voice_rate() as f32)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        self.connection.set_voice_rate(rate as i32)?;
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

    // Pitches are in [-100, 100], well within both types' exact ranges.
    #[allow(clippy::cast_precision_loss)]
    fn get_pitch(&self) -> Result<f32, Error> {
        Ok(self.connection.get_voice_pitch() as f32)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        self.connection.set_voice_pitch(pitch as i32)?;
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

    // Volumes are in [-100, 100], well within both types' exact ranges.
    #[allow(clippy::cast_precision_loss)]
    fn get_volume(&self) -> Result<f32, Error> {
        Ok(self.connection.get_volume() as f32)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        self.connection.set_volume(volume as i32)?;
        Ok(())
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        Ok(*self.speaking.lock())
    }

    fn voices(&self) -> Result<Vec<Voice>, Error> {
        let rv = self
            .connection
            .list_synthesis_voices()?
            .iter()
            .filter(|v| LanguageTag::parse(v.language.clone()).is_ok())
            .map(|v| Voice {
                id: v.name.clone(),
                name: v.name.clone(),
                gender: None,
                language: LanguageTag::parse(v.language.clone()).unwrap(),
            })
            .collect::<Vec<Voice>>();
        Ok(rv)
    }

    fn voice(&self) -> Result<Option<Voice>, Error> {
        unimplemented!()
    }

    fn set_voice(&mut self, voice: &Voice) -> Result<(), Error> {
        for v in self.connection.list_synthesis_voices()? {
            if v.name == voice.name {
                self.connection.set_synthesis_voice(&v)?;
                return Ok(());
            }
        }
        Err(Error::OperationFailed)
    }
}
