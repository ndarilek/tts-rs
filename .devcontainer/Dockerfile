FROM debian:13

# Mesa's software rasteriser, which the emulator renders through. libegl-mesa0 also pulls in
# libX11, the only other system library the headless qemu binary needs. procps is for the `pkill`
# in bin/emulator-start, which clears stale AVD locks.
RUN apt-get update && \
    apt-get install -y libgl1-mesa-dri libegl-mesa0 libgbm1 procps && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*
