; NSIS installer hooks for Vitals.
;
; The design goal is that installing and updating are not events the user has
; to participate in. No Next button, no licence agreement, no component
; selection, no "Vitals has been installed successfully" page — just a
; progress indicator and then the app.
;
; The wizard pages themselves are suppressed by launching with /S. What these
; hooks do is make a silent install SAFE, which it is not by default: NSIS
; will happily overwrite files that a running process has open, producing an
; installation that is half old and half new.
;
; Every external tool is called by absolute path ($SYSDIR\...): the installer
; runs elevated, and a bare "taskkill" resolves through the current directory
; and PATH first, so a planted taskkill.exe beside the installer would run as
; administrator.

!macro NSIS_HOOK_PREINSTALL
  ; A running instance holds its own executable open. Without this the
  ; install appears to succeed while leaving the old binary in place, and the
  ; user sees the previous version after "updating".
  ;
  ; /SD IDOK makes the fallback dialog auto-answer under /S, so an unattended
  ; install can never hang on a prompt nobody is there to see.
  DetailPrint "Closing any running instance of Vitals..."
  nsExec::Exec '"$SYSDIR\taskkill.exe" /IM ${MAINBINARYNAME}.exe /F'
  Pop $0
  ; The watchdog runs from $INSTDIR\watchdog and holds its exe open the same
  ; way. Ending it is safe: the app starts it again at its next launch.
  nsExec::Exec '"$SYSDIR\taskkill.exe" /IM vitals-watchdog.exe /F'
  Pop $0

  ; Give the OS a moment to release file handles. Without the pause the very
  ; next file write can still fail with a sharing violation.
  Sleep 500
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Registering Vitals..."

  ; ADR 0038: tells the app an all-users copy is one somebody chose, not a
  ; 1.0.0-1.0.3 per-machine copy that an update merely kept in Program Files.
  ; The template's own "AllUsers" value cannot say this: it is written on
  ; every install, including the passive /UPDATE an elevated old app starts.
  ; Only a real install writes it; an update leaves whatever was there.
  ${If} $MultiUser.InstallMode == "AllUsers"
  ${AndIf} $UpdateMode <> 1
    WriteRegStr SHCTX "${UNINSTKEY}" "VitalsScopeChosen" "1"
  ${EndIf}

  ; DEV-3 (ADR 0038): a 1.0.0-1.0.3 copy lived in Program Files only. When
  ; such a copy updates without administrator rights, the app runs this
  ; setup with /CurrentUser /UPDATE, and the template skips every shortcut
  ; on /UPDATE - so the new per-user copy would have no Start menu entry,
  ; and the all-users one (pointing at the old copy) goes when the user
  ; removes that copy. Created here only when missing, so an ordinary
  ; per-user update never touches a shortcut the user moved or pinned.
  ${If} $MultiUser.InstallMode == "CurrentUser"
    ${IfNot} ${FileExists} "$SMPROGRAMS\${PRODUCTNAME}.lnk"
      CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
      !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
    ${EndIf}
    ; A desktop shortcut only for someone who had one: the old copy's lived
    ; on the public desktop.
    ReadEnvStr $1 PUBLIC
    ${If} ${FileExists} "$1\Desktop\${PRODUCTNAME}.lnk"
    ${AndIfNot} ${FileExists} "$DESKTOP\${PRODUCTNAME}.lnk"
      CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
      !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Same reasoning as install: an open handle turns a clean uninstall into a
  ; partial one that leaves orphaned files behind.
  DetailPrint "Closing Vitals..."
  nsExec::Exec '"$SYSDIR\taskkill.exe" /IM ${MAINBINARYNAME}.exe /F'
  Pop $0
  nsExec::Exec '"$SYSDIR\taskkill.exe" /IM vitals-watchdog.exe /F'
  Pop $0
  Sleep 500

  ; The logon entry names $INSTDIR\watchdog\vitals-watchdog.exe. Left behind
  ; it would make Windows try to start a deleted file at every sign-in.
  ; HKCU of the user running the uninstaller, which is who enabled it; a
  ; watchdog installed by hand elsewhere with --install is not ours.
  ; `vitals-watchdog --uninstall` is deliberately not used: it deletes the
  ; value whatever path it names, which is exactly the hand-installed copy
  ; this check exists to leave alone; taskkill above already stopped ours.
  ReadRegStr $1 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "VitalsWatchdog"
  ${If} $1 == '"$INSTDIR\watchdog\vitals-watchdog.exe"'
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "VitalsWatchdog"
  ${EndIf}

  ${If} $UpdateMode <> 1
    ; Start-with-Windows (tauri_plugin_autostart, app_name "Vitals") writes
    ; HKCU Run "Vitals" = `<exe path> --minimized`, unquoted, plus a
    ; StartupApproved\Run "Vitals" toggle. The template deletes the Run value
    ; after this hook (unconditionally, when not updating) but never the
    ; StartupApproved one, which then lingers in Task Manager's Startup tab
    ; as a dead entry. Prefix-compared so only an entry for THIS install is
    ; touched; == is case-insensitive, as Windows paths are.
    ReadRegStr $1 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Vitals"
    StrLen $2 "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $3 $1 $2
    ${If} $3 == "$INSTDIR\${MAINBINARYNAME}.exe"
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Vitals"
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Vitals"
    ${EndIf}

    ; The CPU sensors helper, once the user turned it on, is a LocalSystem
    ; AutoStart service ("vitals-sensors") running a copy of itself from
    ; %ProgramFiles%\Vitals Sensors. Nothing under $INSTDIR names it, so the
    ; template's file deletes leave a SYSTEM service starting at every boot
    ; for an app that is gone. The helper's own `uninstall` stops and deletes
    ; the service and removes that folder; PawnIO is left installed on
    ; purpose (shared with FanControl, LibreHardwareMonitor).
    ;
    ; The bundled copy is preferred: it is not the image the service runs,
    ; so it can delete the installed one. Skipped on /UPDATE: an update must
    ; not switch off sensors the user enabled.
    ;
    ; S21-06: `uninstall` needs administrator rights. A per-machine
    ; uninstaller has them; a per-user one (installMode both, ADR 0038)
    ; usually does not, and until 1.0.4 it fell through to sc.exe, which
    ; failed the same way and left a SYSTEM service and its Program Files
    ; folder behind for an app that was gone. The helper answers
    ; NOT_ELEVATED (5) without touching anything, so that exit code is the
    ; signal to ask: the same helper again with the "runas" verb, one UAC
    ; prompt naming vitals-sensors.exe. Declining is the user's call; they
    ; get the manual command instead of a silent leftover.
    ;
    ; Nothing is asked unless the service's SCM key exists (readable by any
    ; user) or its folder does, so someone who never turned sensors on never
    ; sees a prompt. Asked even under /S: an unattended per-user uninstall is
    ; rare (winget runs it in front of the person), and a prompt they can
    ; decline beats a privileged service nobody knows is there.
    StrCpy $1 ""
    ${If} ${FileExists} "$INSTDIR\sensors\vitals-sensors.exe"
      StrCpy $1 "$INSTDIR\sensors\vitals-sensors.exe"
    ${ElseIf} ${FileExists} "$PROGRAMFILES64\Vitals Sensors\vitals-sensors.exe"
      StrCpy $1 "$PROGRAMFILES64\Vitals Sensors\vitals-sensors.exe"
    ${EndIf}
    ClearErrors
    ReadRegStr $4 HKLM "SYSTEM\CurrentControlSet\Services\vitals-sensors" "ImagePath"
    ${If} ${Errors}
    ${AndIfNot} ${FileExists} "$PROGRAMFILES64\Vitals Sensors\*.*"
      ; Never installed: nothing to remove, nothing to ask.
      StrCpy $0 0
    ${ElseIf} $1 == ""
      StrCpy $0 "skipped"
    ${Else}
      DetailPrint "Removing the Vitals sensors service..."
      nsExec::ExecToLog '"$1" uninstall'
      Pop $0
      DetailPrint "vitals-sensors uninstall exited: $0"
      ${If} $0 == 5
        DetailPrint "The sensors service runs as SYSTEM: asking for administrator rights to remove it..."
        ; ENG2-4: an inherited __COMPAT_LAYER=RunAsInvoker would start the
        ; "elevated" helper with this same standard token.
        System::Call 'kernel32::SetEnvironmentVariable(t "__COMPAT_LAYER", p 0)'
        ClearErrors
        ExecShellWait "runas" "$1" "uninstall" SW_HIDE
        ${If} ${Errors}
          ; Declined, or no administrator could approve it.
          StrCpy $0 "declined"
        ${Else}
          ; ExecShellWait gives no exit code, so the SCM key is the answer.
          ; It goes when the last handle closes, a moment after the helper.
          StrCpy $0 "declined"
          ${For} $6 1 20
            ClearErrors
            ReadRegStr $4 HKLM "SYSTEM\CurrentControlSet\Services\vitals-sensors" "ImagePath"
            ${If} ${Errors}
              StrCpy $0 0
              ${Break}
            ${EndIf}
            Sleep 250
          ${Next}
          ${If} $0 == 0
            DetailPrint "Vitals sensors service removed."
          ${EndIf}
        ${EndIf}
      ${EndIf}
    ${EndIf}
    ${If} $0 == "declined"
      ; Without administrator rights sc.exe and RMDir would fail the same
      ; way; say what is left and how to remove it.
      ${If} $LANGUAGE == 1048
        StrCpy $5 "Serviciul de senzori Vitals este ${U+00EE}nc${U+0103} instalat ${U+0219}i ruleaz${U+0103} ca SYSTEM. Ca s${U+0103}-l elimini, ruleaz${U+0103} ${U+00EE}ntr-un Command Prompt deschis ca administrator:$\r$\n$\r$\n$\"$PROGRAMFILES64\Vitals Sensors\vitals-sensors.exe$\" uninstall$\r$\n$\r$\napoi ${U+0219}terge dosarul $PROGRAMFILES64\Vitals Sensors dac${U+0103} mai exist${U+0103}."
      ${Else}
        StrCpy $5 "The Vitals sensors service is still installed and runs as SYSTEM. To remove it, run this from a Command Prompt opened as administrator:$\r$\n$\r$\n$\"$PROGRAMFILES64\Vitals Sensors\vitals-sensors.exe$\" uninstall$\r$\n$\r$\nthen delete the folder $PROGRAMFILES64\Vitals Sensors if it is still there."
      ${EndIf}
      DetailPrint "The sensors service was left installed: the administrator prompt was declined or the removal failed."
      DetailPrint "$5"
      MessageBox MB_ICONINFORMATION|MB_OK "$5" /SD IDOK
    ${Else}
      ; A failed or missing helper must still not leave a SYSTEM service
      ; pointing at a deleted file. Both calls are no-ops when it never existed.
      ${If} $0 != 0
        nsExec::ExecToLog '"$SYSDIR\sc.exe" stop vitals-sensors'
        Pop $2
        Sleep 1000
        nsExec::ExecToLog '"$SYSDIR\sc.exe" delete vitals-sensors'
        Pop $2
        DetailPrint "sc delete vitals-sensors exited: $2"
      ${EndIf}
      ; /REBOOTOK: if the service image was still mapped (stop timed out), the
      ; leftover goes at next boot instead of failing the uninstall.
      RMDir /r /REBOOTOK "$PROGRAMFILES64\Vitals Sensors"
    ${EndIf}
  ${EndIf}

  ; If the user made Vitals their Task Manager, the registry points every
  ; Ctrl+Shift+Esc at a file that is about to be deleted; Windows would then
  ; report "file not found" until someone finds the key by hand. Only remove
  ; the value when it names Vitals: another tool's replacement (Process
  ; Explorer, System Informer) is that user's choice and not ours to undo.
  ;
  ; Compared against the exact quoted path we write (taskmgr.rs
  ; write_replacement), case-insensitively (LogicLib's == is; S== is
  ; case-sensitive) — StrFunc's
  ; ${StrCase}/${StrLoc} are install-side only and the template declares no
  ; uninstaller variants, so they do not compile here. A portable copy
  ; elsewhere is not this install's to remove.
  SetRegView 64
  ReadRegStr $1 HKLM "SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\taskmgr.exe" "Debugger"
  ${If} $1 == '"$INSTDIR\${MAINBINARYNAME}.exe"'
  ${OrIf} $1 == "$INSTDIR\${MAINBINARYNAME}.exe"
    DetailPrint "Restoring Windows Task Manager..."
    DeleteRegValue HKLM "SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\taskmgr.exe" "Debugger"
  ${EndIf}

  ; S21 (owner 2026-10-18): the optional codai sign-in keeps a key in
  ; Credential Manager (generic credential, target vitals/codai-api-key,
  ; crates/vitals-agents cloud.rs CREDENTIAL_TARGET). Settings stay on
  ; uninstall; a credential is not a setting, and a removed app must not leave
  ; a working key behind. Per-user vault of whoever runs the uninstaller (the
  ; one who signed in). Kept on /UPDATE so an update does not sign you out.
  ; cmdkey exits non-zero when there is none, which is fine.
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$SYSDIR\cmdkey.exe" /delete:vitals/codai-api-key'
    Pop $0
    ; The Health page's vmui device token and its address (health.rs), for
    ; the same reason: a removed app must not leave a working token behind.
    nsExec::Exec '"$SYSDIR\cmdkey.exe" /delete:vitals/vmui-health-token'
    Pop $0
    nsExec::Exec '"$SYSDIR\cmdkey.exe" /delete:vitals/vmui-health-base'
    Pop $0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Deliberately NOT deleting user settings here.
  ;
  ; An uninstall is often a reinstall, or a version rollback, and silently
  ; discarding a configured dashboard is unrecoverable. Settings live under
  ; %APPDATA%\Vitals and are documented as such, so removing them is a
  ; deliberate act rather than a side effect.
  DetailPrint "Vitals removed. Settings kept in %APPDATA%\Vitals."
!macroend
