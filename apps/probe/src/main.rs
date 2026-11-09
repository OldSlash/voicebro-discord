//! VoiceBro development probes.
//!
//! Each subcommand exercises one external API live and prints `PASS` or `FAIL` with the
//! values it measured. The pass criteria are written in `docs/MVP-PLAN.md` before the probe
//! runs; a probe never retries a failure into a pass.
//!
//! This binary is a development tool and is not shipped.

mod codex;
mod credential;
mod discord;
mod hotkey;
mod opus;
mod rtc;
mod seed;
mod vosk;

use std::process::ExitCode;

/// What a probe found. `pass` is judged against the written criterion; `details` are the
/// measured values, printed whatever the verdict.
pub struct Outcome {
    pub pass: bool,
    pub details: Vec<String>,
}

impl Outcome {
    pub fn pass(details: Vec<String>) -> Self {
        Self {
            pass: true,
            details,
        }
    }

    pub fn fail(details: Vec<String>) -> Self {
        Self {
            pass: false,
            details,
        }
    }
}

/// One probe: its subcommand, what it checks, and what it needs.
struct Probe {
    name: &'static str,
    summary: &'static str,
    needs: &'static str,
    run: fn(&[String]) -> anyhow::Result<Outcome>,
}

const PROBES: &[Probe] = &[
    Probe {
        name: "codex-schema",
        summary: "dump the app-server JSON schema and check the realtime types",
        needs: "codex on PATH (or CODEX_EXECUTABLE); args: <out-dir>",
        run: codex::schema,
    },
    Probe {
        name: "codex-smoke",
        summary: "initialize, account/read, model/list, open and close a read-only thread",
        needs: "codex on PATH (or CODEX_EXECUTABLE)",
        run: codex::smoke,
    },
    Probe {
        name: "opus-roundtrip",
        summary: "encode and decode 48 kHz 20 ms frames with opus2",
        needs: "nothing",
        run: opus::roundtrip,
    },
    Probe {
        name: "rtc-loopback",
        summary: "two in-process peer connections exchange an Opus track and a data channel",
        needs: "nothing",
        run: rtc::loopback,
    },
    Probe {
        name: "rtc-call",
        summary:
            "WebRTC call to Codex realtime v3 with the canary instruction; args: <in.wav> <out.wav>",
        needs: "codex signed in with a ChatGPT account; a 48 kHz mono WAV",
        run: rtc::call,
    },
    Probe {
        name: "credential",
        summary: "write, read, delete a VoiceBro/test credential in the Windows Credential Manager",
        needs: "nothing; args: [--keep] to leave the entry for a reboot check",
        run: credential::roundtrip,
    },
    Probe {
        name: "vosk-assets",
        summary:
            "download the pinned Vosk library and model, verify hashes, load the model; args: <dir>",
        needs: "network; about 60 MB of downloads",
        run: vosk::assets,
    },
    Probe {
        name: "hotkey",
        summary: "global Ctrl+Alt+M through tauri-plugin-global-shortcut under a full-screen game",
        needs: "the probe-hotkey window (cargo run -p probe-hotkey) and a game in front",
        run: hotkey::run,
    },
    Probe {
        name: "seed-secrets",
        summary: "store the Discord token from probe.local.json in the app's secret store",
        needs: "probe.local.json with discord_token",
        run: seed::secrets,
    },
    Probe {
        name: "discord-lifecycle",
        summary: "join, count VoiceTicks, remove, rejoin; check old handler and track end",
        needs: "VOICEBRO_DISCORD_TOKEN, VOICEBRO_TEST_GUILD, VOICEBRO_TEST_CHANNEL",
        run: discord::lifecycle,
    },
    Probe {
        name: "receive-timing",
        summary: "tick interval and handler duration with two speakers for one minute",
        needs: "VOICEBRO_DISCORD_TOKEN, VOICEBRO_TEST_GUILD, VOICEBRO_TEST_CHANNEL, two speakers",
        run: discord::receive_timing,
    },
];

fn list() {
    println!("{:<18} what it checks", "probe");
    for probe in PROBES {
        println!("{:<18} {}", probe.name, probe.summary);
        println!("{:<18}   needs: {}", "", probe.needs);
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = args.first() else {
        list();
        return ExitCode::from(2);
    };
    if name == "list" {
        list();
        return ExitCode::SUCCESS;
    }
    let Some(probe) = PROBES.iter().find(|probe| probe.name == name) else {
        eprintln!("unknown probe `{name}`; run `probe list`");
        return ExitCode::from(2);
    };
    println!("== {} ==", probe.name);
    match (probe.run)(&args[1..]) {
        Ok(outcome) => {
            for line in &outcome.details {
                println!("  {line}");
            }
            if outcome.pass {
                println!("PASS {}", probe.name);
                ExitCode::SUCCESS
            } else {
                println!("FAIL {}", probe.name);
                ExitCode::from(1)
            }
        }
        Err(error) => {
            println!("  error: {error:#}");
            println!("FAIL {}", probe.name);
            ExitCode::from(1)
        }
    }
}
