## MODIFIED Requirements

### Requirement: Third-party notices
The installer and the zip SHALL include `THIRD-PARTY-NOTICES.txt` naming, with license and
license text, every crate built into `voicebro.exe`, the native libraries compiled or linked
into it from inside those crates, and the installer's own components. It SHALL name Vosk
and every model of the wake catalog, which the app downloads with the user's OK, and the
Codex CLI, which the app needs but does not ship, with their licenses.

#### Scenario: Notices complete
- **WHEN** the notices file of a release is compared with the crates of the release build
- **THEN** every crate in the build appears with its license text, or with its license name and source link when it ships no license file

#### Scenario: Native code inside a crate
- **WHEN** a crate compiles or links a library under its own license into `voicebro.exe`, as `libopus_sys` does with Opus and `webview2-com-sys` with Microsoft's WebView2 loader
- **THEN** the notices carry that library's name, version, license and full license text

#### Scenario: Installer components
- **WHEN** the notices of a release are read
- **THEN** they name NSIS with its license text, Tauri's NSIS plug-in with its license, and Microsoft's WebView2 Runtime installer, which the installer downloads only when the runtime is missing

#### Scenario: A license text goes missing
- **WHEN** a license file the notices take a native library's or the installer's text from does not exist, for example after a dependency update moved it
- **THEN** the notices script stops with an error that names the file and writes no notices file

#### Scenario: Wake models named
- **WHEN** the notices of a release are read
- **THEN** they name the Vosk library and each of the ten catalog models with its license: the nine Apache 2.0 models in one group, and the Czech model with its MIT license text and copyright holder

#### Scenario: Catalog and notices differ
- **WHEN** the wake catalog has a model the notices script does not list, or the script lists one the catalog does not have
- **THEN** the notices script stops with an error that names the model and writes no notices file

## ADDED Requirements

### Requirement: Release artifacts carry no wake files
The installer and the portable zip SHALL contain neither the Vosk library nor any wake
model; the app SHALL download them only after the user's OK.

#### Scenario: Release contents
- **WHEN** the installer of a release is unpacked and the zip is listed
- **THEN** neither contains `libvosk.dll`, a `vosk-model-*` folder or an `assets\` folder
