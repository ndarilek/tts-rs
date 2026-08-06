use std::env;

use tts::{Error, Tts};

/// Creates a [`Tts`] from the backend index given as the first command-line argument, or the
/// platform default when no argument is given. Returns `None`, after listing the available
/// backends, when the argument doesn't select one.
pub fn tts_from_args() -> Result<Option<Tts>, Error> {
    let Some(arg) = env::args().nth(1) else {
        return Tts::default().map(Some);
    };
    let backends = Tts::backends();
    if let Some(&backend) = arg
        .parse::<usize>()
        .ok()
        .and_then(|index| backends.get(index))
    {
        Tts::new(backend).map(Some)
    } else {
        eprintln!("No backend at index {arg}. Available backends:");
        for (index, backend) in backends.iter().enumerate() {
            eprintln!("  {index}: {backend}");
        }
        Ok(None)
    }
}
