# Regenerates engine/engine_core/src/ffmpeg_sys.rs from the FFmpeg 5.1.2 headers
# that ship with the pinned gyan.dev full_build_shared archive.
#
# Requires: bindgen-cli (cargo install bindgen-cli) and libclang.
#   python -m pip install libclang
#
# Usage:
#   ./gen-bindings.ps1 -FfmpegDir C:\path\to\ffmpeg-5.1.2-full_build-shared
param(
    [Parameter(Mandatory = $true)][string]$FfmpegDir
)

$ErrorActionPreference = 'Stop'

$include = Join-Path $FfmpegDir 'include'
if (-not (Test-Path $include)) { throw "No include/ under $FfmpegDir" }

if (-not $env:LIBCLANG_PATH) {
    $candidate = Join-Path (python -c "import clang,os;print(os.path.dirname(clang.__file__))") 'native'
    if (Test-Path (Join-Path $candidate 'libclang.dll')) { $env:LIBCLANG_PATH = $candidate }
}

$bindgen = Join-Path $env:USERPROFILE '.cargo\bin\bindgen.exe'
if (-not (Test-Path $bindgen)) { throw 'bindgen.exe not found; run: cargo install bindgen-cli' }

$wrapper = Join-Path $PSScriptRoot '..\engine_core\ffmpeg-bindings\wrapper.h'
$srcDir = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\engine_core\src')).Path
$out = Join-Path $srcDir 'ffmpeg_sys.rs'
# bindgen writes straight to the target path; going through a temp file keeps an open editor or
# rust-analyzer from turning the rebuild into "Access is denied" (the move is atomic-ish on NTFS).
$tmp = Join-Path $srcDir 'ffmpeg_sys.rs.tmp'

$bindgenArgs = @(
    (Resolve-Path $wrapper).Path
    '--output', $tmp
    '--allowlist-type', 'AV(Codec|CodecContext|CodecParameters|Frame|Packet|Stream|FormatContext|OutputFormat|Dictionary|Rational|ChannelLayout|IOContext|Class)'
    '--allowlist-type', 'SwsContext|AVPixelFormat|AVSampleFormat|AVMediaType|AVCodecID|AVFieldOrder'
    '--allowlist-function', 'av_.*'
    '--allowlist-function', 'sws_.*'
    '--no-doc-comments'
    '--'
    "-I$include"
)

& $bindgen @bindgenArgs
if ($LASTEXITCODE -ne 0) { throw "bindgen failed with $LASTEXITCODE" }
Move-Item -LiteralPath $tmp -Destination $out -Force

$lines = (Get-Content $out | Measure-Object -Line).Lines
Write-Host "Wrote $out ($lines lines)"

