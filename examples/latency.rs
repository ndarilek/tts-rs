use std::io;

use tts::{Error, Tts};

fn main() -> Result<(), Error> {
    env_logger::init();
    let mut tts = Tts::default()?;
    println!("Press Enter and wait for speech.");
    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        tts.speak("Hello, world.", true)?;
    }
}
