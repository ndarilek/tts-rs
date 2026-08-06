use std::fs;

use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use tts::{Error, Features};

mod common;

fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();
    let Some(tts) = common::tts_from_args()? else {
        return Ok(());
    };
    let Features { synthesis, .. } = tts.supported_features();
    if !synthesis {
        println!("This backend does not support synthesis.");
        return Ok(());
    }
    tts.on_synthesis_begin(|utterance| {
        println!("Started synthesizing {utterance}");
    })?;
    tts.on_synthesis_complete(|utterance| {
        println!("Finished synthesizing {utterance}");
    })?;
    let audio = tts.synthesize(&format!("Hello, world from {}.", tts.backend_name()))?;
    println!(
        "Synthesized {} bytes of audio: {:?}",
        audio.as_bytes().len(),
        audio.spec()
    );
    fs::write("synthesized.wav", &audio)?;
    println!("Wrote synthesized.wav");
    Ok(())
}
