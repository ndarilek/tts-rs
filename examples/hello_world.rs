use std::u8;

use tts::*;

fn main() -> Result<(), std::io::Error> {
    env_logger::init();
    let mut tts = TTS::default()?;
    tts.speak("Hello, world.", false)?;
    let Features { rate, .. } = tts.supported_features();
    if rate {
        let original_rate = tts.get_rate()?;
        tts.speak(format!("Current rate: {}", original_rate), false)?;
        tts.set_rate(u8::MAX)?;
        tts.speak("This is very fast.", false)?;
        tts.set_rate(0)?;
        tts.speak("This is very slow.", false)?;
        tts.set_rate(original_rate)?;
    }
    let Features { pitch, .. } = tts.supported_features();
    if pitch {
        let original_pitch = tts.get_pitch()?;
        tts.set_pitch(u8::MAX)?;
        tts.speak("This is high-pitch.", false)?;
        tts.set_pitch(0)?;
        tts.speak("This is low pitch.", false)?;
        tts.set_pitch(original_pitch)?;
    }
    let Features { volume, .. } = tts.supported_features();
    if volume {
        let original_volume = tts.get_volume()?;
        tts.set_volume(u8::MAX)?;
        tts.speak("This is loud!", false)?;
        tts.set_volume(0)?;
        tts.speak("This is quiet.", false)?;
        tts.set_volume(original_volume)?;
    }
    tts.speak("Goodbye.", false)?;
    Ok(())
}
