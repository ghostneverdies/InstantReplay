@echo off
setlocal
set "ROOT=%~dp0..\.."
set "OUT=%ROOT%\engine\bin"
set "OBJ=%~dp0build"
if not exist "%OUT%" mkdir "%OUT%"
if not exist "%OBJ%" mkdir "%OBJ%"
cl /nologo /std:c++20 /O2 /Zi /EHsc /MT /Gd /W3 /WX- /DWIN32 /D_WINDOWS /DUNICODE /D_UNICODE /DNDEBUG /D_CRT_SECURE_NO_WARNINGS /I"%~dp0." "%~dp0engine.cpp" /Fo:"%OBJ%\\" /Fd:"%OBJ%\engine.pdb" /link /DLL /MAP:"%OBJ%\engine.map" /OUT:"%OUT%\engine.dll" kernel32.lib user32.lib gdi32.lib ole32.lib oleaut32.lib uuid.lib runtimeobject.lib windowsapp.lib dxgi.lib d3d11.lib dxguid.lib mf.lib mfplat.lib mfreadwrite.lib mfuuid.lib propsys.lib avrt.lib winmm.lib
if errorlevel 1 (
    echo engine.dll build failed.
    exit /b 1
)
echo Built %OUT%\engine.dll
endlocal
