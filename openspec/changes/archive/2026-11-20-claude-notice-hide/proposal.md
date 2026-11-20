# Proposal

## Why

On 2026-11-20 the user said the Claude subscription notice is right in what it says, but
it shows in two places (the Assistant and Access cards) and cannot be hidden. The user
asked for the dialog to stay as it is, shown once, and for the notice to show only in the
Claude Code card, with a Hide button that hides it for good: the app cannot make anyone
follow Anthropic's terms, and a user who does not care must be able to put it away.

## What Changes

- **The notice leaves the Assistant and Access cards** and shows only in the engine card
  while it is the Claude Code card, under the same conditions as today (Claude Code is the
  engine, its sign-in is a subscription or not known, the access mode is not
  "Only my Discord account").
- **The notice gets a Hide button** next to "Hear only me". Hide stores
  `claude.subscription_notice_hidden = true`, and the notice is not shown again.
- **The dialog stays as it is**: shown once, the first time Claude Code is picked under
  those conditions, with "Hear only me" and "Not now". "Not now" now leaves the notice in
  the Claude Code card.
- A new setting `claude.subscription_notice_hidden`, default `false`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `claude-assistant`: the notice shows in the Claude Code card only, and not after Hide.
- `settings-ui`: the notice's place and its Hide button; the dialog's "Not now" leaves the
  notice in the Claude Code card.
- `app-settings`: `claude.subscription_notice_hidden`.

## Impact

- The window: `apps/desktop/ui/index.html`, `main.js`; the command mock `ui-dev/mock.js`
  (a parameter for a hidden notice); `check-engine-config.mjs`.
- The core: the new field in `settings/mod.rs` with its tests.
- Docs: `docs/ARCHITECTURE.md` (the settings table), `docs/user-guide.md`, `AGENTS.md` (the
  mock's parameter).
- Depends on `models-in-settings` (its delta of the dialog's last scenario); starts from
  `main` after that change is merged.
