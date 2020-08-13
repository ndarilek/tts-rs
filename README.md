# TTS-RS

This library provides a high-level Text-To-Speech (TTS) interface supporting various backends. Currently supported backends are:

* Windows
  * Screen readers/SAPI via Tolk
  * WinRT
* Linux via [Speech Dispatcher](https://freebsoft.org/speechd)
* MacOS
  * AppKit on MacOS 10.13 and below
  * AVFoundation on MacOS 10.14 and, eventually, iDevices
* WebAssembly