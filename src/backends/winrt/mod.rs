#[cfg(windows)]
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use lazy_static::lazy_static;
use log::{info, trace};

mod bindings;

use bindings::windows::{
    foundation::TypedEventHandler,
    media::{
        core::MediaSource,
        playback::{MediaPlaybackState, MediaPlayer, MediaPlayerAudioCategory},
        speech_synthesis::SpeechSynthesizer,
    },
};

use crate::{Backend, BackendId, Error, Features, UtteranceId, CALLBACKS};

impl From<windows::Error> for Error {
    fn from(e: windows::Error) -> Self {
        Error::WinRT(e)
    }
}

#[derive(Clone, Debug)]
pub struct WinRT {
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

impl WinRT {
    pub fn new() -> std::result::Result<Self, Error> {
        info!("Initializing WinRT backend");
        let synth = SpeechSynthesizer::new()?;
        let player = MediaPlayer::new()?;
        player.set_real_time_playback(true)?;
        player.set_audio_category(MediaPlayerAudioCategory::Speech)?;
        let mut backend_id = NEXT_BACKEND_ID.lock().unwrap();
        let bid = BackendId::WinRT(*backend_id);
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
        player.media_ended(TypedEventHandler::new(
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
                                        tts.options()?.set_speaking_rate(utterance.rate.into())?;
                                        tts.options()?.set_audio_pitch(utterance.pitch.into())?;
                                        tts.options()?.set_audio_volume(utterance.volume.into())?;
                                        let stream = tts
                                            .synthesize_text_to_stream_async(
                                                utterance.text.as_str(),
                                            )?
                                            .get()?;
                                        let content_type = stream.content_type()?;
                                        let source =
                                            MediaSource::create_from_stream(stream, content_type)?;
                                        sender.set_source(source)?;
                                        sender.play()?;
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

impl Backend for WinRT {
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
            let utterance_id = UtteranceId::WinRT(*uid);
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
        if no_utterances
            && self.player.playback_session()?.playback_state()? != MediaPlaybackState::Playing
        {
            self.synth.options()?.set_speaking_rate(self.rate.into())?;
            self.synth.options()?.set_audio_pitch(self.pitch.into())?;
            self.synth.options()?.set_audio_volume(self.volume.into())?;
            let stream = self.synth.synthesize_text_to_stream_async(text)?.get()?;
            let content_type = stream.content_type()?;
            let source = MediaSource::create_from_stream(stream, content_type)?;
            self.player.set_source(source)?;
            self.player.play()?;
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
        self.player.pause()?;
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
        let rate = self.synth.options()?.speaking_rate()?;
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
        let pitch = self.synth.options()?.audio_pitch()?;
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
        let volume = self.synth.options()?.audio_volume()?;
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
}

impl Drop for WinRT {
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
