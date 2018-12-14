use tts::TTS;

fn main() {
    env_logger::init();
    let tts: TTS = Default::default();
    tts.speak("Hello, world.", false);
    let original_rate = tts.get_rate();
    tts.speak(format!("Current rate: {}", original_rate), false);
    tts.set_rate(std::u8::MAX);
    tts.speak("This is very fast.", false);
    tts.set_rate(0);
    tts.speak("This is very slow.", false);
    tts.set_rate(original_rate);
    tts.speak("Goodbye.", false);
}
