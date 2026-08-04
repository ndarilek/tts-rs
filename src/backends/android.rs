#[cfg(target_os = "android")]
use std::{
    os::raw::c_void,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use jni::{
    EnvUnowned, JavaVM,
    errors::ThrowRuntimeExAndDefault,
    jni_sig, jni_str,
    objects::{Global, JClass, JObject, JString},
    sys::{self, JNI_VERSION_1_6, jfloat, jint},
};
use log::{error, info};
use parking_lot::{Mutex, RwLock};

use crate::{Backend, BackendId, Callbacks, Error, Features, UtteranceId, Voice};

static BRIDGE: OnceLock<Global<JClass<'static>>> = OnceLock::new();
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
        .iter()
        .find(|(id, _)| *id == backend_id)
        .expect("No callbacks registered for backend")
        .1
        .clone();
    let mut callbacks = callbacks.lock();
    f(&mut callbacks);
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "system" fn JNI_OnLoad(vm: *mut sys::JavaVM, _: *mut c_void) -> jint {
    let vm = unsafe { JavaVM::from_raw(vm) };
    vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let b = env.find_class(jni_str!("rs/tts/Bridge"))?;
        let b = env.new_global_ref(&b)?;
        BRIDGE.set(b).expect("`Bridge` already initialized");
        Ok(())
    })
    .expect("Failed to initialize `Bridge`");
    JNI_VERSION_1_6
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onInit(
    mut env: EnvUnowned,
    obj: JObject,
    status: jint,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let id = env
            .get_field(&obj, jni_str!("backendId"), jni_sig!(int))?
            .i()?;
        let id = u64::try_from(id).expect("Backend ID must be non-negative");
        let mut pending = PENDING_INITIALIZATIONS.write();
        pending.retain(|v| *v != id);
        if status != 0 {
            error!("Failed to initialize TTS engine");
        }
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onStart(
    mut env: EnvUnowned,
    obj: JObject,
    utterance_id: JString,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let backend_id = env
            .get_field(&obj, jni_str!("backendId"), jni_sig!(int))?
            .i()?;
        let backend_id = u64::try_from(backend_id).expect("Backend ID must be non-negative");
        let utterance_id = utterance_id.to_string().parse::<u64>().unwrap();
        let utterance_id = UtteranceId::Android(utterance_id);
        with_callbacks(backend_id, |callbacks| {
            callbacks.utterance_begin(utterance_id);
        });
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onStop(
    mut env: EnvUnowned,
    obj: JObject,
    utterance_id: JString,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let backend_id = env
            .get_field(&obj, jni_str!("backendId"), jni_sig!(int))?
            .i()?;
        let backend_id = u64::try_from(backend_id).expect("Backend ID must be non-negative");
        let utterance_id = utterance_id.to_string().parse::<u64>().unwrap();
        let utterance_id = UtteranceId::Android(utterance_id);
        with_callbacks(backend_id, |callbacks| {
            callbacks.utterance_end(utterance_id);
        });
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onDone(
    mut env: EnvUnowned,
    obj: JObject,
    utterance_id: JString,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let backend_id = env
            .get_field(&obj, jni_str!("backendId"), jni_sig!(int))?
            .i()?;
        let backend_id = u64::try_from(backend_id).expect("Backend ID must be non-negative");
        let utterance_id = utterance_id.to_string().parse::<u64>().unwrap();
        let utterance_id = UtteranceId::Android(utterance_id);
        with_callbacks(backend_id, |callbacks| {
            callbacks.utterance_stop(utterance_id);
        });
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_rs_tts_Bridge_onError(
    mut env: EnvUnowned,
    obj: JObject,
    utterance_id: JString,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let backend_id = env
            .get_field(&obj, jni_str!("backendId"), jni_sig!(int))?
            .i()?;
        let backend_id = u64::try_from(backend_id).expect("Backend ID must be non-negative");
        let utterance_id = utterance_id.to_string().parse::<u64>().unwrap();
        let utterance_id = UtteranceId::Android(utterance_id);
        with_callbacks(backend_id, |callbacks| {
            callbacks.utterance_end(utterance_id);
        });
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}

#[derive(Clone)]
pub(crate) struct Android {
    id: BackendId,
    tts: Arc<Global<JObject<'static>>>,
    rate: f32,
    pitch: f32,
}

impl Android {
    pub(crate) fn new(callbacks: Arc<Mutex<Callbacks>>) -> Result<Self, Error> {
        const MAX_WAIT_TIME: Duration = Duration::from_millis(500);
        info!("Initializing Android backend");
        let bid = NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed);
        let id = BackendId::Android(bid);
        let tts = Self::vm().attach_current_thread(|env| -> Result<_, Error> {
            let ctx = ndk_context::android_context();
            let context = unsafe { JObject::from_raw(env, ctx.context().cast()) };
            let bridge = BRIDGE.get().ok_or(Error::NoneError)?;
            let bid_jint = jint::try_from(bid).map_err(|_| Error::OperationFailed)?;
            let bridge = env.new_object(bridge, jni_sig!("(I)V"), &[bid_jint.into()])?;
            let tts = env.new_object(
                jni_str!("android/speech/tts/TextToSpeech"),
                jni_sig!(
                    "(Landroid/content/Context;Landroid/speech/tts/TextToSpeech$OnInitListener;)V"
                ),
                &[(&context).into(), (&bridge).into()],
            )?;
            env.call_method(
                &tts,
                jni_str!("setOnUtteranceProgressListener"),
                jni_sig!("(Landroid/speech/tts/UtteranceProgressListener;)I"),
                &[(&bridge).into()],
            )?;
            PENDING_INITIALIZATIONS.write().push(bid);
            Ok(env.new_global_ref(&tts)?)
        })?;
        // This hack makes my brain bleed.
        let start = Instant::now();
        // Wait a max of 500ms for initialization, then return an error to avoid hanging.
        loop {
            {
                let pending = PENDING_INITIALIZATIONS.read();
                if !pending.contains(&bid) {
                    break;
                }
                if start.elapsed() > MAX_WAIT_TIME {
                    return Err(Error::OperationFailed);
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        CALLBACKS.lock().push((bid, callbacks));
        Ok(Self {
            id,
            tts: Arc::new(tts),
            rate: 1.,
            pitch: 1.,
        })
    }

    fn vm() -> JavaVM {
        let ctx = ndk_context::android_context();
        unsafe { JavaVM::from_raw(ctx.vm().cast()) }
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
        let uid = NEXT_UTTERANCE_ID.fetch_add(1, Ordering::Relaxed);
        let id = UtteranceId::Android(uid);
        let rv = Self::vm().attach_current_thread(|env| -> Result<jint, Error> {
            let text = env.new_string(text)?;
            let queue_mode = jint::from(!interrupt);
            let uid = env.new_string(uid.to_string())?;
            let rv = env.call_method(
                self.tts.as_obj(),
                jni_str!("speak"),
                jni_sig!("(Ljava/lang/CharSequence;ILandroid/os/Bundle;Ljava/lang/String;)I"),
                &[
                    (&text).into(),
                    queue_mode.into(),
                    (&JObject::null()).into(),
                    (&uid).into(),
                ],
            )?;
            Ok(rv.i()?)
        })?;
        if rv == 0 {
            Ok(Some(id))
        } else {
            Err(Error::OperationFailed)
        }
    }

    fn stop(&mut self) -> Result<(), Error> {
        let rv = Self::vm().attach_current_thread(|env| -> Result<jint, Error> {
            let rv = env.call_method(self.tts.as_obj(), jni_str!("stop"), jni_sig!("()I"), &[])?;
            Ok(rv.i()?)
        })?;
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
        let rate = rate as jfloat;
        let rv = Self::vm().attach_current_thread(|env| -> Result<jint, Error> {
            let rv = env.call_method(
                self.tts.as_obj(),
                jni_str!("setSpeechRate"),
                jni_sig!("(F)I"),
                &[rate.into()],
            )?;
            Ok(rv.i()?)
        })?;
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
        let pitch = pitch as jfloat;
        let rv = Self::vm().attach_current_thread(|env| -> Result<jint, Error> {
            let rv = env.call_method(
                self.tts.as_obj(),
                jni_str!("setPitch"),
                jni_sig!("(F)I"),
                &[pitch.into()],
            )?;
            Ok(rv.i()?)
        })?;
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
        Self::vm().attach_current_thread(|env| -> Result<bool, Error> {
            let rv = env.call_method(
                self.tts.as_obj(),
                jni_str!("isSpeaking"),
                jni_sig!("()Z"),
                &[],
            )?;
            Ok(rv.z()?)
        })
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
        CALLBACKS.lock().retain(|(id, _)| *id != bid);
    }
}
