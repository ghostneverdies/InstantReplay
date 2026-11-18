# Replaces the FFmpeg binaries in engine/encoder with the pinned FFmpeg 5.1.2 shared build
# (gyan.dev full_build-shared, mirrored on GitHub). Download, checksum and staging all happen
# before anything installed is touched, so a network failure leaves the previous install intact.
# Stale DLLs/EXEs (including any FFmpeg 8.x avcodec-62 / avformat-62 / avutil-60 / swresample-6)
# are removed only after the new binaries are in place.
#
# It also installs the build SDK (headers + import libs) into engine\encoder\sdk, which
# engine\engine_core\build.bat compiles and links the C++ engine against.
#
# Usage:
#   ./fetch-ffmpeg.ps1     # download, verify, install the 4 runtime DLLs and the build SDK
param()

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$version = '5.1.2'
$archive = "ffmpeg-$version-full_build-shared.zip"
$url = "https://github.com/GyanD/codexffmpeg/releases/download/$version/$archive"
$sha256 = 'bbfa11cd2fa7eee47ca23a32f0ae8743d65bb9d37a8e3847b8cd23accfcc2a31'
$dlls = 'avcodec-59.dll', 'avformat-59.dll', 'avutil-57.dll', 'swresample-4.dll'
$libs = 'avcodec', 'avformat', 'avutil', 'swresample'

$dest = $PSScriptRoot
$work = Join-Path $env:TEMP "ir-ffmpeg-$version"
$zip = Join-Path $work $archive
$extract = Join-Path $work 'x'
$stage = Join-Path $work 'stage'
$backup = Join-Path $work 'backup'

New-Item -ItemType Directory -Force $work | Out-Null

# Never trust a cached zip just because it exists: a truncated download is discarded and retried.
if (Test-Path -LiteralPath $zip) {
    $cached = Get-Item -LiteralPath $zip
    if ($cached.Length -lt (50 * 1024 * 1024)) {
        Write-Host "Cached $archive is incomplete ($($cached.Length) bytes); downloading again."
        Remove-Item -LiteralPath $zip -Force
    }
}
if (-not (Test-Path -LiteralPath $zip)) {
    Write-Host "Downloading $url"
    Invoke-WebRequest -Uri $url -OutFile $zip
}

$hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($hash -ne $sha256) {
    Remove-Item -LiteralPath $zip -Force
    throw "Checksum mismatch for $archive : got $hash, expected $sha256"
}

if (Test-Path -LiteralPath $extract) { Remove-Item -LiteralPath $extract -Recurse -Force }
if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Force $stage | Out-Null
Expand-Archive -LiteralPath $zip -DestinationPath $extract -Force
$root = (Get-ChildItem -LiteralPath $extract -Directory | Select-Object -First 1).FullName
if (-not $root) { throw "Archive has no root folder: $archive" }

# Validate and stage every file before moving anything installed out of the way.
foreach ($dll in $dlls) {
    $src = Join-Path $root "bin\$dll"
    if (-not (Test-Path -LiteralPath $src)) { throw "Archive is missing $dll" }
    Copy-Item -LiteralPath $src -Destination (Join-Path $stage $dll) -Force
}
$sdkNew = Join-Path $stage 'sdk'
New-Item -ItemType Directory -Force (Join-Path $sdkNew 'lib') | Out-Null
$include = Join-Path $root 'include'
if (-not (Test-Path -LiteralPath $include)) { throw "Archive is missing include" }
Copy-Item -LiteralPath $include -Destination (Join-Path $sdkNew 'include') -Recurse -Force
foreach ($lib in $libs) {
    $src = Join-Path $root "lib\$lib.lib"
    if (-not (Test-Path -LiteralPath $src)) { throw "Archive is missing $lib.lib" }
    Copy-Item -LiteralPath $src -Destination (Join-Path $sdkNew 'lib') -Force
}

if (Test-Path -LiteralPath $backup) { Remove-Item -LiteralPath $backup -Recurse -Force }
New-Item -ItemType Directory -Force $backup | Out-Null
$sdk = Join-Path $dest 'sdk'
try {
    foreach ($dll in $dlls) {
        $target = Join-Path $dest $dll
        if (Test-Path -LiteralPath $target) { Move-Item -LiteralPath $target -Destination (Join-Path $backup $dll) -Force }
    }
    if (Test-Path -LiteralPath $sdk) { Move-Item -LiteralPath $sdk -Destination (Join-Path $backup 'sdk') -Force }

    foreach ($dll in $dlls) {
        Move-Item -LiteralPath (Join-Path $stage $dll) -Destination (Join-Path $dest $dll) -Force
    }
    Move-Item -LiteralPath $sdkNew -Destination $sdk -Force

    Get-ChildItem -LiteralPath $dest -File | Where-Object { $_.Extension -in @('.dll', '.exe') -and $_.Name -notin $dlls } | ForEach-Object {
        Write-Host "Removing stale binary $($_.Name)"
        Remove-Item -LiteralPath $_.FullName -Force
    }
} catch {
    # Restore the previous install if any replacement step failed.
    foreach ($dll in $dlls) {
        $saved = Join-Path $backup $dll
        if (Test-Path -LiteralPath $saved) { Move-Item -LiteralPath $saved -Destination (Join-Path $dest $dll) -Force }
    }
    $savedSdk = Join-Path $backup 'sdk'
    if (Test-Path -LiteralPath $savedSdk) {
        if (Test-Path -LiteralPath $sdk) { Remove-Item -LiteralPath $sdk -Recurse -Force }
        Move-Item -LiteralPath $savedSdk -Destination $sdk -Force
    }
    throw
}

Write-Host "Build SDK (headers + import libs): $sdk"
Write-Host "FFmpeg $version runtime installed in $dest :"
Get-ChildItem -LiteralPath $dest -Filter *.dll | Select-Object Name, Length | Format-Table -AutoSize
