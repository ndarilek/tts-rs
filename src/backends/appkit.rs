// NSSpeechSynthesizer is deprecated, but we can't use AVSpeechSynthesizer
// on older macOS.
#![allow(deprecated)]
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSSpeechSynthesizer, NSSpeechSynthesizerDelegate};
use objc2_foundation::{NSMutableArray, NSObject, NSObjectProtocol, NSString};
use tracing::{Span, info_span, instrument, trace};

use crate::{Backend, BackendId, Error, Features, UtteranceId, Voice};

#[derive(Debug)]
struct Ivars {
    synth: Retained<NSSpeechSynthesizer>,
    strings: Retained<NSMutableArray<NSString>>,
    // Delegate methods fire from the run loop; entering this span there connects them back to
    // the backend that created the synthesizer.
    span: Span,
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
            let Ivars {
                strings,
                synth,
                span,
            } = self.ivars();
            let _entered = span.enter();
            trace!("Finished speaking");
            if let Some(_str) = strings.firstObject() {
                strings.removeObjectAtIndex(0);
                if let Some(str) = strings.firstObject() {
                    synth.startSpeakingString(&str);
                }
            }
        }
    }
);

impl Delegate {
    #[instrument(level = "trace", skip(self, string), fields(text = %string))]
    fn enqueue_and_speak(&self, string: &NSString) {
        let Ivars { strings, synth, .. } = self.ivars();
        strings.addObject(string);
        if let Some(str) = strings.firstObject() {
            synth.startSpeakingString(&str);
        }
    }

    #[instrument(level = "trace", skip(self))]
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
    // Construction can't fail here, but backend constructors share a fallible signature.
    #[allow(clippy::unnecessary_wraps)]
    #[instrument(level = "info", err)]
    pub(crate) fn new() -> Result<Self, Error> {
        let synth = NSSpeechSynthesizer::new();

        // TODO: It is UB to use NSSpeechSynthesizerDelegate off the main
        // thread, we should somehow expose the need to be on the main thread.
        //
        // Maybe just returning an error?
        let mtm = unsafe { MainThreadMarker::new_unchecked() };

        let delegate = Delegate::alloc(mtm).set_ivars(Ivars {
            synth: synth.clone(),
            strings: NSMutableArray::new(),
            span: info_span!("appkit"),
        });
        let delegate: Retained<Delegate> = unsafe { msg_send![super(delegate), init] };

        Ok(AppKit { synth, delegate })
    }
}

impl Backend for AppKit {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> Option<BackendId> {
        None
    }

    #[instrument(level = "trace", skip(self))]
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: true,
            volume: true,
            is_speaking: true,
            ..Default::default()
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        if interrupt {
            self.stop()?;
        }
        let str = NSString::from_str(text);
        self.delegate.enqueue_and_speak(&str);
        Ok(None)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn stop(&mut self) -> Result<(), Error> {
        self.delegate.clear_queue();
        self.synth.stopSpeaking();
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn min_rate(&self) -> f32 {
        10.
    }

    #[instrument(level = "trace", skip(self))]
    fn max_rate(&self) -> f32 {
        500.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_rate(&self) -> f32 {
        175.
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_rate(&self) -> Result<f32, Error> {
        let rate: f32 = self.synth.rate();
        Ok(rate)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        self.synth.setRate(rate);
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn min_pitch(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self))]
    fn max_pitch(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_pitch(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_pitch(&self) -> Result<f32, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self, _pitch), err)]
    fn set_pitch(&mut self, _pitch: f32) -> Result<(), Error> {
        unimplemented!()
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
        let volume = self.synth.volume();
        Ok(volume)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        self.synth.setVolume(volume);
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_speaking(&self) -> Result<bool, Error> {
        let is_speaking = self.synth.isSpeaking();
        Ok(is_speaking)
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn voice(&self) -> Result<Option<Voice>, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err)]
    fn voices(&self) -> Result<Vec<Voice>, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self, _voice), err)]
    fn set_voice(&mut self, _voice: &Voice) -> Result<(), Error> {
        unimplemented!()
    }
}
