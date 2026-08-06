#[cfg(windows)]
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use oxilangtag::LanguageTag;
use parking_lot::Mutex;
use tracing::{info_span, instrument, trace};
use windows::{
    Foundation::TypedEventHandler,
    Media::{
        Core::MediaSource,
        Playback::{MediaPlayer, MediaPlayerAudioCategory},
        SpeechSynthesis::{
            SpeechSynthesisStream, SpeechSynthesizer, VoiceGender, VoiceInformation,
        },
    },
    Storage::Streams::DataReader,
    core::Ref,
};

use crate::{Backend, Callbacks, Error, Features, Gender, SynthesizedAudio, UtteranceId, Voice};

static NEXT_BACKEND_ID: AtomicU64 = AtomicU64::new(0);
static NEXT_UTTERANCE_ID: AtomicU64 = AtomicU64::new(0);

struct Utterance {
    id: UtteranceId,
    text: String,
    rate: f32,
    pitch: f32,
    volume: f32,
    voice: VoiceInformation,
}

impl Utterance {
    #[instrument(
        level = "debug",
        skip_all,
        fields(utterance_id = ?self.id, text = %self.text),
        err
    )]
    fn synthesize(
        &self,
        synth: &SpeechSynthesizer,
    ) -> std::result::Result<SpeechSynthesisStream, windows::core::Error> {
        let options = synth.Options()?;
        options.SetSpeakingRate(self.rate.into())?;
        options.SetAudioPitch(self.pitch.into())?;
        options.SetAudioVolume(self.volume.into())?;
        synth.SetVoice(&self.voice)?;
        synth
            .SynthesizeTextToStreamAsync(&self.text.as_str().into())?
            .join()
    }

    #[instrument(
        level = "debug",
        skip_all,
        fields(utterance_id = ?self.id, text = %self.text),
        err
    )]
    fn speak(
        &self,
        synth: &SpeechSynthesizer,
        player: &MediaPlayer,
        callbacks: &mut Callbacks,
    ) -> std::result::Result<(), windows::core::Error> {
        let stream = self.synthesize(synth)?;
        let content_type = stream.ContentType()?;
        let source = MediaSource::CreateFromStream(&stream, &content_type)?;
        player.SetSource(&source)?;
        player.Play()?;
        callbacks.utterance_begin(self.id);
        Ok(())
    }
}

/// Playback state shared with the `MediaEnded` handler.
#[derive(Default)]
struct State {
    utterances: VecDeque<Utterance>,
    paused: bool,
}

#[derive(Clone)]
pub struct WinRt {
    synth: SpeechSynthesizer,
    player: MediaPlayer,
    state: Arc<Mutex<State>>,
    callbacks: Arc<Mutex<Callbacks>>,
    rate: f32,
    pitch: f32,
    volume: f32,
    voice: VoiceInformation,
}

impl WinRt {
    #[instrument(level = "info", skip(callbacks), err)]
    pub fn new(callbacks: Arc<Mutex<Callbacks>>) -> std::result::Result<Self, Error> {
        let player = MediaPlayer::new()?;
        player.SetRealTimePlayback(true)?;
        player.SetAudioCategory(MediaPlayerAudioCategory::Speech)?;
        let id = NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed);
        let tts = Self {
            synth: SpeechSynthesizer::new()?,
            player,
            state: Arc::new(Mutex::new(State::default())),
            callbacks,
            rate: 1.,
            pitch: 1.,
            volume: 1.,
            voice: SpeechSynthesizer::DefaultVoice()?,
        };
        // Media events arrive on a system thread; entering this span there connects them back
        // to the backend that registered the handler.
        let span = info_span!("winrt", backend_id = id);
        let synth = tts.synth.clone();
        let state = tts.state.clone();
        let callbacks = tts.callbacks.clone();
        tts.player.MediaEnded(&TypedEventHandler::new(
            move |player: Ref<MediaPlayer>, _args| {
                let _entered = span.enter();
                trace!("Media ended");
                if let Some(player) = player.as_ref() {
                    let mut state = state.lock();
                    let mut callbacks = callbacks.lock();
                    if let Some(utterance) = state.utterances.pop_front() {
                        callbacks.utterance_end(utterance.id);
                        if let Some(utterance) = state.utterances.front() {
                            utterance.speak(&synth, player, &mut callbacks)?;
                        }
                    }
                }
                Ok(())
            },
        ))?;
        Ok(tts)
    }
}

impl Backend for WinRt {
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
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn speak(
        &mut self,
        text: &str,
        interrupt: bool,
    ) -> std::result::Result<Option<UtteranceId>, Error> {
        if interrupt && self.is_speaking()? {
            self.stop()?;
        }
        let utterance = Utterance {
            id: UtteranceId::WinRt(NEXT_UTTERANCE_ID.fetch_add(1, Ordering::Relaxed)),
            text: text.into(),
            rate: self.rate,
            pitch: self.pitch,
            volume: self.volume,
            voice: self.voice.clone(),
        };
        let utterance_id = utterance.id;
        let mut state = self.state.lock();
        // `paused` implies a non-empty queue.
        if state.utterances.is_empty() {
            let mut callbacks = self.callbacks.lock();
            utterance.speak(&self.synth, &self.player, &mut callbacks)?;
        }
        state.utterances.push_back(utterance);
        Ok(Some(utterance_id))
    }

    #[instrument(level = "debug", skip(self), err)]
    fn synthesize(&mut self, text: &str) -> std::result::Result<SynthesizedAudio, Error> {
        let utterance = Utterance {
            id: UtteranceId::WinRt(NEXT_UTTERANCE_ID.fetch_add(1, Ordering::Relaxed)),
            text: text.into(),
            rate: self.rate,
            pitch: self.pitch,
            volume: self.volume,
            voice: self.voice.clone(),
        };
        self.callbacks.lock().synthesis_begin(utterance.id);
        let stream = utterance.synthesize(&self.synth)?;
        let size = u32::try_from(stream.Size()?)
            .map_err(|_| Error::OperationFailed("synthesized audio size"))?;
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
        reader.LoadAsync(size)?.join()?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes)?;
        let audio = SynthesizedAudio::from_wav(bytes)?;
        self.callbacks.lock().synthesis_complete(utterance.id);
        Ok(audio)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn stop(&mut self) -> std::result::Result<(), Error> {
        let mut state = self.state.lock();
        state.paused = false;
        if state.utterances.is_empty() {
            return Ok(());
        }
        let mut callbacks = self.callbacks.lock();
        for utterance in &state.utterances {
            callbacks.utterance_stop(utterance.id);
        }
        state.utterances.clear();
        self.player.Pause()?;
        Ok(())
    }

    #[instrument(level = "debug", skip(self), err)]
    fn pause(&mut self) -> std::result::Result<(), Error> {
        let mut state = self.state.lock();
        if state.paused {
            return Ok(());
        }
        if let Some(id) = state.utterances.front().map(|u| u.id) {
            self.player.Pause()?;
            state.paused = true;
            self.callbacks.lock().utterance_pause(id);
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self), err)]
    fn resume(&mut self) -> std::result::Result<(), Error> {
        let mut state = self.state.lock();
        if !state.paused {
            return Ok(());
        }
        state.paused = false;
        if let Some(id) = state.utterances.front().map(|u| u.id) {
            self.player.Play()?;
            self.callbacks.lock().utterance_resume(id);
        }
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_paused(&self) -> std::result::Result<bool, Error> {
        Ok(self.state.lock().paused)
    }

    #[instrument(level = "trace", skip(self))]
    fn min_rate(&self) -> f32 {
        0.5
    }

    #[instrument(level = "trace", skip(self))]
    fn max_rate(&self) -> f32 {
        6.0
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_rate(&self) -> f32 {
        1.
    }

    // WinRT reports f64, but this crate's API is f32.
    #[allow(clippy::cast_possible_truncation)]
    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_rate(&self) -> std::result::Result<f32, Error> {
        let rate = self.synth.Options()?.SpeakingRate()?;
        Ok(rate as f32)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_rate(&mut self, rate: f32) -> std::result::Result<(), Error> {
        self.rate = rate;
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn min_pitch(&self) -> f32 {
        0.
    }

    #[instrument(level = "trace", skip(self))]
    fn max_pitch(&self) -> f32 {
        2.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_pitch(&self) -> f32 {
        1.
    }

    // WinRT reports f64, but this crate's API is f32.
    #[allow(clippy::cast_possible_truncation)]
    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_pitch(&self) -> std::result::Result<f32, Error> {
        let pitch = self.synth.Options()?.AudioPitch()?;
        Ok(pitch as f32)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_pitch(&mut self, pitch: f32) -> std::result::Result<(), Error> {
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

    // WinRT reports f64, but this crate's API is f32.
    #[allow(clippy::cast_possible_truncation)]
    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_volume(&self) -> std::result::Result<f32, Error> {
        let volume = self.synth.Options()?.AudioVolume()?;
        Ok(volume as f32)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_volume(&mut self, volume: f32) -> std::result::Result<(), Error> {
        self.volume = volume;
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_speaking(&self) -> std::result::Result<bool, Error> {
        Ok(!self.state.lock().utterances.is_empty())
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn voice(&self) -> Result<Option<Voice>, Error> {
        let voice = self.synth.Voice()?;
        let voice = voice.try_into()?;
        Ok(Some(voice))
    }

    #[instrument(level = "debug", skip(self), err)]
    fn voices(&self) -> Result<Vec<Voice>, Error> {
        let mut rv: Vec<Voice> = vec![];
        for voice in SpeechSynthesizer::AllVoices()? {
            rv.push(voice.try_into()?);
        }
        Ok(rv)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_voice(&mut self, voice: &Voice) -> Result<(), Error> {
        for v in SpeechSynthesizer::AllVoices()? {
            let vid: String = v.Id()?.try_into()?;
            if vid == voice.id {
                self.voice = v;
                return Ok(());
            }
        }
        Err(Error::VoiceNotFound(voice.id.clone()))
    }
}

impl TryInto<Voice> for VoiceInformation {
    type Error = Error;

    fn try_into(self) -> Result<Voice, Self::Error> {
        let gender = self.Gender()?;
        let gender = if gender == VoiceGender::Male {
            Gender::Male
        } else {
            Gender::Female
        };
        let language: String = self.Language()?.try_into()?;
        let language = LanguageTag::parse(language).unwrap();
        Ok(Voice {
            id: self.Id()?.try_into()?,
            name: self.DisplayName()?.try_into()?,
            gender: Some(gender),
            language,
        })
    }
}
