# Proposal

## Why

The assistant's voice reaches people over two network legs: OpenAI → the bot, and the bot →
Discord's voice server → each listener. Change `codex-adaptive-playout` made the first leg
measurable and smooth. The second is neither measured nor protected.

**Two calls on 2026-11-10 show the second leg decides what people hear:**
- **India region:** stutter and worse sound.
- **Singapore region**, ping about 30: much better.

On the OpenAI side the Singapore call was as uneven or worse: the playout depth reached its
500 ms ceiling, about 6 % of the time was inserted, and there were no gaps. Our counters cannot
see the Discord leg at all.

**Why that leg breaks up.** songbird encodes our voice for Discord itself: Opus at 128 kbit/s
in `Application::Audio`, with no in-band FEC. A packet lost on the way to Discord or to a
listener cannot be rebuilt from the next one, so the listener's client conceals it. The
longer and noisier the route, the more often that is heard.

## What Changes

- **The Discord leg is measured.**
  - The voice server's endpoint and region are logged at the join and shown in the channel
    status.
  - Discord's RTCP packets are taken through songbird's `RtcpPacket` event. When they report
    on our outgoing stream, the loss and jitter of the assistant's voice on that leg are
    derived from them.
  - A live probe first finds out what Discord actually sends.
- **The voice toward Discord is encoded by the app, with in-band FEC.** The output builds 20
  ms Opus packets with its own encoder, and songbird sends them as they are (its passthrough
  mode). Everything done to the voice today stays in our PCM before the encoder: playout,
  output gain, earcons, mute.
- **The protection follows the measurement.** The encoder's expected-loss setting, which
  decides how much redundancy each packet carries, comes from the measured loss of the
  Discord leg. When Discord reports nothing, a cautious default applies, and the counters say
  so.
- **Counters for the Discord leg** in the status, the log and the Performance tab:
  - packets encoded;
  - the expected loss in use and where it came from;
  - the reported loss and jitter;
  - whether songbird passes our packets through or re-encodes them.
- **Not engine-specific.** It covers the assistant's voice toward Discord whatever the
  assistant engine. Receiving people's voices does not change.

Out of scope:
- choosing or changing the channel's voice region;
- the OpenAI-leg playout regulator (a separate step of `codex-adaptive-playout`);
- adaptive bitrate;
- Opus DTX.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `audio-transport`: new requirements for how the assistant's voice is encoded and delivered
  to Discord, for measuring that leg, and for its counters.

## Impact

- **Code:**
  - `crates/voicebro-core/src/audio/output.rs`: the output source gives 20 ms Opus packets
    from the same PCM it builds today;
  - `crates/voicebro-core/src/discord/session.rs`: the input handed to songbird, the RTCP and
    connect events, the endpoint;
  - `crates/voicebro-core/src/discord/mod.rs`: the channel status;
  - a new module for the encoder and the Discord-leg estimate;
  - `apps/desktop/ui/main.js` and the `ui-dev` mock: stat cards.
- **Dependencies:** none new. `opus2` and `symphonia-core` are already used.
- **Docs:** `docs/ARCHITECTURE.md` (the output path, the thread table, the counters, the
  contract table) and `AGENTS.md` (the probe).
- **Risk:** if songbird stops passing our packets through, it would decode and re-encode
  them without FEC. A counter shows this, and a test checks the conditions.
