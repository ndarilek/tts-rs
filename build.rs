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

    #[cfg(feature = "ffi")]
    generate_c_bindings();
}

#[cfg(feature = "ffi")]
fn generate_c_bindings() {
    use std::path::PathBuf;
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut header_path: PathBuf = std::env::var("OUT_DIR").unwrap().into();
    header_path.push("tts.h");
    cbindgen::generate(crate_dir)
        .unwrap()
        .write_to_file(header_path);
}
