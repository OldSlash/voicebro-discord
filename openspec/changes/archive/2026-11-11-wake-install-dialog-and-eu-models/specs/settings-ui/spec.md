## MODIFIED Requirements

### Requirement: Wake model list
The wake model control SHALL list the catalog models and the local models found at start,
each with its language and whether it is downloaded, still to be downloaded (with its size)
or from the models folder. Its help line SHALL say that a model not on this PC is
downloaded only after the user agrees, where the models folder is, that small Vosk models
are the ones to use and that a restart picks up a new folder, and an "Open models folder"
button SHALL open the folder.

#### Scenario: Fresh install
- **WHEN** the app runs for the first time
- **THEN** the list shows the ten catalog models, each marked as still to be downloaded with its size, and the English model is selected

#### Scenario: Local model
- **WHEN** a model folder was found in `models\` at start
- **THEN** the list shows it marked as from the models folder

#### Scenario: Opening the folder
- **WHEN** the user presses "Open models folder"
- **THEN** the folder `models\` next to the executable opens in Explorer

### Requirement: Download notice
While the wake engine waits for a download, the Assistant card SHALL show under the wake
model control what will be downloaded, from which site and how many megabytes, with a
Download button, whether or not the selected model has phrases. The download SHALL start
only when the button is pressed; the notice SHALL then show the progress, and an error in
place with the button again.

#### Scenario: Notice
- **WHEN** wake-word mode is on and the English model is selected while nothing is on disk, with or without phrases
- **THEN** the notice lists the Vosk library (15 MB, github.com) and the English model (41 MB, alphacephei.com), and nothing downloads

#### Scenario: Download pressed
- **WHEN** the user presses Download
- **THEN** the notice shows the progress of each file and disappears when the files are installed

#### Scenario: Download failed
- **WHEN** a download fails or its hash does not match
- **THEN** the notice shows the error and the Download button again

## ADDED Requirements

### Requirement: Choosing a model that is not installed
When the user picks a model in the wake model list whose files or the Vosk library are not
on disk, the window SHALL open the install dialog before saving. Download SHALL save the
choice and start that model's download; Cancel SHALL put the list back on the stored model
and save nothing. A model whose files are all on disk SHALL be saved at once, as before.

#### Scenario: Missing model picked
- **WHEN** the English model is installed and selected and the user picks the German model, which is not on disk
- **THEN** the install dialog lists the German model with its size and site, and the settings file still names the English model

#### Scenario: Download chosen
- **WHEN** the user presses Download in that dialog
- **THEN** the German model is saved as the wake model, its download starts, and the dialog shows the progress

#### Scenario: Cancel chosen
- **WHEN** the user presses Cancel in that dialog
- **THEN** the list shows the English model again, nothing is saved, and nothing is downloaded

#### Scenario: Installed model picked
- **WHEN** the user picks a model whose files and the Vosk library are on disk
- **THEN** no dialog opens and the choice is saved at once

#### Scenario: The list while the dialog is open
- **WHEN** the install dialog is open and the wake model list changes anyway (a script, a keyboard path)
- **THEN** the list goes back to the stored model, nothing is saved, and the dialog's Download keeps naming the model it asked about

#### Scenario: Local model without the library
- **WHEN** the user picks a local model while the Vosk library is not on disk
- **THEN** the dialog lists only the Vosk library
