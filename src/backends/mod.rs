#[cfg(target_os = "linux")]
mod speech_dispatcher;

#[cfg(windows)]
mod tolk;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_os = "linux")]
pub use self::speech_dispatcher::*;

#[cfg(windows)]
pub use self::tolk::*;

#[cfg(target_arch = "wasm32")]
pub use self::web::*;
