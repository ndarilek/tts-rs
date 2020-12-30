#[cfg(target_os = "android")]
use std::sync::Mutex;

use jni::objects::{GlobalRef, JObject};
use jni::JavaVM;
use lazy_static::lazy_static;
use log::info;

use crate::{Backend, BackendId, Error, Features, UtteranceId, CALLBACKS};

lazy_static! {
    static ref NEXT_BACKEND_ID: Mutex<u64> = Mutex::new(0);
}

#[derive(Clone)]
pub(crate) struct Android {
    id: BackendId,
    tts: GlobalRef,
}

impl Android {
    pub(crate) fn new() -> Result<Self, Error> {
        info!("Initializing Android backend");
        let mut backend_id = NEXT_BACKEND_ID.lock().unwrap();
        let id = BackendId::Android(*backend_id);
        *backend_id += 1;
        let native_activity = ndk_glue::native_activity();
        let vm = Self::vm()?;
        let env = vm.attach_current_thread_permanently()?;
        let tts = env.new_object(
            "android/speech/tts/TextToSpeech",
            "(Landroid/content/Context;Landroid/speech/tts/TextToSpeech$OnInitListener;)V",
            &[
                native_activity.activity().into(),
                native_activity.activity().into(),
            ],
        )?;
        println!("Creating global ref");
        let tts = env.new_global_ref(tts)?;
        println!("Returning");
        Ok(Self { id, tts })
    }

    fn vm() -> Result<JavaVM, jni::errors::Error> {
        let native_activity = ndk_glue::native_activity();
        let vm_ptr = native_activity.vm();
        unsafe { jni::JavaVM::from_raw(vm_ptr) }
    }
}

impl Backend for Android {
    fn id(&self) -> Option<BackendId> {
        Some(self.id)
    }

    fn supported_features(&self) -> Features {
        Features {
            stop: false,
            rate: false,
            pitch: false,
            volume: false,
            is_speaking: false,
            utterance_callbacks: false,
        }
    }

    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        println!("Speaking {}, {:?}", text, interrupt);
        let vm = Self::vm()?;
        println!("Retrieved");
        let env = vm.get_env()?;
        println!("attached");
        let tts = self.tts.as_obj();
        let text = env.new_string(text)?;
        let queue_mode = if interrupt { 0 } else { 1 };
        println!("Calling");
        env.call_method(
            tts,
            "speak",
            "(Ljava/lang/CharSequence;ILandroid/os/Bundle;Ljava/lang/String;)I",
            &[
                text.into(),
                queue_mode.into(),
                JObject::null().into(),
                JObject::null().into(),
            ],
        )?;
        println!("Returning");
        Ok(None)
    }

    fn stop(&mut self) -> Result<(), Error> {
        todo!()
    }

    fn min_rate(&self) -> f32 {
        todo!()
    }

    fn max_rate(&self) -> f32 {
        todo!()
    }

    fn normal_rate(&self) -> f32 {
        todo!()
    }

    fn get_rate(&self) -> Result<f32, Error> {
        todo!()
    }

    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        todo!()
    }

    fn min_pitch(&self) -> f32 {
        todo!()
    }

    fn max_pitch(&self) -> f32 {
        todo!()
    }

    fn normal_pitch(&self) -> f32 {
        todo!()
    }

    fn get_pitch(&self) -> Result<f32, Error> {
        todo!()
    }

    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        todo!()
    }

    fn min_volume(&self) -> f32 {
        todo!()
    }

    fn max_volume(&self) -> f32 {
        todo!()
    }

    fn normal_volume(&self) -> f32 {
        todo!()
    }

    fn get_volume(&self) -> Result<f32, Error> {
        todo!()
    }

    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        todo!()
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        todo!()
    }
}
