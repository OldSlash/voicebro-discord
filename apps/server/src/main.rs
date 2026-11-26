//! Console entry point for the existing VoiceBro bot runtime (change
//! `cross-platform-headless`): every assistant mode, engine and access mode of the desktop
//! app, without a window. Linux support is not tested.

mod echo;
mod log_file;
mod records;

use anyhow::{bail, Context, Result};
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
#[cfg(not(windows))]
use tokio::signal::unix::Signal;
use tokio::sync::broadcast;
use voicebro_core::assistant::claude::SignInKind;
use voicebro_core::assistant::switch::Engines;
use voicebro_core::discord::ConnectionState;
use voicebro_core::owner::{OwnerStatus, PendingCode};
use voicebro_core::secrets::{MemoryStore, SecretKey, SecretStore};
use voicebro_core::settings::store::SettingsStore;
use voicebro_core::settings::{AccessMode, AssistantEngine, ValidationError};
use voicebro_core::status::ConnectionErrors;
use voicebro_core::{assets, paths, protect, setup, wake};
use voicebro_core::{AppEvent, EventBus, Runtime, Settings};

const BUILD_STAMP: &str = env!("VOICEBRO_BUILD_STAMP");
const TOKEN_FILE_ENV: &str = "DISCORD_TOKEN_FILE";
const LINK_OWNER_ARG: &str = "--link-owner";
const SET_TOKEN_ARG: &str = "--set-token";
const CLEAR_TOKEN_ARG: &str = "--clear-token";
const UNLINK_OWNER_ARG: &str = "--unlink-owner";
const LIST_CONSENTS_ARG: &str = "--list-consents";
const REMOVE_CONSENT_ARG: &str = "--remove-consent";
const ANTHROPIC_TERMS: &str = "https://www.anthropic.com/legal/consumer-terms";
const CLAUDE_CODE_LEGAL: &str = "https://code.claude.com/docs/en/legal-and-compliance";
const OPENAI_TERMS: &str = "https://openai.com/policies/terms-of-use/";
const OPENAI_SERVICE_TERMS: &str = "https://openai.com/policies/service-terms/";

/// What the command line asks for.
enum Command {
    Run { link_owner: bool },
    InstallAssets,
    SetToken,
    ClearToken,
    UnlinkOwner,
    ListConsents,
    RemoveConsent(String),
}

fn main() -> ExitCode {
    let data_dir = paths::data_dir();
    let logger = log_file::install(&data_dir);
    // The first line of every start is the build stamp (design D7).
    log::info!("VoiceBro {BUILD_STAMP}");
    match run(&data_dir, logger) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            log::error!("{}", describe(&error));
            ExitCode::FAILURE
        }
    }
}

/// The error with each cause its message does not already include.
fn describe(error: &anyhow::Error) -> String {
    let mut text = String::new();
    for cause in error.chain() {
        let cause = cause.to_string();
        if !text.contains(&cause) {
            if !text.is_empty() {
                text.push_str(": ");
            }
            text.push_str(&cause);
        }
    }
    text
}

fn run(data_dir: &Path, logger: &'static log_file::ServerLog) -> Result<()> {
    if !cfg!(any(
        windows,
        all(
            target_os = "linux",
            target_arch = "x86_64",
            target_env = "gnu"
        )
    )) {
        bail!("VoiceBro headless requires Windows or glibc-based Linux x86-64");
    }
    let store = SettingsStore::new(paths::settings_path(data_dir));
    match command()? {
        Command::InstallAssets => install_assets(data_dir, &store),
        Command::SetToken => set_token(data_dir, &store),
        Command::ClearToken => records::clear_token(data_dir),
        Command::UnlinkOwner => records::unlink_owner(data_dir),
        Command::ListConsents => records::list_consents(data_dir, &store),
        Command::RemoveConsent(id) => records::remove_consent(data_dir, &store, &id),
        Command::Run { link_owner } => run_bot(data_dir, store, link_owner, logger),
    }
}

fn command() -> Result<Command> {
    let args = std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| anyhow::anyhow!("the arguments must be text"))
        })
        .collect::<Result<Vec<_>>>()?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let command = match args.as_slice() {
        [] => Command::Run { link_owner: false },
        [wake::INSTALL_ASSETS_ARG] => Command::InstallAssets,
        [LINK_OWNER_ARG] => Command::Run { link_owner: true },
        [SET_TOKEN_ARG] => Command::SetToken,
        [CLEAR_TOKEN_ARG] => Command::ClearToken,
        [UNLINK_OWNER_ARG] => Command::UnlinkOwner,
        [LIST_CONSENTS_ARG] => Command::ListConsents,
        [REMOVE_CONSENT_ARG, id] => Command::RemoveConsent((*id).to_owned()),
        _ => bail!(
            "usage: voicebro-server [{} | {LINK_OWNER_ARG} | {SET_TOKEN_ARG} | {CLEAR_TOKEN_ARG} | {UNLINK_OWNER_ARG} | {LIST_CONSENTS_ARG} | {REMOVE_CONSENT_ARG} <Discord user id>]",
            wake::INSTALL_ASSETS_ARG
        ),
    };
    Ok(command)
}

/// The desktop's validation, and on Linux no Windows voice in the Whisper mode (design D4).
fn validate_settings(settings: &Settings) -> Result<(), ValidationError> {
    settings.validate()?;
    #[cfg(not(windows))]
    if settings.assistant.mode == voicebro_core::settings::AssistantMode::Whisper
        && voicebro_core::tts::catalog::windows_voice(&settings.tts.voice).is_some()
    {
        return Err(ValidationError {
            field: "tts.voice".to_owned(),
            message: "Windows voices exist only on Windows; pick another voice".to_owned(),
        });
    }
    Ok(())
}

/// The settings file or the defaults, checked, never written.
fn load_settings(store: &SettingsStore) -> Result<Settings> {
    let settings = store.load()?.unwrap_or_default();
    validate_settings(&settings)?;
    Ok(settings)
}

/// Where the stored token lives: Credential Manager on Windows, `discord-token.bin` on
/// Linux (design D5).
pub(crate) fn token_store(data_dir: &Path) -> Box<dyn SecretStore> {
    #[cfg(windows)]
    {
        let _ = data_dir;
        voicebro_core::secrets::platform_store()
    }
    #[cfg(not(windows))]
    {
        Box::new(voicebro_core::secrets::FileSecretStore::new(data_dir))
    }
}

/// `DISCORD_TOKEN_FILE` when set, else the stored token, read once into memory.
fn load_secrets(data_dir: &Path) -> Result<Arc<dyn SecretStore>> {
    let token = if let Some(file) = std::env::var_os(TOKEN_FILE_ENV) {
        let file = std::path::PathBuf::from(file);
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("reading the Discord token file {}", file.display()))?;
        // An editor may save the file with a byte order mark; it is never part of a token.
        let token = text.trim().trim_start_matches('\u{feff}').trim().to_owned();
        if token.is_empty() {
            bail!("the Discord token file {} is empty", file.display());
        }
        token
    } else {
        let stored = token_store(data_dir)
            .get(SecretKey::DiscordToken)
            .map_err(|error| {
                anyhow::anyhow!(
                    "the stored Discord token cannot be read ({error}); run voicebro-server {SET_TOKEN_ARG} again"
                )
            })?;
        stored.filter(|token| !token.trim().is_empty()).with_context(|| {
            format!("no Discord token: run voicebro-server {SET_TOKEN_ARG}, or set {TOKEN_FILE_ENV} to a file that holds it")
        })?
    };
    // The runtime gets the value without writing it back.
    let secrets = MemoryStore::default();
    secrets.set(SecretKey::DiscordToken, token.trim())?;
    Ok(Arc::new(secrets))
}

/// `--set-token`: one line from standard input, stored where the platform keeps it.
fn set_token(data_dir: &Path, store: &SettingsStore) -> Result<()> {
    let settings = store.load()?.unwrap_or_default();
    protect::set_encryption(settings.linux.encrypt_files);
    let interactive = std::io::stdin().is_terminal();
    if interactive {
        eprint!("Discord bot token (not shown while you type): ");
        let _ = std::io::stderr().flush();
    }
    let mut line = String::new();
    {
        let _quiet = if interactive {
            echo::EchoOff::new()
        } else {
            None
        };
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .context("reading the token from standard input")?;
    }
    if interactive {
        eprintln!();
    }
    // A shell may put a byte order mark in front of piped text; it is never part of a token.
    let token = line.trim().trim_start_matches('\u{feff}').trim();
    if token.is_empty() {
        bail!("the token was blank; nothing was stored and an earlier token is kept");
    }
    if !token.chars().all(|c| c.is_ascii_graphic()) {
        bail!(
            "the input holds characters a Discord token never has; nothing was stored and an earlier token is kept"
        );
    }
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("creating {}", data_dir.display()))?;
    token_store(data_dir).set(SecretKey::DiscordToken, token)?;
    if cfg!(windows) {
        log::info!(
            "The Discord token is stored in Windows Credential Manager as {}.",
            SecretKey::DiscordToken.target()
        );
    } else {
        log::info!(
            "The Discord token is stored in {}, {}.",
            voicebro_core::secrets::token_path(data_dir).display(),
            if protect::encryption() {
                "encrypted"
            } else {
                "unencrypted (linux.encrypt_files is off)"
            }
        );
    }
    Ok(())
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// `--install-assets`: everything the current settings need, verified (design D3).
fn install_assets(data_dir: &Path, store: &SettingsStore) -> Result<()> {
    let settings = load_settings(store)?;
    let missing = setup::missing_assets(&settings, data_dir).map_err(anyhow::Error::msg)?;
    if missing.is_empty() {
        log::info!(
            "Everything the {} mode needs is installed.",
            settings.assistant.mode.name()
        );
        return Ok(());
    }
    let assets_dir = paths::assets_dir(data_dir);
    assets::cleanup_partial(&assets_dir);
    for asset in &missing {
        log::info!(
            "{}: {}, {}, from {}",
            asset.label,
            asset.file_name(),
            megabytes(asset.size),
            asset.page
        );
        if *asset == voicebro_core::tts::catalog::PHONEMIZER {
            log::info!(
                "piper-phonemize is licensed under GPL-3.0; its source is {}",
                voicebro_core::tts::catalog::PHONEMIZER_SOURCE
            );
        }
    }
    for asset in &missing {
        assets::ensure(asset, &assets_dir, &|progress| match progress {
            assets::Progress::Downloaded(percent) => {
                log::info!("{}: {percent}%", asset.label)
            }
            assets::Progress::Unpacking => log::info!("{}: verifying and unpacking", asset.label),
        })
        .map_err(anyhow::Error::msg)?;
    }
    log::info!(
        "Installed what the {} mode needs.",
        settings.assistant.mode.name()
    );
    Ok(())
}

#[cfg(windows)]
fn shutdown_signals() -> std::io::Result<(
    tokio::signal::windows::CtrlC,
    tokio::signal::windows::CtrlClose,
)> {
    use tokio::signal::windows::{ctrl_c, ctrl_close};

    Ok((ctrl_c()?, ctrl_close()?))
}

#[cfg(not(windows))]
fn shutdown_signals() -> std::io::Result<(Signal, Signal)> {
    use tokio::signal::unix::{signal, SignalKind};

    Ok((
        signal(SignalKind::interrupt())?,
        signal(SignalKind::terminate())?,
    ))
}

fn run_bot(
    data_dir: &Path,
    store: SettingsStore,
    link_owner: bool,
    logger: &'static log_file::ServerLog,
) -> Result<()> {
    let secrets = load_secrets(data_dir)?;
    let executor = tokio::runtime::Runtime::new().context("starting the async runtime")?;
    let (mut interrupt, mut terminate) = executor
        .block_on(async { shutdown_signals() })
        .context("registering shutdown signals")?;
    let handle = executor.handle().clone();
    let events = EventBus::for_terminal();
    let messages = events.subscribe();
    let errors = Arc::new(ConnectionErrors::new(events.clone()));
    // The engines are built as the desktop builds them (design D4), from the file as it is.
    let initial = store.load().ok().flatten().unwrap_or_default();
    let engines = Engines::build(
        handle.clone(),
        events.clone(),
        Arc::clone(&errors),
        data_dir,
        &initial,
    );
    let runtime = Arc::new(Runtime::with_assistant(
        BUILD_STAMP,
        data_dir.to_path_buf(),
        store,
        secrets,
        events,
        handle.clone(),
        errors,
        Arc::clone(&engines.switch),
    ));
    if let Err(error) = runtime.start_checked(validate_settings) {
        runtime.shutdown();
        return Err(error.into());
    }
    let settings = runtime.settings();
    logger.set_verbose(settings.diagnostics.verbose_log);
    let owner = runtime.owner().status();
    report_access(&runtime, &settings, &owner);
    // The Codex lists at start, only while Codex is the engine, as on the desktop.
    if settings.assistant.engine == AssistantEngine::Codex {
        engines
            .codex
            .read_lists_at_start(settings.codex.refresh_lists_at_start);
    }
    if settings.assistant.engine == AssistantEngine::Claude
        && settings.discord.access != AccessMode::Owner
    {
        let claude = engines.claude.clone();
        handle.spawn(async move {
            let kind = claude
                .check()
                .await
                .map(|sign_in| sign_in.kind)
                .unwrap_or(SignInKind::Unknown);
            if !matches!(kind, SignInKind::ApiKey | SignInKind::Cloud) {
                subscription_notice();
            }
        });
    }
    // Change `chatgpt-account-notice`, D4: without reading the account, which would start
    // Codex before a join.
    if settings.assistant.engine == AssistantEngine::Codex
        && settings.discord.access != AccessMode::Owner
    {
        chatgpt_account_notice();
    }
    if link_owner {
        let runtime = Arc::clone(&runtime);
        let (codex, claude) = (engines.codex.clone(), engines.claude.clone());
        let engine = settings.assistant.engine;
        handle.spawn(async move {
            // The selected engine reads its account as Check does; OpenCode has none.
            let read = match engine {
                AssistantEngine::Codex => codex.check().await.map(|_| ()),
                AssistantEngine::Claude => claude.check().await.map(|_| ()),
                AssistantEngine::Opencode => Ok(()),
            };
            if let Err(error) = read {
                log::warn!(
                    "owner link: the {} account could not be read: {error}",
                    engine.product_name()
                );
            }
            match runtime.start_link() {
                Ok(code) => {
                    print_code(&code);
                    wait_for_expiry(&runtime).await;
                }
                Err(reason) => log::warn!("owner link: no code was issued: {reason}"),
            }
        });
    }

    let result = executor.block_on(async {
        tokio::select! {
            _ = interrupt.recv() => Ok(()),
            _ = terminate.recv() => Ok(()),
            result = report_events(messages, owner) => result,
        }
    });
    // The shared shutdown waits for async cleanup using Handle::block_on.
    runtime.shutdown();
    result
}

/// The access mode, the owner and the people who agreed, at start (design D8).
fn report_access(runtime: &Runtime, settings: &Settings, owner: &OwnerStatus) {
    let consents = runtime.consent_status();
    log::info!(
        "access: {}; owner: {}; people who agreed: {}",
        settings.discord.access.name(),
        owner
            .linked
            .as_ref()
            .map_or("not linked", |linked| linked.name.as_str()),
        consents.records.len()
    );
    if let Some(reason) = &owner.link_ended {
        log::warn!("owner link: {reason}");
    }
    if let Some(reason) = &consents.unreadable {
        log::warn!("consent records: {reason}");
    }
}

/// The Claude subscription notice, which the window shows as a card (design D5).
fn subscription_notice() {
    log::warn!(
        "Claude Code may be signed in with a Claude subscription, which only its holder may use (Anthropic's Consumer Terms: {ANTHROPIC_TERMS}; Claude Code: {CLAUDE_CODE_LEGAL}). While other people's voices reach the assistant, they use that subscription. To let only your own voice through, set discord.access to \"owner\" in settings.json and link your Discord account with voicebro-server {LINK_OWNER_ARG}, or sign Claude Code in with an API key. VoiceBro does not enforce this."
    );
}

/// The ChatGPT account notice, which the window shows in the Codex card (change
/// `chatgpt-account-notice`, D4).
fn chatgpt_account_notice() {
    log::warn!(
        "If Codex is signed in with a ChatGPT account (any plan), only its holder may use it (OpenAI's Terms of Use: {OPENAI_TERMS}). While other people's voices reach the assistant, they use that account. In the realtime voice mode the assistant speaks with ChatGPT's voice, which OpenAI's Service Terms ({OPENAI_SERVICE_TERMS}) allow for non-commercial use only and not distributed as a recording or a sound file: do not record and publish what the assistant says. To let only your own voice through, set discord.access to \"owner\" in settings.json and link your Discord account with voicebro-server {LINK_OWNER_ARG}. This does not apply to an OpenAI API key, Amazon Bedrock or Google Cloud sign-in. VoiceBro does not enforce this."
    );
}

/// The code goes to standard output only, never through the log (design D8).
fn print_code(code: &PendingCode) {
    let minutes = voicebro_core::owner::CODE_LIFETIME.as_secs() / 60;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "Owner link code: {}", code.code);
    let _ = writeln!(
        out,
        "Send /link {} to the bot from your Discord account within {minutes} minutes, in a server or in a direct message.",
        code.code
    );
    let _ = out.flush();
}

/// Says when the printed code expired unused; a link or the fifth wrong code is logged by
/// the owner state itself.
async fn wait_for_expiry(runtime: &Runtime) {
    tokio::time::sleep(voicebro_core::owner::CODE_LIFETIME + std::time::Duration::from_secs(1))
        .await;
    if runtime.owner().status().code_ended.as_deref() == Some("the code expired") {
        log::warn!(
            "owner link: the code expired; run voicebro-server {LINK_OWNER_ARG} for a new one"
        );
    }
}

/// Terminal lines for what the window would show, and the exit on a Discord failure.
async fn report_events(
    mut messages: broadcast::Receiver<AppEvent>,
    mut owner: OwnerStatus,
) -> Result<()> {
    let mut agreed: Option<usize> = None;
    loop {
        match messages.recv().await {
            Ok(AppEvent::ConnectionError {
                source,
                message: Some(reason),
            }) => log::warn!("{source}: {reason}"),
            Ok(AppEvent::Discord(status)) if status.state == ConnectionState::Error => bail!(
                "Discord: {}",
                status.last_error.as_deref().unwrap_or("connection failed")
            ),
            Ok(AppEvent::Owner(status)) => {
                if status.linked.is_none() && owner.linked.is_some() {
                    log::info!("owner: not linked");
                }
                owner = *status;
            }
            Ok(AppEvent::Consent(status)) => {
                let count = status.records.len();
                if agreed.is_some_and(|before| before != count) {
                    log::info!("people who agreed: {count}");
                }
                agreed = Some(count);
            }
            Err(broadcast::error::RecvError::Closed) => return Ok(()),
            _ => {}
        }
    }
}
