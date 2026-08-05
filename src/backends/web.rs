#[cfg(target_arch = "wasm32")]
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use oxilangtag::LanguageTag;
use parking_lot::Mutex;
use tracing::{Span, info_span, instrument};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    SpeechSynthesisErrorCode, SpeechSynthesisErrorEvent, SpeechSynthesisEvent,
    SpeechSynthesisUtterance, SpeechSynthesisVoice,
};

use crate::{Backend, Callbacks, Error, Features, UtteranceId, Voice};

#[derive(Clone, Debug)]
pub struct Web {
    callbacks: Arc<Mutex<Callbacks>>,
    rate: f32,
    pitch: f32,
    volume: f32,
    voice: Option<SpeechSynthesisVoice>,
    // Utterance events fire from the browser's event loop; entering this span there connects
    // them back to the backend that queued the utterance.
    span: Span,
}

static NEXT_BACKEND_ID: AtomicU64 = AtomicU64::new(0);
static NEXT_UTTERANCE_ID: AtomicU64 = AtomicU64::new(0);

impl Web {
    // Construction can't fail here, but backend constructors share a fallible signature.
    #[allow(clippy::unnecessary_wraps)]
    #[instrument(level = "info", skip(callbacks), err)]
    pub fn new(callbacks: Arc<Mutex<Callbacks>>) -> Result<Self, Error> {
        let id = NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed);
        Ok(Web {
            callbacks,
            rate: 1.,
            pitch: 1.,
            volume: 1.,
            voice: None,
            span: info_span!("web", backend_id = id),
        })
    }
}

impl Backend for Web {
    #[instrument(level = "trace", skip(self))]
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: true,
            pitch: true,
            volume: true,
            is_speaking: true,
            voice: true,
            get_voice: true,
            utterance_callbacks: true,
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        let utterance = SpeechSynthesisUtterance::new_with_text(text).unwrap();
        utterance.set_rate(self.rate);
        utterance.set_pitch(self.pitch);
        utterance.set_volume(self.volume);
        if self.voice.is_some() {
            utterance.set_voice(self.voice.as_ref());
        }
        let utterance_id = UtteranceId::Web(NEXT_UTTERANCE_ID.fetch_add(1, Ordering::Relaxed));
        let callback = Closure::wrap(Box::new({
            let callbacks = self.callbacks.clone();
            let span = self.span.clone();
            move |_evt: SpeechSynthesisEvent| {
                let _entered = span.enter();
                callbacks.lock().utterance_begin(utterance_id);
            }
        }) as Box<dyn Fn(_)>);
        utterance.set_onstart(Some(callback.as_ref().unchecked_ref()));
        let callback = Closure::wrap(Box::new({
            let callbacks = self.callbacks.clone();
            let span = self.span.clone();
            move |_evt: SpeechSynthesisEvent| {
                let _entered = span.enter();
                callbacks.lock().utterance_end(utterance_id);
            }
        }) as Box<dyn Fn(_)>);
        utterance.set_onend(Some(callback.as_ref().unchecked_ref()));
        let callback = Closure::wrap(Box::new({
            let callbacks = self.callbacks.clone();
            let span = self.span.clone();
            move |evt: SpeechSynthesisErrorEvent| {
                let _entered = span.enter();
                if evt.error() == SpeechSynthesisErrorCode::Canceled {
                    callbacks.lock().utterance_stop(utterance_id);
                }
            }
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
            Err(Error::BackendUnavailable("no window object"))
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn stop(&mut self) -> Result<(), Error> {
        if let Some(window) = web_sys::window() {
            let speech_synthesis = window.speech_synthesis().unwrap();
            speech_synthesis.cancel();
        }
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn min_rate(&self) -> f32 {
        0.1
    }

    #[instrument(level = "trace", skip(self))]
    fn max_rate(&self) -> f32 {
        10.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_rate(&self) -> f32 {
        1.
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_rate(&self) -> Result<f32, Error> {
        Ok(self.rate)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        self.rate = rate;
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn min_pitch(&self) -> f32 {
        0.
    }

    #[instrument(level = "trace", skip(self))]
    fn max_pitch(&self) -> f32 {
        2.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_pitch(&self) -> f32 {
        1.
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_pitch(&self) -> Result<f32, Error> {
        Ok(self.pitch)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        self.pitch = pitch;
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn min_volume(&self) -> f32 {
        0.
    }

    #[instrument(level = "trace", skip(self))]
    fn max_volume(&self) -> f32 {
        1.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_volume(&self) -> f32 {
        1.
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_volume(&self) -> Result<f32, Error> {
        Ok(self.volume)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        self.volume = volume;
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_speaking(&self) -> Result<bool, Error> {
        if let Some(window) = web_sys::window() {
            match window.speech_synthesis() {
                Ok(speech_synthesis) => Ok(speech_synthesis.speaking()),
                Err(e) => Err(Error::JavaScriptError(e)),
            }
        } else {
            Err(Error::BackendUnavailable("no window object"))
        }
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn voice(&self) -> Result<Option<Voice>, Error> {
        if let Some(voice) = &self.voice {
            Ok(Some(voice.clone().into()))
        } else {
            if let Some(window) = web_sys::window() {
                let speech_synthesis = window.speech_synthesis().unwrap();
                for voice in speech_synthesis.get_voices().iter() {
                    let voice: SpeechSynthesisVoice = voice.into();
                    if voice.default() {
                        return Ok(Some(voice.into()));
                    }
                }
            } else {
                return Err(Error::BackendUnavailable("no window object"));
            }
            Ok(None)
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn voices(&self) -> Result<Vec<Voice>, Error> {
        if let Some(window) = web_sys::window() {
            let speech_synthesis = window.speech_synthesis().unwrap();
            let mut rv: Vec<Voice> = vec![];
            for v in speech_synthesis.get_voices().iter() {
                let v: SpeechSynthesisVoice = v.into();
                rv.push(v.into());
            }
            Ok(rv)
        } else {
            Err(Error::BackendUnavailable("no window object"))
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_voice(&mut self, voice: &Voice) -> Result<(), Error> {
        if let Some(window) = web_sys::window() {
            let speech_synthesis = window.speech_synthesis().unwrap();
            for v in speech_synthesis.get_voices().iter() {
                let v: SpeechSynthesisVoice = v.into();
                if v.voice_uri() == voice.id {
                    self.voice = Some(v);
                    return Ok(());
                }
            }
            Err(Error::VoiceNotFound(voice.id.clone()))
        } else {
            Err(Error::BackendUnavailable("no window object"))
        }
    }
}

impl From<SpeechSynthesisVoice> for Voice {
    fn from(other: SpeechSynthesisVoice) -> Self {
        let language = LanguageTag::parse(other.lang()).unwrap();
        Voice {
            id: other.voice_uri(),
            name: other.name(),
            gender: None,
            language,
        }
    }
}
