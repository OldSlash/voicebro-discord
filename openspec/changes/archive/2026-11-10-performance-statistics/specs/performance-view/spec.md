# Spec Delta

## Purpose

Gives the user the first version's Performance tab: a quick verdict on call quality, live
charts and the measured counters, over a time window the user chooses.

## ADDED Requirements

### Requirement: Quality verdict
The Performance tab SHALL show a health dot and a quality label: "idle" when no call is
connected, "stable" when a call is connected and in the chosen window the late and concealed
packets are under 1 % of those received and no input frame was dropped, and "degraded"
otherwise, naming the counter that broke the rule.

#### Scenario: Clean call
- **WHEN** a call runs with no late, concealed or dropped audio in the last 60 s
- **THEN** the dot is green and the label reads "Quality: stable"

#### Scenario: Lossy call
- **WHEN** 3 % of the received packets in the window were concealed
- **THEN** the dot turns amber and the label reads "Quality: degraded (concealed packets)"

### Requirement: Charts over a chosen window
The tab SHALL offer the windows 30 s, 60 s and 5 min and SHALL draw two charts over the chosen
window, sampled once per second: "Audio timing" with the latency of each turn, and "Media
packets" with packets per second sent to and received from the call.

#### Scenario: Changing the window
- **WHEN** the user switches from 60 s to 5 min
- **THEN** both charts redraw over the last five minutes of samples kept since the window opened

### Requirement: Stat cards are measured values
The tab SHALL show stat cards for the Discord, call, wake engine, level and latency counters
of the status, each a number read from the status, never a placeholder text.

#### Scenario: Cards during a session
- **WHEN** the user opens the tab during a session
- **THEN** every card shows a number, and Refresh re-reads them at once
