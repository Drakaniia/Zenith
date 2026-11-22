; Zenith CEF per-user NSIS installer (Windows 11 x64).
; Invoked by apps/desktop/scripts/package-nsis.ps1 with:
;   makensis /DVERSION=x.y.z /DSTAGE_DIR=... /DOUT_FILE=... /DICON_FILE=... installer.nsi

Unicode True
SetCompressor /SOLID lzma
SetCompressorDictSize 32

!include "MUI2.nsh"
!include "FileFunc.nsh"

!ifndef VERSION
  !error "VERSION must be defined"
!endif
!ifndef STAGE_DIR
  !error "STAGE_DIR must be defined"
!endif
!ifndef OUT_FILE
  !error "OUT_FILE must be defined"
!endif
!ifndef ICON_FILE
  !error "ICON_FILE must be defined"
!endif

Name "Zenith ${VERSION}"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\Zenith"
InstallDirRegKey HKCU "Software\Zenith" "InstallDir"
RequestExecutionLevel user
ShowInstDetails hide
ShowUninstDetails hide

!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zenith"
!define MUI_ICON "${ICON_FILE}"
!define MUI_UNICON "${ICON_FILE}"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\zenith.exe"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Var NoRun

Function .onInit
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/NORUN" $R1
  IfErrors +2 0
    StrCpy $NoRun "1"

  ; Interactive pre-upgrade warning when an existing legacy WebView2 profile is detected.
  IfSilent skip_migration_warn
  IfFileExists "$LOCALAPPDATA\dev.zenith.browser\WebView2\*.*" 0 skip_migration_warn
  IfFileExists "$LOCALAPPDATA\dev.zenith.browser\CEF\v1\.migrated-v1" skip_migration_warn 0
  MessageBox MB_OKCANCEL|MB_ICONINFORMATION \
    "Zenith is upgrading to its bundled Chromium (CEF) browser engine.$\r$\n$\r$\n\
Preserved (and backed up automatically): bookmarks, passwords, history, settings, profiles, and open tabs.$\r$\n$\r$\n\
Starts fresh: website sign-ins, cookies, site storage, and extension data. Your old WebView2 folder will be left untouched.$\r$\n$\r$\n\
Click OK to continue installing Zenith ${VERSION}." \
    IDOK skip_migration_warn
  Abort
  skip_migration_warn:
FunctionEnd

Section "Install"
  SetOutPath "$INSTDIR"

  ; Stop any running Zenith process before replacing runtime binaries.
  nsExec::ExecToLog 'taskkill /IM zenith.exe /T /F'
  Sleep 400

  ; Remove prior engine files so old and new CEF components can never mix.
  Delete "$INSTDIR\zenith.exe"
  Delete "$INSTDIR\zenith.exe.manifest"
  Delete "$INSTDIR\zenith.dll"
  Delete "$INSTDIR\runtime-manifest.json"
  Delete "$INSTDIR\*.dll"
  Delete "$INSTDIR\*.pak"
  Delete "$INSTDIR\*.dat"
  Delete "$INSTDIR\*.bin"
  Delete "$INSTDIR\*.json"
  RMDir /r "$INSTDIR\locales"
  RMDir /r "$INSTDIR\ui"

  ; Copy complete verified CEF runtime + UI bundle.
  File "${STAGE_DIR}\zenith.exe"
  File "${STAGE_DIR}\zenith.exe.manifest"
  File "${STAGE_DIR}\zenith.dll"
  File "${STAGE_DIR}\runtime-manifest.json"
  File "${STAGE_DIR}\libcef.dll"
  File "${STAGE_DIR}\chrome_elf.dll"
  File "${STAGE_DIR}\d3dcompiler_47.dll"
  File /nonfatal "${STAGE_DIR}\dxcompiler.dll"
  File "${STAGE_DIR}\icudtl.dat"
  File "${STAGE_DIR}\resources.pak"
  File "${STAGE_DIR}\chrome_100_percent.pak"
  File "${STAGE_DIR}\chrome_200_percent.pak"
  File "${STAGE_DIR}\v8_context_snapshot.bin"
  File /nonfatal "${STAGE_DIR}\vk_swiftshader.dll"
  File /nonfatal "${STAGE_DIR}\vk_swiftshader_icd.json"
  File /nonfatal "${STAGE_DIR}\vulkan-1.dll"

  SetOutPath "$INSTDIR\locales"
  File "${STAGE_DIR}\locales\en-US.pak"

  SetOutPath "$INSTDIR\ui"
  File /r "${STAGE_DIR}\ui\*.*"

  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\Zenith" "InstallDir" "$INSTDIR"

  ; Add/Remove Programs registration.
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "Zenith"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "Zenith"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\zenith.exe,0"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1

  ; Start Menu & Desktop shortcuts.
  CreateShortcut "$SMPROGRAMS\Zenith.lnk" "$INSTDIR\zenith.exe"
  CreateShortcut "$DESKTOP\Zenith.lnk" "$INSTDIR\zenith.exe"

  ; Register Zenith as a Windows Default Apps browser candidate.
  ExecWait '"$INSTDIR\zenith.exe" --register-browser'

  ; Silent auto-update relaunch (skipped when `/NORUN` is passed).
  IfSilent 0 done
  StrCmp $NoRun "1" done
  Exec '"$INSTDIR\zenith.exe"'
  done:
SectionEnd

Section "Uninstall"
  nsExec::ExecToLog 'taskkill /IM zenith.exe /T /F'
  Sleep 400
  ExecWait '"$INSTDIR\zenith.exe" --unregister-browser'

  Delete "$SMPROGRAMS\Zenith.lnk"
  Delete "$DESKTOP\Zenith.lnk"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\Zenith"

  RMDir /r "$INSTDIR\locales"
  RMDir /r "$INSTDIR\ui"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$INSTDIR\*.*"
  RMDir "$INSTDIR"
SectionEnd
