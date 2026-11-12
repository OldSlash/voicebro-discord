# Design

## Context

See proposal.md. `AppEvent` is `#[serde(tag = "kind")]`; `AppEvent::Chat(Box<ChatEntry>)`
serializes the tag and then the entry's own `kind` field, so the JSON object holds `kind`
twice and the window's `JSON.parse` keeps the last one (`agent`, `user`, …). The Codex
chat is built from `CHAT_KIND_LABELS`, `chatElement`, `renderChat`, filters and copy as
text; the Console is a read-only text area with search, match count, Clear, Copy, Copy
selection and Auto-scroll. After change `first-version-parity` the window receives every
log record and can fetch the last 500.

## Goals / Non-Goals

**Goals:** a working chat tab with source labels and filters; the Console.

**Non-Goals:** rendering Markdown in agent messages (they are shown raw); changing what the
chat log records.

## Decisions

**D1. Rename the event tag to `event`.** `AppEvent` becomes `#[serde(tag = "event")]` and
the window switches on `payload.event`. *Alternative:* rename `ChatEntry::kind`. Rejected:
the daily chat log files use `kind` for the entry, and other payloads (wake state) may grow
a `kind` too. A unit test serializes one instance of every
`AppEvent` variant and checks that the `event` key appears once and holds the variant name.

**D2. Build the Codex chat view.** A label map, a row element, filters, a copy text format
and auto-scroll are written into the tab placeholder of `ui-layout-port`; rows of kind
`assistant` and `task` get a highlighted background. The list is capped at 600 rows like the
core's buffer.

**D3. The Console is a read-only text area** filled from the forwarded log (the buffer at
load, then each record), capped at 2 000 lines in the view; search highlights by selecting
matches and counts them; Clear empties the view only. A text area keeps Copy selection
native and cheap for thousands of lines.

## Risks / Trade-offs

- [Renaming the tag breaks any consumer of `kind`] → the window is the only consumer; the
  test pins the new shape.
- [A long session fills the view] → caps of 600 chat rows and 2 000 console lines; the files
  keep everything.
