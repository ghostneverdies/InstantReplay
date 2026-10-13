SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"

!include "FileFunc.nsh"

!define MyAppName "Instant Replay"
!define MyAppVersion "1.5"
!define MyAppPublisher "InstantReplay"
!define MyAppExeName "InstantReplay.exe"
!define MyAppAssocName "${MyAppName} File"
!define MyAppAssocExt ".ir"
!define MyAppAssocKey "InstantReplayFile"
!define GitHubUrl "https://github.com/ghostneverdies/InstantReplay"

VIProductVersion "1.5.0.0"
VIAddVersionKey /LANG=1033 "ProductName" "${MyAppName}"
VIAddVersionKey /LANG=1033 "ProductVersion" "${MyAppVersion}"
VIAddVersionKey /LANG=1033 "FileVersion" "1.5.0.0"
VIAddVersionKey /LANG=1033 "FileDescription" "${MyAppName} installer"
VIAddVersionKey /LANG=1033 "CompanyName" "${MyAppPublisher}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "Copyright (c) 2026 GHO5T"


!define MyAppUninstallKey "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MyAppName}"

Name "${MyAppName} ${MyAppVersion}"
BrandingText "${MyAppName}"
OutFile "InstantReplaySetup.exe"
InstallDir "$PROGRAMFILES64\${MyAppName}"
RequestExecutionLevel admin
ShowInstDetails show
ShowUninstDetails show

!define MUI_ICON "..\assets\icons\icon.ico"
!define MUI_UNICON "..\assets\icons\icon.ico"

Var UpdateMode

Var CheckboxRemoveRecordings
Var RemoveRecordingsState

Function un.OnRecordingsClick
    ${NSD_GetState} $CheckboxRemoveRecordings $RemoveRecordingsState
FunctionEnd

Function un.UninstallOptionsPageShow
    nsDialogs::Create 1018
    Pop $0

    ${NSD_CreateLabel} 0 0 100% 20u "Select what to remove along with ${MyAppName}:"
    Pop $0

    ${NSD_CreateCheckbox} 0 30u 100% 15u "Remove All Recordings"
    Pop $CheckboxRemoveRecordings
    ${NSD_OnClick} $CheckboxRemoveRecordings un.OnRecordingsClick
    StrCpy $RemoveRecordingsState ${BST_UNCHECKED}

    nsDialogs::Show
FunctionEnd

Function un.UninstallOptionsPageLeave
FunctionEnd

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "terms.txt"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\${MyAppExeName}"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
UninstPage custom un.UninstallOptionsPageShow un.UninstallOptionsPageLeave
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

Function .onInit
    ${GetParameters} $R0
    ClearErrors
    ${GetOptions} $R0 "-quiet" $0
    ${IfNot} ${Errors}
        SetSilent silent
    ${EndIf}
    StrCpy $UpdateMode "0"
    ClearErrors
    ${GetOptions} $R0 "/UPDATE=" $0
    ${IfNot} ${Errors}
        ${If} $0 == "1"
            StrCpy $UpdateMode "1"
            SetSilent silent
        ${EndIf}
    ${EndIf}
FunctionEnd

Function .onInstSuccess
    ${If} ${Silent}
        ${If} $UpdateMode != "1"
            ExecShell "open" "$INSTDIR\${MyAppExeName}"
        ${EndIf}
    ${EndIf}
FunctionEnd

Section "Instant Replay (Required)" SecMain
    SectionIn RO

    DetailPrint "[+] Installing Instant Replay..."
    SetOutPath "$INSTDIR"
    File /r "..\bin\publish\win-x64\*"

    WriteRegStr HKLM "Software\Classes\${MyAppAssocExt}\OpenWithProgids" "${MyAppAssocKey}" ""
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}" "" "${MyAppAssocName}"
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}\DefaultIcon" "" "$INSTDIR\${MyAppExeName},0"
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}\shell\open\command" "" '"$INSTDIR\${MyAppExeName}" "%1"'
    ${If} $UpdateMode != "1"
        DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "InstantReplay"
        WriteRegStr HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "${MyAppName}" '"$INSTDIR\${MyAppExeName}" --tray'
    ${EndIf}
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    WriteRegStr HKLM "${MyAppUninstallKey}" "DisplayName" "${MyAppName}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
    WriteRegStr HKLM "${MyAppUninstallKey}" "Publisher" "${MyAppPublisher}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "DisplayVersion" "${MyAppVersion}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "InstallLocation" "$INSTDIR"
SectionEnd

Section "Start Menu Shortcut" SecStartMenu
    ${If} $UpdateMode != "1"
        CreateShortcut "$SMPROGRAMS\${MyAppName}.lnk" "$INSTDIR\${MyAppExeName}"
    ${EndIf}
SectionEnd

Section "Desktop Shortcut" SecDesktop
    ${If} $UpdateMode != "1"
        CreateShortcut "$DESKTOP\${MyAppName}.lnk" "$INSTDIR\${MyAppExeName}"
    ${EndIf}
SectionEnd

Section "-PostInstall"
    ${IfNot} ${Silent}
        DetailPrint "[+] Opening Instant Replay GitHub page..."
        ExecShell "open" "${GitHubUrl}"
    ${EndIf}
SectionEnd

Section "Uninstall"
    DetailPrint "[+] Killing running processes..."
    nsExec::ExecToStack 'taskkill /F /IM ${MyAppExeName}'
    Pop $0
    Pop $1

    DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "${MyAppName}"
    DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "InstantReplay"
    DeleteRegKey HKLM "Software\Classes\${MyAppAssocExt}\OpenWithProgids"
    DeleteRegKey HKLM "Software\Classes\${MyAppAssocKey}"
    DeleteRegKey HKLM "${MyAppUninstallKey}"

    ${If} $RemoveRecordingsState == ${BST_CHECKED}
        DetailPrint "[+] Removing recordings..."
        IfFileExists "$PROFILE\Pictures\Instant Replay" 0 DoneRecordingsU
        DetailPrint "[+] Removing recordings from default location..."
        RMDir /r "$PROFILE\Pictures\Instant Replay"
        DoneRecordingsU:
    ${EndIf}

    RMDir /r "$INSTDIR"
    Delete "$SMPROGRAMS\${MyAppName}.lnk"
    Delete "$DESKTOP\${MyAppName}.lnk"
SectionEnd
