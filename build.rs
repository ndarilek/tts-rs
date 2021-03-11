fn main() {
    if std::env::var("TARGET").unwrap().contains("windows") {
        windows::build!(
            windows::foundation::{EventRegistrationToken, IAsyncOperation, TypedEventHandler},
            windows::media::core::MediaSource,
            windows::media::playback::{MediaPlaybackSession, MediaPlaybackState, MediaPlayer, MediaPlayerAudioCategory},
            windows::media::speech_synthesis::{SpeechSynthesisStream, SpeechSynthesizer, SpeechSynthesizerOptions},
            windows::storage::streams::IRandomAccessStream,
        );
    } else if std::env::var("TARGET").unwrap().contains("-apple") {
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        if !std::env::var("CARGO_CFG_TARGET_OS")
            .unwrap()
            .contains("ios")
        {
            println!("cargo:rustc-link-lib=framework=AppKit");
        }
    }
}
