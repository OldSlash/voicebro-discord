# Spec Delta

## Purpose

Shows the app's own log in the window, as the first version's Console tab does, so the user
can follow what the app does and copy it into a report without opening files.

## ADDED Requirements

### Requirement: Console shows the app log
The Console tab SHALL show the app log as forwarded to the window, one line per record with
time and level, starting with the records kept from before the window opened.

#### Scenario: Window opened during a session
- **WHEN** the user opens the Console tab after the bot joined a channel
- **THEN** the lines written since start are shown, ending with the latest one

### Requirement: Console search
The Console SHALL filter or highlight lines matching a typed search text and SHALL show the
number of matching lines.

#### Scenario: Searching for the recognizer
- **WHEN** the user types "heard" into the search box
- **THEN** the match count equals the number of lines containing "heard"

### Requirement: Console actions
The Console SHALL offer Clear (empties the view only), Copy (all shown lines), Copy selection
and Auto-scroll; with Auto-scroll on, the view follows new lines, and with it off, the view
stays where the user scrolled.

#### Scenario: Reading back
- **WHEN** Auto-scroll is off and new lines arrive
- **THEN** the view does not move

#### Scenario: Clear
- **WHEN** the user presses Clear
- **THEN** the view is empty, the log file is unchanged, and new lines appear as they arrive
