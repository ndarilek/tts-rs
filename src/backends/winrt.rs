#[cfg(windows)]
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use log::{info, trace};
use oxilangtag::LanguageTag;
use windows::{
    core::Ref,
    Foundation::TypedEventHandler,
    Media::{
        Core::MediaSource,
        Playback::{MediaPlayer, MediaPlayerAudioCategory},
        SpeechSynthesis::{SpeechSynthesizer, VoiceGender, VoiceInformation},
    },
};

use crate::{Backend, BackendId, Callbacks, Error, Features, Gender, UtteranceId, Voice};

impl From<windows::core::Error> for Error {
    fn from(e: windows::core::Error) -> Self {
        Error::WinRt(e)
    }
}

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
    fn speak(
        &self,
        synth: &SpeechSynthesizer,
        player: &MediaPlayer,
        callbacks: &mut Callbacks,
    ) -> std::result::Result<(), windows::core::Error> {
        synth.Options()?.SetSpeakingRate(self.rate.into())?;
        synth.Options()?.SetAudioPitch(self.pitch.into())?;
        synth.Options()?.SetAudioVolume(self.volume.into())?;
        synth.SetVoice(&self.voice)?;
        let stream = synth
            .SynthesizeTextToStreamAsync(&self.text.as_str().into())?
            .get()?;
        let content_type = stream.ContentType()?;
        let source = MediaSource::CreateFromStream(&stream, &content_type)?;
        player.SetSource(&source)?;
        player.Play()?;
        callbacks.utterance_begin(self.id);
        Ok(())
    }
}

#[derive(Clone)]
pub struct WinRt {
    id: BackendId,
    synth: SpeechSynthesizer,
    player: MediaPlayer,
    utterances: Arc<Mutex<VecDeque<Utterance>>>,
    callbacks: Arc<Mutex<Callbacks>>,
    rate: f32,
    pitch: f32,
    volume: f32,
    voice: VoiceInformation,
}

impl WinRt {
    pub fn new(callbacks: Arc<Mutex<Callbacks>>) -> std::result::Result<Self, Error> {
        info!("Initializing WinRT backend");
        let player = MediaPlayer::new()?;
        player.SetRealTimePlayback(true)?;
        player.SetAudioCategory(MediaPlayerAudioCategory::Speech)?;
        let tts = Self {
            id: BackendId::WinRt(NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed)),
            synth: SpeechSynthesizer::new()?,
            player,
            utterances: Arc::new(Mutex::new(VecDeque::new())),
            callbacks,
            rate: 1.,
            pitch: 1.,
            volume: 1.,
            voice: SpeechSynthesizer::DefaultVoice()?,
        };
        let synth = tts.synth.clone();
        let utterances = tts.utterances.clone();
        let callbacks = tts.callbacks.clone();
        tts.player.MediaEnded(&TypedEventHandler::new(
            move |player: Ref<MediaPlayer>, _args| {
                if let Some(player) = player.as_ref() {
                    let mut utterances = utterances.lock().unwrap();
                    let mut callbacks = callbacks.lock().unwrap();
                    if let Some(utterance) = utterances.pop_front() {
                        callbacks.utterance_end(utterance.id);
                        if let Some(utterance) = utterances.front() {
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
            voice: true,
            get_voice: true,
            utterance_callbacks: true,
        }
    }

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
        let mut utterances = self.utterances.lock().unwrap();
        if utterances.is_empty() {
            let mut callbacks = self.callbacks.lock().unwrap();
            utterance.speak(&self.synth, &self.player, &mut callbacks)?;
        }
        utterances.push_back(utterance);
        Ok(Some(utterance_id))
    }

    fn stop(&mut self) -> std::result::Result<(), Error> {
        trace!("stop()");
        let mut utterances = self.utterances.lock().unwrap();
        if utterances.is_empty() {
            return Ok(());
        }
        let mut callbacks = self.callbacks.lock().unwrap();
        for utterance in utterances.iter() {
            callbacks.utterance_stop(utterance.id);
        }
        utterances.clear();
        self.player.Pause()?;
        Ok(())
    }

    fn min_rate(&self) -> f32 {
        0.5
    }

    fn max_rate(&self) -> f32 {
        6.0
    }

    fn normal_rate(&self) -> f32 {
        1.
    }

    fn get_rate(&self) -> std::result::Result<f32, Error> {
        let rate = self.synth.Options()?.SpeakingRate()?;
        Ok(rate as f32)
    }

    fn set_rate(&mut self, rate: f32) -> std::result::Result<(), Error> {
        self.rate = rate;
        Ok(())
    }

    fn min_pitch(&self) -> f32 {
        0.
    }

    fn max_pitch(&self) -> f32 {
        2.
    }

    fn normal_pitch(&self) -> f32 {
        1.
    }

    fn get_pitch(&self) -> std::result::Result<f32, Error> {
        let pitch = self.synth.Options()?.AudioPitch()?;
        Ok(pitch as f32)
    }

    fn set_pitch(&mut self, pitch: f32) -> std::result::Result<(), Error> {
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

    fn get_volume(&self) -> std::result::Result<f32, Error> {
        let volume = self.synth.Options()?.AudioVolume()?;
        Ok(volume as f32)
    }

    fn set_volume(&mut self, volume: f32) -> std::result::Result<(), Error> {
        self.volume = volume;
        Ok(())
    }

    fn is_speaking(&self) -> std::result::Result<bool, Error> {
        Ok(!self.utterances.lock().unwrap().is_empty())
    }

    fn voice(&self) -> Result<Option<Voice>, Error> {
        let voice = self.synth.Voice()?;
        let voice = voice.try_into()?;
        Ok(Some(voice))
    }

    fn voices(&self) -> Result<Vec<Voice>, Error> {
        let mut rv: Vec<Voice> = vec![];
        for voice in SpeechSynthesizer::AllVoices()? {
            rv.push(voice.try_into()?);
        }
        Ok(rv)
    }

    fn set_voice(&mut self, voice: &Voice) -> Result<(), Error> {
        for v in SpeechSynthesizer::AllVoices()? {
            let vid: String = v.Id()?.try_into()?;
            if vid == voice.id {
                self.voice = v;
                return Ok(());
            }
        }
        Err(Error::OperationFailed)
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
