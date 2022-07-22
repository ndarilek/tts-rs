#[cfg(target_os = "android")]
use std::{
    collections::HashSet,
    ffi::{CStr, CString},
    os::raw::c_void,
    sync::{Mutex, RwLock},
    thread,
    time::{Duration, Instant},
};

use jni::{
    objects::{GlobalRef, JObject, JString},
    sys::{jfloat, jint, JNI_VERSION_1_6},
    JNIEnv, JavaVM,
};
use lazy_static::lazy_static;
use log::{error, info};

use crate::{Backend, BackendId, Error, Features, UtteranceId, Voice, CALLBACKS};

lazy_static! {
    static ref BRIDGE: Mutex<Option<GlobalRef>> = Mutex::new(None);
    static ref NEXT_BACKEND_ID: Mutex<u64> = Mutex::new(0);
    static ref PENDING_INITIALIZATIONS: RwLock<HashSet<u64>> = RwLock::new(HashSet::new());
    static ref NEXT_UTTERANCE_ID: Mutex<u64> = Mutex::new(0);
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
    let env = vm.get_env().expect("Cannot get reference to the JNIEnv");
    let b = env
        .find_class("rs/tts/Bridge")
        .expect("Failed to find `Bridge`");
    let b = env
        .new_global_ref(b)
        .expect("Failed to create `Bridge` `GlobalRef`");
    let mut bridge = BRIDGE.lock().unwrap();
    *bridge = Some(b);
    JNI_VERSION_1_6
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onInit(env: JNIEnv, obj: JObject, status: jint) {
    let id = env
        .get_field(obj, "backendId", "I")
        .expect("Failed to get backend ID")
        .i()
        .expect("Failed to cast to int") as u64;
    let mut pending = PENDING_INITIALIZATIONS.write().unwrap();
    (*pending).remove(&id);
    if status != 0 {
        error!("Failed to initialize TTS engine");
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onStart(
    env: JNIEnv,
    obj: JObject,
    utterance_id: JString,
) {
    let backend_id = env
        .get_field(obj, "backendId", "I")
        .expect("Failed to get backend ID")
        .i()
        .expect("Failed to cast to int") as u64;
    let backend_id = BackendId::Android(backend_id);
    let utterance_id = CString::from(CStr::from_ptr(
        env.get_string(utterance_id).unwrap().as_ptr(),
    ))
    .into_string()
    .unwrap();
    let utterance_id = utterance_id.parse::<u64>().unwrap();
    let utterance_id = UtteranceId::Android(utterance_id);
    let mut callbacks = CALLBACKS.lock().unwrap();
    let cb = callbacks.get_mut(&backend_id).unwrap();
    if let Some(f) = cb.utterance_begin.as_mut() {
        f(utterance_id);
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onStop(
    env: JNIEnv,
    obj: JObject,
    utterance_id: JString,
) {
    let backend_id = env
        .get_field(obj, "backendId", "I")
        .expect("Failed to get backend ID")
        .i()
        .expect("Failed to cast to int") as u64;
    let backend_id = BackendId::Android(backend_id);
    let utterance_id = CString::from(CStr::from_ptr(
        env.get_string(utterance_id).unwrap().as_ptr(),
    ))
    .into_string()
    .unwrap();
    let utterance_id = utterance_id.parse::<u64>().unwrap();
    let utterance_id = UtteranceId::Android(utterance_id);
    let mut callbacks = CALLBACKS.lock().unwrap();
    let cb = callbacks.get_mut(&backend_id).unwrap();
    if let Some(f) = cb.utterance_end.as_mut() {
        f(utterance_id);
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onDone(
    env: JNIEnv,
    obj: JObject,
    utterance_id: JString,
) {
    let backend_id = env
        .get_field(obj, "backendId", "I")
        .expect("Failed to get backend ID")
        .i()
        .expect("Failed to cast to int") as u64;
    let backend_id = BackendId::Android(backend_id);
    let utterance_id = CString::from(CStr::from_ptr(
        env.get_string(utterance_id).unwrap().as_ptr(),
    ))
    .into_string()
    .unwrap();
    let utterance_id = utterance_id.parse::<u64>().unwrap();
    let utterance_id = UtteranceId::Android(utterance_id);
    let mut callbacks = CALLBACKS.lock().unwrap();
    let cb = callbacks.get_mut(&backend_id).unwrap();
    if let Some(f) = cb.utterance_stop.as_mut() {
        f(utterance_id);
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onError(
    env: JNIEnv,
    obj: JObject,
    utterance_id: JString,
) {
    let backend_id = env
        .get_field(obj, "backendId", "I")
        .expect("Failed to get backend ID")
        .i()
        .expect("Failed to cast to int") as u64;
    let backend_id = BackendId::Android(backend_id);
    let utterance_id = CString::from(CStr::from_ptr(
        env.get_string(utterance_id).unwrap().as_ptr(),
    ))
    .into_string()
    .unwrap();
    let utterance_id = utterance_id.parse::<u64>().unwrap();
    let utterance_id = UtteranceId::Android(utterance_id);
    let mut callbacks = CALLBACKS.lock().unwrap();
    let cb = callbacks.get_mut(&backend_id).unwrap();
    if let Some(f) = cb.utterance_end.as_mut() {
        f(utterance_id);
    }
}

#[derive(Clone)]
pub(crate) struct Android {
    id: BackendId,
    tts: GlobalRef,
    rate: f32,
    pitch: f32,
}

impl Android {
    pub(crate) fn new() -> Result<Self, Error> {
        info!("Initializing Android backend");
        let mut backend_id = NEXT_BACKEND_ID.lock().unwrap();
        let bid = *backend_id;
        let id = BackendId::Android(bid);
        *backend_id += 1;
        drop(backend_id);
        let native_activity = ndk_glue::native_activity();
        let vm = Self::vm()?;
        let env = vm.attach_current_thread_permanently()?;
        let bridge = BRIDGE.lock().unwrap();
        if let Some(bridge) = &*bridge {
            let bridge = env.new_object(bridge, "(I)V", &[(bid as jint).into()])?;
            let tts = env.new_object(
                "android/speech/tts/TextToSpeech",
                "(Landroid/content/Context;Landroid/speech/tts/TextToSpeech$OnInitListener;)V",
                &[native_activity.activity().into(), bridge.into()],
            )?;
            env.call_method(
                tts,
                "setOnUtteranceProgressListener",
                "(Landroid/speech/tts/UtteranceProgressListener;)I",
                &[bridge.into()],
            )?;
            {
                let mut pending = PENDING_INITIALIZATIONS.write().unwrap();
                (*pending).insert(bid);
            }
            let tts = env.new_global_ref(tts)?;
            // This hack makes my brain bleed.
            const MAX_WAIT_TIME: Duration = Duration::from_millis(500);
            let start = Instant::now();
            // Wait a max of 500ms for initialization, then return an error to avoid hanging.
            loop {
                {
                    let pending = PENDING_INITIALIZATIONS.read().unwrap();
                    if !(*pending).contains(&bid) {
                        break;
                    }
                    if start.elapsed() > MAX_WAIT_TIME {
                        return Err(Error::OperationFailed);
                    }
                }
                thread::sleep(Duration::from_millis(5));
            }
            Ok(Self {
                id,
                tts,
                rate: 1.,
                pitch: 1.,
            })
        } else {
            Err(Error::NoneError)
        }
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
            stop: true,
            rate: true,
            pitch: true,
            volume: false,
            is_speaking: true,
            utterance_callbacks: true,
            voice: false,
            get_voice: false,
        }
    }

    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        let vm = Self::vm()?;
        let env = vm.get_env()?;
        let tts = self.tts.as_obj();
        let text = env.new_string(text)?;
        let queue_mode = if interrupt { 0 } else { 1 };
        let mut utterance_id = NEXT_UTTERANCE_ID.lock().unwrap();
        let uid = *utterance_id;
        *utterance_id += 1;
        drop(utterance_id);
        let id = UtteranceId::Android(uid);
        let uid = env.new_string(uid.to_string())?;
        let rv = env.call_method(
            tts,
            "speak",
            "(Ljava/lang/CharSequence;ILandroid/os/Bundle;Ljava/lang/String;)I",
            &[
                text.into(),
                queue_mode.into(),
                JObject::null().into(),
                uid.into(),
            ],
        )?;
        let rv = rv.i()?;
        if rv == 0 {
            Ok(Some(id))
        } else {
            Err(Error::OperationFailed)
        }
    }

    fn stop(&mut self) -> Result<(), Error> {
        let vm = Self::vm()?;
        let env = vm.get_env()?;
        let tts = self.tts.as_obj();
        let rv = env.call_method(tts, "stop", "()I", &[])?;
        let rv = rv.i()?;
        if rv == 0 {
            Ok(())
        } else {
            Err(Error::OperationFailed)
        }
    }

    fn min_rate(&self) -> f32 {
        0.1
    }

    fn max_rate(&self) -> f32 {
        10.
    }

    fn normal_rate(&self) -> f32 {
        1.
    }

    fn get_rate(&self) -> Result<f32, Error> {
        Ok(self.rate)
    }

    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        let vm = Self::vm()?;
        let env = vm.get_env()?;
        let tts = self.tts.as_obj();
        let rate = rate as jfloat;
        let rv = env.call_method(tts, "setSpeechRate", "(F)I", &[rate.into()])?;
        let rv = rv.i()?;
        if rv == 0 {
            self.rate = rate;
            Ok(())
        } else {
            Err(Error::OperationFailed)
        }
    }

    fn min_pitch(&self) -> f32 {
        0.1
    }

    fn max_pitch(&self) -> f32 {
        2.
    }

    fn normal_pitch(&self) -> f32 {
        1.
    }

    fn get_pitch(&self) -> Result<f32, Error> {
        Ok(self.pitch)
    }

    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        let vm = Self::vm()?;
        let env = vm.get_env()?;
        let tts = self.tts.as_obj();
        let pitch = pitch as jfloat;
        let rv = env.call_method(tts, "setPitch", "(F)I", &[pitch.into()])?;
        let rv = rv.i()?;
        if rv == 0 {
            self.pitch = pitch;
            Ok(())
        } else {
            Err(Error::OperationFailed)
        }
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

    fn set_volume(&mut self, _volume: f32) -> Result<(), Error> {
        todo!()
    }

    fn is_speaking(&self) -> Result<bool, Error> {
        let vm = Self::vm()?;
        let env = vm.get_env()?;
        let tts = self.tts.as_obj();
        let rv = env.call_method(tts, "isSpeaking", "()Z", &[])?;
        let rv = rv.z()?;
        Ok(rv)
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
