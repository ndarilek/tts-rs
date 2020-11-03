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

#[derive(Clone, Debug)]
pub struct WinRT {
    id: BackendId,
    synth: SpeechSynthesizer,
    player: MediaPlayer,
    playback_list: MediaPlaybackList,
}

lazy_static! {
    static ref NEXT_BACKEND_ID: Mutex<u64> = Mutex::new(0);
    static ref NEXT_UTTERANCE_ID: Mutex<u64> = Mutex::new(0);
    static ref UTTERANCE_MAPPINGS: Mutex<Vec<(BackendId, MediaPlaybackItem, UtteranceId)>> =
        Mutex::new(Vec::new());
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
        *backend_id += 1;
        let mut backend_to_media_player = BACKEND_TO_MEDIA_PLAYER.lock().unwrap();
        backend_to_media_player.insert(bid, player.clone());
        player.media_ended(TypedEventHandler::new(|sender, _args| {
            let backend_to_media_player = BACKEND_TO_MEDIA_PLAYER.lock().unwrap();
            let id = backend_to_media_player.iter().find(|v| v.1 == sender);
            if let Some(id) = id {
                let id = id.0;
                let mut callbacks = CALLBACKS.lock().unwrap();
                let callbacks = callbacks.get_mut(&id).unwrap();
                if let Some(callback) = callbacks.utterance_end.as_mut() {
                    let last_spoken_utterance = LAST_SPOKEN_UTTERANCE.lock().unwrap();
                    if let Some(utterance_id) = last_spoken_utterance.get(&id) {
                        callback(utterance_id.clone());
                    }
                }
            }
            Ok(())
        }))?;
        let mut backend_to_playback_list = BACKEND_TO_PLAYBACK_LIST.lock().unwrap();
        backend_to_playback_list.insert(bid, playback_list.clone());
        playback_list.current_item_changed(TypedEventHandler::new(
            |sender: &MediaPlaybackList, args: &CurrentMediaPlaybackItemChangedEventArgs| {
                let backend_to_playback_list = BACKEND_TO_PLAYBACK_LIST.lock().unwrap();
                let id = backend_to_playback_list.iter().find(|v| v.1 == sender);
                if let Some(id) = id {
                    let id = id.0;
                    let mut callbacks = CALLBACKS.lock().unwrap();
                    let callbacks = callbacks.get_mut(&id).unwrap();
                    let old_item = args.old_item()?;
                    if !old_item.is_null() {
                        let mut mappings = UTTERANCE_MAPPINGS.lock().unwrap();
                        if let Some(callback) = callbacks.utterance_end.as_mut() {
                            for mapping in &*mappings {
                                if mapping.1 == old_item {
                                    callback(mapping.2);
                                }
                            }
                            mappings.retain(|v| v.1 != old_item);
                        }
                    }
                    let new_item = args.new_item()?;
                    if !new_item.is_null() {
                        let mut last_spoken_utterance = LAST_SPOKEN_UTTERANCE.lock().unwrap();
                        let mappings = UTTERANCE_MAPPINGS.lock().unwrap();
                        for mapping in &*mappings {
                            if mapping.1 == new_item {
                                last_spoken_utterance.insert(*id, mapping.2);
                            }
                        }
                        if let Some(callback) = callbacks.utterance_begin.as_mut() {
                            for mapping in &*mappings {
                                if mapping.1 == new_item {
                                    callback(mapping.2);
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
            synth: SpeechSynthesizer::new()?,
            player,
            playback_list,
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
        if interrupt {
            self.stop()?;
        }
        let stream = self.synth.synthesize_text_to_stream_async(text)?.get()?;
        let content_type = stream.content_type()?;
        let source = MediaSource::create_from_stream(stream, content_type)?;
        let item = MediaPlaybackItem::create(source)?;
        self.playback_list.items()?.append(&item)?;
        if !self.is_speaking()? {
            self.player.play()?;
        }
        let mut uid = NEXT_UTTERANCE_ID.lock().unwrap();
        let utterance_id = UtteranceId::WinRT(*uid);
        *uid += 1;
        drop(uid);
        let mut mappings = UTTERANCE_MAPPINGS.lock().unwrap();
        mappings.push((self.id, item, utterance_id));
        Ok(Some(utterance_id))
    }

    fn stop(&mut self) -> std::result::Result<(), Error> {
        trace!("stop()");
        self.playback_list.items()?.clear()?;
        let mut mappings = UTTERANCE_MAPPINGS.lock().unwrap();
        let mut callbacks = CALLBACKS.lock().unwrap();
        let callbacks = callbacks.get_mut(&self.id).unwrap();
        if let Some(callback) = callbacks.utterance_stop.as_mut() {
            for mapping in &*mappings {
                callback(mapping.2);
            }
        }
        mappings.retain(|v| v.0 != self.id);
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
        let playing = state == MediaPlaybackState::Playing;
        Ok(playing)
    }
}

impl Drop for WinRT {
    fn drop(&mut self) {
        let id = self.id;
        let mut backend_to_playback_list = BACKEND_TO_PLAYBACK_LIST.lock().unwrap();
        backend_to_playback_list.remove(&id);
        let mut backend_to_media_player = BACKEND_TO_MEDIA_PLAYER.lock().unwrap();
        backend_to_media_player.remove(&id);
        let mut last_spoken_utterance = LAST_SPOKEN_UTTERANCE.lock().unwrap();
        last_spoken_utterance.remove(&id);
        let mut mappings = UTTERANCE_MAPPINGS.lock().unwrap();
        mappings.retain(|v| v.0 != id);
    }
}
