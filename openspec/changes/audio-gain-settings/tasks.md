# Tasks

## 1. Settings (spec `app-settings`)

- [ ] 1.1 Create the branch `change/audio-gain-settings` from the tip of `change/voice-path-quality`; verify `git log -1` shows the previous tip as parent
- [ ] 1.2 Add the `audio` section (`input_gain`, `output_gain`, default 1.0, range 0.0 to 4.0) with validation naming the field and the range, and `changed_sections` reporting `audio`; verify unit tests: valid values, 5.0 rejected with the field name, a file without the section loads with 1.0 and saves the section

## 2. Gains in the audio path (spec `audio-gain`)

- [ ] 2.1 Apply the input gain in the gate right before sending to the assistant (live and replayed frames), keep the wake feed and the pre-roll unscaled, count input clips and keep the per-second input peak; verify unit tests: gain 2.0 doubles the sent samples while the wake feed is unchanged, 0.0 sends silence, 4.0 on 10 000 clamps to 32 767 and counts one clip, 1.0 leaves the frame unchanged
- [ ] 2.2 Apply the output gain in `PcmSource` before the earcon is mixed, clamp to -1.0..1.0, count output clips and keep the per-second output peak; verify unit tests: gain 2.5 scales pushed frames and jitter-buffer frames, an earcon plays at its volume under gain 4.0, a clamp is counted
- [ ] 2.3 Apply a saved `audio` section to the shared gains in `Runtime::apply` and put both peaks and clip counters in the status; verify a runtime test: after a save the next frame sent and the next frame played use the new gains, no gate or output is rebuilt, and the status carries the four readings

## 3. Audio card (spec `settings-ui`)

- [ ] 3.1 Add the Audio card to the Settings tab: two sliders 0 to 400 % with live percentage labels, saved automatically on release, the earcon switch and volume moved into the card, the error shown next to a slider on a rejected save; verify by running the app with a scratch data dir and checking `settings.json` after the user moves the sliders (the window itself is the user's step)

## 4. Docs and checks

- [ ] 4.1 Update `docs/ARCHITECTURE.md` (settings schema with `audio`, the audio contracts: where each gain applies) and `AGENTS.md`; verify the schema block lists the section
- [ ] 4.2 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass and report their output

## Workflow follow-up

- Checkpoint commits per task group on `change/audio-gain-settings`, with the user's OK.
- Hearing the effect in the channel is the user's step.
- Merged into `main` with `--no-ff --log`, in chain order, once the app works end to end and
  the user says so; the change is archived in the same step.
