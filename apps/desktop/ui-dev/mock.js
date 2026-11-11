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
    assistant: { engine: "codex", speaker_labels: "start" },
    // `handoff_mode` as the app fills it for a file without the field: the default.
    codex: { executable: "", model: "", reasoning_effort: "", voice: "", instructions: "Answer briefly.", handoff_mode: "items" },
    hotkey: { mute: "Ctrl+Alt+M" },
    ui: { theme: "dark", font_size: 13 },
    // Both off, as the app fills them for a file without the section.
    diagnostics: { performance: false, memory: false },
  };
  let tokenSet = true;
  let muted = false;
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
  const wakeModels = [
    { id: "vosk-model-small-en-us-0.15", engine: "vosk", language: "en-US", display_name: "Vosk small English (US) 0.15", license: "Apache-2.0", source: "catalog", installed: false, bytes: 41205931 },
    { id: "vosk-model-small-ru-0.22", engine: "vosk", language: "ru", display_name: "Vosk small Russian 0.22", license: "Apache-2.0", source: "catalog", installed: true, bytes: 46220000 },
    { id: "local:vosk-model-small-de-0.15", engine: "vosk", language: "de", display_name: "vosk-model-small-de-0.15", license: "", source: "local", installed: true, bytes: 0 },
  ];
  // The wake engine as the app runs it: the Vosk library is on disk (the Russian model was
  // used), a model that is not asks first, and Download walks through the progress.
  let libraryInstalled = true;
  let consentFor = null;
  let download = null; // { model, percent, failed }
  let failNextDownload = false;
  const LIBRARY = { label: "Vosk library", host: "github.com", bytes: 14900000 };
  // Missing lists are empty, as the app fills them when it reads the document.
  function selectedPhrases() {
    const entry = settings.wake.phrases[settings.wake.model] || {};
    return { activation: entry.activation || [], deactivation: entry.deactivation || [] };
  }
  function wakeState() {
    if (settings.session.mode !== "wake_word") return { kind: "off" };
    const phrases = selectedPhrases();
    if (!normalize(phrases.activation).length && !normalize(phrases.deactivation).length) return { kind: "no_phrases" };
    if (download && download.model === settings.wake.model) {
      if (download.failed) return { kind: "error", message: `${download.label}: SHA-256 mismatch; nothing was extracted` };
      return { kind: "downloading", label: download.label, percent: download.percent };
    }
    const model = wakeModels.find((entry) => entry.id === settings.wake.model);
    if (!model) return { kind: "error", message: `the model folder C:\\VoiceBro\\models\\${settings.wake.model.slice(6)} is missing; put the model back or choose another one` };
    const items = [];
    if (!libraryInstalled) items.push(clone(LIBRARY));
    if (!model.installed) items.push({ label: model.language === "ru" ? "Vosk Russian model" : "Vosk English model", host: "alphacephei.com", bytes: model.bytes });
    if (items.length && consentFor !== settings.wake.model) return { kind: "needs_download", items };
    return { kind: "ready" };
  }
  function startDownload() {
    const state = wakeState();
    if (state.kind !== "needs_download" && state.kind !== "error") throw "nothing is waiting for a download";
    consentFor = settings.wake.model;
    const model = wakeModels.find((entry) => entry.id === settings.wake.model);
    download = { model: model.id, label: model.language === "ru" ? "Vosk Russian model" : "Vosk English model", percent: 0, failed: false };
    const step = () => {
      if (!download) return;
      if (failNextDownload && download.percent >= 40) {
        failNextDownload = false;
        download.failed = true;
        emit("app-event", { event: "wake" });
        return;
      }
      download.percent += 20;
      if (download.percent >= 100) {
        model.installed = true;
        libraryInstalled = true;
        download = null;
      }
      emit("app-event", { event: "wake" });
      if (download) setTimeout(step, 400);
    };
    setTimeout(step, 400);
  }
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
    for (const field of ["performance", "memory"]) {
      if (typeof doc.diagnostics[field] !== "boolean") {
        throw `invalid type: expected a boolean at diagnostics.${field}`;
      }
    }
  }

  function emit(name, payload) {
    for (const handler of listeners[name] || []) handler({ payload });
  }

  function status() {
    return {
      build_stamp: "0.2.0+mock 2026-11-10",
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
        voice_channels: {
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
        engine: "codex", summary: "signed in (chatgpt)", signed_in: true, account_type: "chatgpt",
        email: null, plan: "plus", executable: "C:\\codex\\codex.exe", version: "codex-cli 0.167.0",
        running: true, error: null, model_in_use: null, reasoning_effort_in_use: null,
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
      version: "0.2.0", build_stamp: "0.2.0+mock 2026-11-10", process_id: 4242,
      uptime_seconds: Math.floor((Date.now() - started) / 1000), runtime: "Rust / Tauri 2 / WebView2 (mock)",
      platform: "Windows (mock)", architecture: "x86_64", cpu_cores: 8,
    }),
    wake_models: () => clone(wakeModels),
    wake_download: () => startDownload(),
    open_models_folder: () => "C:\\VoiceBro\\models",
    capabilities: () => clone(capabilities),
    chat_log: () => [
      { id: 1, at: "12:00:01", guild_id: "1091401169441656933", kind: "user", who: "Alex", text: "What time is it?" },
      { id: 2, at: "12:00:03", guild_id: "1091401169441656933", kind: "assistant", who: null, text: "It is noon." },
    ],
    log_history: () => ["[2026-11-10][12:00:00][INFO][voicebro] VoiceBro 0.2.0+mock (mock log)"],
    set_muted: ({ muted: next }) => {
      muted = next;
      return muted;
    },
    toggle_mute: () => (muted = !muted),
    discord_connect: () => {},
    discord_disconnect: () => {},
    codex_check: () => ({ signed_in: true, kind: "chatgpt", email: null, plan: "plus" }),
    codex_sign_in: () => "https://chatgpt.com/mock",
    codex_models: () => clone(capabilities.models),
    open_log_folder: () => "C:\\VoiceBro\\logs",
    app_info: () => ({ version: "0.2.0", build_stamp: "0.2.0+mock 2026-11-10", data_dir: "C:\\VoiceBro" }),
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
    // The wake engine: `wakeState()` as the status reports it; `failNextDownload()` makes
    // the next Download stop at 40 % with a hash error; `resetDownloads()` puts the English
    // model and the library back to "not downloaded" and forgets the consent.
    wakeState: () => wakeState(),
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
