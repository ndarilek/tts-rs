// NSSpeechSynthesizer is deprecated, but we can't use AVSpeechSynthesizer
// on older macOS.
#![allow(deprecated)]
use log::{info, trace};
use objc2::rc::Retained;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSSpeechSynthesizer, NSSpeechSynthesizerDelegate};
use objc2_foundation::{NSMutableArray, NSObject, NSObjectProtocol, NSString};

use crate::{Backend, BackendId, Error, Features, UtteranceId, Voice};

#[derive(Debug)]
struct Ivars {
    synth: Retained<NSSpeechSynthesizer>,
    strings: Retained<NSMutableArray<NSString>>,
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super(NSObject))]
    #[name = "MyNSSpeechSynthesizerDelegate"]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSSpeechSynthesizerDelegate for Delegate {
        #[unsafe(method(speechSynthesizer:didFinishSpeaking:))]
        fn speech_synthesizer_did_finish_speaking(
            &self,
            _sender: &NSSpeechSynthesizer,
            _finished_speaking: bool,
        ) {
            let Ivars { strings, synth } = self.ivars();
            if let Some(_str) = strings.firstObject() {
                strings.removeObjectAtIndex(0);
                if let Some(str) = strings.firstObject() {
                    unsafe { synth.startSpeakingString(&str) };
                }
            }
        }
    }
);

impl Delegate {
    fn enqueue_and_speak(&self, string: &NSString) {
        let Ivars { strings, synth } = self.ivars();
        strings.addObject(string);
        if let Some(str) = strings.firstObject() {
            unsafe { synth.startSpeakingString(&str) };
        }
    }

    fn clear_queue(&self) {
        let strings = &self.ivars().strings;
        let mut count = strings.count();
        while count > 0 {
            strings.removeObjectAtIndex(0);
            count = strings.count();
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AppKit {
    synth: Retained<NSSpeechSynthesizer>,
    delegate: Retained<Delegate>,
}

impl AppKit {
    pub(crate) fn new() -> Result<Self, Error> {
        info!("Initializing AppKit backend");
        let synth = unsafe { NSSpeechSynthesizer::new() };

        // TODO: It is UB to use NSSpeechSynthesizerDelegate off the main
        // thread, we should somehow expose the need to be on the main thread.
        //
        // Maybe just returning an error?
        let mtm = unsafe { MainThreadMarker::new_unchecked() };

        let delegate = Delegate::alloc(mtm).set_ivars(Ivars {
            synth: synth.clone(),
            strings: NSMutableArray::new(),
        });
        let delegate: Retained<Delegate> = unsafe { msg_send![super(delegate), init] };

        Ok(AppKit { synth, delegate })
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
        let str = NSString::from_str(text);
        self.delegate.enqueue_and_speak(&str);
        Ok(None)
    }

    fn synthesize(&mut self, text: &str) -> Result<Vec<u8>, Error> {
        unimplemented!();
    }

    fn stop(&mut self) -> Result<(), Error> {
        trace!("stop()");
        self.delegate.clear_queue();
        unsafe { self.synth.stopSpeaking() };
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
        let rate: f32 = unsafe { self.synth.rate() };
        Ok(rate)
    }

    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        trace!("set_rate({})", rate);
        unsafe { self.synth.setRate(rate) };
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
        let volume = unsafe { self.synth.volume() };
        Ok(volume)
    }

    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        unsafe { self.synth.setVolume(volume) };
        Ok(())
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        let is_speaking = unsafe { self.synth.isSpeaking() };
        Ok(is_speaking)
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
