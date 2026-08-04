use std::io;

use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use tts::{Error, Features, Tts};

fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();
    let mut tts = Tts::default()?;
    let Features { voice, .. } = tts.supported_features();
    if !voice {
        println!("This backend does not support voice selection.");
        return Ok(());
    }
    let voices = tts.voices()?;
    println!("Available voices:\n===");
    for v in &voices {
        println!("{v:?}");
    }
    let Features { get_voice, .. } = tts.supported_features();
    let original_voice = if get_voice { tts.voice()? } else { None };
    for v in &voices {
        tts.set_voice(v)?;
        tts.speak(format!("This is {}.", v.name()), false)?;
    }
    if let Some(original_voice) = original_voice {
        tts.set_voice(&original_voice)?;
    }
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
