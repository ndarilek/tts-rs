use std::{io, thread, time};

use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use tts::{Error, Tts};

fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();
    let tts = Tts::default()?;
    let mut bottles = 99;
    while bottles > 0 {
        tts.speak(&format!("{bottles} bottles of beer on the wall,"), false)?;
        tts.speak(&format!("{bottles} bottles of beer,"), false)?;
        tts.speak("Take one down, pass it around", false)?;
        tts.speak("Give us a bit to drink this...", false)?;
        let time = time::Duration::from_secs(15);
        thread::sleep(time);
        bottles -= 1;
        tts.speak(&format!("{bottles} bottles of beer on the wall,"), false)?;
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
