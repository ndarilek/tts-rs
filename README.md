# TTS-RS

This library provides a high-level Text-To-Speech (TTS) interface supporting various backends. Currently supported backends are:

* Windows
  * Screen readers/SAPI via Tolk (requires `tolk` Cargo feature)
  * WinRT
* Linux via [Speech Dispatcher](https://freebsoft.org/speechd)
* macOS/iOS/tvOS/watchOS/visionOS.
  * AppKit on macOS 10.13 and below.
  * AVFoundation on macOS 10.14 and above, and iOS/tvOS/watchOS/visionOS.
* Android
* WebAssembly

## Android Setup

On most platforms, this library is plug-and-play. Because of JNI's complexity, Android setup is a bit more involved. In general, look to the Android example for guidance. Here are some rough steps to get going:

* Set up _Cargo.toml_ as the example does. Be sure to depend on `ndk-glue`.
* Place _Bridge.java_ appropriately in your app. This is needed to support various Android TTS callbacks.
* Create a main activity similar to _MainActivity.kt_. In particular, you need to derive `android.app.NativeActivity`, and you need a `System.loadLibrary(...)` call appropriate for your app. `System.loadLibrary(...)` is needed to trigger `JNI_OnLoad`.
* * Even though you've loaded the library in your main activity, add a metadata tag to your activity in _AndroidManifest.xml_ referencing it. Yes, this is redundant but necessary.
* Set if your various build.gradle scripts to reference the plugins, dependencies, etc. from the example. In particular, you'll want to set up [cargo-ndk-android-gradle](https://github.com/willir/cargo-ndk-android-gradle/) and either [depend on androidx.annotation](https://developer.android.com/reference/androidx/annotation/package-summary) or otherwise configure your app to keep the class _rs.tts.Bridge_.

And I think that should about do it. Good luck!
