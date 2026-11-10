// VoiceBro UI. Talks to Rust through window.__TAURI__ (withGlobalTauri): tabs, cards and a
// status bar over this app's commands and events.
//
// The settings form is kept apart from the status: refreshes on events and every 5 s update
// the status values only. A form field is written from the stored settings at load, after
// its own save and after a settings change, and only while it is clean (no unsaved or
// rejected value), not focused and, for a list, not open.
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

  // ---------- Console: the app log, forwarded line by line as `app-log` ----------
  // The lines kept from before the window opened, then every new record, in a read-only
  // text area. The search box narrows the view to the lines that contain its text.
  const consoleOutput = $("console-output");
  const CONSOLE_LIMIT = 2000;
  let consoleLines = [];
  let consolePending = false;
  const consoleQuery = () => $("console-search").value.trim().toLowerCase();
  function visibleConsoleLines() {
    const query = consoleQuery();
    if (!query) return consoleLines;
    return consoleLines.filter((line) => line.toLowerCase().includes(query));
  }
  function renderConsole() {
    consolePending = false;
    const visible = visibleConsoleLines();
    const top = consoleOutput.scrollTop;
    consoleOutput.value = visible.join("\n");
    const found = consoleQuery() ? visible.length : 0;
    $("console-match-count").textContent = found === 1 ? "1 match" : `${found} matches`;
    // With auto-scroll the view follows new lines; without it, it stays where it was.
    consoleOutput.scrollTop = $("console-autoscroll").checked ? consoleOutput.scrollHeight : top;
  }
  function appendLogLine(line) {
    consoleLines.push(line);
    const excess = consoleLines.length - CONSOLE_LIMIT;
    if (excess > 0) consoleLines = consoleLines.slice(excess);
    if (!consolePending) {
      consolePending = true;
      requestAnimationFrame(renderConsole);
    }
  }
  // The few messages the window itself produces.
  function appendLog(line) {
    appendLogLine(`[window][${new Date().toLocaleTimeString()}] ${line}`);
  }
  async function copyText(text, what) {
    try {
      await navigator.clipboard.writeText(text);
      appendLog(`${what} copied to the clipboard`);
    } catch (error) {
      appendLog(`copy failed: ${error}`);
    }
  }
  $("console-search").addEventListener("input", renderConsole);
  $("console-autoscroll").addEventListener("change", renderConsole);
  // Clear empties this view only; the log file keeps every line.
  $("console-clear").addEventListener("click", () => {
    consoleLines = [];
    renderConsole();
  });
  $("console-copy").addEventListener("click", () => copyText(visibleConsoleLines().join("\n"), "console"));
  $("console-copy-selection").addEventListener("click", () => {
    const { selectionStart: from, selectionEnd: to, value } = consoleOutput;
    if (to > from) copyText(value.slice(from, to), "selection");
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

  // ---------- Values and paths ----------
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
  function sameValue(a, b) {
    return JSON.stringify(a ?? null) === JSON.stringify(b ?? null);
  }
  function percentLabel(control) {
    const label = $(`${control.id}-label`);
    if (label) label.textContent = `${control.value} %`;
  }
  function writeControl(control, value) {
    if (control.dataset.kind === "percent") {
      control.value = Math.round(Number(value ?? 0) * 100);
      percentLabel(control);
    } else if (control.dataset.kind === "bool") {
      control.checked = Boolean(value);
    } else if (control.dataset.kind === "list") {
      control.value = Array.isArray(value) ? value.join("\n") : "";
    } else {
      control.value = value == null ? "" : value;
    }
  }

  // ---------- Form state ----------
  let stored = null; // the stored settings document
  const fieldState = new Map(); // id → clean | dirty | saving | error
  const busyLists = new Set(); // selects that are open or focused
  const pendingRebuilds = new Set(); // lists to rebuild once they are no longer busy
  const stateOf = (id) => fieldState.get(id) || "clean";
  function setState(id, state) {
    fieldState.set(id, state);
    const control = $(id);
    const label = control?.closest("label");
    label?.classList.toggle("is-dirty", state === "dirty");
    label?.classList.toggle("is-invalid", state === "error");
  }
  const messageTimers = new Map();
  function fieldMessage(id, text, kind) {
    const node = document.querySelector(`.field-message[data-for="${id}"]`);
    if (!node) return;
    clearTimeout(messageTimers.get(id));
    node.textContent = text;
    node.classList.toggle("error", kind === "error");
    node.classList.toggle("pending", kind === "pending");
    if (kind === "saved") {
      messageTimers.set(id, setTimeout(() => {
        if (node.textContent === text) node.textContent = "";
      }, 2000));
    }
  }
  // The listed text fields (Codex model, effort, voice) are edited through their select.
  const listedSelect = (id) => document.querySelector(`select.listed[data-for="${id}"]`);
  function isBusy(control) {
    if (document.activeElement === control || busyLists.has(control.id)) return true;
    const select = listedSelect(control.id);
    return Boolean(select && (document.activeElement === select || busyLists.has(select.id)));
  }
  function mirrorListed(control) {
    const select = listedSelect(control.id);
    if (!select || select.options.length === 0 || busyLists.has(select.id)) return;
    const text = control.value;
    if (![...select.options].some((option) => option.value === text)) {
      const option = document.createElement("option");
      option.value = text;
      option.textContent = text ? `${text} (not in the current list)` : "(default)";
      select.appendChild(option);
    }
    select.value = text;
  }
  // Writes a field from the stored settings unless it is being edited or holds a value of
  // its own; `force` after the field's own successful save.
  function fillField(control, force) {
    if (!stored) return;
    if (!force && (stateOf(control.id) !== "clean" || isBusy(control))) return;
    writeControl(control, readByPath(stored, control.dataset.path));
    if (control.tagName === "SELECT" && !control.options.length) return;
    mirrorListed(control);
  }
  function fillForm() {
    document.querySelectorAll("[data-path]").forEach((control) => fillField(control, false));
    fillConfig(false);
    applyAppearance(stored?.ui);
    $("config-preview").textContent = stored ? JSON.stringify(stored, null, 2) : "";
  }

  async function saveControl(control) {
    if (!invoke) return;
    setState(control.id, "saving");
    fieldMessage(control.id, "Saving…", "pending");
    try {
      stored = await invoke("save_settings", { patch: patchFor(control.dataset.path, controlValue(control)) });
      setState(control.id, "clean");
      fillField(control, true);
      fieldMessage(control.id, "Saved", "saved");
      if (control.id === "codex-model") applyEffortList();
    } catch (error) {
      // The rejected value stays so it can be corrected; the stored value is unchanged.
      setState(control.id, "error");
      fieldMessage(control.id, String(error), "error");
    }
  }

  // Every Settings field saves on its own change: text when leaving the field or on Enter,
  // sliders on release, lists and switches at once.
  document.querySelectorAll("[data-path]").forEach((control) => {
    control.addEventListener("input", () => {
      if (control.dataset.kind === "percent") percentLabel(control);
      if (stateOf(control.id) === "saving") return;
      const differs = !stored || !sameValue(controlValue(control), readByPath(stored, control.dataset.path));
      if (stateOf(control.id) !== "error") setState(control.id, differs ? "dirty" : "clean");
    });
    control.addEventListener("change", () => {
      if (control.dataset.path.startsWith("ui.")) {
        applyAppearance({ theme: $("setting-theme").value, font_size: Number($("setting-font-size").value) });
      }
      saveControl(control);
    });
    if (control.tagName === "SELECT") trackOpenList(control);
  });

  // A list counts as open from the first press or focus until a choice or blur, and is
  // never rebuilt meanwhile.
  function trackOpenList(select) {
    const done = () => {
      busyLists.delete(select.id);
      if (pendingRebuilds.size) runPendingRebuilds();
    };
    select.addEventListener("pointerdown", () => busyLists.add(select.id));
    select.addEventListener("focus", () => busyLists.add(select.id));
    select.addEventListener("change", done);
    select.addEventListener("blur", done);
  }

  // ---------- Lists ----------
  function fillListed(id, options) {
    const input = $(id);
    const select = listedSelect(id);
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
    mirrorListed(input);
  }
  document.querySelectorAll("select.listed").forEach((select) => {
    trackOpenList(select);
    select.addEventListener("change", () => {
      const input = $(select.dataset.for);
      input.value = select.value;
      saveControl(input);
    });
  });

  let wakeModels = [];
  let capabilities = { models: [], voices: [] };
  let capabilitiesKey = "";
  function fillSelect(select, items, current) {
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
  }
  // The wake lists come from the catalog, which is fixed while the app runs: built once.
  function buildWakeLists() {
    if (busyLists.has("wake-engine") || busyLists.has("wake-model")) {
      pendingRebuilds.add("wake");
      return;
    }
    const engines = [...new Set(wakeModels.map((model) => model.engine))];
    fillSelect($("wake-engine"), engines.map((engine) => ({ value: engine, label: engine })), readByPath(stored, "wake.engine"));
    fillSelect(
      $("wake-model"),
      wakeModels.map((model) => ({ value: model.id, label: `${model.display_name} [${model.language}, ${model.license}]` })),
      readByPath(stored, "wake.model"),
    );
  }
  function applyEffortList() {
    if (busyLists.has("codex-effort-select")) {
      pendingRebuilds.add("codex");
      return;
    }
    const modelId = $("codex-model").value;
    const model = capabilities.models.find((entry) => entry.id === modelId)
      || capabilities.models.find((entry) => entry.is_default);
    const efforts = model ? model.reasoning_efforts : [];
    const options = [{ value: "", label: model?.default_reasoning_effort ? `(default: ${model.default_reasoning_effort})` : "(default)" }]
      .concat(efforts.map((effort) => ({ value: effort, label: effort })));
    fillListed("codex-effort", efforts.length ? options : []);
  }
  function buildCodexLists() {
    if (["codex-model-select", "codex-effort-select", "codex-voice-select"].some((id) => busyLists.has(id))) {
      pendingRebuilds.add("codex");
      return;
    }
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
  function runPendingRebuilds() {
    const pending = [...pendingRebuilds];
    pendingRebuilds.clear();
    if (pending.includes("wake")) buildWakeLists();
    if (pending.includes("codex")) buildCodexLists();
  }
  // Rebuilds the Codex lists only when what the engine offers has changed.
  async function loadCapabilities() {
    if (!invoke) return;
    try {
      const next = await invoke("capabilities");
      const key = JSON.stringify({ models: next.models, voices: next.voices, default_voice: next.default_voice });
      if (key === capabilitiesKey) return;
      capabilitiesKey = key;
      capabilities = next;
      buildCodexLists();
    } catch (error) {
      appendLog(`capabilities failed: ${error}`);
    }
  }
  async function loadSettings() {
    if (!invoke) return;
    try {
      stored = (await invoke("get_settings")).settings;
      fillForm();
    } catch (error) {
      appendLog(`settings failed: ${error}`);
    }
  }

  // ---------- Config tab: edited fields wait for Save changes ----------
  function configDiffers(control) {
    return stored && control.value.trim() !== String(readByPath(stored, control.dataset.configPath) ?? "");
  }
  function updateConfigActions() {
    const dirty = [...document.querySelectorAll("[data-config-path]")].some((control) => stateOf(control.id) === "dirty");
    const token = $("config-token").value.trim().length > 0;
    $("config-save").disabled = !dirty && !token;
    $("config-reset").disabled = !dirty && !token;
    $("config-token").closest("label").classList.toggle("is-dirty", token);
    const feedback = $("config-feedback");
    if (dirty || token) {
      feedback.textContent = "Unsaved changes.";
      feedback.className = "form-feedback warning-text";
    } else if (feedback.textContent === "Unsaved changes.") {
      feedback.textContent = "";
    }
  }
  document.querySelectorAll("[data-config-path]").forEach((control) => {
    control.addEventListener("input", () => {
      setState(control.id, configDiffers(control) ? "dirty" : "clean");
      updateConfigActions();
    });
  });
  $("config-token").addEventListener("input", updateConfigActions);
  function fillConfig(force) {
    document.querySelectorAll("[data-config-path]").forEach((control) => {
      if (!stored) return;
      if (!force && (stateOf(control.id) !== "clean" || document.activeElement === control)) return;
      const value = readByPath(stored, control.dataset.configPath);
      control.value = value == null ? "" : value;
      setState(control.id, "clean");
    });
    updateConfigActions();
  }
  $("config-reset").addEventListener("click", () => {
    $("config-token").value = "";
    fillConfig(true);
    $("config-feedback").textContent = "Reset to the stored values.";
    $("config-feedback").className = "form-feedback";
  });
  $("config-save").addEventListener("click", async () => {
    const feedback = $("config-feedback");
    const patch = {};
    for (const control of document.querySelectorAll("[data-config-path]")) {
      if (stateOf(control.id) === "dirty") mergePatch(patch, patchFor(control.dataset.configPath, control.value.trim()));
    }
    const token = $("config-token").value.trim();
    feedback.textContent = "Saving…";
    feedback.className = "form-feedback";
    try {
      if (Object.keys(patch).length) stored = await invoke("save_settings", { patch });
      if (token) {
        await invoke("set_secret", { name: "discord_token", value: token });
        $("config-token").value = "";
      }
      fillConfig(true);
      feedback.textContent = "Saved.";
      feedback.className = "form-feedback";
      $("config-preview").textContent = JSON.stringify(stored, null, 2);
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

  // ---------- Codex chat (ported from the first version's chat view) ----------
  const chatLog = $("chat-log");
  const CHAT_LIMIT = 600;
  const CHAT_KIND_LABELS = {
    assistant: "GPT",
    context: "Context → GPT",
    task: "Task → Codex",
    agent: "Codex",
    reasoning: "Codex thinking",
    tool: "Codex tool",
    system: "Call",
    error: "Error",
  };
  let chatEntries = [];
  function chatVisible(entry) {
    if (entry.kind === "context") return $("chat-filter-context").checked;
    if (["task", "agent", "reasoning", "tool"].includes(entry.kind)) return $("chat-filter-agent").checked;
    if (entry.kind === "system") return $("chat-filter-system").checked;
    return true;
  }
  function chatWho(entry) {
    if (entry.kind === "user") return entry.who || "Voice channel";
    return CHAT_KIND_LABELS[entry.kind] || entry.kind;
  }
  function chatElement(entry) {
    const row = document.createElement("div");
    row.className = `chat-entry kind-${entry.kind}`;
    const time = document.createElement("span");
    time.className = "chat-time";
    time.textContent = entry.at;
    const who = document.createElement("span");
    who.className = "chat-who";
    who.textContent = chatWho(entry);
    who.title = who.textContent;
    const body = document.createElement("div");
    body.className = "chat-text";
    body.textContent = entry.text;
    row.append(time, who, body);
    return row;
  }
  function scrollChatToEnd(force) {
    if (force || $("chat-autoscroll").checked) chatLog.scrollTop = chatLog.scrollHeight;
  }
  function renderChat() {
    const visible = chatEntries.filter(chatVisible);
    if (!visible.length) {
      const empty = document.createElement("div");
      empty.className = "empty-state";
      empty.textContent = "Nothing yet. Everything heard and said in voice calls appears here, together with what Codex does for handed-off tasks.";
      chatLog.replaceChildren(empty);
      return;
    }
    chatLog.replaceChildren(...visible.map(chatElement));
    scrollChatToEnd(true);
  }
  function addChatEntry(entry) {
    if (!entry || chatEntries.some((existing) => existing.id === entry.id)) return;
    chatEntries.push(entry);
    if (chatEntries.length > CHAT_LIMIT) chatEntries.splice(0, chatEntries.length - CHAT_LIMIT);
    if (!chatVisible(entry)) return;
    chatLog.querySelector(".empty-state")?.remove();
    chatLog.append(chatElement(entry));
    while (chatLog.childElementCount > CHAT_LIMIT) chatLog.firstElementChild?.remove();
    scrollChatToEnd(false);
  }
  function chatAsText() {
    return chatEntries
      .filter(chatVisible)
      .map((entry) => `[${entry.at}] ${chatWho(entry)}: ${entry.text}`)
      .join("\n");
  }
  ["chat-filter-context", "chat-filter-agent", "chat-filter-system"].forEach((id) => {
    $(id).addEventListener("change", renderChat);
  });
  $("chat-copy").addEventListener("click", () => copyText(chatAsText(), "chat"));
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

  // ---------- Status (never touches the form) ----------
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

    // Readings next to the form, not form fields.
    $("input-gain-level").textContent = levelText(status.audio?.input);
    $("output-gain-level").textContent = levelText(status.audio?.output);
    const unknown = (status.wake || {}).unknown_words || [];
    $("unknown-words").textContent = unknown.length
      ? `Not in the model vocabulary (these phrases are ignored): ${unknown.join(", ")}`
      : "";
    const badge = $("token-badge");
    badge.textContent = tokenSet ? "stored" : "not stored";
    badge.className = `badge ${tokenSet ? "ok" : "warn"}`;
    const settingsPath = `${status.data_dir}\\settings.json`;
    setText("config-path", settingsPath);
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
  function renderUptime() {
    if (!systemFacts) return;
    const uptime = systemFacts.uptime_seconds + (Date.now() - systemFetchedAt) / 1000;
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
    await loadCapabilities();
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
        // Lines that arrived while the history was on its way are kept after it.
        const kept = new Set(history);
        consoleLines = history.concat(consoleLines.filter((line) => !kept.has(line)));
        renderConsole();
      } catch (error) {
        appendLog(`log history failed: ${error}`);
      }
      // `event` names the event; a chat entry's own `kind` is the entry's kind.
      await listen("app-event", ({ payload }) => {
        switch (payload.event) {
          // Log lines arrive through `app-log` with everything else the log file gets.
          case "log":
            break;
          case "chat":
            addChatEntry(payload);
            break;
          case "settings_changed":
            loadSettings();
            refresh();
            break;
          case "assistant":
            refresh();
            loadCapabilities();
            break;
          default:
            refresh();
        }
      });
    }
    await refreshSystem();
    try {
      wakeModels = await invoke("wake_models");
    } catch (error) {
      appendLog(`wake_models failed: ${error}`);
    }
    await loadSettings();
    buildWakeLists();
    await loadCapabilities();
    fillForm();
    await refresh();
    await loadChat();
    setInterval(refresh, 5000);
    setInterval(renderUptime, 1000);
  }

  start();
})();
