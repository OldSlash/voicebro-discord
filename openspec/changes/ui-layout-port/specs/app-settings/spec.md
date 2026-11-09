# Spec Delta

## ADDED Requirements

### Requirement: Appearance settings
The settings SHALL hold `ui.theme` (one of `dark`, `darcula`, `light`, `intellij`, default
`dark`) and `ui.font_size` (11 to 20, default 13). Other values SHALL be rejected naming the
field.

#### Scenario: Valid appearance
- **WHEN** the user saves the theme `intellij` and the font size 15
- **THEN** the save succeeds and the window applies both

#### Scenario: Unknown theme
- **WHEN** a saved document sets `ui.theme` to `pink`
- **THEN** the save is rejected and the message names `ui.theme` and the allowed themes
