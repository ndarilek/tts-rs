use std::{thread, time};

use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use tts::Error;

mod common;

fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();
    let Some(tts) = common::tts_from_args()? else {
        return Ok(());
    };
    let mut phrase = 1;
    loop {
        tts.speak(&format!("Phrase {phrase}"), false)?;
        #[cfg(target_os = "macos")]
        {
            let run_loop = objc2_foundation::NSRunLoop::currentRunLoop();
            let date = objc2_foundation::NSDate::distantFuture();
            unsafe { run_loop.runMode_beforeDate(objc2_foundation::NSDefaultRunLoopMode, &date) };
        }
        let time = time::Duration::from_secs(5);
        thread::sleep(time);
        phrase += 1;
    }
}
