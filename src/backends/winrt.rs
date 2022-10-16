#[cfg(windows)]
use std::{
    collections::VecDeque,
    str::FromStr,
    sync::{Arc, Mutex},
};

use lazy_static::lazy_static;
use log::{info, trace};
use unic_langid::LanguageIdentifier;
use windows::{
    Foundation::TypedEventHandler,
    Media::{
        Core::MediaSource,
        Playback::{MediaPlayer, MediaPlayerAudioCategory},
        SpeechSynthesis::{SpeechSynthesizer, VoiceGender, VoiceInformation},
    },
};

use crate::{
    Backend, BackendId, Callbacks, Error, Features, Gender, UtteranceId, Voice, CALLBACKS,
};

impl From<windows::core::Error> for Error {
    fn from(e: windows::core::Error) -> Self {
        Error::WinRt(e)
    }
}

#[derive(Clone)]
pub struct WinRt {
    id: BackendId,
    synth: Arc<SpeechSynthesizer>,
    player: MediaPlayer,
    utterances: Arc<Mutex<VecDeque<Utterance>>>,
    rate: f32,
    pitch: f32,
    volume: f32,
    voice: VoiceInformation,
}

#[derive(Debug)]
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
    ) -> Result<(), windows::core::Error> {
        synth.Options()?.SetSpeakingRate(self.rate.into())?;
        synth.Options()?.SetAudioPitch(self.pitch.into())?;
        synth.Options()?.SetAudioVolume(self.volume.into())?;
        synth.SetVoice(&self.voice)?;

        let stream = synth
            .SynthesizeTextToStreamAsync(&self.text.clone().into())?
            .get()?;
        let content_type = stream.ContentType()?;
        let source = MediaSource::CreateFromStream(&stream, &content_type)?;

        player.SetSource(&source)?;
        player.Play()?;

        if let Some(callback) = callbacks.utterance_begin.as_mut() {
            callback(self.id);
        }

        Ok(())
    }
}

lazy_static! {
    static ref NEXT_BACKEND_ID: Mutex<u64> = Mutex::new(0);
    static ref NEXT_UTTERANCE_ID: Mutex<u64> = Mutex::new(0);
}

impl WinRt {
    pub fn new() -> std::result::Result<Self, Error> {
        info!("Initializing WinRT backend");

        let player = MediaPlayer::new()?;
        player.SetRealTimePlayback(true)?;
        player.SetAudioCategory(MediaPlayerAudioCategory::Speech)?;

        let bid = {
            let mut backend_id = NEXT_BACKEND_ID.lock().unwrap();
            let bid = BackendId::WinRt(*backend_id);
            *backend_id += 1;

            bid
        };

        let tts = Self {
            id: bid,
            synth: Arc::new(SpeechSynthesizer::new()?),
            player,
            utterances: Arc::new(Mutex::new(VecDeque::new())),
            rate: 1.,
            pitch: 1.,
            volume: 1.,
            voice: SpeechSynthesizer::DefaultVoice()?,
        };

        let synth_clone = tts.synth.clone();
        let utterances_clone = tts.utterances.clone();
        tts.player.MediaEnded(&TypedEventHandler::new(
            move |player: &Option<MediaPlayer>, _args| {
                let mut utterances = utterances_clone.lock().unwrap();

                let ended_utterance = utterances.pop_front().unwrap();

                if let Some(callback) = CALLBACKS
                    .lock()
                    .unwrap()
                    .get_mut(&bid)
                    .unwrap()
                    .utterance_end
                    .as_mut()
                {
                    callback(ended_utterance.id);
                }

                if let Some(new_utterance) = utterances.front() {
                    new_utterance.speak(
                        &synth_clone,
                        player.as_ref().unwrap(),
                        CALLBACKS.lock().unwrap().get_mut(&bid).unwrap(),
                    )?;
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

        let utterance_id = {
            let mut uid = NEXT_UTTERANCE_ID.lock().unwrap();
            let utterance_id = UtteranceId::WinRt(*uid);
            *uid += 1;
            utterance_id
        };

        let utterance = Utterance {
            id: utterance_id,
            text: text.to_string(),
            rate: self.rate,
            pitch: self.pitch,
            volume: self.volume,
            voice: self.voice.clone(),
        };

        if !self.is_speaking()? {
            utterance.speak(
                &self.synth,
                &self.player,
                CALLBACKS.lock().unwrap().get_mut(&self.id).unwrap(),
            )?;
        }

        self.utterances.lock().unwrap().push_back(utterance);
        Ok(Some(utterance_id))
    }

    fn stop(&mut self) -> std::result::Result<(), Error> {
        trace!("stop()");
        if !self.is_speaking()? {
            return Ok(());
        }
        let mut utterances = self.utterances.lock().unwrap();
        let mut callbacks = CALLBACKS.lock().unwrap();
        let callbacks = callbacks.get_mut(&self.id).unwrap();
        if let Some(callback) = callbacks.utterance_stop.as_mut() {
            let utterances = utterances.iter();
            for utterance in utterances {
                callback(utterance.id);
            }
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
        Ok(self.rate)
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
        Ok(self.pitch)
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
        Ok(self.volume)
    }

    fn set_volume(&mut self, volume: f32) -> std::result::Result<(), Error> {
        self.volume = volume;
        Ok(())
    }

    fn is_speaking(&self) -> std::result::Result<bool, Error> {
        Ok(!self.utterances.lock().unwrap().is_empty())
    }

    fn voice(&self) -> Result<Option<Voice>, Error> {
        Ok(Some((&self.voice).try_into()?))
    }

    fn voices(&self) -> Result<Vec<Voice>, Error> {
        let mut rv: Vec<Voice> = vec![];
        for voice in SpeechSynthesizer::AllVoices()? {
            rv.push((&voice).try_into()?);
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

impl TryInto<Voice> for &VoiceInformation {
    type Error = Error;

    fn try_into(self) -> Result<Voice, Self::Error> {
        let gender = self.Gender()?;
        let gender = if gender == VoiceGender::Male {
            Gender::Male
        } else {
            Gender::Female
        };
        let language: String = self.Language()?.try_into()?;
        let language = LanguageIdentifier::from_str(&language).unwrap();
        Ok(Voice {
            id: self.Id()?.try_into()?,
            name: self.DisplayName()?.try_into()?,
            gender: Some(gender),
            language,
        })
    }
}
