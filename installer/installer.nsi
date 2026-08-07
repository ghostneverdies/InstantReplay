; ==========================================================================
;  Instant Replay - NSIS Installer
; ==========================================================================

SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"

!include "Rollback.nsh"

; --------------------------------
; 1. Definitions
; --------------------------------
!define MyAppName "Instant Replay"
!define MyAppVersion "1.3"
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
!define JavaDownloadUrl "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.7%2B6/OpenJDK21U-jdk_x64_windows_hotspot_21.0.7_6.msi"
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
; 3. Variables
; --------------------------------
Var AppInstalled
Var FFmpegInstalled
Var VACInstalled
Var JavaInstalled

Var CheckboxRemoveRecordings
Var CheckboxRemoveDeps
Var RemoveRecordingsState
Var RemoveDepsState

; --------------------------------
; 4. Callbacks
; --------------------------------
Function SkipIfInstalled
    ${If} $AppInstalled == "1"
        Abort
    ${EndIf}
FunctionEnd

Function un.OnRecordingsClick
    ${NSD_GetState} $CheckboxRemoveRecordings $RemoveRecordingsState
FunctionEnd

Function un.OnDepsClick
    ${NSD_GetState} $CheckboxRemoveDeps $RemoveDepsState
FunctionEnd

Function un.UninstallOptionsPageShow
    nsDialogs::Create 1018
    Pop $0

    ${NSD_CreateLabel} 0 0 100% 20u "Select what to remove along with ${MyAppName}:"
    Pop $0

    ${NSD_CreateCheckbox} 0 30u 100% 15u "Remove All Recordings"
    Pop $CheckboxRemoveRecordings
    ${NSD_OnClick} $CheckboxRemoveRecordings un.OnRecordingsClick

    ${NSD_CreateCheckbox} 0 50u 100% 15u "Remove All Dependencies (FFmpeg, Virtual Audio Capturer)"
    Pop $CheckboxRemoveDeps
    ${NSD_OnClick} $CheckboxRemoveDeps un.OnDepsClick
    ${NSD_Check} $CheckboxRemoveDeps
    StrCpy $RemoveDepsState ${BST_CHECKED}
    StrCpy $RemoveRecordingsState ${BST_UNCHECKED}

    nsDialogs::Show
FunctionEnd

Function un.UninstallOptionsPageLeave
FunctionEnd

Function InstFilesShow
    ${If} $AppInstalled == "1"
        SendMessage $HWNDPARENT ${WM_SETTEXT} 0 "STR:${MyAppName} - Uninstalling"
    ${EndIf}
FunctionEnd

Function FinishShow
    ${If} $AppInstalled == "1"
        SendMessage $HWNDPARENT ${WM_SETTEXT} 0 "STR:${MyAppName} - Uninstall Complete"
    ${EndIf}
FunctionEnd

; --------------------------------
; 5. Interface Pages
; --------------------------------
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfInstalled
!insertmacro MUI_PAGE_WELCOME
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfInstalled
!insertmacro MUI_PAGE_LICENSE "terms.txt"
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfInstalled
!insertmacro MUI_PAGE_COMPONENTS
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfInstalled
!insertmacro MUI_PAGE_DIRECTORY
!define MUI_PAGE_CUSTOMFUNCTION_SHOW InstFilesShow
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\${MyAppExeName}"
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfInstalled
!define MUI_PAGE_CUSTOMFUNCTION_SHOW FinishShow
!insertmacro MUI_PAGE_FINISH

; Uninstall pages
!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
UninstPage custom un.UninstallOptionsPageShow un.UninstallOptionsPageLeave
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

; --------------------------------
; 6. Detection (.onInit)
; --------------------------------
Function .onInit
    InitPluginsDir

    ; Clean up leftover NSIS temp folders from previous crashed/aborted runs
    StrCpy $0 ""
    FindFirst $1 $2 "$TEMP\ns*"
    ${DoWhile} $2 != ""
        StrCpy $3 "$TEMP\$2"
        ${If} $3 != $PLUGINSDIR
            RMDir /r "$TEMP\$2"
        ${EndIf}
        FindNext $1 $2
    ${Loop}
    FindClose $1
    Delete "$TEMP\vac-setup.exe"

    StrCpy $AppInstalled "0"
    ReadRegStr $0 HKLM "${MyAppUninstallKey}" "UninstallString"
    ${If} $0 != ""
        StrCpy $AppInstalled "1"
    ${EndIf}
    IfFileExists "$INSTDIR\Uninstall.exe" 0 +2
    StrCpy $AppInstalled "1"

    ${If} $AppInstalled == "1"
        DetailPrint "[+] App detected - launching uninstaller"
        ReadRegStr $0 HKLM "${MyAppUninstallKey}" "UninstallString"
        ${If} $0 == ""
            StrCpy $0 '"$INSTDIR\Uninstall.exe"'
        ${EndIf}
        ExecWait '$0'
        Quit
    ${EndIf}

    StrCpy $FFmpegInstalled "0"
    IfFileExists "${FFmpegBinDir}\ffmpeg.exe" 0 +2
    StrCpy $FFmpegInstalled "1"

    StrCpy $VACInstalled "0"
    ReadRegStr $0 HKLM "${AudioCaptureUninstallKey}" "UninstallString"
    ${If} $0 != ""
        StrCpy $VACInstalled "1"
    ${EndIf}
    ${If} $VACInstalled == "0"
        ReadRegStr $0 HKLM "${AudioCaptureUninstallKeyWow64}" "UninstallString"
        ${If} $0 != ""
            StrCpy $VACInstalled "1"
        ${EndIf}
    ${EndIf}

    StrCpy $JavaInstalled "0"
    ReadRegStr $0 HKLM "SOFTWARE\Eclipse Adoptium" ""
    ${If} $0 != ""
        StrCpy $JavaInstalled "1"
    ${EndIf}
    ${If} $JavaInstalled == "0"
        ReadRegStr $0 HKLM "SOFTWARE\JavaSoft" ""
        ${If} $0 != ""
            StrCpy $JavaInstalled "1"
        ${EndIf}
    ${EndIf}
    ${If} $JavaInstalled == "0"
        ReadRegStr $0 HKCU "SOFTWARE\Eclipse Adoptium" ""
        ${If} $0 != ""
            StrCpy $JavaInstalled "1"
        ${EndIf}
    ${EndIf}
    ${If} $JavaInstalled == "0"
        ReadRegStr $0 HKCU "SOFTWARE\JavaSoft" ""
        ${If} $0 != ""
            StrCpy $JavaInstalled "1"
        ${EndIf}
    ${EndIf}
FunctionEnd

Function .onInstFailed
    ${RbRollbackFailed}
FunctionEnd

; --------------------------------
; 7. Installation Sections
; --------------------------------

Section "Instant Replay (Required)" SecMain
    SectionIn RO

    DetailPrint "[+] Installing Instant Replay..."
    SetOutPath "$INSTDIR"
    File /r "..\publish\*"

    WriteRegStr HKLM "Software\Classes\${MyAppAssocExt}\OpenWithProgids" "${MyAppAssocKey}" ""
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}" "" "${MyAppAssocName}"
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}\DefaultIcon" "" "$INSTDIR\${MyAppExeName},0"
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}\shell\open\command" "" '"$INSTDIR\${MyAppExeName}" "%1"'
    DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "InstantReplay"
    WriteRegStr HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "${MyAppName}" '"$INSTDIR\${MyAppExeName}" --tray'
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    WriteRegStr HKLM "${MyAppUninstallKey}" "DisplayName" "${MyAppName}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
    WriteRegStr HKLM "${MyAppUninstallKey}" "Publisher" "${MyAppPublisher}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "DisplayVersion" "${MyAppVersion}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "InstallLocation" "$INSTDIR"
SectionEnd

Section "FFmpeg (Required)" SecFFmpeg
    SectionIn RO

    ${If} $AppInstalled == "1"
        Return
    ${EndIf}

    ${If} $FFmpegInstalled == "1"
        DetailPrint "[+] FFmpeg already installed, skipping..."
        Return
    ${EndIf}

    DetailPrint "[+] Downloading FFmpeg..."
    NScurl::http GET "${FFmpegDownloadUrl}" "$PLUGINSDIR\ffmpeg.zip" /END
    Pop $0
    StrCmp $0 "OK" DownloadSuccessFFmpeg
        MessageBox MB_OK|MB_ICONSTOP "Failed to download FFmpeg. Error: $0"
        Abort "Installation aborted due to download failure."
    DownloadSuccessFFmpeg:

    DetailPrint "[+] Extracting FFmpeg..."
    CreateDirectory "${FFmpegBinDir}"
    nsExec::ExecToStack 'tar -xf "$PLUGINSDIR\ffmpeg.zip" -C "$PLUGINSDIR"'
    Pop $0
    Pop $1
    StrCmp $0 "0" ExtractSuccessFFmpeg
        MessageBox MB_OK|MB_ICONSTOP "Failed to extract FFmpeg. (Exit Code: $0)"
        Abort "Installation aborted due to extraction failure."
    ExtractSuccessFFmpeg:

    ; Copy ffmpeg.exe from extracted folder to C:\ffmpeg
    IfFileExists "$PLUGINSDIR\ffmpeg-master-latest-win64-gpl\bin\ffmpeg.exe" 0 FFmpegNotFound
        CopyFiles "$PLUGINSDIR\ffmpeg-master-latest-win64-gpl\bin\ffmpeg.exe" "${FFmpegBinDir}\ffmpeg.exe"
        Goto FFmpegExtractDone
    FFmpegNotFound:
        MessageBox MB_OK|MB_ICONSTOP "FFmpeg extraction completed but ffmpeg.exe was not found at the expected path."
        Abort "Installation aborted - ffmpeg.exe not found after extraction."
    FFmpegExtractDone:

    ; Cleanup
    Delete "$PLUGINSDIR\ffmpeg.zip"
    RMDir /r "$PLUGINSDIR\ffmpeg-master-latest-win64-gpl"

    DetailPrint "[+] FFmpeg installed to ${FFmpegBinDir}"
SectionEnd

Section "Virtual Audio Capturer (Required)" SecVAC
    SectionIn RO

    ${If} $AppInstalled == "1"
        Return
    ${EndIf}

    ${If} $VACInstalled == "1"
        DetailPrint "[+] Virtual Audio Capturer already installed, skipping..."
        Return
    ${EndIf}

    DetailPrint "[+] Downloading Virtual Audio Capturer..."
    NScurl::http GET "${AudioCaptureDownloadUrl}" "$PLUGINSDIR\audio-capture-setup.exe" /END
    Pop $0
    StrCmp $0 "OK" DownloadSuccessVAC
        MessageBox MB_OK|MB_ICONSTOP "Failed to download Virtual Audio Capturer. Error: $0"
        Abort "Installation aborted due to download failure."
    DownloadSuccessVAC:

    DetailPrint "[+] Installing Virtual Audio Capturer..."
    ExecWait '"$PLUGINSDIR\audio-capture-setup.exe" /SILENT /SUPPRESSMSGBOXES /FORCECLOSEAPPLICATIONS /NORESTART' $3
    ${If} $3 != "0"
        DetailPrint "[!] VAC installer returned exit code: $3"
        MessageBox MB_OK|MB_ICONSTOP "Failed to install Virtual Audio Capturer. (Exit Code: $3)$\r$\nThe virtual audio driver could not be installed. You may need to install it manually."
        Abort "Installation aborted due to VAC installation failure."
    ${EndIf}
    StrCpy $VACInstalled "1"
    Delete "$PLUGINSDIR\audio-capture-setup.exe"
    DetailPrint "[+] Virtual Audio Capturer setup complete."
SectionEnd

Section "Java Runtime (Required)" SecJava
    SectionIn RO

    ${If} $AppInstalled == "1"
        Return
    ${EndIf}

    ${If} $JavaInstalled == "1"
        DetailPrint "[+] Java Runtime already installed, skipping..."
        Return
    ${EndIf}

    DetailPrint "[+] Downloading Java Runtime..."
    NScurl::http GET "${JavaDownloadUrl}" "$PLUGINSDIR\java-setup.msi" /END
    Pop $0
    StrCmp $0 "OK" DownloadSuccessJava
        MessageBox MB_OK|MB_ICONSTOP "Failed to download Java Runtime. Error: $0"
        Abort "Installation aborted due to download failure."
    DownloadSuccessJava:

    DetailPrint "[+] Installing Java Runtime..."
    nsExec::ExecToStack 'msiexec /i "$PLUGINSDIR\java-setup.msi" /quiet /norestart ADDLOCAL=FeatureMain,FeatureEnvironment,FeatureJarFileRunWith,FeatureJavaHome'
    Pop $0
    Pop $1
    StrCmp $0 "0" InstallSuccessJava
        MessageBox MB_OK|MB_ICONSTOP "Failed to install Java Runtime. (Exit Code: $0)"
        Abort "Installation aborted due to Java installation failure."
    InstallSuccessJava:
    Delete "$PLUGINSDIR\java-setup.msi"

    DetailPrint "[+] Java Runtime setup complete."
SectionEnd

Section "Start Menu Shortcut" SecStartMenu
    ${If} $AppInstalled == "0"
        CreateShortcut "$SMPROGRAMS\${MyAppName}.lnk" "$INSTDIR\${MyAppExeName}"
    ${EndIf}
SectionEnd

Section "Desktop Shortcut" SecDesktop
    ${If} $AppInstalled == "0"
        CreateShortcut "$DESKTOP\${MyAppName}.lnk" "$INSTDIR\${MyAppExeName}"
    ${EndIf}
SectionEnd

Section "-PostInstall"
    ${If} $AppInstalled == "0"
        ${If} $AppInstalled == "0"
            DetailPrint "[+] Opening Instant Replay GitHub page..."
            ExecShell "open" "${GitHubUrl}"
        ${EndIf}
    ${EndIf}
SectionEnd

; --------------------------------
; 8. Uninstaller Section
; --------------------------------
Section "Uninstall"
    DetailPrint "[+] Killing running processes..."
    nsExec::ExecToStack 'taskkill /F /IM ${MyAppExeName}'
    Pop $0
    Pop $1
    nsExec::ExecToStack 'taskkill /F /IM ffmpeg.exe'
    Pop $0
    Pop $1

    DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "${MyAppName}"
    DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "InstantReplay"
    DeleteRegKey HKLM "Software\Classes\${MyAppAssocExt}\OpenWithProgids"
    DeleteRegKey HKLM "Software\Classes\${MyAppAssocKey}"
    DeleteRegKey HKLM "${MyAppUninstallKey}"

    ; --- Remove recordings ---
    ${If} $RemoveRecordingsState == ${BST_CHECKED}
        DetailPrint "[+] Removing recordings..."
        IfFileExists "$PROFILE\Pictures\Instant Replay" 0 DoneRecordingsU
        DetailPrint "[+] Removing recordings from default location..."
        RMDir /r "$PROFILE\Pictures\Instant Replay"
        DoneRecordingsU:
    ${EndIf}

    ; --- Remove dependencies ---
    ${If} $RemoveDepsState == ${BST_CHECKED}
        DetailPrint "[+] Removing dependencies..."

        ; Uninstall Virtual Audio Capturer
        StrCpy $1 ""
        ReadRegStr $1 HKLM "${AudioCaptureUninstallKey}" "UninstallString"
        ${If} $1 == ""
            ReadRegStr $1 HKLM "${AudioCaptureUninstallKeyWow64}" "UninstallString"
        ${EndIf}
        ${If} $1 != ""
            DetailPrint "[+] Uninstalling Virtual Audio Capturer..."
            ; Ensure path is properly quoted for CreateProcess
            StrCpy $5 $1 1
            ${If} $5 == '"'
                StrCpy $5 '$1 /VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
            ${Else}
                StrCpy $5 '"$1" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
            ${EndIf}
            StrCpy $2 0
            ${Do}
                nsExec::ExecToStack '$5'
                Pop $3
                Pop $4
                DetailPrint "[+] VAC uninstall exit code: $3"
                ReadRegStr $0 HKLM "${AudioCaptureUninstallKey}" "UninstallString"
                ${If} $0 == ""
                    ReadRegStr $0 HKLM "${AudioCaptureUninstallKeyWow64}" "UninstallString"
                ${EndIf}
                ${If} $0 == ""
                    DetailPrint "[+] VAC uninstalled successfully."
                    ${ExitDo}
                ${EndIf}
                IntOp $2 $2 + 1
                ${If} $2 >= 3
                    DetailPrint "[!] Failed to remove VAC after 3 attempts, continuing..."
                    ${ExitDo}
                ${EndIf}
                DetailPrint "[!] VAC still present, retrying... (attempt $2/3)"
                Sleep 2000
            ${Loop}
        ${EndIf}

        ; Remove FFmpeg folder
        DetailPrint "[+] Removing FFmpeg..."
        StrCpy $2 0
        ${Do}
            RMDir /r "${FFmpegBinDir}"
            ${If} ${FileExists} "${FFmpegBinDir}"
                IntOp $2 $2 + 1
                ${If} $2 >= 3
                    DetailPrint "[!] Failed to remove FFmpeg folder after 3 attempts, continuing..."
                    ${ExitDo}
                ${EndIf}
                DetailPrint "[!] FFmpeg folder still present, retrying... (attempt $2/3)"
                Sleep 2000
            ${Else}
                DetailPrint "[+] FFmpeg folder removed successfully."
                ${ExitDo}
            ${EndIf}
        ${Loop}
    ${EndIf}

    RMDir /r "$INSTDIR"
    Delete "$SMPROGRAMS\${MyAppName}.lnk"
    Delete "$DESKTOP\${MyAppName}.lnk"
SectionEnd
