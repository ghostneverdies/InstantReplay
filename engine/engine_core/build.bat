@echo off
rem ---------------------------------------------------------------------------------------------
rem Builds engine\bin\engine.dll (the C++20 engine) with MSVC. No CMake.
rem
rem   build.bat           release build
rem   build.bat debug     debug build (/Od /Zi)
rem   build.bat clean     remove build output
rem
rem Needs: Visual Studio / Build Tools (C++ workload, Windows 10/11 SDK) in its default install
rem location. Video/audio encoding and muxing use Media Foundation only; shader bytecode is
rem embedded in video.cpp, so no external shader or encoder SDK is required.
rem ---------------------------------------------------------------------------------------------
setlocal EnableExtensions EnableDelayedExpansion

set "HERE=%~dp0"
if "%HERE:~-1%"=="\" set "HERE=%HERE:~0,-1%"
set "ENGINE_ROOT=%HERE%\.."
set "OUT_DIR=%ENGINE_ROOT%\bin"
set "BUILD_DIR=%HERE%\build"
set "OBJ_DIR=%BUILD_DIR%\obj"

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

if not exist "%OBJ_DIR%" mkdir "%OBJ_DIR%"
if not exist "%OUT_DIR%" mkdir "%OUT_DIR%"

rem ---- 2. Engine DLL ----
set "CFLAGS=/nologo /std:c++20 /EHsc /MT /W3 /utf-8 /permissive- /DUNICODE /D_UNICODE /DNOMINMAX /DWIN32_LEAN_AND_MEAN"
if /i "%MODE%"=="debug" (
    set "CFLAGS=!CFLAGS! /Od /Zi /DDEBUG"
) else (
    set "CFLAGS=!CFLAGS! /O2 /GL /DNDEBUG"
)
set LFLAGS=/DLL /OUT:"%OUT_DIR%\engine.dll" /OPT:REF /OPT:ICF
if /i not "%MODE%"=="debug" set LFLAGS=!LFLAGS! /LTCG
if /i "%MODE%"=="debug" set LFLAGS=!LFLAGS! /DEBUG

echo Compiling engine...
cl.exe %CFLAGS% /I"%BUILD_DIR%" /Fo"%OBJ_DIR%\\" /Fd"%OBJ_DIR%\engine.pdb" ^
    "%HERE%\src\capture.cpp" "%HERE%\src\video.cpp" "%HERE%\src\audio.cpp" "%HERE%\src\engine.cpp" ^
    /link %LFLAGS% ^
    d3d11.lib dxgi.lib dwmapi.lib mfplat.lib mf.lib mfreadwrite.lib mfuuid.lib mmdevapi.lib ole32.lib oleaut32.lib propsys.lib ^
    runtimeobject.lib user32.lib gdi32.lib advapi32.lib wmcodecdspuuid.lib
if errorlevel 1 (
    echo ERROR: engine build failed.
    exit /b 1
)
echo Built "%OUT_DIR%\engine.dll"
exit /b 0
