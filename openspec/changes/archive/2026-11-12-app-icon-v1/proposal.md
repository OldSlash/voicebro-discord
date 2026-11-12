# Proposal

## Why

The app still carries the placeholder icon from the first commit: a cyan circle with an
exclamation mark, a single 64 px image, which reads as a warning sign on the taskbar, in
the window's title bar, on the installer and on the shortcuts. The user has an icon for
VoiceBro, version 1, and wants it in the app now.

## What Changes

- The icon, version 1: the user's 1024 × 1024 PNG, a bird in sunglasses at a microphone,
  sitting on a speaker, in a rounded square with a cyan-to-pink rim. The image is the
  user's own (made with ChatGPT, whose terms give the output to the user), so it ships
  under the repository's license like the rest of the project and needs no third-party
  notice.
- Outside the rounded square the image is already fully transparent: no visible pixel lies
  outside it, and its soft edge holds no dark pixel. The transparent pixels store black as
  their hidden colour; they get the colour of the nearest edge instead, with transparency
  unchanged, so no tool that scales without premultiplied alpha can pull a dark rim into
  the edge. Every visible pixel stays as the user gave it.
- `apps/desktop/icons/icon.ico` is generated from that PNG with Tauri's `cargo tauri icon`:
  16, 24, 32, 48, 64 and 256 px. The 1024 px PNG is kept next to it as the source, so the
  icon can be generated again.
- `voicebro.exe`, the main window's title bar and taskbar button, the installer and
  uninstaller, and the Start menu and desktop shortcuts show the new icon. The program and
  the window take it from `bundle.icon`; the installer and uninstaller need
  `bundle.windows.nsis.installerIcon` and `uninstallerIcon` pointing to the same file,
  since without them NSIS puts its own default icon on both (found in the first trial
  build of this change). No code changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-distribution`: a new requirement "Application icon": the program file, the window,
  the installer and the shortcuts show the VoiceBro icon, generated in every size Windows
  asks for from a kept source image.

## Impact

- `apps/desktop/icons/icon.ico` (replaced), new `apps/desktop/icons/app-icon.png` (the
  1024 px source).
- `apps/desktop/tauri.conf.json`: `installerIcon` and `uninstallerIcon` under
  `bundle.windows.nsis`.
- `docs/ARCHITECTURE.md`: the repository layout names `icons/`, and 4.10 says where the
  icon comes from and how it is generated again.
- No code, app setting or dependency changes. The README header stays without the icon;
  adding it there is a separate decision.
