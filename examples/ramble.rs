#[cfg(target_os = "macos")]
use cocoa_foundation::base::id;
#[cfg(target_os = "macos")]
use cocoa_foundation::foundation::NSDefaultRunLoopMode;
#[cfg(target_os = "macos")]
use cocoa_foundation::foundation::NSRunLoop;
#[cfg(target_os = "macos")]
use objc::class;
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};
use std::{thread, time};
use tts::*;

fn main() -> Result<(), Error> {
    env_logger::init();
    let mut tts = Tts::default()?;
    let mut phrase = 1;
    loop {
        tts.speak(format!("Phrase {}", phrase), false)?;
        #[cfg(target_os = "macos")]
        {
            let run_loop: id = unsafe { NSRunLoop::currentRunLoop() };
            unsafe {
                let date: id = msg_send![class!(NSDate), distantFuture];
                let _: () = msg_send![run_loop, runMode:NSDefaultRunLoopMode beforeDate:date];
            }
        }
        let time = time::Duration::from_secs(5);
        thread::sleep(time);
        phrase += 1;
    }
}
