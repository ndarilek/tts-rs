#[cfg(windows)]
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use lazy_static::lazy_static;
use log::{info, trace};
use windows::{
    Foundation::TypedEventHandler,
    Media::{
        Core::MediaSource,
        Playback::{MediaPlayer, MediaPlayerAudioCategory},
        SpeechSynthesis::SpeechSynthesizer,
    },
};

use crate::{Backend, BackendId, Error, Features, UtteranceId, CALLBACKS};

impl From<windows::core::Error> for Error {
    fn from(e: windows::core::Error) -> Self {
        Error::WinRt(e)
    }
}

#[derive(Clone)]
pub struct WinRt {
    id: BackendId,
    synth: SpeechSynthesizer,
    player: MediaPlayer,
    rate: f32,
    pitch: f32,
    volume: f32,
}

struct Utterance {
    id: UtteranceId,
    text: String,
    rate: f32,
    pitch: f32,
    volume: f32,
}

lazy_static! {
    static ref NEXT_BACKEND_ID: Mutex<u64> = Mutex::new(0);
    static ref NEXT_UTTERANCE_ID: Mutex<u64> = Mutex::new(0);
    static ref BACKEND_TO_SPEECH_SYNTHESIZER: Mutex<HashMap<BackendId, SpeechSynthesizer>> = {
        let v: HashMap<BackendId, SpeechSynthesizer> = HashMap::new();
        Mutex::new(v)
    };
    static ref BACKEND_TO_MEDIA_PLAYER: Mutex<HashMap<BackendId, MediaPlayer>> = {
        let v: HashMap<BackendId, MediaPlayer> = HashMap::new();
        Mutex::new(v)
    };
    static ref UTTERANCES: Mutex<HashMap<BackendId, VecDeque<Utterance>>> = {
        let utterances: HashMap<BackendId, VecDeque<Utterance>> = HashMap::new();
        Mutex::new(utterances)
    };
}

impl WinRt {
    pub fn new() -> std::result::Result<Self, Error> {
        info!("Initializing WinRT backend");
        let synth = SpeechSynthesizer::new()?;
        let player = MediaPlayer::new()?;
        player.SetRealTimePlayback(true)?;
        player.SetAudioCategory(MediaPlayerAudioCategory::Speech)?;
        let mut backend_id = NEXT_BACKEND_ID.lock().unwrap();
        let bid = BackendId::WinRt(*backend_id);
        *backend_id += 1;
        drop(backend_id);
        {
            let mut utterances = UTTERANCES.lock().unwrap();
            utterances.insert(bid, VecDeque::new());
        }
        let mut backend_to_media_player = BACKEND_TO_MEDIA_PLAYER.lock().unwrap();
        backend_to_media_player.insert(bid, player.clone());
        drop(backend_to_media_player);
        let mut backend_to_speech_synthesizer = BACKEND_TO_SPEECH_SYNTHESIZER.lock().unwrap();
        backend_to_speech_synthesizer.insert(bid, synth.clone());
        drop(backend_to_speech_synthesizer);
        let bid_clone = bid;
        player.MediaEnded(TypedEventHandler::new(
            move |sender: &Option<MediaPlayer>, _args| {
                if let Some(sender) = sender {
                    let backend_to_media_player = BACKEND_TO_MEDIA_PLAYER.lock().unwrap();
                    let id = backend_to_media_player.iter().find(|v| v.1 == sender);
                    if let Some((id, _)) = id {
                        let mut utterances = UTTERANCES.lock().unwrap();
                        if let Some(utterances) = utterances.get_mut(id) {
                            if let Some(utterance) = utterances.pop_front() {
                                let mut callbacks = CALLBACKS.lock().unwrap();
                                let callbacks = callbacks.get_mut(id).unwrap();
                                if let Some(callback) = callbacks.utterance_end.as_mut() {
                                    callback(utterance.id);
                                }
                                if let Some(utterance) = utterances.front() {
                                    let backend_to_speech_synthesizer =
                                        BACKEND_TO_SPEECH_SYNTHESIZER.lock().unwrap();
                                    let id = backend_to_speech_synthesizer
                                        .iter()
                                        .find(|v| *v.0 == bid_clone);
                                    if let Some((_, tts)) = id {
                                        tts.Options()?.SetSpeakingRate(utterance.rate.into())?;
                                        tts.Options()?.SetAudioPitch(utterance.pitch.into())?;
                                        tts.Options()?.SetAudioVolume(utterance.volume.into())?;
                                        let stream = tts
                                            .SynthesizeTextToStreamAsync(utterance.text.as_str())?
                                            .get()?;
                                        let content_type = stream.ContentType()?;
                                        let source =
                                            MediaSource::CreateFromStream(stream, content_type)?;
                                        sender.SetSource(source)?;
                                        sender.Play()?;
                                        if let Some(callback) = callbacks.utterance_begin.as_mut() {
                                            callback(utterance.id);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(())
            },
        ))?;
        Ok(Self {
            id: bid,
            synth,
            player,
            rate: 1.,
            pitch: 1.,
            volume: 1.,
        })
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
            voices: true,
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
        let mut no_utterances = false;
        {
            let mut utterances = UTTERANCES.lock().unwrap();
            if let Some(utterances) = utterances.get_mut(&self.id) {
                no_utterances = utterances.is_empty();
                let utterance = Utterance {
                    id: utterance_id,
                    text: text.into(),
                    rate: self.rate,
                    pitch: self.pitch,
                    volume: self.volume,
                };
                utterances.push_back(utterance);
            }
        }
        if no_utterances {
            self.synth.Options()?.SetSpeakingRate(self.rate.into())?;
            self.synth.Options()?.SetAudioPitch(self.pitch.into())?;
            self.synth.Options()?.SetAudioVolume(self.volume.into())?;
            let stream = self.synth.SynthesizeTextToStreamAsync(text)?.get()?;
            let content_type = stream.ContentType()?;
            let source = MediaSource::CreateFromStream(stream, content_type)?;
            self.player.SetSource(source)?;
            self.player.Play()?;
            let mut callbacks = CALLBACKS.lock().unwrap();
            let callbacks = callbacks.get_mut(&self.id).unwrap();
            if let Some(callback) = callbacks.utterance_begin.as_mut() {
                callback(utterance_id);
            }
        }
        Ok(Some(utterance_id))
    }

    fn stop(&mut self) -> std::result::Result<(), Error> {
        trace!("stop()");
        if !self.is_speaking()? {
            return Ok(());
        }
        let mut utterances = UTTERANCES.lock().unwrap();
        if let Some(utterances) = utterances.get(&self.id) {
            let mut callbacks = CALLBACKS.lock().unwrap();
            let callbacks = callbacks.get_mut(&self.id).unwrap();
            if let Some(callback) = callbacks.utterance_stop.as_mut() {
                for utterance in utterances {
                    callback(utterance.id);
                }
            }
        }
        if let Some(utterances) = utterances.get_mut(&self.id) {
            utterances.clear();
        }
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
        let utterances = UTTERANCES.lock().unwrap();
        let utterances = utterances.get(&self.id).unwrap();
        Ok(!utterances.is_empty())
    }

    fn voice(&self) -> Result<String,Error> {
        unimplemented!()
    }

    fn list_voices(&self) -> Vec<String> {
        unimplemented!()
    }

    fn set_voice(&mut self, voice: &str) -> Result<(),Error> {
        unimplemented!()
    }
}

impl Drop for WinRt {
    fn drop(&mut self) {
        let id = self.id;
        let mut backend_to_media_player = BACKEND_TO_MEDIA_PLAYER.lock().unwrap();
        backend_to_media_player.remove(&id);
        let mut backend_to_speech_synthesizer = BACKEND_TO_SPEECH_SYNTHESIZER.lock().unwrap();
        backend_to_speech_synthesizer.remove(&id);
        let mut utterances = UTTERANCES.lock().unwrap();
        utterances.remove(&id);
    }
}
