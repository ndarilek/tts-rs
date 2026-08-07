# TTS-RS

This library provides a high-level Text-To-Speech (TTS) interface supporting various backends. Currently supported backends are:

* Windows
  * NVDA via the [NVDA Controller Client](https://github.com/nvaccess/nvda/tree/master/extras/controllerClient) (requires shipping `nvdaControllerClient.dll` with your application)
  * Screen readers/SAPI via Tolk (requires `tolk` Cargo feature)
  * WinRT
* Linux via [Speech Dispatcher](https://freebsoft.org/speechd)
* macOS/iOS/tvOS/watchOS/visionOS via AVFoundation (macOS 10.14 and above)
* Android
* WebAssembly

## Android

Plug-and-play like the other platforms — no Java sources, Gradle plugin, or manifest entries in your
app — given two things:

* **`minSdkVersion` 26 or above.** The `UtteranceProgressListener` subclass Android callbacks need
  ships as an embedded dex, loaded with `InMemoryDexClassLoader`.
* **A `Context` supplied before the first `Tts`**, one of two ways. [`ndk-context`] is consulted by
  default; [`android-activity`] publishes there for you, including by way of `winit` or a game
  engine. Or call `tts::android::set_context` with a `JavaVM` and `Context` of your choosing — it
  takes precedence over `ndk-context` and is the right entry point when the process outlives its
  `Activity` or never has one, such as speech from a foreground service: `ndk-context`'s slot is
  typically owned by the `Activity`'s glue and released when it's destroyed, while `set_context`
  holds its own reference for as long as backends need it. Either way the supplied context is traded
  for its application context, so no `Activity` is pinned.

Nothing here waits for the engine. Android reports it ready on the app's Java main thread, so a
backend that blocked for that would deadlock any app whose main thread is itself waiting on the
caller — which is every `NativeActivity` app, since [`android-activity`] holds the main thread until
the event loop acknowledges each lifecycle callback. `Tts::default()` therefore returns as soon as
the engine has been *asked* to connect, and anything spoken before it answers is queued and replayed
in order. The one exception is `synthesize`, which returns audio and so has to wait for it; call
that off your event-loop thread.

See _examples/android\_hello\_world.rs_, built with [`cargo-apk`]:

```shell
cargo apk run --example android_hello_world
adb logcat -s tts
```

Editing _android/Bridge.java_ needs a JDK and `ANDROID_HOME`; _build.rs_ rebuilds the checked-in dex
from it.

[`ndk-context`]: https://crates.io/crates/ndk-context
[`android-activity`]: https://crates.io/crates/android-activity
[`cargo-apk`]: https://crates.io/crates/cargo-apk
