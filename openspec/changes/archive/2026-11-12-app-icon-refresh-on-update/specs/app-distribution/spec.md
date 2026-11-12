# Spec Delta

## ADDED Requirements

### Requirement: Icon refresh after an install

After every install the installer SHALL tell the Windows shell that `voicebro.exe` changed
and that icons must be read again, so no icon cached for an earlier build at the same path
stays on screen.

#### Scenario: An install over a build with another icon

- **WHEN** the user installs over an earlier VoiceBro whose `voicebro.exe` had another
  icon, at the same path
- **THEN** right after the install, without a sign-out or a manual cache refresh,
  `voicebro.exe` in Explorer, the Start menu and desktop shortcuts and the taskbar
  button show the icon of the newly installed file
