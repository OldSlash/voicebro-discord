//! P0.9: global hotkey under a full-screen game.
//!
//! The window lives in `apps/probe-hotkey` (a minimal Tauri app, because the plugin needs
//! a Tauri app handle). This subcommand runs it and relays its output. The verdict is the
//! user's: press Ctrl+Alt+M with a full-screen game in front and see whether a line appears.

use crate::Outcome;
use anyhow::Context;
use std::process::Stdio;

pub fn run(_args: &[String]) -> anyhow::Result<Outcome> {
    println!("  starting the probe-hotkey window (cargo run -p probe-hotkey)…");
    println!("  bring a full-screen game to the front and press Ctrl+Alt+M a few times;");
    println!("  close the window (or Ctrl+C here) when done.");
    let status = std::process::Command::new("cargo")
        .args(["run", "-q", "-p", "probe-hotkey"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("running cargo run -p probe-hotkey")?;
    let details = vec![
        format!("probe-hotkey exited with {status}"),
        "verdict is manual: PASS only if presses were logged while the game had focus".to_owned(),
    ];
    Ok(Outcome {
        pass: status.success(),
        details,
    })
}
