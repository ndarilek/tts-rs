fn main() {
    windows::build!(
        windows::foundation::{EventRegistrationToken, IAsyncOperation, TypedEventHandler},
        windows::media::core::MediaSource,
        windows::media::playback::{MediaPlaybackSession, MediaPlaybackState, MediaPlayer, MediaPlayerAudioCategory},
        windows::media::speech_synthesis::{SpeechSynthesisStream, SpeechSynthesizer, SpeechSynthesizerOptions},
        windows::storage::streams::IRandomAccessStream,
    );
}
