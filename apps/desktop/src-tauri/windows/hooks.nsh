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
  nsExec::Exec 'taskkill /IM vitals.exe /F'
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
  nsExec::Exec 'taskkill /IM vitals.exe /F'
  Pop $0
  Sleep 500
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
