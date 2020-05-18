#[cfg(windows)]
use winrt::*;

import!(
    dependencies
        os
    modules
        "windows.media.core"
        "windows.media.playback"
        "windows.media.speechsynthesis"
);

use log::{info, trace};
use windows::media::core::MediaSource;
use windows::media::playback::{MediaPlaybackItem, MediaPlaybackList, MediaPlayer};
use windows::media::speech_synthesis::SpeechSynthesizer;

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
            rate: false,
            pitch: false,
            volume: false,
        }
    }

    fn speak(&self, text: &str, interrupt: bool) -> std::result::Result<(), Error> {
        trace!("speak({}, {})", text, interrupt);
        let stream = self.synth.synthesize_text_to_stream_async(text)?.get()?;
        let content_type = stream.content_type()?;
        let source = MediaSource::create_from_stream(stream, content_type)?;
        let item = MediaPlaybackItem::create(source)?;
        self.playback_list.items()?.append(item)?;
        Ok(())
    }

    fn stop(&self) -> std::result::Result<(), Error> {
        trace!("stop()");
        self.player.close()?;
        self.playback_list.items()?.clear()?;
        Ok(())
    }

    fn get_rate(&self) -> std::result::Result<u8, Error> {
        unimplemented!();
    }

    fn set_rate(&mut self, _rate: u8) -> std::result::Result<(), Error> {
        unimplemented!();
    }

    fn get_pitch(&self) -> std::result::Result<u8, Error> {
        unimplemented!();
    }

    fn set_pitch(&mut self, _pitch: u8) -> std::result::Result<(), Error> {
        unimplemented!();
    }

    fn get_volume(&self) -> std::result::Result<u8, Error> {
        unimplemented!();
    }

    fn set_volume(&mut self, _volume: u8) -> std::result::Result<(), Error> {
        unimplemented!();
    }
}
