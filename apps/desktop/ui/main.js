// VoiceBro UI. Talks to Rust through window.__TAURI__ (withGlobalTauri): tabs, cards and a
// status bar over this app's commands and events.
(() => {
  const tauri = window.__TAURI__;
  const invoke = tauri?.core?.invoke;
  const listen = tauri?.event?.listen;
  const $ = (id) => document.getElementById(id);
  const setText = (id, text) => {
    const node = $(id);
    if (!node) return;
    node.textContent = text;
    node.title = text;
  };

  // ---------- Console: the app log as the file has it, forwarded as `app-log` ----------
  const logPane = $("log");
  const MAX_LOG_LINES = 500;
  function appendLogLine(line) {
    logPane.textContent += `${line}\n`;
    const lines = logPane.textContent.split("\n");
    if (lines.length > MAX_LOG_LINES + 1) {
      logPane.textContent = lines.slice(lines.length - MAX_LOG_LINES - 1).join("\n");
    }
    if ($("console-autoscroll").checked) logPane.scrollTop = logPane.scrollHeight;
  }
  // The few messages the window itself produces.
  function appendLog(line) {
    appendLogLine(`[window][${new Date().toLocaleTimeString()}] ${line}`);
  }
  $("console-clear").addEventListener("click", () => {
    logPane.textContent = "";
  });
  $("console-copy").addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(logPane.textContent);
      appendLog("console copied to the clipboard");
    } catch (error) {
      appendLog(`copy failed: ${error}`);
    }
  });

  // ---------- Tabs: one view at a time; each remembers how far it was scrolled ----------
  const tabButtons = [...document.querySelectorAll(".tab-button")];
  const tabPanels = [...document.querySelectorAll(".tab-panel")];
  const savedScroll = new Map();
  function showTab(name) {
    const open = tabPanels.find((panel) => panel.classList.contains("active"));
    if (open) savedScroll.set(open.dataset.panel, open.scrollTop);
    for (const button of tabButtons) button.classList.toggle("active", button.dataset.tab === name);
    for (const panel of tabPanels) {
      const shown = panel.dataset.panel === name;
      panel.classList.toggle("active", shown);
      if (shown) panel.scrollTop = savedScroll.get(name) || 0;
    }
    if (name === "system") refreshSystem();
  }
  for (const button of tabButtons) button.addEventListener("click", () => showTab(button.dataset.tab));

  // ---------- Appearance ----------
  function applyAppearance(ui) {
    if (!ui) return;
    document.documentElement.dataset.theme = ui.theme || "dark";
    document.documentElement.style.setProperty("--font-size", `${ui.font_size || 13}px`);
  }

  // ---------- Settings fields: each carries its JSON path and value kind ----------
  function readByPath(object, path) {
    return path.split(".").reduce((o, key) => (o == null ? undefined : o[key]), object);
  }
  function patchFor(path, value) {
    const keys = path.split(".");
    const patch = {};
    let cursor = patch;
    keys.forEach((key, index) => {
      if (index === keys.length - 1) cursor[key] = value;
      else cursor = cursor[key] = {};
    });
    return patch;
  }
  function mergePatch(target, patch) {
    for (const [key, value] of Object.entries(patch)) {
      if (value && typeof value === "object" && !Array.isArray(value) && target[key] && typeof target[key] === "object") {
        mergePatch(target[key], value);
      } else {
        target[key] = value;
      }
    }
    return target;
  }
  function controlValue(control) {
    switch (control.dataset.kind) {
      case "number": return Number(control.value);
      // A slider in percent for a multiplier: 339 % is stored as 3.39.
      case "percent": return Number(control.value) / 100;
      case "bool": return control.checked;
      case "list":
        return control.value
          .split(/[\n,]/)
          .map((item) => item.trim())
          .filter((item) => item.length > 0);
      default: return control.value;
    }
  }
  function percentLabel(control) {
    const label = $(`${control.id}-label`);
    if (label) label.textContent = `${control.value} %`;
  }
  function setControlValue(control, value) {
    if (document.activeElement === control) return;
    if (control.dataset.kind === "percent") {
      control.value = Math.round(Number(value ?? 0) * 100);
      percentLabel(control);
      return;
    }
    if (control.dataset.kind === "bool") control.checked = Boolean(value);
    else if (control.dataset.kind === "list") control.value = Array.isArray(value) ? value.join("\n") : "";
    else control.value = value == null ? "" : value;
    // A listed field mirrors its text input in the select when the select has options.
    const select = document.querySelector(`select.listed[data-for="${control.id}"]`);
    if (select && select.options.length > 0 && document.activeElement !== select) {
      const text = control.value;
      if (![...select.options].some((option) => option.value === text)) {
        const option = document.createElement("option");
        option.value = text;
        option.textContent = text ? `${text} (not in the current list)` : "(default)";
        select.appendChild(option);
      }
      select.value = text;
    }
  }
  function fieldMessage(id, text, isError) {
    const node = document.querySelector(`.field-message[data-for="${id}"]`);
    if (!node) return;
    node.textContent = text;
    node.classList.toggle("error", Boolean(isError));
    if (!isError && text) setTimeout(() => {
      if (node.textContent === text) node.textContent = "";
    }, 2000);
  }
  async function saveControl(control) {
    if (!invoke) return;
    fieldMessage(control.id, "Saving…", false);
    try {
      await invoke("save_settings", { patch: patchFor(control.dataset.path, controlValue(control)) });
      fieldMessage(control.id, "Saved", false);
    } catch (error) {
      fieldMessage(control.id, String(error), true);
      // A rejected slider returns to the stored value on the refresh below.
      if (control.type === "range") control.blur();
    }
    await refresh();
  }
  // Every Settings field saves on its own change: text when leaving the field or on Enter,
  // sliders on release, selects and switches at once.
  document.querySelectorAll("[data-path]").forEach((control) => {
    if (control.dataset.kind === "percent") control.addEventListener("input", () => percentLabel(control));
    control.addEventListener("change", () => {
      if (control.dataset.path.startsWith("ui.")) {
        applyAppearance({ theme: $("setting-theme").value, font_size: Number($("setting-font-size").value) });
      }
      saveControl(control);
    });
  });

  // Listed fields: a select from the live list when there is one, the text input otherwise.
  function fillListed(id, options) {
    const input = $(id);
    const select = document.querySelector(`select.listed[data-for="${id}"]`);
    const datalist = $(`${id}-list`);
    datalist.innerHTML = "";
    for (const option of options) {
      const node = document.createElement("option");
      node.value = option.value;
      datalist.appendChild(node);
    }
    select.innerHTML = "";
    if (options.length === 0) {
      input.style.display = "";
      return;
    }
    for (const option of options) {
      const node = document.createElement("option");
      node.value = option.value;
      node.textContent = option.label;
      select.appendChild(node);
    }
    input.style.display = "none";
    const current = input.value;
    if (![...select.options].some((option) => option.value === current)) {
      const node = document.createElement("option");
      node.value = current;
      node.textContent = current ? `${current} (not in the current list)` : "(default)";
      select.appendChild(node);
    }
    select.value = current;
  }
  document.querySelectorAll("select.listed").forEach((select) => {
    select.addEventListener("change", () => {
      const input = $(select.dataset.for);
      input.value = select.value;
      if (select.dataset.for === "codex-model") applyEffortList();
      saveControl(input);
    });
  });

  let wakeModels = [];
  let capabilities = { models: [], voices: [] };
  function applyWakeLists(settings) {
    const fill = (select, items, current) => {
      if (document.activeElement === select) return;
      select.innerHTML = "";
      for (const item of items) {
        const option = document.createElement("option");
        option.value = item.value;
        option.textContent = item.label;
        select.appendChild(option);
      }
      if (current && ![...select.options].some((option) => option.value === current)) {
        const option = document.createElement("option");
        option.value = current;
        option.textContent = `${current} (not in the catalog)`;
        select.appendChild(option);
      }
      select.value = current || "";
    };
    const engines = [...new Set(wakeModels.map((model) => model.engine))];
    fill($("wake-engine"), engines.map((engine) => ({ value: engine, label: engine })), settings?.wake?.engine);
    fill(
      $("wake-model"),
      wakeModels.map((model) => ({ value: model.id, label: `${model.display_name} [${model.language}, ${model.license}]` })),
      settings?.wake?.model,
    );
  }
  function applyEffortList() {
    const modelId = $("codex-model").value;
    const model = capabilities.models.find((entry) => entry.id === modelId)
      || capabilities.models.find((entry) => entry.is_default);
    const efforts = model ? model.reasoning_efforts : [];
    const options = [{ value: "", label: model?.default_reasoning_effort ? `(default: ${model.default_reasoning_effort})` : "(default)" }]
      .concat(efforts.map((effort) => ({ value: effort, label: effort })));
    fillListed("codex-effort", efforts.length ? options : []);
  }
  function applyCodexLists() {
    const models = capabilities.models || [];
    fillListed(
      "codex-model",
      models.length
        ? [{ value: "", label: "(account default)" }].concat(
            models.map((model) => ({
              value: model.id,
              label: `${model.display_name}${model.is_default ? " (default)" : ""} [${model.reasoning_efforts.join(", ") || "no efforts"}]`,
            })),
          )
        : [],
    );
    applyEffortList();
    const voices = capabilities.voices || [];
    fillListed(
      "codex-voice",
      voices.length
        ? [{ value: "", label: "(server default)" }].concat(voices.map((voice) => ({ value: voice, label: voice })))
        : [],
    );
  }
  async function loadLists() {
    if (!invoke) return;
    try {
      wakeModels = await invoke("wake_models");
    } catch (error) {
      appendLog(`wake_models failed: ${error}`);
    }
    try {
      capabilities = await invoke("capabilities");
    } catch (error) {
      appendLog(`capabilities failed: ${error}`);
    }
    applyCodexLists();
  }

  // ---------- Config tab: edited fields wait for Save changes ----------
  const configDirty = new Set();
  let storedSettings = null;
  document.querySelectorAll("[data-config-path]").forEach((control) => {
    control.addEventListener("input", () => {
      configDirty.add(control.id);
      $("config-feedback").textContent = "Unsaved changes.";
      $("config-feedback").className = "form-feedback warning-text";
    });
  });
  $("config-token").addEventListener("input", () => {
    $("config-feedback").textContent = "Unsaved changes.";
    $("config-feedback").className = "form-feedback warning-text";
  });
  function fillConfig(settings, force) {
    document.querySelectorAll("[data-config-path]").forEach((control) => {
      if (!force && (configDirty.has(control.id) || document.activeElement === control)) return;
      const value = readByPath(settings, control.dataset.configPath);
      control.value = value == null ? "" : value;
    });
  }
  $("config-reset").addEventListener("click", () => {
    configDirty.clear();
    $("config-token").value = "";
    if (storedSettings) fillConfig(storedSettings, true);
    $("config-feedback").textContent = "Reset to the stored values.";
    $("config-feedback").className = "form-feedback";
  });
  $("config-save").addEventListener("click", async () => {
    const feedback = $("config-feedback");
    const patch = {};
    for (const id of configDirty) {
      const control = $(id);
      mergePatch(patch, patchFor(control.dataset.configPath, control.value.trim()));
    }
    const token = $("config-token").value.trim();
    try {
      if (Object.keys(patch).length) await invoke("save_settings", { patch });
      if (token) {
        await invoke("set_secret", { name: "discord_token", value: token });
        $("config-token").value = "";
      }
      configDirty.clear();
      feedback.textContent = Object.keys(patch).length || token ? "Saved." : "Nothing to save.";
      feedback.className = "form-feedback";
    } catch (error) {
      feedback.textContent = String(error);
      feedback.className = "form-feedback error-text";
    }
    await refresh();
  });
  $("token-clear").addEventListener("click", async () => {
    try {
      await invoke("clear_secret", { name: "discord_token" });
      $("config-feedback").textContent = "Token cleared; Discord disconnected.";
      $("config-feedback").className = "form-feedback";
    } catch (error) {
      $("config-feedback").textContent = `Token not cleared: ${error}`;
      $("config-feedback").className = "form-feedback error-text";
    }
    await refresh();
  });

  // ---------- Codex chat ----------
  const chatPane = $("chat");
  let chatEntries = [];
  const CHAT_GROUPS = {
    context: ["context"],
    agent: ["task", "agent", "reasoning", "tool"],
    system: ["system"],
  };
  function chatVisible(entry) {
    for (const [group, kinds] of Object.entries(CHAT_GROUPS)) {
      if (kinds.includes(entry.kind) && !$(`chat-filter-${group}`).checked) return false;
    }
    return true;
  }
  function chatWho(entry) {
    return entry.who || (entry.kind === "user" ? "user" : entry.kind);
  }
  function chatLineText(entry) {
    return `${entry.at} [${entry.guild_id}] ${chatWho(entry)}: ${entry.text}`;
  }
  function renderChat() {
    chatPane.innerHTML = "";
    for (const entry of chatEntries) {
      if (!chatVisible(entry)) continue;
      const line = document.createElement("div");
      line.className = `chat-entry kind-${entry.kind}`;
      const at = document.createElement("span");
      at.className = "chat-time";
      at.textContent = entry.at;
      const who = document.createElement("span");
      who.className = "chat-who";
      who.textContent = chatWho(entry);
      const text = document.createElement("span");
      text.className = "chat-text";
      text.textContent = entry.text;
      line.append(at, who, text);
      chatPane.appendChild(line);
    }
    if (chatPane.childElementCount === 0) {
      const empty = document.createElement("div");
      empty.className = "empty-state";
      empty.textContent = "The conversation appears here once a session starts.";
      chatPane.appendChild(empty);
    }
    if ($("chat-autoscroll").checked) chatPane.scrollTop = chatPane.scrollHeight;
  }
  function addChatEntry(entry) {
    chatEntries.push(entry);
    if (chatEntries.length > 600) chatEntries = chatEntries.slice(chatEntries.length - 600);
    renderChat();
  }
  ["chat-filter-context", "chat-filter-agent", "chat-filter-system"].forEach((id) => {
    $(id).addEventListener("change", renderChat);
  });
  $("chat-copy").addEventListener("click", async () => {
    const text = chatEntries.filter(chatVisible).map(chatLineText).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      appendLog("chat copied to the clipboard");
    } catch (error) {
      appendLog(`copy failed: ${error}`);
    }
  });
  $("chat-open-folder").addEventListener("click", async () => {
    try {
      appendLog(`opened ${await invoke("open_log_folder")}`);
    } catch (error) {
      appendLog(`open log folder failed: ${error}`);
    }
  });
  async function loadChat() {
    if (!invoke) return;
    try {
      chatEntries = await invoke("chat_log");
      renderChat();
    } catch (error) {
      appendLog(`chat log failed: ${error}`);
    }
  }

  // ---------- Status ----------
  function wakeText(wake) {
    const state = wake.state || {};
    let text;
    switch (state.kind) {
      case "off": text = "off (the session mode is not wake word)"; break;
      case "no_phrases": text = "no phrases configured"; break;
      case "downloading": text = `downloading the ${state.label}: ${state.percent}%`; break;
      case "loading": text = `loading ${wake.model}`; break;
      case "ready": text = `ready (${wake.model})`; break;
      case "error": text = `error: ${state.message}`; break;
      default: text = "unknown";
    }
    if (wake.dropped_chunks) text += `; ${wake.dropped_chunks} audio chunks dropped`;
    return text;
  }
  function gateText(gate) {
    const parts = [gate.mode === "always" ? "always on" : gate.active ? "session active" : "waiting for the wake word"];
    if (gate.muted) parts.push("muted");
    if (gate.session_wanted) parts.push(gate.assistant_ready ? "assistant ready" : "assistant connecting");
    if (gate.last_wake_phrase) parts.push(`last phrase “${gate.last_wake_phrase}”`);
    if (gate.replay_held) parts.push(`${gate.replay_held} frames held`);
    return parts.join(", ");
  }
  function levelText(reading) {
    if (!reading) return "";
    const peak = Math.round((reading.peak || 0) * 100);
    return `Peak in the last second: ${peak} % of full scale. Clipped samples since start: ${reading.clips || 0}.`;
  }
  const STATE_LABELS = { connected: "Connected", connecting: "Connecting", disconnected: "Disconnected", error: "Error" };
  let lastStatus = null;

  function renderGuilds(discord) {
    const list = $("guild-list");
    const guilds = discord.guilds || [];
    $("guild-count").textContent = String(guilds.length || discord.guild_count || 0);
    list.innerHTML = "";
    if (guilds.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty-state";
      empty.textContent = discord.state === "connected" ? "The bot is in no server." : "Waiting for Discord...";
      list.appendChild(empty);
      return;
    }
    for (const guild of guilds) {
      const item = document.createElement("div");
      item.className = "list-item";
      const voice = (discord.voice_channels || {})[guild.id];
      if (voice) item.classList.add("selected");
      const name = document.createElement("strong");
      name.textContent = guild.name;
      const detail = document.createElement("small");
      detail.textContent = voice ? `in ${voice.channel_name}` : `id ${guild.id}`;
      item.append(name, detail);
      list.appendChild(item);
    }
  }

  function renderStatus(status) {
    lastStatus = status;
    const discord = status.discord || {};
    const assistant = status.assistant || {};
    const channels = Object.values(discord.voice_channels || {});

    // Status bar.
    const footer = $("footer-connection");
    footer.textContent = STATE_LABELS[discord.state] || "Unknown";
    footer.className = `connection-state ${discord.state || "disconnected"}`;
    $("footer-guilds").textContent = String(discord.guild_count || 0);
    $("footer-voice").textContent = String(channels.length);

    // Status tab.
    renderGuilds(discord);
    let discordText = STATE_LABELS[discord.state] || "unknown";
    if (discord.state === "error" && discord.last_error) discordText = `Error: ${discord.last_error}`;
    setText("status-discord", discordText);
    setText("status-bot", discord.user_name || "—");
    setText("status-voice", channels.length ? channels.map((c) => `${c.guild_name} / ${c.channel_name}`).join("; ") : "none");
    const people = channels.flatMap((c) => c.participants || []);
    setText("status-people", people.length ? people.join(", ") : "—");
    setText("status-engine", `${assistant.engine || "—"}${assistant.version ? ` (${assistant.version})` : ""}`);
    const gates = Object.values(status.gates || {});
    setText("status-gates", gates.length ? gates.map(gateText).join("; ") : "not in a voice channel");
    setText("status-wake", wakeText(status.wake || {}));
    setText("status-mute", status.muted ? "muted" : "not muted");
    $("mute-toggle").textContent = status.muted ? "Unmute GPT" : "Mute GPT";
    setText("status-codex", assistant.summary || "—");
    const calls = assistant.calls || [];
    setText("status-call", calls.length
      ? calls.map((call) => `${call.state}${call.connected ? ", connected" : ""}${call.last_error ? `, error: ${call.last_error}` : ""}`).join("; ")
      : "no call");
    setText("status-thread", assistant.model_in_use != null
      ? `${assistant.model_in_use || "default model"}, reasoning ${assistant.reasoning_effort_in_use || "default"}`
      : "—");
    const hotkey = status.hotkey || {};
    setText("status-hotkey", !hotkey.shortcut
      ? "none configured"
      : hotkey.registered ? `${hotkey.shortcut} (registered)` : `${hotkey.shortcut}: not registered${hotkey.error ? `: ${hotkey.error}` : ""}`);
    const tokenSet = Boolean(status.secrets?.discord_token?.is_set);
    setText("status-token", tokenSet ? "stored" : "not stored");
    setText("status-build", status.build_stamp || "—");
    $("discord-connect").disabled = !tokenSet || discord.state === "connected" || discord.state === "connecting";
    $("discord-disconnect").disabled = discord.state === "disconnected";

    const errors = Object.entries(status.connection_errors || {});
    const list = $("status-errors");
    list.innerHTML = "";
    if (errors.length === 0) {
      const li = document.createElement("li");
      li.className = "muted";
      li.textContent = "none";
      list.appendChild(li);
    } else {
      for (const [source, message] of errors) {
        const li = document.createElement("li");
        li.textContent = `${source}: ${message}`;
        list.appendChild(li);
      }
    }
    const counters = { voice: discord.counters || {}, audio: status.audio || {} };
    for (const [guild, channel] of Object.entries(discord.voice_channels || {})) {
      counters[`output ${guild}`] = channel.output;
    }
    for (const call of calls) {
      if (call.audio) counters[`call ${call.guild_id}`] = call.audio;
    }
    $("status-counters").textContent = JSON.stringify(counters, null, 2);

    // Settings tab.
    applyAppearance(status.settings?.ui);
    applyWakeLists(status.settings);
    document.querySelectorAll("[data-path]").forEach((control) => {
      setControlValue(control, readByPath(status.settings, control.dataset.path));
    });
    $("input-gain-level").textContent = levelText(status.audio?.input);
    $("output-gain-level").textContent = levelText(status.audio?.output);
    const unknown = (status.wake || {}).unknown_words || [];
    $("unknown-words").textContent = unknown.length
      ? `Not in the model vocabulary (these phrases are ignored): ${unknown.join(", ")}`
      : "";

    // Config tab.
    storedSettings = status.settings;
    fillConfig(status.settings, false);
    const badge = $("token-badge");
    badge.textContent = tokenSet ? "stored" : "not stored";
    badge.className = `badge ${tokenSet ? "ok" : "warn"}`;
    $("config-preview").textContent = JSON.stringify(status.settings, null, 2);
    const settingsPath = `${status.data_dir}\\settings.json`;
    setText("config-path", settingsPath);

    // System tab, the parts the status knows.
    setText("system-data-dir", status.data_dir || "—");
    setText("system-settings-path", settingsPath);
    setText("system-codex", assistant.executable || "not found");
  }

  async function refresh() {
    if (!invoke) return;
    try {
      renderStatus(await invoke("status"));
    } catch (error) {
      appendLog(`status failed: ${error}`);
    }
  }

  // ---------- System and uptime ----------
  let systemFacts = null;
  let systemFetchedAt = 0;
  function formatUptime(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = Math.floor(seconds % 60);
    const pad = (n) => String(n).padStart(2, "0");
    return h > 0 ? `${h}h ${pad(m)}m ${pad(s)}s` : `${m}m ${pad(s)}s`;
  }
  function uptimeNow() {
    if (!systemFacts) return null;
    return systemFacts.uptime_seconds + (Date.now() - systemFetchedAt) / 1000;
  }
  function renderUptime() {
    const uptime = uptimeNow();
    if (uptime == null) return;
    $("footer-uptime").textContent = formatUptime(uptime);
    $("system-uptime").textContent = formatUptime(uptime);
  }
  async function refreshSystem() {
    if (!invoke) return;
    try {
      systemFacts = await invoke("system_status");
      systemFetchedAt = Date.now();
      setText("system-version", systemFacts.version);
      setText("system-build", systemFacts.build_stamp);
      setText("system-pid", String(systemFacts.process_id));
      setText("system-runtime", systemFacts.runtime);
      setText("system-platform", systemFacts.platform);
      setText("system-arch", systemFacts.architecture);
      setText("system-cpus", String(systemFacts.cpu_cores));
      $("build-stamp").textContent = `build ${systemFacts.build_stamp}`;
      renderUptime();
    } catch (error) {
      appendLog(`system_status failed: ${error}`);
    }
  }

  // ---------- Actions ----------
  $("mute-toggle").addEventListener("click", async () => {
    const status = await invoke("status");
    await invoke("set_muted", { muted: !status.muted });
    await refresh();
  });
  $("discord-connect").addEventListener("click", async () => {
    try {
      await invoke("discord_connect");
    } catch (error) {
      appendLog(`connect: ${error}`);
    }
    await refresh();
  });
  $("discord-disconnect").addEventListener("click", async () => {
    await invoke("discord_disconnect");
    await refresh();
  });
  $("codex-sign-in").addEventListener("click", async () => {
    try {
      await invoke("codex_sign_in");
      $("status-feedback").textContent = "The ChatGPT sign-in page opened. Finish it there, then press Check Codex.";
    } catch (error) {
      $("status-feedback").textContent = `Sign-in failed: ${error}`;
    }
  });
  $("codex-check").addEventListener("click", async () => {
    try {
      const account = await invoke("codex_check");
      $("status-feedback").textContent = account.signed_in
        ? `Codex is signed in (${account.kind || "account"}).`
        : "Codex is not signed in; press Sign in to Codex.";
    } catch (error) {
      $("status-feedback").textContent = `Codex check failed: ${error}`;
    }
    await loadLists();
    await refresh();
  });
  $("status-refresh").addEventListener("click", () => {
    refresh();
    refreshSystem();
  });

  // ---------- Start ----------
  async function start() {
    if (!invoke) {
      appendLog("The Tauri API is not available; the page shows its layout only.");
      return;
    }
    if (listen) {
      // Subscribe first, then load what was logged before the window existed.
      await listen("app-log", ({ payload }) => appendLogLine(payload));
      try {
        const history = await invoke("log_history");
        logPane.textContent = history.length ? `${history.join("\n")}\n` : "";
        logPane.scrollTop = logPane.scrollHeight;
      } catch (error) {
        appendLog(`log history failed: ${error}`);
      }
      await listen("app-event", ({ payload }) => {
        switch (payload.kind) {
          // Log lines arrive through `app-log` with everything else the log file gets.
          case "log":
            break;
          case "chat":
            addChatEntry(payload);
            break;
          case "assistant":
            refresh();
            loadLists();
            break;
          default:
            refresh();
        }
      });
    }
    await refreshSystem();
    await loadLists();
    await refresh();
    await loadChat();
    setInterval(refresh, 5000);
    setInterval(renderUptime, 1000);
  }

  start();
})();
