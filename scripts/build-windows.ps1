$ErrorActionPreference = 'Stop'
flutter config --no-analytics --enable-windows-desktop
flutter create --no-pub --project-name qianbian_ui --platforms windows,web ui
Remove-Item -LiteralPath ui/test/widget_test.dart -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path ui/assets/fonts | Out-Null
Invoke-WebRequest -Uri 'https://raw.githubusercontent.com/google/fonts/main/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf' -OutFile ui/assets/fonts/NotoSansSC.ttf
Push-Location ui
flutter pub get --enforce-lockfile
if ($LASTEXITCODE) { throw 'Flutter依赖解析失败' }
flutter analyze --no-fatal-infos
if ($LASTEXITCODE) { throw 'Flutter静态检查失败' }
flutter build web --release --no-web-resources-cdn
if ($LASTEXITCODE) { throw 'Web构建失败' }
flutter build windows --release
if ($LASTEXITCODE) { throw 'Windows UI构建失败' }
Pop-Location
Copy-Item ui/build/web/* assets/web -Recurse -Force
cargo build --locked --release --bin qianbian
if ($LASTEXITCODE) { throw 'Rust后台构建失败' }
New-Item -ItemType Directory -Force -Path dist/windows-payload | Out-Null
Copy-Item ui/build/windows/x64/runner/Release/* dist/windows-payload -Recurse -Force
Copy-Item target/release/qianbian.exe dist/windows-payload/qianbian-core.exe
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw '找不到MSVC运行库，请确认C++桌面构建工具已安装' }
$redist = Get-ChildItem -LiteralPath (Join-Path $vs 'VC/Redist/MSVC') -Directory | Sort-Object Name -Descending | Select-Object -First 1
$crt = Join-Path $redist.FullName 'x64/Microsoft.VC143.CRT'
if (-not (Test-Path -LiteralPath $crt)) { throw '找不到可随应用分发的MSVC CRT' }
Copy-Item (Join-Path $crt '*.dll') dist/windows-payload -Force
Compress-Archive -Path dist/windows-payload/* -DestinationPath dist/windows-ui-payload.zip -Force
$env:QIANBIAN_UI_PAYLOAD = (Resolve-Path dist/windows-ui-payload.zip).Path
cargo build --locked --release --bin qianbian-ui
if ($LASTEXITCODE) { throw '单文件启动器构建失败' }
Copy-Item target/release/qianbian-ui.exe dist/qianbian-windows-ui.exe
Get-FileHash dist/qianbian-windows-ui.exe -Algorithm SHA256
