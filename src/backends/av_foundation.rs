#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::sync::Mutex;

use cocoa_foundation::base::{id, nil, NO};
use cocoa_foundation::foundation::NSString;
use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use lazy_static::lazy_static;
use log::{info, trace};
use objc::runtime::{Object, Sel};
use objc::{class, declare::ClassDecl, msg_send, sel, sel_impl};
use oxilangtag::LanguageTag;

use crate::{Backend, BackendId, Error, Features, Gender, UtteranceId, Voice, CALLBACKS};

#[derive(Clone, Debug)]
pub(crate) struct AvFoundation {
    id: BackendId,
    delegate: *mut Object,
    synth: *mut Object,
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
        let mut decl = ClassDecl::new("MyNSSpeechSynthesizerDelegate", class!(NSObject))
            .ok_or(Error::OperationFailed)?;
        decl.add_ivar::<u64>("backend_id");

        extern "C" fn speech_synthesizer_did_start_speech_utterance(
            this: &Object,
            _: Sel,
            _synth: *const Object,
            utterance: id,
        ) {
            trace!("speech_synthesizer_did_start_speech_utterance");
            unsafe {
                let backend_id: u64 = *this.get_ivar("backend_id");
                let backend_id = BackendId::AvFoundation(backend_id);
                trace!("Locking callbacks");
                let mut callbacks = CALLBACKS.lock().unwrap();
                trace!("Locked");
                let callbacks = callbacks.get_mut(&backend_id).unwrap();
                if let Some(callback) = callbacks.utterance_begin.as_mut() {
                    trace!("Calling utterance_begin");
                    let utterance_id = UtteranceId::AvFoundation(utterance);
                    callback(utterance_id);
                    trace!("Called");
                }
            }
            trace!("Done speech_synthesizer_did_start_speech_utterance");
        }

        extern "C" fn speech_synthesizer_did_finish_speech_utterance(
            this: &Object,
            _: Sel,
            _synth: *const Object,
            utterance: id,
        ) {
            trace!("speech_synthesizer_did_finish_speech_utterance");
            unsafe {
                let backend_id: u64 = *this.get_ivar("backend_id");
                let backend_id = BackendId::AvFoundation(backend_id);
                trace!("Locking callbacks");
                let mut callbacks = CALLBACKS.lock().unwrap();
                trace!("Locked");
                let callbacks = callbacks.get_mut(&backend_id).unwrap();
                if let Some(callback) = callbacks.utterance_end.as_mut() {
                    trace!("Calling utterance_end");
                    let utterance_id = UtteranceId::AvFoundation(utterance);
                    callback(utterance_id);
                    trace!("Called");
                }
            }
            trace!("Done speech_synthesizer_did_finish_speech_utterance");
        }

        extern "C" fn speech_synthesizer_did_cancel_speech_utterance(
            this: &Object,
            _: Sel,
            _synth: *const Object,
            utterance: id,
        ) {
            trace!("speech_synthesizer_did_cancel_speech_utterance");
            unsafe {
                let backend_id: u64 = *this.get_ivar("backend_id");
                let backend_id = BackendId::AvFoundation(backend_id);
                trace!("Locking callbacks");
                let mut callbacks = CALLBACKS.lock().unwrap();
                trace!("Locked");
                let callbacks = callbacks.get_mut(&backend_id).unwrap();
                if let Some(callback) = callbacks.utterance_stop.as_mut() {
                    trace!("Calling utterance_stop");
                    let utterance_id = UtteranceId::AvFoundation(utterance);
                    callback(utterance_id);
                    trace!("Called");
                }
            }
            trace!("Done speech_synthesizer_did_cancel_speech_utterance");
        }

        unsafe {
            decl.add_method(
                sel!(speechSynthesizer:didStartSpeechUtterance:),
                speech_synthesizer_did_start_speech_utterance
                    as extern "C" fn(&Object, Sel, *const Object, id) -> (),
            );
            decl.add_method(
                sel!(speechSynthesizer:didFinishSpeechUtterance:),
                speech_synthesizer_did_finish_speech_utterance
                    as extern "C" fn(&Object, Sel, *const Object, id) -> (),
            );
            decl.add_method(
                sel!(speechSynthesizer:didCancelSpeechUtterance:),
                speech_synthesizer_did_cancel_speech_utterance
                    as extern "C" fn(&Object, Sel, *const Object, id) -> (),
            );
        }

        let delegate_class = decl.register();
        let delegate_obj: *mut Object = unsafe { msg_send![delegate_class, new] };
        let mut backend_id = NEXT_BACKEND_ID.lock().unwrap();
        let rv = unsafe {
            trace!("Creating synth");
            let synth: *mut Object = msg_send![class!(AVSpeechSynthesizer), new];
            trace!("Allocated {:?}", synth);
            delegate_obj
                .as_mut()
                .unwrap()
                .set_ivar("backend_id", *backend_id);
            trace!("Set backend ID in delegate");
            let _: () = msg_send![synth, setDelegate: delegate_obj];
            trace!("Assigned delegate: {:?}", delegate_obj);
            AvFoundation {
                id: BackendId::AvFoundation(*backend_id),
                delegate: delegate_obj,
                synth,
                rate: 0.5,
                volume: 1.,
                pitch: 1.,
                voice: None,
            }
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
        let mut utterance: id;
        unsafe {
            trace!("Allocating utterance string");
            let mut str = NSString::alloc(nil);
            str = str.init_str(text);
            trace!("Allocating utterance");
            utterance = msg_send![class!(AVSpeechUtterance), alloc];
            trace!("Initializing utterance");
            utterance = msg_send![utterance, initWithString: str];
            trace!("Setting rate to {}", self.rate);
            let _: () = msg_send![utterance, setRate: self.rate];
            trace!("Setting volume to {}", self.volume);
            let _: () = msg_send![utterance, setVolume: self.volume];
            trace!("Setting pitch to {}", self.pitch);
            let _: () = msg_send![utterance, setPitchMultiplier: self.pitch];
            if let Some(voice) = &self.voice {
                let mut vid = NSString::alloc(nil);
                vid = vid.init_str(&voice.id());
                let v: id = msg_send![class!(AVSpeechSynthesisVoice), voiceWithIdentifier: vid];
                let _: () = msg_send![utterance, setVoice: v];
            }
            trace!("Enqueuing");
            let _: () = msg_send![self.synth, speakUtterance: utterance];
            trace!("Done queuing");
        }
        Ok(Some(UtteranceId::AvFoundation(utterance)))
    }

    fn stop(&mut self) -> Result<(), Error> {
        trace!("stop()");
        unsafe {
            let _: () = msg_send![self.synth, stopSpeakingAtBoundary: 0];
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
        let is_speaking: i8 = unsafe { msg_send![self.synth, isSpeaking] };
        Ok(is_speaking != NO as i8)
    }

    fn voice(&self) -> Result<Option<Voice>, Error> {
        unimplemented!()
    }

    fn voices(&self) -> Result<Vec<Voice>, Error> {
        let voices: CFArray = unsafe {
            CFArray::wrap_under_get_rule(msg_send![class!(AVSpeechSynthesisVoice), speechVoices])
        };
        let rv = voices
            .iter()
            .map(|v| {
                let id: CFString = unsafe {
                    CFString::wrap_under_get_rule(msg_send![*v as *const Object, identifier])
                };
                let name: CFString =
                    unsafe { CFString::wrap_under_get_rule(msg_send![*v as *const Object, name]) };
                let gender: i64 = unsafe { msg_send![*v as *const Object, gender] };
                let gender = match gender {
                    1 => Some(Gender::Male),
                    2 => Some(Gender::Female),
                    _ => None,
                };
                let language: CFString = unsafe {
                    CFString::wrap_under_get_rule(msg_send![*v as *const Object, language])
                };
                let language = language.to_string();
                let language = LanguageTag::parse(&language).unwrap();
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

impl Drop for AvFoundation {
    fn drop(&mut self) {
        unsafe {
            let _: Object = msg_send![self.delegate, release];
            let _: Object = msg_send![self.synth, release];
        }
    }
}
