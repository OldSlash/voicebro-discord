# Design

## Context

See proposal.md. `AppEvent` is `#[serde(tag = "kind")]`; `AppEvent::Chat(Box<ChatEntry>)`
serializes the tag and then the entry's own `kind` field, so the JSON object holds `kind`
twice and the window's `JSON.parse` keeps the last one (`agent`, `user`, …). The first
version's Codex chat (the owner's chat code: `CHAT_KIND_LABELS`, `chatElement`,
`renderChat`, filters, copy as text) and its Console
(a read-only text area with search, match count, Clear, Copy,
Copy selection, Auto-scroll) are the reference. After change `first-version-parity` the
window receives every log record and can fetch the last 500.

## Goals / Non-Goals

**Goals:** a working chat tab with the first version's labels and filters; the Console.

**Non-Goals:** rendering Markdown in agent messages (the first version shows it raw);
changing what the chat log records.

## Decisions

**D1. Rename the event tag to `event`.** `AppEvent` becomes `#[serde(tag = "event")]` and
the window switches on `payload.event`. *Alternative:* rename `ChatEntry::kind`. Rejected:
the daily chat log files and the first version's code use `kind` for the entry, and other
payloads (wake state) may grow a `kind` too. A unit test serializes one instance of every
`AppEvent` variant and checks that the `event` key appears once and holds the variant name.

**D2. Port the Codex chat view.** The first version's label map, row element, filters, copy
text format and auto-scroll are ported into the tab placeholder of `ui-layout-port`; rows of
kind `assistant` and `task` get the highlighted background as in its screenshot. The list
is capped at 600 rows like the core's buffer.

**D3. The Console is a read-only text area** filled from the forwarded log (the buffer at
load, then each record), capped at 2 000 lines in the view; search highlights by selecting
matches and counts them; Clear empties the view only. A text area keeps Copy selection
native and cheap for thousands of lines.

## Risks / Trade-offs

- [Renaming the tag breaks any consumer of `kind`] → the window is the only consumer; the
  test pins the new shape.
- [A long session fills the view] → caps of 600 chat rows and 2 000 console lines; the files
  keep everything.
