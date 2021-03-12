#[cfg(any(target_os = "macos", target_os = "ios"))]
#[link(name = "AVFoundation", kind = "framework")]
use std::sync::Mutex;

use cocoa_foundation::base::{id, nil};
use cocoa_foundation::foundation::NSString;
use lazy_static::lazy_static;
use log::{info, trace};
use objc::runtime::{Object, Sel};
use objc::{class, declare::ClassDecl, msg_send, sel, sel_impl};

use crate::{Backend, BackendId, Error, Features, UtteranceId, CALLBACKS};

#[derive(Clone, Debug)]
pub(crate) struct AvFoundation {
    id: BackendId,
    delegate: *mut Object,
    synth: *mut Object,
    rate: f32,
    volume: f32,
    pitch: f32,
}

lazy_static! {
    static ref NEXT_BACKEND_ID: Mutex<u64> = Mutex::new(0);
}

impl AvFoundation {
    pub(crate) fn new() -> Self {
        info!("Initializing AVFoundation backend");
        let mut decl = ClassDecl::new("MyNSSpeechSynthesizerDelegate", class!(NSObject)).unwrap();
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
            let synth: *mut Object = msg_send![class!(AVSpeechSynthesizer), new];
            delegate_obj
                .as_mut()
                .unwrap()
                .set_ivar("backend_id", *backend_id);
            let _: () = msg_send![synth, setDelegate: delegate_obj];
            AvFoundation {
                id: BackendId::AvFoundation(*backend_id),
                delegate: delegate_obj,
                synth,
                rate: 0.5,
                volume: 1.,
                pitch: 1.,
            }
        };
        *backend_id += 1;
        rv
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
            utterance_callbacks: true,
        }
    }

    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        trace!("speak({}, {})", text, interrupt);
        if interrupt && self.is_speaking()? {
            self.stop()?;
        }
        let utterance: id;
        unsafe {
            let str = NSString::alloc(nil).init_str(text);
            utterance = msg_send![class!(AVSpeechUtterance), alloc];
            let _: () = msg_send![utterance, initWithString: str];
            let _: () = msg_send![utterance, setRate: self.rate];
            let _: () = msg_send![utterance, setVolume: self.volume];
            let _: () = msg_send![utterance, setPitchMultiplier: self.pitch];
            let _: () = msg_send![self.synth, speakUtterance: utterance];
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
        Ok(is_speaking == 1)
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
