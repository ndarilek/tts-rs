use std::{thread, time};

use tts::*;

fn main() -> Result<(), Error> {
    env_logger::init();
    let mut tts = Tts::default()?;
    let mut phrase = 1;
    loop {
        tts.speak(format!("Phrase {}", phrase), false)?;
        let time = time::Duration::from_secs(5);
        thread::sleep(time);
        phrase += 1;
    }
}
