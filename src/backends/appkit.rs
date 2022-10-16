#[cfg(target_os = "macos")]
use cocoa_foundation::base::{id, nil};
use cocoa_foundation::foundation::NSString;
use log::{info, trace};
use objc::declare::ClassDecl;
use objc::runtime::*;
use objc::*;

use crate::{Backend, BackendId, Error, Features, UtteranceId, Voice};

#[derive(Clone, Debug)]
pub(crate) struct AppKit(*mut Object, *mut Object);

impl AppKit {
    pub(crate) fn new() -> Result<Self, Error> {
        info!("Initializing AppKit backend");
        unsafe {
            let obj: *mut Object = msg_send![class!(NSSpeechSynthesizer), new];
            let mut decl = ClassDecl::new("MyNSSpeechSynthesizerDelegate", class!(NSObject))
                .ok_or(Error::OperationFailed)?;
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
            decl.add_method(
                sel!(enqueueAndSpeak:),
                enqueue_and_speak as extern "C" fn(&Object, Sel, id) -> (),
            );

            extern "C" fn speech_synthesizer_did_finish_speaking(
                this: &Object,
                _: Sel,
                synth: *const Object,
                _: BOOL,
            ) {
                unsafe {
                    let strings: id = *this.get_ivar("strings");
                    let count: u32 = msg_send![strings, count];
                    if count > 0 {
                        let str: id = msg_send!(strings, firstObject);
                        let _: () = msg_send![str, release];
                        let _: () = msg_send!(strings, removeObjectAtIndex:0);
                        if count > 1 {
                            let str: id = msg_send!(strings, firstObject);
                            let _: BOOL = msg_send![synth, startSpeakingString: str];
                        }
                    }
                }
            }
            decl.add_method(
                sel!(speechSynthesizer:didFinishSpeaking:),
                speech_synthesizer_did_finish_speaking
                    as extern "C" fn(&Object, Sel, *const Object, BOOL) -> (),
            );

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
                sel!(clearQueue),
                clear_queue as extern "C" fn(&Object, Sel) -> (),
            );

            let delegate_class = decl.register();
            let delegate_obj: *mut Object = msg_send![delegate_class, new];
            delegate_obj
                .as_mut()
                .ok_or(Error::OperationFailed)?
                .set_ivar("synth", obj);
            let strings: id = msg_send![class!(NSMutableArray), new];
            delegate_obj
                .as_mut()
                .ok_or(Error::OperationFailed)?
                .set_ivar("strings", strings);
            let _: Object = msg_send![obj, setDelegate: delegate_obj];
            Ok(AppKit(obj, delegate_obj))
        }
    }
}

impl Backend for AppKit {
    fn id(&self) -> Option<BackendId> {
        None
    }

    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: true,
            volume: true,
            is_speaking: true,
            ..Default::default()
        }
    }

    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        trace!("speak({}, {})", text, interrupt);
        if interrupt {
            self.stop()?;
        }
        unsafe {
            let str = NSString::alloc(nil).init_str(text);
            let _: () = msg_send![self.1, enqueueAndSpeak: str];
        }
        Ok(None)
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
        10.
    }

    fn max_rate(&self) -> f32 {
        500.
    }

    fn normal_rate(&self) -> f32 {
        175.
    }

    fn get_rate(&self) -> Result<f32, Error> {
        let rate: f32 = unsafe { msg_send![self.0, rate] };
        Ok(rate)
    }

    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        trace!("set_rate({})", rate);
        unsafe {
            let _: () = msg_send![self.0, setRate: rate];
        }
        Ok(())
    }

    fn min_pitch(&self) -> f32 {
        unimplemented!()
    }

    fn max_pitch(&self) -> f32 {
        unimplemented!()
    }

    fn normal_pitch(&self) -> f32 {
        unimplemented!()
    }

    fn get_pitch(&self) -> Result<f32, Error> {
        unimplemented!()
    }

    fn set_pitch(&mut self, _pitch: f32) -> Result<(), Error> {
        unimplemented!()
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
        let volume: f32 = unsafe { msg_send![self.0, volume] };
        Ok(volume)
    }

    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        unsafe {
            let _: () = msg_send![self.0, setVolume: volume];
        }
        Ok(())
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        let is_speaking: i8 = unsafe { msg_send![self.0, isSpeaking] };
        Ok(is_speaking != NO as i8)
    }

    fn voice(&self) -> Result<Option<Voice>, Error> {
        unimplemented!()
    }

    fn voices(&self) -> Result<Vec<Voice>, Error> {
        unimplemented!()
    }

    fn set_voice(&mut self, _voice: &Voice) -> Result<(), Error> {
        unimplemented!()
    }
}

impl Drop for AppKit {
    fn drop(&mut self) {
        unsafe {
            let _: Object = msg_send![self.0, release];
            let _: Object = msg_send![self.1, release];
        }
    }
}
