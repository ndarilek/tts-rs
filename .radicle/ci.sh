#!/usr/bin/env bash
set -xeuo pipefail

rustup target add \
    aarch64-apple-darwin \
    aarch64-linux-android \
    wasm32-unknown-unknown \
    x86_64-pc-windows-msvc

cargo fmt --all --check
(cd examples/web && cargo fmt --all --check)

cargo clippy --all-targets
cargo clippy --all-targets --target x86_64-pc-windows-msvc
cargo clippy --all-targets --target aarch64-apple-darwin
cargo clippy --all-targets --target wasm32-unknown-unknown
cargo clippy --all-targets --target aarch64-linux-android

(cd examples/web && cargo build --target wasm32-unknown-unknown)

if command -v cargo-apk >/dev/null 2>&1 && [ -n "${ANDROID_NDK_ROOT:-}" ]; then
    cargo apk build --example android_hello_world
else
    echo "Skipping Android example: cargo-apk or ANDROID_NDK_ROOT is unavailable"
fi
