// A stand-in for the app's Tauri commands and events, so the real UI (`../ui/main.js`,
// `../ui/styles.css`) can be checked in a browser without the app. It keeps a settings
// document in memory with the app's validation messages for the common cases, answers the
// commands the page calls with the same names and payload shapes, and emits status events
// every second. It is outside `frontendDist` and never ships.
//
// `window.__mock` exposes what a check needs: `calls` (every command with its arguments),
// `settings()` (the stored document), `changeCapabilities()` (the engine offers a new
// model list, as after a sign-in) and `emit(name, payload)`.
(() => {
  const THEMES = ["dark", "darcula", "light", "intellij"];
  const clone = (value) => JSON.parse(JSON.stringify(value));
  let settings = {
    version: 2,
    discord: { guild_id: "1091401169441656933" },
    session: { mode: "wake_word", idle_timeout_seconds: 60, earcons: true, earcon_volume: 0.18 },
    audio: { input_gain: 1, output_gain: 1 },
    // Version 2: the phrases of each model under its id.
    wake: {
      engine: "vosk",
      model: "vosk-model-small-ru-0.22",
      phrases: { "vosk-model-small-ru-0.22": { activation: ["alpha"], deactivation: ["omega"] } },
    },
    assistant: { engine: "codex", speaker_labels: "start", save_chat_history: false },
    // `handoff_mode` as the app fills it for a file without the field: the default.
    codex: { executable: "", model: "", reasoning_effort: "", voice: "", instructions: "Answer briefly.", handoff_mode: "items" },
    hotkey: { mute: "Ctrl+Alt+M" },
    ui: { theme: "dark", font_size: 13 },
    // All off, as the app fills them for a file without the section.
    diagnostics: { performance: false, memory: false, verbose_log: false },
  };
  let tokenSet = true;
  let muted = false;
  // Codex as the status reports it, and the install offer: `__mock.setCodex(...)` switches.
  let codex = { installed: true, signedIn: true, accountRead: true, winget: true, nextExitCode: 0 };
  let install = { kind: "idle" };
  let capabilities = {
    models: [
      { id: "model-a", display_name: "Model A", description: "", reasoning_efforts: ["low", "medium", "high"], default_reasoning_effort: "medium", is_default: true },
      { id: "model-b", display_name: "Model B", description: "", reasoning_efforts: ["low", "high"], default_reasoning_effort: "low", is_default: false },
    ],
    voices: ["cove", "juniper", "maple"],
    default_voice: "cove",
    signed_in: true,
  };
  // As `wake_models` answers: the catalog with its download state, then the models folder.
  // `asset` is the mock's own: the label the app gives the model's archive.
  const catalog = [
    ["vosk-model-small-en-us-0.15", "en-US", "English (US) 0.15", "English", 41205931],
    ["vosk-model-small-ru-0.22", "ru", "Russian 0.22", "Russian", 46220000],
    ["vosk-model-small-cs-0.4-rhasspy", "cs", "Czech 0.4", "Czech", 46088666, "MIT"],
    ["vosk-model-small-nl-0.22", "nl", "Dutch 0.22", "Dutch", 40441176],
    ["vosk-model-small-fr-0.22", "fr", "French 0.22", "French", 42233323],
    ["vosk-model-small-de-0.15", "de", "German 0.15", "German", 46499967],
    ["vosk-model-small-it-0.22", "it", "Italian 0.22", "Italian", 49665141],
    ["vosk-model-small-pl-0.22", "pl", "Polish 0.22", "Polish", 52979372],
    ["vosk-model-small-pt-0.3", "pt-BR", "Portuguese (Brazil) 0.3", "Portuguese (Brazil)", 32453112],
    ["vosk-model-small-es-0.42", "es", "Spanish 0.42", "Spanish", 39817833],
  ].map(([id, language, name, asset, bytes, license = "Apache-2.0"]) => ({
    id, engine: "vosk", language, display_name: `Vosk small ${name}`, license, source: "catalog",
    installed: false, bytes, asset: `Vosk ${asset} model`,
  }));
  const wakeModels = [
    ...catalog,
    { id: "local:vosk-model-small-ca-0.4", engine: "vosk", language: "ca", display_name: "vosk-model-small-ca-0.4", license: "", source: "local", installed: true, bytes: 0 },
  ];
  // `?fresh` in the page's address: a first start in an empty folder (English selected, no
  // phrases, nothing downloaded, the bot in no voice channel). Otherwise the Russian model
  // and the Vosk library are on disk and the bot sits in a channel.
  const FRESH = new URLSearchParams(location.search).has("fresh");
  let libraryInstalled = !FRESH;
  if (!FRESH) wakeModels[1].installed = true;
  if (FRESH) {
    settings.wake.model = "vosk-model-small-en-us-0.15";
    settings.wake.phrases = {};
  }
  // `?always`: the session mode starts as always listening.
  if (new URLSearchParams(location.search).has("always")) settings.session.mode = "always";
  let inVoice = !FRESH;
  // The wake engine as the app runs it: in wake-word mode the files are checked before the
  // phrases, a model that is not on disk asks first, and Download walks through the
  // progress. The consent covers one model.
  let consentFor = null;
  let download = null; // { model, items, index, percent, failed }
  let failNextDownload = false;
  const LIBRARY = { label: "Vosk library", host: "github.com", bytes: 14900000 };
  // Missing lists are empty, as the app fills them when it reads the document.
  function selectedPhrases() {
    const entry = settings.wake.phrases[settings.wake.model] || {};
    return { activation: entry.activation || [], deactivation: entry.deactivation || [] };
  }
  // What choosing `id` needs to download, as `wake_requirements` answers.
  function requirements(id) {
    const model = wakeModels.find((entry) => entry.id === id);
    if (!model) throw `model «${id}» is neither in the catalog nor a local: model`;
    const items = [];
    if (!libraryInstalled) items.push(clone(LIBRARY));
    if (!model.installed) items.push({ label: model.asset, host: "alphacephei.com", bytes: model.bytes });
    return items;
  }
  function wakeState() {
    if (settings.session.mode !== "wake_word") return { kind: "off" };
    if (download && download.model === settings.wake.model) {
      if (download.failed) return { kind: "error", message: `${download.items[download.index].label}: SHA-256 mismatch; nothing was extracted` };
      return { kind: "downloading", label: download.items[download.index].label, percent: download.percent };
    }
    const model = wakeModels.find((entry) => entry.id === settings.wake.model);
    if (!model) return { kind: "error", message: `the model folder C:\\VoiceBro\\models\\${settings.wake.model.slice(6)} is missing; put the model back or choose another one` };
    const items = requirements(model.id);
    if (items.length && consentFor !== settings.wake.model) return { kind: "needs_download", items };
    const phrases = selectedPhrases();
    if (!normalize(phrases.activation).length && !normalize(phrases.deactivation).length) return { kind: "no_phrases" };
    return { kind: "ready" };
  }
  function startDownload(id) {
    if (id !== settings.wake.model) throw `${id} is not the selected wake model (${settings.wake.model})`;
    const state = wakeState();
    if (state.kind !== "needs_download" && state.kind !== "error") throw "nothing is waiting for a download";
    consentFor = settings.wake.model;
    const model = wakeModels.find((entry) => entry.id === settings.wake.model);
    // A retry goes on with what is still missing.
    download = { model: model.id, items: requirements(model.id), index: 0, percent: 0, failed: false };
    const step = () => {
      if (!download || download.model !== settings.wake.model) return;
      if (failNextDownload && download.percent >= 40) {
        failNextDownload = false;
        download.failed = true;
        emit("app-event", { event: "wake" });
        return;
      }
      download.percent += 20;
      if (download.percent >= 100) {
        if (download.items[download.index].label === LIBRARY.label) libraryInstalled = true;
        else model.installed = true;
        download.index += 1;
        download.percent = 0;
        if (download.index >= download.items.length) download = null;
      }
      emit("app-event", { event: "wake" });
      if (download) setTimeout(step, 300);
    };
    setTimeout(step, 300);
  }
  // A sample session for the documentation's screenshots (the page opened with `?sample`),
  // whose build stamp then reads like a release.
  const SAMPLE = new URLSearchParams(location.search).has("sample");
  const STAMP = SAMPLE ? "1.0.0 2026-11-11" : "1.0.0+mock 2026-11-11";
  const SAMPLE_CHAT = [
    ["20:14:02", "system", null, "Call started"],
    ["20:14:02", "context", null, "In the channel: Alex, Sam. Addressed by: Alex."],
    ["20:14:04", "user", "Alex", "Hey bro, how long is one day in Minecraft?"],
    ["20:14:06", "assistant", null, "A full day in Minecraft lasts twenty real minutes, about ten of them daylight."],
    ["20:15:31", "user", "Sam", "Brother, what's the weather in Berlin tomorrow?"],
    ["20:15:32", "assistant", null, "Let me check."],
    ["20:15:32", "task", null, "Weather forecast for Berlin tomorrow"],
    ["20:15:34", "tool", null, "Web search: Berlin weather forecast tomorrow"],
    ["20:15:39", "agent", null, "Tomorrow in Berlin: cloudy, 8 to 12 °C, light rain in the evening."],
    ["20:15:41", "assistant", null, "Cloudy tomorrow in Berlin, eight to twelve degrees, and some rain in the evening."],
    ["20:16:05", "user", "Alex", "Thanks, bro. Goodbye."],
    ["20:16:05", "system", null, "Call ended"],
  ].map(([at, kind, who, text], index) => ({ id: index + 1, at, guild_id: "1", kind, who, text }));
  const started = Date.now();
  const listeners = {};
  const calls = [];
  // A connected call: 50 packets a second each way; `setLoss` conceals a share of them.
  let lossRate = 0;
  let concealed = 0;
  let lastTick = Date.now();
  const callAudio = {
    frames_sent: 0, input_drops: 0, input_underruns: 0, packets_received: 0, late: 0, concealed: 0,
    concealed_fec: 0, playout_underruns: 0, output_drops: 0, decode_errors: 0,
    stretched_pause_ms: 0, stretched_speech_ms: 0, removed_ms: 0, bridged_frames: 0, speech_gaps: 0,
    depth_ms: 60, depth_max_ms: 60,
  };
  function advanceCall() {
    const now = Date.now();
    const packets = ((now - lastTick) / 1000) * 50;
    lastTick = now;
    callAudio.frames_sent += Math.round(packets);
    callAudio.packets_received += Math.round(packets);
    concealed += packets * lossRate;
    callAudio.concealed = Math.round(concealed);
    callAudio.concealed_fec = Math.floor(callAudio.concealed / 2);
    // A server a little slower than real time: the playout lengthens pauses to keep up.
    callAudio.stretched_pause_ms += Math.round(packets) % 3 === 0 ? 20 : 0;
    callAudio.depth_ms = 220;
    callAudio.depth_max_ms = 360;
  }
  const turns = [];
  function addTurn() {
    const n = turns.length + 1;
    turns.push({
      at: new Date().toLocaleTimeString(), turn: n, kind: "response",
      first_speech_to_answer_ms: 3000 + 100 * (n % 5), last_speech_to_answer_ms: 1400 + 50 * (n % 3),
      answer_to_playback_ms: 60, first_speech_to_playback_ms: 3060 + 100 * (n % 5),
    });
    if (turns.length > 50) turns.shift();
  }

  function merge(target, patch) {
    for (const [key, value] of Object.entries(patch)) {
      if (value && typeof value === "object" && !Array.isArray(value) && target[key] && typeof target[key] === "object") {
        merge(target[key], value);
      } else {
        target[key] = value;
      }
    }
    return target;
  }
  const normalize = (phrases) => phrases.map((p) => String(p).trim().toLowerCase()).filter(Boolean);
  // The app's messages, `<field>: <message>`, for the cases the checks use.
  function validate(doc) {
    const timeout = doc.session.idle_timeout_seconds;
    if (timeout !== 0 && !(timeout >= 5 && timeout <= 3600)) {
      throw `session.idle_timeout_seconds: ${timeout} is outside 0 or 5..3600`;
    }
    for (const [model, lists] of Object.entries(doc.wake.phrases || {})) {
      const activation = normalize(lists.activation || []);
      const deactivation = normalize(lists.deactivation || []);
      const shared = activation.find((phrase) => deactivation.includes(phrase));
      if (shared) {
        throw `wake.phrases.${model}.deactivation: «${shared}» is in both the activation and the deactivation list of «${model}»`;
      }
    }
    const local = doc.wake.model.startsWith("local:") ? doc.wake.model.slice(6) : null;
    const known = wakeModels.some((entry) => entry.source === "catalog" && entry.id === doc.wake.model);
    if (!known && (local === null || !local.trim() || /[\\/:]|\.\./.test(local))) {
      throw `wake.model: «${doc.wake.model}» is neither in the model catalog nor a local:<folder> model`;
    }
    for (const field of ["input_gain", "output_gain"]) {
      const gain = doc.audio[field];
      if (!(gain >= 0 && gain <= 4)) throw `audio.${field}: ${gain} is outside 0 to 4 (0 to 400 %)`;
    }
    if (!THEMES.includes(doc.ui.theme)) {
      throw `ui.theme: «${doc.ui.theme}» is not a theme; use one of ${THEMES.join(", ")}`;
    }
    if (!(doc.ui.font_size >= 11 && doc.ui.font_size <= 20)) {
      throw `ui.font_size: ${doc.ui.font_size} is outside 11 to 20`;
    }
    const modes = ["items", "commentary", "bem_tags", "thinking"];
    if (!modes.includes(doc.codex.handoff_mode)) {
      throw `codex.handoff_mode: «${doc.codex.handoff_mode}» is not an answer handoff mode; use one of ${modes.join(", ")}`;
    }
    // The app rejects a wrong type when it reads the patched document.
    for (const field of ["performance", "memory", "verbose_log"]) {
      if (typeof doc.diagnostics[field] !== "boolean") {
        throw `invalid type: expected a boolean at diagnostics.${field}`;
      }
    }
    if (typeof doc.assistant.save_chat_history !== "boolean") {
      throw "invalid type: expected a boolean at assistant.save_chat_history";
    }
  }

  function emit(name, payload) {
    for (const handler of listeners[name] || []) handler({ payload });
  }

  function status() {
    return {
      build_stamp: STAMP,
      data_dir: "C:\\VoiceBro",
      muted,
      hotkey: { shortcut: settings.hotkey.mute, registered: true, error: null },
      secrets: { discord_token: { is_set: tokenSet } },
      connection_errors: {},
      discord: {
        state: tokenSet ? "connected" : "disconnected",
        user_name: "Mock bot",
        guild_count: 1,
        guilds: [{ id: "1091401169441656933", name: "Mock server" }],
        voice_channels: !inVoice ? {} : {
          "1091401169441656933": {
            guild_name: "Mock server",
            channel_name: "General",
            participants: ["Alex"],
            output: { pushed: 0, dropped: 0, played: 0, underruns: 0, silence_frames: 0, flushed: 0 },
            delivery: (() => {
              // About one receiver report a second from Discord, with a lossy second now and then.
              const seconds = Math.floor((Date.now() - started) / 1000);
              const packets = Math.floor((Date.now() - started) / 20);
              return {
                voice_server: "finland14098.discord.media", region: "finland",
                packets_encoded: packets, passed_through: packets, encode_errors: 0,
                expected_loss_percent: seconds < 1 ? 10 : 4,
                expected_loss_source: seconds < 1 ? "default" : "measured",
                reports: seconds, last_reported_loss_percent: seconds ? (seconds % 7 === 0 ? 5.9 : 0.4) : null,
                last_reported_jitter_ms: seconds ? 1.4 : null,
              };
            })(),
          },
        },
        last_error: null,
        counters: { ticks: Math.floor((Date.now() - started) / 20), speaker_frames: 0, malformed_frames: 0, unnamed_sources: 0 },
      },
      wake: {
        state: wakeState(), engine: settings.wake.engine, model: settings.wake.model,
        activation_phrases: normalize(selectedPhrases().activation), deactivation_phrases: normalize(selectedPhrases().deactivation),
        unknown_words: normalize(selectedPhrases().activation).includes("zzz") ? ["zzz"] : [], dropped_chunks: 0,
      },
      gates: {
        "1091401169441656933": {
          guild_id: "1091401169441656933", mode: settings.session.mode, muted, active: false,
          listening_for_wake_word: !muted, session_wanted: false, assistant_ready: false,
          last_wake_phrase: null, replay_held: 0, counters: { input_dropped: 0, replay_dropped: 0, replay_timeouts: 0 },
        },
      },
      audio: {
        input: { gain: settings.audio.input_gain, peak: 0.12, clips: 0 },
        output: { gain: settings.audio.output_gain, peak: 0, clips: 0 },
      },
      assistant: {
        engine: "codex",
        summary: !codex.installed
          ? "codex not found: press Install Codex on the Status tab, or set the path of codex.exe in Config"
          : codex.signedIn ? "signed in (chatgpt)" : codex.accountRead ? "not signed in" : "codex found; it starts at /join",
        signed_in: codex.installed && codex.signedIn, account_type: codex.installed && codex.signedIn ? "chatgpt" : null,
        email: null, plan: "plus",
        executable: codex.installed ? "C:\\Users\\me\\AppData\\Local\\Microsoft\\WinGet\\Links\\codex.exe" : null,
        version: codex.installed ? "codex-cli 0.167.0" : null,
        running: codex.installed, error: null, model_in_use: null, reasoning_effort_in_use: null,
        calls: [{
          guild_id: "1091401169441656933", state: "live", connected: true, audio: (advanceCall(), clone(callAudio)),
          thread_id: "mock-thread", last_user_text: null, last_assistant_text: null, last_error: null,
        }],
      },
      latency: { "1091401169441656933": clone(turns) },
      settings: clone(settings),
    };
  }

  const commands = {
    status: () => status(),
    get_settings: () => ({ settings: clone(settings), secrets: { discord_token: { is_set: tokenSet } } }),
    save_settings: ({ patch }) => {
      const next = merge(clone(settings), patch);
      validate(next);
      const wakeChanged = JSON.stringify(next.wake) !== JSON.stringify(settings.wake) || next.session.mode !== settings.session.mode;
      settings = next;
      setTimeout(() => emit("app-event", { event: "settings_changed" }), 0);
      // The app's wake engine publishes its new state after a wake or session change.
      if (wakeChanged) setTimeout(() => emit("app-event", { event: "wake" }), 10);
      return clone(settings);
    },
    set_secret: ({ value }) => {
      if (!String(value).trim()) throw "the value is empty";
      tokenSet = true;
    },
    clear_secret: () => {
      tokenSet = false;
    },
    system_status: () => ({
      version: "1.0.0", build_stamp: STAMP, process_id: 4242,
      uptime_seconds: Math.floor((Date.now() - started) / 1000), runtime: "Rust / Tauri 2 / WebView2 (mock)",
      platform: "Windows (mock)", architecture: "x86_64", cpu_cores: 8,
    }),
    wake_models: () => wakeModels.map(({ asset, ...model }) => clone(model)),
    wake_requirements: ({ model }) => requirements(model),
    wake_download: ({ model }) => startDownload(model),
    open_models_folder: () => "C:\\VoiceBro\\models",
    capabilities: () => clone(capabilities),
    // `?sample` in the page's address: a sample session for the screenshots in docs/images.
    chat_log: () => (SAMPLE ? clone(SAMPLE_CHAT) : [
      { id: 1, at: "12:00:01", guild_id: "1091401169441656933", kind: "user", who: "Alex", text: "What time is it?" },
      { id: 2, at: "12:00:03", guild_id: "1091401169441656933", kind: "assistant", who: null, text: "It is noon." },
    ]),
    log_history: () => ["[2026-11-11][12:00:00][INFO][voicebro] VoiceBro 1.0.0+mock (mock log)"],
    set_muted: ({ muted: next }) => {
      muted = next;
      return muted;
    },
    toggle_mute: () => (muted = !muted),
    discord_connect: () => {},
    discord_disconnect: () => {},
    codex_check: () => {
      codex.accountRead = true;
      return { signed_in: codex.signedIn, kind: codex.signedIn ? "chatgpt" : null, email: null, plan: codex.signedIn ? "plus" : null };
    },
    codex_install_info: () => ({
      winget: codex.winget, package: "OpenAI.Codex", source: "OpenAI's GitHub releases (github.com/openai/codex)",
      download_mb: 200, disk_mb: 370, state: clone(install),
    }),
    // The console runs for 1.5 s; exit code 0 installs Codex (signed out), anything else not.
    codex_install: () => {
      if (install.kind === "running") throw "an install is already running in its console window";
      install = { kind: "running" };
      setTimeout(() => {
        install = { kind: "finished", code: codex.nextExitCode };
        if (codex.nextExitCode === 0) {
          codex.installed = true;
          codex.signedIn = false;
          codex.accountRead = false;
        }
        emit("app-event", { event: "codex_install", state: clone(install) });
      }, 1500);
    },
    codex_sign_in: () => "https://chatgpt.com/mock",
    codex_models: () => clone(capabilities.models),
    open_log_folder: () => "C:\\VoiceBro\\logs",
    app_info: () => ({ version: "1.0.0", build_stamp: STAMP, data_dir: "C:\\VoiceBro" }),
    // `null` while the switch is off, as the app answers when its reader is not running.
    memory_status: () => (settings.diagnostics.memory
      ? {
          total_mb: 172.4, app_mb: 61.2, webview_mb: 111.2, webview_processes: 5, codex_mb: null,
          metric: "private working set",
        }
      : null),
  };

  async function invoke(name, args = {}) {
    calls.push({ name, args: clone(args) });
    const command = commands[name];
    if (!command) {
      console.error(`mock: unknown command ${name}`);
      throw `unknown command ${name}`;
    }
    await new Promise((resolve) => setTimeout(resolve, 20));
    return command(args);
  }

  async function listen(name, handler) {
    (listeners[name] = listeners[name] || []).push(handler);
    return () => {
      listeners[name] = listeners[name].filter((h) => h !== handler);
    };
  }

  // Status events every second, as a connected app sends them; `quiet(true)` stops them
  // so the page's own polling can be counted.
  let tick = 0;
  let quietEvents = false;
  setInterval(() => {
    tick += 1;
    if (quietEvents) return;
    emit("app-event", { event: tick % 2 ? "discord" : "gate" });
    emit("app-log", `[2026-11-10][12:00:${String(tick % 60).padStart(2, "0")}][INFO][mock] status tick ${tick}`);
  }, 1000);

  window.__TAURI__ = { core: { invoke }, event: { listen } };
  setInterval(addTurn, 4000);
  window.__mock = {
    calls,
    settings: () => clone(settings),
    emit,
    // How many times a command was called since the page loaded, or since `sinceIndex`
    // (a previous `calls.length`), for checking how often the page polls.
    callsOf: (name, sinceIndex = 0) => calls.slice(sinceIndex).filter((call) => call.name === name).length,
    quiet(on) {
      quietEvents = on;
    },
    // The share of received packets the call conceals from now on (0.03 = 3 %).
    setLoss(rate) {
      lossRate = rate;
    },
    // Codex: `setCodex({ installed, signedIn, accountRead, winget, nextExitCode })` changes
    // what the status reports and what the next Install Codex ends with; the install state
    // goes back to idle.
    setCodex(changes) {
      Object.assign(codex, changes);
      install = { kind: "idle" };
      emit("app-event", { event: "assistant" });
    },
    // The wake engine: `wakeState()` as the status reports it; `failNextDownload()` makes
    // the next Download stop at 40 % with a hash error; `resetDownloads()` puts the English
    // model and the library back to "not downloaded" and forgets the consent;
    // `setInstalled(ids, library)` marks those models (and the library) as on disk.
    wakeState: () => wakeState(),
    requirements: (id) => requirements(id),
    failNextDownload() {
      failNextDownload = true;
    },
    resetDownloads(library = false) {
      wakeModels[0].installed = false;
      libraryInstalled = !library;
      consentFor = null;
      download = null;
      emit("app-event", { event: "wake" });
    },
    setInstalled(ids, library = true) {
      for (const model of wakeModels) if (ids.includes(model.id)) model.installed = true;
      libraryInstalled = library;
      emit("app-event", { event: "wake" });
    },
    // The bot joins or leaves the mock server's voice channel, as `/join` and `/leave` do.
    join() {
      inVoice = true;
      emit("app-event", { event: "discord" });
    },
    leave() {
      inVoice = false;
      emit("app-event", { event: "discord" });
    },
    changeCapabilities() {
      capabilities = clone(capabilities);
      capabilities.models.push({
        id: "model-c", display_name: "Model C", description: "", reasoning_efforts: ["minimal", "low"],
        default_reasoning_effort: "low", is_default: false,
      });
      emit("app-event", { event: "assistant" });
    },
    // One chat entry of each kind, as the app sends them: `event` names the event and
    // `kind` the entry.
    emitChatOfEveryKind() {
      const kinds = [
        ["user", "Alex", "Find where the rope gun is"],
        ["context", null, "In the channel: Alex, Sam"],
        ["assistant", null, "Let me check."],
        ["task", null, "Find the rope gun location in Sons of the Forest"],
        ["reasoning", null, "Searching the web for the location"],
        ["tool", null, "Web search: Sons of the Forest rope gun location"],
        ["agent", null, "It is in the maintenance bunker."],
        ["system", null, "Call started"],
        ["error", null, "The call ended unexpectedly"],
      ];
      kinds.forEach(([kind, who, text], index) => {
        emit("app-event", {
          event: "chat", id: 100 + index, guild_id: "1091401169441656933",
          at: `12:01:${String(index).padStart(2, "0")}`, kind, who, text,
        });
      });
    },
  };
})();
