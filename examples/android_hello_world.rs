//! The `hello_world` example, packaged as an Android app.
//!
//! `cargo apk run --example android_hello_world`, then `adb logcat -s tts`. The activity is blank;
//! speech starts as soon as it comes up. Everything Android-specific here is about being an app —
//! nothing sets up the TTS bridge, since `Tts::default()` loads it from the crate's embedded dex.

#![cfg(target_os = "android")]

use std::time::Duration;

use android_activity::{AndroidApp, MainEvent, PollEvent};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use tts::{Error, Features, Tts, android::AudioStream};

// Signature dictated by `android-activity`, which looks this up by symbol name. It can't return a
// `Result`, which is the only reason the example proper lives in `run`.
#[unsafe(no_mangle)]
#[allow(clippy::needless_pass_by_value)]
extern "Rust" fn android_main(app: AndroidApp) {
    tracing_subscriber::fmt()
        .with_writer(paranoid_android::AndroidLogMakeWriter::new(
            "tts".to_owned(),
        ))
        // Logcat colours, timestamps and labels lines itself.
        .with_ansi(false)
        .without_time()
        .with_level(false)
        // Nothing sets `RUST_LOG` for an installed app; `tts=trace` shows the backend's spans.
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();
    if let Err(e) = run(&app) {
        error!("Failed: {e}");
    }
}

fn run(app: &AndroidApp) -> Result<(), Error> {
    let tts = Tts::default()?;
    if Tts::screen_reader_available() {
        info!("A screen reader is available on this platform.");
    } else {
        info!("No screen reader is available on this platform.");
    }
    let Features {
        utterance_callbacks,
        ..
    } = tts.supported_features();
    if utterance_callbacks {
        tts.on_utterance_begin(|utterance| {
            info!("Started speaking {utterance:?}");
        })?;
        tts.on_utterance_end(|utterance| {
            info!("Finished speaking {utterance:?}");
        })?;
        tts.on_utterance_stop(|utterance| {
            info!("Stopped speaking {utterance:?}");
        })?;
    }
    let Features { is_speaking, .. } = tts.supported_features();
    if is_speaking {
        info!("Are we speaking? {}", tts.is_speaking()?);
    }
    tts.speak(&format!("Hello, world from {}.", tts.backend_name()), false)?;
    let Features { rate, .. } = tts.supported_features();
    if rate {
        let original_rate = tts.get_rate()?;
        tts.speak(&format!("Current rate: {original_rate}"), false)?;
        tts.set_rate(tts.max_rate()?)?;
        tts.speak("This is very fast.", false)?;
        tts.set_rate(tts.min_rate()?)?;
        tts.speak("This is very slow.", false)?;
        tts.set_rate(tts.normal_rate()?)?;
        tts.speak("This is the normal rate.", false)?;
        tts.set_rate(original_rate)?;
    }
    let Features { pitch, .. } = tts.supported_features();
    if pitch {
        let original_pitch = tts.get_pitch()?;
        tts.set_pitch(tts.max_pitch()?)?;
        tts.speak("This is high-pitch.", false)?;
        tts.set_pitch(tts.min_pitch()?)?;
        tts.speak("This is low pitch.", false)?;
        tts.set_pitch(tts.normal_pitch()?)?;
        tts.speak("This is normal pitch.", false)?;
        tts.set_pitch(original_pitch)?;
    }
    let Features { volume, .. } = tts.supported_features();
    if volume {
        let original_volume = tts.get_volume()?;
        tts.set_volume(tts.max_volume()?)?;
        tts.speak("This is loud!", false)?;
        tts.set_volume(tts.min_volume()?)?;
        tts.speak("This is quiet.", false)?;
        tts.set_volume(tts.normal_volume()?)?;
        tts.speak("This is normal volume.", false)?;
        tts.set_volume(original_volume)?;
    }
    let original_stream = tts.audio_stream();
    tts.set_audio_stream(AudioStream::Accessibility)?;
    tts.speak("This is on the accessibility stream.", false)?;
    tts.set_audio_stream(original_stream)?;
    tts.speak("Goodbye.", false)?;
    let mut destroyed = false;
    while !destroyed {
        app.poll_events(Some(Duration::from_millis(500)), |event| {
            if let PollEvent::Main(MainEvent::Destroy) = event {
                destroyed = true;
            }
        });
    }
    Ok(())
}
