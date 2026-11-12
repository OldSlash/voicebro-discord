# Tasks

## 1. Measuring correctly

- [x] 1.1 Create the branch `change/memory-measurement` from the tip of `change/performance-statistics`; verify `git log -1` shows the previous tip as parent
- [x] 1.2 Extend `scripts\measure.ps1`: private working set (performance counter) next to private bytes and working set for the app, its WebView2 and Codex processes, refuse a debug build without `-AllowDebug`, print the build stamp; verify a run against the release build shows the three columns and a run against `target\debug` refuses (the debug run exited with code 2 and the reason)
- [x] 1.3 Measure the release build three times each in idle always mode and idle wake-word mode, replace the measurement table and the conclusion in `docs/MVP-PLAN.md` (and the wrong sentence about "well above the expected 150–200 MB"); verify the plan no longer cites 434 MB as the app's memory (always mode: 4.8 MB for `voicebro.exe`, about 91 to 95 MB with WebView2; wake-word mode: 169.8 to 169.9 MB, about 255 to 259 MB with WebView2; 434 MB is mentioned only as the earlier wrong reading)

## 2. Memory in the window (spec `app-window`)

- [x] 2.1 Add the memory reading in the shell (process tree, `PROCESS_MEMORY_COUNTERS_EX2`, every 3 s on a blocking thread) and expose it to the window; verify a unit test of the tree filter with a fake process list and a run where the reading is within 10 % of `scripts\measure.ps1` (`memory.rs` with unit tests of the tree and the sum; the window's reading matched the script within 1 % in all six runs)
- [x] 2.2 Show the total in the status bar and the split on the System tab's "Native host" card; Codex shown as not running when stopped; verify in the running app with a scratch data dir before and after `/join` is simulated by starting Codex through Check (before: "not running", total 260.8 MB; after Check: Codex 25.6 MB, total 286.6 MB; a minute later Codex stopped and showed "not running" again)

## 3. Docs and checks

- [x] 3.1 Update `docs/ARCHITECTURE.md` (observability: the memory metric) and `AGENTS.md` (the measuring command and the metric); verify both name the private working set
- [x] 3.2 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass and report their output (fmt clean; clippy no errors, the same 6 warnings in code this change does not touch; tests: core 136, desktop 6, all pass)

## Workflow follow-up

- Checkpoint commits per task group on `change/memory-measurement`, with the user's OK.
- Measuring during a call with people in the channel is the user's step.
- Merged into `main` with `--no-ff --log`, in chain order, once the app works end to end and
  the user says so; the change is archived in the same step.
