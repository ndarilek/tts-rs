use std::io;

use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use tts::{Error, Features, Tts};

mod common;

fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();
    let Some(tts) = common::tts_from_args()? else {
        return Ok(());
    };
    if Tts::screen_reader_available() {
        println!("A screen reader is available on this platform.");
    } else {
        println!("No screen reader is available on this platform.");
    }
    let Features {
        utterance_callbacks,
        ..
    } = tts.supported_features();
    if utterance_callbacks {
        tts.on_utterance_begin(|utterance| {
            println!("Started speaking {utterance:?}");
        })?;
        tts.on_utterance_end(|utterance| {
            println!("Finished speaking {utterance:?}");
        })?;
        tts.on_utterance_stop(|utterance| {
            println!("Stopped speaking {utterance:?}");
        })?;
    }
    let tts_clone = tts.clone();
    drop(tts);
    let Features { is_speaking, .. } = tts_clone.supported_features();
    if is_speaking {
        println!("Are we speaking? {}", tts_clone.is_speaking()?);
    }
    tts_clone.speak(
        &format!("Hello, world from {}.", tts_clone.backend_name()),
        false,
    )?;
    let Features { rate, .. } = tts_clone.supported_features();
    if rate {
        let original_rate = tts_clone.get_rate()?;
        tts_clone.speak(&format!("Current rate: {original_rate}"), false)?;
        tts_clone.set_rate(tts_clone.max_rate()?)?;
        tts_clone.speak("This is very fast.", false)?;
        tts_clone.set_rate(tts_clone.min_rate()?)?;
        tts_clone.speak("This is very slow.", false)?;
        tts_clone.set_rate(tts_clone.normal_rate()?)?;
        tts_clone.speak("This is the normal rate.", false)?;
        tts_clone.set_rate(original_rate)?;
    }
    let Features { pitch, .. } = tts_clone.supported_features();
    if pitch {
        let original_pitch = tts_clone.get_pitch()?;
        tts_clone.set_pitch(tts_clone.max_pitch()?)?;
        tts_clone.speak("This is high-pitch.", false)?;
        tts_clone.set_pitch(tts_clone.min_pitch()?)?;
        tts_clone.speak("This is low pitch.", false)?;
        tts_clone.set_pitch(tts_clone.normal_pitch()?)?;
        tts_clone.speak("This is normal pitch.", false)?;
        tts_clone.set_pitch(original_pitch)?;
    }
    let Features { volume, .. } = tts_clone.supported_features();
    if volume {
        let original_volume = tts_clone.get_volume()?;
        tts_clone.set_volume(tts_clone.max_volume()?)?;
        tts_clone.speak("This is loud!", false)?;
        tts_clone.set_volume(tts_clone.min_volume()?)?;
        tts_clone.speak("This is quiet.", false)?;
        tts_clone.set_volume(tts_clone.normal_volume()?)?;
        tts_clone.speak("This is normal volume.", false)?;
        tts_clone.set_volume(original_volume)?;
    }
    tts_clone.speak("Goodbye.", false)?;
    let mut input = String::new();
    // The below is only needed to make the example run on MacOS because there is no NSRunLoop in this context.
    // It shouldn't be needed in an app or game that almost certainly has one already.
    #[cfg(target_os = "macos")]
    {
        let run_loop = objc2_foundation::NSRunLoop::currentRunLoop();
        run_loop.run();
    }
    io::stdin().read_line(&mut input)?;
    Ok(())
}
