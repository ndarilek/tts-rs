#[cfg(target_os = "linux")]
mod speech_dispatcher;

#[cfg(all(windows, feature = "use_tolk"))]
mod tolk;

#[cfg(windows)]
mod winrt;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_os = "macos")]
mod appkit;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod av_foundation;

/// cbindgen:ignore
#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "linux")]
pub(crate) use self::speech_dispatcher::*;

#[cfg(all(windows, feature = "use_tolk"))]
pub(crate) use self::tolk::*;

#[cfg(windows)]
pub(crate) use self::winrt::*;

#[cfg(target_arch = "wasm32")]
pub(crate) use self::web::*;

#[cfg(target_os = "macos")]
pub(crate) use self::appkit::*;

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) use self::av_foundation::*;

#[cfg(target_os = "android")]
pub(crate) use self::android::*;
