# Spec Delta

## ADDED Requirements

### Requirement: Application icon

`voicebro.exe`, the main window's title bar and taskbar button, the installer, the
uninstaller and the Start menu and desktop shortcuts SHALL show the VoiceBro icon. The
icon SHALL come in 16, 24, 32, 48, 64 and 256 px, generated from one source image kept in
the repository, and SHALL be transparent outside its rounded square.

#### Scenario: The program file and the window

- **WHEN** the user starts `voicebro.exe` from the portable zip or from an install
- **THEN** the file in Explorer, the window's title bar and its taskbar button show the
  VoiceBro icon, not a generic or placeholder icon

#### Scenario: The installer and the shortcuts

- **WHEN** the user runs the installer and opens the Start menu or the desktop after it
- **THEN** the installer file, its window, the uninstaller and the `VoiceBro` shortcuts
  show the VoiceBro icon

#### Scenario: Small sizes and dark backgrounds

- **WHEN** Windows shows the icon at 16 px on a dark taskbar or at 32 px on a light
  Explorer view
- **THEN** the area outside the rounded square is transparent and no dark rim shows
  around the square
