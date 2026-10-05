# Builds Gezik on Linux and tries the clipboard and drag and drop under Xvfb (X11) and a
# headless sway (Wayland). See scripts/linux/test.sh.
FROM rust:1-trixie
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        pkg-config libfontconfig-dev libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev \
        libxcb1 libxcursor1 libxi6 libxrandr2 libx11-6 libx11-xcb1 \
        xvfb xclip xdotool x11-utils \
        sway wl-clipboard wtype foot \
        fonts-dejavu-core dbus python3 imagemagick \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add clippy
WORKDIR /src
