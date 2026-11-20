# Design

## Context

See `proposal.md` and `findings.md`; the ids below (D-1, P-1, L-1 …) are defined in
`findings.md`.

- **Release script.** `scripts\build-release.ps1` builds only from `main` at the tag
  `v<version>` with a clean tree. Before writing it deletes `target\release\dist\`, which
  today holds the 1.0.1 installer and zip of 2026-11-14.
- **Version.** It is 1.3.0 in `tauri.conf.json` and both `Cargo.toml` files.
- **Publishing.** `gh` is not signed in on this PC, so the user publishes the GitHub
  release.
- **Commit.** "One commit per OpenSpec change" puts the docs, the notices script, the
  release notes and the archived change in one commit on `change/release-1-3-0`. The merge,
  the tag and the build follow it.

## Goals / Non-Goals

**Goals:**
- Docs, privacy policy template and notices that state what the 1.3.0 code does and what
  the licenses ask.
- A release built and checked with the release script, with notes the user can paste as
  they are.

**Non-Goals:**
- **No change to the app's code or behavior** (the user's decision of 2026-11-20). The
  data-handling findings are written down, not fixed, and no voice leaves the catalog.
- No version change and no code signing.
- The agent publishes nothing: no push, no GitHub release.

## Decisions

### D1. Docs and notices only, in one change

**Decision.** Everything goes into one change: the docs fixes, the privacy facts, the
license texts, the release notes and the update check. It lands in one commit, before the
tag.

**Why.** All of it must be in place before the tag, and none of it changes the app. That
includes the window's own help text that says posts carry no question
(`index.html:472`): it is app code and stays as it is, while the user guide and the
template say how it really works.

### D2. A license text for every crate

**Order.** For each crate the notices script uses:

1. the package's own license files;
2. otherwise, the standard text of one of the crate's licenses, with the crate's
   `authors` named as its copyright holders. These are the SPDX texts for MIT, ISC and
   BSD-3-Clause, and the official texts for Apache-2.0 and MPL-2.0, kept once each under
   `scripts\notices\licenses\<SPDX>.txt`. Each text is printed once at the end of section
   1 and referenced from each crate by name.

When neither applies, the script stops and names the crate.

**How it got here.** The first run found 22 crates without a text:

- **The 13 from `findings.md` L-1, plus `webview2-com-macros`.** These are under MIT, ISC
  or BSD-3-Clause.
- **8 MPL-2.0 crates.** These are `hpke-rs`, `hpke-rs-crypto`, `hpke-rs-rust-crypto`,
  `selectors` and four `symphonia` crates.

A first version kept a copy of each upstream license file per crate and version under
`scripts\notices\crates\`. On 2026-11-20 the user asked for it to be standardized
instead: no folder per crate. The standard texts do it the way cargo-about does, and a
crate update needs no new copy.

### D3. Voices written down with what they come from

For each catalog voice the notices script's voice list gets:

- the data license;
- the model it was fine-tuned from and that model's data license;
- for CC BY, the attribution (creator, dataset, link).

**Source.** The voice's MODEL_CARD: the copy in the pinned archive, which is the
piper-voices revision it was converted from, or the upstream card at
`rhasspy/piper-voices` v1.0.0. An archive is downloaded into `target\` only when its
revision cannot be told otherwise.

**Wording.** As the card says it ("fine-tuned from lessac; lessac data: research use",
"dataset license: unknown"). The user guide's voice part points to the notices. The
catalog in the code is unchanged.

### D4. The privacy facts in the template and the guide

**What goes in.** Each fact of P-1 to P-7 becomes one or two plain sentences in the
privacy part of `docs/user-guide.md` and in the matching part of
`docs/PRIVACY-POLICY-TEMPLATE.md`, which an operator then publishes.

**Template variants.** The template gets a variant for each mode.

**Checks.** Each sentence is checked against the code line `findings.md` gives. Facts
about other companies' handling (P-7) are stated as "under their own terms" with a link to
those terms, not as claims about what they do.

**Already fine.** The "Already fine" list of `findings.md` goes in too, so the policy says
what is protected and how.

### D5. Docs from the findings, then a full read

**Fixes.** Each item of D-1 to D-4 is fixed where it is.

**Full read.** Then the whole user guide is read against the window of a trial build, card
by card, through the WebView2 debugging port. That catches what the audit missed.

**Screenshots.** They are retaken from the command mock with `?sample` at 1280 × 800, as
`LOCAL-TESTING.md` describes. The mock's build stamp must read 1.3.0 first.

### D6. Release notes in the change folder

`release-notes.md` in the change folder is in English and is archived with the change. Its
outline:

- **What VoiceBro is**, in one line.
- **What is new since 1.0.1**, grouped:
  - assistant modes and engines;
  - who the assistant hears;
  - recognition and voices;
  - the window;
  - privacy.

  Each item is a short bullet written for users.
- **Updating from 1.0.1**: what carries over, and that the instruction moved from the
  Codex card to the Assistant card.
- **Requirements.**
- **Downloads**: the three files and how to check them with `Get-FileHash`.
- **Known limitations.**

**Paste-ready copy.** After the build from the tag, `target\release\dist\RELEASE-NOTES.md`
holds the same text with the two SHA-256 values from `SHA256SUMS.txt`.

**Source archive.** GitHub's own source archive is not mentioned as a file.

### D7. The update check from 1.0.1

1. Copy the 1.0.1 artifacts to `target\upgrade-test\1.0.1\`.
2. Install 1.0.1 silently into `target\upgrade-test\VoiceBro`.
3. Start it once, give it wake phrases and `codex.instructions`, and download the English
   wake model through its window (debugging port).
4. Close it and install the 1.3.0 trial over it silently.

**Pass criteria.** The 1.3.0 window shows:

- the same phrases;
- the instruction in Assistant;
- the model installed;
- the token set;
- build 1.3.0.

The settings file holds `assistant.instructions` and no `codex.instructions`.

**Afterwards.** Uninstall and remove the test folder. Nothing else of VoiceBro runs
meanwhile: the token is the real one.

## Risks / Trade-offs

- **The docs describe behavior an operator may not like** (names in the log, OpenCode
  leftovers, uninstall leftovers). → That is the decision. The guide says how to remove
  each by hand: delete `logs\`, the OpenCode session folder and the leftover folders.
- **A kept license copy differs from the crate's version.** → It is taken at the commit in
  `.cargo_vcs_info.json`, and copies are keyed by name and version.
- **A voice's model card is unclear about its base.** → Its line says what the card says,
  and "not stated" where it says nothing.
- **The window's posting help still says posts carry no question** (D1). → The guide and
  the template say how it works.

## Migration Plan

- Updates keep all data, and settings from 1.0.1 migrate on load as today (checked by D7).
- Rollback: install 1.0.1 over 1.3.0.
