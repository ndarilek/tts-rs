#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
use cocoa_foundation::base::nil;
use cocoa_foundation::foundation::NSString;
use log::{info, trace};
use objc::declare::ClassDecl;
use objc::runtime::*;
use objc::*;

use crate::{Backend, Error, Features};

pub struct NSSpeechSynthesizerBackend(*mut Object, *mut Object);

impl NSSpeechSynthesizerBackend {
    pub fn new() -> Self {
        info!("Initializing NSSpeechSynthesizer backend");
        let mut obj: *mut Object = unsafe { msg_send![class!(NSSpeechSynthesizer), alloc] };
        obj = unsafe { msg_send![obj, init] };
        let mut decl = ClassDecl::new("MyNSSpeechSynthesizerDelegate", class!(NSObject)).unwrap();
        extern "C" fn speech_synthesizer_did_finish_speaking(
            _: &Object,
            _: Sel,
            _: *const Object,
            _: BOOL,
        ) {
            println!("Got it");
        }
        unsafe {
            decl.add_method(
                sel!(speechSynthesizer:didFinishSpeaking:),
                speech_synthesizer_did_finish_speaking
                    as extern "C" fn(&Object, Sel, *const Object, BOOL) -> (),
            )
        };
        let delegate_class = decl.register();
        let delegate_obj: *mut Object = unsafe { msg_send![delegate_class, new] };
        let _: Object = unsafe { msg_send![obj, setDelegate: delegate_obj] };
        NSSpeechSynthesizerBackend(obj, delegate_obj)
    }
}

impl Backend for NSSpeechSynthesizerBackend {
    fn supported_features(&self) -> Features {
        Features {
            stop: false,
            rate: false,
            pitch: false,
            volume: false,
            is_speaking: false,
        }
    }

    fn speak(&mut self, text: &str, interrupt: bool) -> Result<(), Error> {
        trace!("speak({}, {})", text, interrupt);
        let str = unsafe { NSString::alloc(nil).init_str(text) };
        let _: BOOL = unsafe { msg_send![self.0, startSpeakingString: str] };
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Error> {
        trace!("stop()");
        unimplemented!()
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
        unimplemented!()
    }

    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        unimplemented!()
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
        unimplemented!()
    }

    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        unimplemented!()
    }

    fn min_volume(&self) -> f32 {
        -100.
    }

    fn max_volume(&self) -> f32 {
        100.
    }

    fn normal_volume(&self) -> f32 {
        0.
    }

    fn get_volume(&self) -> Result<f32, Error> {
        unimplemented!()
    }

    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        unimplemented!()
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        unimplemented!()
    }
}

impl Drop for NSSpeechSynthesizerBackend {
    fn drop(&mut self) {
        unsafe {
            let _: Object = msg_send!(self.0, release);
            let _: Object = msg_send!(self.1, release);
        }
    }
}
