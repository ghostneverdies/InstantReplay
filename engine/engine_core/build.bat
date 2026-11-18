@echo off
rem ---------------------------------------------------------------------------------------------
rem Builds engine\bin\engine.dll (the C++17 engine) with MSVC. No CMake.
rem
rem   build.bat           release build
rem   build.bat debug     debug build (/Od /Zi)
rem   build.bat clean     remove build output
rem
rem Needs: Visual Studio / Build Tools (C++ workload, Windows 10/11 SDK) in its default install
rem location, and the FFmpeg 5.1.2 SDK (headers + import libs) in engine\encoder\sdk, which
rem engine\encoder\fetch-ffmpeg.ps1 installs.
rem ---------------------------------------------------------------------------------------------
setlocal EnableExtensions EnableDelayedExpansion

set "HERE=%~dp0"
if "%HERE:~-1%"=="\" set "HERE=%HERE:~0,-1%"
set "ENGINE_ROOT=%HERE%\.."
set "OUT_DIR=%ENGINE_ROOT%\bin"
set "BUILD_DIR=%HERE%\build"
set "GEN_DIR=%BUILD_DIR%\gen"
set "OBJ_DIR=%BUILD_DIR%\obj"
set "FFSDK=%ENGINE_ROOT%\encoder\sdk"

set "MODE=%~1"
if /i "%MODE%"=="clean" (
    if exist "%BUILD_DIR%" rmdir /s /q "%BUILD_DIR%"
    if exist "%OUT_DIR%\engine.dll" del /q "%OUT_DIR%\engine.dll"
    echo Cleaned.
    exit /b 0
)

rem ---- 1. MSVC environment (default install locations; always select x64) ----
rem A cl.exe already on PATH may be the 32-bit compiler, so run vcvars64.bat whenever it is
rem available; only fall back to the ambient PATH for custom/non-default installs.
set "VCVARS="
for %%Y in (18 2022 2019) do (
    for %%E in (Community Professional Enterprise BuildTools Preview) do (
        for %%P in ("%ProgramFiles%" "%ProgramFiles(x86)%") do (
            if not defined VCVARS if exist "%%~P\Microsoft Visual Studio\%%Y\%%E\VC\Auxiliary\Build\vcvars64.bat" (
                set "VCVARS=%%~P\Microsoft Visual Studio\%%Y\%%E\VC\Auxiliary\Build\vcvars64.bat"
            )
        )
    )
)
if defined VCVARS (
    echo Using !VCVARS!
    call "!VCVARS!" >nul
    if errorlevel 1 (
        echo ERROR: vcvars64.bat failed.
        exit /b 1
    )
) else (
    where cl.exe >nul 2>nul
    if errorlevel 1 (
        echo ERROR: Visual Studio / Build Tools with the C++ workload was not found in its default location.
        echo        Install "Desktop development with C++" or run this from an x64 Native Tools prompt.
        exit /b 1
    )
)

rem ---- 2. FFmpeg SDK ----
for %%F in ("%FFSDK%\include\libavcodec\avcodec.h" "%FFSDK%\lib\avcodec.lib" "%FFSDK%\lib\avformat.lib" "%FFSDK%\lib\avutil.lib" "%FFSDK%\lib\swresample.lib") do (
    if not exist "%%~F" (
        echo ERROR: FFmpeg 5.1.2 SDK file missing: %%~F
        echo        Run engine\encoder\fetch-ffmpeg.ps1 first.
        exit /b 1
    )
)

rem ---- 3. fxc (Windows SDK) ----
set "FXC="
if defined WindowsSdkVerBinPath if exist "%WindowsSdkVerBinPath%x64\fxc.exe" set "FXC=%WindowsSdkVerBinPath%x64\fxc.exe"
if not defined FXC (
    for /f "delims=" %%D in ('dir /b /ad /o-n "%ProgramFiles(x86)%\Windows Kits\10\bin\10.*" 2^>nul') do (
        if not defined FXC if exist "%ProgramFiles(x86)%\Windows Kits\10\bin\%%D\x64\fxc.exe" set "FXC=%ProgramFiles(x86)%\Windows Kits\10\bin\%%D\x64\fxc.exe"
    )
)
if not defined FXC (
    echo ERROR: fxc.exe was not found. Install the Windows 10/11 SDK.
    exit /b 1
)

if not exist "%GEN_DIR%" mkdir "%GEN_DIR%"
if not exist "%OBJ_DIR%" mkdir "%OBJ_DIR%"
if not exist "%OUT_DIR%" mkdir "%OUT_DIR%"

rem ---- 4. Shaders: hand-translated OBS effects -> DXBC byte arrays (build\gen\*.h) ----
echo Compiling shaders with !FXC!
set "SH=%HERE%\shaders"
call :fxc "%SH%\obs_nv12.hlsl"          VSPos             vs_5_0 obs_vs_pos           || exit /b 1
call :fxc "%SH%\obs_nv12.hlsl"          VSTexPos_Left     vs_5_0 obs_vs_texpos_left   || exit /b 1
call :fxc "%SH%\obs_nv12.hlsl"          PS_Y              ps_5_0 obs_ps_y             || exit /b 1
call :fxc "%SH%\obs_nv12.hlsl"          PS_UV_Wide        ps_5_0 obs_ps_uv_wide       || exit /b 1
call :fxc "%SH%\obs_scale_bicubic.hlsl" VSDefault         vs_5_0 obs_vs_bicubic       || exit /b 1
call :fxc "%SH%\obs_scale_bicubic.hlsl" PSDrawBicubicRGBA ps_5_0 obs_ps_bicubic       || exit /b 1
call :fxc "%SH%\obs_scale_bilinear.hlsl" VSDefault        vs_5_0 obs_vs_bilinear      || exit /b 1
call :fxc "%SH%\obs_scale_bilinear.hlsl" PSDrawBilinear   ps_5_0 obs_ps_bilinear      || exit /b 1

rem ---- 5. Engine DLL ----
set "CFLAGS=/nologo /std:c++17 /EHsc /MT /W3 /utf-8 /permissive- /DUNICODE /D_UNICODE /DNOMINMAX /DWIN32_LEAN_AND_MEAN"
if /i "%MODE%"=="debug" (
    set "CFLAGS=!CFLAGS! /Od /Zi /DDEBUG"
) else (
    set "CFLAGS=!CFLAGS! /O2 /GL /DNDEBUG"
)
set LFLAGS=/DLL /OUT:"%OUT_DIR%\engine.dll" /LIBPATH:"%FFSDK%\lib" /OPT:REF /OPT:ICF
if /i not "%MODE%"=="debug" set LFLAGS=!LFLAGS! /LTCG
if /i "%MODE%"=="debug" set LFLAGS=!LFLAGS! /DEBUG

echo Compiling engine...
cl.exe %CFLAGS% /I"%FFSDK%\include" /I"%BUILD_DIR%" /Fo"%OBJ_DIR%\\" /Fd"%OBJ_DIR%\engine.pdb" ^
    "%HERE%\src\ir_common.cpp" "%HERE%\src\gpu_pipeline.cpp" "%HERE%\src\ffmpeg_rt.cpp" ^
    "%HERE%\src\mf_encoder.cpp" "%HERE%\src\capture.cpp" "%HERE%\src\audio.cpp" "%HERE%\src\engine.cpp" ^
    /link %LFLAGS% ^
    avcodec.lib avformat.lib avutil.lib swresample.lib delayimp.lib ^
    d3d11.lib dxgi.lib dwmapi.lib mfplat.lib mf.lib mfuuid.lib mmdevapi.lib ole32.lib oleaut32.lib propsys.lib ^
    runtimeobject.lib user32.lib gdi32.lib advapi32.lib ^
    /DELAYLOAD:avcodec-59.dll /DELAYLOAD:avformat-59.dll /DELAYLOAD:avutil-57.dll /DELAYLOAD:swresample-4.dll
if errorlevel 1 (
    echo ERROR: engine build failed.
    exit /b 1
)
echo Built "%OUT_DIR%\engine.dll"
exit /b 0

:fxc
rem %1 = source, %2 = entry point, %3 = profile, %4 = C array name
"%FXC%" /nologo /O3 /E %2 /T %3 /Vn %4 /Fh "%GEN_DIR%\%4.h" %1
if errorlevel 1 (
    echo ERROR: shader %~nx1 [%2] failed to compile.
    exit /b 1
)
exit /b 0
