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
      default: return control.value;
    }
  }
  function setControlValue(control, value) {
    if (document.activeElement === control) return;
    if (control.dataset.kind === "bool") control.checked = Boolean(value);
    else control.value = value == null ? "" : value;
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

  function renderStatus(status) {
    const discord = status.discord || {};
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
    $("status-data-dir").textContent = status.data_dir;
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

    document.querySelectorAll("[data-path]").forEach((control) => {
      setControlValue(control, readByPath(status.settings, control.dataset.path));
    });
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
          case "discord":
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
    await refresh();
    setInterval(refresh, 5000);
  }

  start();
})();
