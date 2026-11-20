@echo off
rem ---------------------------------------------------------------------------------------------
rem Builds engine\bin\engine.dll as a single C11 translation unit with MSVC. No CMake.
rem
rem   build.bat           release build
rem   build.bat debug     debug build (/Od /Zi)
rem   build.bat clean     remove build output
rem
rem Needs: Visual Studio / Build Tools (C++ workload, Windows 10/11 SDK) in its default install
rem location. Compute shaders are compiled from the existing shaders\obs_nv12.hlsl with fxc into
rem build\gen; video/audio encoding and muxing use Media Foundation only.
rem ---------------------------------------------------------------------------------------------
setlocal EnableExtensions EnableDelayedExpansion

set "HERE=%~dp0"
if "%HERE:~-1%"=="\" set "HERE=%HERE:~0,-1%"
set "ENGINE_ROOT=%HERE%\.."
set "OUT_DIR=%ENGINE_ROOT%\bin"
set "BUILD_DIR=%HERE%\build"
set "OBJ_DIR=%BUILD_DIR%\obj"
set "GEN_DIR=%BUILD_DIR%\gen"
set "SHADER=%HERE%\shaders\obs_nv12.hlsl"

set "MODE=%~1"
if /i "%MODE%"=="clean" (
    if exist "%BUILD_DIR%" rmdir /s /q "%BUILD_DIR%"
    if exist "%OUT_DIR%\engine.dll" del /q "%OUT_DIR%\engine.dll"
    echo Cleaned.
    exit /b 0
)

rem ---- 1. MSVC environment (default install locations; always select x64) ----
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

if not exist "%OBJ_DIR%" mkdir "%OBJ_DIR%"
if not exist "%GEN_DIR%" mkdir "%GEN_DIR%"
if not exist "%OUT_DIR%" mkdir "%OUT_DIR%"

rem ---- 2. Shader headers ----
if /i "%MODE%"=="debug" (
    set "FXCFLAGS=/nologo /Od /Zi /Ges /T cs_5_0 /WX"
) else (
    set "FXCFLAGS=/nologo /O3 /Qstrip_reflect /Qstrip_debug /Qstrip_priv /Ges /T cs_5_0 /WX"
)

if not exist "%SHADER%" (
    echo ERROR: shader source not found: %SHADER%
    exit /b 1
)

echo Compiling shaders...
call :fxc ResizePassH IrResizePassHShaderBytes || exit /b 1
call :fxc ResizePassV IrResizePassVShaderBytes || exit /b 1
call :fxc ResizeLinearPassH IrResizeLinearPassHShaderBytes || exit /b 1
call :fxc ResizeLinearPassV IrResizeLinearPassVShaderBytes || exit /b 1
call :fxc ConvertSinglePass IrConvertSinglePassShaderBytes || exit /b 1
call :fxc ConvertPass1 IrConvertPass1ShaderBytes || exit /b 1
call :fxc ConvertPass2 IrConvertPass2ShaderBytes || exit /b 1

rem ---- 3. Engine DLL ----
set CFLAGS=/nologo /TC /std:c11 /experimental:c11atomics /W3 /utf-8 /MT /DUNICODE /D_UNICODE /DNOMINMAX /DWIN32_LEAN_AND_MEAN /D_CRT_SECURE_NO_WARNINGS /I"%BUILD_DIR%"
if /i "%MODE%"=="debug" (
    set "CFLAGS=!CFLAGS! /Od /Zi /DDEBUG"
) else (
    set "CFLAGS=!CFLAGS! /O2 /GL /DNDEBUG"
)
set LFLAGS=/DLL /OUT:"%OUT_DIR%\engine.dll" /OPT:REF /OPT:ICF
if /i not "%MODE%"=="debug" set LFLAGS=!LFLAGS! /LTCG
if /i "%MODE%"=="debug" set LFLAGS=!LFLAGS! /DEBUG

echo Compiling engine...
cl.exe %CFLAGS% /Fo"%OBJ_DIR%\engine.obj" /Fd"%OBJ_DIR%\engine.pdb" "%HERE%\src\engine.c" ^
    /link %LFLAGS% ^
    d3d11.lib d3dcompiler.lib dxgi.lib dwmapi.lib mfplat.lib mf.lib mfreadwrite.lib mfuuid.lib ^
    evr.lib mmdevapi.lib ole32.lib oleaut32.lib propsys.lib runtimeobject.lib coremessaging.lib ^
    strmiids.lib mincore.lib synchronization.lib ^
    user32.lib gdi32.lib advapi32.lib wmcodecdspuuid.lib avrt.lib ntdll.lib
if errorlevel 1 (
    echo ERROR: engine build failed.
    exit /b 1
)
echo Built "%OUT_DIR%\engine.dll"
exit /b 0

:fxc
fxc.exe %FXCFLAGS% /E %1 /Fo "%GEN_DIR%\%1.dxbc" "%SHADER%" || exit /b 1
fxc.exe /nologo /compress /Vn %2 /Fo "%GEN_DIR%\%1.dxbc" /Fh "%GEN_DIR%\%1.h" "%GEN_DIR%\%1.dxbc" || exit /b 1
exit /b 0
