# Spec Delta

## ADDED Requirements

### Requirement: The download notice offers the download by hand
Next to its Download button the Assistant card's download notice SHALL show the same
download-by-hand part as the install dialog: each missing file's archive name with a link
to the official page it is published on, an "Open assets folder" button and the line on
putting the `.zip` files there as they are and pressing Download. The part SHALL be hidden
while a download runs and shown again after a failure.

#### Scenario: Notice with the links
- **WHEN** wake-word mode is on and the English model is selected while nothing is on disk
- **THEN** the notice names `vosk-win64-0.3.45.zip` with a link to the Vosk release page on github.com and `vosk-model-small-en-us-0.15.zip` with a link to the Vosk models page on alphacephei.com, with the "Open assets folder" button

#### Scenario: A link is clicked
- **WHEN** the user clicks one of the notice's links
- **THEN** the page opens in the default browser, the window stays on Settings → Assistant, and no setting changes

#### Scenario: Downloading
- **WHEN** the user presses the notice's Download
- **THEN** the links and the folder button are hidden while the progress shows

#### Scenario: Download failed
- **WHEN** a download fails or its hash does not match
- **THEN** the notice shows the error, the Download button, the links and the folder button again
