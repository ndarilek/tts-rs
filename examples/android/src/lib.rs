use tts::*;

// The `loop {}` below only simulates an app loop.
// Without it, the `TTS` instance gets dropped before callbacks can run.
#[allow(unreachable_code)]
fn run() -> Result<(), Error> {
    let mut tts = TTS::default()?;
    let Features {
        utterance_callbacks,
        ..
    } = tts.supported_features();
    if utterance_callbacks {
        tts.on_utterance_begin(Some(Box::new(|utterance| {
            println!("Started speaking {:?}", utterance)
        })))?;
        tts.on_utterance_end(Some(Box::new(|utterance| {
            println!("Finished speaking {:?}", utterance)
        })))?;
        tts.on_utterance_stop(Some(Box::new(|utterance| {
            println!("Stopped speaking {:?}", utterance)
        })))?;
    }
    let Features { is_speaking, .. } = tts.supported_features();
    if is_speaking {
        println!("Are we speaking? {}", tts.is_speaking()?);
    }
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
    tts.speak("Goodbye.", false)?;
    loop {}
    Ok(())
}

#[cfg_attr(target_os = "android", ndk_glue::main(backtrace = "on"))]
pub fn main() {
    run().expect("Failed to run");
}
