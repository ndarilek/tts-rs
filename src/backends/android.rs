#[cfg(target_os = "android")]
use std::{
    ffi::{CStr, CString},
    os::raw::c_void,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock, RwLock,
    },
    thread,
    time::{Duration, Instant},
};

use jni::{
    objects::{GlobalRef, JObject, JString},
    sys::{jfloat, jint, JNI_VERSION_1_6},
    JNIEnv, JavaVM,
};
use log::{error, info};

use crate::{Backend, BackendId, Callbacks, Error, Features, UtteranceId, Voice};

static BRIDGE: OnceLock<GlobalRef> = OnceLock::new();
static NEXT_BACKEND_ID: AtomicU64 = AtomicU64::new(0);
static PENDING_INITIALIZATIONS: RwLock<Vec<u64>> = RwLock::new(Vec::new());
static NEXT_UTTERANCE_ID: AtomicU64 = AtomicU64::new(0);
// The JNI callbacks below only receive a backend ID from Java, so per-instance
// callbacks must be reachable through a process-wide registry.
static CALLBACKS: Mutex<Vec<(u64, Arc<Mutex<Callbacks>>)>> = Mutex::new(Vec::new());

fn with_callbacks(backend_id: u64, f: impl FnOnce(&mut Callbacks)) {
    // Release the registry lock before the user callback runs.
    let callbacks = CALLBACKS
        .lock()
        .unwrap()
        .iter()
        .find(|(id, _)| *id == backend_id)
        .expect("No callbacks registered for backend")
        .1
        .clone();
    let mut callbacks = callbacks.lock().unwrap();
    f(&mut callbacks);
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
    let mut env = vm.get_env().expect("Cannot get reference to the JNIEnv");
    let b = env
        .find_class("rs/tts/Bridge")
        .expect("Failed to find `Bridge`");
    let b = env
        .new_global_ref(b)
        .expect("Failed to create `Bridge` `GlobalRef`");
    BRIDGE.set(b).expect("`Bridge` already initialized");
    JNI_VERSION_1_6
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onInit(mut env: JNIEnv, obj: JObject, status: jint) {
    let id = env
        .get_field(obj, "backendId", "I")
        .expect("Failed to get backend ID")
        .i()
        .expect("Failed to cast to int") as u64;
    let mut pending = PENDING_INITIALIZATIONS.write().unwrap();
    pending.retain(|v| *v != id);
    if status != 0 {
        error!("Failed to initialize TTS engine");
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onStart(
    mut env: JNIEnv,
    obj: JObject,
    utterance_id: JString,
) {
    let backend_id = env
        .get_field(obj, "backendId", "I")
        .expect("Failed to get backend ID")
        .i()
        .expect("Failed to cast to int") as u64;
    let utterance_id = CString::from(CStr::from_ptr(
        env.get_string(&utterance_id).unwrap().as_ptr(),
    ))
    .into_string()
    .unwrap();
    let utterance_id = utterance_id.parse::<u64>().unwrap();
    let utterance_id = UtteranceId::Android(utterance_id);
    with_callbacks(backend_id, |callbacks| {
        callbacks.utterance_begin(utterance_id)
    });
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onStop(
    mut env: JNIEnv,
    obj: JObject,
    utterance_id: JString,
) {
    let backend_id = env
        .get_field(obj, "backendId", "I")
        .expect("Failed to get backend ID")
        .i()
        .expect("Failed to cast to int") as u64;
    let utterance_id = CString::from(CStr::from_ptr(
        env.get_string(&utterance_id).unwrap().as_ptr(),
    ))
    .into_string()
    .unwrap();
    let utterance_id = utterance_id.parse::<u64>().unwrap();
    let utterance_id = UtteranceId::Android(utterance_id);
    with_callbacks(backend_id, |callbacks| {
        callbacks.utterance_end(utterance_id)
    });
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onDone(
    mut env: JNIEnv,
    obj: JObject,
    utterance_id: JString,
) {
    let backend_id = env
        .get_field(obj, "backendId", "I")
        .expect("Failed to get backend ID")
        .i()
        .expect("Failed to cast to int") as u64;
    let utterance_id = CString::from(CStr::from_ptr(
        env.get_string(&utterance_id).unwrap().as_ptr(),
    ))
    .into_string()
    .unwrap();
    let utterance_id = utterance_id.parse::<u64>().unwrap();
    let utterance_id = UtteranceId::Android(utterance_id);
    with_callbacks(backend_id, |callbacks| {
        callbacks.utterance_stop(utterance_id)
    });
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onError(
    mut env: JNIEnv,
    obj: JObject,
    utterance_id: JString,
) {
    let backend_id = env
        .get_field(obj, "backendId", "I")
        .expect("Failed to get backend ID")
        .i()
        .expect("Failed to cast to int") as u64;
    let utterance_id = CString::from(CStr::from_ptr(
        env.get_string(&utterance_id).unwrap().as_ptr(),
    ))
    .into_string()
    .unwrap();
    let utterance_id = utterance_id.parse::<u64>().unwrap();
    let utterance_id = UtteranceId::Android(utterance_id);
    with_callbacks(backend_id, |callbacks| {
        callbacks.utterance_end(utterance_id)
    });
}

#[derive(Clone)]
pub(crate) struct Android {
    id: BackendId,
    tts: GlobalRef,
    rate: f32,
    pitch: f32,
}

impl Android {
    pub(crate) fn new(callbacks: Arc<Mutex<Callbacks>>) -> Result<Self, Error> {
        info!("Initializing Android backend");
        let bid = NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed);
        let id = BackendId::Android(bid);
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }?;
        let context = unsafe { JObject::from_raw(ctx.context().cast()) };
        let mut env = vm.attach_current_thread_permanently()?;
        let bridge = BRIDGE.get().ok_or(Error::NoneError)?;
        let bridge = env.new_object(bridge, "(I)V", &[(bid as jint).into()])?;
        let tts = env.new_object(
            "android/speech/tts/TextToSpeech",
            "(Landroid/content/Context;Landroid/speech/tts/TextToSpeech$OnInitListener;)V",
            &[(&context).into(), (&bridge).into()],
        )?;
        env.call_method(
            &tts,
            "setOnUtteranceProgressListener",
            "(Landroid/speech/tts/UtteranceProgressListener;)I",
            &[(&bridge).into()],
        )?;
        PENDING_INITIALIZATIONS.write().unwrap().push(bid);
        let tts = env.new_global_ref(tts)?;
        // This hack makes my brain bleed.
        const MAX_WAIT_TIME: Duration = Duration::from_millis(500);
        let start = Instant::now();
        // Wait a max of 500ms for initialization, then return an error to avoid hanging.
        loop {
            {
                let pending = PENDING_INITIALIZATIONS.read().unwrap();
                if !pending.contains(&bid) {
                    break;
                }
                if start.elapsed() > MAX_WAIT_TIME {
                    return Err(Error::OperationFailed);
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        CALLBACKS.lock().unwrap().push((bid, callbacks));
        Ok(Self {
            id,
            tts,
            rate: 1.,
            pitch: 1.,
        })
    }

    fn vm() -> Result<JavaVM, jni::errors::Error> {
        let ctx = ndk_context::android_context();
        unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
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
        let mut env = vm.get_env()?;
        let tts = self.tts.as_obj();
        let text = env.new_string(text)?;
        let queue_mode = if interrupt { 0 } else { 1 };
        let uid = NEXT_UTTERANCE_ID.fetch_add(1, Ordering::Relaxed);
        let id = UtteranceId::Android(uid);
        let uid = env.new_string(uid.to_string())?;
        let rv = env.call_method(
            tts,
            "speak",
            "(Ljava/lang/CharSequence;ILandroid/os/Bundle;Ljava/lang/String;)I",
            &[
                (&text).into(),
                queue_mode.into(),
                (&JObject::null()).into(),
                (&uid).into(),
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
        let mut env = vm.get_env()?;
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
        let mut env = vm.get_env()?;
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
        let mut env = vm.get_env()?;
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
        let mut env = vm.get_env()?;
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

impl Drop for Android {
    fn drop(&mut self) {
        let BackendId::Android(bid) = self.id;
        CALLBACKS.lock().unwrap().retain(|(id, _)| *id != bid);
    }
}
