; Preserve raw startup values because Tauri's template deletes PRODUCTNAME
; before POSTUNINSTALL. Raw bytes/type preserve foreign REG_EXPAND_SZ/binary
; values too. Never overwrite a value that appeared meanwhile.
Var OnlyMonitorStartupData
Var OnlyMonitorStartupType
Var OnlyMonitorStartupSize
!define MUI_CUSTOMFUNCTION_GUIINIT OnlyMonitorCaptureStartup

!macro OnlyMonitorStartupFunctions PREFIX
Function ${PREFIX}OnlyMonitorCaptureStartup
  Push $0
  Push $1
  Push $2
  Push $3
  StrCpy $OnlyMonitorStartupData ""
  System::Call 'advapi32::RegOpenKeyExW(p 0x80000001, w "Software\Microsoft\Windows\CurrentVersion\Run", i 0, i 0x20019, *p.r0) i.r1'
  ${If} $1 == 0
    System::Call 'advapi32::RegQueryValueExW(p r0, w "Only Monitor", p 0, *i.r2, p 0, *i.r3) i.r1'
    ${If} $1 == 0
      ; Even an empty value needs a non-null buffer.
      IntOp $3 $3 + 2
      System::Alloc $3
      Pop $OnlyMonitorStartupData
      ${If} $OnlyMonitorStartupData == 0
        System::Call 'advapi32::RegCloseKey(p r0)'
        Abort "Cannot preserve startup registration: allocation failed"
      ${EndIf}
      System::Call 'advapi32::RegQueryValueExW(p r0, w "Only Monitor", p 0, *i.r2, p $OnlyMonitorStartupData, *i r3 r3) i.r1'
      ${If} $1 == 0
        StrCpy $OnlyMonitorStartupType $2
        StrCpy $OnlyMonitorStartupSize $3
      ${Else}
        System::Free $OnlyMonitorStartupData
        StrCpy $OnlyMonitorStartupData ""
        System::Call 'advapi32::RegCloseKey(p r0)'
        Abort "Cannot preserve startup registration: read failed"
      ${EndIf}
    ${ElseIf} $1 != 2
      System::Call 'advapi32::RegCloseKey(p r0)'
      Abort "Cannot read startup registration"
    ${EndIf}
    System::Call 'advapi32::RegCloseKey(p r0)'
  ${ElseIf} $1 != 2
    Abort "Cannot open startup registration"
  ${EndIf}
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

Function ${PREFIX}OnlyMonitorRestoreStartup
  ${If} $OnlyMonitorStartupData != ""
    Push $0
    Push $1
    System::Call 'advapi32::RegOpenKeyExW(p 0x80000001, w "Software\Microsoft\Windows\CurrentVersion\Run", i 0, i 0x2001B, *p.r0) i.r1'
    ${If} $1 == 0
      System::Call 'advapi32::RegQueryValueExW(p r0, w "Only Monitor", p 0, p 0, p 0, p 0) i.r1'
      ${If} $1 == 2
        System::Call 'advapi32::RegSetValueExW(p r0, w "Only Monitor", i 0, i $OnlyMonitorStartupType, p $OnlyMonitorStartupData, i $OnlyMonitorStartupSize) i.r1'
      ${EndIf}
      System::Call 'advapi32::RegCloseKey(p r0)'
    ${EndIf}
    System::Free $OnlyMonitorStartupData
    StrCpy $OnlyMonitorStartupData ""
    ${If} $1 != 0
      Abort "Cannot restore startup registration"
    ${EndIf}
    Pop $1
    Pop $0
  ${EndIf}
FunctionEnd
!macroend

!insertmacro OnlyMonitorStartupFunctions ""
!insertmacro OnlyMonitorStartupFunctions "un."

!macro NSIS_HOOK_PREINSTALL
  ${If} $OnlyMonitorStartupData == ""
    Call OnlyMonitorCaptureStartup
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Restores an existing registration only; never enables a previously absent one.
  Call OnlyMonitorRestoreStartup
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode != 1
    Call un.OnlyMonitorCaptureStartup
    Push $R0
    ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Only Monitor"
    ${If} $R0 == '$\"$INSTDIR\only-monitor.exe$\" --autostart'
    ${AndIf} $OnlyMonitorStartupType == 1
      ; Our own exact REG_SZ is intentionally removed by the template.
      System::Free $OnlyMonitorStartupData
      StrCpy $OnlyMonitorStartupData ""
    ${EndIf}
    Pop $R0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode != 1
    Call un.OnlyMonitorRestoreStartup
  ${EndIf}
!macroend