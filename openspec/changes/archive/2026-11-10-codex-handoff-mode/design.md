# Design

## Context

See proposal.md for the motivation and the test results. Today
`realtime_start_params` (`assistant/codex/session.rs`) builds `thread/realtime/start` with
`threadId`, `outputModality`, `version: "v3"`, `prompt`, `includeStartupContext`, the WebRTC
transport and an optional `voice`, and nothing about how agent answers reach the voice
model. `start_with_history` already retries a rejected start without the conversation
history. The Codex settings reach a session through the `CodexConfig` snapshot that the
session reads at each call start; the Settings tab lists save themselves through the form
model of change `settings-form-behavior`.

The Codex CLI 0.167.0 schema describes the fields (`ThreadRealtimeStartParams`):
`codexResponseHandoffMode` (`thinking`, the default when omitted; `commentary`; `bemTags`)
"selects how automatic Codex responses are routed", and `codexResponsesAsItems` "sends
automatic Codex responses as realtime conversation items instead of handoff appends". Both
are marked experimental with the rest of realtime v3. The 20 live calls on 2026-11-10 used
exactly these fields through a test-only hook that was removed afterwards.

## Goals / Non-Goals

**Goals:** the routing is a user setting with the tested-best default; each choice is
explained where it is chosen; the log shows which one a call used.

**Non-Goals:** forwarding the agent's answer from the app (`clientManagedHandoffs`);
switching off the "I'll check" filler; the voice's "I put it on the screen" claims; reading
the agent's answer word for word, which none of the modes does.

## Decisions

**D1. Values in snake_case, mapped to Codex's names.** `codex.handoff_mode` takes `items`,
`commentary`, `bem_tags` and `thinking`, as the other enum settings are snake_case
(`wake_word`, `speaker_labels`). The mapping lives next to `realtime_start_params`:

| Setting | `thread/realtime/start` |
|---|---|
| `items` (default) | `codexResponsesAsItems: true` |
| `commentary` | `codexResponseHandoffMode: "commentary"` |
| `bem_tags` | `codexResponseHandoffMode: "bemTags"` |
| `thinking` | neither field |

`thinking` sends nothing rather than `"thinking"`, so it stays exactly what the app sent
before this change. `items` sends no handoff mode either:
the tested combination was `codexResponsesAsItems` alone. *Alternative:* store Codex's own
names. Rejected: two field names behind one choice, and the setting would break if Codex
renamed a value.

**D2. A string validated in one place, not a serde enum.** As for `ui.theme`, the field is a
string checked by `Settings::validate`, so a wrong value is reported with the field name and
the allowed values instead of a parse error. A missing field takes `items` through the
section's `Default`.

**D3. Applied at the next call.** `CodexConfig` gains the mode; `configure` already swaps the
snapshot, and a call reads it when it starts. A running call is not restarted: the mode is a
start parameter and a restart would cut the conversation.

**D4. Fallback when Codex rejects the field.** If `thread/realtime/start` fails while a mode
other than `thinking` was sent, the start is retried once without the handoff fields, before
the existing retry without history, and `log_used` names the requested mode, `thinking` and
Codex's error. The fields are experimental; a Codex update must not stop voice from working.

**D5. The start line names the mode.** The existing "Call starting: voice …; Codex: …" line in
the log and the chat gains "answers: <label>", so a session report shows which routing was
in effect.

**D6. The control and its hints.** A select in the Codex card, next to Voice, with these
labels and hints (English, as all UI text); the hint of the selected value is shown in the
field help under it and updates on change:

| Value | Label | Hint |
|---|---|---|
| `items` | Conversation items (recommended) | The agent's answer is added to the conversation as its own message and the voice retells it. In the tests it kept the agent's facts every time. |
| `commentary` | Commentary | The agent's answer reaches the voice as commentary. In the tests the facts were kept; once the voice gave the agent the same task twice, once it acknowledged in English. |
| `bem_tags` | Tagged channels | The agent's answer reaches the voice with channel tags (analysis, commentary, final). In the tests the facts were kept, sometimes shortened. |
| `thinking` | Built-in (thinking) | Codex's own routing, used before this setting existed: the answer is hidden context and the voice speaks in its own words. In the tests it twice made up an answer before the agent had finished. |

The list saves on change through the form model; the `ui-dev` mock gets the field, its
default and its validation message so the control can be checked without the app.

**D7. The live test can run each mode.** `live_call_when_enabled` reads
`VOICEBRO_CODEX_HANDOFF_MODE` into the settings it builds the engine with (no hook in the
app's code), and after a delegated question it waits for the task to finish and then for the
voice's retelling, as the analysis did. It prints the agent's answer and the retelling.

## Risks / Trade-offs

- [The fields are experimental and may change in a later Codex CLI] → D4 falls back to the
  built-in routing and logs it; the Codex version stays in the status.
- [Twenty calls, one per question and mode, is a small sample] → `items` is a default, not a
  lock; every mode stays selectable, and the live test can repeat the comparison.
- [Existing users get `items` without choosing it] → intended by the user; the start line
  names the mode and the old behaviour is one choice away.

## Migration Plan

No schema version change: a `codex` section without `handoff_mode` loads as `items` and the
next save writes the field. Rollback is choosing "Built-in (thinking)" or reverting the
branch.
