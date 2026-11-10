# Spec Delta

## MODIFIED Requirements

### Requirement: Charts over a chosen window
The tab SHALL offer the windows 30 s, 60 s and 5 min and SHALL draw two charts over the chosen
window, sampled once per second while performance monitoring is on: "Audio timing" with the
latency of each turn, and "Media packets" with packets per second sent to and received from
the call. While performance monitoring is off, no sample SHALL be taken and none kept.

#### Scenario: Changing the window
- **WHEN** the user switches from 60 s to 5 min
- **THEN** both charts redraw over the last five minutes of samples kept since monitoring was turned on

#### Scenario: Monitoring off
- **WHEN** performance monitoring is off for a minute
- **THEN** no status sample is taken for the tab in that minute and nothing is kept for it

## ADDED Requirements

### Requirement: Monitoring switch on the tab
The Performance tab's toolbar SHALL have a "Performance monitoring" switch that is the
`diagnostics.performance` setting: a change SHALL be saved automatically, without a button,
and SHALL take effect at once; the saved value SHALL be the one shown after a restart.

#### Scenario: Turned on from the tab
- **WHEN** the user turns the switch on in the Performance tab's toolbar
- **THEN** the setting is saved without a button and sampling starts within a second

#### Scenario: Kept across restarts
- **WHEN** the user turns the switch on, closes the app and starts it again
- **THEN** the Performance tab opens with the switch on and sampling running

### Requirement: Off state
While performance monitoring is off, the Performance tab SHALL show, besides its switch,
only a note saying that the monitoring is off and what turning it on costs: no verdict, no
charts and no stat cards. Between app events the window SHALL then read the status no more
often than every 5 s.

#### Scenario: Tab opened with monitoring off
- **WHEN** the user opens the Performance tab with the switch off
- **THEN** the tab shows the switch and the note and nothing else, and the window reads the status every 5 s between events

#### Scenario: Turned on
- **WHEN** the user turns performance monitoring on, from the tab or from Settings
- **THEN** within a second the tab shows the verdict, the charts and the cards, and samples are taken once per second, without a restart

#### Scenario: Turned off
- **WHEN** the user turns performance monitoring off during a call
- **THEN** the kept samples are dropped, the tab shows the note, and the status is read every 5 s between events again
