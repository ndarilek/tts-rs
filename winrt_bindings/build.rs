fn main() {
    windows::build!(
        windows::media::core::MediaSource
        windows::media::playback::{MediaPlaybackState, MediaPlayer}
        windows::media::speech_synthesis::SpeechSynthesizer
    );
}
