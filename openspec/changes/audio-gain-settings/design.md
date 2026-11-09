# Design

## Context

See proposal.md. The first version multiplied the samples sent toward the assistant by the
input gain (`audio.rs` line 657) and the samples played into Discord by the output gain at
the float conversion in its reader (line 1194), both clamped to 0..4 and kept in atomics so a
change applied at once (`AUDIO_INPUT_VOLUME`, `AUDIO_OUTPUT_VOLUME` in `config.rs`, default
1.0; the UI slider shows the value times 100 as a percentage). In this app the frames toward
the assistant leave the gate through `FrameSender::send`; the audio toward Discord leaves
through `PcmSource`, which after change `voice-path-quality` pulls either pushed frames or
the call's jitter buffer and mixes earcons in.

## Goals / Non-Goals

**Goals:** the two gains with the first version's range and behaviour; live changes; a
visible sign of clipping.

**Non-Goals:** automatic gain control, compression or limiting; per-speaker gains; moving
the earcon fields out of the `session` section.

## Decisions

**D1. Input gain at the gate's send.** The gate multiplies the frame it is about to send to
the assistant (live audio and replayed pre-roll alike) right before `FrameSender::send`;
the frame given to the wake feed and kept in the pre-roll stays unscaled. The gain lives in
an atomic in the `RelayRegistry`, shared by every gate. *Alternative:* scale in the call's
send thread. Rejected: the stub assistant and any later engine would miss it.

**D2. Output gain at playout.** `PcmSource` multiplies the assistant's samples when it turns
them into floats for songbird, before mixing the earcon in, as the first version's reader
did. The gain lives in an atomic shared through `OutputHandle`, so a change reaches the next
frame read. *Alternative:* scale at `OutputHandle::push`. Rejected: frames already queued
(up to 160 ms) would keep the old gain, and the pulled jitter buffer would bypass it.

**D3. Clamping and readings.** Scaling is done in `f32` and clamped once to the 16-bit range
(input) or to -1.0..1.0 (output); each clamp increments that direction's clip counter. Each
direction keeps the peak of the current one-second window; the status shows the last
completed window as a share of full scale.

**D4. Settings without a version bump.** A new `audio` section with `#[serde(default)]`; an
older file loads with 1.0 and 1.0 and the next save writes the section (the migration rule
of `app-settings` already covers this). Defaults stay 1.0 as in the first version; the user
sets 339 % and 252 % once on the card. The earcon fields stay in `session`; the Audio card
only groups them.

**D5. Applying a save.** `Runtime::apply` handles the `audio` section by storing both gains
in the shared atomics; no gate, output or call is rebuilt.

**D6. UI before the layout port.** The Audio card is added to the current Settings tab with
two range inputs and percentage labels, saved on `change` (release of the slider) and the
label updated on `input`. Change `ui-layout-port` carries the card into the new layout
unchanged in behaviour.

## Risks / Trade-offs

- [High gains clip loud speech and distort recognition] → the clip counters and peaks show
  it; the defaults are neutral.
- [The output gain also scales the stub assistant's audio] → intended: it scales whatever
  the assistant says.
