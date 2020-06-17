#[cfg(windows)]
use log::{info, trace};

use tts_winrt_bindings::windows::media::core::MediaSource;
use tts_winrt_bindings::windows::media::playback::{
    MediaPlaybackItem, MediaPlaybackList, MediaPlaybackState, MediaPlayer,
};
use tts_winrt_bindings::windows::media::speech_synthesis::SpeechSynthesizer;

use crate::{Backend, Error, Features};

impl From<winrt::Error> for Error {
    fn from(e: winrt::Error) -> Self {
        Error::WinRT(e)
    }
}

pub struct WinRT {
    synth: SpeechSynthesizer,
    player: MediaPlayer,
    playback_list: MediaPlaybackList,
}

impl WinRT {
    pub fn new() -> std::result::Result<Self, Error> {
        info!("Initializing WinRT backend");
        let player = MediaPlayer::new()?;
        player.set_auto_play(true)?;
        let playback_list = MediaPlaybackList::new()?;
        player.set_source(&playback_list)?;
        Ok(Self {
            synth: SpeechSynthesizer::new()?,
            player: player,
            playback_list: playback_list,
        })
    }
}

impl Backend for WinRT {
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: true,
            pitch: true,
            volume: true,
            is_speaking: false,
        }
    }

    fn speak(&self, text: &str, interrupt: bool) -> std::result::Result<(), Error> {
        trace!("speak({}, {})", text, interrupt);
        if interrupt {
            self.stop()?;
        }
        let stream = self.synth.synthesize_text_to_stream_async(text)?.get()?;
        let content_type = stream.content_type()?;
        let source = MediaSource::create_from_stream(stream, content_type)?;
        let item = MediaPlaybackItem::create(source)?;
        self.playback_list.items()?.append(item)?;
        if !self.is_speaking()? {
            self.player.play()?;
        }
        Ok(())
    }

    fn stop(&self) -> std::result::Result<(), Error> {
        trace!("stop()");
        self.player.pause()?;
        self.playback_list.items()?.clear()?;
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
