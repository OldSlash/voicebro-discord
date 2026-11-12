# Proposal

## Why

Every Opus packet the app makes or reads goes through libopus, a C library that
`libopus_sys` 0.3.3 builds from its sources with CMake (Opus 1.5, through the `opus2` 0.4
binding). It is the only reason the build needs CMake on PATH, and the only C library the
app compiles itself. On 2026-11-12 the user asked to move toward pure Rust and to settle
which pure-Rust Opus is good enough by an experiment: three candidates against libopus,
switchable in the app's settings, compared by the agent with the conversation tests it ran
before (the live Codex calls of 2026-11-09 and 2026-11-10), and then by the user in a real
channel. The user leans toward opus-rs if it works well.

The three candidates, checked on 2026-11-12:

- **opus-rs** 0.1.42 (restsend, BSD-3-Clause): a port of libopus 1.6, float, actively
  developed (released on 2026-11-10, 248 thousand downloads). Its own API.
- **ropus** 0.12.18 (BSD-3-Clause): a port of the fixed-point libopus that claims bit-exact
  output against the reference. Its own API. Built from crates.io it compiles no C: its
  build script only compiles a C helper when the Xiph reference sources sit next to it.
- **mousiki** 0.2.1 (MIT): a port of the reference opus-c, float. `opus2` 0.4 already offers
  it as a second backend with the same API, but its GitHub repository answered 404 on
  2026-11-12; the crate on crates.io is all that is left.

`opus2` cannot hold both backends in one build (`compile_error!` when both features are on),
and songbird pins `opus2` with its default features, that is libopus. So the switch cannot
be an `opus2` feature: the codecs are compiled in side by side and the app picks one.

The comparison was run with all four codecs (`docs/OPUS-CODEC-COMPARISON.md`). On
2026-11-12 the user decided to keep two: libopus, the C library, as the default, and
opus-rs, which matched libopus on every path and is the one actively developed. ropus
(its concealment under loss is measurably worse) and mousiki (it garbles hybrid frames,
and its repository is gone) are taken out of the build and the settings again; their
numbers stay in the report.

## What Changes

- **One Opus layer, two codecs.** A new module in the core wraps libopus (through `opus2`)
  and opus-rs behind one encoder and one decoder type. Each codec gets the same parameters
  the app uses today.
- **A setting picks the codec**: `diagnostics.opus_codec`, `libopus` (the default) or
  `opus-rs`. Settings → Diagnostics gets an "Opus codec (experiment)" list with a help line
  and the codec in use. The choice takes effect at the next join for the Discord side and
  at the next call for the Codex side; saving it restarts nothing. A settings file that
  names `ropus` or `mousiki` (from a trial build of the four-codec stage) loads with
  libopus.
- **What the setting switches**: the encoder of the assistant's voice toward Discord, the
  encoder toward the Codex call, the decoder of the Codex call (with its FEC and
  concealment), and the decoder of the people speaking in the Discord channel. With
  `libopus` every path is the one the app has today; for the people in the channel that is
  songbird's own decoder. With opus-rs songbird only decrypts, and the app decodes each
  speaker itself.
- **What it does not switch**: songbird's fallback when it refuses passthrough (it then
  decodes our packets and encodes the mix again with its own libopus) and songbird's soft
  clipping on that path. They stay libopus. Removing them is part of a later change that
  removes libopus.
- **A codec error is handled, a codec panic ends the join honestly** (the user's decision
  of 2026-11-12). An encode or decode error of either codec is counted and logged, and the
  frame is replaced by silence or concealment, as an encode error is today. A panic inside
  opus-rs is caught at the codec module's boundary and logged with its message; the bot
  then leaves that server's voice channel as `/leave` does, and the window shows a notice
  that the bot stopped because opus-rs failed. Nothing is swapped silently: the setting
  stays opus-rs, and the user can `/join` again or pick libopus.
- **Release builds unwind on panic**, so that the codec's panic can be caught at all
  (`panic = "unwind"` instead of `"abort"`). A panic hook logs every panic; outside the
  codec it shows a Windows dialog and ends the process at once, so every other bug still
  stops the app as before, now with its reason in the log.
- **Comparison tools**: a probe command that runs a speech corpus through every codec
  (time per frame, decoding by libopus of every encoder's packets, the difference from
  libopus, FEC and concealment under simulated loss, WAV files to listen to), and a
  `VOICEBRO_OPUS_CODEC` variable for the live call test, the loopback test, the replay stand
  and the probes that encode or decode.
- **The comparison itself**: the agent ran it with the four candidates and wrote
  `docs/OPUS-CODEC-COMPARISON.md` with the results, what was not tested and a
  recommendation; it gets a section with the user's decision.

Not part of this change: making opus-rs the default, removing libopus or CMake (and with it
the path to the CMake inside the Visual Studio Build Tools that the scripts and the docs
add to `PATH`), and patching songbird. Whether opus-rs replaces libopus is the user's
decision after the test in a real channel; that is a later change.

## Capabilities

### New Capabilities

- `opus-codecs`: libopus and opus-rs behind one encoder and decoder; which paths use the
  selected codec and with which parameters; when a choice takes effect; packets that
  libopus must decode; FEC and concealment with both codecs; codec errors handled and an
  opus-rs panic ending the join.

### Modified Capabilities

- `app-settings`: the `diagnostics.opus_codec` setting, and a stored codec this build does
  not have.
- `settings-ui`: the "Opus codec (experiment)" list in the Diagnostics card with the codec
  in use.
- `audio-transport`: the people in the channel are decoded by the selected codec.
- `app-distribution`: the notices carry the Opus license for the Opus port built in.
- `app-window`: the notice that the bot left the voice channel because the codec failed.
- `diagnostic-logging`: every panic is logged; outside the codec the app ends with a dialog.

## Impact

- One new dependency of `voicebro-core`, pinned to its exact version: `opus-rs` 0.1.42
  (ropus 0.12.18 and mousiki 0.2.1 were added for the comparison and are removed again,
  with the workspace's `[profile.dev.package.mousiki]`). libopus, `opus2` and CMake stay.
  The binary grows by opus-rs; the comparison measured +2.3 MB for the three ports.
- Code: a new `crates/voicebro-core/src/audio/codec.rs`; `audio/opus_out.rs` (the encoder
  toward Discord), `assistant/codex/rtc.rs` and `jitter.rs` (the call's encoder and
  decoder), `discord/mod.rs` and `discord/receive.rs` (songbird's decode mode per join and
  the app's own decoding of each speaker), `settings/mod.rs`, `status.rs`; the codec panic
  report and its handling in `events.rs` and `runtime.rs`.
- Build: the workspace's release profile goes from `panic = "abort"` to `"unwind"`; the
  binary grows by the unwind tables (measured).
- Desktop shell: a panic hook in `apps/desktop/src/main.rs`, with the `windows-sys`
  feature for the Windows message box.
- Window: `apps/desktop/ui/index.html`, `main.js`, `styles.css` (the codec failure
  notice), `apps/desktop/ui-dev/mock.js`.
- Probes: `apps/probe` gets `opus-compare` and a codec choice for `opus-roundtrip`,
  `rtc-loopback`, `rtc-call` and `discord-delivery`.
- `scripts/third-party-notices.ps1`: nothing to add for opus-rs, which ships the Opus
  license as its own `COPYING`; the block added for mousiki's Opus text goes again.
- `scripts/run-wake-corpus.ps1`: `-CorpusRoot`, for the decoded copies.
- Docs: `docs/ARCHITECTURE.md`, `docs/user-guide.md` (the Diagnostics card), `AGENTS.md`
  (commands), `LOCAL-TESTING.md`, and the new `docs/OPUS-CODEC-COMPARISON.md`.
