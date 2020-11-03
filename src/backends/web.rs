#[cfg(target_arch = "wasm32")]
use std::sync::Mutex;

use lazy_static::lazy_static;
use log::{info, trace};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    SpeechSynthesisErrorCode, SpeechSynthesisErrorEvent, SpeechSynthesisEvent,
    SpeechSynthesisUtterance,
};

use crate::{Backend, BackendId, Error, Features, UtteranceId, CALLBACKS};

#[derive(Clone)]
pub struct Web {
    id: BackendId,
    rate: f32,
    pitch: f32,
    volume: f32,
}

lazy_static! {
    static ref NEXT_BACKEND_ID: Mutex<u64> = Mutex::new(0);
    static ref UTTERANCE_MAPPINGS: Mutex<Vec<(BackendId, UtteranceId)>> = Mutex::new(Vec::new());
    static ref NEXT_UTTERANCE_ID: Mutex<u64> = Mutex::new(0);
}

impl Web {
    pub fn new() -> Result<Self, Error> {
        info!("Initializing Web backend");
        let mut backend_id = NEXT_BACKEND_ID.lock().unwrap();
        let rv = Web {
            id: BackendId::Web(*backend_id),
            rate: 1.,
            pitch: 1.,
            volume: 1.,
        };
        *backend_id += 1;
        Ok(rv)
    }
}

impl Backend for Web {
    fn id(&self) -> Option<BackendId> {
        Some(self.id)
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
        let utterance = SpeechSynthesisUtterance::new_with_text(text).unwrap();
        utterance.set_rate(self.rate);
        utterance.set_pitch(self.pitch);
        utterance.set_volume(self.volume);
        let id = self.id().unwrap();
        let mut uid = NEXT_UTTERANCE_ID.lock().unwrap();
        let utterance_id = UtteranceId::Web(*uid);
        *uid += 1;
        drop(uid);
        let mut mappings = UTTERANCE_MAPPINGS.lock().unwrap();
        mappings.push((self.id, utterance_id));
        drop(mappings);
        let callback = Closure::wrap(Box::new(move |_evt: SpeechSynthesisEvent| {
            let mut callbacks = CALLBACKS.lock().unwrap();
            let callback = callbacks.get_mut(&id).unwrap();
            if let Some(f) = callback.utterance_begin.as_mut() {
                f(utterance_id);
            }
        }) as Box<dyn Fn(_)>);
        utterance.set_onstart(Some(callback.as_ref().unchecked_ref()));
        let callback = Closure::wrap(Box::new(move |_evt: SpeechSynthesisEvent| {
            let mut callbacks = CALLBACKS.lock().unwrap();
            let callback = callbacks.get_mut(&id).unwrap();
            if let Some(f) = callback.utterance_end.as_mut() {
                f(utterance_id);
            }
            let mut mappings = UTTERANCE_MAPPINGS.lock().unwrap();
            mappings.retain(|v| v.1 != utterance_id);
        }) as Box<dyn Fn(_)>);
        utterance.set_onend(Some(callback.as_ref().unchecked_ref()));
        let callback = Closure::wrap(Box::new(move |evt: SpeechSynthesisErrorEvent| {
            if evt.error() == SpeechSynthesisErrorCode::Canceled {
                let mut callbacks = CALLBACKS.lock().unwrap();
                let callback = callbacks.get_mut(&id).unwrap();
                if let Some(f) = callback.utterance_stop.as_mut() {
                    f(utterance_id);
                }
            }
            let mut mappings = UTTERANCE_MAPPINGS.lock().unwrap();
            mappings.retain(|v| v.1 != utterance_id);
        }) as Box<dyn Fn(_)>);
        utterance.set_onerror(Some(callback.as_ref().unchecked_ref()));
        if interrupt {
            self.stop()?;
        }
        if let Some(window) = web_sys::window() {
            let speech_synthesis = window.speech_synthesis().unwrap();
            speech_synthesis.speak(&utterance);
            Ok(Some(utterance_id))
        } else {
            Err(Error::NoneError)
        }
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

impl Drop for Web {
    fn drop(&mut self) {
        let mut mappings = UTTERANCE_MAPPINGS.lock().unwrap();
        mappings.retain(|v| v.0 != self.id);
    }
}
