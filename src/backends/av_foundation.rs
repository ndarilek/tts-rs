use std::{
    collections::HashSet,
    io::Cursor,
    ptr::{self, NonNull},
    slice,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::Duration,
};

use block2::RcBlock;
use hound::{SampleFormat, WavSpec, WavWriter};
use objc2::{
    AllocAnyThread, DefinedClass, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
    sel,
};
use objc2_avf_audio::{
    AVAudioBuffer, AVAudioCommonFormat, AVAudioPCMBuffer, AVSpeechBoundary, AVSpeechSynthesisVoice,
    AVSpeechSynthesisVoiceGender, AVSpeechSynthesizer, AVSpeechSynthesizerDelegate,
    AVSpeechUtterance,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
use oxilangtag::LanguageTag;
use parking_lot::Mutex;
use tracing::{Span, info_span, instrument, trace};

use crate::{Backend, Callbacks, Error, Features, Gender, SynthesizedAudio, UtteranceId, Voice};

#[derive(Debug)]
struct Ivars {
    callbacks: Arc<Mutex<Callbacks>>,
    /// Addresses of utterances being written to audio; their delegate events belong to
    /// `synthesize`, not to speech.
    syntheses: Arc<Mutex<HashSet<usize>>>,
    // Delegate methods fire on AVFoundation's own threads; entering this span there connects
    // them back to the backend that created the synthesizer.
    span: Span,
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super(NSObject))]
    #[name = "MyAVSpeechSynthesizerDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl AVSpeechSynthesizerDelegate for Delegate {
        #[unsafe(method(speechSynthesizer:didStartSpeechUtterance:))]
        fn speech_synthesizer_did_start_speech_utterance(
            &self,
            _synthesizer: &AVSpeechSynthesizer,
            utterance: &AVSpeechUtterance,
        ) {
            let ivars = self.ivars();
            let _entered = ivars.span.enter();
            let address = ptr::from_ref(utterance) as usize;
            let utterance_id = UtteranceId::AvFoundation(address);
            trace!(?utterance_id, "Utterance started");
            if ivars.syntheses.lock().contains(&address) {
                return;
            }
            ivars.callbacks.lock().utterance_begin(utterance_id);
        }

        #[unsafe(method(speechSynthesizer:didFinishSpeechUtterance:))]
        fn speech_synthesizer_did_finish_speech_utterance(
            &self,
            _synthesizer: &AVSpeechSynthesizer,
            utterance: &AVSpeechUtterance,
        ) {
            let ivars = self.ivars();
            let _entered = ivars.span.enter();
            let address = ptr::from_ref(utterance) as usize;
            let utterance_id = UtteranceId::AvFoundation(address);
            trace!(?utterance_id, "Utterance finished");
            if ivars.syntheses.lock().remove(&address) {
                return;
            }
            ivars.callbacks.lock().utterance_end(utterance_id);
        }

        #[unsafe(method(speechSynthesizer:didCancelSpeechUtterance:))]
        fn speech_synthesizer_did_cancel_speech_utterance(
            &self,
            _synthesizer: &AVSpeechSynthesizer,
            utterance: &AVSpeechUtterance,
        ) {
            let ivars = self.ivars();
            let _entered = ivars.span.enter();
            let address = ptr::from_ref(utterance) as usize;
            let utterance_id = UtteranceId::AvFoundation(address);
            trace!(?utterance_id, "Utterance canceled");
            if ivars.syntheses.lock().remove(&address) {
                return;
            }
            ivars.callbacks.lock().utterance_stop(utterance_id);
        }

        #[unsafe(method(speechSynthesizer:didPauseSpeechUtterance:))]
        fn speech_synthesizer_did_pause_speech_utterance(
            &self,
            _synthesizer: &AVSpeechSynthesizer,
            utterance: &AVSpeechUtterance,
        ) {
            let ivars = self.ivars();
            let _entered = ivars.span.enter();
            let address = ptr::from_ref(utterance) as usize;
            let utterance_id = UtteranceId::AvFoundation(address);
            trace!(?utterance_id, "Utterance paused");
            if ivars.syntheses.lock().contains(&address) {
                return;
            }
            ivars.callbacks.lock().utterance_pause(utterance_id);
        }

        #[unsafe(method(speechSynthesizer:didContinueSpeechUtterance:))]
        fn speech_synthesizer_did_continue_speech_utterance(
            &self,
            _synthesizer: &AVSpeechSynthesizer,
            utterance: &AVSpeechUtterance,
        ) {
            let ivars = self.ivars();
            let _entered = ivars.span.enter();
            let address = ptr::from_ref(utterance) as usize;
            let utterance_id = UtteranceId::AvFoundation(address);
            trace!(?utterance_id, "Utterance resumed");
            if ivars.syntheses.lock().contains(&address) {
                return;
            }
            ivars.callbacks.lock().utterance_resume(utterance_id);
        }
    }
);

/// Requests handled by the synthesizer thread. Each carries a reply sender so callers can block
/// until the synthesizer has acted.
enum Command {
    Speak {
        text: String,
        interrupt: bool,
        rate: f32,
        volume: f32,
        pitch: f32,
        voice_id: Option<String>,
        /// Replies with the utterance's address, which identifies it in delegate callbacks.
        reply: Sender<Result<usize, Error>>,
    },
    Synthesize {
        text: String,
        rate: f32,
        volume: f32,
        pitch: f32,
        voice_id: Option<String>,
        reply: Sender<Result<SynthesizedAudio, Error>>,
    },
    Stop {
        reply: Sender<()>,
    },
    Pause {
        reply: Sender<()>,
    },
    Resume {
        reply: Sender<()>,
    },
    IsSpeaking {
        reply: Sender<bool>,
    },
    IsPaused {
        reply: Sender<bool>,
    },
}

// `Retained<AVSpeechSynthesizer>` is not `Send`, so a dedicated thread owns the synthesizer and
// its delegate for their entire lifetime; the backend only holds a channel to it.
fn run_synthesizer(callbacks: Arc<Mutex<Callbacks>>, span: Span, commands: Receiver<Command>) {
    let synth = unsafe { AVSpeechSynthesizer::new() };
    let delegate = Delegate::alloc().set_ivars(Ivars {
        callbacks,
        syntheses: Arc::new(Mutex::new(HashSet::new())),
        span,
    });
    let delegate: Retained<Delegate> = unsafe { msg_send![super(delegate), init] };
    unsafe { synth.setDelegate(Some(ProtocolObject::from_ref(&*delegate))) };

    // The iterator ends when every backend clone has dropped its sender.
    for command in commands {
        let _entered = delegate.ivars().span.enter();
        match command {
            Command::Speak {
                text,
                interrupt,
                rate,
                volume,
                pitch,
                voice_id,
                reply,
            } => {
                let _ = reply.send(speak_utterance(
                    &synth,
                    &text,
                    interrupt,
                    rate,
                    volume,
                    pitch,
                    voice_id.as_deref(),
                ));
            }
            Command::Synthesize {
                text,
                rate,
                volume,
                pitch,
                voice_id,
                reply,
            } => {
                let _ = reply.send(synthesize_utterance(
                    &synth,
                    &delegate,
                    &text,
                    rate,
                    volume,
                    pitch,
                    voice_id.as_deref(),
                ));
            }
            Command::Stop { reply } => {
                unsafe { synth.stopSpeakingAtBoundary(AVSpeechBoundary::Immediate) };
                let _ = reply.send(());
            }
            Command::Pause { reply } => {
                unsafe { synth.pauseSpeakingAtBoundary(AVSpeechBoundary::Immediate) };
                let _ = reply.send(());
            }
            Command::Resume { reply } => {
                unsafe { synth.continueSpeaking() };
                let _ = reply.send(());
            }
            Command::IsSpeaking { reply } => {
                let _ = reply.send(unsafe { synth.isSpeaking() });
            }
            Command::IsPaused { reply } => {
                let _ = reply.send(unsafe { synth.isPaused() });
            }
        }
    }
}

fn build_utterance(
    text: &str,
    rate: f32,
    volume: f32,
    pitch: f32,
    voice_id: Option<&str>,
) -> Result<Retained<AVSpeechUtterance>, Error> {
    unsafe {
        let text = NSString::from_str(text);
        let utterance = AVSpeechUtterance::initWithString(AVSpeechUtterance::alloc(), &text);
        utterance.setRate(rate);
        utterance.setVolume(volume);
        utterance.setPitchMultiplier(pitch);
        if let Some(voice_id) = voice_id {
            let ns_voice_id = NSString::from_str(voice_id);
            let voice = AVSpeechSynthesisVoice::voiceWithIdentifier(&ns_voice_id)
                .ok_or_else(|| Error::VoiceNotFound(voice_id.to_string()))?;
            utterance.setVoice(Some(&voice));
        }
        Ok(utterance)
    }
}

fn speak_utterance(
    synth: &AVSpeechSynthesizer,
    text: &str,
    interrupt: bool,
    rate: f32,
    volume: f32,
    pitch: f32,
    voice_id: Option<&str>,
) -> Result<usize, Error> {
    unsafe {
        if interrupt && synth.isSpeaking() {
            synth.stopSpeakingAtBoundary(AVSpeechBoundary::Immediate);
        }
    }
    let utterance = build_utterance(text, rate, volume, pitch, voice_id)?;
    unsafe { synth.speakUtterance(&utterance) };
    Ok(ptr::from_ref(&*utterance) as usize)
}

/// One buffer's worth of synthesized audio, in the engine's native sample type.
enum Chunk {
    F32(WavSpec, Vec<f32>),
    I16(WavSpec, Vec<i16>),
    /// The zero-length buffer that ends a write.
    Done,
    Unsupported,
}

/// Copies samples out interleaved. `data` points to one pointer per channel; each channel's
/// samples are `stride` elements apart, per the `AVAudioPCMBuffer` channel-data layout.
unsafe fn gather<T: Copy>(
    data: *mut NonNull<T>,
    channels: usize,
    frames: usize,
    stride: usize,
) -> Vec<T> {
    unsafe {
        let pointers = slice::from_raw_parts(data, channels);
        let mut samples = Vec::with_capacity(frames * channels);
        for frame in 0..frames {
            for pointer in pointers {
                samples.push(*pointer.as_ptr().add(frame * stride));
            }
        }
        samples
    }
}

// Sample rates are small positive integers reported as f64.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn extract_chunk(buffer: &AVAudioBuffer) -> Chunk {
    let Some(pcm) = buffer.downcast_ref::<AVAudioPCMBuffer>() else {
        return Chunk::Unsupported;
    };
    unsafe {
        let frames = pcm.frameLength() as usize;
        if frames == 0 {
            return Chunk::Done;
        }
        let format = pcm.format();
        let Ok(channels) = u16::try_from(format.channelCount()) else {
            return Chunk::Unsupported;
        };
        let sample_rate = format.sampleRate() as u32;
        let stride = pcm.stride();
        match format.commonFormat() {
            AVAudioCommonFormat::PCMFormatFloat32 => {
                let data = pcm.floatChannelData();
                if data.is_null() {
                    return Chunk::Unsupported;
                }
                let spec = WavSpec {
                    channels,
                    sample_rate,
                    bits_per_sample: 32,
                    sample_format: SampleFormat::Float,
                };
                Chunk::F32(spec, gather(data, channels.into(), frames, stride))
            }
            AVAudioCommonFormat::PCMFormatInt16 => {
                let data = pcm.int16ChannelData();
                if data.is_null() {
                    return Chunk::Unsupported;
                }
                let spec = WavSpec {
                    channels,
                    sample_rate,
                    bits_per_sample: 16,
                    sample_format: SampleFormat::Int,
                };
                Chunk::I16(spec, gather(data, channels.into(), frames, stride))
            }
            _ => Chunk::Unsupported,
        }
    }
}

fn to_wav(chunk: &Chunk) -> Result<SynthesizedAudio, Error> {
    let mut cursor = Cursor::new(Vec::new());
    match chunk {
        Chunk::F32(spec, samples) => {
            let mut writer = WavWriter::new(&mut cursor, *spec)?;
            for sample in samples {
                writer.write_sample(*sample)?;
            }
            writer.finalize()?;
        }
        Chunk::I16(spec, samples) => {
            let mut writer = WavWriter::new(&mut cursor, *spec)?;
            for sample in samples {
                writer.write_sample(*sample)?;
            }
            writer.finalize()?;
        }
        Chunk::Done | Chunk::Unsupported => {
            return Err(Error::OperationFailed("synthesis produced no audio"));
        }
    }
    SynthesizedAudio::from_wav(cursor.into_inner())
}

/// Bounds the wait between buffers so a stalled write can't hang the synthesizer thread.
const SYNTHESIS_CHUNK_TIMEOUT: Duration = Duration::from_secs(30);

fn assemble(chunks: &Receiver<Chunk>) -> Result<SynthesizedAudio, Error> {
    let mut audio: Option<Chunk> = None;
    loop {
        let chunk = chunks
            .recv_timeout(SYNTHESIS_CHUNK_TIMEOUT)
            .map_err(|_| Error::OperationFailed("synthesis timed out"))?;
        match (&mut audio, chunk) {
            (_, Chunk::Unsupported) => {
                return Err(Error::OperationFailed("unsupported synthesis audio format"));
            }
            (None, Chunk::Done) => {
                return Err(Error::OperationFailed("synthesis produced no audio"));
            }
            (Some(chunk), Chunk::Done) => return to_wav(chunk),
            (None, chunk) => audio = Some(chunk),
            (Some(Chunk::F32(_, samples)), Chunk::F32(_, mut more)) => samples.append(&mut more),
            (Some(Chunk::I16(_, samples)), Chunk::I16(_, mut more)) => samples.append(&mut more),
            _ => {
                return Err(Error::OperationFailed(
                    "inconsistent synthesis audio format",
                ));
            }
        }
    }
}

fn synthesize_utterance(
    synth: &AVSpeechSynthesizer,
    delegate: &Delegate,
    text: &str,
    rate: f32,
    volume: f32,
    pitch: f32,
    voice_id: Option<&str>,
) -> Result<SynthesizedAudio, Error> {
    // `writeUtterance:toBufferCallback:` is unavailable before macOS 10.15, where calling it
    // would crash with an unrecognized selector.
    if !synth.respondsToSelector(sel!(writeUtterance:toBufferCallback:)) {
        return Err(Error::UnsupportedFeature);
    }
    let ivars = delegate.ivars();
    let utterance = build_utterance(text, rate, volume, pitch, voice_id)?;
    let address = ptr::from_ref(&*utterance) as usize;
    let id = UtteranceId::AvFoundation(address);
    ivars.syntheses.lock().insert(address);
    ivars.callbacks.lock().synthesis_begin(id);
    let (chunks, assembled) = channel();
    let block = RcBlock::new(move |buffer: NonNull<AVAudioBuffer>| {
        let chunk = extract_chunk(unsafe { buffer.as_ref() });
        let _ = chunks.send(chunk);
    });
    unsafe { synth.writeUtterance_toBufferCallback(&utterance, RcBlock::as_ptr(&block)) };
    let result = assemble(&assembled);
    if result.is_err() {
        // No delegate completion event will clear the entry, so clear it here.
        ivars.syntheses.lock().remove(&address);
    }
    let audio = result?;
    ivars.callbacks.lock().synthesis_complete(id);
    Ok(audio)
}

#[derive(Clone, Debug)]
pub(crate) struct AvFoundation {
    commands: Sender<Command>,
    rate: f32,
    volume: f32,
    pitch: f32,
    voice: Option<Voice>,
}

static NEXT_BACKEND_ID: AtomicU64 = AtomicU64::new(0);

impl AvFoundation {
    #[instrument(level = "info", skip(callbacks), err)]
    pub(crate) fn new(callbacks: Arc<Mutex<Callbacks>>) -> Result<Self, Error> {
        let id = NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed);
        let span = info_span!("av_foundation", backend_id = id);
        let (commands, receiver) = channel();
        thread::Builder::new()
            .name("tts-av-foundation".into())
            .spawn(move || run_synthesizer(callbacks, span, receiver))?;

        Ok(AvFoundation {
            commands,
            rate: 0.5,
            volume: 1.,
            pitch: 1.,
            voice: None,
        })
    }

    /// Sends a command and blocks on its reply, erroring if the synthesizer thread is gone.
    fn request<T>(&self, build: impl FnOnce(Sender<T>) -> Command) -> Result<T, Error> {
        let (reply, response) = channel();
        self.commands
            .send(build(reply))
            .map_err(|_| Error::BackendUnavailable("synthesizer thread terminated"))?;
        response
            .recv()
            .map_err(|_| Error::BackendUnavailable("synthesizer thread terminated"))
    }
}

impl Backend for AvFoundation {
    #[instrument(level = "trace", skip(self))]
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            pause: true,
            rate: true,
            pitch: true,
            volume: true,
            is_speaking: true,
            voice: true,
            get_voice: true,
            utterance_callbacks: true,
            synthesis: true,
            ..Default::default()
        }
    }

    #[instrument(
        level = "debug",
        skip(self),
        fields(rate = self.rate, volume = self.volume, pitch = self.pitch),
        err
    )]
    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        let address = self.request(|reply| Command::Speak {
            text: text.to_string(),
            interrupt,
            rate: self.rate,
            volume: self.volume,
            pitch: self.pitch,
            voice_id: self.voice.as_ref().map(|v| v.id().to_string()),
            reply,
        })??;
        Ok(Some(UtteranceId::AvFoundation(address)))
    }

    #[instrument(
        level = "debug",
        skip(self),
        fields(rate = self.rate, volume = self.volume, pitch = self.pitch),
        err
    )]
    fn synthesize(&mut self, text: &str) -> Result<SynthesizedAudio, Error> {
        self.request(|reply| Command::Synthesize {
            text: text.to_string(),
            rate: self.rate,
            volume: self.volume,
            pitch: self.pitch,
            voice_id: self.voice.as_ref().map(|v| v.id().to_string()),
            reply,
        })?
    }

    #[instrument(level = "debug", skip(self), err)]
    fn stop(&mut self) -> Result<(), Error> {
        self.request(|reply| Command::Stop { reply })
    }

    #[instrument(level = "debug", skip(self), err)]
    fn pause(&mut self) -> Result<(), Error> {
        self.request(|reply| Command::Pause { reply })
    }

    #[instrument(level = "debug", skip(self), err)]
    fn resume(&mut self) -> Result<(), Error> {
        self.request(|reply| Command::Resume { reply })
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_paused(&self) -> Result<bool, Error> {
        self.request(|reply| Command::IsPaused { reply })
    }

    #[instrument(level = "trace", skip(self))]
    fn min_rate(&self) -> f32 {
        0.1
    }

    #[instrument(level = "trace", skip(self))]
    fn max_rate(&self) -> f32 {
        2.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_rate(&self) -> f32 {
        0.5
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_rate(&self) -> Result<f32, Error> {
        Ok(self.rate)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        self.rate = rate;
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn min_pitch(&self) -> f32 {
        0.5
    }

    #[instrument(level = "trace", skip(self))]
    fn max_pitch(&self) -> f32 {
        2.0
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_pitch(&self) -> f32 {
        1.0
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_pitch(&self) -> Result<f32, Error> {
        Ok(self.pitch)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        self.pitch = pitch;
        Ok(())
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
        Ok(self.volume)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        self.volume = volume;
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_speaking(&self) -> Result<bool, Error> {
        self.request(|reply| Command::IsSpeaking { reply })
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn voice(&self) -> Result<Option<Voice>, Error> {
        if let Some(voice) = &self.voice {
            return Ok(Some(voice.clone()));
        }
        Ok(unsafe { AVSpeechSynthesisVoice::voiceWithLanguage(None) }
            .as_deref()
            .map(Voice::from))
    }

    #[instrument(level = "debug", skip(self), err)]
    fn voices(&self) -> Result<Vec<Voice>, Error> {
        let voices = unsafe { AVSpeechSynthesisVoice::speechVoices() };
        Ok(voices.iter().map(|v| Voice::from(&*v)).collect())
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_voice(&mut self, voice: &Voice) -> Result<(), Error> {
        // Validate eagerly so an unknown id fails here rather than at speak time.
        unsafe { AVSpeechSynthesisVoice::voiceWithIdentifier(&NSString::from_str(voice.id())) }
            .ok_or_else(|| Error::VoiceNotFound(voice.id().to_string()))?;
        self.voice = Some(voice.clone());
        Ok(())
    }
}

impl From<&AVSpeechSynthesisVoice> for Voice {
    fn from(voice: &AVSpeechSynthesisVoice) -> Self {
        let id = unsafe { voice.identifier() };
        let name = unsafe { voice.name() };
        let gender = match unsafe { voice.gender() } {
            AVSpeechSynthesisVoiceGender::Male => Some(Gender::Male),
            AVSpeechSynthesisVoiceGender::Female => Some(Gender::Female),
            _ => None,
        };
        // Apple documents voice languages as BCP 47 tags.
        let language = unsafe { voice.language() }.to_string();
        let language = LanguageTag::parse_and_normalize(&language).unwrap();
        Voice {
            id: id.to_string(),
            name: name.to_string(),
            gender,
            language,
        }
    }
}
