# Proposal

## Why

When the voice model hands a question to the Codex agent, the agent's answer reaches the
voice model through Codex's default routing (`thinking`), and the voice model then speaks
its own version. In a real session and in 20 live calls on 2026-11-10 (five delegated
questions in four routing modes, report kept uncommitted in
`docs/HANDOFF-MODES-REPORT.md`) the default routing was the one that misled: twice the
voice answered with made-up facts before the agent had finished, and once it changed a
number when retelling. With `codexResponsesAsItems` the retelling kept the agent's facts in
every call. The first version used the same default routing and had the same problem. The
user wants the routing to be a setting, defaulting to the mode that tested most accurate.

## What Changes

- A new setting `codex.handoff_mode`: how the Codex agent's answer reaches the voice model.
  Four choices: `items` (the answer is added to the conversation as its own item;
  **default**), `commentary`, `bem_tags`, and `thinking` (Codex's built-in default, what the
  app sends today).
- The realtime call is started with the matching `thread/realtime/start` field:
  `codexResponsesAsItems: true` for `items`, `codexResponseHandoffMode: "commentary"` or
  `"bemTags"` for the two handoff modes, and neither field for `thinking`. A change applies
  from the next call.
- The call's start line in the log and in the Codex chat names the mode in use.
- The Settings tab's Codex card gets an "Answer handoff" list with a short hint for each
  choice saying what it does and how it behaved in the tests, the hint of the chosen one shown
  under the list. It is part of the Codex card, so it shows only for the ChatGPT voice
  through Codex engine.
- Existing settings files without the field take the default, `items`, so the change of
  behaviour reaches existing users at their next call.
- The gated live call test waits for the voice model's retelling after a delegated task (it
  ends before it today), so each mode can be checked live.

Out of scope: the voice claiming it "put a link on the screen" (seen in all modes when the
agent's answer had a link); the `clientManagedHandoffs` mode, where the app itself would
forward the agent's answer; the `delegationAckFiller` switch.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `codex-assistant`: the call is started with the configured answer handoff mode, and the
  call's start line names it.
- `app-settings`: the `codex.handoff_mode` field, its values, its default and its validation.
- `settings-ui`: the "Answer handoff" control with per-choice hints in the Codex card.

## Impact

- `crates/voicebro-core/src/settings` (field, default, validation),
  `assistant/codex/mod.rs` (`CodexConfig`), `assistant/codex/session.rs`
  (`realtime_start_params`, the call start line), the live call test in
  `assistant/codex/mod.rs`.
- `apps/desktop/ui` (`index.html`, `main.js`: the list and its hint) and the `ui-dev` mock
  (the field in its settings document and its validation).
- Docs: `docs/ARCHITECTURE.md` 4.5 (realtime start parameters) and 4.6 (settings schema),
  `AGENTS.md` (the chain of branches gains this change).
- Branch: `change/codex-handoff-mode`, chained from the tip of `change/memory-measurement`.
