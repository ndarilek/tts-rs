winrt::build!(
    dependencies
        os
    types
        windows::media::core::MediaSource
        windows::media::playback::{MediaPlaybackState, MediaPlayer}
        windows::media::speech_synthesis::SpeechSynthesizer
);

fn main() {
    build();
}
