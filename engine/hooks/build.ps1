# Builds the x64 in-game hooks with the installed Visual C++ toolset:
#
#   engine/hooks/bin/capture_hook.dll   - Rust present-hook driver (cargo build)
#   engine/hooks/bin/overlay_hook.dll   - in-game clip notification card
#
#   powershell -ExecutionPolicy Bypass -File engine/hooks/build.ps1
#
# MinHook (BSD-2-Clause) is vendored under third_party/minhook.

param(
    [ValidateSet('x64', 'x86')]
    [string]$Arch = 'x64'
)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $here
try {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) {
        $vswhere = Join-Path $env:ProgramFiles 'Microsoft Visual Studio\Installer\vswhere.exe'
    }
    if (-not (Test-Path $vswhere)) { throw 'vswhere.exe not found; install Visual Studio C++ build tools.' }

    $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vs) { throw 'No Visual Studio installation with the C++ toolset was found.' }
    $vcvars = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'
    if ($Arch -eq 'x86') { $vcvars = Join-Path $vs 'VC\Auxiliary\Build\vcvars32.bat' }
    if (-not (Test-Path $vcvars)) { throw "vcvars not found: $vcvars" }

    New-Item -ItemType Directory -Force -Path (Join-Path $here 'obj'), (Join-Path $here 'bin') | Out-Null

    if ($Arch -ne 'x64') { throw 'The application injects x64 games only; the capture hook is x64-only.' }
    $suffix = ''
    $minhook = @(
        'third_party\minhook\src\buffer.c',
        'third_party\minhook\src\hook.c',
        'third_party\minhook\src\trampoline.c',
        'third_party\minhook\src\hde\hde32.c',
        'third_party\minhook\src\hde\hde64.c'
    )

    function Invoke-OverlayBuild([string]$source, [string]$output, [string]$objDir) {
        New-Item -ItemType Directory -Force -Path (Join-Path $here $objDir) | Out-Null
        $pdb = Join-Path $objDir 'hook.pdb'
        $cmd = 'call "' + $vcvars + '" >nul && cl /nologo /std:c++17 /O2 /EHsc /MT /W3 ' +
            '/DNDEBUG /DWIN32 /D_WINDOWS /D_CRT_SECURE_NO_WARNINGS ' +
            '/I. /Ithird_party\minhook\include ' +
            $source + ' /Fo:' + $objDir + '\ /Fd:' + $pdb + ' /link /DLL ' +
            '/LIBPATH:obj\minhook minhook.lib ' +
            '/OUT:' + $output + ' user32.lib gdi32.lib opengl32.lib gdiplus.lib'
        cmd.exe /c $cmd
        if ($LASTEXITCODE -ne 0) { throw "cl failed for $source with exit code $LASTEXITCODE" }
    }

    function Invoke-CaptureBuild([string]$output) {
        # The capture hook is the Rust cdylib in capture_hook/src/capture_hook.rs.
        # In recorder builds it only drives per-frame overlay callbacks inside
        # the game; video capture itself uses WGC / DXGI desktop duplication.
        $manifest = Join-Path $here 'capture_hook\Cargo.toml'
        & cargo build --release --manifest-path $manifest
        if ($LASTEXITCODE -ne 0) { throw "Rust capture hook build failed with exit code $LASTEXITCODE" }
        $built = Join-Path $here 'capture_hook\target\release\capture_hook.dll'
        if (-not (Test-Path $built)) { throw "Rust capture hook output not found: $built" }
        Copy-Item -Force $built (Join-Path $here $output)
    }

    $minhookObj = Join-Path $here 'obj\minhook'
    New-Item -ItemType Directory -Force -Path $minhookObj | Out-Null
    $minhookSources = $minhook -join ' '
    $minhookCmd = 'call "' + $vcvars + '" >nul && cl /nologo /O2 /MT /Zl /W3 /c ' +
        '/DNDEBUG /DWIN32 /D_WINDOWS /Ithird_party\minhook\include ' +
        $minhookSources + ' /Fo:obj\minhook\ && lib /nologo /OUT:obj\minhook\minhook.lib obj\minhook\*.obj'
    cmd.exe /c $minhookCmd
    if ($LASTEXITCODE -ne 0) { throw "MinHook build failed with exit code $LASTEXITCODE" }

    $capture = "bin\capture_hook$suffix.dll"
    $overlay = "bin\overlay_hook$suffix.dll"
    Invoke-CaptureBuild $capture

    Invoke-OverlayBuild 'overlay_hook\overlay_hook.cpp' $overlay 'obj\overlay'

    Write-Host "Built $here\$capture"
    Write-Host "Built $here\$overlay"
}
finally {
    Pop-Location
}
