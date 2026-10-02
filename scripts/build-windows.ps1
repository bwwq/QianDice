$ErrorActionPreference = 'Stop'
flutter config --no-analytics --enable-windows-desktop
$lockedDependencies = [System.IO.File]::ReadAllBytes((Join-Path $PWD 'ui/pubspec.lock'))
flutter create --no-pub --project-name qianbian_ui --platforms windows ui
[System.IO.File]::WriteAllBytes((Join-Path $PWD 'ui/pubspec.lock'), $lockedDependencies)
Remove-Item -LiteralPath ui/test/widget_test.dart -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path ui/assets/fonts | Out-Null
Invoke-WebRequest -Uri 'https://raw.githubusercontent.com/google/fonts/main/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf' -OutFile ui/assets/fonts/NotoSansSC.ttf
Invoke-WebRequest -Uri 'https://raw.githubusercontent.com/google/fonts/main/ofl/notosanssc/OFL.txt' -OutFile ui/assets/fonts/OFL.txt
Push-Location ui
flutter pub get --enforce-lockfile
if ($LASTEXITCODE) { throw 'Flutter依赖解析失败' }
flutter analyze --no-fatal-infos
if ($LASTEXITCODE) { throw 'Flutter静态检查失败' }
flutter build windows --release
if ($LASTEXITCODE) { throw 'Windows UI构建失败' }
Pop-Location
Push-Location web
npm ci --ignore-scripts
if ($LASTEXITCODE) { throw 'React依赖安装失败' }
npm run build
if ($LASTEXITCODE) { throw 'React Web构建失败' }
Pop-Location
Copy-Item web/dist/* assets/web -Recurse -Force
cargo build --locked --release --bin qianbian
if ($LASTEXITCODE) { throw 'Rust后台构建失败' }
New-Item -ItemType Directory -Force -Path dist/windows-payload | Out-Null
Copy-Item ui/build/windows/x64/runner/Release/* dist/windows-payload -Recurse -Force
Copy-Item target/release/qianbian.exe dist/windows-payload/qianbian-core.exe
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw '找不到MSVC运行库，请确认C++桌面构建工具已安装' }
$redistRoot = Join-Path $vs 'VC/Redist/MSVC'
$crtDirectory = Get-ChildItem -LiteralPath $redistRoot -Directory -Recurse |
    Where-Object { $_.Name -match '^Microsoft\.VC\d+\.CRT$' -and $_.Parent.Name -eq 'x64' } |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $crtDirectory) { throw "找不到可随应用分发的x64 MSVC CRT：$redistRoot" }
$crt = $crtDirectory.FullName
Copy-Item (Join-Path $crt '*.dll') dist/windows-payload -Force
Compress-Archive -Path dist/windows-payload/* -DestinationPath dist/windows-ui-payload.zip -Force
$env:QIANBIAN_UI_PAYLOAD = (Resolve-Path dist/windows-ui-payload.zip).Path
cargo build --locked --release --bin qianbian-ui
if ($LASTEXITCODE) { throw '单文件启动器构建失败' }
Copy-Item target/release/qianbian-ui.exe dist/qianbian-windows-ui.exe
Get-FileHash dist/qianbian-windows-ui.exe -Algorithm SHA256
