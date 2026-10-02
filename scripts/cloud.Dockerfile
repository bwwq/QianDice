FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
ENV TAR_OPTIONS=--no-same-owner
# Install before CNB mounts the host timezone into the build container.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl git unzip zip xz-utils clang cmake ninja-build pkg-config libgtk-3-dev liblzma-dev libstdc++-12-dev gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 python3 && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.90.0
ENV PATH="/root/.cargo/bin:/opt/flutter/bin:${PATH}"
RUN git clone --depth 1 --branch 3.47.6 https://github.com/flutter/flutter.git /opt/flutter && flutter config --no-analytics --enable-linux-desktop --enable-web && flutter precache --linux --web
RUN rustup target add x86_64-pc-windows-gnu
RUN curl -fL https://nodejs.org/dist/v22.15.0/node-v22.15.0-linux-x64.tar.xz | tar -xJ --no-same-owner --strip-components=1 -C /usr/local
RUN npm install --prefix /opt/qianbian-visual playwright@1.51.1 && /opt/qianbian-visual/node_modules/.bin/playwright install --with-deps chromium
