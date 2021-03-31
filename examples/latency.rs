use std::io;

use tts::*;

fn main() -> Result<(), Error> {
    env_logger::init();
    let mut tts = Tts::default()?;
    println!("Press Enter and wait for speech.");
    loop {
        let mut _input = String::new();
        io::stdin().read_line(&mut _input)?;
        tts.speak("Hello, world.", true)?;
    }
}
