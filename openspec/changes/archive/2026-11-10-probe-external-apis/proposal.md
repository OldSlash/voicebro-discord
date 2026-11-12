# Proposal

## Why

The MVP plan ([docs/MVP-PLAN.md](../../../docs/MVP-PLAN.md), the probes phase) builds on
external APIs that nobody has exercised from this code base: the `webrtc` 0.21 crate with its
new API, the experimental realtime methods of Codex CLI 0.167.0, the Windows Credential
Manager, songbird 0.6.1 lifecycle details and a global hotkey under a full-screen game. A
wrong assumption about such an API can cost days (for example that websocket realtime works
with a ChatGPT sign-in, or that the session `prompt` is used). Every later change depends on
these answers, so they are collected first, in code that stays in the repository.

## What Changes

- Add the headless `apps/probe` binary and the first Cargo workspace manifest that hosts it.
  It is a development tool, not shipped.
- Run nine probes, each with a written pass criterion: Codex realtime schema, WebRTC call
  from Rust, canary instruction in `initialItems`, `opus2` encode/decode, Credential Manager
  round trip, Vosk asset pins, songbird `remove` and track end, receive-path timing, global
  hotkey under a game.
- Record every result in `docs/MVP-PLAN.md` (dependencies and the probe results) and, where
  a result changes a contract, in `docs/ARCHITECTURE.md`.
- Decide the WebRTC crate (`webrtc` 0.21 or `str0m` 0.25) from the probe result. If neither
  completes the call, stop and return the decision to the user; no other transport is
  tried.

## Capabilities

### New Capabilities

None. The probe binary is development tooling; it defines no product behavior. The change
sets `skip_specs: true`. Behavior that the probes confirm is specified in the later changes
that implement it.

### Modified Capabilities

None.

## Impact

- New files: `Cargo.toml` (workspace), `apps/probe/`, probe WAV fixtures outside the
  repository (paths through environment variables).
- Documents updated with results: `docs/MVP-PLAN.md`, `docs/ARCHITECTURE.md`, `AGENTS.md`
  (commands section: how to run a probe).
- Requires the installed Codex CLI 0.167.0 with a ChatGPT sign-in, a Discord bot token and a
  test guild for the voice probes, and network access for the Vosk downloads.
- No product code is written; the skeleton change (`workspace-skeleton`) follows and may
  reuse the workspace manifest.
