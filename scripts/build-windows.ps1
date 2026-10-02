$ErrorActionPreference = 'Stop'
flutter config --no-analytics --enable-windows-desktop
flutter create --project-name qianbian_ui --platforms windows,web ui
New-Item -ItemType Directory -Force -Path ui/assets/fonts | Out-Null
Invoke-WebRequest -Uri 'https://raw.githubusercontent.com/google/fonts/main/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf' -OutFile ui/assets/fonts/NotoSansSC.ttf
Push-Location ui
flutter pub get
if ($LASTEXITCODE) { throw 'Flutter依赖解析失败' }
flutter analyze --no-fatal-infos
if ($LASTEXITCODE) { throw 'Flutter静态检查失败' }
flutter build web --release --no-web-resources-cdn
if ($LASTEXITCODE) { throw 'Web构建失败' }
flutter build windows --release
if ($LASTEXITCODE) { throw 'Windows UI构建失败' }
Pop-Location
Copy-Item ui/build/web/* assets/web -Recurse -Force
cargo build --release --bin qianbian
if ($LASTEXITCODE) { throw 'Rust后台构建失败' }
New-Item -ItemType Directory -Force -Path dist/windows-payload | Out-Null
Copy-Item ui/build/windows/x64/runner/Release/* dist/windows-payload -Recurse -Force
Copy-Item target/release/qianbian.exe dist/windows-payload/qianbian-core.exe
Compress-Archive -Path dist/windows-payload/* -DestinationPath dist/windows-ui-payload.zip -Force
$env:QIANBIAN_UI_PAYLOAD = (Resolve-Path dist/windows-ui-payload.zip).Path
cargo build --release --bin qianbian-ui
if ($LASTEXITCODE) { throw '单文件启动器构建失败' }
Copy-Item target/release/qianbian-ui.exe dist/qianbian-windows-ui.exe
Get-FileHash dist/qianbian-windows-ui.exe -Algorithm SHA256
