# Builds the version of Maze Cat that stays above the taskbar, signed with your local
# certificate and installed for all users under Program Files (the installer asks for admin).
# Run from the repo folder: powershell -ExecutionPolicy Bypass -File scripts\build-local.ps1
#
# Two programs go into the installer:
#   mazecat.exe      the app (lists, picker, timer logic), a normal program
#   mazecat-bar.exe  the bar on the taskbar, small and native, with the uiAccess permission
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
foreach ($t in @(@{c='cargo'; why='Rust is not installed. Run: winget install --id Rustlang.Rustup -e (then open a new PowerShell)'}, @{c='npx'; why='Node.js is not installed. Install Node 22 from nodejs.org'})) {
  if (-not (Get-Command $t.c -ErrorAction SilentlyContinue)) { Write-Host $t.why -ForegroundColor Yellow; exit 1 }
}
if (-not (Test-Path node_modules)) { npm ci }
$sign = (Resolve-Path scripts\sign.ps1).Path

# 1. The bar: build, sign, and hand it to Tauri as a bundled program ("sidecar").
cargo build --release --manifest-path bar\Cargo.toml
if ($LASTEXITCODE -ne 0) { throw 'Building the bar failed.' }
$triple = ((rustc -vV) | Select-String '^host: (.+)$').Matches[0].Groups[1].Value
New-Item -ItemType Directory -Force src-tauri\binaries | Out-Null
$barExe = "src-tauri\binaries\mazecat-bar-$triple.exe"
Copy-Item bar\target\release\mazecat-bar.exe $barExe -Force
& $sign (Resolve-Path $barExe).Path

# 2. The app and the installer, with the bar inside.
$conf = Join-Path $env:TEMP 'mazecat-local.conf.json'
@{ bundle = @{
  externalBin = @('binaries/mazecat-bar')
  windows = @{
    nsis = @{ installMode = 'perMachine' }
    signCommand = @{ cmd = 'powershell'; args = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $sign, '%1') }
  }
} } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 $conf
try { npx tauri build --bundles nsis --config $conf } finally { Remove-Item $conf }
Get-ChildItem src-tauri\target\release\bundle\nsis\*.exe -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "Installer: $($_.FullName)" }
