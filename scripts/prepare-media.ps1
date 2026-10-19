$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
New-Item -ItemType Directory -Force '.tools/media','src-tauri/resources/bin','src-tauri/resources/licenses' | Out-Null
$taskVersion = '9.0.2'
$taskUrl = "https://github.com/GyanD/codexffmpeg/releases/download/$taskVersion/ffmpeg-$taskVersion-full_build.zip"
$taskArchive = [IO.Path]::GetFullPath('.tools/media/ffmpeg-9.0.2.zip')
Invoke-WebRequest $taskUrl -OutFile $taskArchive
if ((Get-FileHash $taskArchive -Algorithm SHA256).Hash.ToLowerInvariant() -ne '759d0a9831c436a0eb331ad36f236c06cb04aaa0005da46571f0e9d3d9206f6b') { throw 'FFmpeg archive checksum failed' }
Expand-Archive -LiteralPath $taskArchive -DestinationPath '.tools/media' -Force
$taskMediaRoot = Get-ChildItem '.tools/media' -Directory | Where-Object Name -eq 'ffmpeg-9.0.2-full_build' | Select-Object -First 1
if (-not $taskMediaRoot) { throw 'Official FFmpeg package layout changed' }
Copy-Item -LiteralPath (Join-Path $taskMediaRoot.FullName 'bin/ffmpeg.exe'),(Join-Path $taskMediaRoot.FullName 'bin/ffprobe.exe') -Destination 'src-tauri/resources/bin/' -Force
Copy-Item -LiteralPath (Join-Path $taskMediaRoot.FullName 'LICENSE') -Destination 'src-tauri/resources/licenses/FFmpeg-GPL-3.0.txt' -Force
Copy-Item -LiteralPath (Join-Path $taskMediaRoot.FullName 'README.txt') -Destination 'src-tauri/resources/licenses/FFmpeg-BUILD.txt' -Force
Invoke-WebRequest 'https://github.com/FFmpeg/FFmpeg/archive/946fcce07b.tar.gz' -OutFile 'src-tauri/resources/licenses/FFmpeg-9.0.2-source.tar.gz'
Invoke-WebRequest 'https://raw.githubusercontent.com/ggml-org/whisper.cpp/v1.9.4/LICENSE' -OutFile 'src-tauri/resources/licenses/whisper-cpp-MIT.txt'
Invoke-WebRequest 'https://raw.githubusercontent.com/openai/whisper/main/LICENSE' -OutFile 'src-tauri/resources/licenses/Whisper-MIT.txt'
& '.\src-tauri\resources\bin\ffmpeg.exe' -hide_banner -h filter=whisper
if ($LASTEXITCODE) { throw 'Required Whisper filter is unavailable' }
Write-Host 'Pinned media tools and upstream source/license files are ready.'
