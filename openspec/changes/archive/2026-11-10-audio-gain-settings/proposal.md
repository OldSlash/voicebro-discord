# Proposal

## Why

The app has no volume controls. The user needs the assistant to hear the channel about 3.4
times louder (339 %) and its answer to play about 2.5 times louder (252 %) than the levels
the app passes through now, and requires a setting for each direction.

## What Changes

- A new settings section `audio` with `input_gain` (how loud the assistant hears the
  channel) and `output_gain` (how loud the assistant speaks in the channel), each a
  multiplier from 0.0 to 4.0, default 1.0.
- The input gain scales the frames the session gate sends to the assistant; the wake engine
  keeps the unscaled audio. The output gain scales the assistant's audio on its way to
  Discord; earcons keep their own volume.
- Samples pushed past the 16-bit range are clamped and counted as clipped; the peak level of
  the last second in each direction and the clip counts are in the status.
- A saved change applies to the running call within one frame, without rejoining.
- The Settings tab gets an "Audio" card: two sliders with their percentage ("What the
  assistant hears", "How loud the assistant speaks") and the existing earcon switch and
  volume, saved automatically on change.

## Capabilities

### New Capabilities

- `audio-gain`: how loud the assistant hears the channel and how loud it speaks, clipping,
  and the level readings.

### Modified Capabilities

- `app-settings`: the new `audio` section and its validation.
- `settings-ui`: the Audio card.

## Impact

- `crates/voicebro-core/src/settings` (section, defaults, validation), `gate.rs` (input
  gain, input peak and clip count), `audio/output.rs` (output gain at playout, peak and clip
  count), `runtime.rs` (applying the `audio` section to live gates and outputs),
  `apps/desktop/ui` (Audio card).
- No schema version change: a file without the section gets the defaults.
- Docs: `docs/ARCHITECTURE.md` (settings schema, audio contracts), `AGENTS.md`.
