SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"

!include "Rollback.nsh"

!define MyAppName "Instant Replay"
!define MyAppVersion "1.4"
!define MyAppPublisher "InstantReplay"
!define MyAppExeName "InstantReplay.exe"
!define MyAppAssocName "${MyAppName} File"
!define MyAppAssocExt ".ir"
!define MyAppAssocKey "InstantReplayFile"
!define GitHubUrl "https://github.com/ghostneverdies/InstantReplay"

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

Var AppInstalled

Var CheckboxRemoveRecordings
Var RemoveRecordingsState

Function SkipIfInstalled
    ${If} $AppInstalled == "1"
        Abort
    ${EndIf}
FunctionEnd

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

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
UninstPage custom un.UninstallOptionsPageShow un.UninstallOptionsPageLeave
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

Function .onInit
    InitPluginsDir

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
FunctionEnd

Function .onInstFailed
    ${RbRollbackFailed}
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
    DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "InstantReplay"
    WriteRegStr HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "${MyAppName}" '"$INSTDIR\${MyAppExeName}" --tray'
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    WriteRegStr HKLM "${MyAppUninstallKey}" "DisplayName" "${MyAppName}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
    WriteRegStr HKLM "${MyAppUninstallKey}" "Publisher" "${MyAppPublisher}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "DisplayVersion" "${MyAppVersion}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "InstallLocation" "$INSTDIR"
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
