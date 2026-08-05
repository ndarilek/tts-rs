use std::io;

use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use tts::{Error, Tts};

fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();
    let tts = Tts::default()?;
    println!("Press Enter and wait for speech.");
    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        tts.speak("Hello, world.", true)?;
    }
}
