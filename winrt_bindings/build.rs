winrt::build!(
    dependencies
        os
    types
        windows::media::core::MediaSource
        windows::media::playback::{MediaPlaybackItem, MediaPlaybackList, MediaPlaybackState, MediaPlayer}
        windows::media::speech_synthesis::SpeechSynthesizer
);

fn main() {
    build();
}
