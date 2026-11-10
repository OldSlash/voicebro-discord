# Proposal

## Why

The assistant's voice stutters in long answers. Measurements of 2026-11-10 show the cause:
the realtime server sends the voice 2–7 % slower than real time, in stalls of 60–314 ms
that it never catches up, while it processes speech or speaks. The lag reached 2.5 s in a
36 s call. The call's jitter buffer holds at most 160 ms and plays silence whenever it runs
dry, so every stall becomes a hole: 29 playout underruns in a 117 s session, 15 in a 36 s
test call. The handoff mode makes no difference.

The server does the same to Chromium. In a comparison call through Chromium's WebRTC stack,
as the first version used through WebView2, the lag was 2.65 s over 62 s. Chromium's NetEQ
hid it: it stretched 1.88 s of speech, concealed 1.18 s (0.67 s of it in pauses) and grew
its buffer from 56 to 365 ms. The first version never stuttered for that reason.

The faster delivery other bots rely on is not available here. With a ChatGPT sign-in, Codex
0.167.0 refuses the websocket transport ("realtime conversation requires API key auth",
checked live), and in a WebRTC call the app-server sends no `outputAudio/delta`.

The `neteq` crate (0.9.1) was read and rejected:
- it allocates on every 10 ms pull;
- it indexes slices on assumptions that would abort the app under `panic = "abort"`;
- it has no hook for Opus in-band FEC;
- it does not tell which packet is playing, which the turn latency line needs.

The app gets its own small playout instead.

## What Changes

- **Adaptive playout for the Codex call.** A new file in the Codex call module sits behind
  the existing `FrameProvider`. It keeps the voice continuous when the server is late:
  - the buffer depth follows the lateness measured against the RTP clock over the last
    seconds (from 60 ms up to a 500 ms ceiling), instead of the fixed 3 to 8 frames;
  - below that depth it stretches the voice: pauses first (an extra 20 ms of the pause),
    voiced speech only when there is no pause to use, by one pitch period without changing
    the pitch;
  - above that depth it shrinks the same way, so the delay does not keep growing;
  - when the buffer runs dry in the middle of speech, it bridges up to 60 ms with Opus loss
    concealment instead of silence, and then plays the late packet instead of dropping it.
- **The start of a call.** Added after the first implementation, where the very first stall
  of a call was still heard once. A call starts from the depth the guild's previous call
  learnt during this run of the app. The first call starts from 200 ms, which fades to the
  measured value on a steady server.
- **No fixed rate anywhere.** Nothing assumes how far behind the server is. The depth and
  the stretching follow what the buffer measures, so 2 % or 10 % behind are handled the same
  way.
- **New counters** in the call's audio counters, the end-of-call log line, the status and
  the Performance tab:
  - time stretched in pauses;
  - time stretched in speech;
  - time removed;
  - frames bridged after a dry buffer;
  - audible gaps in speech;
  - the depth now and at its highest.
  
  Loss concealment keeps its own counter, so the Performance tab's quality rule keeps its
  meaning.
- **A replay stand.** The gated live call test can record a call's packets: arrival time,
  sequence, timestamp and payload. A gated test then replays a recording through the playout
  on a simulated 20 ms clock and prints the counters and the added delay. It also writes a
  WAV, so the result can be heard before Discord.
- **Codex only.** The Discord output queue, the gate, the wake engine and any other engine
  are untouched. No new dependency and no new setting.

Out of scope:
- a user setting for a minimum voice delay (possible later, once the measured depth is
  known);
- changes to the send path toward the call;
- changes to the Performance tab's quality rule;
- an API-key websocket path.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `assistant-call-audio`: two requirements change.
  - The answer's playback: from "a short buffer, silence when dry" to a measured depth with
    stretching, shrinking and bridging.
  - The call audio counters: the new counters are added.

## Impact

- **Code:**
  - new `crates/voicebro-core/src/assistant/codex/playout.rs`;
  - `jitter.rs`: lateness per packet, decoded frames handed to the playout, bridging
    instead of a dry stop;
  - `rtc.rs`: counters and the log line;
  - `mod.rs`: the live test records, and a gated replay test;
  - `apps/desktop/ui/main.js`: new stat cards;
  - `apps/desktop/ui-dev/mock.js`: the new counters.
- **Behaviour:** when the server is uneven, the answer starts up to a few hundred
  milliseconds later. The turn latency line's "ChatGPT audio -> Discord relay handoff" shows
  that time, as it measures the time in the buffer.
- **Docs:**
  - `docs/ARCHITECTURE.md`: the receive part of 4.5 and the contract table;
  - `AGENTS.md`: the recording and replay variables.
- **Dependencies:** none added.
