#[cfg(target_os = "android")]
use std::{
    fs,
    os::raw::c_void,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{Sender, channel},
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
use parking_lot::{Mutex, RwLock};
use tracing::{Span, error, field::Empty, info_span, instrument};

use crate::{Backend, Callbacks, Error, Features, SynthesizedAudio, UtteranceId, Voice};

static BRIDGE: OnceLock<Global<JClass<'static>>> = OnceLock::new();
static NEXT_BACKEND_ID: AtomicU64 = AtomicU64::new(0);
static PENDING_INITIALIZATIONS: RwLock<Vec<u64>> = RwLock::new(Vec::new());
static NEXT_UTTERANCE_ID: AtomicU64 = AtomicU64::new(0);
// The JNI callbacks below only receive a backend ID from Java, so per-instance
// callbacks must be reachable through a process-wide registry. Each backend's span rides
// along so JNI callback executions can be connected back to the backend that spawned them.
type CallbacksEntry = (u64, Span, Arc<Mutex<Callbacks>>);
static CALLBACKS: Mutex<Vec<CallbacksEntry>> = Mutex::new(Vec::new());
/// In-flight `synthesize` calls by utterance ID; their progress events complete the blocked
/// synthesizing thread instead of reaching user callbacks.
type SynthesisEntry = (u64, Sender<Result<(), Error>>);
static SYNTHESES: Mutex<Vec<SynthesisEntry>> = Mutex::new(Vec::new());

fn is_synthesis(utterance_id: u64) -> bool {
    SYNTHESES.lock().iter().any(|(id, _)| *id == utterance_id)
}

fn take_synthesis(utterance_id: u64) -> Option<Sender<Result<(), Error>>> {
    let mut syntheses = SYNTHESES.lock();
    syntheses
        .iter()
        .position(|(id, _)| *id == utterance_id)
        .map(|index| syntheses.remove(index).1)
}

fn with_callbacks(backend_id: u64, f: impl FnOnce(&mut Callbacks)) {
    // Release the registry lock before the user callback runs.
    let (span, callbacks) = {
        let registry = CALLBACKS.lock();
        let (_, span, callbacks) = registry
            .iter()
            .find(|(id, _, _)| *id == backend_id)
            .expect("No callbacks registered for backend");
        (span.clone(), callbacks.clone())
    };
    let _entered = span.enter();
    let mut callbacks = callbacks.lock();
    f(&mut callbacks);
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
#[instrument(level = "debug", skip_all)]
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
#[instrument(level = "debug", skip(env, obj), fields(backend_id = Empty))]
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
        Span::current().record("backend_id", id);
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
#[instrument(level = "trace", skip_all)]
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
        if is_synthesis(utterance_id) {
            return Ok(());
        }
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
#[instrument(level = "trace", skip_all)]
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
        if let Some(done) = take_synthesis(utterance_id) {
            let _ = done.send(Err(Error::OperationFailed("synthesis stopped")));
            return Ok(());
        }
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
#[instrument(level = "trace", skip_all)]
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
        if let Some(done) = take_synthesis(utterance_id) {
            let _ = done.send(Ok(()));
            return Ok(());
        }
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
#[instrument(level = "trace", skip_all)]
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
        if let Some(done) = take_synthesis(utterance_id) {
            let _ = done.send(Err(Error::OperationFailed("synthesis")));
            return Ok(());
        }
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
    id: u64,
    tts: Arc<Global<JObject<'static>>>,
    rate: f32,
    pitch: f32,
}

impl Android {
    #[instrument(level = "info", skip(callbacks), err)]
    pub(crate) fn new(callbacks: Arc<Mutex<Callbacks>>) -> Result<Self, Error> {
        const MAX_WAIT_TIME: Duration = Duration::from_millis(500);
        let bid = NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed);
        let tts = Self::vm().attach_current_thread(|env| -> Result<_, Error> {
            let ctx = ndk_context::android_context();
            let context = unsafe { JObject::from_raw(env, ctx.context().cast()) };
            let bridge = BRIDGE.get().ok_or(Error::BackendUnavailable(
                "Android TTS bridge not registered",
            ))?;
            let bid_jint =
                jint::try_from(bid).map_err(|_| Error::OperationFailed("backend id conversion"))?;
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
                    return Err(Error::BackendUnavailable(
                        "Android TTS initialization timed out",
                    ));
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        CALLBACKS
            .lock()
            .push((bid, info_span!("android", backend_id = bid), callbacks));
        Ok(Self {
            id: bid,
            tts: Arc::new(tts),
            rate: 1.,
            pitch: 1.,
        })
    }

    #[instrument(level = "trace")]
    fn vm() -> JavaVM {
        let ctx = ndk_context::android_context();
        unsafe { JavaVM::from_raw(ctx.vm().cast()) }
    }
}

impl Backend for Android {
    #[instrument(level = "trace", skip(self))]
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: true,
            pitch: true,
            is_speaking: true,
            utterance_callbacks: true,
            synthesis: true,
            ..Default::default()
        }
    }

    #[instrument(level = "debug", skip(self), err)]
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
            Err(Error::OperationFailed("speak"))
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn synthesize(&mut self, text: &str) -> Result<SynthesizedAudio, Error> {
        let uid = NEXT_UTTERANCE_ID.fetch_add(1, Ordering::Relaxed);
        let id = UtteranceId::Android(uid);
        // The engine can only synthesize to a file, so stage a temporary one in the app's cache
        // directory: dropping it deletes it on every exit path, and the OS evicts anything a
        // hard kill leaves behind.
        let cache_dir = Self::vm().attach_current_thread(|env| -> Result<String, Error> {
            let ctx = ndk_context::android_context();
            let context = unsafe { JObject::from_raw(env, ctx.context().cast()) };
            let cache_dir = env
                .call_method(
                    &context,
                    jni_str!("getCacheDir"),
                    jni_sig!("()Ljava/io/File;"),
                    &[],
                )?
                .l()?;
            let cache_dir = env
                .call_method(
                    &cache_dir,
                    jni_str!("getAbsolutePath"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            let cache_dir = env.cast_local::<JString>(cache_dir)?;
            Ok(cache_dir.to_string())
        })?;
        let file = tempfile::Builder::new()
            .prefix("tts-synthesis-")
            .suffix(".wav")
            .tempfile_in(&cache_dir)?;
        let path = file
            .path()
            .to_str()
            .ok_or(Error::OperationFailed("synthesis path encoding"))?
            .to_string();
        let (done, completion) = channel();
        SYNTHESES.lock().push((uid, done));
        with_callbacks(self.id, |callbacks| {
            callbacks.synthesis_begin(id);
        });
        let rv = Self::vm().attach_current_thread(|env| -> Result<jint, Error> {
            let text = env.new_string(text)?;
            let jpath = env.new_string(&path)?;
            let file = env.new_object(
                jni_str!("java/io/File"),
                jni_sig!("(Ljava/lang/String;)V"),
                &[(&jpath).into()],
            )?;
            let uid = env.new_string(uid.to_string())?;
            let rv = env.call_method(
                self.tts.as_obj(),
                jni_str!("synthesizeToFile"),
                jni_sig!(
                    "(Ljava/lang/CharSequence;Landroid/os/Bundle;Ljava/io/File;Ljava/lang/String;)I"
                ),
                &[
                    (&text).into(),
                    (&JObject::null()).into(),
                    (&file).into(),
                    (&uid).into(),
                ],
            )?;
            Ok(rv.i()?)
        });
        match rv {
            Ok(0) => {}
            Ok(_) => {
                take_synthesis(uid);
                return Err(Error::OperationFailed("synthesizeToFile"));
            }
            Err(e) => {
                take_synthesis(uid);
                return Err(e);
            }
        }
        completion
            .recv()
            .map_err(|_| Error::OperationFailed("synthesis completion"))??;
        let bytes = fs::read(file.path())?;
        let audio = SynthesizedAudio::from_wav(bytes)?;
        with_callbacks(self.id, |callbacks| {
            callbacks.synthesis_complete(id);
        });
        Ok(audio)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn stop(&mut self) -> Result<(), Error> {
        let rv = Self::vm().attach_current_thread(|env| -> Result<jint, Error> {
            let rv = env.call_method(self.tts.as_obj(), jni_str!("stop"), jni_sig!("()I"), &[])?;
            Ok(rv.i()?)
        })?;
        if rv == 0 {
            Ok(())
        } else {
            Err(Error::OperationFailed("stop"))
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn min_rate(&self) -> f32 {
        0.1
    }

    #[instrument(level = "trace", skip(self))]
    fn max_rate(&self) -> f32 {
        10.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_rate(&self) -> f32 {
        1.
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_rate(&self) -> Result<f32, Error> {
        Ok(self.rate)
    }

    #[instrument(level = "debug", skip(self), err)]
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
            Err(Error::OperationFailed("set_rate"))
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn min_pitch(&self) -> f32 {
        0.1
    }

    #[instrument(level = "trace", skip(self))]
    fn max_pitch(&self) -> f32 {
        2.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_pitch(&self) -> f32 {
        1.
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_pitch(&self) -> Result<f32, Error> {
        Ok(self.pitch)
    }

    #[instrument(level = "debug", skip(self), err)]
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
            Err(Error::OperationFailed("set_pitch"))
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn min_volume(&self) -> f32 {
        todo!()
    }

    #[instrument(level = "trace", skip(self))]
    fn max_volume(&self) -> f32 {
        todo!()
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_volume(&self) -> f32 {
        todo!()
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_volume(&self) -> Result<f32, Error> {
        todo!()
    }

    #[instrument(level = "debug", skip(self, _volume), err)]
    fn set_volume(&mut self, _volume: f32) -> Result<(), Error> {
        todo!()
    }

    #[instrument(level = "trace", skip(self), err, ret)]
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

impl Drop for Android {
    fn drop(&mut self) {
        CALLBACKS.lock().retain(|(id, _, _)| *id != self.id);
    }
}
