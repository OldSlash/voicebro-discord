# Tasks

## 1. Latency tracker (spec `call-latency`)

- [x] 1.1 Create the branch `change/performance-statistics` from the tip of `change/chat-and-console-tabs`; verify `git log -1` shows the previous tip as parent
- [x] 1.2 Write the latency state machine with the turn rules and constants into the core, fed by timestamps (no clock inside, so it is testable); verify unit tests with synthetic timelines: two sentences 200 ms apart make one turn, an answer gives the four durations, speech during an answer reports one interruption and labels the next answer, an answer 20 s after the last speech is autonomous (`latency.rs`, plus a test that the status keeps 50 turns)
- [x] 1.3 Feed it from the gate's send toward the assistant and from `PcmSource` (first frame with signal received and played), log one line per turn, and keep the last 50 turns in the status; verify the gated live call test prints a turn line and the status holds the turn (the live call printed "turn 1 (response): first Discord speech -> ChatGPT audio 3423 ms; last Discord speech -> ChatGPT audio 1602 ms; ChatGPT audio -> Discord relay handoff 65 ms; first speech -> Discord relay handoff 3489 ms" and a second turn at 3825, 1475, 34 and 3860 ms; the runtime test `a_turn_is_measured_from_the_speech_sent_to_the_answer_played` checks the status through the gate and the output. "Received" is the packet's network arrival, see design D1)

## 2. Performance tab (spec `performance-view`)

- [x] 2.1 Build the tab in the `ui-layout-port` placeholder: window selector, Refresh, health dot and quality label, the two canvases with a `drawChart` helper, stat cards for every counter of the status; sample the status once a second and keep 5 minutes; verify with the `ui-dev` mock feeding a clean call and then 3 % concealed packets: the label goes from "Quality: stable" to "Quality: degraded (concealed packets)", the charts redraw on a window change, every card shows a number (24 cards, all numbers; the charts differ between 30 s and 5 min once samples older than 30 s exist)
- [x] 2.2 Check the tab against the running app with a scratch data dir and the gated live call test's numbers (the live session in the channel is the user's step); verify the cards match the call's end-of-call log line (in the running app without a call the tab reads "Quality: idle" and every card is a number; the live call test runs without the window, so matching the cards with a call's log line is the user's step in a channel)

## 3. Docs and checks

- [x] 3.1 Update `docs/ARCHITECTURE.md` (observability: the tracker's points, the turn line, the Performance tab) and `AGENTS.md`; verify the turn line format is documented as written
- [x] 3.2 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass and report their output (fmt clean; clippy no errors, the same 6 warnings in code this change does not touch; tests: core 136, desktop 3, all pass)

## Workflow follow-up

- Checkpoint commits per task group on `change/performance-statistics`, with the user's OK.
- Reading the tab during a real session is the user's step.
- Merged into `main` with `--no-ff --log`, in chain order, once the app works end to end and
  the user says so; the change is archived in the same step.
