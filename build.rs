fn main() {
    #[cfg(windows)]
    if std::env::var("TARGET").unwrap().contains("windows") {
        windows::build!(
            Windows::Foundation::{EventRegistrationToken, IAsyncOperation, TypedEventHandler},
            Windows::Media::Core::MediaSource,
            Windows::Media::Playback::{MediaPlaybackSession, MediaPlaybackState, MediaPlayer, MediaPlayerAudioCategory},
            Windows::Media::SpeechSynthesis::{SpeechSynthesisStream, SpeechSynthesizer, SpeechSynthesizerOptions},
            Windows::Storage::Streams::IRandomAccessStream,
        );
    }
    if std::env::var("TARGET").unwrap().contains("-apple") {
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        if !std::env::var("CARGO_CFG_TARGET_OS")
            .unwrap()
            .contains("ios")
        {
            println!("cargo:rustc-link-lib=framework=AppKit");
        }
    }
}
