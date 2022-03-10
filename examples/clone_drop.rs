use std::io;

#[cfg(target_os = "macos")]
use cocoa_foundation::base::id;
#[cfg(target_os = "macos")]
use cocoa_foundation::foundation::NSRunLoop;
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};

use tts::*;

fn main() -> Result<(), Error> {
    env_logger::init();
    let tts = Tts::default()?;
    let mut tts_clone = tts.clone();
    drop(tts);
    if Tts::screen_reader_available() {
        println!("A screen reader is available on this platform.");
    } else {
        println!("No screen reader is available on this platform.");
    }
    let Features {
        utterance_callbacks,
        ..
    } = tts_clone.supported_features();
    if utterance_callbacks {
        tts_clone.on_utterance_begin(Some(Box::new(|utterance| {
            println!("Started speaking {:?}", utterance)
        })))?;
        tts_clone.on_utterance_end(Some(Box::new(|utterance| {
            println!("Finished speaking {:?}", utterance)
        })))?;
        tts_clone.on_utterance_stop(Some(Box::new(|utterance| {
            println!("Stopped speaking {:?}", utterance)
        })))?;
    }
    let Features { is_speaking, .. } = tts_clone.supported_features();
    if is_speaking {
        println!("Are we speaking? {}", tts_clone.is_speaking()?);
    }
    tts_clone.speak("Hello, world.", false)?;
    let Features { rate, .. } = tts_clone.supported_features();
    if rate {
        let original_rate = tts_clone.get_rate()?;
        tts_clone.speak(format!("Current rate: {}", original_rate), false)?;
        tts_clone.set_rate(tts_clone.max_rate())?;
        tts_clone.speak("This is very fast.", false)?;
        tts_clone.set_rate(tts_clone.min_rate())?;
        tts_clone.speak("This is very slow.", false)?;
        tts_clone.set_rate(tts_clone.normal_rate())?;
        tts_clone.speak("This is the normal rate.", false)?;
        tts_clone.set_rate(original_rate)?;
    }
    let Features { pitch, .. } = tts_clone.supported_features();
    if pitch {
        let original_pitch = tts_clone.get_pitch()?;
        tts_clone.set_pitch(tts_clone.max_pitch())?;
        tts_clone.speak("This is high-pitch.", false)?;
        tts_clone.set_pitch(tts_clone.min_pitch())?;
        tts_clone.speak("This is low pitch.", false)?;
        tts_clone.set_pitch(tts_clone.normal_pitch())?;
        tts_clone.speak("This is normal pitch.", false)?;
        tts_clone.set_pitch(original_pitch)?;
    }
    let Features { volume, .. } = tts_clone.supported_features();
    if volume {
        let original_volume = tts_clone.get_volume()?;
        tts_clone.set_volume(tts_clone.max_volume())?;
        tts_clone.speak("This is loud!", false)?;
        tts_clone.set_volume(tts_clone.min_volume())?;
        tts_clone.speak("This is quiet.", false)?;
        tts_clone.set_volume(tts_clone.normal_volume())?;
        tts_clone.speak("This is normal volume.", false)?;
        tts_clone.set_volume(original_volume)?;
    }
    tts_clone.speak("Goodbye.", false)?;
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
