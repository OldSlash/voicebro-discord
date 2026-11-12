# Design

## Context

Checked on the installed 1.0.0 (`%LOCALAPPDATA%\VoiceBro`, installed over earlier builds
at the same path):

- `voicebro.exe` has one `RT_GROUP_ICON` (`#32512`, the id Tauri uses) with six entries,
  16 to 256 px, each byte-equal to the matching entry of `apps/desktop/icons/icon.ico`.
  No other icon resource is in the file.
- Tauri 2.12.5 on Windows does not compile a window icon into the binary:
  `generate_context!` emits `default_window_icon_from_app_icon_resource()`, which loads
  resource `#32512` at run time at the system's large icon size. The title bar shows the
  same bird as the file.
- The generated `target\release\nsis\x64\installer.nsi` copies `voicebro.exe`, writes the
  uninstaller and the registry, creates the Start menu shortcut, then inserts
  `NSIS_HOOK_POSTINSTALL`; the desktop shortcut is created later, by the finish page's
  check box. Nothing in it calls `SHChangeNotify`.

The shell keeps an icon cache keyed by the icon's file path and index, kept in memory by
Explorer and persisted in `iconcache_*.db`. It is not invalidated by a file being
overwritten in place. So after an update every surface Explorer draws from that cache
(the file, the shortcuts that point to it, a pinned or grouped taskbar button) keeps the
placeholder, while a file the shell has never drawn (`uninstall.exe`) is read fresh.

## Goals / Non-Goals

**Goals:**

- After any install, fresh or over an older version, the shell draws the icon that is in
  the installed `voicebro.exe`.

**Non-Goals:**

- Changing the icon, its sizes or the window's `ICON_BIG`.
- Clearing the cache on a machine that already has 1.0.0 without installing again.

## Decisions

**Notify the shell from the post-install hook.** Two `SHChangeNotify` calls through
NSIS's System plugin (bundled with the NSIS that Tauri downloads):
`SHCNE_UPDATEITEM` (`0x00002000`) with `SHCNF_PATHW | SHCNF_FLUSH` (`0x1005`) on
`$INSTDIR\voicebro.exe`, then `SHCNE_ASSOCCHANGED` (`0x08000000`) with
`SHCNF_IDLIST | SHCNF_FLUSH` (`0x1000`). The first marks that file changed; the second
is the documented way to make the shell drop its cached icons, the call installers make
after changing icons or associations. `SHCNF_FLUSH` returns only after the shell has
taken the notice. Alternative: run `ie4uinit.exe -show`. An undocumented tool with no
guaranteed behaviour across Windows builds; the notice is the API it relies on.
Alternative: give each build a different file name. Breaks shortcuts, pins and the
update path.

**Do it on every install, not only on updates.** A fresh install after an uninstall can
hit the same stale entry, and the notice costs nothing when there is none.

## Risks / Trade-offs

- [`SHCNE_ASSOCCHANGED` makes Explorer redraw its icons once; the desktop may flicker] →
  Accepted: once per install.
- [Not checkable without installing: the trial installer must not go over the user's own
  install, and no second machine with the placeholder cached is at hand] → The check
  covers the generated script and the compiled installer; the effect on a real update is
  left to the next install of a release, and the report says so.
