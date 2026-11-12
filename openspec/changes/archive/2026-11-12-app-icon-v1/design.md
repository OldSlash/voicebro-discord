# Design

## Context

`apps/desktop/tauri.conf.json` lists `bundle.icon: ["icons/icon.ico"]`. `tauri-build`
embeds that file into `voicebro.exe` as the program's icon resource, and
`tauri::generate_context!` makes its largest entry the default window icon (the main
window, built in code by `build_main_window`, sets no icon of its own and so takes the
default). Tauri hands it to tao's `set_window_icon`, which sends only
`WM_SETICON ICON_SMALL`; with no `ICON_BIG` the taskbar button falls back to that icon or
the program's resource. `tauri-bundler` does not pass `bundle.icon` to NSIS: without
`bundle.windows.nsis.installerIcon` and `uninstallerIcon` the installer and uninstaller
carry NSIS's own default icon, as the first trial build of this change showed. The Start
menu and desktop shortcuts point to `voicebro.exe` and show its resource. The current file
holds one 64 px BMP image, the placeholder.

The user's image (see proposal.md) is a 1024 × 1024 RGBA PNG. Measured on the file: the
visible square spans x and y 8..1015; outside it 74 932 pixels have alpha 0 and RGB
0, 0, 0; the soft edge has 1 660 pixels with partial alpha, none of them dark (brightest
channel ≥ 60 on every one); no visible pixel is detached from the square.

## Goals / Non-Goals

**Goals:**

- One kept source image and one generated `icon.ico`; nothing else to keep in sync.
- No visible change to the user's drawing.

**Non-Goals:**

- Hand-drawn versions for 16 and 24 px. At these sizes the drawing is scaled down as it
  is; a simplified small icon would be a later version of the icon.
- The README header, the window's own HTML and a favicon. The UI shows no logo today; the
  README header was left without an icon because of the placeholder and can take this one
  in a separate step if the user wants it.
- A version bump. The UI already shows the build stamp; the version of the next release is
  set when it is tagged.

## Decisions

**Generate with `cargo tauri icon`, keep only `icon.ico`.** Tauri CLI 2.12.4 is installed
and is the generator Tauri's own templates use. It writes `icon.ico` with 16, 24, 32, 48,
64 and 256 px, all PNG-compressed, scaled with Lanczos3. It also writes macOS, iOS,
Android and Microsoft Store images, which a Windows NSIS app does not use; the command runs
into a scratch folder and only `icon.ico` is copied into `apps/desktop/icons/`.
Alternative: a hand-written ICO packer. More code to keep, and no better result.

**Keep the source as `apps/desktop/icons/app-icon.png`.** `app-icon.png` is the name the
Tauri CLI looks for by default, so `cargo tauri icon icons\app-icon.png -o <scratch>` from
`apps/desktop` regenerates the set. Alternative: keep the source outside the repository.
Then nobody could regenerate the icon or reuse it, for example in the README.

**Give the transparent pixels the edge colour instead of black.** Each pixel with alpha 0
takes the average colour of its already filled neighbours, ring by ring outward from the
square, and keeps alpha 0. Checked on the result: alpha unchanged everywhere, every visible
pixel unchanged, no black left under the transparency. Tauri's scaler handles alpha
correctly: `icon.ico` generated from the original and from the cleaned PNG are byte-equal.
So the fill changes nothing in the app today. It protects the kept PNG for tools that
scale without premultiplied alpha, which would pull the hidden black into the rim. The
fill is a one-off step on the image, done with a throwaway PowerShell and .NET script; the
cleaned PNG is what is kept, not the script. Alternative: keep the PNG exactly as given.
That is equally correct for the ICO but leaves the trap for any other use of the PNG.

**Point the installer and the uninstaller at the same `icon.ico`.** Two keys under
`bundle.windows.nsis` (`installerIcon`, `uninstallerIcon`), resolved by the bundler from
`apps/desktop`. Alternative: a second, installer-only icon. Nothing asks for a different
look, and one file stays the single source.

**Check the real surfaces, not only the files.** The built `voicebro.exe` is checked
through the shell's icon for the file, the running window through `WM_GETICON` on its
handle, and the trial installer through its file icon. The shortcuts show the program
file's icon, which the first check covers.

## Risks / Trade-offs

- [Windows caches icons per path: a shortcut or taskbar pin made with the placeholder can
  keep showing it after an update] → Windows refreshes it in time or after a sign-out;
  `ie4uinit.exe -show` refreshes it at once. Mention it in the report; the installer does
  not clear the cache.
- [At 16 px the drawing is a few dozen pixels and reads as a purple square with a bird] →
  Accepted for version 1; a simplified small icon can come later.
- [.NET Framework's `System.Drawing.Icon` does not decode PNG-compressed entries and draws
  them as noise] → Only checking tools are affected; Windows itself reads such entries
  since Vista. Checks read the entries as PNG or go through the shell.
- [`tauri-build` embeds the icon at compile time] → After the file changes, check that the
  rebuilt `voicebro.exe` carries the new icon, not a cached old resource.
- [The window's small icon reaches Windows as 32 × 32, so at 300 % the 48 px title bar slot
  shows it scaled up and slightly soft, and the taskbar button's source is Windows'
  fallback] → Accepted: Tauri offers no taskbar icon of its own, and setting `ICON_BIG`
  would need code in the shell. A later change can do it if the soft edge bothers.
