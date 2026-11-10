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
    version: 1,
    discord: { guild_id: "1091401169441656933" },
    session: { mode: "wake_word", idle_timeout_seconds: 60, earcons: true, earcon_volume: 0.18 },
    audio: { input_gain: 1, output_gain: 1 },
    wake: {
      engine: "vosk",
      model: "vosk-model-small-ru-0.22",
      activation_phrases: ["alpha"],
      deactivation_phrases: ["omega"],
    },
    assistant: { engine: "codex", speaker_labels: "start" },
    // `handoff_mode` as the app fills it for a file without the field: the default.
    codex: { executable: "", model: "", reasoning_effort: "", voice: "", instructions: "Answer briefly.", handoff_mode: "items" },
    hotkey: { mute: "Ctrl+Alt+M" },
    ui: { theme: "dark", font_size: 13 },
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
  const wakeModels = [
    { id: "vosk-model-small-ru-0.22", engine: "vosk", language: "ru", display_name: "Vosk small Russian 0.22 (45 MB)", license: "Apache-2.0" },
    { id: "vosk-model-small-en-us-0.15", engine: "vosk", language: "en-US", display_name: "Vosk small English 0.15 (40 MB)", license: "Apache-2.0" },
  ];
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
    const activation = normalize(doc.wake.activation_phrases);
    const deactivation = normalize(doc.wake.deactivation_phrases);
    const shared = activation.find((phrase) => deactivation.includes(phrase));
    if (shared) {
      throw `wake.deactivation_phrases: «${shared}» is in both the activation and the deactivation list`;
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
          },
        },
        last_error: null,
        counters: { ticks: Math.floor((Date.now() - started) / 20), speaker_frames: 0, malformed_frames: 0, unnamed_sources: 0 },
      },
      wake: { state: { kind: "ready" }, model: settings.wake.model, unknown_words: [], dropped_chunks: 0 },
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
      settings = next;
      setTimeout(() => emit("app-event", { event: "settings_changed" }), 0);
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
    memory_status: () => ({
      total_mb: 172.4, app_mb: 61.2, webview_mb: 111.2, webview_processes: 5, codex_mb: null,
      metric: "private working set",
    }),
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

  // Status events every second, as a connected app sends them.
  let tick = 0;
  setInterval(() => {
    tick += 1;
    emit("app-event", { event: tick % 2 ? "discord" : "gate" });
    emit("app-log", `[2026-11-10][12:00:${String(tick % 60).padStart(2, "0")}][INFO][mock] status tick ${tick}`);
  }, 1000);

  window.__TAURI__ = { core: { invoke }, event: { listen } };
  setInterval(addTurn, 4000);
  window.__mock = {
    calls,
    settings: () => clone(settings),
    emit,
    // The share of received packets the call conceals from now on (0.03 = 3 %).
    setLoss(rate) {
      lossRate = rate;
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
