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
  // `true` once the text is on the clipboard; the Console line names only `what`.
  async function copyText(text, what) {
    try {
      await navigator.clipboard.writeText(text);
      appendLog(`${what} copied to the clipboard`);
      return true;
    } catch (error) {
      appendLog(`copy failed: ${error}`);
      return false;
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
    // The answers' channel list: a channel's "can't post" mark may have changed in Discord.
    if (name === "config") loadTextChannels();
    // Which Whisper models and voices are on disk may have changed (a download, a file put
    // into the assets folder by hand).
    if (name === "config" || name === "settings") reloadWhisperLists();
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
      // A slider for a factor shown as such (the voice's speed, 1.25×).
      case "factor": return Number(control.value);
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
  // A slider's percentage (or factor) on its name line; a switch's "On" or "Off" next to the
  // switch.
  const LABELLED_KINDS = ["percent", "bool", "factor"];
  function percentLabel(control) {
    const label = $(`${control.id}-label`);
    if (!label) return;
    const kind = control.dataset.kind;
    label.textContent = kind === "bool"
      ? (control.checked ? "On" : "Off")
      : kind === "factor" ? `${Number(control.value).toFixed(2)}×` : `${control.value} %`;
  }
  function writeControl(control, value) {
    if (control.dataset.kind === "percent") {
      control.value = Math.round(Number(value ?? 0) * 100);
      percentLabel(control);
    } else if (control.dataset.kind === "factor") {
      control.value = value == null ? control.defaultValue : value;
      percentLabel(control);
    } else if (control.dataset.kind === "bool") {
      control.checked = Boolean(value);
      percentLabel(control);
    } else if (control.dataset.kind === "list") {
      // Phrases are separated by commas; a phrase may have several words.
      control.value = Array.isArray(value) ? value.join(", ") : "";
    } else {
      control.value = value == null ? "" : value;
    }
  }

  // The engines' names (change `claude-and-opencode-engines`): the product for the window,
  // the short name the chat labels use.
  const ENGINE_PRODUCTS = { codex: "Codex", claude: "Claude Code", opencode: "OpenCode" };
  const engineProduct = (engine) => ENGINE_PRODUCTS[engine] || "Codex";
  // The stored engine; Codex for a document without it.
  const engineOf = () => stored?.assistant?.engine || "codex";

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
  const feedbackTimers = new Map();
  // The feedback line of the tab that holds `node` (Settings and Config each have one for
  // their automatically saved fields); none on the other tabs.
  const tabFeedback = (node) => node.closest(".tab-panel")?.querySelector(".form-feedback[data-autosave]") || null;
  // A line in a tab's feedback line, back to its resting text after two seconds.
  function showTabFeedback(node, text, ms = SAVED_MARK_MS) {
    const feedback = tabFeedback(node);
    if (!feedback) return;
    feedback.textContent = text;
    clearTimeout(feedbackTimers.get(feedback.id));
    feedbackTimers.set(feedback.id, setTimeout(() => {
      feedback.textContent = feedback.dataset.autosave;
    }, ms));
  }
  function markSaved(control) {
    const label = control.closest("label");
    if (label) {
      label.classList.add("is-saved");
      clearTimeout(savedTimers.get(control.id));
      savedTimers.set(control.id, setTimeout(() => label.classList.remove("is-saved"), SAVED_MARK_MS));
    }
    showTabFeedback(control, `Saved: ${fieldName(control)}`);
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
  // What a field shows: the stored value, or for a phrase list never set the selected
  // model's default phrases, which `wake_models` lists with each model. A Whisper phrase list
  // never set shows "bro" or "peace out", whatever the language (change
  // `whisper-config-layout`, D2).
  function shownValue(control) {
    const value = readByPath(stored, pathOf(control));
    if (value !== undefined) return value;
    if (control.dataset.whisperPhrases) return WHISPER_DEFAULT_PHRASES[control.dataset.whisperPhrases] ?? [];
    if (!control.dataset.phrases) return value;
    const model = wakeModels.find((entry) => entry.id === stored?.wake?.model);
    return model?.[`default_${control.dataset.phrases}`] ?? [];
  }
  // Writes a field from the stored settings unless it is being edited or holds a value of
  // its own; `force` after the field's own successful save.
  function fillField(control, force) {
    if (!stored) return;
    if (!force && (stateOf(control.id) !== "clean" || isBusy(control))) return;
    writeControl(control, shownValue(control));
    if (control.tagName === "SELECT" && !control.options.length) return;
    mirrorListed(control);
  }
  function fillForm() {
    document.querySelectorAll("[data-path]").forEach((control) => fillField(control, false));
    showPhraseModel();
    fillConfig(false);
    applyAppearance(stored?.ui);
    applyCollapsedCards();
    handoffHint();
    applyDiagnostics();
    renderAnswersChannel();
    applyAssistantMode();
    applyEngine();
    showVoiceInfo();
  }

  // ---------- Collapsible cards on the Settings and Config tabs (`ui.collapsed_cards`) ----------
  // The title button folds a card to its title row. The state is saved like the theme; ids
  // this window does not know (from a newer build) are kept in the stored list. Not a form
  // field: no saved mark, no feedback line.
  const collapsibleCards = [...document.querySelectorAll(".collapsible[data-card]")];
  const cardIds = collapsibleCards.map((card) => card.dataset.card);
  let collapsedCards = new Set();
  let collapseSaves = 0; // saves in flight: the window's own state wins until they return
  function showCollapsed() {
    for (const card of collapsibleCards) {
      const collapsed = collapsedCards.has(card.dataset.card);
      card.classList.toggle("is-collapsed", collapsed);
      card.querySelector(".card-toggle").setAttribute("aria-expanded", String(!collapsed));
      card.querySelector(".card-body").hidden = collapsed;
    }
  }
  function applyCollapsedCards() {
    if (collapseSaves > 0) return;
    const ids = Array.isArray(stored?.ui?.collapsed_cards) ? stored.ui.collapsed_cards : [];
    collapsedCards = new Set(ids.filter((id) => cardIds.includes(id)));
    showCollapsed();
  }
  async function toggleCard(id) {
    if (collapsedCards.has(id)) collapsedCards.delete(id);
    else collapsedCards.add(id);
    showCollapsed();
    if (!invoke) return;
    const unknown = (stored?.ui?.collapsed_cards || []).filter((other) => !cardIds.includes(other));
    const list = unknown.concat(cardIds.filter((other) => collapsedCards.has(other)));
    collapseSaves += 1;
    try {
      stored = await invoke("save_settings", { patch: { ui: { collapsed_cards: list } } });
    } catch (error) {
      appendLog(`collapsed cards not saved: ${error}`);
    } finally {
      collapseSaves -= 1;
    }
  }
  for (const card of collapsibleCards) {
    card.querySelector(".card-toggle").addEventListener("click", () => toggleCard(card.dataset.card));
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
      ? `${hint} Realtime voice mode only. Applies from the next call. Default: conversation items.`
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
      if (control.dataset.path === "assistant.mode") applyAssistantMode();
      if (control.dataset.path === "assistant.one_speaker_at_a_time") applyInterruptSwitch();
      // The access mode's warnings follow a saved mode or URL at once, and so does the
      // Claude subscription notice and the ChatGPT account notice.
      if (control.dataset.path.startsWith("discord.")) {
        renderAccessNotes();
        renderClaudeNotice();
        renderCodexNotice();
      }
      // The CUDA runtime's notice follows the chosen device at once.
      if (control.id === "whisper-device" && lastStatus) renderWhisperNotices(lastStatus);
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
      if (LABELLED_KINDS.includes(control.dataset.kind)) percentLabel(control);
      if (stateOf(control.id) === "saving") return;
      const differs = !stored || !sameValue(controlValue(control), shownValue(control));
      if (stateOf(control.id) !== "error") setState(control.id, differs ? "dirty" : "clean");
    });
    control.addEventListener("change", () => {
      if (control.dataset.path.startsWith("ui.")) {
        applyAppearance({ theme: $("setting-theme").value, font_size: Number($("setting-font-size").value) });
      }
      // The wake model saves at once; its phrases follow it (change `models-in-settings`).
      if (control.id === "wake-model") {
        chooseWakeModel(control);
        return;
      }
      // A chosen channel for the answers saves together with its channel.
      if (control.id === "post-answers") {
        choosePostAnswers();
        return;
      }
      // Another engine (change `claude-and-opencode-engines`): the Whisper mode with it
      // when it has no voice mode, and the Claude subscription dialog the first time.
      if (control.id === "assistant-engine") {
        chooseEngine(control);
        return;
      }
      // "Realtime voice" with Claude Code or OpenCode takes Codex in the same save (change
      // `realtime-switches-to-codex`).
      if (control.id === "assistant-mode") {
        chooseMode(control);
        return;
      }
      // A Whisper model, device or voice saves at once; its card's notice lists what it
      // misses (change `models-in-settings`).
      if (WHISPER_CHOICES[control.id]) {
        chooseWhisper(control);
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
  // (`wake.engine` has no field: Vosk is its only value, shown by Recognition.)
  function buildWakeLists() {
    if (busyLists.has("wake-model")) {
      pendingRebuilds.add("wake");
      return;
    }
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
      document.querySelectorAll("[data-phrases]").forEach((phrases) => fillField(phrases, false));
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
  // The model list of Claude Code or OpenCode, from what the engine reported at its last
  // Check (change `claude-and-opencode-engines`); the free entry takes any other name.
  function buildEngineLists(engine) {
    const id = `${engine}-model`;
    if (busyLists.has(`${id}-select`)) {
      pendingRebuilds.add("codex");
      return;
    }
    const models = capabilities.models || [];
    const first = engine === "claude" ? "(Claude Code's default)" : "(OpenCode's default)";
    fillListed(
      id,
      models.length
        ? [{ value: "", label: first }].concat(
            models
              .filter((model) => model.id !== "default")
              .map((model) => ({ value: model.id, label: model.display_name === model.id ? model.id : `${model.display_name} (${model.id})` })),
          )
        : [],
    );
  }
  function buildCodexLists() {
    // The capabilities are the selected engine's: Claude Code and OpenCode fill their own.
    if (engineOf() !== "codex") {
      buildEngineLists(engineOf());
      return;
    }
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
    if (pending.includes("answers")) renderAnswersChannel();
    if (pending.includes("whisper")) buildWhisperLists();
  }

  // ---------- Post answers to Discord (`assistant.post_answers`, `assistant.answers_channel`) ----------
  // Off, the voice channel's chat and the channel of /join save at once and keep the stored
  // channel. A chosen channel saves together with its channel: at once when one is stored,
  // else when one is picked; until then nothing is saved and the card asks for a channel.
  // The channel list holds the text channels of the bot's servers (`discord_text_channels`),
  // read when the Config tab opens and when the Discord connection changes.
  const postAnswers = $("post-answers");
  const answersChannel = $("answers-channel");
  let textChannels = []; // [{ guild_id, guild_name, channels: [{ id, name, can_post }] }]
  let answersChoice = null; // the channel being saved
  let discordState = null; // the connection state the channel list was read in
  const storedChannel = () => String(readByPath(stored, "assistant.answers_channel") ?? "").trim();
  function addOption(parent, value, text) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = text;
    parent.appendChild(option);
  }
  function renderAnswersChannel() {
    const chosen = postAnswers.value === "channel";
    const current = answersChoice ?? storedChannel();
    const connected = textChannels.length > 0;
    $("answers-channel-field").hidden = !chosen;
    $("answers-channel-pick").hidden = !(chosen && !current);
    $("answers-channel-note").textContent = connected
      ? "The text channels of the servers the bot is in. Only answers from this channel's server are posted; in another server nothing is. A channel marked can't post lacks one of the three permissions for the bot: fix it in Discord, and the mark goes when this tab is opened again."
      : "The channel names appear once the bot is connected.";
    if (busyLists.has(answersChannel.id)) {
      pendingRebuilds.add("answers");
      return;
    }
    answersChannel.innerHTML = "";
    if (!current) addOption(answersChannel, "", "Pick a channel…");
    const known = textChannels.some((group) => group.channels.some((channel) => channel.id === current));
    if (current && !known) {
      addOption(answersChannel, current, connected ? `${current} (not in the bot's servers)` : `Channel ${current}`);
    }
    for (const group of textChannels) {
      const optgroup = document.createElement("optgroup");
      optgroup.label = group.guild_name;
      for (const channel of group.channels) {
        addOption(optgroup, channel.id, `#${channel.name}${channel.can_post === false ? " (can't post)" : ""}`);
      }
      answersChannel.appendChild(optgroup);
    }
    answersChannel.value = current;
  }
  async function loadTextChannels() {
    if (!invoke) return;
    try {
      textChannels = await invoke("discord_text_channels");
    } catch (error) {
      textChannels = [];
      appendLog(`text channels failed: ${error}`);
    }
    renderAnswersChannel();
  }
  // Both fields in one save; `control` is the one the user changed.
  async function saveAnswerPosting(control, patch) {
    setState(control.id, "saving");
    try {
      stored = await invoke("save_settings", { patch: { assistant: patch } });
      for (const id of [postAnswers.id, answersChannel.id]) {
        setState(id, "clean");
        fieldMessage(id, "");
      }
      fillField(postAnswers, true);
      markSaved(control);
    } catch (error) {
      setState(control.id, "error");
      fieldMessage(control.id, String(error));
    }
    answersChoice = null;
    renderAnswersChannel();
  }
  function choosePostAnswers() {
    if (postAnswers.value !== "channel") {
      saveControl(postAnswers).then(renderAnswersChannel);
      return;
    }
    const channel = storedChannel();
    if (channel) {
      saveAnswerPosting(postAnswers, { post_answers: "channel", answers_channel: channel });
    } else {
      setState(postAnswers.id, "dirty");
      renderAnswersChannel();
    }
  }
  // Registered before the open-list tracking, so the choice is taken before a waiting
  // rebuild of the list runs.
  answersChannel.addEventListener("change", () => {
    const channel = answersChannel.value;
    if (!channel || !invoke) return;
    answersChoice = channel;
    saveAnswerPosting(answersChannel, { post_answers: "channel", answers_channel: channel });
  });
  trackOpenList(answersChannel);
  // When the lists were read: "Read 2026-11-14 18:30" in local time, or "Never read".
  const pad2 = (n) => String(n).padStart(2, "0");
  function readTime(ms) {
    const at = new Date(ms);
    return `${at.getFullYear()}-${pad2(at.getMonth() + 1)}-${pad2(at.getDate())} ${pad2(at.getHours())}:${pad2(at.getMinutes())}`;
  }
  let listsBusy = false; // Refresh lists is reading
  function showListsRead(text, error) {
    const line = $("codex-lists-read");
    line.textContent = text;
    line.classList.toggle("is-error", Boolean(error));
  }
  function renderListsRead() {
    if (listsBusy) return;
    const ms = capabilities.lists_read_at_ms;
    showListsRead(ms ? `Read ${readTime(ms)}` : "Never read");
  }
  // Refresh lists: what Check Codex does (account, voices, models, saved to the file),
  // then the lists again. A failure keeps the lists and shows why next to the button.
  $("codex-lists-refresh").addEventListener("click", async () => {
    if (!invoke || listsBusy) return;
    const button = $("codex-lists-refresh");
    listsBusy = true;
    button.disabled = true;
    showListsRead("Reading…");
    let failure = "";
    try {
      await invoke("codex_check");
    } catch (error) {
      failure = String(error);
    }
    listsBusy = false;
    button.disabled = false;
    await loadCapabilities();
    if (failure) showListsRead(`Not read: ${failure}`, true);
    refresh();
  });
  // Rebuilds the Codex lists only when what the engine offers has changed; the read time
  // is shown every time.
  async function loadCapabilities() {
    if (!invoke) return;
    try {
      const next = await invoke("capabilities");
      capabilities.lists_read_at_ms = next.lists_read_at_ms;
      renderListsRead();
      const key = JSON.stringify({ engine: engineOf(), models: next.models, voices: next.voices, default_voice: next.default_voice });
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

  // ---------- Chat ----------
  const chatLog = $("chat-log");
  const CHAT_LIMIT = 600;
  // The labels name the engine that wrote the entry (change `claude-and-opencode-engines`):
  // `GPT` is the realtime voice, which only Codex has.
  function chatKindLabel(kind, engine) {
    const labels = {
      assistant: "GPT",
      context: engine === "Codex" ? "Context → GPT" : `Context → ${engine}`,
      task: `Task → ${engine}`,
      agent: engine,
      reasoning: `${engine} thinking`,
      tool: `${engine} tool`,
      system: "Call",
      error: "Error",
    };
    return labels[kind];
  }
  let chatEntries = [];
  function chatVisible(entry) {
    if (entry.kind === "context") return $("chat-filter-context").checked;
    if (["task", "agent", "reasoning", "tool"].includes(entry.kind)) return $("chat-filter-agent").checked;
    if (entry.kind === "system") return $("chat-filter-system").checked;
    return true;
  }
  function chatWho(entry) {
    if (entry.kind === "user") return entry.who || "Voice channel";
    return chatKindLabel(entry.kind, entry.engine || "Codex") || entry.kind;
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
      empty.textContent = "Nothing yet. Everything heard and said in voice sessions appears here, together with what the assistant's engine does for each answer.";
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
  // The codec failure notice: the bot left a voice channel because the Opus codec failed.
  // One dialog collects every failure until OK closes it.
  const failureDialog = $("codec-failure");
  function showCodecFailure(failure) {
    const item = document.createElement("li");
    const codec = document.createElement("strong");
    codec.textContent = failure.codec;
    const message = document.createElement("span");
    message.className = "failure-message";
    message.textContent = `“${failure.message}”`;
    item.append(
      codec,
      ` failed while ${failure.activity}: `,
      message,
      `. The bot left ${failure.channel_name} in ${failure.guild_name}.`,
    );
    $("codec-failure-items").append(item);
    if (!failureDialog.open) failureDialog.showModal();
  }
  $("codec-failure-log").addEventListener("click", async () => {
    try {
      appendLog(`opened ${await invoke("open_log_folder")}`);
    } catch (error) {
      appendLog(`open log folder failed: ${error}`);
    }
  });
  $("codec-failure-ok").addEventListener("click", () => failureDialog.close());
  failureDialog.addEventListener("close", () => $("codec-failure-items").replaceChildren());

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
      link.title = item.page;
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
      showManual($("wake-download-manual"), !$("wake-download").hidden && $("wake-download").dataset.failed === "1");
      if (install.subject !== "wake") return;
      renderManual($("wake-install-manual"), items);
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
  // What a button of the Assistant card did goes to the Config tab's feedback line.
  const OPENED_MS = 6000;
  $("wake-download-assets").addEventListener("click", () => openAssetsFolder((line) => {
    showTabFeedback($("wake-download-assets"), line, OPENED_MS);
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
    const button = $("open-models-folder");
    try {
      const path = await invoke("open_models_folder");
      showTabFeedback(button, `Opened ${path}`, OPENED_MS);
    } catch (error) {
      showTabFeedback(button, `Could not open the models folder: ${error}`, OPENED_MS);
    }
  });

  // ---------- The install dialog: the wake word's files are not on this PC ----------
  // One modal dialog. It opens only when a /join was refused because files are missing
  // (change `models-in-settings`, D4): the core logs the refusal, whose reply starts with
  // `WAKE_JOIN_REFUSED`. Never because the window started, a mode or activation was switched
  // or a model was picked: a choice saves at once and its notice lists the files. Download
  // asks for that model's files and shows the progress here; Not now, Hide, Close and Escape
  // store nothing, and the Assistant card's notice stays.
  //
  // The same dialog has a second subject, `whisper` (change `whisper-mode`): the files of the
  // Whisper mode. `install.subject` says which one is open; see "The install dialog for the
  // Whisper mode" below.
  const installDialog = $("wake-install");
  const install = {
    armed: false, // set by `start()` once the model list and the settings are loaded
    subject: "wake", // what the open dialog is about: `wake` or `whisper`
    reason: "",
    model: "",
    downloading: false, // Download was pressed for `model`
  };
  // The start of the reply to a /join refused for the wake word's files (`JOIN_NEEDS_WAKE_FILES`).
  const WAKE_JOIN_REFUSED = "The wake word needs downloads";
  const WAKE_MANUAL_NOTE = $("wake-install-manual-note").textContent;
  const installNames = (id) => wakeModels.find((entry) => entry.id === id)?.display_name || id;
  // `size` writes a file's size; the Whisper subject always shows the total.
  function installSetItems(items, size = (bytes) => `${megabytes(bytes)} MB`, alwaysTotal = false) {
    const list = $("wake-install-items");
    list.innerHTML = "";
    for (const item of items) {
      const line = document.createElement("li");
      line.textContent = `${item.label} — ${size(item.bytes)} from ${item.host}`;
      list.appendChild(line);
    }
    const total = items.reduce((sum, item) => sum + Number(item.bytes || 0), 0);
    $("wake-install-total").textContent = items.length > 1 || (alwaysTotal && items.length)
      ? `${size(total)} in total.`
      : "";
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
    Object.assign(install, { subject: "wake", reason, model, downloading: false });
    installDialog.dataset.reason = reason;
    installDialog.dataset.subject = "wake";
    $("wake-install-title").textContent = "The wake word is not installed";
    $("wake-install-manual-note").textContent = WAKE_MANUAL_NOTE;
    const onlyLibrary = items.length === 1 && items[0].label === "Vosk library";
    $("wake-install-text").textContent = onlyLibrary
      ? `The wake word needs the Vosk recognizer to use ${installNames(model)}, and it is not on this PC yet. Download it now?`
      : `VoiceBro listens for your wake phrase with the ${installNames(model)} model, which is not on this PC yet. Download it now?`;
    installSetItems(items);
    renderManual($("wake-install-manual"), items);
    showManual($("wake-install-manual"), true);
    $("wake-install-progress").hidden = true;
    $("wake-install-error").hidden = true;
    $("wake-install-note").textContent = "Nothing is downloaded until you press Download. Not now keeps the notice under Wake model in Config → Assistant. Run /join again once the files are here.";
    installButtons("Download", "Not now");
    installDialog.showModal();
    // The window may come to the front while the user types elsewhere: a stray Enter or
    // Space must not agree to a download, so the focus starts on Not now.
    $("wake-install-secondary").focus();
  }
  function installClose() {
    if (pendingRebuilds.size) runPendingRebuilds();
    if (installDialog.open) installDialog.close();
  }
  // A /join refused for the wake word's files: the dialog lists what the stored model misses.
  async function openForRefusedWakeJoin() {
    if (!invoke || !install.armed || installDialog.open || claudeDialog.open) return;
    const model = readByPath(stored, "wake.model") ?? lastWakeModel;
    let items = [];
    try {
      items = await invoke("wake_requirements", { model });
    } catch (error) {
      appendLog(`wake_requirements failed: ${error}`);
    }
    if (items.length && !installDialog.open && !claudeDialog.open) installOpen("join", model, items);
  }
  // The open dialog follows the wake state of the model it is about.
  function renderInstall(status) {
    const wake = status.wake || {};
    const state = wake.state || {};
    const kind = state.kind || "";
    if (installDialog.open && install.subject === "wake" && wake.model === install.model) {
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
  }
  // The wake model list saves at once (change `models-in-settings`, D3): a model that misses
  // files is listed in the notice under the field, with Download.
  function chooseWakeModel(select) {
    // The dialog of a refused join is modal, but a script or a keyboard path can still reach
    // the list: while it is open, nothing about the wake model changes.
    if (installDialog.open) {
      select.value = readByPath(stored, "wake.model") ?? "";
      setState(select.id, "clean");
      return;
    }
    saveControl(select);
  }
  $("wake-install-primary").addEventListener("click", async () => {
    const button = $("wake-install-primary");
    // Hidden while a download runs, disabled while one starts: a click that still arrives
    // (from a script) must not ask for a download twice.
    if (!invoke || button.hidden || button.disabled || !installDialog.open) return;
    if (install.subject === "whisper") {
      whisperInstallDownload();
      return;
    }
    button.disabled = true;
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

  // ---------- The Whisper mode's fields (changes `whisper-mode`, `whisper-config-layout`, `models-in-settings`) ----------
  // Config → Assistant holds the mode and the Whisper phrases (the rows of the mode shown,
  // design D3) and the note on what the mode misses; Settings → Whisper the model, device,
  // threads, language and what is in use; Settings → Local voice the voice (D4; D1 of
  // `models-in-settings`). The lists come from `whisper_models`,
  // `tts_voices` and `whisper_languages`; the fields are written from the stored settings like
  // every other field. The "In use now" lines, the download notices (from the status's
  // `missing`, in both modes) and the install dialog's Whisper subject follow the status
  // (`whisper` and `tts`), never the form.
  let whisperModels = [];
  let ttsVoices = [];
  let whisperLanguages = []; // [code, English name] in whisper.cpp's order
  // The wake catalog's languages, listed first.
  const WAKE_LANGUAGES = ["en", "ru", "cs", "nl", "fr", "de", "it", "pl", "pt", "es"];
  // A Whisper phrase list never set (`WhisperSettings::phrases_in_effect`), for every language.
  const WHISPER_DEFAULT_PHRASES = { activation: ["bro"], deactivation: ["peace out"] };
  const MODEL_GROUPS = [
    ["multilingual", "Multilingual"],
    ["english_only", "English only"],
    ["russian_tuned", "Russian-tuned"],
    ["local", "Your models"],
  ];
  const ENGINE_NAMES = { supertonic: "Supertonic 3", piper: "Piper", kokoro: "Kokoro", windows: "Windows" };
  const DEVICE_NAMES = { cpu: "CPU", gpu: "GPU (CUDA)" };
  const REGION_NAMES = { US: "US", GB: "UK" };
  // The start of the reply to a /join refused for missing files (`JOIN_NEEDS_DOWNLOADS`); the
  // app logs the refusal as an event, and a refused join leaves the bot out of the channel.
  const WHISPER_JOIN_REFUSED = "The Whisper mode needs downloads";
  // The fields whose choice may need files, and the requirement each one sets.
  const WHISPER_CHOICES = { "whisper-model": "model", "whisper-device": "device", "tts-voice": "voice" };
  const WHISPER_MANUAL_NOTE = $("whisper-download-manual").querySelector(".manual-note").textContent;

  // Sizes below one megabyte keep a decimal (the phonemizer is 0.4 MB).
  function sizeText(bytes) {
    const mb = Number(bytes || 0) / 1e6;
    return mb > 0 && mb < 1 ? `${mb.toFixed(1)} MB` : `${Math.round(mb)} MB`;
  }
  // "English (US)", "Portuguese (Brazil)"; `multi` is Supertonic's 31 languages.
  function languageName(tag) {
    if (tag === "multi") return "31 languages";
    const [primary = "", region] = String(tag || "").split("-");
    const name = whisperLanguages.find(([code]) => code === primary.toLowerCase())?.[1] || primary;
    if (!region) return name;
    const upper = region.toUpperCase();
    let regionName = REGION_NAMES[upper];
    if (!regionName) {
      try {
        regionName = new Intl.DisplayNames(["en"], { type: "region" }).of(upper);
      } catch {
        regionName = upper;
      }
    }
    return `${name} (${regionName})`;
  }
  const whisperModelName = (id) => whisperModels.find((entry) => entry.id === id)?.display_name || id || "—";
  const voiceName = (id) => ttsVoices.find((entry) => entry.id === id)?.display_name || id || "—";
  function whisperModelLabel(model) {
    const state = model.group === "local" ? "Whisper models folder" : model.installed ? "downloaded" : "to download";
    return [model.display_name, model.bytes ? sizeText(model.bytes) : "", state, model.note].filter(Boolean).join(" · ");
  }
  // A voice's `bytes` is what it still has to download.
  function voiceLabel(voice) {
    if (voice.engine === "windows") return `${voice.display_name} · built into Windows`;
    if (voice.installed) return `${voice.display_name} · downloaded`;
    const phonemizer = voice.needs_phonemizer ? " · needs the GPL-3.0 phonemizer" : "";
    return `${voice.display_name} · ${sizeText(voice.bytes)} · to download${phonemizer}`;
  }
  // One group per engine and language, in the order the app lists the voices.
  function voiceGroups() {
    const groups = new Map();
    for (const voice of ttsVoices) {
      const key = `${voice.engine}|${voice.language}`;
      if (!groups.has(key)) {
        groups.set(key, { label: `${ENGINE_NAMES[voice.engine] || voice.engine} — ${languageName(voice.language)}`, items: [] });
      }
      groups.get(key).items.push({ value: voice.id, label: voiceLabel(voice) });
    }
    return [...groups.values()];
  }
  // Automatic, then the wake models' ten languages, then the rest in Whisper's order.
  function languageGroups() {
    const names = new Map(whisperLanguages);
    const option = (code) => ({ value: code, label: `${names.get(code)} (${code})` });
    return [
      { items: [{ value: "auto", label: "Automatic" }] },
      { label: "Languages of the wake models", items: WAKE_LANGUAGES.filter((code) => names.has(code)).map(option) },
      {
        label: "Other languages",
        items: whisperLanguages.map(([code]) => code).filter((code) => !WAKE_LANGUAGES.includes(code)).map(option),
      },
    ];
  }
  function fillGroupedSelect(select, groups, current) {
    select.innerHTML = "";
    for (const group of groups) {
      if (!group.items.length) continue;
      let parent = select;
      if (group.label) {
        parent = document.createElement("optgroup");
        parent.label = group.label;
        select.appendChild(parent);
      }
      for (const item of group.items) addOption(parent, item.value, item.label);
    }
    if (current && ![...select.options].some((option) => option.value === current)) {
      addOption(select, current, `${current} (not in the catalog)`);
    }
    select.value = current || "";
  }
  // Rebuilt when the tab opens and after a download; never while a list is open or waits
  // for the dialog.
  function buildWhisperLists() {
    if (["whisper-model", "whisper-language", "tts-voice"].some((id) => busyLists.has(id))) {
      pendingRebuilds.add("whisper");
      return;
    }
    fillGroupedSelect(
      $("whisper-model"),
      MODEL_GROUPS.map(([group, label]) => ({
        label,
        items: whisperModels.filter((model) => model.group === group).map((model) => ({ value: model.id, label: whisperModelLabel(model) })),
      })),
      readByPath(stored, "whisper.model"),
    );
    fillGroupedSelect($("whisper-language"), languageGroups(), readByPath(stored, "whisper.language"));
    fillGroupedSelect($("tts-voice"), voiceGroups(), readByPath(stored, "tts.voice"));
    showVoiceInfo();
  }
  async function invokeOr(name, fallback) {
    try {
      return await invoke(name);
    } catch (error) {
      appendLog(`${name} failed: ${error}`);
      return fallback;
    }
  }
  async function reloadWhisperLists() {
    if (!invoke) return;
    whisperModels = await invokeOr("whisper_models", whisperModels);
    ttsVoices = await invokeOr("tts_voices", ttsVoices);
    buildWhisperLists();
  }
  // The engine, language and data license of the voice in the field; Piper and Kokoro say
  // that they need the GPL-3.0 phonemizer.
  function showVoiceInfo() {
    const voice = ttsVoices.find((entry) => entry.id === $("tts-voice").value);
    if (!voice) {
      $("tts-voice-info").textContent = "";
      return;
    }
    const parts = [`${ENGINE_NAMES[voice.engine] || voice.engine}, ${languageName(voice.language)}`];
    parts.push(voice.engine === "windows" ? "built into Windows, nothing to download" : `voice data license: ${voice.data_license}`);
    if (voice.needs_phonemizer) parts.push("needs the phonemizer piper-phonemize (GPL-3.0), a separate download");
    $("tts-voice-info").textContent = `${parts.join("; ")}.`;
  }
  $("tts-voice").addEventListener("input", showVoiceInfo);

  // The stored mode decides which rows of Config → Assistant show (the other mode's are
  // hidden, not disabled), the grey Recognition value, the Local voice card's note and the
  // Settings → Whisper card's On/Off label. Like a field, it follows the stored settings,
  // never a status refresh.
  let lastStatus = null; // the last status, for redrawing the notices after a save
  // Whisper hears in the Whisper mode and in the text answers mode (change
  // `text-answers-mode`), which has no voice; the rows of both are the Whisper mode's.
  const isWhisperMode = (mode) => mode === "whisper" || mode === "text";
  const whisperMode = () => isWhisperMode(readByPath(stored, "assistant.mode"));
  const textMode = () => readByPath(stored, "assistant.mode") === "text";
  function applyAssistantMode() {
    const mode = whisperMode() ? "whisper" : "realtime";
    document.querySelectorAll("#card-assistant-body [data-mode]").forEach((row) => {
      row.hidden = row.dataset.mode !== mode;
    });
    $("recognition-engine").value = mode === "whisper" ? "Whisper" : "Vosk";
    $("local-voice-note").textContent = textMode()
      ? "Not used in the text answers mode: the answers are posted as text in a Discord chat. You can download a voice now for the Whisper mode."
      : mode === "whisper"
        ? "This voice reads the assistant's answers into the voice channel."
        : "Not used in the realtime voice mode: the Codex call speaks with its own voice (Codex → Voice). You can download a voice now for the Whisper mode.";
    $("whisper-onoff").classList.toggle("is-on", mode === "whisper");
    $("whisper-onoff-flag").textContent = mode === "whisper" ? "On" : "Off";
    $("whisper-onoff-text").textContent = textMode()
      ? ": the assistant runs in the Whisper mode with text answers."
      : mode === "whisper"
        ? ": the assistant runs in the Whisper mode."
        : ": turn it on in Config → Assistant → Assistant mode.";
    applyInterruptSwitch();
    if (lastStatus) renderWhisperNotices(lastStatus);
  }
  // Change `interrupt-on-speech`: the switch shows in the Whisper mode only (the text answers
  // mode has no voice to stop) and is disabled, keeping its value, while One speaker at a time
  // is on, which never stops an answer for a question.
  function applyInterruptSwitch() {
    $("interrupt-row").hidden = !whisperMode() || textMode();
    const oneAtATime = Boolean(readByPath(stored, "assistant.one_speaker_at_a_time"));
    $("interrupt-on-speech").disabled = oneAtATime;
    $("interrupt-note").textContent = oneAtATime
      ? "No effect while One speaker at a time is on: a question then waits for the answer instead of stopping it."
      : "";
  }

  // ---------- The engine (change `claude-and-opencode-engines`) ----------
  // The engine card's title and rows, the Status tab's engine buttons, and the note under
  // the mode: the realtime voice, which only Codex has, stays selectable and switches the
  // engine to Codex (change `realtime-switches-to-codex`, design D3).
  function applyEngine() {
    const engine = engineOf();
    $("engine-card-title").textContent = engineProduct(engine);
    document.querySelectorAll("#card-codex-body [data-engine]").forEach((row) => {
      row.hidden = row.dataset.engine !== engine;
    });
    document.querySelectorAll("[data-engine-status]").forEach((node) => {
      node.hidden = node.dataset.engineStatus !== engine;
    });
    $("assistant-mode-note").textContent = engine === "codex"
      ? ""
      : "The voice mode works only with Codex: choosing Realtime voice switches the engine to Codex.";
    renderClaudeNotice();
    renderCodexNotice();
  }

  // After a save that changed the engine, from the Engine list or the mode list: the form,
  // the field's saved mark, the new engine's lists and the status (design D2).
  async function afterEngineSaved(select) {
    setState(select.id, "clean");
    fieldMessage(select.id, "");
    fillForm();
    markSaved(select);
    capabilitiesKey = "";
    await loadCapabilities();
  }

  // The mode list (change `realtime-switches-to-codex`, design D1): "Realtime voice" with
  // Claude Code or OpenCode saves the engine Codex in the same save, the mirror of the
  // Engine list; every other mode change saves the mode alone.
  async function chooseMode(select) {
    const engine = engineOf();
    if (select.value !== "realtime" || engine === "codex") {
      saveControl(select);
      return;
    }
    const previous = readByPath(stored, "assistant.mode");
    if (!invoke) return;
    if (installDialog.open || claudeDialog.open) {
      select.value = previous;
      setState(select.id, "clean");
      return;
    }
    setState(select.id, "saving");
    try {
      stored = await invoke("save_settings", {
        patch: { assistant: { mode: "realtime", engine: "codex" } },
      });
    } catch (error) {
      select.value = previous;
      setState(select.id, "error");
      fieldMessage(select.id, String(error));
      return;
    }
    await afterEngineSaved(select);
    refresh();
  }

  // The engine list: Claude Code and OpenCode take the Whisper mode in the same save; a
  // first choice of Claude Code with a subscription (or a sign-in not known yet) asks
  // whether only the owner's voice should pass (design D6).
  async function chooseEngine(select) {
    const engine = select.value;
    const previous = engineOf();
    if (!invoke || engine === previous) return;
    if (installDialog.open || claudeDialog.open) {
      select.value = previous;
      setState(select.id, "clean");
      return;
    }
    setState(select.id, "saving");
    // The kind of sign-in decides the dialog: read it first (Check, no model call).
    let kind = "unknown";
    if (engine === "claude") {
      try {
        kind = (await invoke("claude_check"))?.kind || "unknown";
      } catch (error) {
        appendLog(`claude_check failed: ${error}`);
      }
      claudeNotice.kind = kind;
    }
    const patch = { assistant: { engine } };
    const toWhisper = engine !== "codex" && !whisperMode();
    if (toWhisper) patch.assistant.mode = "whisper";
    try {
      stored = await invoke("save_settings", { patch });
    } catch (error) {
      select.value = previous;
      setState(select.id, "error");
      fieldMessage(select.id, String(error));
      return;
    }
    await afterEngineSaved(select);
    if (engine === "claude" && noticeApplies(kind) && !stored?.claude?.subscription_notice_shown) {
      openClaudeDialog();
    }
    refresh();
  }

  // ---------- The Claude subscription notice (design D6) ----------
  const ANTHROPIC_TERMS = "https://www.anthropic.com/legal/consumer-terms";
  const CLAUDE_CODE_LEGAL = "https://code.claude.com/docs/en/legal-and-compliance";
  const claudeDialog = $("claude-notice-dialog");
  const claudeNotice = { kind: null };
  // One save at a time for both account notices (Claude's and ChatGPT's): Hear only me
  // and Hide.
  let noticeBusy = false;
  // A subscription, or a sign-in not known yet (not read, not signed in): the notice
  // applies. An API key or a cloud provider: it does not.
  const noticeApplies = (kind) => !["api_key", "cloud"].includes(kind || "unknown");
  function noticeLink(href, label) {
    const anchor = document.createElement("a");
    anchor.href = href;
    anchor.target = "_blank";
    anchor.rel = "noopener";
    anchor.textContent = label;
    return anchor;
  }
  // The notice's two buttons and the line for a refused save; Hide runs `onHide(container)`.
  function noticeActions(container, onHide) {
    const actions = document.createElement("span");
    actions.className = "row-actions";
    const button = document.createElement("button");
    button.type = "button";
    button.className = "button primary";
    button.textContent = "Hear only me";
    button.addEventListener("click", () => hearOnlyMe(false));
    // Change `claude-notice-hide`: the notice goes for good; the choice is the user's.
    const hide = document.createElement("button");
    hide.type = "button";
    hide.className = "button";
    hide.textContent = "Hide";
    hide.addEventListener("click", () => onHide(container));
    actions.append(button, hide);
    const message = document.createElement("small");
    message.className = "field-message";
    message.dataset.noticeMessage = "";
    container.append(actions, message);
  }
  function noticeParagraphs(container, withButton) {
    container.replaceChildren();
    const text = document.createElement("p");
    text.textContent = "A Claude subscription (Free, Pro, Max, Team, Enterprise or another claude.ai plan) may be used only by you. Anthropic's Consumer Terms do not let you make your account available to anyone else, and Claude Code's terms say a subscription is meant for ordinary, individual use. While other people's voices reach the assistant, they use your subscription.";
    const advice = document.createElement("p");
    advice.textContent = "Let only your own voice through with \"Only my Discord account\" in the Access card (everyone still hears the answers), or sign Claude Code in with an API key, which Anthropic's Commercial Terms cover. VoiceBro does not enforce this: the choice is yours.";
    const links = document.createElement("p");
    links.className = "notice-links";
    links.append(noticeLink(ANTHROPIC_TERMS, "Anthropic's Consumer Terms"), " · ", noticeLink(CLAUDE_CODE_LEGAL, "Claude Code: legal and compliance"));
    container.append(text, advice, links);
    if (withButton) noticeActions(container, hideClaudeNotice);
  }
  // Shown at the top of the Claude Code card (change `claude-notice-hide`) while the engine
  // is Claude Code, its sign-in is a subscription or not known, more voices than the owner's
  // pass, and the user has not hidden it.
  function renderClaudeNotice() {
    // The kind of the last status of Claude Code, or of the Check the engine choice ran.
    const show = engineOf() === "claude" && noticeApplies(claudeNotice.kind)
      && (stored?.discord?.access || "everyone") !== "owner" && !stored?.claude?.subscription_notice_hidden;
    document.querySelectorAll("[data-claude-notice]").forEach((box) => {
      if (show && !box.childElementCount) noticeParagraphs(box, true);
      box.hidden = !show;
    });
  }
  // Hide: stores `claude.subscription_notice_hidden`; the notice then never shows again. A
  // refused save leaves it with the reason under it.
  async function hideClaudeNotice(box) {
    await hideNotice(box, { claude: { subscription_notice_hidden: true } });
  }
  // Saves a notice's Hide flag; a refused save leaves the notice with the reason under it.
  async function hideNotice(box, patch) {
    if (!invoke || noticeBusy) return;
    noticeBusy = true;
    const message = box.querySelector("[data-notice-message]");
    try {
      stored = await invoke("save_settings", { patch });
      if (message) message.textContent = "";
      renderClaudeNotice();
      renderCodexNotice();
    } catch (error) {
      if (message) message.textContent = `Not hidden: ${error}`;
    } finally {
      noticeBusy = false;
    }
  }
  function openClaudeDialog() {
    noticeParagraphs($("claude-notice-text"), false);
    $("claude-notice-progress").hidden = true;
    claudeDialog.showModal();
    // A stray Enter must not change the access mode: the focus starts on Not now.
    $("claude-notice-later").focus();
  }
  // Remembers that the dialog was answered. No download dialog follows it (change
  // `models-in-settings`, D7): Config → Assistant says what the Whisper mode misses.
  async function closeClaudeDialog() {
    if (claudeDialog.open) claudeDialog.close();
    try {
      stored = await invoke("save_settings", { patch: { claude: { subscription_notice_shown: true } } });
    } catch (error) {
      appendLog(`saving the notice failed: ${error}`);
    }
    refresh();
  }
  // "Hear only me": the owner-only access mode, and a link code when nobody is linked.
  async function hearOnlyMe(fromDialog) {
    if (!invoke || noticeBusy) return;
    noticeBusy = true;
    try {
      stored = await invoke("save_settings", { patch: { discord: { access: "owner" } } });
      fillField($("access-mode"), true);
      renderAccessNotes();
      renderClaudeNotice();
      renderCodexNotice();
      if (fromDialog) await closeClaudeDialog();
      if (!ownerStatus.linked) {
        // Open the Access card and show the code there.
        if (collapsedCards.has("access")) {
          collapsedCards.delete("access");
          showCollapsed();
        }
        $("owner-link").scrollIntoView({ block: "center" });
        await ownerAction("owner_link_start", ownerBusyText());
      }
    } catch (error) {
      appendLog(`Hear only me failed: ${error}`);
    } finally {
      noticeBusy = false;
    }
  }
  $("claude-notice-owner").addEventListener("click", () => hearOnlyMe(true));
  $("claude-notice-later").addEventListener("click", () => closeClaudeDialog());
  // Escape is "Not now".
  claudeDialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeClaudeDialog();
  });

  // ---------- The ChatGPT account notice (change `chatgpt-account-notice`, D1 and D2) ----
  const OPENAI_TERMS = "https://openai.com/policies/terms-of-use/";
  const OPENAI_SERVICE_TERMS = "https://openai.com/policies/service-terms/";
  // The Codex account's `type` from the last Codex status; none before Codex has read it.
  const codexNotice = { kind: null };
  // A ChatGPT account, none read yet or a type this window does not know: the notice
  // applies. An API key, Amazon Bedrock or Google Cloud: it does not.
  const codexNoticeApplies = (kind) => !["apiKey", "amazonBedrock", "gcp"].includes(kind);
  function codexNoticeParagraphs(container) {
    container.replaceChildren();
    const text = document.createElement("p");
    text.textContent = "A ChatGPT account (Free, Go, Plus, Pro, Business, Enterprise or another ChatGPT plan) may be used only by you. OpenAI's Terms of Use do not let you make your account available to anyone else. While other people's voices reach the assistant, they use your account.";
    const voice = document.createElement("p");
    voice.textContent = "In the realtime voice mode the assistant speaks with ChatGPT's voice. OpenAI's Service Terms allow that voice for non-commercial use only and do not allow distributing it as a recording or a sound file: do not record and publish what the assistant says.";
    const advice = document.createElement("p");
    advice.textContent = "Let only your own voice through with \"Only my Discord account\" in the Access card (everyone still hears the answers). VoiceBro does not enforce this: the choice is yours.";
    const links = document.createElement("p");
    links.className = "notice-links";
    links.append(noticeLink(OPENAI_TERMS, "OpenAI Terms of Use"), " · ", noticeLink(OPENAI_SERVICE_TERMS, "OpenAI Service Terms"));
    container.append(text, voice, advice, links);
    noticeActions(container, (box) => hideNotice(box, { codex: { account_notice_hidden: true } }));
  }
  // Shown at the top of the Codex card while the engine is Codex, its account is ChatGPT or
  // not known, more voices than the owner's pass, and the user has not hidden it. It never
  // starts Codex and opens no dialog.
  function renderCodexNotice() {
    const show = engineOf() === "codex" && codexNoticeApplies(codexNotice.kind)
      && (stored?.discord?.access || "everyone") !== "owner" && !stored?.codex?.account_notice_hidden;
    document.querySelectorAll("[data-codex-notice]").forEach((box) => {
      if (show && !box.childElementCount) codexNoticeParagraphs(box);
      box.hidden = !show;
    });
  }

  // ---- A model, device or voice: saved at once in both modes (change `models-in-settings`,
  // D3); its card's notice lists what it misses, with Download.
  const whisperSelection = () => ({
    model: readByPath(stored, "whisper.model") ?? "",
    device: readByPath(stored, "whisper.device") ?? "cpu",
    voice: readByPath(stored, "tts.voice") ?? "",
  });
  // One action per Download (design D5): Whisper's files of the selected model and device,
  // or the selected voice's files. Each is accepted in either mode.
  const downloadWhisper = () => invoke("whisper_download", { model: whisperSelection().model });
  const downloadVoice = () => invoke("tts_download", { voice: whisperSelection().voice });
  async function chooseWhisper(select) {
    // The dialog of a refused join is modal, but a script or a keyboard path can still reach
    // the list: while it is open, nothing about the choice changes.
    if (installDialog.open) {
      select.value = readByPath(stored, select.dataset.path) ?? "";
      setState(select.id, "clean");
      showVoiceInfo();
      return;
    }
    await saveControl(select);
    showVoiceInfo();
  }

  // ---- The install dialog for the Whisper mode: the same dialog, subject `whisper`. It
  // opens only at a /join refused for missing files (change `models-in-settings`, D4);
  // never because the window started, the mode was switched or something was picked. It
  // lists Whisper's and the voice's missing files, and Download asks for both.
  const whisperInstall = {
    joinRefusedAt: 0, // when the app refused a /join for missing files
    failed: false, // the dialog shows a failed download
  };
  function whisperOpen(reason, items) {
    if (installDialog.open) return;
    Object.assign(install, { subject: "whisper", reason, model: "", downloading: false });
    whisperInstall.failed = false;
    installDialog.dataset.reason = reason;
    installDialog.dataset.subject = "whisper";
    $("wake-install-title").textContent = "The Whisper mode is not installed";
    $("wake-install-text").textContent = textMode()
      ? "A /join was refused: the text answers mode recognizes speech on this PC with these files of Whisper, which are not here yet. Download them now?"
      : "A /join was refused: the Whisper mode recognizes speech and speaks the answers on this PC with these files of Whisper and the local voice, which are not here yet. Download them now?";
    installSetItems(items, sizeText, true);
    renderManual($("wake-install-manual"), items);
    $("wake-install-manual-note").textContent = WHISPER_MANUAL_NOTE;
    showManual($("wake-install-manual"), true);
    $("wake-install-progress").hidden = true;
    $("wake-install-error").hidden = true;
    $("wake-install-note").textContent = "Nothing is downloaded until you press Download. Not now keeps the Download buttons in Settings → Whisper and Settings → Local voice. Run /join again once the files are here.";
    installButtons("Download", "Not now");
    installDialog.showModal();
    // As for the wake word: a stray Enter or Space must not agree to a download.
    $("wake-install-secondary").focus();
  }
  // What the status says is missing: Whisper's files (runtime, model, VAD), then the voice's,
  // each file once. Computed by the app in both modes. The text answers mode needs no voice.
  function missingItems(status) {
    const seen = new Set();
    const items = [];
    const spoken = textMode() ? [] : status.tts?.missing || [];
    for (const item of (status.whisper?.missing || []).concat(spoken)) {
      if (seen.has(item.file)) continue;
      seen.add(item.file);
      items.push(item);
    }
    return items;
  }
  async function whisperInstallDownload() {
    const button = $("wake-install-primary");
    button.disabled = true;
    whisperInstall.failed = false;
    $("wake-install-error").hidden = true;
    $("wake-install-progress").hidden = false;
    // Not "the download": files put into the assets folder are only checked and unpacked.
    $("wake-install-progress").textContent = "Starting…";
    showManual($("wake-install-manual"), false);
    // Both parts; a part with nothing missing refuses, which is fine while the other starts.
    // The text answers mode asks for Whisper's files only.
    const results = await Promise.allSettled(textMode() ? [downloadWhisper()] : [downloadWhisper(), downloadVoice()]);
    const refusals = results.filter((result) => result.status === "rejected").map((result) => String(result.reason));
    if (refusals.length < results.length) {
      install.downloading = true;
      installButtons(null, "Hide");
    } else {
      $("wake-install-progress").hidden = true;
      $("wake-install-error").hidden = false;
      $("wake-install-error").textContent = `Could not start the download: ${refusals.join("; ")}`;
      showManual($("wake-install-manual"), true);
      installButtons("Try again", "Close");
    }
    refresh();
  }
  const whisperFetchText = (state) => (state.kind === "downloading"
    ? `Downloading the ${state.label}: ${state.percent} %`
    : `Checking and unpacking the ${state.label}…`);
  // The dialog follows both parts' states while it is open; a refused join opens it.
  function renderWhisperInstall(status) {
    const parts = [status.whisper?.state || {}, status.tts?.state || {}];
    const mode = status.settings?.assistant?.mode ?? readByPath(stored, "assistant.mode") ?? "realtime";
    const items = missingItems(status);
    const needs = items.length > 0 || parts.some((state) => state.kind === "needs_download");
    if (installDialog.open && install.subject === "whisper") {
      // Whisper's files and the voice's download side by side: one line each.
      const active = parts.filter((state) => fetching(state.kind));
      const failed = parts.find((state) => state.kind === "error");
      if (active.length) {
        install.downloading = true;
        whisperInstall.failed = false;
        $("wake-install-progress").hidden = false;
        $("wake-install-progress").textContent = active.map(whisperFetchText).join("\n");
        $("wake-install-error").hidden = true;
        showManual($("wake-install-manual"), false);
        installButtons(null, "Hide");
      } else if (failed && install.downloading) {
        $("wake-install-progress").hidden = true;
        $("wake-install-error").hidden = false;
        $("wake-install-error").textContent = `The download failed: ${failed.message}`;
        // Only what is still missing, for downloading by hand.
        if (!whisperInstall.failed) renderManual($("wake-install-manual"), items);
        whisperInstall.failed = true;
        showManual($("wake-install-manual"), true);
        installButtons("Try again", "Close");
      } else if (install.downloading && !needs && !failed) {
        installClose();
      }
    }
    const recent = (since, ms) => since > 0 && Date.now() - since < ms;
    // The Claude subscription dialog comes first; this one waits until it is answered.
    if (install.armed && !installDialog.open && !claudeDialog.open && isWhisperMode(mode) && items.length
      && recent(whisperInstall.joinRefusedAt, 5000)) {
      whisperInstall.joinRefusedAt = 0;
      whisperOpen("join", items);
    }
  }

  // ---- The download notices (design D5; change `models-in-settings`, D2), drawn from the
  // status's `missing` in both modes:
  // - `whisper`, in Settings → Whisper: every file the selected model misses for the
  //   selected device (the runtime, the model, the VAD model);
  // - `tts`, in Settings → Local voice: the voice engine, the phonemizer for Piper and
  //   Kokoro, and the voice.
  // Each Download asks for exactly its part: `whisper_download` (the runtime is part of the
  // model's set for that device) or `tts_download`. The progress comes through the part's
  // state (`downloading`, `installing`), in either mode.
  const NOTICE_IDS = ["whisper", "tts"];
  function noticeText(id) {
    if (id === "whisper") {
      const model = whisperModelName(readByPath(stored, "whisper.model"));
      return `Whisper needs these files for ${model} to write down what is said on this PC, and they are not here yet. Nothing is downloaded until you press Download.`;
    }
    return "The voice needs these files to speak on this PC, and they are not here yet. Nothing is downloaded until you press Download.";
  }
  // Per notice: it listed files or its Download was pressed (`active`), a download of its part
  // ran since (`fetched`), and a Download that could not start (`requestError`).
  const noticeMemo = Object.fromEntries(NOTICE_IDS.map((id) => [id, { active: false, fetched: false, requestError: "" }]));
  const partLast = { whisper: "", tts: "" }; // each part's last state, to see a download end
  const noticeItems = (id, status) => status[id]?.missing || [];
  function renderNotice(id, status) {
    const state = status[id]?.state || {};
    const items = noticeItems(id, status);
    const memo = noticeMemo[id];
    const box = $(`${id}-download`);
    const text = $(`${id}-download-text`);
    const list = $(`${id}-download-items`);
    const button = $(`${id}-download-button`);
    const manual = $(`${id}-download-manual`);
    const total = $(`${id}-download-total`);
    list.innerHTML = "";
    if (total) total.textContent = "";
    if (fetching(state.kind)) {
      memo.fetched = true;
      box.hidden = false;
      box.dataset.failed = "";
      text.textContent = whisperFetchText(state);
      showManual(manual, false);
      button.hidden = true;
    } else if (state.kind === "error" && (memo.fetched || box.dataset.failed === "1")) {
      box.hidden = false;
      box.dataset.failed = "1";
      text.textContent = `The download failed: ${state.message}`;
      renderManual(manual, items);
      showManual(manual, true);
      button.hidden = false;
      button.disabled = false;
    } else if (items.length) {
      memo.active = true;
      memo.fetched = false;
      box.hidden = false;
      box.dataset.failed = "";
      text.textContent = memo.requestError || noticeText(id);
      for (const item of items) {
        const line = document.createElement("li");
        line.textContent = `${item.label} — ${sizeText(item.bytes)} from ${item.host}`;
        list.appendChild(line);
      }
      if (total && items.length > 1) {
        total.textContent = `${sizeText(items.reduce((sum, item) => sum + Number(item.bytes || 0), 0))} in total.`;
      }
      renderManual(manual, items);
      showManual(manual, true);
      button.hidden = false;
      button.disabled = false;
    } else {
      Object.assign(memo, { active: false, fetched: false, requestError: "" });
      box.hidden = true;
      box.dataset.failed = "";
    }
  }
  // Change `models-in-settings` (D6): in the Whisper mode, Config → Assistant says what of
  // Whisper and the voice is not on this PC, and its button shows the card that downloads it.
  // It follows the status, never the form.
  function renderWhisperMissing(status) {
    const box = $("whisper-missing");
    const heard = status.whisper?.missing || [];
    // The text answers mode needs no voice (change `text-answers-mode`).
    const spoken = textMode() ? [] : status.tts?.missing || [];
    if (!whisperMode() || (!heard.length && !spoken.length)) {
      box.hidden = true;
      return;
    }
    const parts = [];
    if (heard.length) parts.push(`Whisper's files for the model ${whisperModelName(readByPath(stored, "whisper.model"))}`);
    if (spoken.length) parts.push(`the voice ${voiceName(readByPath(stored, "tts.voice"))}`);
    const them = parts.length > 1 ? "them" : "it";
    $("whisper-missing-text").textContent = `Not downloaded: ${parts.join(" and ")}. The Whisper mode needs ${them} on this PC, and /join is refused until then: download ${them} first in Settings.`;
    box.dataset.card = heard.length ? "whisper" : "local-voice";
    box.hidden = false;
  }
  // Shows the Settings tab with the card expanded and its download notice in view.
  async function openSettingsCard(id) {
    showTab("settings");
    if (collapsedCards.has(id)) await toggleCard(id);
    const notice = $(id === "whisper" ? "whisper-download" : "tts-download");
    const target = notice && !notice.hidden ? notice : document.querySelector(`[data-card="${id}"]`);
    target.scrollIntoView({ block: "center" });
  }
  $("whisper-missing-open").addEventListener("click", () => openSettingsCard($("whisper-missing").dataset.card || "whisper"));
  function renderWhisperNotices(status) {
    for (const id of NOTICE_IDS) renderNotice(id, status);
    renderWhisperMissing(status);
    // A download ended: the lists' "downloaded" marks changed.
    let ended = false;
    for (const part of ["whisper", "tts"]) {
      const kind = status[part]?.state?.kind || "";
      if (fetching(partLast[part]) && !fetching(kind)) ended = true;
      partLast[part] = kind;
    }
    if (ended) reloadWhisperLists();
  }
  for (const id of NOTICE_IDS) {
    $(`${id}-download-button`).addEventListener("click", async () => {
      if (!invoke) return;
      const memo = noticeMemo[id];
      const button = $(`${id}-download-button`);
      button.disabled = true;
      try {
        await (id === "tts" ? downloadVoice() : downloadWhisper());
        Object.assign(memo, { active: true, fetched: true, requestError: "" });
      } catch (error) {
        memo.requestError = `Could not start the download: ${error}`;
        $(`${id}-download-text`).textContent = memo.requestError;
        button.disabled = false;
      }
      refresh();
    });
    $(`${id}-download-assets`).addEventListener("click", () => openAssetsFolder((line) => {
      showTabFeedback($(`${id}-download-assets`), line, OPENED_MS);
    }));
  }
  $("open-whisper-models-folder").addEventListener("click", async () => {
    if (!invoke) return;
    const button = $("open-whisper-models-folder");
    try {
      showTabFeedback(button, `Opened ${await invoke("open_whisper_models_folder")}`, OPENED_MS);
    } catch (error) {
      showTabFeedback(button, `Could not open the Whisper models folder: ${error}`, OPENED_MS);
    }
  });

  // ---- "In use now" on Settings → Whisper, and the Status tab's tiles (status, not form).
  function whisperStateText(state, name) {
    switch (state.kind) {
      case "needs_download": return `${name}: its files are not on this PC; nothing is downloaded until you press Download`;
      case "downloading": return `downloading the ${state.label}: ${state.percent} %`;
      case "installing": return `checking and unpacking the ${state.label}`;
      case "idle": return `${name}: on this PC; loads at the next /join`;
      case "loading": return `${name}: loading`;
      case "ready": return `${name}: loaded`;
      case "error": return `error: ${state.message}`;
      default: return "—";
    }
  }
  function whisperDeviceText(whisper) {
    const requested = whisper.device_requested || "cpu";
    const used = whisper.device_used;
    if (!used) return `${DEVICE_NAMES[requested] || requested} chosen; nothing is loaded`;
    if (requested === "gpu" && used === "cpu") {
      return `GPU requested, CPU used: ${whisper.fallback_reason || "the GPU did not load"}${whisper.backend ? ` (${whisper.backend})` : ""}`;
    }
    return `${DEVICE_NAMES[used] || used}${whisper.backend ? `: ${whisper.backend}` : ""}`;
  }
  function ttsText(tts) {
    const state = tts.state || {};
    const name = voiceName(tts.voice);
    if (state.kind !== "ready") return whisperStateText(state, name);
    const details = [
      ENGINE_NAMES[tts.engine] || tts.engine,
      tts.sample_rate ? `${tts.sample_rate / 1000} kHz` : "",
      tts.load_ms != null ? `loaded in ${tts.load_ms} ms` : "",
    ].filter(Boolean).join(", ");
    const first = tts.last_first_audio_ms != null ? `; the last answer's first audio came after ${tts.last_first_audio_ms} ms` : "";
    return `${name}: ready${details ? ` (${details})` : ""}${first}`;
  }
  // The last turn of the Whisper mode, from the latency entries of every guild.
  function lastWhisperTurn(status) {
    const turns = allTurns(status).filter((turn) => turn.kind === "whisper" && turn.whisper);
    return turns[turns.length - 1] || null;
  }
  function whisperTimingText(timing) {
    const device = DEVICE_NAMES[timing.device] || timing.device;
    const start = `speech end → text ${number(timing.speech_to_text_ms)} ms (${decimal(timing.audio_seconds)} s of audio, ${whisperModelName(timing.model)}, ${device}); `
      + `text → Codex first words ${number(timing.text_to_first_words_ms)} ms; `;
    // The text answers mode has no voice (change `text-answers-mode`).
    if (timing.voice === false) return `${start}no voice (text answers)`;
    return `${start}first words → voice ${number(timing.first_words_to_voice_ms)} ms; `
      + `speech end → voice ${number(timing.speech_to_voice_ms)} ms`;
  }
  function renderWhisperNow(status) {
    const whisper = status.whisper || {};
    const state = whisper.state || {};
    const off = !state.kind || state.kind === "off";
    document.querySelectorAll("[data-whisper-row]").forEach((row) => {
      row.hidden = off;
    });
    if (off) {
      setText("whisper-now-recognition", "Off: nothing is loaded. The assistant is in the realtime voice mode.");
      return;
    }
    let recognition = whisperStateText(state, whisperModelName(whisper.model));
    if (state.kind === "ready" && whisper.load_ms != null) recognition += ` in ${whisper.load_ms} ms`;
    setText("whisper-now-recognition", recognition);
    setText("whisper-now-device", whisperDeviceText(whisper));
    setText("whisper-now-voice", status.settings?.assistant?.mode === "text"
      ? "Not used: the text answers mode posts the answers as text."
      : ttsText(status.tts || {}));
    const turn = lastWhisperTurn(status);
    setText("whisper-now-last", turn ? whisperTimingText(turn.whisper) : "No answer measured yet.");
  }
  function shortState(state) {
    switch (state.kind) {
      case "off": return "off";
      case "needs_download": return "files missing";
      case "downloading": return `downloading, ${state.percent} %`;
      case "installing": return "unpacking";
      case "idle": return "on this PC, loads at /join";
      case "loading": return "loading";
      case "ready": return "ready";
      case "error": return `error: ${state.message}`;
      default: return "unknown";
    }
  }
  function renderWhisperTiles(status, whisperOn) {
    const textAnswers = status.settings?.assistant?.mode === "text";
    setText("status-mode", textAnswers ? "Whisper, text answers" : whisperOn ? "Whisper" : "Realtime voice");
    document.querySelectorAll("[data-whisper-only]").forEach((tile) => {
      tile.hidden = !whisperOn;
    });
    if (!whisperOn) return;
    const whisper = status.whisper || {};
    const used = whisper.device_used;
    setText("status-whisper", `${whisperModelName(whisper.model)}, ${shortState(whisper.state || {})}`);
    setText("status-whisper-device", !used
      ? `not loaded (${DEVICE_NAMES[whisper.device_requested] || "CPU"} chosen)`
      : whisper.device_requested === "gpu" && used === "cpu"
        ? `CPU (GPU requested: ${whisper.fallback_reason || "not available"})`
        : DEVICE_NAMES[used] || used);
    setText("status-tts", textAnswers
      ? "not used: the answers are posted as text"
      : `${voiceName(status.tts?.voice)}, ${shortState(status.tts?.state || {})}`);
  }

  // ---- Preview: the voice reads the app's sample sentence; it plays here only.
  let previewUrl = "";
  $("tts-preview").addEventListener("click", async () => {
    if (!invoke) return;
    const button = $("tts-preview");
    const line = $("tts-preview-state");
    const audio = $("tts-preview-audio");
    button.disabled = true;
    line.classList.remove("is-error");
    line.textContent = "Reading the sample sentence…";
    try {
      const bytes = await invoke("tts_preview", { voice: $("tts-voice").value, speed: Number($("tts-speed").value) });
      if (previewUrl) URL.revokeObjectURL(previewUrl);
      previewUrl = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "audio/wav" }));
      audio.src = previewUrl;
      audio.hidden = false;
      line.textContent = "Playing here in the window only; nothing goes to Discord.";
      try {
        await audio.play();
      } catch (error) {
        line.textContent = `The sample is ready; press play to hear it (${error?.message || error}).`;
      }
    } catch (error) {
      line.textContent = `Preview failed: ${error}`;
      line.classList.add("is-error");
    } finally {
      button.disabled = false;
    }
  });
  $("tts-preview-audio").addEventListener("ended", () => {
    $("tts-preview-state").textContent = "Played here in the window only; nothing went to Discord.";
  });

  // ---------- Status (never touches the form) ----------
  function wakeText(wake) {
    const state = wake.state || {};
    let text;
    switch (state.kind) {
      case "off": text = "off (the session mode is not wake word)"; break;
      case "no_phrases": text = "no phrases configured"; break;
      case "needs_download":
        text = `waiting for your OK to download ${(state.items || []).map((item) => `the ${item.label} (${megabytes(item.bytes)} MB)`).join(" and ")}: Config → Assistant`;
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
    // One speaker at a time (change `speaker-turns`): who waits for the floor.
    if (gate.floor_waiting) parts.push(`${gate.floor_waiting} waiting to speak`);
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

    // The answers' channel list follows the connection (read again on every change of it).
    if (discord.state !== discordState) {
      discordState = discord.state;
      loadTextChannels();
    }

    // Status bar.
    const footer = $("footer-connection");
    footer.textContent = STATE_LABELS[discord.state] || "Unknown";
    footer.className = `connection-state ${discord.state || "disconnected"}`;
    $("footer-guilds").textContent = String(discord.guild_count || 0);
    $("footer-voice").textContent = String(channels.length);

    // The Opus codec of each join and call (Settings → Diagnostics); not a form field.
    const unique = (labels) => [...new Set(labels.filter(Boolean))].join(", ");
    const joinCodecs = unique(channels.map((channel) => channel.opus_codec));
    const callCodecs = unique((assistant.calls || []).map((call) => call.opus_codec));
    setText(
      "opus-codec-in-use",
      joinCodecs || callCodecs
        ? `In use: Discord ${joinCodecs || "none"}; call ${callCodecs || "none"}.`
        : "In use: none (the bot is in no voice channel).",
    );

    // Status tab.
    renderGuilds(discord);
    let discordText = STATE_LABELS[discord.state] || "unknown";
    if (discord.state === "error" && discord.last_error) discordText = `Error: ${discord.last_error}`;
    setText("status-discord", discordText);
    setText("status-bot", discord.user_name || "—");
    setText("status-voice", channels.length ? channels.map((c) => `${c.guild_name} / ${c.channel_name}`).join("; ") : "none");
    const people = channels.flatMap((c) => c.participants || []);
    setText("status-people", people.length ? people.join(", ") : "—");
    const engine = assistant.engine || engineOf();
    setText("status-engine", `${engineProduct(engine)}${assistant.version ? ` (${assistant.version})` : ""}`);
    const gates = Object.values(status.gates || {});
    setText("status-gates", gates.length ? gates.map(gateText).join("; ") : "not in a voice channel");
    // The Whisper mode (change `whisper-mode`): Vosk is off and Whisper spots the phrases.
    const whisperOn = isWhisperMode(status.settings?.assistant?.mode);
    setText("status-wake", whisperOn ? "off: the Whisper mode spots the phrases" : wakeText(status.wake || {}));
    renderWhisperTiles(status, whisperOn);
    setText("status-mute", status.muted ? "muted" : "not muted");
    $("mute-toggle").textContent = status.muted ? "Unmute assistant" : "Mute assistant";
    // The engine's account (change `claude-and-opencode-engines`): Codex's ChatGPT account,
    // Claude Code's kind of sign-in, OpenCode's providers.
    setText("status-account-label", engine === "claude" ? "Claude Code sign-in" : engine === "opencode" ? "OpenCode providers" : "Codex account");
    setText("status-codex", assistant.summary || "—");
    renderCodexSetup(engine === "codex" ? assistant : { signed_in: true, executable: "-" });
    // The ChatGPT account notice follows the account type Codex last read.
    if (engine === "codex") codexNotice.kind = assistant.account_type || null;
    renderClaudeSetup(engine === "claude" ? assistant : null);
    renderOpencodeSetup(engine === "opencode" ? assistant : null);
    const calls = assistant.calls || [];
    setText("status-call", calls.length
      ? calls.map((call) => `${call.state}${call.connected ? ", connected" : ""}${call.last_error ? `, error: ${call.last_error}` : ""}`).join("; ")
      : "no call");
    setText("status-thread-label", engine === "codex" ? "Codex thread" : "Model");
    setText("status-thread", engine !== "codex"
      ? assistant.model_in_use || "the engine's default"
      : assistant.model_in_use != null
        ? `${assistant.model_in_use || "default model"}, reasoning ${assistant.reasoning_effort_in_use || "default"}`
        : "—");
    renderClaudeNotice();
    renderCodexNotice();
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
    lastStatus = status;
    renderWhisperNow(status);
    renderWhisperNotices(status);
    renderWhisperInstall(status);
    ownerBotId = status.discord?.user_id || "";
    renderOwner(status.owner);
    renderConsent(status.consent);
    const badge = $("token-badge");
    badge.textContent = tokenSet ? "stored" : "not stored";
    badge.className = `badge ${tokenSet ? "ok" : "warn"}`;
    const settingsPath = `${status.data_dir}\\settings.json`;
    setText("config-path", settingsPath);
    setText("system-data-dir", status.data_dir || "—");
    setText("system-settings-path", settingsPath);
    setText("system-codex", assistant.executable || "not found");
  }

  // ---------- Access card: the owner link (change `owner-only-access`) ----------
  // Status, not form: the link state, the waiting code and its countdown are redrawn from
  // every status; the switch above them is a form field like the others.
  let ownerStatus = {};
  let ownerTimer = 0;
  // The bot's account id from the Discord status, for the copied mention (change
  // `owner-link-paste`, design D5); empty while the bot is not connected.
  let ownerBotId = "";
  // The code Copy last put on the clipboard and what came of it; a new code clears it.
  let ownerCopiedCode = "";
  let ownerCopyResult = "";
  const sentence = (text) => (text ? `${text[0].toUpperCase()}${text.slice(1)}.` : "");
  function renderOwner(owner) {
    ownerStatus = owner || {};
    const linked = ownerStatus.linked;
    const pending = ownerStatus.pending;
    const left = pending ? Math.max(0, Math.ceil((pending.expires_at_ms - Date.now()) / 1000)) : 0;
    const waiting = Boolean(pending) && left > 0;
    setText("owner-state", linked ? `${linked.name} (${linked.id})` : "not linked");
    $("owner-reason").textContent = !linked && ownerStatus.link_ended ? `Not linked: ${ownerStatus.link_ended}.` : "";
    $("owner-code").hidden = !waiting;
    if (waiting) {
      $("owner-command").textContent = pending.code;
      $("owner-countdown").textContent = `${Math.floor(left / 60)}:${pad2(left % 60)} left`;
      // Copy needs the bot's id for its mention (design D6).
      $("owner-copy").disabled = !ownerBotId;
      setText("owner-copy-message", !ownerBotId
        ? "Copy works once the bot is connected to Discord."
        : pending.code === ownerCopiedCode ? ownerCopyResult : "");
    }
    const ended = pending && !waiting ? "the code expired" : ownerStatus.code_ended;
    $("owner-code-ended").textContent = !waiting && !linked ? sentence(ended) : "";
    $("owner-link-start").hidden = waiting;
    $("owner-link-start").textContent = linked ? "Link another account" : "Link my Discord account";
    $("owner-link-cancel").hidden = !waiting;
    $("owner-unlink").hidden = !linked || waiting;
    renderAccessNotes();
    if (waiting && !ownerTimer) {
      ownerTimer = setInterval(() => renderOwner(ownerStatus), 1000);
    } else if (!waiting && ownerTimer) {
      clearInterval(ownerTimer);
      ownerTimer = 0;
    }
  }
  async function ownerAction(command, busyText) {
    if (!invoke) return;
    const buttons = ["owner-link-start", "owner-link-cancel", "owner-unlink"].map($);
    buttons.forEach((button) => (button.disabled = true));
    $("owner-message").textContent = busyText || "";
    try {
      await invoke(command);
      $("owner-message").textContent = "";
    } catch (error) {
      $("owner-message").textContent = String(error);
    } finally {
      buttons.forEach((button) => (button.disabled = false));
      await refresh();
    }
  }
  // The engine in use reads its account first; OpenCode has none.
  function ownerBusyText() {
    const engine = engineOf();
    if (engine === "claude") return "Reading the Claude account…";
    if (engine === "opencode") return "Making a code…";
    return "Reading the Codex account…";
  }
  $("owner-link-start").addEventListener("click", () => ownerAction("owner_link_start", ownerBusyText()));
  $("owner-link-cancel").addEventListener("click", () => ownerAction("owner_link_cancel"));
  // Copy (change `owner-link-paste`, design D6): the bot's mention, `link` and the code, one
  // message to paste in Discord. The Console line names no code.
  $("owner-copy").addEventListener("click", async () => {
    const code = ownerStatus.pending?.code;
    if (!code || !ownerBotId) return;
    const copied = await copyText(`<@${ownerBotId}> link ${code}`, "link message");
    ownerCopiedCode = code;
    ownerCopyResult = copied
      ? "Copied. Paste it in Discord and send it."
      : "Copying failed: type /link and the code in Discord instead.";
    setText("owner-copy-message", ownerCopyResult);
  });
  $("owner-unlink").addEventListener("click", () => ownerAction("owner_unlink"));

  // ---------- Access card: the warnings of each mode (change `voice-consent`) ----------
  // From the stored settings and the link, never from the form fields.
  function renderAccessNotes() {
    const access = stored?.discord?.access || "everyone";
    const linked = Boolean(ownerStatus.linked);
    $("owner-note").textContent = access === "owner" && !linked
      ? "No account is linked: the assistant hears nobody until you link one."
      : "";
    $("consent-note").textContent = access === "agreed" && !stored?.discord?.privacy_url
      ? "No privacy policy URL: /optin is refused, so only your linked account is heard, until you set one."
      : "";
  }

  // ---------- Access card: the people who agreed (change `voice-consent`) ----------
  // Status, not form: redrawn from every status and after each Remove.
  let consentStatus = {};
  const agreedTime = (rfc3339) => {
    const time = new Date(rfc3339);
    return Number.isNaN(time.getTime()) ? rfc3339 : time.toLocaleString();
  };
  function renderConsent(consent) {
    consentStatus = consent || {};
    const records = consentStatus.records || [];
    const list = $("consent-people");
    const shown = Array.from(list.children).map((item) => item.dataset.id).join(",");
    if (shown !== records.map((record) => record.id).join(",")) {
      list.innerHTML = "";
      for (const record of records) {
        const item = document.createElement("li");
        item.dataset.id = record.id;
        const name = document.createElement("span");
        name.className = "consent-name";
        name.textContent = record.name;
        const time = document.createElement("span");
        time.className = "consent-time";
        time.textContent = `agreed ${agreedTime(record.agreed_at)}`;
        const remove = document.createElement("button");
        remove.type = "button";
        remove.className = "button";
        remove.textContent = "Remove";
        remove.setAttribute("aria-label", `Remove ${record.name}'s consent`);
        remove.addEventListener("click", () => removeConsent(record.id, remove));
        item.append(name, time, remove);
        list.appendChild(item);
      }
    }
    $("consent-empty").hidden = records.length > 0;
    $("consent-unreadable").textContent = consentStatus.unreadable ? sentence(consentStatus.unreadable) : "";
  }
  async function removeConsent(id, button) {
    if (!invoke) return;
    button.disabled = true;
    $("consent-message").textContent = "";
    try {
      renderConsent(await invoke("consent_remove", { id }));
    } catch (error) {
      $("consent-message").textContent = String(error);
      button.disabled = false;
    }
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
      ["Floor passes (one speaker at a time)", number(gates.reduce((sum, gate) => sum + (gate.counters?.floor_passes || 0), 0))],
      ["Waiting speech dropped, frames", number(gates.reduce((sum, gate) => sum + (gate.counters?.floor_dropped || 0), 0))],
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
      // The engine's share (change `claude-and-opencode-engines`): Codex, Claude Code or
      // OpenCode, whichever is selected.
      setText("memory-engine-name", engineProduct(stored?.assistant?.engine));
      setText("memory-codex", memory.engine_mb == null ? "not running" : mb(memory.engine_mb));
      setText("memory-total", mb(memory.total_mb));
      const share = (value) => `${memory.total_mb > 0 ? (100 * (value || 0)) / memory.total_mb : 0}%`;
      $("memory-meter-app").style.width = share(memory.app_mb);
      $("memory-meter-webview").style.width = share(memory.webview_mb);
      $("memory-meter-codex").style.width = share(memory.engine_mb);
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
  // ---------- Claude Code and OpenCode setup (change `claude-and-opencode-engines`) ----------
  // The same install offer as Codex's; then Claude Code's own sign-in, or where OpenCode's
  // providers are set up. `null` hides the box (another engine is selected).
  let claudeInstall = null; // claude_install_info
  let opencodeInstall = null; // opencode_install_info
  async function loadEngineInstalls() {
    if (!invoke) return;
    claudeInstall = await invokeOr("claude_install_info", null);
    opencodeInstall = await invokeOr("opencode_install_info", null);
  }
  function setupBox(prefix, lines, text, button) {
    $(`${prefix}-setup-text`).textContent = text;
    const ways = $(`${prefix}-setup-ways`);
    ways.replaceChildren(...lines.map((line) => {
      const item = document.createElement("li");
      item.textContent = line;
      return item;
    }));
    $(`${prefix}-install`).hidden = !button;
    $(`${prefix}-setup`).hidden = false;
  }
  // While the engine is missing: winget's offer, or the other ways without winget.
  function installOffer(prefix, info, product, otherWays) {
    const state = info.state || { kind: "idle" };
    if (!info.winget) {
      setupBox(prefix, otherWays, `${product} is not installed, and winget (the Windows Package Manager) is not on this PC. Install it another way, then come back:`, false);
      return;
    }
    $(`${prefix}-install`).disabled = state.kind === "running";
    if (state.kind === "running") {
      setupBox(prefix, [], `Installing ${product} in the console window… Answer winget there if it asks; the window closes after you press a key.`, true);
    } else if (state.kind === "finished" && state.code !== 0) {
      setupBox(prefix, [], `winget ended with code ${state.code}; the console window showed why. ${product} is still not installed.`, true);
    } else if (state.kind === "failed") {
      setupBox(prefix, [], `The install did not start: ${state.message}`, true);
    } else {
      setupBox(prefix, [], `${product} is not installed; the assistant needs it with this engine. Install ${product} runs winget in a console window you can see: package ${info.package} from ${info.source}, about ${info.download_mb} MB to download and ${info.disk_mb} MB on disk. Nothing is installed until you press the button.`, true);
    }
  }
  function renderClaudeSetup(assistant) {
    if (!assistant) {
      $("claude-setup").hidden = true;
      return;
    }
    claudeNotice.kind = assistant.sign_in_kind || null;
    if (!assistant.executable) {
      if (!claudeInstall) {
        $("claude-setup").hidden = true;
        return;
      }
      installOffer("claude", claudeInstall, "Claude Code", [
        "Claude Code's own installer: see code.claude.com/docs/en/setup",
        "Then set the path of claude.exe in Config → Claude Code executable if VoiceBro does not find it by itself",
      ]);
      return;
    }
    if (assistant.sign_in_kind === "none") {
      setupBox("claude", [], `Claude Code is installed${assistant.version ? ` (${assistant.version})` : ""}. Next: Sign in to Claude Code, which opens Claude Code's own sign-in in a console window; VoiceBro never sees your credentials. A Claude subscription is yours alone: see the notice in the Claude Code card on the Config tab.`, false);
      return;
    }
    $("claude-setup").hidden = true;
  }
  function renderOpencodeSetup(assistant) {
    if (!assistant) {
      $("opencode-setup").hidden = true;
      return;
    }
    if (!assistant.executable) {
      if (!opencodeInstall) {
        $("opencode-setup").hidden = true;
        return;
      }
      installOffer("opencode", opencodeInstall, "OpenCode", [
        "With Node.js: npm install -g opencode-ai",
        "Or download OpenCode from github.com/anomalyco/opencode/releases and set the path of opencode.exe in Config → OpenCode executable",
      ]);
      return;
    }
    if (!assistant.signed_in) {
      setupBox("opencode", [
        "In a terminal: opencode auth login, then pick a provider and sign in or paste its key",
        "The providers and their keys: opencode.ai/docs/providers",
      ], `OpenCode is installed${assistant.version ? ` (${assistant.version})` : ""}, and no provider is set up yet. Providers and keys are set up in OpenCode itself; VoiceBro never sees them. Then press Check OpenCode.`, false);
      return;
    }
    $("opencode-setup").hidden = true;
  }
  const engineAction = (command, done) => async () => {
    if (!invoke) return;
    try {
      const result = await invoke(command);
      if (done) $("status-feedback").textContent = done(result);
    } catch (error) {
      $("status-feedback").textContent = `${command.replace(/_/g, " ")}: ${error}`;
    }
    await loadEngineInstalls();
    capabilitiesKey = "";
    await loadCapabilities();
    await refresh();
  };
  $("claude-install").addEventListener("click", engineAction("claude_install"));
  $("opencode-install").addEventListener("click", engineAction("opencode_install"));
  $("claude-sign-in").addEventListener("click", engineAction(
    "claude_sign_in",
    () => "Claude Code's sign-in opened in a console window. Finish it there; VoiceBro reads the sign-in again when the window closes.",
  ));
  $("claude-check").addEventListener("click", engineAction(
    "claude_check",
    (signIn) => `Claude Code: ${signIn?.kind === "none" ? "not signed in; press Sign in to Claude Code" : `signed in (${(signIn?.kind || "unknown").replace("_", " ")})`}.`,
  ));
  $("opencode-check").addEventListener("click", engineAction(
    "opencode_check",
    (result) => `OpenCode: ${result?.providers?.length ? `providers ${result.providers.join(", ")}` : "no provider set up yet"}.`,
  ));

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
          // Log lines arrive through `app-log` with everything else the log file gets. Two are
          // also triggers, the only ones of the install dialog (change `models-in-settings`,
          // D4): a /join refused for missing files in the Whisper mode or for the wake word,
          // which leaves the bot out of the channel and the status otherwise unchanged.
          case "log": {
            const line = String(payload.line || "");
            if (line.includes(WHISPER_JOIN_REFUSED)) {
              whisperInstall.joinRefusedAt = Date.now();
              refresh();
            } else if (line.includes(WAKE_JOIN_REFUSED)) {
              openForRefusedWakeJoin();
            }
            break;
          }
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
          // Change `claude-and-opencode-engines`: the same for the other engines; Claude
          // Code's sign-in console closed and its sign-in was read again.
          case "claude_install":
          case "opencode_install":
          case "claude_sign_in":
            loadEngineInstalls().then(() => {
              capabilitiesKey = "";
              loadCapabilities();
              refresh();
            });
            break;
          case "codec_failure":
            showCodecFailure(payload);
            refresh();
            break;
          default:
            refresh();
        }
      });
    }
    await refreshSystem();
    await loadCodexInstall();
    await loadEngineInstalls();
    try {
      wakeModels = await invoke("wake_models");
    } catch (error) {
      appendLog(`wake_models failed: ${error}`);
    }
    whisperModels = await invokeOr("whisper_models", []);
    ttsVoices = await invokeOr("tts_voices", []);
    whisperLanguages = await invokeOr("whisper_languages", []);
    await loadSettings();
    buildWakeLists();
    buildWhisperLists();
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
