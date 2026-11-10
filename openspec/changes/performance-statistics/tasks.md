# Tasks

## 1. Latency tracker (spec `call-latency`)

- [ ] 1.1 Create the branch `change/performance-statistics` from the tip of `change/chat-and-console-tabs`; verify `git log -1` shows the previous tip as parent
- [ ] 1.2 Write the latency state machine with the first version's rules and constants into the core, fed by timestamps (no clock inside, so it is testable); verify unit tests with synthetic timelines: two sentences 200 ms apart make one turn, an answer gives the four durations, speech during an answer reports one interruption and labels the next answer, an answer 20 s after the last speech is autonomous
- [ ] 1.3 Feed it from the gate's send toward the assistant and from `PcmSource` (first frame with signal received and played), log one line per turn with the first version's wording, and keep the last 50 turns in the status; verify the gated live call test prints a turn line and the status holds the turn

## 2. Performance tab (spec `performance-view`)

- [ ] 2.1 Build the tab in the `ui-layout-port` placeholder: window selector, Refresh, health dot and quality label, the two canvases with the first version's `drawChart`, stat cards for every counter of the status; sample the status once a second and keep 5 minutes; verify with the `ui-dev` mock feeding a clean call and then 3 % concealed packets: the label goes from "Quality: stable" to "Quality: degraded (concealed packets)", the charts redraw on a window change, every card shows a number
- [ ] 2.2 Check the tab against the running app with a scratch data dir and the gated live call test's numbers (the live session in the channel is the user's step); verify the cards match the call's end-of-call log line

## 3. Docs and checks

- [ ] 3.1 Update `docs/ARCHITECTURE.md` (observability: the tracker's points, the turn line, the Performance tab) and `AGENTS.md`; verify the turn line format is documented as written
- [ ] 3.2 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass and report their output

## Workflow follow-up

- Checkpoint commits per task group on `change/performance-statistics`, with the user's OK.
- Reading the tab during a real session is the user's step.
- Merged into `main` with `--no-ff --log`, in chain order, once the app works end to end and
  the user says so; the change is archived in the same step.
