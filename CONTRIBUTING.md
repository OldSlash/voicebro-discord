# Contributing to VoiceBro

Thanks for helping. VoiceBro is maintained by one person, so a clear bug report is often
the most useful help. Everyone taking part follows the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Reporting a bug

First look at [Troubleshooting](docs/user-guide.md#13-troubleshooting) in the user guide.
If that does not help, open an issue with the **Bug report** form. The most useful parts:

- the build stamp from the bottom right corner of the window;
- the assistant mode and the engine (Config → Assistant);
- the log: turn on **Settings → Diagnostics → Verbose log**, repeat the problem, press
  **Chat → Open log folder** and attach `logs\voicebro.log`. The log names the people in
  the channel, and with **Verbose log** on it holds what they said, so read it before you
  share it. VoiceBro never writes the Discord token into it.

A security problem goes through [SECURITY.md](SECURITY.md), not a public issue.

## Suggesting a feature

Open an issue with the **Feature request** form. Describe what you want to do and why,
not only the button you have in mind.

## Changing the code

Pull requests are welcome for small fixes. For anything larger, open an issue first, so
the approach is agreed on before you spend time on it.

- **Build** as described in [Building from source](README.md#building-from-source):
  Windows, Rust stable with the MSVC toolchain, and the Visual Studio 2022 Build Tools with
  CMake.
- **Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first**, and update it in the same
  change as the code it describes.
- **Specifications** are in [openspec/](openspec/). If your change alters behavior
  described there, say so in the pull request.
- **English everywhere**: code, comments, docs, UI strings and log lines.
- **No secrets and no personal data** in code, tests, test recordings or attached logs.
- **The version** is set by the maintainer at release; do not change it.
- **Before you send it**, run:

  ```powershell
  cargo fmt --check
  cargo clippy --all-targets
  cargo test
  ```

  `cargo test` needs no Discord account and no engine. For a change to the window, also
  run the UI checks, which drive the window's command mock in a headless Microsoft Edge:

  ```powershell
  node apps/desktop/ui-dev/check-wake-dialog.mjs
  node apps/desktop/ui-dev/check-config-tab.mjs
  node apps/desktop/ui-dev/check-whisper-config.mjs
  node apps/desktop/ui-dev/check-engine-config.mjs
  ```

  Say in the pull request what you tested and what you could not test.

## License

By contributing, you agree that your contribution is released under the project's
[MIT License](LICENSE).
