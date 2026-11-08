; Zenith NSIS installer hooks (Tauri bundler).
; Registers Zenith as a browser candidate (http/https + HTML file types, Default Apps
; capabilities) for the current user after install, and removes the keys before uninstall.
; Windows does not allow installers to change the default browser silently; users pick
; Zenith in Settings > Apps > Default apps (the app offers a shortcut to that page).

!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\zenith.exe" --register-browser'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\zenith.exe" --unregister-browser'
!macroend
