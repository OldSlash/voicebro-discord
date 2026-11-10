# Design

## Context

See proposal.md. The first version measured each turn on its media transport (a new turn
after 350 ms without speech, one answer while its audio keeps coming within 1 s, a 15 s
window to match an answer to its turn, a signal at peak 256). Its Performance tab had a
window selector, Refresh, a health dot, two canvases
and fifteen stat cards fed by browser counters. In this app the input point is the gate's
send toward the assistant and the output point is `PcmSource` (playback into songbird); the
counters come from changes `voice-path-quality` (call) and `audio-gain-settings` (levels).

## Goals / Non-Goals

**Goals:** the first version's latency lines and Performance tab on this app's own points
and counters.

**Non-Goals:** CPU charts (the first version's "cpu-chart" showed packets, not CPU); memory
readings (change `memory-measurement`); persisting statistics across restarts.

## Decisions

**D1. One tracker per guild in the gate's neighbourhood.** The tracker is fed with a
timestamp and "speech or not" (peak ≥ 700, the gate's threshold) for each frame sent toward
the assistant, and with "signal or not" (peak ≥ 256) plus "received" and "played" times for
the assistant's audio. It applies the first version's rules and constants, written new, and logs
with the first version's wording, with "ChatGPT audio" kept as the answer's name. The
answer's "received" time is when the first frame with signal leaves the jitter buffer; the
"played" time is when `PcmSource` hands it to songbird.

*Changed while implementing (2026-11-10):* both of those moments fall in the same
`PcmSource` read, so "ChatGPT audio -> Discord relay handoff" would always read 0 ms. The
"received" time is instead the network arrival of the packet the first frame with signal
was decoded from (the jitter buffer keeps each packet's arrival and exposes it through
`FrameProvider::arrival`), so that duration is the time the answer waited in the jitter
buffer; the first live call measured 34 and 65 ms. Pushed frames (the stub assistant) have
no arrival time and use the playback time.

**D2. Status, not a new channel.** The last 50 turns go into the status snapshot; the window
samples the status once a second while open and keeps up to 5 minutes of samples in memory
for the charts. *Alternative:* push an event per turn. Rejected: the snapshot is already the
source of truth and refreshed every second here.

**D3. Charts on a canvas** with a small `drawChart` helper of its own (two series, no
library), resized to the card.

**D4. Quality rule** as in the spec: computed in the window from the counter deltas over the
chosen window; "idle" when the assistant has no connected call.

## Risks / Trade-offs

- [The peak threshold misses quiet speech] → it is the gate's own threshold, so turns match
  what the gate treats as speech; the input gain raises quiet speakers.
- [Delegated answers start with a filler ("let me check") before the real answer] → the line
  measures the first audio, as in the first version; the chat log shows the delegated task's
  own timing.
