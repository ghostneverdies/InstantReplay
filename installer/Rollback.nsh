!ifndef ROLLBACK_NSH
!define ROLLBACK_NSH

!define RbRollbackFailed '!insertmacro _RbRollbackFailed'

!macro _RbRollbackFailed
    DetailPrint "[!] Installation failed - rolling back..."
    DetailPrint "[+] Stopping any running processes..."
    nsExec::ExecToStack 'taskkill /F /IM ${MyAppExeName}'
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

    DetailPrint "[!] Rollback complete."
!macroend

!endif
