#[cfg(target_os = "android")]
use std::{
    ffi::c_void,
    fs,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{Sender, channel},
    },
};

use jni::{
    Env, JavaVM, NativeMethod, jni_sig, jni_str, native_method,
    objects::{Global, JClass, JObject, JString},
    sys::{jboolean, jfloat, jint},
};
use parking_lot::{Condvar, Mutex};
use tracing::{Span, error, field::Empty, info_span, instrument};

use crate::{Backend, Callbacks, Error, Features, SynthesizedAudio, UtteranceId, Voice};

/// `rs.tts.Bridge`, compiled from _android/Bridge.java_ by the build script.
static BRIDGE_DEX: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/android/bridge.dex"));
static BRIDGE: OnceLock<Global<JClass<'static>>> = OnceLock::new();
static NEXT_BACKEND_ID: AtomicU64 = AtomicU64::new(0);
static NEXT_UTTERANCE_ID: AtomicU64 = AtomicU64::new(0);
/// App-supplied VM and context, consulted ahead of `ndk-context`; see [`set_context`].
static CONTEXT: Mutex<Option<(JavaVM, Arc<Global<JObject<'static>>>)>> = Mutex::new(None);

/// Trades the supplied context for its application context when one exists, so holding the
/// resulting reference for a backend's lifetime doesn't pin an `Activity`.
fn application_context(
    env: &mut Env,
    context: &JObject<'_>,
) -> Result<Global<JObject<'static>>, Error> {
    let app = env
        .call_method(
            context,
            jni_str!("getApplicationContext"),
            jni_sig!("()Landroid/content/Context;"),
            &[],
        )?
        .l()?;
    if app.is_null() {
        // Not yet attached to a base context; the supplied one is all there is.
        Ok(env.new_global_ref(context)?)
    } else {
        Ok(env.new_global_ref(&app)?)
    }
}

/// Supplies the `JavaVM` and `Context` every subsequently constructed backend uses, taking
/// precedence over whatever `ndk-context` publishes.
///
/// Without this, backends resolve both through `ndk-context`, whose slot is typically owned by an
/// `Activity`: a headless process (say, a foreground service that never shows UI) has nothing
/// published there, and windowing crates like `tao` release it when their `Activity` is destroyed.
/// Supplying a process-lived `Context` — a service works fine — keeps speech available in both
/// cases.
///
/// The context is traded for its application context when one exists and held as a global
/// reference. Calling again replaces the stored pair; backends keep whatever they were constructed
/// with.
///
/// # Safety
///
/// `vm` must be a valid `JavaVM` pointer and `context` a valid JNI reference to an
/// `android.content.Context`. Both only need to remain valid for the duration of the call.
///
/// # Errors
///
/// Returns an error if attaching to the VM or creating the global reference fails.
#[instrument(level = "info", skip_all, err)]
pub unsafe fn set_context(vm: *mut c_void, context: *mut c_void) -> Result<(), Error> {
    let vm = unsafe { JavaVM::from_raw(vm.cast()) };
    let context = vm.attach_current_thread(|env| -> Result<_, Error> {
        let context = unsafe { JObject::from_raw(env, context.cast()) };
        application_context(env, &context)
    })?;
    *CONTEXT.lock() = Some((vm, Arc::new(context)));
    Ok(())
}

/// The override if set, otherwise a fresh read of `ndk-context` — per construction, not cached,
/// since that slot's owner can republish it with a new `Activity`.
#[instrument(level = "debug", err)]
fn vm_and_context() -> Result<(JavaVM, Arc<Global<JObject<'static>>>), Error> {
    if let Some((vm, context)) = &*CONTEXT.lock() {
        return Ok((vm.clone(), Arc::clone(context)));
    }
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };
    let context = vm.attach_current_thread(|env| -> Result<_, Error> {
        let context = unsafe { JObject::from_raw(env, ctx.context().cast()) };
        application_context(env, &context)
    })?;
    Ok((vm, Arc::new(context)))
}

/// Per-backend engine state, reached from the JNI callbacks through [`INSTANCES`].
///
/// Android reports the engine ready on the app's Java main thread, which a `NativeActivity` app
/// keeps blocked until its event loop acknowledges each lifecycle callback. Waiting for that from
/// the event loop's own thread deadlocks, so nothing here waits: early utterances queue and
/// `on_init` replays them.
struct Instance {
    state: Mutex<State>,
    ready: Condvar,
}

struct State {
    /// `None` until `on_init` reports, then whether the engine came up.
    initialized: Option<bool>,
    /// `on_init` can arrive before this is set: with no engine installed at all, Android dispatches
    /// it synchronously from the `TextToSpeech` constructor.
    tts: Option<Arc<Global<JObject<'static>>>>,
    queued: Vec<Queued>,
    /// What the app last asked for, restored after a replay leaves the engine on the last
    /// utterance's settings.
    rate: f32,
    pitch: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            initialized: None,
            tts: None,
            queued: Vec::new(),
            rate: 1.,
            pitch: 1.,
        }
    }
}

struct Queued {
    id: u64,
    text: String,
    interrupt: bool,
    /// Frozen at the `speak` call; the app can change them again before the queue drains.
    rate: f32,
    pitch: f32,
}

static INSTANCES: Mutex<Vec<(u64, Arc<Instance>)>> = Mutex::new(Vec::new());

fn instance(backend_id: u64) -> Option<Arc<Instance>> {
    INSTANCES
        .lock()
        .iter()
        .find(|(id, _)| *id == backend_id)
        .map(|(_, instance)| Arc::clone(instance))
}

/// Leaves late callbacks for the backend with nothing to find, so they become no-ops.
fn unregister(backend_id: u64) {
    INSTANCES.lock().retain(|(id, _)| *id != backend_id);
    CALLBACKS.lock().retain(|(id, _, _)| *id != backend_id);
}

/// `TextToSpeech` only writes these into its parameter bundle, so they work while disconnected.
fn set_rate_pitch_now(
    env: &mut Env,
    tts: &JObject<'static>,
    rate: f32,
    pitch: f32,
) -> Result<(), Error> {
    for (method, value, failure) in [
        (jni_str!("setSpeechRate"), rate, "set_rate"),
        (jni_str!("setPitch"), pitch, "set_pitch"),
    ] {
        let rv = env.call_method(tts, method, jni_sig!("(F)I"), &[(value as jfloat).into()])?;
        if rv.i()? != 0 {
            return Err(Error::OperationFailed(failure));
        }
    }
    Ok(())
}

fn speak_now(
    env: &mut Env,
    tts: &JObject<'static>,
    id: u64,
    text: &str,
    interrupt: bool,
) -> Result<(), Error> {
    let text = env.new_string(text)?;
    let queue_mode = jint::from(!interrupt);
    let id = env.new_string(id.to_string())?;
    let rv = env.call_method(
        tts,
        jni_str!("speak"),
        jni_sig!("(Ljava/lang/CharSequence;ILandroid/os/Bundle;Ljava/lang/String;)I"),
        &[
            (&text).into(),
            queue_mode.into(),
            (&JObject::null()).into(),
            (&id).into(),
        ],
    )?;
    if rv.i()? == 0 {
        Ok(())
    } else {
        Err(Error::OperationFailed("speak"))
    }
}

// The JNI callbacks only receive a backend ID, so per-instance callbacks need a process-wide
// registry. Each backend's span rides along to connect callback executions back to it.
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

/// Bound with `RegisterNatives`, since the bridge's own class loader puts it out of reach of
/// symbol-based binding. Names are `lowerCamelCase`d and must match _android/Bridge.java_.
const BRIDGE_METHODS: &[NativeMethod] = &[
    native_method! { fn on_init(status: jint), },
    native_method! { fn on_start(utterance_id: JString), },
    native_method! { fn on_stop(utterance_id: JString, interrupted: jboolean), },
    native_method! { fn on_done(utterance_id: JString), },
    native_method! { fn on_error(utterance_id: JString), },
];

/// Defines the class from [`BRIDGE_DEX`] on first use — deferred, because the parent class loader
/// comes from a context that only exists once the app runs.
#[instrument(level = "debug", skip_all, err)]
fn bridge(env: &mut Env, context: &JObject<'_>) -> Result<&'static Global<JClass<'static>>, Error> {
    if let Some(bridge) = BRIDGE.get() {
        return Ok(bridge);
    }
    let parent = env
        .call_method(
            context,
            jni_str!("getClassLoader"),
            jni_sig!("()Ljava/lang/ClassLoader;"),
            &[],
        )?
        .l()?;
    // The class loader keeps referencing the buffer, so leak a copy of the read-only dex data.
    let dex = Box::leak(Box::<[u8]>::from(BRIDGE_DEX));
    let buffer = unsafe { env.new_direct_byte_buffer(dex.as_mut_ptr(), dex.len()) }?;
    let loader = env.new_object(
        jni_str!("dalvik/system/InMemoryDexClassLoader"),
        jni_sig!("(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V"),
        &[(&buffer).into(), (&parent).into()],
    )?;
    let name = env.new_string("rs.tts.Bridge")?;
    let class = env
        .call_method(
            &loader,
            jni_str!("loadClass"),
            jni_sig!("(Ljava/lang/String;)Ljava/lang/Class;"),
            &[(&name).into()],
        )?
        .l()?;
    let class = env.cast_local::<JClass>(class)?;
    unsafe { env.register_native_methods(&class, BRIDGE_METHODS) }?;
    let class = env.new_global_ref(&class)?;
    // A racing thread may have won, in which case its class wins and ours is dropped.
    Ok(BRIDGE.get_or_init(|| class))
}

/// The `backendId` a bridge was constructed with, the only instance state a callback carries.
fn backend_id(env: &mut Env, bridge: JObject) -> jni::errors::Result<u64> {
    let id = env
        .get_field(bridge, jni_str!("backendId"), jni_sig!(int))?
        .i()?;
    Ok(u64::try_from(id).expect("Backend ID must be non-negative"))
}

// Taken by value so the callbacks, whose signatures `native_method!` fixes, can hand off ownership.
#[allow(clippy::needless_pass_by_value)]
fn utterance(
    env: &mut Env,
    bridge: JObject,
    utterance_id: JString,
) -> jni::errors::Result<(u64, u64)> {
    let backend_id = backend_id(env, bridge)?;
    let utterance_id = utterance_id
        .try_to_string(env)?
        .parse()
        .expect("Utterance IDs are generated as integers");
    Ok((backend_id, utterance_id))
}

#[instrument(level = "debug", skip(env, this), fields(backend_id = Empty))]
fn on_init(env: &mut Env, this: JObject, status: jint) -> jni::errors::Result<()> {
    let id = backend_id(env, this)?;
    Span::current().record("backend_id", id);
    let Some(instance) = instance(id) else {
        // Dropped before the engine finished connecting.
        return Ok(());
    };
    // Collect under the lock, then release it: replaying re-enters Java, which can call the
    // progress callbacks back synchronously.
    let (tts, queued, rate, pitch) = {
        let mut state = instance.state.lock();
        state.initialized = Some(status == 0);
        instance.ready.notify_all();
        (
            state.tts.clone(),
            std::mem::take(&mut state.queued),
            state.rate,
            state.pitch,
        )
    };
    if status != 0 {
        error!("Failed to initialize TTS engine");
        // Nothing will speak these, so release anyone waiting on their IDs.
        for utterance in queued {
            with_callbacks(id, |callbacks| {
                callbacks.utterance_stop(UtteranceId::Android(utterance.id));
            });
        }
        return Ok(());
    }
    let Some(tts) = tts else {
        // Unreachable on success: that `onInit` arrives long after the constructor returned.
        return Ok(());
    };
    for utterance in queued {
        let replay = set_rate_pitch_now(env, tts.as_obj(), utterance.rate, utterance.pitch)
            .and_then(|()| {
                speak_now(
                    env,
                    tts.as_obj(),
                    utterance.id,
                    &utterance.text,
                    utterance.interrupt,
                )
            });
        if let Err(e) = replay {
            error!("Failed to speak queued utterance: {e}");
        }
    }
    // The replay left the engine on the last utterance's settings.
    if let Err(e) = set_rate_pitch_now(env, tts.as_obj(), rate, pitch) {
        error!("Failed to restore rate and pitch: {e}");
    }
    Ok(())
}

#[instrument(level = "trace", skip_all)]
fn on_start(env: &mut Env, this: JObject, utterance_id: JString) -> jni::errors::Result<()> {
    let (backend_id, utterance_id) = utterance(env, this, utterance_id)?;
    // A `synthesize` call reports its own progress.
    if is_synthesis(utterance_id) {
        return Ok(());
    }
    with_callbacks(backend_id, |callbacks| {
        callbacks.utterance_begin(UtteranceId::Android(utterance_id));
    });
    Ok(())
}

#[instrument(level = "trace", skip_all)]
fn on_stop(
    env: &mut Env,
    this: JObject,
    utterance_id: JString,
    _interrupted: jboolean,
) -> jni::errors::Result<()> {
    let (backend_id, utterance_id) = utterance(env, this, utterance_id)?;
    if let Some(done) = take_synthesis(utterance_id) {
        let _ = done.send(Err(Error::OperationFailed("synthesis stopped")));
        return Ok(());
    }
    with_callbacks(backend_id, |callbacks| {
        callbacks.utterance_stop(UtteranceId::Android(utterance_id));
    });
    Ok(())
}

#[instrument(level = "trace", skip_all)]
fn on_done(env: &mut Env, this: JObject, utterance_id: JString) -> jni::errors::Result<()> {
    let (backend_id, utterance_id) = utterance(env, this, utterance_id)?;
    if let Some(done) = take_synthesis(utterance_id) {
        let _ = done.send(Ok(()));
        return Ok(());
    }
    with_callbacks(backend_id, |callbacks| {
        callbacks.utterance_end(UtteranceId::Android(utterance_id));
    });
    Ok(())
}

#[instrument(level = "trace", skip_all)]
fn on_error(env: &mut Env, this: JObject, utterance_id: JString) -> jni::errors::Result<()> {
    let (backend_id, utterance_id) = utterance(env, this, utterance_id)?;
    if let Some(done) = take_synthesis(utterance_id) {
        let _ = done.send(Err(Error::OperationFailed("synthesis")));
        return Ok(());
    }
    with_callbacks(backend_id, |callbacks| {
        callbacks.utterance_end(UtteranceId::Android(utterance_id));
    });
    Ok(())
}

#[derive(Clone)]
pub(crate) struct Android {
    id: u64,
    /// Holds rate and pitch too, since the post-init drain needs them.
    instance: Arc<Instance>,
    tts: Arc<Global<JObject<'static>>>,
    vm: JavaVM,
    /// Application context, owned for the backend's lifetime so nothing outside the crate can
    /// invalidate it.
    context: Arc<Global<JObject<'static>>>,
}

impl Android {
    pub(crate) const NAME: &str = "Android";

    /// Always worth trying: whether the JNI bridge is registered only surfaces on init.
    #[instrument(level = "debug", ret)]
    pub(crate) fn is_available() -> bool {
        true
    }

    /// Returns as soon as the engine has been *asked* to connect; see [`Instance`] for why it must
    /// not wait for the answer.
    #[instrument(level = "info", skip(callbacks), err)]
    pub(crate) fn new(callbacks: Arc<Mutex<Callbacks>>) -> Result<Self, Error> {
        let bid = NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed);
        let instance = Arc::new(Instance {
            state: Mutex::new(State::default()),
            ready: Condvar::new(),
        });
        // Both live before the constructor runs, which calls `onInit` from inside itself when no
        // engine is installed.
        INSTANCES.lock().push((bid, Arc::clone(&instance)));
        CALLBACKS
            .lock()
            .push((bid, info_span!("android", backend_id = bid), callbacks));
        let resolved = vm_and_context();
        let (vm, context) = resolved.inspect_err(|_| unregister(bid))?;
        let tts = vm.attach_current_thread(|env| -> Result<_, Error> {
            let bridge = bridge(env, context.as_obj())?;
            let bid_jint =
                jint::try_from(bid).map_err(|_| Error::OperationFailed("backend id conversion"))?;
            let bridge = env.new_object(bridge, jni_sig!("(I)V"), &[bid_jint.into()])?;
            let tts = env.new_object(
                jni_str!("android/speech/tts/TextToSpeech"),
                jni_sig!(
                    "(Landroid/content/Context;Landroid/speech/tts/TextToSpeech$OnInitListener;)V"
                ),
                &[(context.as_obj()).into(), (&bridge).into()],
            )?;
            env.call_method(
                &tts,
                jni_str!("setOnUtteranceProgressListener"),
                jni_sig!("(Landroid/speech/tts/UtteranceProgressListener;)I"),
                &[(&bridge).into()],
            )?;
            Ok(env.new_global_ref(&tts)?)
        });
        // Nothing will answer for this ID now.
        let tts = tts.inspect_err(|_| unregister(bid))?;
        let tts = Arc::new(tts);
        // `on_init` needs this to replay the queue.
        instance.state.lock().tts = Some(Arc::clone(&tts));
        Ok(Self {
            id: bid,
            instance,
            tts,
            vm,
            context,
        })
    }

    /// Blocks until the engine answers. Only [`Backend::synthesize`] needs it, being blocking by
    /// nature; see [`Instance`] for the thread that rules out.
    #[instrument(level = "debug", skip(self), err)]
    fn wait_until_ready(&self) -> Result<(), Error> {
        let mut state = self.instance.state.lock();
        while state.initialized.is_none() {
            self.instance.ready.wait(&mut state);
        }
        if state.initialized == Some(true) {
            Ok(())
        } else {
            Err(Error::BackendUnavailable("Android TTS engine"))
        }
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
        // Queued rather than refused while the engine connects; the ID is live either way.
        {
            let mut state = self.instance.state.lock();
            match state.initialized {
                None => {
                    if interrupt {
                        state.queued.clear();
                    }
                    let (rate, pitch) = (state.rate, state.pitch);
                    state.queued.push(Queued {
                        id: uid,
                        text: text.to_owned(),
                        interrupt,
                        rate,
                        pitch,
                    });
                    return Ok(Some(id));
                }
                Some(false) => return Err(Error::BackendUnavailable("Android TTS engine")),
                Some(true) => {}
            }
        }
        self.vm
            .attach_current_thread(|env| speak_now(env, self.tts.as_obj(), uid, text, interrupt))?;
        Ok(Some(id))
    }

    #[instrument(level = "debug", skip(self), err)]
    fn synthesize(&mut self, text: &str) -> Result<SynthesizedAudio, Error> {
        self.wait_until_ready()?;
        let uid = NEXT_UTTERANCE_ID.fetch_add(1, Ordering::Relaxed);
        let id = UtteranceId::Android(uid);
        // The engine only synthesizes to a file. In the cache directory, so dropping it cleans up
        // on every exit path and the OS evicts whatever a hard kill leaves behind.
        let cache_dir = self
            .vm
            .attach_current_thread(|env| -> Result<String, Error> {
                let cache_dir = env
                    .call_method(
                        self.context.as_obj(),
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
        let rv = self.vm.attach_current_thread(|env| -> Result<jint, Error> {
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
        // Discarding the queue is what stopping means for utterances the engine never saw; report
        // them the way it reports flushing its own.
        let (initialized, queued) = {
            let mut state = self.instance.state.lock();
            (state.initialized, std::mem::take(&mut state.queued))
        };
        for utterance in queued {
            with_callbacks(self.id, |callbacks| {
                callbacks.utterance_stop(UtteranceId::Android(utterance.id));
            });
        }
        // `TextToSpeech.stop` only errors while disconnected, when there is nothing to stop.
        if initialized != Some(true) {
            return Ok(());
        }
        let rv = self
            .vm
            .attach_current_thread(|env| -> Result<jint, Error> {
                let rv =
                    env.call_method(self.tts.as_obj(), jni_str!("stop"), jni_sig!("()I"), &[])?;
                Ok(rv.i()?)
            })?;
        if rv == 0 {
            Ok(())
        } else {
            Err(Error::OperationFailed("stop"))
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn pause(&mut self) -> Result<(), Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err)]
    fn resume(&mut self) -> Result<(), Error> {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_paused(&self) -> Result<bool, Error> {
        unimplemented!()
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
        Ok(self.instance.state.lock().rate)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        let pitch = self.instance.state.lock().pitch;
        self.vm
            .attach_current_thread(|env| set_rate_pitch_now(env, self.tts.as_obj(), rate, pitch))?;
        self.instance.state.lock().rate = rate;
        Ok(())
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
        Ok(self.instance.state.lock().pitch)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        let rate = self.instance.state.lock().rate;
        self.vm
            .attach_current_thread(|env| set_rate_pitch_now(env, self.tts.as_obj(), rate, pitch))?;
        self.instance.state.lock().pitch = pitch;
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn min_volume(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self))]
    fn max_volume(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_volume(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_volume(&self) -> Result<f32, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self, _volume), err)]
    fn set_volume(&mut self, _volume: f32) -> Result<(), Error> {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_speaking(&self) -> Result<bool, Error> {
        self.vm.attach_current_thread(|env| -> Result<bool, Error> {
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
        unregister(self.id);
    }
}
