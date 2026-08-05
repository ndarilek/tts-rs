use std::{
    ptr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
};

use objc2::{
    AllocAnyThread, DefinedClass, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
};
use objc2_avf_audio::{
    AVSpeechBoundary, AVSpeechSynthesisVoice, AVSpeechSynthesisVoiceGender, AVSpeechSynthesizer,
    AVSpeechSynthesizerDelegate, AVSpeechUtterance,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
use oxilangtag::LanguageTag;
use parking_lot::Mutex;
use tracing::{Span, info_span, instrument, trace};

use crate::{Backend, Callbacks, Error, Features, Gender, UtteranceId, Voice};

#[derive(Debug)]
struct Ivars {
    callbacks: Arc<Mutex<Callbacks>>,
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
            let utterance_id = UtteranceId::AvFoundation(ptr::from_ref(utterance) as usize);
            trace!(?utterance_id, "Utterance started");
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
            let utterance_id = UtteranceId::AvFoundation(ptr::from_ref(utterance) as usize);
            trace!(?utterance_id, "Utterance finished");
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
            let utterance_id = UtteranceId::AvFoundation(ptr::from_ref(utterance) as usize);
            trace!(?utterance_id, "Utterance canceled");
            ivars.callbacks.lock().utterance_stop(utterance_id);
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
    Stop {
        reply: Sender<()>,
    },
    IsSpeaking {
        reply: Sender<bool>,
    },
}

// `Retained<AVSpeechSynthesizer>` is not `Send`, so a dedicated thread owns the synthesizer and
// its delegate for their entire lifetime; the backend only holds a channel to it.
fn run_synthesizer(callbacks: Arc<Mutex<Callbacks>>, span: Span, commands: Receiver<Command>) {
    let synth = unsafe { AVSpeechSynthesizer::new() };
    let delegate = Delegate::alloc().set_ivars(Ivars { callbacks, span });
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
            Command::Stop { reply } => {
                unsafe { synth.stopSpeakingAtBoundary(AVSpeechBoundary::Immediate) };
                let _ = reply.send(());
            }
            Command::IsSpeaking { reply } => {
                let _ = reply.send(unsafe { synth.isSpeaking() });
            }
        }
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
        synth.speakUtterance(&utterance);
        Ok(ptr::from_ref(&*utterance) as usize)
    }
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
            rate: true,
            pitch: true,
            volume: true,
            is_speaking: true,
            voice: true,
            get_voice: false,
            utterance_callbacks: true,
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

    #[instrument(level = "debug", skip(self), err)]
    fn stop(&mut self) -> Result<(), Error> {
        self.request(|reply| Command::Stop { reply })
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
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err)]
    fn voices(&self) -> Result<Vec<Voice>, Error> {
        let voices = unsafe { AVSpeechSynthesisVoice::speechVoices() };
        let rv = voices
            .iter()
            .map(|v| {
                let id = unsafe { v.identifier() };
                let name = unsafe { v.name() };
                let gender = unsafe { v.gender() };
                let gender = match gender {
                    AVSpeechSynthesisVoiceGender::Male => Some(Gender::Male),
                    AVSpeechSynthesisVoiceGender::Female => Some(Gender::Female),
                    _ => None,
                };
                let language = unsafe { v.language() };
                let language = language.to_string();
                let language = LanguageTag::parse(language).unwrap();
                Voice {
                    id: id.to_string(),
                    name: name.to_string(),
                    gender,
                    language,
                }
            })
            .collect();
        Ok(rv)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_voice(&mut self, voice: &Voice) -> Result<(), Error> {
        self.voice = Some(voice.clone());
        Ok(())
    }
}
