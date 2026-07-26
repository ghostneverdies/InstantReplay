; ==========================================================================
;  Rollback.nsh - Simple hardcoded rollback for Instant Replay installer
; ==========================================================================
;  On .onInstFailed, runs the same cleanup as the uninstaller.
;  No log files, no parsing — just knows what to undo.

!ifndef ROLLBACK_NSH
!define ROLLBACK_NSH

!define RbRollbackFailed '!insertmacro _RbRollbackFailed'

!macro _RbRollbackFailed
    DetailPrint "[!] Installation failed - rolling back..."
    DetailPrint "[+] Stopping any running processes..."
    nsExec::ExecToStack 'taskkill /F /IM ${MyAppExeName}'
    Pop $0
    Pop $1
    nsExec::ExecToStack 'taskkill /F /IM ffmpeg.exe'
    Pop $0
    Pop $1

    DetailPrint "[+] Removing installed files..."
    RMDir /r "$INSTDIR"

    DetailPrint "[+] Removing shortcuts..."
    Delete "$SMPROGRAMS\${MyAppName}.lnk"
    Delete "$DESKTOP\${MyAppName}.lnk"

    DetailPrint "[+] Removing registry entries..."
    DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "${MyAppName}"
    DeleteRegValue HKCU "SOFTWARE\Microsoft\Windows\CurrentVersion\Run" "InstantReplay"
    DeleteRegKey HKLM "Software\Classes\${MyAppAssocExt}\OpenWithProgids"
    DeleteRegKey HKLM "Software\Classes\${MyAppAssocKey}"
    DeleteRegKey HKLM "${MyAppUninstallKey}"

    DetailPrint "[+] Removing FFmpeg..."
    RMDir /r "${FFmpegBinDir}"
    Push "${FFmpegBinDir}"
    Call RbRemoveFromPath

    DetailPrint "[!] Rollback complete."
!macroend

Function RbRemoveFromPath
    Pop $0
    ReadRegStr $1 HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path"
    StrLen $2 $0
    StrLen $3 $1
    StrCpy $4 0
    ${DoWhile} $4 < $3
        StrCpy $5 $1 $2 $4
        ${If} $5 == $0
            ${If} $4 > 0
                IntOp $6 $4 - 1
                StrCpy $5 $1 1 $6
                ${If} $5 == ";"
                    StrCpy $5 $1 $6
                    IntOp $6 $4 + $2
                    StrCpy $6 $1 "" $6
                    StrCpy $1 "$5$6"
                    WriteRegStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path" $1
                    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
                    Return
                ${EndIf}
            ${EndIf}
            IntOp $6 $4 + $2
            StrCpy $5 $1 1 $6
            ${If} $5 == ";"
                StrCpy $5 $1 $4
                IntOp $6 $6 + 1
                StrCpy $6 $1 "" $6
                StrCpy $1 "$5$6"
                WriteRegStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path" $1
                SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
                Return
            ${EndIf}
            StrCpy $5 $1 $4
            IntOp $6 $4 + $2
            StrCpy $6 $1 "" $6
            StrCpy $1 "$5$6"
            WriteRegStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path" $1
            SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
            Return
        ${EndIf}
        IntOp $4 $4 + 1
    ${Loop}
FunctionEnd

!endif
