SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"

!include "FileFunc.nsh"

!define MyAppName "Instant Replay"

!ifndef MyAppVersion
    !define MyAppVersion "1.6"
!endif

!ifndef MyAppVersion4
    !define MyAppVersion4 "1.6.0.0"
!endif

!define MyAppPublisher "InstantReplay"
!define MyAppExeName "InstantReplay.exe"
; The app payload lives in <install dir>\ship. The install root only holds the
; InstantReplay.lnk launcher, the uninstaller and that folder, so the Start
; menu / Explorer view stays clean while WinUI keeps every file next to the exe.
!define MyAppFolder "ship"
!define MyAppLauncher "InstantReplay.lnk"
!define MyAppAssocName "${MyAppName} File"
!define MyAppAssocExt ".ir"
!define MyAppAssocKey "InstantReplayFile"
!define GitHubUrl "https://github.com/ghostneverdies/InstantReplay"

VIProductVersion "${MyAppVersion4}"
VIAddVersionKey /LANG=1033 "ProductName" "${MyAppName}"
VIAddVersionKey /LANG=1033 "ProductVersion" "${MyAppVersion}"
VIAddVersionKey /LANG=1033 "FileVersion" "${MyAppVersion4}"
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
Var AppDir
Var RootDir
Var LegacyLayout

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
!define MUI_FINISHPAGE_RUN "$INSTDIR\${MyAppFolder}\${MyAppExeName}"
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
    StrCpy $LegacyLayout "0"
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
            ExecShell "open" "$AppDir\${MyAppExeName}"
        ${ElseIf} $LegacyLayout == "1"
            ; A flat pre-1.7 install updated itself: the old exe is gone, so the
            ; non-elevated update helper cannot restart the app and this does it.
            ExecShell "open" "$AppDir\${MyAppExeName}"
        ${EndIf}
    ${EndIf}
FunctionEnd

Section "Instant Replay (Required)" SecMain
    SectionIn RO

    ; $INSTDIR is the installation root, except when the updater passes the
    ; folder that currently holds the executable: either <root>\ship (current
    ; layout) or <root> itself (legacy flat layout before the ship folder).
    StrCpy $RootDir "$INSTDIR"
    StrCpy $AppDir "$INSTDIR\${MyAppFolder}"
    ${GetFileName} "$INSTDIR" $1
    ${If} $1 == "${MyAppFolder}"
        ; The updater passes the app folder, and a manual install may select it:
        ; install in place and use its parent as the root.
        ${If} $UpdateMode == "1"
        ${OrIf} ${FileExists} "$INSTDIR\${MyAppExeName}"
            StrCpy $AppDir "$INSTDIR"
            ${GetParent} "$INSTDIR" $RootDir
        ${EndIf}
    ${ElseIf} ${FileExists} "$INSTDIR\${MyAppExeName}"
        ; A flat install: the exe sits in the root instead of root\ship.
        StrCpy $LegacyLayout "1"
    ${EndIf}

    DetailPrint "[+] Installing Instant Replay..."
    SetOutPath "$AppDir"
    File /r "..\bin\publish\win-x64\*"

    ; The only thing at the install root is a launcher for the payload in ship\.
    CreateShortcut "$RootDir\${MyAppLauncher}" "$AppDir\${MyAppExeName}" "" "$AppDir\${MyAppExeName}" 0

    ${If} $LegacyLayout == "1"
        ; One-time migration from the flat layout. Runs after the new payload is
        ; in place, and only removes entries written by the old installers, so a
        ; failed copy cannot leave the user without a working app.
        DetailPrint "[+] Removing the previous flat installation..."
        Delete "$INSTDIR\*.dll"
        Delete "$INSTDIR\*.exe"
        Delete "$INSTDIR\*.json"
        Delete "$INSTDIR\*.pdb"
        Delete "$INSTDIR\*.winmd"
        Delete "$INSTDIR\*.pri"
        Delete "$INSTDIR\*.xbf"
        Delete "$INSTDIR\*.mui"
        RMDir /r "$INSTDIR\engine"
        RMDir /r "$INSTDIR\assets"
        RMDir /r "$INSTDIR\mediaplayer"
        RMDir /r "$INSTDIR\meta"
        RMDir /r "$INSTDIR\symbols"
        RMDir /r "$INSTDIR\Themes"
        RMDir /r "$INSTDIR\Microsoft.UI.Xaml"
        ; WinUI locale folders only hold .mui resources, so this is safe.
        FindFirst $0 $1 "$INSTDIR\*.*"
        ShipLocaleLoop:
            StrCmp $1 "" ShipLocaleDone
            StrCmp $1 "." ShipLocaleNext
            StrCmp $1 ".." ShipLocaleNext
            StrCmp $1 "${MyAppFolder}" ShipLocaleNext
            IfFileExists "$INSTDIR\$1\*.mui" 0 ShipLocaleNext
            RMDir /r "$INSTDIR\$1"
            ShipLocaleNext:
            FindNext $0 $1
            Goto ShipLocaleLoop
        ShipLocaleDone:
        FindClose $0
    ${EndIf}

    WriteRegStr HKLM "Software\Classes\${MyAppAssocExt}\OpenWithProgids" "${MyAppAssocKey}" ""
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}" "" "${MyAppAssocName}"
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}\DefaultIcon" "" "$AppDir\${MyAppExeName},0"
    WriteRegStr HKLM "Software\Classes\${MyAppAssocKey}\shell\open\command" "" '"$AppDir\${MyAppExeName}" "%1"'

    ${If} $UpdateMode != "1"
        DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "InstantReplay"
        WriteRegStr HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "${MyAppName}" '"$AppDir\${MyAppExeName}" --tray'
    ${ElseIf} $LegacyLayout == "1"
        ; The old startup entry pointed at the exe that was just removed.
        WriteRegStr HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "${MyAppName}" '"$AppDir\${MyAppExeName}" --tray'
    ${EndIf}

    WriteUninstaller "$AppDir\Uninstall.exe"
    WriteRegStr HKLM "${MyAppUninstallKey}" "DisplayName" "${MyAppName}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "UninstallString" '"$AppDir\Uninstall.exe"'
    WriteRegStr HKLM "${MyAppUninstallKey}" "Publisher" "${MyAppPublisher}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "DisplayVersion" "${MyAppVersion}"
    WriteRegStr HKLM "${MyAppUninstallKey}" "InstallLocation" "$RootDir"
SectionEnd

Section "Start Menu Shortcut" SecStartMenu
    ${If} $UpdateMode != "1"
        CreateShortcut "$SMPROGRAMS\${MyAppName}.lnk" "$AppDir\${MyAppExeName}" "" "$AppDir\${MyAppExeName}" 0
    ${ElseIf} ${FileExists} "$SMPROGRAMS\${MyAppName}.lnk"
        ; Refresh the target without recreating a shortcut the user removed.
        CreateShortcut "$SMPROGRAMS\${MyAppName}.lnk" "$AppDir\${MyAppExeName}" "" "$AppDir\${MyAppExeName}" 0
    ${EndIf}
SectionEnd

Section "Desktop Shortcut" SecDesktop
    ${If} $UpdateMode != "1"
        CreateShortcut "$DESKTOP\${MyAppName}.lnk" "$AppDir\${MyAppExeName}" "" "$AppDir\${MyAppExeName}" 0
    ${ElseIf} ${FileExists} "$DESKTOP\${MyAppName}.lnk"
        CreateShortcut "$DESKTOP\${MyAppName}.lnk" "$AppDir\${MyAppExeName}" "" "$AppDir\${MyAppExeName}" 0
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
    nsExec::ExecToStack "taskkill /F /IM ${MyAppExeName}"
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

    ; $INSTDIR is ship\ (it holds the uninstaller); the launcher is one level up.
    Delete "$INSTDIR\..\${MyAppLauncher}"
    RMDir /r "$INSTDIR"
    RMDir "$INSTDIR\.."
    Delete "$SMPROGRAMS\${MyAppName}.lnk"
    Delete "$DESKTOP\${MyAppName}.lnk"
SectionEnd
