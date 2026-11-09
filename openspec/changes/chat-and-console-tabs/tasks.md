# Tasks

## 1. Events (spec `assistant-chat-log`)

- [ ] 1.1 Create the branch `change/chat-and-console-tabs` from the tip of `change/settings-form-behavior`; verify `git log -1` shows the previous tip as parent
- [ ] 1.2 Rename the `AppEvent` tag to `event`, switch the window to `payload.event`, and add the test that serializes every variant and finds `event` once with the variant name; verify the test passes and a chat entry of kind `agent` serializes with `event: "chat"` and `kind: "agent"`

## 2. Codex chat tab (spec `assistant-chat-log`)

- [ ] 2.1 Port the first version's chat view (label map, rows, highlights, filters, Copy, Open log folder, Auto-scroll, 600-row cap) into the Codex chat tab; verify with the `ui-dev` mock emitting one entry of each kind: every label appears as specified, the agent-work filter hides task, agent, reasoning and tool rows, Copy yields one line per shown row
- [ ] 2.2 Check live entries with the gated Codex live test running against the app's event bus (or the running app with a scratch data dir and the user's session): rows appear within a second and nothing is printed as raw JSON in the Console; verify by the test output or the user's session log

## 3. Console tab (spec `app-console`)

- [ ] 3.1 Build the Console from the forwarded log: the buffer at load, then each record, 2 000-line view cap, search with match count, Clear, Copy, Copy selection, Auto-scroll; verify with the mock: typing "heard" counts the matching lines, Clear empties only the view, Auto-scroll off keeps the position while lines arrive

## 4. Docs and checks

- [ ] 4.1 Update `docs/ARCHITECTURE.md` (the event tag, the two tabs); verify no text still names `kind` as the event tag
- [ ] 4.2 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass and report their output

## Workflow follow-up

- Checkpoint commits per task group on `change/chat-and-console-tabs`, with the user's OK.
- The user compares both tabs with the first version's screenshots.
- Merged into `main` with `--no-ff --log`, in chain order, once the app works end to end and
  the user says so; the change is archived in the same step.
