# Proposal

## Why

In the first real-channel session (build `0.2.0+19bcf29`, 2026-11-09) the assistant
misheard the user, answered fragments of sentences (a one-word "uh-huh" or "yes" after half a
sentence) and
spoke with stutters, while the first version (GPTVoice) handled the same game talk well.
The causes are in this app's call audio path and in one thread parameter taken from the
rewrite documents instead of the working code (`docs/NEXT-CHANGES.md`, section 1).

## What Changes

- The audio sent into the call is complete: every frame the gate passes reaches the call in
  order, on a steady 20 ms clock, with silence only when nothing is queued. Today the send
  thread keeps only the newest frame per tick and sends nothing on an empty tick, so speech
  is lost whenever songbird's tick and the send clock drift.
- The assistant's voice plays steadily: incoming packets go through a jitter buffer that
  reorders them, conceals a lost packet (Opus in-band FEC, else packet loss concealment)
  and plays out one 20 ms frame per tick. Today packets are decoded in arrival order and
  pushed straight into the output queue.
- The Opus encoder toward the call runs at a higher complexity and a fixed bitrate with
  in-band FEC, measured against transcript fidelity.
- Each call reports its audio counters (frames sent, packets received, late, lost,
  concealed, playout underruns, queue drops) in the status and in one log line at its end.
- The Codex thread uses the first version's `approvalPolicy: untrusted` instead of
  `never`; the agent keeps web search and is not restricted beyond acting on the computer.
- An empty `codex.voice` means the server's default voice, which is what the first version
  effectively used; the log names the voice actually used.
- songbird's RTCP decryption warnings stop flooding the log.

## Capabilities

### New Capabilities

- `assistant-call-audio`: the audio between the app and the assistant's call: completeness
  and order toward the call, steady playout of the answer, loss concealment, and the call's
  audio counters.

### Modified Capabilities

- `codex-assistant`: the thread's approval policy and what the agent may do; the voice
  setting gains a "server default" value.

## Impact

- `crates/voicebro-core/src/assistant/codex/rtc.rs` (send loop, receive path), a new jitter
  buffer module, `session.rs` (counters, voice), `thread.rs` (approval policy),
  `assistant/mod.rs` (`CallStatus` counters), `settings` (empty voice allowed),
  `apps/desktop/src/main.rs` (log level for songbird's receive task), the Settings voice
  list.
- No new dependencies: `opus2` already provides FEC decode and concealment.
- Docs: `docs/ARCHITECTURE.md` 4.5 (`rtc.rs`), `AGENTS.md` audio facts.
