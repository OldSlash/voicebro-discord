// VoiceBro UI shell. Talks to Rust through window.__TAURI__ (withGlobalTauri).
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

  function renderStatus(status) {
    $("status-mute").textContent = status.muted ? "muted" : "not muted";
    $("mute-toggle").textContent = status.muted ? "Unmute" : "Mute";
    $("status-data-dir").textContent = status.data_dir;
    const tokenSet = Boolean(status.secrets?.discord_token?.is_set);
    $("status-token").textContent = tokenSet ? "set" : "not set";
    const badge = $("token-badge");
    badge.textContent = tokenSet ? "set" : "not set";
    badge.classList.toggle("ok", tokenSet);
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
    if (document.activeElement !== $("idle-timeout")) {
      $("idle-timeout").value = status.settings.session.idle_timeout_seconds;
    }
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
    const message = $("settings-message");
    message.textContent = text;
    message.classList.toggle("error", Boolean(isError));
  }

  $("mute-toggle").addEventListener("click", async () => {
    const status = await invoke("status");
    await invoke("set_muted", { muted: !status.muted });
    await refresh();
  });

  $("token-set").addEventListener("click", async () => {
    const input = $("token-input");
    try {
      await invoke("set_secret", { name: "discord_token", value: input.value });
      input.value = "";
      showMessage("Token stored.", false);
    } catch (error) {
      showMessage(`Token not stored: ${error}`, true);
    }
    await refresh();
  });

  $("token-clear").addEventListener("click", async () => {
    try {
      await invoke("clear_secret", { name: "discord_token" });
      showMessage("Token cleared.", false);
    } catch (error) {
      showMessage(`Token not cleared: ${error}`, true);
    }
    await refresh();
  });

  $("idle-save").addEventListener("click", async () => {
    const value = Number($("idle-timeout").value);
    try {
      await invoke("save_settings", { patch: { session: { idle_timeout_seconds: value } } });
      showMessage("Saved.", false);
    } catch (error) {
      showMessage(String(error), true);
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
  }

  start();
})();
