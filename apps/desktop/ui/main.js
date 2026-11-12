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
  // A path is "a.b.c", or an array of keys when a key holds dots (a wake model id).
  const keysOf = (path) => (Array.isArray(path) ? path : path.split("."));
  function readByPath(object, path) {
    return keysOf(path).reduce((o, key) => (o == null ? undefined : o[key]), object);
  }
  // The phrase fields belong to the selected wake model: `wake.phrases.<model>.<list>`.
  function pathOf(control) {
    if (control.dataset.phrases) return ["wake", "phrases", stored?.wake?.model ?? "", control.dataset.phrases];
    return control.dataset.path;
  }
  function patchFor(path, value) {
    const keys = keysOf(path);
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
  // The value line between a field's name and its control: the percentage of a slider,
  // "on" or "off" for a switch, so a switch column has the same rows as a slider column.
  function percentLabel(control) {
    const label = $(`${control.id}-label`);
    if (!label) return;
    label.textContent = control.dataset.kind === "bool" ? (control.checked ? "on" : "off") : `${control.value} %`;
  }
  function writeControl(control, value) {
    if (control.dataset.kind === "percent") {
      control.value = Math.round(Number(value ?? 0) * 100);
      percentLabel(control);
    } else if (control.dataset.kind === "bool") {
      control.checked = Boolean(value);
      percentLabel(control);
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
    label?.classList.toggle("is-saving", state === "saving");
  }
  // The line under a field holds its validation error and nothing else, so a save never
  // changes the layout: the saved mark is the field's name in the success colour.
  function fieldMessage(id, text) {
    const node = document.querySelector(`.field-message[data-for="${id}"]`);
    if (node) node.textContent = text;
  }
  // The field's name as the user reads it: the label's own text, or its span for a
  // toolbar switch.
  function fieldName(control) {
    const label = control.closest("label");
    if (!label) return control.id;
    const text = [...label.childNodes].find((node) => node.nodeType === Node.TEXT_NODE && node.textContent.trim());
    return (text?.textContent || label.querySelector("span")?.textContent || control.id).trim();
  }
  const SAVED_MARK_MS = 2000;
  const savedTimers = new Map();
  let feedbackTimer = 0;
  function markSaved(control) {
    const label = control.closest("label");
    if (label) {
      label.classList.add("is-saved");
      clearTimeout(savedTimers.get(control.id));
      savedTimers.set(control.id, setTimeout(() => label.classList.remove("is-saved"), SAVED_MARK_MS));
    }
    const feedback = $("settings-feedback");
    feedback.textContent = `Saved: ${fieldName(control)}`;
    clearTimeout(feedbackTimer);
    feedbackTimer = setTimeout(() => {
      feedback.textContent = "Changes save automatically.";
    }, SAVED_MARK_MS);
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
    writeControl(control, readByPath(stored, pathOf(control)));
    if (control.tagName === "SELECT" && !control.options.length) return;
    mirrorListed(control);
  }
  function fillForm() {
    document.querySelectorAll("[data-path]").forEach((control) => fillField(control, false));
    showPhraseModel();
    fillConfig(false);
    applyAppearance(stored?.ui);
    handoffHint();
    applyDiagnostics();
    $("config-preview").textContent = stored ? JSON.stringify(stored, null, 2) : "";
  }

  // What each answer handoff mode does, and how it behaved in the tests of 2026-11-10.
  const HANDOFF_HINTS = {
    items: "The agent's answer is added to the conversation as its own message and the voice retells it. In the tests it kept the agent's facts every time.",
    commentary: "The agent's answer reaches the voice as commentary. In the tests the facts were kept; once the voice gave the agent the same task twice, once it acknowledged in English.",
    bem_tags: "The agent's answer reaches the voice with channel tags (analysis, commentary, final). In the tests the facts were kept, sometimes shortened.",
    thinking: "Codex's own routing, used before this setting existed: the answer is hidden context and the voice speaks in its own words. In the tests it twice made up an answer before the agent had finished.",
  };
  function handoffHint() {
    const hint = HANDOFF_HINTS[$("codex-handoff").value];
    $("codex-handoff-hint").textContent = hint
      ? `${hint} Applies from the next call. Default: conversation items.`
      : "";
  }
  $("codex-handoff").addEventListener("change", handoffHint);

  async function saveControl(control) {
    if (!invoke) return;
    setState(control.id, "saving");
    try {
      stored = await invoke("save_settings", { patch: patchFor(pathOf(control), controlValue(control)) });
      setState(control.id, "clean");
      fieldMessage(control.id, "");
      fillField(control, true);
      // Another wake model: its own phrases in the phrase fields, its name in their labels.
      if (control.id === "wake-model") {
        document.querySelectorAll("[data-phrases]").forEach((phrases) => fillField(phrases, false));
        showPhraseModel();
      }
      // A second control of the same setting (the Performance tab's switch and the
      // Diagnostics card) follows at once, before the settings event arrives.
      document.querySelectorAll(`[data-path="${control.dataset.path}"]`).forEach((other) => {
        if (other !== control) fillField(other, false);
      });
      markSaved(control);
      if (control.id === "codex-model") applyEffortList();
      if (control.dataset.path.startsWith("diagnostics.")) applyDiagnostics();
    } catch (error) {
      // The rejected value stays so it can be corrected; the stored value is unchanged.
      setState(control.id, "error");
      fieldMessage(control.id, String(error));
    }
  }

  // Every Settings field saves on its own change: text when leaving the field or on Enter,
  // sliders on release, lists and switches at once.
  document.querySelectorAll("[data-path]").forEach((control) => {
    control.addEventListener("input", () => {
      if (control.dataset.kind === "percent" || control.dataset.kind === "bool") percentLabel(control);
      if (stateOf(control.id) === "saving") return;
      const differs = !stored || !sameValue(controlValue(control), readByPath(stored, pathOf(control)));
      if (stateOf(control.id) !== "error") setState(control.id, differs ? "dirty" : "clean");
    });
    control.addEventListener("change", () => {
      if (control.dataset.path.startsWith("ui.")) {
        applyAppearance({ theme: $("setting-theme").value, font_size: Number($("setting-font-size").value) });
      }
      // A wake model that is not on this PC asks in the install dialog before it is saved.
      if (control.id === "wake-model") {
        chooseWakeModel(control);
        return;
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
      wakeModels.map((model) => ({ value: model.id, label: wakeModelLabel(model) })),
      readByPath(stored, "wake.model"),
    );
  }
  // Decimal megabytes, rounded, as the app's log writes them.
  const megabytes = (bytes) => Math.round(Number(bytes || 0) / 1e6);
  function wakeModelLabel(model) {
    const state = model.source === "local"
      ? "models folder"
      : model.installed ? "downloaded" : `to download, ${megabytes(model.bytes)} MB`;
    return `${model.language || "?"} — ${model.display_name} · ${state}`;
  }
  // The selected model's name in the phrase fields' labels.
  function showPhraseModel() {
    const id = stored?.wake?.model ?? "";
    const model = wakeModels.find((entry) => entry.id === id);
    const name = model ? model.display_name : id;
    document.querySelectorAll("[data-model-name]").forEach((node) => {
      node.textContent = name ? ` — ${name}` : "";
    });
  }
  // The download state of the wake models changes when a download ends: list them again.
  async function reloadWakeModels() {
    try {
      wakeModels = await invoke("wake_models");
      buildWakeLists();
      showPhraseModel();
    } catch (error) {
      appendLog(`wake_models failed: ${error}`);
    }
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

  // ---------- Codex chat ----------
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

  // ---------- Downloading the wake word's files by hand ----------
  // The same part in the notice and in the install dialog: each missing file's archive
  // name with a link to the official page it is published on, the assets folder button,
  // and what Download then does with the files. A link has target="_blank": the app hands
  // every new-window request to the default browser and never shows a page itself.
  function renderManual(container, items) {
    const list = container.querySelector(".manual-items");
    list.innerHTML = "";
    for (const item of items || []) {
      if (!item.file || !item.page) continue;
      const line = document.createElement("li");
      const name = document.createElement("code");
      name.textContent = item.file;
      const link = document.createElement("a");
      link.href = item.page;
      link.target = "_blank";
      link.rel = "noopener";
      link.textContent = item.page.replace(/^https?:\/\//, "");
      const source = document.createElement("span");
      source.className = "manual-source";
      source.append("from ", link);
      line.append(name, source);
      list.appendChild(line);
    }
    container.dataset.empty = list.children.length ? "" : "1";
  }
  function showManual(container, visible) {
    container.hidden = !visible || container.dataset.empty === "1";
  }
  // After a failed download only what is still missing is listed.
  async function refreshManualItems(model) {
    if (!invoke || !model) return;
    try {
      const items = await invoke("wake_requirements", { model });
      renderManual($("wake-download-manual"), items);
      renderManual($("wake-install-manual"), items);
      showManual($("wake-download-manual"), !$("wake-download").hidden && $("wake-download").dataset.failed === "1");
      showManual($("wake-install-manual"), installDialog.open && !$("wake-install-error").hidden);
    } catch (error) {
      appendLog(`wake_requirements failed: ${error}`);
    }
  }
  async function openAssetsFolder(report) {
    if (!invoke) return;
    try {
      report(`Opened ${await invoke("open_assets_folder")}`);
    } catch (error) {
      report(`Could not open the assets folder: ${error}`);
    }
  }
  $("wake-download-assets").addEventListener("click", () => openAssetsFolder((line) => {
    $("settings-feedback").textContent = line;
  }));
  $("wake-install-assets").addEventListener("click", () => openAssetsFolder(appendLog));

  // ---------- The wake model download: nothing downloads without the button ----------
  let lastWakeKind = "";
  let lastWakeModel = "";
  let downloadRequestError = "";
  const fetching = (kind) => kind === "downloading" || kind === "installing";
  function renderWakeDownload(wake) {
    const state = wake.state || {};
    lastWakeModel = wake.model || lastWakeModel;
    const box = $("wake-download");
    const text = $("wake-download-text");
    const list = $("wake-download-items");
    const button = $("wake-download-button");
    const manual = $("wake-download-manual");
    const afterDownload = fetching(lastWakeKind) || lastWakeKind === "needs_download" || box.dataset.failed === "1";
    list.innerHTML = "";
    if (state.kind === "needs_download") {
      box.hidden = false;
      box.dataset.failed = "";
      text.textContent = downloadRequestError
        || "The wake word listens with this model, which is not on this PC yet. VoiceBro needs these files; nothing is downloaded until you press Download.";
      for (const item of state.items || []) {
        const line = document.createElement("li");
        line.textContent = `${item.label} — ${megabytes(item.bytes)} MB from ${item.host}`;
        list.appendChild(line);
      }
      renderManual(manual, state.items);
      showManual(manual, true);
      button.hidden = false;
      button.disabled = false;
    } else if (fetching(state.kind)) {
      box.hidden = false;
      box.dataset.failed = "";
      text.textContent = state.kind === "downloading"
        ? `Downloading the ${state.label}: ${state.percent} %`
        : `Checking and unpacking the ${state.label}…`;
      showManual(manual, false);
      button.hidden = true;
    } else if (state.kind === "error" && afterDownload) {
      box.hidden = false;
      const firstError = box.dataset.failed !== "1";
      box.dataset.failed = "1";
      text.textContent = `The download failed: ${state.message}`;
      showManual(manual, true);
      if (firstError) refreshManualItems(wake.model);
      button.hidden = false;
      button.disabled = false;
    } else {
      box.hidden = true;
      box.dataset.failed = "";
    }
    // A finished download, with or without phrases to load: the list marks it downloaded.
    if ((state.kind === "ready" && lastWakeKind && lastWakeKind !== "ready")
      || (fetching(lastWakeKind) && !fetching(state.kind))) reloadWakeModels();
    if (state.kind !== "needs_download") downloadRequestError = "";
    lastWakeKind = state.kind || "";
  }
  $("wake-download-button").addEventListener("click", async () => {
    if (!invoke) return;
    $("wake-download-button").disabled = true;
    try {
      await invoke("wake_download", { model: lastWakeModel });
      downloadRequestError = "";
    } catch (error) {
      downloadRequestError = `Could not start the download: ${error}`;
      $("wake-download-text").textContent = downloadRequestError;
      $("wake-download-button").disabled = false;
    }
    refresh();
  });
  $("open-models-folder").addEventListener("click", async () => {
    if (!invoke) return;
    try {
      const path = await invoke("open_models_folder");
      $("settings-feedback").textContent = `Opened ${path}`;
    } catch (error) {
      $("settings-feedback").textContent = `Could not open the models folder: ${error}`;
    }
  });

  // ---------- The install dialog: the wake word's files are not on this PC ----------
  // One modal dialog. It never opens because the window started: it opens when the bot
  // joins a voice channel while files are missing (`join`; a bot already in a channel at
  // the window's first status counts, as after a reload), when the wake state turns to
  // needs_download later, as after a switch to wake word (`mode`), and before a model
  // choice whose files are missing is saved (`choice`). Download asks for that model's
  // files and shows the progress here; Not now, Hide, Close and Escape store nothing, and
  // the Assistant card's notice stays.
  const installDialog = $("wake-install");
  const install = {
    armed: false, // set by `start()` once the model list and the settings are loaded
    started: false, // the first status after arming was seen
    lastKind: "",
    voiceCount: null,
    reason: "",
    model: "",
    previous: "", // the stored model a `choice` goes back to on Cancel
    saved: true, // false while a `choice` is not saved yet
    downloading: false, // Download was pressed for `model`
  };
  const installNames = (id) => wakeModels.find((entry) => entry.id === id)?.display_name || id;
  function installSetItems(items) {
    const list = $("wake-install-items");
    list.innerHTML = "";
    for (const item of items) {
      const line = document.createElement("li");
      line.textContent = `${item.label} — ${megabytes(item.bytes)} MB from ${item.host}`;
      list.appendChild(line);
    }
    const total = items.reduce((sum, item) => sum + Number(item.bytes || 0), 0);
    $("wake-install-total").textContent = items.length > 1 ? `${megabytes(total)} MB in total.` : "";
  }
  function installButtons(primary, secondary) {
    const button = $("wake-install-primary");
    button.hidden = !primary;
    button.disabled = false;
    if (primary) button.textContent = primary;
    $("wake-install-secondary").textContent = secondary;
  }
  function installOpen(reason, model, items) {
    if (installDialog.open) return;
    Object.assign(install, { reason, model, saved: reason !== "choice", downloading: false });
    installDialog.dataset.reason = reason;
    const onlyLibrary = items.length === 1 && items[0].label === "Vosk library";
    $("wake-install-text").textContent = onlyLibrary
      ? `The wake word needs the Vosk recognizer to use ${installNames(model)}, and it is not on this PC yet. Download it now?`
      : `VoiceBro listens for your wake phrase with the ${installNames(model)} model, which is not on this PC yet. Download it now?`;
    installSetItems(items);
    renderManual($("wake-install-manual"), items);
    showManual($("wake-install-manual"), true);
    $("wake-install-progress").hidden = true;
    $("wake-install-error").hidden = true;
    $("wake-install-note").textContent = reason === "choice"
      ? `Nothing is downloaded until you press Download. Cancel keeps ${installNames(install.previous)}.`
      : "Nothing is downloaded until you press Download. Not now keeps the notice under Wake model in Settings → Assistant.";
    installButtons("Download", reason === "choice" ? "Cancel" : "Not now");
    installDialog.showModal();
    // The window may come to the front while the user types elsewhere: a stray Enter or
    // Space must not agree to a download, so the focus starts on Not now (Cancel).
    $("wake-install-secondary").focus();
  }
  // Closes the dialog; a `choice` that was never saved goes back to the stored model.
  function installClose() {
    if (!install.saved) {
      const select = $("wake-model");
      select.value = install.previous;
      setState(select.id, "clean");
      install.saved = true;
    }
    busyLists.delete("wake-model");
    if (pendingRebuilds.size) runPendingRebuilds();
    if (installDialog.open) installDialog.close();
  }
  // The dialog follows the wake state of the model it is about; the triggers open it.
  function renderInstall(status) {
    const wake = status.wake || {};
    const state = wake.state || {};
    const kind = state.kind || "";
    const voiceCount = Object.keys(status.discord?.voice_channels || {}).length;
    const wakeWord = (stored?.session?.mode ?? status.settings?.session?.mode) === "wake_word";
    if (installDialog.open && wake.model === install.model) {
      if (fetching(kind)) {
        install.downloading = true;
        $("wake-install-progress").hidden = false;
        $("wake-install-progress").textContent = kind === "downloading"
          ? `Downloading the ${state.label}: ${state.percent} %`
          : `Checking and unpacking the ${state.label}…`;
        $("wake-install-error").hidden = true;
        showManual($("wake-install-manual"), false);
        installButtons(null, "Hide");
      } else if (kind === "error" && install.downloading) {
        $("wake-install-progress").hidden = true;
        $("wake-install-error").hidden = false;
        $("wake-install-error").textContent = `The download failed: ${state.message}`;
        showManual($("wake-install-manual"), true);
        installButtons("Try again", "Close");
      } else if (install.downloading && ["ready", "no_phrases", "loading"].includes(kind)) {
        installClose();
      }
    }
    if (install.armed && !installDialog.open && kind === "needs_download" && wakeWord) {
      const items = state.items || [];
      // The window's start never opens it; a bot already in a voice channel does.
      if (!install.started) {
        if (voiceCount > 0) installOpen("join", wake.model, items);
      } else if (install.lastKind && install.lastKind !== "needs_download") installOpen("mode", wake.model, items);
      else if (install.voiceCount !== null && voiceCount > install.voiceCount) installOpen("join", wake.model, items);
    }
    if (install.armed) install.started = true;
    install.lastKind = kind;
    install.voiceCount = voiceCount;
  }
  // The wake model list: a model whose files are all on disk saves at once; otherwise the
  // dialog asks first and nothing is saved until Download.
  async function chooseWakeModel(select) {
    const model = select.value;
    const previous = readByPath(stored, "wake.model") ?? "";
    // The dialog is modal, but a script or a keyboard path can still reach the list: while
    // it is open, nothing about the wake model changes.
    if (installDialog.open) {
      select.value = previous;
      setState(select.id, "clean");
      return;
    }
    let items = [];
    try {
      items = await invoke("wake_requirements", { model });
    } catch (error) {
      appendLog(`wake_requirements failed: ${error}`);
    }
    if (!items.length) {
      saveControl(select);
      return;
    }
    busyLists.add(select.id);
    install.previous = previous;
    installOpen("choice", model, items);
  }
  $("wake-install-primary").addEventListener("click", async () => {
    const button = $("wake-install-primary");
    // Hidden while a download runs, disabled while one starts: a click that still arrives
    // (from a script) must not ask for a download of a model that is no longer the choice.
    if (!invoke || button.hidden || button.disabled || !installDialog.open) return;
    button.disabled = true;
    if (!install.saved) {
      const select = $("wake-model");
      await saveControl(select);
      if (stored?.wake?.model !== install.model) {
        // The save was refused; its message is under the field.
        install.saved = true;
        installClose();
        return;
      }
      install.saved = true;
      busyLists.delete(select.id);
    }
    $("wake-install-error").hidden = true;
    $("wake-install-progress").hidden = false;
    // Not "the download": archives put into the assets folder are only checked and unpacked.
    $("wake-install-progress").textContent = "Starting…";
    showManual($("wake-install-manual"), false);
    try {
      await invoke("wake_download", { model: install.model });
      install.downloading = true;
      installButtons(null, "Hide");
    } catch (error) {
      $("wake-install-progress").hidden = true;
      $("wake-install-error").hidden = false;
      $("wake-install-error").textContent = `Could not start the download: ${error}`;
      showManual($("wake-install-manual"), true);
      installButtons("Try again", "Close");
    }
    refresh();
  });
  $("wake-install-secondary").addEventListener("click", installClose);
  // Escape is Not now (or Cancel), never a half-closed dialog.
  installDialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    installClose();
  });

  // ---------- Status (never touches the form) ----------
  function wakeText(wake) {
    const state = wake.state || {};
    let text;
    switch (state.kind) {
      case "off": text = "off (the session mode is not wake word)"; break;
      case "no_phrases": text = "no phrases configured"; break;
      case "needs_download":
        text = `waiting for your OK to download ${(state.items || []).map((item) => `the ${item.label} (${megabytes(item.bytes)} MB)`).join(" and ")}: Settings → Assistant`;
        break;
      case "downloading": text = `downloading the ${state.label}: ${state.percent}%`; break;
      case "installing": text = `checking and unpacking the ${state.label}`; break;
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
    renderCodexSetup(assistant);
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
      if (channel.delivery) counters[`delivery ${guild}`] = channel.delivery;
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
    renderWakeDownload(status.wake || {});
    renderInstall(status);
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
      const status = await invoke("status");
      renderStatus(status);
      samplePerformance(status);
    } catch (error) {
      appendLog(`status failed: ${error}`);
    }
  }

  // ---------- Diagnostics: what runs is decided by the two switches ----------
  // Performance monitoring on: the status is read once a second and sampled for the
  // Performance tab. Off: the status is read every 5 s and on events, nothing is kept, and
  // the tab shows its switch and a note. Memory measurement on: `memory_status` every 3 s.
  // Off: no reading, no status bar item, the System card says so.
  const STATUS_POLL_MS = 5000;
  const STATUS_SAMPLE_MS = 1000;
  const MEMORY_POLL_MS = 3000;
  let statusTimer = 0;
  let statusTimerMs = 0;
  let memoryTimer = 0;
  const monitoringOn = () => Boolean(stored?.diagnostics?.performance);
  const memoryOn = () => Boolean(stored?.diagnostics?.memory);
  function applyDiagnostics() {
    const wantedMs = monitoringOn() ? STATUS_SAMPLE_MS : STATUS_POLL_MS;
    if (wantedMs !== statusTimerMs) {
      clearInterval(statusTimer);
      statusTimer = setInterval(refresh, wantedMs);
      statusTimerMs = wantedMs;
    }
    if (!monitoringOn()) samples = [];
    $("performance-live").classList.toggle("hidden", !monitoringOn());
    $("performance-content").classList.toggle("hidden", !monitoringOn());
    $("performance-off").classList.toggle("hidden", monitoringOn());
    if (monitoringOn()) renderPerformance();

    if (memoryOn() && !memoryTimer) {
      memoryTimer = setInterval(refreshMemory, MEMORY_POLL_MS);
      refreshMemory();
    } else if (!memoryOn() && memoryTimer) {
      clearInterval(memoryTimer);
      memoryTimer = 0;
    }
    $("footer-memory-item").classList.toggle("hidden", !memoryOn());
    $("memory-on").classList.toggle("hidden", !memoryOn());
    $("memory-off").classList.toggle("hidden", memoryOn());
    if (!memoryOn()) setText("system-memory", "not measured (Settings → Diagnostics)");
  }

  // ---------- Performance: samples of the status, once a second, five minutes kept ----------
  const SAMPLE_KEEP_MS = 5 * 60 * 1000;
  let samples = [];
  let lastSampleAt = 0;
  // The connected call's counters, or null when no call is connected.
  function callAudio(status) {
    const call = (status.assistant?.calls || []).find((entry) => entry.connected && entry.audio);
    return call ? call.audio : null;
  }
  function allTurns(status) {
    return Object.values(status.latency || {}).flat();
  }
  function samplePerformance(status) {
    if (!monitoringOn()) return;
    const now = Date.now();
    if (now - lastSampleAt < 900) return;
    lastSampleAt = now;
    samples.push({ t: now, status });
    samples = samples.filter((sample) => now - sample.t <= SAMPLE_KEEP_MS);
    if (document.querySelector('.tab-panel[data-panel="performance"]').classList.contains("active")) {
      renderPerformance();
    }
  }
  function windowSamples() {
    const span = Number($("performance-window").value) * 1000;
    const now = Date.now();
    return samples.filter((sample) => now - sample.t <= span);
  }
  // A counter's growth over the window; a call that restarted begins again at zero.
  function delta(first, last, field) {
    const a = Number(first?.[field] || 0);
    const b = Number(last?.[field] || 0);
    return b >= a ? b - a : b;
  }
  function quality(inWindow) {
    const last = inWindow[inWindow.length - 1];
    const lastAudio = last ? callAudio(last.status) : null;
    if (!lastAudio) return { kind: "idle", label: "Quality: idle" };
    const first = inWindow.find((sample) => callAudio(sample.status)) || last;
    const firstAudio = callAudio(first.status);
    const received = delta(firstAudio, lastAudio, "packets_received");
    const late = delta(firstAudio, lastAudio, "late");
    const concealed = delta(firstAudio, lastAudio, "concealed");
    const drops = delta(firstAudio, lastAudio, "input_drops");
    if (drops > 0) return { kind: "degraded", label: "Quality: degraded (dropped input frames)" };
    if (received > 0 && (late + concealed) / received >= 0.01) {
      return {
        kind: "degraded",
        label: `Quality: degraded (${concealed >= late ? "concealed packets" : "late packets"})`,
      };
    }
    return { kind: "stable", label: "Quality: stable" };
  }
  // A line chart on a canvas: the log background, four guide lines and one 2 px line per
  // series, scaled to the largest value, which the caption under the chart names.
  function drawChart(id, series, unit) {
    const canvas = $(id);
    if (!canvas) return;
    const css = getComputedStyle(document.documentElement);
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth || 560;
    const h = 190;
    const pad = 6;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    const ctx = canvas.getContext("2d");
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = css.getPropertyValue("--log-bg").trim();
    ctx.fillRect(0, 0, w, h);
    ctx.strokeStyle = css.getPropertyValue("--line").trim();
    ctx.lineWidth = 1;
    for (let step = 1; step <= 4; step++) {
      const y = Math.round((h * step) / 5) + 0.5;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
    }
    const finite = series.flatMap((line) => line.values.map(Number).filter(Number.isFinite));
    const top = Math.max(1, ...finite);
    const yOf = (value) => h - pad - (Number(value) / top) * (h - 2 * pad);
    ctx.lineWidth = 2;
    ctx.lineJoin = "round";
    for (const line of series) {
      const values = line.values.length ? line.values : [0];
      const xOf = (i) => (values.length > 1 ? (i * w) / (values.length - 1) : w / 2);
      ctx.strokeStyle = line.color;
      ctx.beginPath();
      values.forEach((value, i) => (i ? ctx.lineTo(xOf(i), yOf(value)) : ctx.moveTo(xOf(i), yOf(value))));
      ctx.stroke();
    }
    $(`${id}-scale`).textContent = `Top of the chart: ${Math.round(top)} ${unit}.`;
  }
  const number = (value) => (Number.isFinite(Number(value)) ? Math.round(Number(value)) : 0);
  // One decimal, or a dash when nothing was measured.
  const decimal = (value) => (value === null || value === undefined || !Number.isFinite(Number(value)) ? "—" : Number(value).toFixed(1));
  function statCards(status) {
    const discord = status.discord?.counters || {};
    // The leg toward Discord of the channel the bot is in (the first, if several).
    const delivery = Object.values(status.discord?.voice_channels || {}).map((channel) => channel.delivery).find(Boolean) || {};
    const lossSource = delivery.expected_loss_source === "measured" ? "measured" : "default";
    const audio = callAudio(status) || (status.assistant?.calls || []).map((call) => call.audio).find(Boolean) || {};
    const gates = Object.values(status.gates || {});
    const turns = allTurns(status);
    const lastTurn = turns[turns.length - 1] || {};
    const levels = status.audio || {};
    return [
      ["Discord ticks", number(discord.ticks)],
      ["Speaker frames", number(discord.speaker_frames)],
      ["Malformed frames", number(discord.malformed_frames)],
      ["Frames sent to the call", number(audio.frames_sent)],
      ["Input drops", number(audio.input_drops)],
      ["Input underruns", number(audio.input_underruns)],
      ["Packets received", number(audio.packets_received)],
      ["Late packets", number(audio.late)],
      ["Concealed packets", number(audio.concealed)],
      ["Recovered by FEC", number(audio.concealed_fec)],
      ["Playout underruns", number(audio.playout_underruns)],
      ["Output drops", number(audio.output_drops)],
      ["Decode errors", number(audio.decode_errors)],
      ["Pauses stretched, ms", number(audio.stretched_pause_ms)],
      ["Speech stretched, ms", number(audio.stretched_speech_ms)],
      ["Removed, ms", number(audio.removed_ms)],
      ["Bridged frames", number(audio.bridged_frames)],
      ["Speech gaps", number(audio.speech_gaps)],
      ["Voice buffer, ms", number(audio.depth_ms)],
      ["Voice buffer max, ms", number(audio.depth_max_ms)],
      // The host's first label fits a card; the full host is in the Status tab's counters.
      ["Discord voice server", (delivery.voice_server || "—").split(".")[0]],
      ["Discord region", delivery.region || "—"],
      ["Packets to Discord", number(delivery.packets_encoded)],
      ["Passed through with FEC", number(delivery.passed_through)],
      ["Encode errors", number(delivery.encode_errors)],
      ["Expected loss, %", delivery.expected_loss_source ? `${number(delivery.expected_loss_percent)} (${lossSource})` : "—"],
      ["Reports from Discord", number(delivery.reports)],
      ["Reported loss, %", decimal(delivery.last_reported_loss_percent)],
      ["Reported jitter, ms", decimal(delivery.last_reported_jitter_ms)],
      ["Frames the assistant missed", number(gates.reduce((sum, gate) => sum + (gate.counters?.input_dropped || 0), 0))],
      ["Wake chunks dropped", number(status.wake?.dropped_chunks)],
      ["Input peak, %", number((levels.input?.peak || 0) * 100)],
      ["Input clips", number(levels.input?.clips)],
      ["Output peak, %", number((levels.output?.peak || 0) * 100)],
      ["Output clips", number(levels.output?.clips)],
      ["Answers measured", turns.length],
      ["Last: first speech → answer, ms", number(lastTurn.first_speech_to_answer_ms)],
      ["Last: last speech → answer, ms", number(lastTurn.last_speech_to_answer_ms)],
      ["Last: answer → playback, ms", number(lastTurn.answer_to_playback_ms)],
      ["Last: first speech → playback, ms", number(lastTurn.first_speech_to_playback_ms)],
    ];
  }
  function renderPerformance() {
    if (!monitoringOn()) return;
    const inWindow = windowSamples();
    const verdict = quality(inWindow);
    $("performance-health").className = `health-dot ${verdict.kind}`;
    $("performance-quality").textContent = verdict.label;

    // Audio timing: each answer measured within the window, in order.
    const span = Number($("performance-window").value) * 1000;
    const latest = samples[samples.length - 1];
    const turns = latest ? allTurns(latest.status) : [];
    const seen = new Map();
    for (const sample of samples) {
      for (const turn of allTurns(sample.status)) {
        const key = `${turn.at}|${turn.turn}|${turn.first_speech_to_playback_ms}|${turn.answer_to_playback_ms}`;
        if (!seen.has(key)) seen.set(key, sample.t);
      }
    }
    const recentTurns = turns.filter((turn) => {
      const key = `${turn.at}|${turn.turn}|${turn.first_speech_to_playback_ms}|${turn.answer_to_playback_ms}`;
      return Date.now() - (seen.get(key) || Date.now()) <= span;
    });
    drawChart("latency-chart", [
      { values: recentTurns.map((turn) => Number(turn.first_speech_to_playback_ms ?? turn.answer_to_playback_ms ?? 0)), color: "#4c9be8" },
      { values: recentTurns.map((turn) => Number(turn.last_speech_to_answer_ms ?? 0)), color: "#46c77a" },
    ], "ms");

    // Media packets: per second, each way, from consecutive samples.
    const sent = [];
    const received = [];
    for (let i = 1; i < inWindow.length; i++) {
      const seconds = Math.max(0.5, (inWindow[i].t - inWindow[i - 1].t) / 1000);
      const before = callAudio(inWindow[i - 1].status);
      const after = callAudio(inWindow[i].status);
      sent.push(after ? delta(before, after, "frames_sent") / seconds : 0);
      received.push(after ? delta(before, after, "packets_received") / seconds : 0);
    }
    drawChart("packets-chart", [
      { values: sent, color: "#4c9be8" },
      { values: received, color: "#46c77a" },
    ], "packets per second");

    const grid = $("performance-stats");
    const cards = statCards(latest ? latest.status : {});
    if (grid.childElementCount !== cards.length) {
      grid.replaceChildren(...cards.map(() => {
        const card = document.createElement("div");
        card.className = "stat-card";
        card.append(document.createElement("span"), document.createElement("strong"));
        return card;
      }));
    }
    cards.forEach(([label, value], index) => {
      const card = grid.children[index];
      card.firstChild.textContent = label;
      card.lastChild.textContent = String(value);
    });
  }
  $("performance-window").addEventListener("change", renderPerformance);
  $("performance-refresh").addEventListener("click", async () => {
    lastSampleAt = 0;
    await refresh();
    renderPerformance();
  });
  document.querySelector('.tab-button[data-tab="performance"]').addEventListener("click", renderPerformance);

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

  // ---------- Memory: the private working set, read by the shell every 3 s while on ----------
  async function refreshMemory() {
    if (!invoke || !memoryOn()) return;
    try {
      const memory = await invoke("memory_status");
      // `null` until the shell's first reading after the switch went on.
      if (!memory) return;
      const mb = (value) => `${Number(value).toFixed(1)} MB`;
      $("footer-memory").textContent = mb(memory.total_mb);
      setText("system-memory", `${mb(memory.total_mb)} (${memory.metric})`);
      setText("memory-app", mb(memory.app_mb));
      setText("memory-webview", `${mb(memory.webview_mb)} in ${memory.webview_processes} processes`);
      setText("memory-codex", memory.codex_mb == null ? "not running" : mb(memory.codex_mb));
      setText("memory-total", mb(memory.total_mb));
      const share = (value) => `${memory.total_mb > 0 ? (100 * (value || 0)) / memory.total_mb : 0}%`;
      $("memory-meter-app").style.width = share(memory.app_mb);
      $("memory-meter-webview").style.width = share(memory.webview_mb);
      $("memory-meter-codex").style.width = share(memory.codex_mb);
    } catch (error) {
      appendLog(`memory_status failed: ${error}`);
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
      accountSaysSignedOut = !account.signed_in;
      $("status-feedback").textContent = account.signed_in
        ? `Codex is signed in (${account.kind || "account"}).`
        : "Codex is not signed in; press Sign in to Codex.";
    } catch (error) {
      $("status-feedback").textContent = `Codex check failed: ${error}`;
    }
    await loadCapabilities();
    await refresh();
  });

  // ---------- Codex setup: the install offer while Codex is missing, then the sign-in ----------
  let codexInstall = null; // codex_install_info: winget, package, sizes, state
  let justInstalled = false; // winget ended with 0 in this window: sign-in is next
  let accountSaysSignedOut = false; // the last Check read no account
  async function loadCodexInstall() {
    if (!invoke) return;
    try {
      codexInstall = await invoke("codex_install_info");
    } catch (error) {
      appendLog(`codex_install_info failed: ${error}`);
    }
  }
  function renderCodexSetup(assistant) {
    const box = $("codex-setup");
    const text = $("codex-setup-text");
    const ways = $("codex-setup-ways");
    const button = $("codex-install");
    const showWays = (lines) => {
      ways.innerHTML = "";
      for (const line of lines) {
        const item = document.createElement("li");
        item.textContent = line;
        ways.appendChild(item);
      }
    };
    if (assistant.signed_in) {
      justInstalled = false;
      accountSaysSignedOut = false;
    }
    if (!assistant.executable) {
      if (!codexInstall) {
        box.hidden = true;
        return;
      }
      box.hidden = false;
      const state = codexInstall.state || { kind: "idle" };
      if (!codexInstall.winget) {
        text.textContent = "Codex CLI is not installed, and winget (the Windows Package Manager) is not on this PC. Install Codex another way, then come back:";
        showWays([
          "With Node.js: npm install -g @openai/codex",
          "Or download Codex from github.com/openai/codex/releases and set the path of codex.exe in Config → Codex executable",
        ]);
        button.hidden = true;
        return;
      }
      showWays([]);
      button.hidden = false;
      button.disabled = state.kind === "running";
      if (state.kind === "running") {
        text.textContent = "Installing Codex in the console window… Answer winget there if it asks; the window closes after you press a key.";
      } else if (state.kind === "finished" && state.code !== 0) {
        text.textContent = `winget ended with code ${state.code}; the console window showed why. Codex CLI is still not installed.`;
      } else if (state.kind === "failed") {
        text.textContent = `The install did not start: ${state.message}`;
      } else {
        text.textContent = `Codex CLI is not installed; the assistant needs it. Install Codex runs winget (the Windows Package Manager) in a console window you can see: package ${codexInstall.package} from ${codexInstall.source}, about ${codexInstall.download_mb} MB to download and ${codexInstall.disk_mb} MB on disk. Nothing is installed until you press the button.`;
      }
      return;
    }
    if (!assistant.signed_in && (justInstalled || accountSaysSignedOut || assistant.summary === "not signed in")) {
      box.hidden = false;
      text.textContent = `Codex is installed${assistant.version ? ` (${assistant.version})` : ""}. Next: Sign in to Codex (opens the ChatGPT sign-in in your browser). If you signed in to Codex before, press Check Codex.`;
      showWays([]);
      button.hidden = true;
      return;
    }
    box.hidden = true;
  }
  $("codex-install").addEventListener("click", async () => {
    if (!invoke) return;
    $("codex-install").disabled = true;
    try {
      await invoke("codex_install");
    } catch (error) {
      $("status-feedback").textContent = `Install Codex: ${error}`;
    }
    await loadCodexInstall();
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
            loadCodexInstall().then(refresh);
            loadCapabilities();
            break;
          // The console with winget closed: its final state, then the finder looks again.
          case "codex_install":
            if (codexInstall) codexInstall.state = payload.state;
            if (payload.state?.kind === "finished" && payload.state.code === 0) justInstalled = true;
            refresh();
            break;
          default:
            refresh();
        }
      });
    }
    await refreshSystem();
    await loadCodexInstall();
    try {
      wakeModels = await invoke("wake_models");
    } catch (error) {
      appendLog(`wake_models failed: ${error}`);
    }
    await loadSettings();
    buildWakeLists();
    await loadCapabilities();
    fillForm();
    // The install dialog may open from here on: the model names and the mode are known.
    install.armed = true;
    await refresh();
    await loadChat();
    // The status timer and the memory timer are set by the diagnostics switches
    // (`applyDiagnostics`, run by `fillForm`); without stored settings both are off.
    if (!statusTimer) applyDiagnostics();
    setInterval(renderUptime, 1000);
  }

  start();
})();
