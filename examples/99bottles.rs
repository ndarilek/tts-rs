use std::io;
use std::{thread, time};

use tts::*;

fn main() -> Result<(), Error> {
    env_logger::init();
    let mut tts = Tts::default()?;
    let mut bottles = 99;
    while bottles > 0 {
        tts.speak(format!("{} bottles of beer on the wall,", bottles), false)?;
        tts.speak(format!("{} bottles of beer,", bottles), false)?;
        tts.speak("Take one down, pass it around", false)?;
        tts.speak("Give us a bit to drink this...", false)?;
        let time = time::Duration::from_secs(15);
        thread::sleep(time);
        bottles -= 1;
        tts.speak(format!("{} bottles of beer on the wall,", bottles), false)?;
    }
    let mut _input = String::new();
    // The below is only needed to make the example run on MacOS because there is no NSRunLoop in this context.
    // It shouldn't be needed in an app or game that almost certainly has one already.
    #[cfg(target_os = "macos")]
    {
        let run_loop = unsafe { objc2_foundation::NSRunLoop::currentRunLoop() };
        unsafe { run_loop.run() };
    }
    io::stdin().read_line(&mut _input)?;
    Ok(())
}
