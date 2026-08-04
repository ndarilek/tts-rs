use std::{
    ptr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_avf_audio::{
    AVSpeechBoundary, AVSpeechSynthesisVoice, AVSpeechSynthesisVoiceGender, AVSpeechSynthesizer,
    AVSpeechSynthesizerDelegate, AVSpeechUtterance,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
use oxilangtag::LanguageTag;
use parking_lot::Mutex;
use tracing::{Span, info_span, instrument, trace};

use crate::{Backend, BackendId, Callbacks, Error, Features, Gender, UtteranceId, Voice};

#[derive(Debug)]
struct Ivars {
    callbacks: Arc<Mutex<Callbacks>>,
    // Delegate methods fire on AVFoundation's own threads; entering this span there connects
    // them back to the backend that created the synthesizer.
    span: Span,
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
            let ivars = self.ivars();
            let _entered = ivars.span.enter();
            let utterance_id = UtteranceId::AvFoundation(ptr::from_ref(utterance) as usize);
            trace!(?utterance_id, "Utterance started");
            ivars.callbacks.lock().utterance_begin(utterance_id);
        }

        #[unsafe(method(speechSynthesizer:didFinishSpeechUtterance:))]
        fn speech_synthesizer_did_finish_speech_utterance(
            &self,
            _synthesizer: &AVSpeechSynthesizer,
            utterance: &AVSpeechUtterance,
        ) {
            let ivars = self.ivars();
            let _entered = ivars.span.enter();
            let utterance_id = UtteranceId::AvFoundation(ptr::from_ref(utterance) as usize);
            trace!(?utterance_id, "Utterance finished");
            ivars.callbacks.lock().utterance_end(utterance_id);
        }

        #[unsafe(method(speechSynthesizer:didCancelSpeechUtterance:))]
        fn speech_synthesizer_did_cancel_speech_utterance(
            &self,
            _synthesizer: &AVSpeechSynthesizer,
            utterance: &AVSpeechUtterance,
        ) {
            let ivars = self.ivars();
            let _entered = ivars.span.enter();
            let utterance_id = UtteranceId::AvFoundation(ptr::from_ref(utterance) as usize);
            trace!(?utterance_id, "Utterance canceled");
            ivars.callbacks.lock().utterance_stop(utterance_id);
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

static NEXT_BACKEND_ID: AtomicU64 = AtomicU64::new(0);

impl AvFoundation {
    // Construction can't fail here, but backend constructors share a fallible signature.
    #[allow(clippy::unnecessary_wraps)]
    #[instrument(level = "info", skip(callbacks), err)]
    pub(crate) fn new(callbacks: Arc<Mutex<Callbacks>>) -> Result<Self, Error> {
        let id = BackendId::AvFoundation(NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed));
        let span = info_span!("av_foundation", backend_id = ?id);
        let synth = unsafe { AVSpeechSynthesizer::new() };
        let delegate = Delegate::alloc().set_ivars(Ivars { callbacks, span });
        let delegate: Retained<Delegate> = unsafe { msg_send![super(delegate), init] };
        unsafe { synth.setDelegate(Some(ProtocolObject::from_ref(&*delegate))) };

        Ok(AvFoundation {
            id,
            _delegate: delegate,
            synth,
            rate: 0.5,
            volume: 1.,
            pitch: 1.,
            voice: None,
        })
    }
}

impl Backend for AvFoundation {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> Option<BackendId> {
        Some(self.id)
    }

    #[instrument(level = "trace", skip(self))]
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

    #[instrument(
        level = "debug",
        skip(self),
        fields(rate = self.rate, volume = self.volume, pitch = self.pitch),
        err
    )]
    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        if interrupt && self.is_speaking()? {
            self.stop()?;
        }
        let utterance;
        unsafe {
            let str = NSString::from_str(text);
            utterance = AVSpeechUtterance::initWithString(AVSpeechUtterance::alloc(), &str);
            utterance.setRate(self.rate);
            utterance.setVolume(self.volume);
            utterance.setPitchMultiplier(self.pitch);
            if let Some(voice) = &self.voice {
                let vid = NSString::from_str(&voice.id());
                let v = AVSpeechSynthesisVoice::voiceWithIdentifier(&vid)
                    .ok_or(Error::OperationFailed)?;
                utterance.setVoice(Some(&v));
            }
            self.synth.speakUtterance(&utterance);
        }
        Ok(Some(UtteranceId::AvFoundation(
            ptr::from_ref(&*utterance) as usize
        )))
    }

    #[instrument(level = "debug", skip(self), err)]
    fn stop(&mut self) -> Result<(), Error> {
        unsafe {
            self.synth
                .stopSpeakingAtBoundary(AVSpeechBoundary::Immediate);
        }
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn min_rate(&self) -> f32 {
        0.1
    }

    #[instrument(level = "trace", skip(self))]
    fn max_rate(&self) -> f32 {
        2.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_rate(&self) -> f32 {
        0.5
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
        0.5
    }

    #[instrument(level = "trace", skip(self))]
    fn max_pitch(&self) -> f32 {
        2.0
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_pitch(&self) -> f32 {
        1.0
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
        let is_speaking = unsafe { self.synth.isSpeaking() };
        Ok(is_speaking)
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn voice(&self) -> Result<Option<Voice>, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err)]
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

    #[instrument(level = "debug", skip(self), err)]
    fn set_voice(&mut self, voice: &Voice) -> Result<(), Error> {
        self.voice = Some(voice.clone());
        Ok(())
    }
}
