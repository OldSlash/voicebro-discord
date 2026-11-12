# Tasks

## 1. Branch

- [x] 1.1 Create the branch `change/app-icon-refresh-on-update` from `main`; verify `git log -1` shows the `main` tip `263fc1d` and `git status` lists nothing changed besides this change (both `263fc1d`; besides the change only `docs/HANDOFF-MODES-REPORT.md`, untracked from before and not part of this change)

## 2. The installer

- [x] 2.1 In `apps/desktop/installer-hooks.nsh`, make `NSIS_HOOK_POSTINSTALL` send `SHCNE_UPDATEITEM` for `$INSTDIR\voicebro.exe` and then `SHCNE_ASSOCCHANGED`, both with `SHCNF_FLUSH`, through the System plugin; verify the plugin is in the NSIS that Tauri uses (`%LOCALAPPDATA%\tauri\NSIS\Plugins\x86-unicode\System.dll` is there; Tauri's own `FileAssociation.nsh` makes the same `SHCNE_ASSOCCHANGED` call through it, but only for file associations, which VoiceBro has none of)
- [x] 2.2 In `docs/ARCHITECTURE.md` 4.10, say that the window icon is loaded from the program's icon resource at run time and that the installer refreshes the shell's icons after an install (the Icon item now names group `#32512` and `default_window_icon_from_app_icon_resource` instead of `generate_context!` taking the largest entry, and ends with the notice; the Update item names both hooks)
- [x] 2.3 Run `scripts\build-release.ps1 -Trial`; verify the generated `installer.nsi` has both calls inside the post-install hook after the program file is copied, `makensis` compiles it without a warning about them, the trial installer's `voicebro.exe` still carries the bird in its one icon group, and `openspec validate app-icon-refresh-on-update --strict` passes; the report says the trial installer was not installed (it would go over the user's own install), so the effect on a real update is not seen (built in 1 min 07 s, exit 0, `target\release\dist-trial\VoiceBro_1.0.0_x64-setup-trial.exe`; the generated script includes the hooks file from `apps\desktop`; `makensis -V4` on it exits 0 and lists, inside `Section "Install"`, `File: "voicebro.exe"`, then `Call "CreateOrUpdateStartMenuShortcut"`, then both `SHChangeNotify` plugin commands with `$INSTDIR\voicebro.exe` before `SectionEnd`; its three warnings (6010 `Skip`, 6040 `choowHowToInstall`) come from Tauri's template and were there before; the new `voicebro.exe` has one group `#32512` with six entries byte-equal to `icon.ico`; the change validates with `--strict`. Not checked: the trial installer was not installed, so the calls never ran and their effect on a machine with the placeholder cached was not seen)

## Workflow follow-up

- Before the commit run `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test`; commit only with the user's OK.
- Merge into `main` when the user says so; archive the change in the same step.
