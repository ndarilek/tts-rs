#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
use cocoa_foundation::base::{id, nil};
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
        unsafe {
            let obj: *mut Object = msg_send![class!(NSSpeechSynthesizer), new];
            let mut decl =
                ClassDecl::new("MyNSSpeechSynthesizerDelegate", class!(NSObject)).unwrap();
            decl.add_ivar::<id>("synth");
            decl.add_ivar::<id>("strings");
            extern "C" fn enqueue_and_speak(this: &Object, _: Sel, string: id) {
                unsafe {
                    let strings: id = *this.get_ivar("strings");
                    let _: () = msg_send![strings, addObject: string];
                    let count: u32 = msg_send![strings, count];
                    if count == 1 {
                        let str: id = msg_send!(strings, firstObject);
                        let synth: id = *this.get_ivar("synth");
                        let _: BOOL = msg_send![synth, startSpeakingString: str];
                    }
                }
            }
            extern "C" fn speech_synthesizer_did_finish_speaking(
                this: &Object,
                _: Sel,
                synth: *const Object,
                _: BOOL,
            ) {
                unsafe {
                    let strings: id = *this.get_ivar("strings");
                    let str: id = msg_send!(strings, firstObject);
                    let _: () = msg_send![str, release];
                    let _: () = msg_send!(strings, removeObjectAtIndex:0);
                    let count: u32 = msg_send![strings, count];
                    if count > 0 {
                        let str: id = msg_send!(strings, firstObject);
                        let _: BOOL = msg_send![synth, startSpeakingString: str];
                    }
                }
            }
            extern "C" fn clear_queue(this: &Object, _: Sel) {
                unsafe {
                    let strings: id = *this.get_ivar("strings");
                    let mut count: u32 = msg_send![strings, count];
                    while count > 0 {
                        let str: id = msg_send!(strings, firstObject);
                        let _: () = msg_send![str, release];
                        let _: () = msg_send!(strings, removeObjectAtIndex:0);
                        count = msg_send![strings, count];
                    }
                }
            }
            decl.add_method(
                sel!(speechSynthesizer:didFinishSpeaking:),
                speech_synthesizer_did_finish_speaking
                    as extern "C" fn(&Object, Sel, *const Object, BOOL) -> (),
            );
            decl.add_method(
                sel!(enqueueAndSpeak:),
                enqueue_and_speak as extern "C" fn(&Object, Sel, id) -> (),
            );
            decl.add_method(
                sel!(clearQueue),
                clear_queue as extern "C" fn(&Object, Sel) -> (),
            );
            let delegate_class = decl.register();
            let delegate_obj: *mut Object = msg_send![delegate_class, new];
            delegate_obj.as_mut().unwrap().set_ivar("synth", obj);
            let strings: id = msg_send![class!(NSMutableArray), new];
            delegate_obj.as_mut().unwrap().set_ivar("strings", strings);
            let _: Object = msg_send![obj, setDelegate: delegate_obj];
            NSSpeechSynthesizerBackend(obj, delegate_obj)
        }
    }

    /*fn pop_and_speak(&mut self) {
        if let Some(str) = self.2.first() {
            let _: BOOL = unsafe { msg_send![self.0, startSpeakingString: *str] };
        }
    }*/
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
        if interrupt {
            self.stop()?;
        }
        unsafe {
            let str = NSString::alloc(nil).init_str(text);
            let _: () = msg_send![self.1, enqueueAndSpeak: str];
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Error> {
        trace!("stop()");
        unsafe {
            let _: () = msg_send![self.1, clearQueue];
            let _: () = msg_send![self.0, stopSpeaking];
        }
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
        let is_speaking: i8 = unsafe { msg_send![self.0, isSpeaking] };
        Ok(is_speaking == YES)
    }
}

impl Drop for NSSpeechSynthesizerBackend {
    fn drop(&mut self) {
        unsafe {
            let _: Object = msg_send![self.0, release];
            let _: Object = msg_send![self.1, release];
        }
    }
}
