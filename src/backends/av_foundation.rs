use std::sync::Mutex;

use lazy_static::lazy_static;
use log::{info, trace};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_avf_audio::{
    AVSpeechBoundary, AVSpeechSynthesisVoice, AVSpeechSynthesisVoiceGender, AVSpeechSynthesizer,
    AVSpeechSynthesizerDelegate, AVSpeechUtterance,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
use oxilangtag::LanguageTag;

use crate::{Backend, BackendId, Error, Features, Gender, UtteranceId, Voice, CALLBACKS};

#[derive(Debug)]
struct Ivars {
    backend_id: u64,
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super(NSObject))]
    #[name = "MyAVSpeechSynthesizerDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl AVSpeechSynthesizerDelegate for Delegate {
        #[unsafe(method(speechSynthesizer:didStartSpeechUtterance:))]
        fn speech_synthesizer_did_start_speech_utterance(
            &self,
            _synthesizer: &AVSpeechSynthesizer,
            utterance: &AVSpeechUtterance,
        ) {
            trace!("speech_synthesizer_did_start_speech_utterance");
            let backend_id = self.ivars().backend_id;
            let backend_id = BackendId::AvFoundation(backend_id);
            trace!("Locking callbacks");
            let mut callbacks = CALLBACKS.lock().unwrap();
            trace!("Locked");
            let callbacks = callbacks.get_mut(&backend_id).unwrap();
            if let Some(callback) = callbacks.utterance_begin.as_mut() {
                trace!("Calling utterance_begin");
                let utterance_id = UtteranceId::AvFoundation(utterance as *const _ as usize);
                callback(utterance_id);
                trace!("Called");
            }
            trace!("Done speech_synthesizer_did_start_speech_utterance");
        }

        #[unsafe(method(speechSynthesizer:didFinishSpeechUtterance:))]
        fn speech_synthesizer_did_finish_speech_utterance(
            &self,
            _synthesizer: &AVSpeechSynthesizer,
            utterance: &AVSpeechUtterance,
        ) {
            trace!("speech_synthesizer_did_finish_speech_utterance");
            let backend_id = self.ivars().backend_id;
            let backend_id = BackendId::AvFoundation(backend_id);
            trace!("Locking callbacks");
            let mut callbacks = CALLBACKS.lock().unwrap();
            trace!("Locked");
            let callbacks = callbacks.get_mut(&backend_id).unwrap();
            if let Some(callback) = callbacks.utterance_end.as_mut() {
                trace!("Calling utterance_end");
                let utterance_id = UtteranceId::AvFoundation(utterance as *const _ as usize);
                callback(utterance_id);
                trace!("Called");
            }
            trace!("Done speech_synthesizer_did_finish_speech_utterance");
        }

        #[unsafe(method(speechSynthesizer:didCancelSpeechUtterance:))]
        fn speech_synthesizer_did_cancel_speech_utterance(
            &self,
            _synthesizer: &AVSpeechSynthesizer,
            utterance: &AVSpeechUtterance,
        ) {
            trace!("speech_synthesizer_did_cancel_speech_utterance");
            let backend_id = self.ivars().backend_id;
            let backend_id = BackendId::AvFoundation(backend_id);
            trace!("Locking callbacks");
            let mut callbacks = CALLBACKS.lock().unwrap();
            trace!("Locked");
            let callbacks = callbacks.get_mut(&backend_id).unwrap();
            if let Some(callback) = callbacks.utterance_stop.as_mut() {
                trace!("Calling utterance_stop");
                let utterance_id = UtteranceId::AvFoundation(utterance as *const _ as usize);
                callback(utterance_id);
                trace!("Called");
            }
            trace!("Done speech_synthesizer_did_cancel_speech_utterance");
        }
    }
);

#[derive(Clone, Debug)]
pub(crate) struct AvFoundation {
    id: BackendId,
    /// Kept around to avoid deallocting before we're done.
    _delegate: Retained<Delegate>,
    synth: Retained<AVSpeechSynthesizer>,
    rate: f32,
    volume: f32,
    pitch: f32,
    voice: Option<Voice>,
}

lazy_static! {
    static ref NEXT_BACKEND_ID: Mutex<u64> = Mutex::new(0);
}

impl AvFoundation {
    pub(crate) fn new() -> Result<Self, Error> {
        info!("Initializing AVFoundation backend");

        let mut backend_id = NEXT_BACKEND_ID.lock().unwrap();

        trace!("Creating synth");
        let synth = unsafe { AVSpeechSynthesizer::new() };
        trace!("Creating delegate");
        let delegate = Delegate::alloc().set_ivars(Ivars {
            backend_id: *backend_id,
        });
        let delegate: Retained<Delegate> = unsafe { msg_send![super(delegate), init] };
        trace!("Assigning delegate");
        unsafe { synth.setDelegate(Some(ProtocolObject::from_ref(&*delegate))) };

        let rv = AvFoundation {
            id: BackendId::AvFoundation(*backend_id),
            _delegate: delegate,
            synth,
            rate: 0.5,
            volume: 1.,
            pitch: 1.,
            voice: None,
        };
        *backend_id += 1;
        Ok(rv)
    }
}

impl Backend for AvFoundation {
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
            voice: true,
            get_voice: false,
            utterance_callbacks: true,
        }
    }

    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        trace!("speak({}, {})", text, interrupt);
        if interrupt && self.is_speaking()? {
            self.stop()?;
        }
        let utterance;
        unsafe {
            trace!("Creating utterance string");
            let str = NSString::from_str(text);
            trace!("Creating utterance");
            utterance = AVSpeechUtterance::initWithString(AVSpeechUtterance::alloc(), &str);
            trace!("Setting rate to {}", self.rate);
            utterance.setRate(self.rate);
            trace!("Setting volume to {}", self.volume);
            utterance.setVolume(self.volume);
            trace!("Setting pitch to {}", self.pitch);
            utterance.setPitchMultiplier(self.pitch);
            if let Some(voice) = &self.voice {
                let vid = NSString::from_str(&voice.id());
                let v = AVSpeechSynthesisVoice::voiceWithIdentifier(&*vid)
                    .ok_or(Error::OperationFailed)?;
                utterance.setVoice(Some(&v));
            }
            trace!("Enqueuing");
            self.synth.speakUtterance(&utterance);
            trace!("Done queuing");
        }
        Ok(Some(UtteranceId::AvFoundation(
            &*utterance as *const _ as usize,
        )))
    }

    fn stop(&mut self) -> Result<(), Error> {
        trace!("stop()");
        unsafe {
            self.synth
                .stopSpeakingAtBoundary(AVSpeechBoundary::Immediate);
        }
        Ok(())
    }

    fn min_rate(&self) -> f32 {
        0.1
    }

    fn max_rate(&self) -> f32 {
        2.
    }

    fn normal_rate(&self) -> f32 {
        0.5
    }

    fn get_rate(&self) -> Result<f32, Error> {
        Ok(self.rate)
    }

    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        trace!("set_rate({})", rate);
        self.rate = rate;
        Ok(())
    }

    fn min_pitch(&self) -> f32 {
        0.5
    }

    fn max_pitch(&self) -> f32 {
        2.0
    }

    fn normal_pitch(&self) -> f32 {
        1.0
    }

    fn get_pitch(&self) -> Result<f32, Error> {
        Ok(self.pitch)
    }

    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        trace!("set_pitch({})", pitch);
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
        trace!("set_volume({})", volume);
        self.volume = volume;
        Ok(())
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        trace!("is_speaking()");
        let is_speaking = unsafe { self.synth.isSpeaking() };
        Ok(is_speaking)
    }

    fn voice(&self) -> Result<Option<Voice>, Error> {
        unimplemented!()
    }

    fn voices(&self) -> Result<Vec<Voice>, Error> {
        let voices = unsafe { AVSpeechSynthesisVoice::speechVoices() };
        let rv = voices
            .iter()
            .map(|v| {
                let id = unsafe { v.identifier() };
                let name = unsafe { v.name() };
                let gender = unsafe { v.gender() };
                let gender = match gender {
                    AVSpeechSynthesisVoiceGender::Male => Some(Gender::Male),
                    AVSpeechSynthesisVoiceGender::Female => Some(Gender::Female),
                    _ => None,
                };
                let language = unsafe { v.language() };
                let language = language.to_string();
                let language = LanguageTag::parse(language).unwrap();
                Voice {
                    id: id.to_string(),
                    name: name.to_string(),
                    gender,
                    language,
                }
            })
            .collect();
        Ok(rv)
    }

    fn set_voice(&mut self, voice: &Voice) -> Result<(), Error> {
        self.voice = Some(voice.clone());
        Ok(())
    }
}
