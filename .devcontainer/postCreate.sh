#!/usr/bin/env bash
set -euo pipefail

command -v android >/dev/null 2>&1 ||
    curl -fsSL https://dl.google.com/android/cli/latest/linux_x86_64/install.sh | bash

android init

android_home="${ANDROID_HOME:-/usr/local/android}"
sdkmanager="$android_home/cmdline-tools/latest/bin/sdkmanager"
avdmanager="$android_home/cmdline-tools/latest/bin/avdmanager"

# Packages the example into an APK without Gradle; the only extra tool CI needs.
command -v cargo-apk >/dev/null 2>&1 || cargo install cargo-apk

# `google_apis` bundles Google TTS; the AOSP `default` images ship no TTS engine at all.
image="system-images;android-36;google_apis;x86_64"

# Platforms, build-tools and the NDK come from the android-sdk feature in devcontainer.json, which
# has no option for either of these. `emulator` arrives as a dependency of the system image anyway,
# but naming it keeps `bin/emulator-start` from resting on that. `platforms;android-35` is for
# cargo-apk, which refuses to run unless some platform within the NDK's supported range (35 at
# most for NDK 28) is installed, even though the build itself compiles against android-36.
for package in emulator "$image" "platforms;android-35"; do
    [ -d "$android_home/${package//;//}" ] ||
        (yes || true) | "$sdkmanager" "$package" >/dev/null
done

avd_home="${ANDROID_AVD_HOME:-$HOME/.android/avd}"
avd_config="$avd_home/medium_phone.avd/config.ini"
# Recreated when absent or built from a different image. Spaces are stripped because avdmanager
# isn't consistent about writing `key = value` versus `key=value`.
if ! tr -d ' ' < "$avd_config" 2>/dev/null | grep -qF "image.sysdir.1=${image//;//}/"; then
    echo "no" | "$avdmanager" create avd -n medium_phone -k "$image" --force

    # No host GPU is exposed, so render in software. Pairs with LIBGL_ALWAYS_SOFTWARE
    # in devcontainer.json.
    sed -i '/^hw\.gpu\.mode=/d' "$avd_config"
    echo 'hw.gpu.mode=lavapipe' >> "$avd_config"
fi
