use tts::*;

fn main() -> Result<(), Error> {
    env_logger::init();
    let mut tts = Tts::default()?;

    // Check to ensure we can use this feature
    if !tts.supported_features().get_voice {
        return Err(Error::UnsupportedFeature);
    }

    // First, lets see what the default voice is. We'll print the name, and
    // say something in it (to ensure we can speak with it)
    let voice_on_start = tts.voice().unwrap();
    println!(
        "Voice on start: {}",
        voice_on_start
            .as_ref()
            .map_or("None".to_owned(), |voice: &Voice| voice.name())
    );

    if let Some(voice) = voice_on_start {
        tts.speak(format!("The default voice is {}", voice.name()), false)?;
    } else {
        tts.speak("No default voice", false)?;
    }

    // Pick a different voice
    let voices = tts.voices().unwrap();
    let chosen_voice = &voices[5];

    println!("Setting voice: {}", chosen_voice.name());
    tts.set_voice(chosen_voice).unwrap();

    // Say something with the new voice to ensure that it actually changed
    tts.speak(format!("The new voice is {}", chosen_voice.name()), false)?;

    let voice_now = &tts.voice().unwrap();
    println!(
        "Voice is now: {}",
        voice_now
            .as_ref()
            .map_or("None".to_owned(), |voice: &Voice| voice.name())
    );

    assert_eq!(
        voice_now.as_ref().map(|voice: &Voice| voice.name()),
        tts.voice()?.map(|voice| voice.name())
    );

    #[cfg(target_os = "macos")]
    {
        let run_loop = objc2_foundation::NSRunLoop::currentRunLoop();
        let date = objc2_foundation::NSDate::distantFuture();
        unsafe { run_loop.runMode_beforeDate(objc2_foundation::NSDefaultRunLoopMode, &date) };
        run_loop.runUntilDate(&date);
    }

    Ok(())
}
