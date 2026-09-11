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

!macro NSIS_HOOK_PREINSTALL
  ; A running instance holds its own executable open. Without this the
  ; install appears to succeed while leaving the old binary in place, and the
  ; user sees the previous version after "updating".
  ;
  ; /SD IDOK makes the fallback dialog auto-answer under /S, so an unattended
  ; install can never hang on a prompt nobody is there to see.
  DetailPrint "Closing any running instance of Vitals..."
  nsExec::Exec 'taskkill /IM ${MAINBINARYNAME}.exe /F'
  Pop $0

  ; Give the OS a moment to release file handles. Without the pause the very
  ; next file write can still fail with a sharing violation.
  Sleep 500
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Registering Vitals..."
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Same reasoning as install: an open handle turns a clean uninstall into a
  ; partial one that leaves orphaned files behind.
  DetailPrint "Closing Vitals..."
  nsExec::Exec 'taskkill /IM ${MAINBINARYNAME}.exe /F'
  Pop $0
  Sleep 500

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
