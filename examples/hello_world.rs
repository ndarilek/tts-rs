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
    let mut tts = TTS::default()?;
    tts.speak("Hello, world.", false)?;
    let Features { rate, .. } = tts.supported_features();
    if rate {
        let original_rate = tts.get_rate()?;
        tts.speak(format!("Current rate: {}", original_rate), false)?;
        tts.set_rate(tts.max_rate())?;
        tts.speak("This is very fast.", false)?;
        tts.set_rate(tts.min_rate())?;
        tts.speak("This is very slow.", false)?;
        tts.set_rate(tts.normal_rate())?;
        tts.speak("This is the normal rate.", false)?;
        tts.set_rate(original_rate)?;
    }
    let Features { pitch, .. } = tts.supported_features();
    if pitch {
        let original_pitch = tts.get_pitch()?;
        tts.set_pitch(tts.max_pitch())?;
        tts.speak("This is high-pitch.", false)?;
        tts.set_pitch(tts.min_pitch())?;
        tts.speak("This is low pitch.", false)?;
        tts.set_pitch(tts.normal_pitch())?;
        tts.speak("This is normal pitch.", false)?;
        tts.set_pitch(original_pitch)?;
    }
    let Features { volume, .. } = tts.supported_features();
    if volume {
        let original_volume = tts.get_volume()?;
        tts.set_volume(tts.max_volume())?;
        tts.speak("This is loud!", false)?;
        tts.set_volume(tts.min_volume())?;
        tts.speak("This is quiet.", false)?;
        tts.set_volume(tts.normal_volume())?;
        tts.speak("This is normal volume.", false)?;
        tts.set_volume(original_volume)?;
    }
/*    let Features { voices, .. } = tts.supported_features();
    if voices {
        let original_voice = tts.voice()?;
        let voices_list = tts.list_voices();
        println!("Available voices:\n===");
        for v in voices_list.iter() {
            println!("{}",v);
            tts.set_voice(v)?;
            println!("voice set");
            println!("{}", tts.voice()?);
            tts.speak(v,false)?;
        }
        tts.set_voice(original_voice)?;
    }*/
    tts.speak("Goodbye.", false)?;
    let mut _input = String::new();
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
