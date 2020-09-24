#[cfg(windows)]
use std::collections::HashMap;
use std::sync::Mutex;

use lazy_static::lazy_static;
use log::{info, trace};
use winrt::ComInterface;

use tts_winrt_bindings::windows::media::playback::{
    CurrentMediaPlaybackItemChangedEventArgs, MediaPlaybackItem, MediaPlaybackList,
    MediaPlaybackState, MediaPlayer,
};
use tts_winrt_bindings::windows::media::speech_synthesis::SpeechSynthesizer;
use tts_winrt_bindings::windows::{foundation::TypedEventHandler, media::core::MediaSource};

use crate::{Backend, BackendId, Error, Features, UtteranceId, CALLBACKS};

impl From<winrt::Error> for Error {
    fn from(e: winrt::Error) -> Self {
        Error::WinRT(e)
    }
}

pub struct WinRT {
    id: BackendId,
    synth: SpeechSynthesizer,
    player: MediaPlayer,
    playback_list: MediaPlaybackList,
}

lazy_static! {
    static ref NEXT_BACKEND_ID: Mutex<u64> = Mutex::new(0);
    static ref BACKEND_TO_MEDIA_PLAYER: Mutex<HashMap<BackendId, MediaPlayer>> = {
        let v: HashMap<BackendId, MediaPlayer> = HashMap::new();
        Mutex::new(v)
    };
    static ref BACKEND_TO_PLAYBACK_LIST: Mutex<HashMap<BackendId, MediaPlaybackList>> = {
        let v: HashMap<BackendId, MediaPlaybackList> = HashMap::new();
        Mutex::new(v)
    };
    static ref LAST_SPOKEN_UTTERANCE: Mutex<HashMap<BackendId, UtteranceId>> = {
        let v: HashMap<BackendId, UtteranceId> = HashMap::new();
        Mutex::new(v)
    };
}

impl WinRT {
    pub fn new() -> std::result::Result<Self, Error> {
        info!("Initializing WinRT backend");
        let playback_list = MediaPlaybackList::new()?;
        let player = MediaPlayer::new()?;
        player.set_auto_play(true)?;
        player.set_source(&playback_list)?;
        let mut backend_id = NEXT_BACKEND_ID.lock().unwrap();
        let bid = BackendId::WinRT(*backend_id);
        let mut rv = Self {
            id: bid,
            synth: SpeechSynthesizer::new()?,
            player: player,
            playback_list: playback_list,
        };
        *backend_id += 1;
        Self::init_callbacks(&mut rv)?;
        Ok(rv)
    }

    fn reinit_player(&mut self) -> std::result::Result<(), Error> {
        self.playback_list = MediaPlaybackList::new()?;
        self.player = MediaPlayer::new()?;
        self.player.set_auto_play(true)?;
        self.player.set_source(&self.playback_list)?;
        self.init_callbacks()?;
        Ok(())
    }

    fn init_callbacks(&mut self) -> Result<(), winrt::Error> {
        let id = self.id().unwrap();
        let mut backend_to_media_player = BACKEND_TO_MEDIA_PLAYER.lock().unwrap();
        backend_to_media_player.insert(id, self.player.clone());
        self.player
            .media_ended(TypedEventHandler::new(|sender, _args| {
                let backend_to_media_player = BACKEND_TO_MEDIA_PLAYER.lock().unwrap();
                let id = backend_to_media_player.iter().find(|v| v.1 == sender);
                if let Some(id) = id {
                    let id = id.0;
                    let callbacks = CALLBACKS.lock().unwrap();
                    let callbacks = callbacks.get(&id).unwrap();
                    if let Some(callback) = callbacks.utterance_end {
                        let last_spoken_utterance = LAST_SPOKEN_UTTERANCE.lock().unwrap();
                        if let Some(utterance_id) = last_spoken_utterance.get(&id) {
                            callback(utterance_id.clone());
                        }
                    }
                }
                Ok(())
            }))?;
        let mut backend_to_playback_list = BACKEND_TO_PLAYBACK_LIST.lock().unwrap();
        backend_to_playback_list.insert(id, self.playback_list.clone());
        self.playback_list
            .current_item_changed(TypedEventHandler::new(
                |sender: &MediaPlaybackList, args: &CurrentMediaPlaybackItemChangedEventArgs| {
                    let backend_to_playback_list = BACKEND_TO_PLAYBACK_LIST.lock().unwrap();
                    let id = backend_to_playback_list.iter().find(|v| v.1 == sender);
                    if let Some(id) = id {
                        let id = id.0;
                        let callbacks = CALLBACKS.lock().unwrap();
                        let callbacks = callbacks.get(&id).unwrap();
                        let old_item = args.old_item()?;
                        if !old_item.is_null() {
                            if let Some(callback) = callbacks.utterance_end {
                                callback(UtteranceId::WinRT(old_item));
                            }
                        }
                        let new_item = args.new_item()?;
                        if !new_item.is_null() {
                            let utterance_id = UtteranceId::WinRT(new_item);
                            let mut last_spoken_utterance = LAST_SPOKEN_UTTERANCE.lock().unwrap();
                            last_spoken_utterance.insert(*id, utterance_id.clone());
                            if let Some(callback) = callbacks.utterance_begin {
                                callback(utterance_id);
                            }
                        }
                    }
                    Ok(())
                },
            ))?;
        Ok(())
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
        trace!("speak({}, {})", text, interrupt);
        if interrupt {
            self.stop()?;
        }
        let stream = self.synth.synthesize_text_to_stream_async(text)?.get()?;
        let content_type = stream.content_type()?;
        let source = MediaSource::create_from_stream(stream, content_type)?;
        let item = MediaPlaybackItem::create(source)?;
        let state = self.player.playback_session()?.playback_state()?;
        if state == MediaPlaybackState::Paused {
            let index = self.playback_list.current_item_index()?;
            let total = self.playback_list.items()?.size()?;
            if total != 0 && index == total - 1 {
                self.reinit_player()?;
            }
        }
        self.playback_list.items()?.append(&item)?;
        if !self.is_speaking()? {
            self.player.play()?;
        }
        let utterance_id = UtteranceId::WinRT(item);
        Ok(Some(utterance_id))
    }

    fn stop(&mut self) -> std::result::Result<(), Error> {
        trace!("stop()");
        self.reinit_player()?;
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
        self.synth.options()?.set_speaking_rate(rate.into())?;
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
        self.synth.options()?.set_audio_pitch(pitch.into())?;
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
        self.synth.options()?.set_audio_volume(volume.into())?;
        Ok(())
    }

    fn is_speaking(&self) -> std::result::Result<bool, Error> {
        let state = self.player.playback_session()?.playback_state()?;
        let playing = state == MediaPlaybackState::Opening || state == MediaPlaybackState::Playing;
        Ok(playing)
    }
}

impl Drop for WinRT {
    fn drop(&mut self) {
        let id = self.id().unwrap();
        let mut backend_to_playback_list = BACKEND_TO_PLAYBACK_LIST.lock().unwrap();
        backend_to_playback_list.remove(&id);
        let mut backend_to_media_player = BACKEND_TO_MEDIA_PLAYER.lock().unwrap();
        backend_to_media_player.remove(&id);
        let mut last_spoken_utterance = LAST_SPOKEN_UTTERANCE.lock().unwrap();
        last_spoken_utterance.remove(&id);
    }
}
