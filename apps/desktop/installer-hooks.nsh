; Hooks for Tauri's NSIS template (bundle.windows.nsis.installerHooks).
;
; VoiceBro keeps its data next to voicebro.exe, so Tauri's own "Delete the application
; data" step (which clears %APPDATA% and %LOCALAPPDATA%\<identifier>) does not reach it.
; With the box checked, and never during an update, this removes the app's data from the
; install folder and the Discord token from the Windows Credential Manager, then the
; folder itself once it is empty. Unchecked, the data stays for a later install.

; The installer takes English or Russian from the system language without asking, and
; Tauri's template saves that choice nowhere, so the uninstaller (MUI_UNGETLANGUAGE) would
; open an "Installer Language" dialog first. Saving the language where it looks avoids it.
;
; The Windows shell caches an executable's icon by its path. An update writes the new
; voicebro.exe over the old one, so without a notice Explorer, the shortcuts and the
; taskbar keep the icon of the earlier build. This tells the shell that the file changed
; (SHCNE_UPDATEITEM, SHCNF_PATHW | SHCNF_FLUSH) and that cached icons must be read again
; (SHCNE_ASSOCCHANGED, SHCNF_IDLIST | SHCNF_FLUSH). The hook runs after the program file
; is copied and the Start menu shortcut is written; the desktop shortcut comes later.
!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "${MANUPRODUCTKEY}" "Installer Language" $LANGUAGE
  System::Call 'shell32::SHChangeNotify(i 0x00002000, i 0x1005, w "$INSTDIR\${MAINBINARYNAME}.exe", p 0)'
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0x1000, p 0, p 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    RMDir /r "$INSTDIR\logs"
    RMDir /r "$INSTDIR\assets"
    RMDir /r "$INSTDIR\models"
    RMDir /r "$INSTDIR\codex-workspace"
    RMDir /r "$INSTDIR\webview-data"
    Delete "$INSTDIR\settings.json"
    Delete "$INSTDIR\settings.json.tmp"
    Delete "$INSTDIR\.voicebro-write-test"
    nsExec::Exec 'cmdkey /delete:VoiceBro/discord_token'
    Pop $0
    RMDir "$INSTDIR"
  ${EndIf}
!macroend
