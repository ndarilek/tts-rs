#!/usr/bin/env bash
set -euo pipefail

command -v android >/dev/null 2>&1 ||
    curl -fsSL https://dl.google.com/android/cli/latest/linux_x86_64/install.sh | bash

android init

android_home="${ANDROID_HOME:-/usr/local/android}"
sdkmanager="$android_home/cmdline-tools/latest/bin/sdkmanager"
avdmanager="$android_home/cmdline-tools/latest/bin/avdmanager"

image="system-images;android-36;default;x86_64"
[ -d "$android_home/system-images/android-36/default/x86_64" ] ||
    (yes || true) | "$sdkmanager" "$image" >/dev/null

avd_home="${ANDROID_AVD_HOME:-$HOME/.android/avd}"
if [ ! -d "$avd_home/medium_phone.avd" ]; then
    echo "no" | "$avdmanager" create avd -n medium_phone -k "$image" --force

    # No host GPU is exposed to the container, so select software (Mesa
    # lavapipe) rendering in the AVD itself. Pairs with LIBGL_ALWAYS_SOFTWARE
    # in devcontainer.json, which keeps any host Mesa GL on llvmpipe.
    avd_config="$avd_home/medium_phone.avd/config.ini"
    sed -i '/^hw\.gpu\.mode=/d' "$avd_config"
    echo 'hw.gpu.mode=lavapipe' >> "$avd_config"
fi
