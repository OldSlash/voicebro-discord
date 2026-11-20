# Design

## Context

See proposal.md for the request. Today `index.html` has two notice boxes with
`data-claude-notice` (`claude-notice-assistant` under the Engine list,
`claude-notice-access` under "Who the assistant hears"); `renderClaudeNotice` in `main.js`
fills them with `noticeParagraphs(box, true)` (text, advice, links, "Hear only me") and
shows them while `engineOf() === "claude"`, `noticeApplies(claudeNotice.kind)` and the
access mode is not `owner`. The dialog (`claude-notice-dialog`) uses the same paragraphs
without the button and saves `claude.subscription_notice_shown` when answered.

## Goals / Non-Goals

**Goals:**
- One notice box, in the Claude Code card, with Hide.

**Non-Goals:**
- The dialog's text, buttons and once-only rule stay as they are.
- No way to show a hidden notice again; the user did not ask for one.

## Decisions

### D1. One box at the top of the engine card

The two boxes go; one box with `data-claude-notice` and `data-engine="claude"` sits first
in the engine card's form grid, so it shows only while the card is the Claude Code card and
the existing `[data-engine]` rule hides it otherwise. `renderClaudeNotice` keeps its
conditions and adds `!stored.claude.subscription_notice_hidden`.

### D2. Hide is a stored flag

`noticeParagraphs(box, true)` adds a "Hide" button after "Hear only me". Hide saves
`{ claude: { subscription_notice_hidden: true } }` through `save_settings`; on success the
box hides, on failure the error shows under the box and the box stays. The flag is a new
boolean in `ClaudeSettings` with `#[serde(default)]`, written by the next save like the
other `claude` fields; a non-boolean value is refused by the existing deserialization with
the field's name. The dialog does not read the flag: it still opens once, as before.

Alternative: reuse `subscription_notice_shown`. Rejected: that flag means the dialog was
answered, which "Not now" also sets, while the notice must stay after "Not now".

## Risks / Trade-offs

- [A user hides the notice and later shares a subscription without noticing] → Their
  choice, as the user said; the dialog has already said it once and the help lines of the
  Access card still describe "Only my Discord account".
