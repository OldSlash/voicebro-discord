# Proposal

## Why

The user asked for the first version's statistics (`docs/NEXT-CHANGES.md`, section 5): its
Performance tab with live charts and stat cards, and its per-turn latency report in the log
("turn 4 (response): first Discord speech -> ChatGPT audio 4343 ms; …"). Without numbers the
user cannot tell whether a bad answer came from the network, the audio path or the model.

## What Changes

- **Latency per turn**, ported from the first version's tracker: a turn starts with speech
  toward the assistant after a pause; for each answer the app measures first speech to the
  first answer audio, last speech to the first answer audio, answer audio to playback in
  Discord, and first speech to playback; speech during an answer is reported as an
  interruption; answers without a preceding turn are reported as autonomous. Each turn is
  one log line and stays available to the window.
- **Performance tab** as in the first version: a window selector (30 s, 60 s, 5 min),
  Refresh, a health dot with a quality label (idle, stable, degraded), two live charts
  ("Audio timing": latency per turn; "Media packets": packets per second each way) and stat
  cards from this app's counters: Discord ticks and speaker frames, frames sent to the call,
  input drops and underruns, packets received, late and concealed packets, playout
  underruns, output drops, wake engine drops, the levels and clip counts of both directions,
  the last turn's latencies.

## Capabilities

### New Capabilities

- `call-latency`: measuring and reporting the latency of each conversational turn.
- `performance-view`: the Performance tab: quality, charts and stat cards over a chosen
  time window.

### Modified Capabilities

None.

## Impact

- A latency tracker in `crates/voicebro-core` fed at the gate's send toward the assistant
  and at the playout of the assistant's audio (`PcmSource`), with the recent turns in the
  status; `apps/desktop/ui` (the Performance tab, charts on a canvas as in the first
  version).
- Depends on the counters of `voice-path-quality` and the levels of `audio-gain-settings`.
- Docs: `docs/ARCHITECTURE.md` (observability), `AGENTS.md`.
