# Proposal

## Why

Every setting of the MVP must be reachable from the window, not from `settings.json` by
hand: server id, mode, idle timeout, phrases, wake model, Codex model, reasoning effort,
voice, instructions, speaker labels, earcons, hotkey. The Status tab must show what the bot
is doing with measured values only, and the Chat tab must show the conversation. Then the
whole product is tested the only way that counts here: a real channel with two or more
people and a game running, with the memory and CPU measurements the plan calls for.

## What Changes

- Settings tab: every field of `Settings`, saved on change through `save_settings` patches,
  validation messages next to the field, applied at once. Model and reasoning lists filled
  from `model/list` when Codex is available, with free text as the fallback; voices from
  `listVoices`; wake model from the catalog; a note that model and reasoning affect only the
  work the voice model delegates.
- Status tab: bot, voice channel, assistant state (waiting / listening / muted), wake state
  with unknown words, Codex account with Sign in and Check, hotkey, Mute button, counters
  behind a details toggle, build stamp.
- Chat tab as specified in `assistant-chat-log`.
- Acceptance: the full checklist from the plan run in the test guild with two people and a
  game; the memory and CPU measurements; the release profile decision; a version bump to
  0.2.0 and a tag on `main` after the merge.

## Capabilities

### New Capabilities

- `settings-ui`: the window's settings form and how it saves and reports.
- `status-view`: what the Status tab shows and the rule that nothing unmeasured appears.

### Modified Capabilities

None. The Chat tab's behavior is in `assistant-chat-log`.

## Impact

- `apps/desktop/ui/` grows from the shell to the full UI; new commands only where the Codex
  change did not already add them.
- `docs/MVP-PLAN.md` gains the acceptance results and measurements; `docs/ARCHITECTURE.md`
  4.8 is checked against the code.
- A tag `v0.2.0` on `main` after the merge, matching the stamp.
