#!/usr/bin/env bash
set -euo pipefail
export PATH="/opt/flutter/bin:$PATH"
flutter config --no-analytics --enable-linux-desktop --enable-web
flutter create --project-name qianbian_ui --platforms linux,windows,web ui
mkdir -p ui/assets/fonts
curl -fL 'https://raw.githubusercontent.com/google/fonts/main/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf' -o ui/assets/fonts/NotoSansSC.ttf
(cd ui && flutter pub get && flutter analyze --no-fatal-infos && flutter build web --release --no-web-resources-cdn && flutter build linux --release)
cp -a ui/build/web/. assets/web/
cargo test --all-targets
cargo build --release
python3 tests/portable_contract.py target/release/qianbian
mkdir -p dist/linux-ui/runtime/current
cp target/release/qianbian dist/qianbian-linux-x86_64
cp target/release/qianbian dist/linux-ui/qianbian
cp -a ui/build/linux/x64/release/bundle/. dist/linux-ui/runtime/current/
cp scripts/qianbian-ui.sh dist/linux-ui/qianbian-ui
chmod +x dist/linux-ui/qianbian-ui
tar -C dist/linux-ui -czf dist/qianbian-linux-ui-x86_64.tar.gz .
rustup target add x86_64-pc-windows-gnu
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
cargo build --release --target x86_64-pc-windows-gnu --bin qianbian
cp target/x86_64-pc-windows-gnu/release/qianbian.exe dist/qianbian-windows-web.exe
sha256sum dist/qianbian-* > dist/SHA256SUMS
