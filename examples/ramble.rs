use std::{thread, time};

use tts::{Error, Tts};

fn main() -> Result<(), Error> {
    env_logger::init();
    let mut tts = Tts::default()?;
    let mut phrase = 1;
    loop {
        tts.speak(format!("Phrase {phrase}"), false)?;
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
