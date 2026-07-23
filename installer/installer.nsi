; ==========================================================================
;  Instant Replay - NSIS Installer
; ==========================================================================

SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "x64.nsh"
!include "LogicLib.nsh"

; --------------------------------
; 1. Definitions
; --------------------------------
!define MyAppName "Instant Replay"
!define MyAppVersion "1.1"
!define MyAppPublisher "InstantReplay"
!define MyAppExeName "InstantReplay.exe"
!define MyAppAssocName "${MyAppName} File"
!define MyAppAssocExt ".ir"
!define MyAppAssocKey "InstantReplayFile"
!define GitHubUrl "https://github.com/ghostneverdies/InstantReplay"

!define FFmpegDownloadUrl "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
!define FFmpegBinDir "C:\ffmpeg"
!define AudioCaptureDownloadUrl "https://github.com/rdp/screen-capture-recorder-to-video-windows-free/releases/download/v0.13.3/Setup.Screen.Capturer.Recorder.v0.13.3.exe"
!define AudioCaptureUninstallKey "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Screen Capturer Recorder_is1"
!define AudioCaptureUninstallKeyWow64 "SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Screen Capturer Recorder_is1"
!define MyAppUninstallKey "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MyAppName}"

; --------------------------------
; 2. General Settings
; --------------------------------
Name "${MyAppName} ${MyAppVersion}"
BrandingText "${MyAppName}"
OutFile "InstantReplaySetup.exe"
InstallDir "$PROGRAMFILES64\${MyAppName}"
RequestExecutionLevel admin
ShowInstDetails show
ShowUninstDetails show

!define MUI_ICON "..\icon.ico"
!define MUI_UNICON "..\icon.ico"

; --------------------------------
; Variables
; --------------------------------
Var AppInstalled
Var FFmpegInstalled
Var FFmpegInPath
Var VACInstalled
Var AllInstalled

Var CheckboxRemoveRecordings
Var CheckboxRemoveDeps
Var RemoveRecordingsState
Var RemoveDepsState

; --------------------------------
; 3. Callbacks
; --------------------------------
Function SkipIfInstalled
    ${If} $AllInstalled == "1"
        Abort
    ${EndIf}
FunctionEnd

Function OnRecordingsClick
    ${NSD_GetState} $CheckboxRemoveRecordings $RemoveRecordingsState
FunctionEnd

Function OnDepsClick
    ${NSD_GetState} $CheckboxRemoveDeps $RemoveDepsState
FunctionEnd

Function UninstallPageShow
    ${If} $AllInstalled == "0"
        Abort
    ${EndIf}

    SendMessage $HWNDPARENT ${WM_SETTEXT} 0 "STR:${MyAppName} - Uninstall"
    GetDlgItem $0 $HWNDPARENT 1
    SendMessage $0 ${WM_SETTEXT} 0 "STR:Uninstall"

    nsDialogs::Create 1018
    Pop $0

    ${NSD_CreateLabel} 0 0 100% 20u "Instant Replay is already installed. Select what to remove:"
    Pop $0

    ${NSD_CreateCheckbox} 0 30u 100% 15u "Remove All Recordings"
    Pop $CheckboxRemoveRecordings
    ${NSD_OnClick} $CheckboxRemoveRecordings OnRecordingsClick

    ${NSD_CreateCheckbox} 0 50u 100% 15u "Remove All Dependencies (FFmpeg, Virtual Audio Capturer, PATH)"
    Pop $CheckboxRemoveDeps
    ${NSD_OnClick} $CheckboxRemoveDeps OnDepsClick
    ${NSD_Check} $CheckboxRemoveDeps
    StrCpy $RemoveDepsState ${BST_CHECKED}
    StrCpy $RemoveRecordingsState ${BST_UNCHECKED}

    nsDialogs::Show
FunctionEnd

Function UninstallPageLeave
    ${If} $AllInstalled == "0"
        Abort
    ${EndIf}

    MessageBox MB_YESNO|MB_ICONQUESTION "Are you sure you want to uninstall ${MyAppName}?" IDYES ProceedUninstall
    Quit

    ProceedUninstall:
FunctionEnd

Function InstFilesShow
    ${If} $AllInstalled == "1"
        SendMessage $HWNDPARENT ${WM_SETTEXT} 0 "STR:${MyAppName} - Uninstalling"
    ${EndIf}
FunctionEnd

Function FinishShow
    ${If} $AllInstalled == "1"
        SendMessage $HWNDPARENT ${WM_SETTEXT} 0 "STR:${MyAppName} - Uninstall Complete"
    ${EndIf}
FunctionEnd

; --------------------------------
; 4. Interface Pages
; --------------------------------
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfInstalled
!insertmacro MUI_PAGE_WELCOME
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfInstalled
!insertmacro MUI_PAGE_LICENSE "terms.txt"
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfInstalled
!insertmacro MUI_PAGE_COMPONENTS
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfInstalled
!insertmacro MUI_PAGE_DIRECTORY

Page custom UninstallPageShow UninstallPageLeave

!define MUI_PAGE_CUSTOMFUNCTION_SHOW InstFilesShow
!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\${MyAppExeName}"
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfInstalled
!define MUI_PAGE_CUSTOMFUNCTION_SHOW FinishShow
!insertmacro MUI_PAGE_FINISH

; Uninstall pages
!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

; --------------------------------
; 5. Detection
; --------------------------------
Function .onInit
    InitPluginsDir

    StrCpy $AppInstalled "0"
    ReadRegStr $0 HKLM "${MyAppUninstallKey}" "UninstallString"
    StrCmp $0 "" +2
    StrCpy $AppInstalled "1"

    StrCpy $FFmpegInstalled "0"
    IfFileExists "${FFmpegBinDir}\ffmpeg.exe" 0 +2
    StrCpy $FFmpegInstalled "1"

    StrCpy $FFmpegInPath "0"
    FileOpen $4 "$PLUGINSDIR\check_path.ps1" w
    FileWrite $4 "$$found = [bool]($$env:Path -split ';' | Where-Object { $$_ -like '*${FFmpegBinDir}*' })$\r$\n"
    FileWrite $4 "if ($$found) { exit 0 } else { exit 1 }$\r$\n"
    FileClose $4
    nsExec::ExecToStack 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\check_path.ps1"'
    Pop $0
    Pop $1
    ${If} $0 == "0"
        StrCpy $FFmpegInPath "1"
    ${EndIf}

    StrCpy $VACInstalled "0"
    ReadRegStr $0 HKLM "${AudioCaptureUninstallKey}" "UninstallString"
    StrCmp $0 "" 0 +3
    ReadRegStr $0 HKLM "${AudioCaptureUninstallKeyWow64}" "UninstallString"
    StrCmp $0 "" +2
    StrCpy $VACInstalled "1"

    StrCpy $AllInstalled "0"
    ${If} $AppInstalled == "1"
    ${AndIf} $FFmpegInstalled == "1"
    ${AndIf} $FFmpegInPath == "1"
    ${AndIf} $VACInstalled == "1"
        StrCpy $AllInstalled "1"
    ${EndIf}

    ${If} $AllInstalled == "1"
        DetailPrint "[+] All components detected - uninstaller mode"
    ${Else}
        DetailPrint "[*] Components missing - installer mode"
    ${EndIf}
FunctionEnd

; --------------------------------
; 6. Installation Sections
; --------------------------------

Section "Instant Replay (Required)" SecMain
    SectionIn RO

    ${If} $AllInstalled == "1"
        DetailPrint "[*] Running uninstall..."

        DetailPrint "[*] Killing running processes..."
        nsExec::ExecToStack 'taskkill /F /IM ${MyAppExeName}'
        Pop $0
        Pop $1
        nsExec::ExecToStack 'taskkill /F /IM ffmpeg.exe'
        Pop $0
        Pop $1

        DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "${MyAppName}"
        DeleteRegKey HKLM "Software\Classes\${MyAppAssocExt}\OpenWithProgids"
        DeleteRegKey HKLM "Software\Classes\${MyAppAssocKey}"
        DeleteRegKey HKLM "${MyAppUninstallKey}"

        ${If} $RemoveRecordingsState == ${BST_CHECKED}
            DetailPrint "[*] Removing recordings..."
            IfFileExists "$APPDATA\InstantReplay\settings.json" 0 TryDefaultRecordings
            FileOpen $4 "$PLUGINSDIR\remove_recordings.ps1" w
            FileWrite $4 "$$settings = Get-Content -Path '$APPDATA\InstantReplay\settings.json' -Raw | ConvertFrom-Json$\r$\n"
            FileWrite $4 "$$dest = $$settings.SaveDestination$\r$\n"
            FileWrite $4 "if ([string]::IsNullOrWhiteSpace($$dest)) { $$dest = Join-Path ([Environment]::GetFolderPath('MyPictures')) 'Instant Replay' }$\r$\n"
            FileWrite $4 "if (Test-Path $$dest) { Remove-Item -Path $$dest -Recurse -Force }$\r$\n"
            FileClose $4
            nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\remove_recordings.ps1"'
            Goto DoneRecordings
            TryDefaultRecordings:
            IfFileExists "$PROFILE\Pictures\Instant Replay" 0 DoneRecordings
            RMDir /r "$PROFILE\Pictures\Instant Replay"
            DoneRecordings:
        ${EndIf}

        ${If} $RemoveDepsState == ${BST_CHECKED}
            DetailPrint "[*] Removing dependencies..."

            StrCpy $1 ""
            ReadRegStr $1 HKLM "${AudioCaptureUninstallKey}" "UninstallString"
            ${If} $1 == ""
                ReadRegStr $1 HKLM "${AudioCaptureUninstallKeyWow64}" "UninstallString"
            ${EndIf}
            ${If} $1 != ""
                DetailPrint "[*] Uninstalling Virtual Audio Capturer..."
                nsExec::ExecToLog '$1 /VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
            ${EndIf}

            DetailPrint "[*] Removing FFmpeg..."
            RMDir /r "${FFmpegBinDir}"

            DetailPrint "[*] Removing FFmpeg from system PATH..."
            FileOpen $4 "$PLUGINSDIR\remove_path.ps1" w
            FileWrite $4 "$$p = (Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment').Path$\r$\n"
            FileWrite $4 "$$p = $$p -replace '(?i);?C:\\ffmpeg\\?', ''$\r$\n"
            FileWrite $4 "Set-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment' -Name 'Path' -Value $$p$\r$\n"
            FileClose $4
            nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\remove_path.ps1"'
            SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
        ${EndIf}

        DetailPrint "[*] Removing ${MyAppName}..."
        RMDir /r "$INSTDIR"
        Delete "$SMPROGRAMS\${MyAppName}.lnk"
        Delete "$DESKTOP\${MyAppName}.lnk"
        DetailPrint "[+] ${MyAppName} uninstalled successfully."
        MessageBox MB_OK "Instant Replay has been successfully removed from your computer."
        Return
    ${EndIf}

    DetailPrint "[*] Installing Instant Replay..."
    SetOutPath "$INSTDIR"
    File /r "..\publish\*"

    WriteRegStr HKLM "Software\Classes\${MyAppAssocExt}\OpenWithProgids" "${MyAppAssocKey}" ""
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}" "" "${MyAppAssocName}"
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}\DefaultIcon" "" "$INSTDIR\${MyAppExeName},0"
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}\shell\open\command" "" '"$INSTDIR\${MyAppExeName}" "%1"'
    WriteRegStr HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "${MyAppName}" '"$INSTDIR\${MyAppExeName}" --tray'
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    WriteRegStr HKLM "${MyAppUninstallKey}" "DisplayName" "${MyAppName}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
    WriteRegStr HKLM "${MyAppUninstallKey}" "Publisher" "${MyAppPublisher}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "DisplayVersion" "${MyAppVersion}"
SectionEnd

Section "FFmpeg (Required)" SecFFmpeg
    SectionIn RO

    ${If} $AllInstalled == "1"
        Return
    ${EndIf}

    ${If} $FFmpegInstalled == "1"
        DetailPrint "[+] FFmpeg already installed, skipping..."
        Goto SkipFFmpeg
    ${EndIf}

    DetailPrint "[*] Downloading FFmpeg..."
    NScurl::http GET "${FFmpegDownloadUrl}" "$PLUGINSDIR\ffmpeg.zip" /END
    Pop $0
    StrCmp $0 "OK" DownloadSuccessFFmpeg
        MessageBox MB_OK|MB_ICONSTOP "Failed to download FFmpeg. Error: $0"
        Abort "Installation aborted due to download failure."
    DownloadSuccessFFmpeg:

    DetailPrint "[*] Extracting FFmpeg..."
    FileOpen $4 "$PLUGINSDIR\extract_ffmpeg.ps1" w
    FileWrite $4 "New-Item -ItemType Directory -Force -Path '${FFmpegBinDir}' | Out-Null$\r$\n"
    FileWrite $4 "Expand-Archive -Path '$PLUGINSDIR\ffmpeg.zip' -DestinationPath '$PLUGINSDIR\ffmpeg_extract' -Force$\r$\n"
    FileWrite $4 "$$exe = Get-ChildItem -Path '$PLUGINSDIR\ffmpeg_extract' -Filter 'ffmpeg.exe' -Recurse | Select-Object -First 1 -ExpandProperty FullName$\r$\n"
    FileWrite $4 "if ($$exe) { Copy-Item $$exe '${FFmpegBinDir}\ffmpeg.exe' -Force }$\r$\n"
    FileWrite $4 "Remove-Item -Path '$PLUGINSDIR\ffmpeg_extract' -Recurse -Force -ErrorAction SilentlyContinue$\r$\n"
    FileWrite $4 "Remove-Item -Path '$PLUGINSDIR\ffmpeg.zip' -Force -ErrorAction SilentlyContinue$\r$\n"
    FileClose $4
    nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\extract_ffmpeg.ps1"'
    Pop $0
    StrCmp $0 "0" ExtractSuccessFFmpeg
        MessageBox MB_OK|MB_ICONSTOP "Failed to extract FFmpeg. (Exit Code: $0)"
        Abort "Installation aborted due to extraction failure."
    ExtractSuccessFFmpeg:

    SkipFFmpeg:
    ${If} $FFmpegInPath == "0"
        DetailPrint "[*] Adding FFmpeg to system PATH..."
        FileOpen $4 "$PLUGINSDIR\set_path.ps1" w
        FileWrite $4 "$$p = (Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment').Path$\r$\n"
        FileWrite $4 "if ($$p -notlike '*${FFmpegBinDir}*') { Set-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment' -Name 'Path' -Value ($$p + ';${FFmpegBinDir}') }$\r$\n"
        FileClose $4
        nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\set_path.ps1"'
        SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
        DetailPrint "[+] FFmpeg added to PATH"
    ${Else}
        DetailPrint "[+] FFmpeg already in PATH, skipping..."
    ${EndIf}

    DetailPrint "[+] FFmpeg setup complete."
SectionEnd

Section "Virtual Audio Capturer (Required)" SecVAC
    SectionIn RO

    ${If} $AllInstalled == "1"
        Return
    ${EndIf}

    ${If} $VACInstalled == "1"
        DetailPrint "[+] Virtual Audio Capturer already installed, skipping..."
        Return
    ${EndIf}

    DetailPrint "[*] Downloading Virtual Audio Capturer..."
    NScurl::http GET "${AudioCaptureDownloadUrl}" "$PLUGINSDIR\audio-capture-setup.exe" /END
    Pop $0
    StrCmp $0 "OK" DownloadSuccessVAC
        MessageBox MB_OK|MB_ICONSTOP "Failed to download Virtual Audio Capturer. Error: $0"
        Abort "Installation aborted due to download failure."
    DownloadSuccessVAC:

    DetailPrint "[*] Installing Virtual Audio Capturer..."
    nsExec::ExecToStack '"$PLUGINSDIR\audio-capture-setup.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-'
    Pop $0
    Pop $1
    StrCmp $0 "0" InstallSuccessVAC
        MessageBox MB_OK|MB_ICONSTOP "Failed to install Virtual Audio Capturer. (Exit Code: $0)"
        Abort "Installation aborted due to VAC failure."
    InstallSuccessVAC:

    DetailPrint "[+] Virtual Audio Capturer setup complete."
SectionEnd

Section "Start Menu Shortcut" SecStartMenu
    ${If} $AllInstalled == "0"
        CreateShortcut "$SMPROGRAMS\${MyAppName}.lnk" "$INSTDIR\${MyAppExeName}"
    ${EndIf}
SectionEnd

Section "Desktop Shortcut" SecDesktop
    ${If} $AllInstalled == "0"
        CreateShortcut "$DESKTOP\${MyAppName}.lnk" "$INSTDIR\${MyAppExeName}"
    ${EndIf}
SectionEnd

Section "-PostInstall"
    ${If} $AllInstalled == "0"
        ${If} $AppInstalled == "0"
            DetailPrint "[*] Opening Instant Replay GitHub page..."
            ExecShell "open" "${GitHubUrl}"
        ${EndIf}
    ${EndIf}
SectionEnd

; --------------------------------
; 7. Uninstaller Section
; --------------------------------
Section "Uninstall"
    DetailPrint "[*] Killing running processes..."
    nsExec::ExecToStack 'taskkill /F /IM ${MyAppExeName}'
    Pop $0
    Pop $1
    nsExec::ExecToStack 'taskkill /F /IM ffmpeg.exe'
    Pop $0
    Pop $1

    DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "${MyAppName}"
    DeleteRegKey HKLM "Software\Classes\${MyAppAssocExt}\OpenWithProgids"
    DeleteRegKey HKLM "Software\Classes\${MyAppAssocKey}"
    DeleteRegKey HKLM "${MyAppUninstallKey}"

    ${If} $RemoveRecordingsState == ${BST_CHECKED}
        DetailPrint "[*] Removing recordings..."
        IfFileExists "$APPDATA\InstantReplay\settings.json" 0 TryDefaultRecordingsU
        FileOpen $4 "$PLUGINSDIR\remove_recordings.ps1" w
        FileWrite $4 "$$settings = Get-Content -Path '$APPDATA\InstantReplay\settings.json' -Raw | ConvertFrom-Json$\r$\n"
        FileWrite $4 "$$dest = $$settings.SaveDestination$\r$\n"
        FileWrite $4 "if ([string]::IsNullOrWhiteSpace($$dest)) { $$dest = Join-Path ([Environment]::GetFolderPath('MyPictures')) 'Instant Replay' }$\r$\n"
        FileWrite $4 "if (Test-Path $$dest) { Remove-Item -Path $$dest -Recurse -Force }$\r$\n"
        FileClose $4
        nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\remove_recordings.ps1"'
        Goto DoneRecordingsU
        TryDefaultRecordingsU:
        IfFileExists "$PROFILE\Pictures\Instant Replay" 0 DoneRecordingsU
        RMDir /r "$PROFILE\Pictures\Instant Replay"
        DoneRecordingsU:
    ${EndIf}

    ${If} $RemoveDepsState == ${BST_CHECKED}
        DetailPrint "[*] Removing dependencies..."

        StrCpy $1 ""
        ReadRegStr $1 HKLM "${AudioCaptureUninstallKey}" "UninstallString"
        ${If} $1 == ""
            ReadRegStr $1 HKLM "${AudioCaptureUninstallKeyWow64}" "UninstallString"
        ${EndIf}
        ${If} $1 != ""
            DetailPrint "[*] Uninstalling Virtual Audio Capturer..."
            nsExec::ExecToLog '$1 /VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
        ${EndIf}

        DetailPrint "[*] Removing FFmpeg..."
        RMDir /r "${FFmpegBinDir}"

        DetailPrint "[*] Removing FFmpeg from system PATH..."
        FileOpen $4 "$PLUGINSDIR\remove_path.ps1" w
        FileWrite $4 "$$p = (Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment').Path$\r$\n"
        FileWrite $4 "$$p = $$p -replace '(?i);?C:\\ffmpeg\\?', ''$\r$\n"
        FileWrite $4 "Set-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment' -Name 'Path' -Value $$p$\r$\n"
        FileClose $4
        nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\remove_path.ps1"'
        SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
    ${EndIf}

    RMDir /r "$INSTDIR"
    Delete "$SMPROGRAMS\${MyAppName}.lnk"
    Delete "$DESKTOP\${MyAppName}.lnk"
SectionEnd
