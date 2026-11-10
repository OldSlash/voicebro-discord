// VoiceBro UI. Talks to Rust through window.__TAURI__ (withGlobalTauri).
(() => {
  const tauri = window.__TAURI__;
  const invoke = tauri?.core?.invoke;
  const listen = tauri?.event?.listen;
  const $ = (id) => document.getElementById(id);

  const logPane = $("log");
  const MAX_LOG_LINES = 500;
  function appendLog(line) {
    const time = new Date().toLocaleTimeString();
    logPane.textContent += `${time}  ${line}\n`;
    const lines = logPane.textContent.split("\n");
    if (lines.length > MAX_LOG_LINES) {
      logPane.textContent = lines.slice(lines.length - MAX_LOG_LINES).join("\n");
    }
    logPane.scrollTop = logPane.scrollHeight;
  }

  // Tabs
  document.querySelectorAll(".tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      document.querySelectorAll(".tab").forEach((t) => t.classList.toggle("active", t === tab));
      document.querySelectorAll(".panel").forEach((panel) => {
        panel.classList.toggle("active", panel.id === `tab-${tab.dataset.tab}`);
      });
    });
  });

  // Settings controls: each carries its JSON path and value kind.
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
  function controlValue(control) {
    switch (control.dataset.kind) {
      case "number": return Number(control.value);
      case "bool": return control.checked;
      case "list":
        return control.value
          .split(/[\n,]/)
          .map((item) => item.trim())
          .filter((item) => item.length > 0);
      default: return control.value;
    }
  }
  function setControlValue(control, value) {
    if (document.activeElement === control) return;
    if (control.dataset.kind === "bool") control.checked = Boolean(value);
    else if (control.dataset.kind === "list") control.value = Array.isArray(value) ? value.join("\n") : "";
    else control.value = value == null ? "" : value;
    // A listed field mirrors its text input in the select when the select has options.
    const select = document.querySelector(`select.listed[data-for="${control.id}"]`);
    if (select && select.options.length > 0) {
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
      select.style.display = "none";
      input.style.display = "";
      return;
    }
    for (const option of options) {
      const node = document.createElement("option");
      node.value = option.value;
      node.textContent = option.label;
      select.appendChild(node);
    }
    select.style.display = "";
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
    });
  });

  let wakeModels = [];
  let capabilities = { models: [], voices: [] };
  function applyWakeLists(settings) {
    const engineSelect = $("wake-engine");
    const modelSelect = $("wake-model");
    const engines = [...new Set(wakeModels.map((model) => model.engine))];
    const fill = (select, items, current) => {
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
      if (document.activeElement !== select) select.value = current || "";
    };
    fill(engineSelect, engines.map((engine) => ({ value: engine, label: engine })), settings?.wake?.engine);
    fill(
      modelSelect,
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
    fillListed("codex-voice", voices.map((voice) => ({ value: voice, label: voice + (voice === capabilities.default_voice ? " (default)" : "") })));
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
  function fieldMessage(id, text, isError) {
    const node = document.querySelector(`.field-message[data-for="${id}"]`);
    if (!node) return;
    node.textContent = text;
    node.classList.toggle("error", Boolean(isError));
  }
  async function saveControl(control) {
    try {
      await invoke("save_settings", { patch: patchFor(control.dataset.path, controlValue(control)) });
      fieldMessage(control.id, "Saved.", false);
    } catch (error) {
      fieldMessage(control.id, String(error), true);
    }
    await refresh();
  }
  document.querySelectorAll("button.save").forEach((button) => {
    button.addEventListener("click", () => saveControl($(button.dataset.for)));
  });
  document.querySelectorAll("[data-kind='bool']").forEach((control) => {
    control.addEventListener("change", () => saveControl(control));
  });

  function wakeText(wake) {
    const state = wake.state || {};
    let text;
    switch (state.kind) {
      case "off": text = "off (session mode is not wake word)"; break;
      case "no_phrases": text = "no phrases configured"; break;
      case "downloading": text = `downloading the ${state.label}: ${state.percent}%`; break;
      case "loading": text = `loading ${wake.model}`; break;
      case "ready": text = `ready (${wake.model})`; break;
      case "error": text = `error: ${state.message}`; break;
      default: text = "unknown";
    }
    const extras = [];
    if ((wake.unknown_words || []).length) extras.push(`not in the model vocabulary: ${wake.unknown_words.join(", ")}`);
    if (wake.dropped_chunks) extras.push(`${wake.dropped_chunks} audio chunks dropped`);
    return extras.length ? `${text}; ${extras.join("; ")}` : text;
  }

  // Chat log: entries arrive as events; the full list is pulled at start.
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
  function chatLineText(entry) {
    const who = entry.who || (entry.kind === "user" ? "user" : entry.kind);
    return `${entry.at} [${entry.guild_id}] ${who}: ${entry.text}`;
  }
  function renderChat() {
    chatPane.innerHTML = "";
    for (const entry of chatEntries) {
      if (!chatVisible(entry)) continue;
      const line = document.createElement("div");
      line.className = `chat-line ${entry.kind}`;
      const at = document.createElement("span");
      at.className = "at";
      at.textContent = entry.at;
      const who = document.createElement("span");
      who.className = "who";
      who.textContent = entry.who || (entry.kind === "user" ? "user" : entry.kind);
      const text = document.createElement("span");
      text.className = "text";
      text.textContent = entry.text;
      line.append(at, who, text);
      chatPane.appendChild(line);
    }
    if ($("chat-autoscroll").checked) chatPane.scrollTop = chatPane.scrollHeight;
  }
  function addChatEntry(entry) {
    chatEntries.push(entry);
    if (chatEntries.length > 600) chatEntries = chatEntries.slice(chatEntries.length - 600);
    renderChat();
  }
  document.querySelectorAll("#tab-chat input[type='checkbox']").forEach((box) => {
    box.addEventListener("change", renderChat);
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

  function assistantText(assistant) {
    const parts = [assistant.summary || "unknown"];
    if (assistant.version) parts.push(`Codex ${assistant.version}`);
    if (assistant.model_in_use != null) {
      parts.push(`thread: ${assistant.model_in_use || "default model"}, reasoning ${assistant.reasoning_effort_in_use || "default"}`);
    }
    for (const call of assistant.calls || []) {
      parts.push(`call ${call.guild_id}: ${call.state}${call.connected ? ", connected" : ""}${call.last_error ? `, error: ${call.last_error}` : ""}`);
    }
    return parts.join("; ");
  }

  $("codex-sign-in").addEventListener("click", async () => {
    try {
      await invoke("codex_sign_in");
      appendLog("sign-in page opened in the browser; finish there, then press Check Codex");
    } catch (error) {
      appendLog(`sign-in failed: ${error}`);
    }
  });
  $("codex-check").addEventListener("click", async () => {
    try {
      const account = await invoke("codex_check");
      appendLog(account.signed_in ? `Codex: signed in (${account.kind || "account"})` : "Codex: not signed in");
    } catch (error) {
      appendLog(`Codex check failed: ${error}`);
    }
    await refresh();
  });

  function renderStatus(status) {
    const discord = status.discord || {};
    $("status-assistant").textContent = assistantText(status.assistant || {});
    let discordText = discord.state || "unknown";
    if (discord.state === "connected") discordText = `connected as ${discord.user_name} (${discord.guild_count} servers)`;
    if (discord.state === "error") discordText = `error: ${discord.last_error || ""}`;
    $("status-discord").textContent = discordText;
    const channels = Object.values(discord.voice_channels || {});
    $("status-voice").textContent = channels.length
      ? channels.map((c) => `${c.guild_name} / ${c.channel_name}${c.participants.length ? ` (${c.participants.join(", ")})` : ""}`).join("; ")
      : "none";
    $("status-mute").textContent = status.muted ? "muted" : "not muted";
    $("mute-toggle").textContent = status.muted ? "Unmute" : "Mute";
    const gates = Object.values(status.gates || {});
    $("status-gates").textContent = gates.length
      ? gates.map((g) => {
          const parts = [g.mode === "always" ? "always on" : g.active ? "session active" : "waiting for the wake word"];
          if (g.muted) parts.push("muted");
          if (g.session_wanted) parts.push(g.assistant_ready ? "assistant ready" : "assistant connecting");
          if (g.last_wake_phrase) parts.push(`last phrase “${g.last_wake_phrase}”`);
          if (g.replay_held) parts.push(`${g.replay_held} frames held`);
          return `${g.guild_id}: ${parts.join(", ")}`;
        }).join("; ")
      : "none";
    const hotkey = status.hotkey || {};
    $("status-hotkey").textContent = !hotkey.shortcut
      ? "none configured"
      : hotkey.registered
        ? `${hotkey.shortcut} (registered)`
        : `${hotkey.shortcut}: not registered${hotkey.error ? `: ${hotkey.error}` : ""}`;
    $("status-data-dir").textContent = status.data_dir;
    $("status-wake").textContent = wakeText(status.wake || {});
    const tokenSet = Boolean(status.secrets?.discord_token?.is_set);
    $("status-token").textContent = tokenSet ? "set" : "not set";
    const badge = $("token-badge");
    badge.textContent = tokenSet ? "set" : "not set";
    badge.classList.toggle("ok", tokenSet);
    $("discord-connect").disabled = !tokenSet || discord.state === "connected" || discord.state === "connecting";
    $("discord-disconnect").disabled = discord.state === "disconnected";

    const errors = Object.entries(status.connection_errors || {});
    const list = $("status-errors");
    list.innerHTML = "";
    if (errors.length === 0) {
      const li = document.createElement("li");
      li.className = "muted-text";
      li.textContent = "none";
      list.appendChild(li);
    } else {
      for (const [source, message] of errors) {
        const li = document.createElement("li");
        li.textContent = `${source}: ${message}`;
        list.appendChild(li);
      }
    }
    const counters = { voice: discord.counters || {} };
    for (const [guild, channel] of Object.entries(discord.voice_channels || {})) {
      counters[`output ${guild}`] = channel.output;
    }
    $("status-counters").textContent = JSON.stringify(counters, null, 2);

    applyWakeLists(status.settings);
    document.querySelectorAll("[data-path]").forEach((control) => {
      setControlValue(control, readByPath(status.settings, control.dataset.path));
    });
    const unknown = (status.wake || {}).unknown_words || [];
    $("unknown-words").textContent = unknown.length
      ? `Not in the model vocabulary (these phrases are ignored): ${unknown.join(", ")}`
      : "";
  }

  async function refresh() {
    if (!invoke) return;
    try {
      renderStatus(await invoke("status"));
    } catch (error) {
      appendLog(`status failed: ${error}`);
    }
  }

  function showMessage(text, isError) {
    fieldMessage("token-input", text, isError);
  }

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

  $("token-set").addEventListener("click", async () => {
    const input = $("token-input");
    try {
      await invoke("set_secret", { name: "discord_token", value: input.value });
      input.value = "";
      showMessage("Token stored; connecting.", false);
    } catch (error) {
      showMessage(`Token not stored: ${error}`, true);
    }
    await refresh();
  });

  $("token-clear").addEventListener("click", async () => {
    try {
      await invoke("clear_secret", { name: "discord_token" });
      showMessage("Token cleared; disconnected.", false);
    } catch (error) {
      showMessage(`Token not cleared: ${error}`, true);
    }
    await refresh();
  });

  $("log-clear").addEventListener("click", () => {
    logPane.textContent = "";
  });

  async function start() {
    if (!invoke) {
      appendLog("Tauri API is not available; is withGlobalTauri enabled?");
      return;
    }
    try {
      const info = await invoke("app_info");
      $("build-stamp").textContent = `build ${info.build_stamp}`;
    } catch (error) {
      appendLog(`app_info failed: ${error}`);
    }
    if (listen) {
      await listen("app-event", ({ payload }) => {
        switch (payload.kind) {
          case "log":
            appendLog(payload.line);
            break;
          case "mute_changed":
            appendLog(payload.muted ? "muted" : "unmuted");
            refresh();
            break;
          case "assets":
            if (payload.error) appendLog(`${payload.label}: download failed: ${payload.error}`);
            else appendLog(`${payload.label}: ${payload.percent}%`);
            refresh();
            break;
          case "chat":
            addChatEntry(payload);
            break;
          case "assistant":
            refresh();
            loadLists();
            break;
          case "discord":
          case "wake":
          case "gate":
          case "hotkey":
          case "settings_changed":
          case "secret_changed":
          case "connection_error":
            refresh();
            break;
          default:
            appendLog(JSON.stringify(payload));
        }
      });
    }
    await loadLists();
    await refresh();
    await loadChat();
    setInterval(refresh, 5000);
  }

  start();
})();
