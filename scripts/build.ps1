param([switch]$SkipTests)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
if (Test-Path '.tools/cargo/bin/cargo.exe') {
  $env:RUSTUP_HOME = Join-Path (Get-Location) '.tools/rustup'
  $env:CARGO_HOME = Join-Path (Get-Location) '.tools/cargo'
  $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}
if (-not (Test-Path 'src-tauri/resources/bin/ffmpeg.exe')) { & "$PSScriptRoot/prepare-media.ps1" }
node scripts/build-help.mjs
if ($LASTEXITCODE) { throw 'Help generation failed' }
node scripts/check-resources.mjs
if ($LASTEXITCODE) { throw 'Localization checks failed' }
# Tauri's generate_context! embeds frontendDist even in Rust test builds.
# A fresh checkout has no dist directory until the frontend is compiled.
npm run build
if ($LASTEXITCODE) { throw 'Frontend build failed' }
if (-not $SkipTests) {
  cargo test --manifest-path src-tauri/Cargo.toml --locked
  if ($LASTEXITCODE) { throw 'Rust tests failed' }
}
node node_modules/@tauri-apps/cli/tauri.js build
if ($LASTEXITCODE) { throw 'Installer build failed' }
New-Item -ItemType Directory -Force artifacts | Out-Null
$taskVersion = (Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
$taskInstaller = Get-Item "src-tauri/target/release/bundle/nsis/SubForge_${taskVersion}_x64-setup.exe" -ErrorAction SilentlyContinue
if (-not $taskInstaller) { throw 'Missing x64 installer' }
Copy-Item -LiteralPath $taskInstaller.FullName -Destination 'artifacts/SubForge-Setup-x64.exe' -Force
$taskHash = Get-FileHash 'artifacts/SubForge-Setup-x64.exe' -Algorithm SHA256
"$($taskHash.Hash.ToLowerInvariant())  SubForge-Setup-x64.exe" | Set-Content 'artifacts/SHA256SUMS.txt' -Encoding ascii
Write-Host "Installer ready: $((Resolve-Path 'artifacts/SubForge-Setup-x64.exe').Path)"
