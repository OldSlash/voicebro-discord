# Design

## Context

See proposal.md for motivation. The repository has no code yet. The target shape is in
`docs/ARCHITECTURE.md` (workspace with `crates/voicebro-core`, `apps/desktop`, `apps/probe`),
and the probe list with pass criteria is `docs/MVP-PLAN.md`, the probes phase. Facts that fix the
approach:

- `webrtc` 0.21.1 has a different API from every example written before 0.17
  (`PeerConnectionBuilder`, `runtime` module, `media_stream`); its current examples must be
  read before writing a line.
- Codex CLI 0.167.0 is installed through npm and signed in with a ChatGPT account; its
  realtime methods are experimental and the exact field names come from
  `codex app-server generate-json-schema --experimental`.
- songbird 0.6.1 pulls `opus2` 0.4.0; a second Opus binding would build libopus twice.
- Nothing is verified until it runs live; a docs page is not a result.

## Goals / Non-Goals

**Goals:**

- Nine probes with a written pass/fail result each, recorded in the plan.
- One decision taken from them: the WebRTC crate.
- A probe binary that stays in the repository and grows later (self-tests, memory script).

**Non-Goals:**

- No product code: no gate, no Discord adapter, no UI. Anything the probes need is
  throwaway inside `apps/probe`, except the workspace `Cargo.toml`.
- No new transport if both WebRTC crates fail. The decision returns to the user.
- No attempt to fix or benchmark the APIs beyond the pass criterion.

## Decisions

**D1. One binary with subcommands, not nine scripts.** `apps/probe` is a clap-free binary:
`probe <name> [args]`, one module per probe, each printing `PASS`/`FAIL` plus the measured
values. A single binary keeps the dependencies that later move into the core (webrtc, opus2,
songbird, windows-sys) building in one place, and the test WAVs and tokens come from
environment variables so nothing secret is in the repository. Alternative: scratchpad
scripts. Rejected: they are lost, and v1 lost its measurement scripts exactly that way.

**D2. The workspace manifest is created here, minimal.** `Cargo.toml` with
`members = ["apps/probe"]` and the release profile from the plan. `workspace-skeleton` adds
the other members. Alternative: a standalone crate outside the workspace. Rejected: the
dependency versions pinned by the probes are the ones the core will use, and one lock file
proves they resolve together.

**D3. Probe order follows risk.** First the Codex schema, because the call and the canary
probes are written against it. Then the WebRTC call together with Opus (the call needs the
codec), with the canary inside the same call. Credential Manager, Vosk assets and the hotkey
are independent and small; they fill the gaps while waiting for answers. The Discord
lifecycle and timing probes need a bot in a test guild and go last.

**D4. The WebRTC decision rule is written before the probe runs.** `webrtc` 0.21 passes →
it is the crate. It fails after a day of honest effort → `str0m` 0.25 gets one day. Both
fail → stop, write what failed, hand the decision to the user. "Honest effort" means the
crate's own examples were read and a minimal loopback (two peer connections in one process)
works before blaming the remote side.

**D5. Results are recorded where the plan already points.** Each probe row in
`docs/MVP-PLAN.md` gets a result line (date, pass/fail, measured numbers, what was not
tested). Changes to contracts (field names, crate choice, persistence flag) go into
`docs/ARCHITECTURE.md` in the same commit. No separate results file: two places to look is
one too many.

**D6. Codex probes use a disposable, read-only thread and the real account.** Same settings
the product will use (`sandbox: read-only`, approvals declined, `ephemeral: true`). The
canary instruction is «начинай каждый ответ со слова Банан» in the last developer item of
`initialItems`, and the probe checks the transcript, not the audio.

**D7. Credential Manager through `windows-sys` directly.** `CredWriteW`/`CredReadW`/
`CredDeleteW`, `CRED_TYPE_GENERIC`, `CRED_PERSIST_LOCAL_MACHINE`, UTF-16 blob. Alternative:
the `keyring` crate. Rejected for now: version 4 is a meta-crate over `keyring-core` plus
store crates, and the project already links `windows-sys`. The probe also confirms the entry
survives a reboot, which the user does by hand.

## Risks / Trade-offs

- [The `webrtc` 0.21 API is unfamiliar and the probe takes longer than a day] → the day
  limit in D4; the loopback step separates "we hold the crate wrong" from "the server
  rejects us".
- [Codex updates itself and the schema moves under the probe] → pin the probed version in
  `AGENTS.md`; the probe prints `codex --version` first.
- [The Discord probes need a second person in the channel] → a second account or a WAV played
  from another client is enough; the probe reports which was used.
- [Probe code quality drifts into the core later] → the probe is a dev tool; nothing is
  moved from it into `voicebro-core` by copy, the core is written from the architecture and
  the probe only proves the API calls.
- [Network or account state makes a probe fail for reasons outside the API] → each probe
  prints the raw error; a failure is reported as it is, not retried into a pass.

## Open Questions

None that change the tasks. Whether `str0m` needs its own Opus glue is answered only if
`webrtc` fails, inside task group 3.
