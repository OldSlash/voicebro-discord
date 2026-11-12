# Proposal

## Why

After installing 1.0.0 over an older VoiceBro, the user still sees the placeholder icon
(the cyan circle with an exclamation mark) on `voicebro.exe` in Explorer, on the desktop
and Start menu shortcuts and on the taskbar, while `uninstall.exe` in the same folder
shows the bird. The installed `voicebro.exe` carries only the bird: its one icon group
(`#32512`) holds six entries, byte-equal to `apps/desktop/icons/icon.ico`, and Tauri
2.12.5 loads the window icon from that same resource at run time. The placeholder comes
from the Windows shell, which caches an executable's icon by its path and index. The
update writes the new `voicebro.exe` to the old path, and Tauri's NSIS template never
tells the shell that the file changed (no `SHChangeNotify` in the generated
`installer.nsi`), so the shell keeps the cached placeholder for that path and for every
shortcut and taskbar button that points to it. `uninstall.exe` had no cached icon and
was read fresh. The `app-icon-v1` change named this risk and left it to the report
("the installer does not clear the cache"); this change fixes it in the installer.

## What Changes

- `NSIS_HOOK_POSTINSTALL` in `apps/desktop/installer-hooks.nsh` tells the shell, after
  `voicebro.exe` is copied and the Start menu shortcut is written, that the program file
  changed (`SHCNE_UPDATEITEM` on `$INSTDIR\voicebro.exe`) and that icons must be read
  again (`SHCNE_ASSOCCHANGED`), both flushed before the installer goes on. The desktop
  shortcut, created on the last page, comes after it and reads the icon fresh.
- `docs/ARCHITECTURE.md` 4.10: the window icon comes from the program's icon resource at
  run time (Tauri 2.12 on Windows, `default_window_icon_from_app_icon_resource`), not
  from `generate_context!`; the installer refreshes the shell's icons after an install.
- No code, app setting, icon or dependency changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-distribution`: a new requirement "Icon refresh after an install": after an install
  over an older version, the program file, the shortcuts and the taskbar show the new
  icon right away, without a sign-out or a manual cache refresh. It stands apart from
  "Application icon", which stays as it was (with the sentence inside it, that
  requirement's text went over the validator's 500-character limit).

## Impact

- `apps/desktop/installer-hooks.nsh` (the post-install hook), `docs/ARCHITECTURE.md`
  (4.10), `openspec/specs/app-distribution/spec.md` through the delta.
- Users on a version with the placeholder get the bird with the next installer. A machine
  that already installed 1.0.0 keeps the cached placeholder until the next install, a
  sign-out, or `ie4uinit.exe -show`.
