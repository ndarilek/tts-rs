#[cfg(any(target_os = "macos", target_os = "ios"))]
#[link(name = "AVFoundation", kind = "framework")]
use std::sync::Mutex;

use cocoa_foundation::base::{id, nil};
use cocoa_foundation::foundation::NSString;
use lazy_static::lazy_static;
use log::{info, trace};
use objc::runtime::*;
use objc::*;

use crate::{Backend, BackendId, Error, Features, UtteranceId};

pub(crate) struct AvFoundation {
    id: BackendId,
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
        let mut backend_id = NEXT_BACKEND_ID.lock().unwrap();
        let rv = unsafe {
            let synth: *mut Object = msg_send![class!(AVSpeechSynthesizer), new];
            AvFoundation {
                id: BackendId::AvFoundation(*backend_id),
                synth: synth,
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
        if interrupt {
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
        self.volume = volume;
        Ok(())
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        let is_speaking: i8 = unsafe { msg_send![self.synth, isSpeaking] };
        Ok(is_speaking == 1)
    }
}

impl Drop for AvFoundation {
    fn drop(&mut self) {
        unsafe {
            let _: Object = msg_send![self.synth, release];
        }
    }
}
