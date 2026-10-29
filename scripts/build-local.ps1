# Builds the version of Maze Cat that stays above the taskbar, signed with your local
# certificate and installed for all users under Program Files (the installer asks for admin).
# Run from the repo folder: powershell -ExecutionPolicy Bypass -File scripts\build-local.ps1
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
foreach ($t in @(@{c='cargo'; why='Rust is not installed. Run: winget install --id Rustlang.Rustup -e (then open a new PowerShell)'}, @{c='npx'; why='Node.js is not installed. Install Node 22 from nodejs.org'})) {
  if (-not (Get-Command $t.c -ErrorAction SilentlyContinue)) { Write-Host $t.why -ForegroundColor Yellow; exit 1 }
}
if (-not (Test-Path node_modules)) { npm ci }
$sign = (Resolve-Path scripts\sign.ps1).Path
$conf = Join-Path $env:TEMP 'mazecat-local.conf.json'
@{ bundle = @{ windows = @{
  nsis = @{ installMode = 'perMachine' }
  signCommand = @{ cmd = 'powershell'; args = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $sign, '%1') }
} } } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 $conf
$env:MAZECAT_UIACCESS = '1'
try { npx tauri build --bundles nsis --config $conf } finally { Remove-Item Env:MAZECAT_UIACCESS; Remove-Item $conf }
Get-ChildItem src-tauri\target\release\bundle\nsis\*.exe -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "Installer: $($_.FullName)" }
