use std::io;
use std::{thread, time};

#[cfg(target_os = "macos")]
use cocoa_foundation::base::id;
#[cfg(target_os = "macos")]
use cocoa_foundation::foundation::NSRunLoop;
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};

use tts::*;

fn main() -> Result<(), Error> {
    env_logger::init();
    let mut tts = TTS::default()?;
    let mut phrase = 1;
    loop {
        tts.speak(format!("Phrase {}", phrase), false);
        let time = time::Duration::from_secs(5);
        thread::sleep(time);
        phrase += 1;
    }
    let mut _input = String::new();
    // The below is only needed to make the example run on MacOS because there is no NSRunLoop in this context.
    // It shouldn't be needed in an app or game that almost certainly has one already.
    #[cfg(target_os = "macos")]
    {
        let run_loop: id = unsafe { NSRunLoop::currentRunLoop() };
        unsafe {
            let _: () = msg_send![run_loop, run];
        }
    }
    io::stdin().read_line(&mut _input)?;
    Ok(())
}
