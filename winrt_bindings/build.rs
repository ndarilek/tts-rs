fn main() {
    windows::build!(
        windows::foundation::TypedEventHandler,
        windows::media::core::MediaSource,
        windows::media::playback::{MediaPlaybackState, MediaPlayer, MediaPlayerAudioCategory},
        windows::media::speech_synthesis::SpeechSynthesizer,
    );
}
