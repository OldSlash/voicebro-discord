// Runs the window's scenarios of change `claude-and-opencode-engines` against the command
// mock in a headless Edge and stops at the first failure with its name (exit code 1): the
// Engine list and the realtime mode only Codex has, the engine card, the Claude subscription
// notice and its dialog (no download dialog after it since change `models-in-settings`), the Status tab of each
// engine with its install offer and sign-in, the Chat tab's labels and the System tab's
// memory share. Each scenario loads the page afresh. Needs Node 24 (built-in WebSocket) and
// Microsoft Edge; writes its browser profile under target\. Local only; never shipped.
//
//   node apps/desktop/ui-dev/check-engine-config.mjs
import { spawn } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..", "..");
const PAGE_PORT = 5184;
const DEBUG_PORT = 9234;
const EDGE = [
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
].find((path) => existsSync(path));
if (!EDGE) {
  console.error("Microsoft Edge not found");
  process.exit(2);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function waitFor(check, what, ms = 15000) {
  const until = Date.now() + ms;
  for (;;) {
    try {
      const value = await check();
      if (value) return value;
    } catch {}
    if (Date.now() > until) throw new Error(`timed out waiting for ${what}`);
    await sleep(150);
  }
}

const server = spawn(process.execPath, [join(here, "serve.mjs")], {
  env: { ...process.env, PORT: String(PAGE_PORT) },
  stdio: "ignore",
});
const profile = join(repo, "target", "ui-check-engine-profile");
mkdirSync(profile, { recursive: true });
const edge = spawn(EDGE, [
  "--headless=new", `--remote-debugging-port=${DEBUG_PORT}`, `--user-data-dir=${profile}`,
  "--window-size=1280,800", "--no-first-run", "--disable-extensions", "about:blank",
], { stdio: "ignore" });
const stop = () => {
  edge.kill();
  server.kill();
};

let ws;
let nextId = 0;
const pending = new Map();
const pageErrors = [];
function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++nextId;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });
}
async function connect() {
  const pages = await waitFor(async () => {
    const list = await (await fetch(`http://127.0.0.1:${DEBUG_PORT}/json`)).json();
    return list.find((page) => page.type === "page") ? list : null;
  }, "the headless Edge");
  ws = new WebSocket(pages.find((page) => page.type === "page").webSocketDebuggerUrl);
  ws.onmessage = (message) => {
    const data = JSON.parse(message.data);
    if (data.method === "Runtime.exceptionThrown") {
      pageErrors.push(data.params.exceptionDetails.exception?.description || data.params.exceptionDetails.text);
    } else if (data.method === "Runtime.consoleAPICalled" && data.params.type === "error") {
      pageErrors.push(data.params.args.map((arg) => arg.value ?? arg.description).join(" "));
    }
    const waiter = pending.get(data.id);
    if (!waiter) return;
    pending.delete(data.id);
    if (data.error) waiter.reject(new Error(data.error.message));
    else waiter.resolve(data.result);
  };
  await new Promise((resolve) => (ws.onopen = resolve));
  await send("Page.enable");
  await send("Runtime.enable");
}
async function page(body) {
  const helpers = `
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    const $ = (id) => document.getElementById(id);
    const until = async (check, ms = 8000) => { const t = Date.now(); while (Date.now() - t < ms) { if (check()) return true; await sleep(100); } return false; };
    const calls = (name) => window.__mock.calls.filter((c) => c.name === name);
    const stored = () => window.__mock.settings();
    const tab = (name) => { document.querySelector('[data-tab="' + name + '"]').click(); };
    const visible = (node) => Boolean(node) && !node.closest("[hidden]") && node.getClientRects().length > 0;
    const shownRows = () => [...document.querySelectorAll("#card-codex-body label, #card-codex-body .field")].filter(visible).flatMap((row) => [...row.querySelectorAll("input, select, textarea")].map((control) => control.id)).filter(Boolean);
    const choose = async (engine) => { const s = $("assistant-engine"); s.value = engine; s.dispatchEvent(new Event("change", { bubbles: true })); await sleep(900); };
    const pickMode = async (mode) => { const s = $("assistant-mode"); s.value = mode; s.dispatchEvent(new Event("change", { bubbles: true })); await sleep(900); };
    const claudeNoticeShown = () => [...document.querySelectorAll("[data-claude-notice]")].some((box) => visible(box));
    const dialogOpen = () => $("claude-notice-dialog").open;
    const filesDialog = () => $("wake-install").open && $("wake-install").dataset.subject === "whisper";
  `;
  const result = await send("Runtime.evaluate", {
    expression: `(async () => { ${helpers} ${body} })()`,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  return result.result.value;
}
async function load(query = "") {
  await send("Page.navigate", { url: `http://127.0.0.1:${PAGE_PORT}/ui-dev/index.html${query}` });
  await waitFor(() => page(`return Boolean(window.__mock) && document.readyState === "complete";`), "the page");
  await sleep(1200);
}
function expect(condition, message) {
  if (!condition) throw new Error(message);
}

const scenarios = [
  // ---- settings-ui "Engine choice in the Assistant card"
  ["the Engine list offers the three engines; with Codex the card is Codex's and the voice mode is open", async () => {
    await load();
    const r = await page(`
      tab("config");
      return { options: [...$("assistant-engine").options].map((o) => o.value), title: $("engine-card-title").textContent,
        realtime: $("assistant-mode").querySelector('option[value="realtime"]').disabled, rows: shownRows(),
        help: $("assistant-engine").closest("label").querySelector(".field-help").textContent };`);
    expect(JSON.stringify(r.options) === JSON.stringify(["codex", "claude", "opencode"]), `options ${r.options}`);
    expect(r.title === "Codex", `title ${r.title}`);
    expect(!r.realtime, "the realtime voice mode is disabled with Codex");
    expect(r.rows.includes("codex-voice") && !r.rows.includes("claude-model"), `rows ${r.rows}`);
    expect(r.help.includes("only engine with the realtime voice mode"), r.help);
  }],
  ["Claude Code chosen in the realtime mode: one save with the Whisper mode, realtime stays selectable with the note", async () => {
    await load("?notice_shown");
    const r = await page(`
      tab("config");
      const before = calls("save_settings").length;
      await choose("claude");
      const saves = calls("save_settings").slice(before);
      return { saves: saves.map((c) => c.args.patch), stored: stored().assistant, recognition: $("recognition-engine").value,
        realtime: $("assistant-mode").querySelector('option[value="realtime"]').disabled,
        note: $("assistant-mode-note").textContent, title: $("engine-card-title").textContent, rows: shownRows() };`);
    expect(r.saves.length === 1 && r.saves[0].assistant.engine === "claude" && r.saves[0].assistant.mode === "whisper", JSON.stringify(r.saves));
    expect(r.stored.engine === "claude" && r.stored.mode === "whisper", JSON.stringify(r.stored));
    expect(r.recognition === "Whisper", r.recognition);
    expect(!r.realtime, "the realtime voice was disabled with Claude Code");
    expect(r.note === "The voice mode works only with Codex: choosing Realtime voice switches the engine to Codex.", r.note);
    expect(r.title === "Claude Code", r.title);
    for (const id of ["claude-executable", "claude-model", "speaker-labels", "save-chat-history", "post-answers", "codex-instructions"]) {
      expect(r.rows.includes(id) || (id === "claude-model" && r.rows.includes("claude-model-select")), `${id} missing: ${r.rows}`);
    }
    for (const id of ["codex-voice", "codex-handoff", "codex-effort", "codex-model", "codex-refresh-lists", "opencode-model"]) {
      expect(!r.rows.includes(id) && !r.rows.includes(`${id}-select`), `${id} shown with Claude Code`);
    }
  }],
  ["OpenCode chosen with the Whisper files missing: no download dialog, and Config → Assistant says what is not downloaded", async () => {
    await load("?notice_shown");
    const r = await page(`
      tab("config");
      await choose("opencode");
      await sleep(1500);
      return { files: filesDialog(), dialogs: document.querySelectorAll("dialog[open]").length, note: !$("whisper-missing").hidden && $("whisper-missing").offsetParent !== null,
        text: $("whisper-missing-text").textContent, title: $("engine-card-title").textContent, mode: stored().assistant.mode };`);
    expect(r.mode === "whisper", r.mode);
    expect(!r.files && r.dialogs === 0, "a download dialog opened at the engine choice");
    expect(r.note && r.text.startsWith("Not downloaded:"), `note: ${r.text}`);
    expect(r.title === "OpenCode", r.title);
  }],
  ["back to Codex: the mode stays Whisper and the voice mode can be chosen again", async () => {
    await load("?engine=claude&whisper&notice_shown");
    const r = await page(`
      tab("config");
      await choose("codex");
      return { stored: stored().assistant, realtime: $("assistant-mode").querySelector('option[value="realtime"]').disabled, note: $("assistant-mode-note").textContent };`);
    expect(r.stored.engine === "codex" && r.stored.mode === "whisper", JSON.stringify(r.stored));
    expect(!r.realtime && r.note === "", "realtime still closed");
  }],
  ["Claude Code and OpenCode in the text answers mode keep it; every mode stays selectable", async () => {
    await load("?text_answers&whisper&notice_shown");
    const r = await page(`
      tab("config");
      await choose("claude");
      const claude = { stored: { ...stored().assistant }, options: [...$("assistant-mode").options].map((o) => [o.value, o.disabled]) };
      await choose("opencode");
      return { claude, opencode: stored().assistant.mode };`);
    expect(r.claude.stored.engine === "claude" && r.claude.stored.mode === "text", JSON.stringify(r.claude.stored));
    expect(JSON.stringify(r.claude.options) === JSON.stringify([["realtime", false], ["whisper", false], ["text", false]]), JSON.stringify(r.claude.options));
    expect(r.opencode === "text", r.opencode);
  }],
  // ---- settings-ui "Realtime voice switches the engine to Codex" (change `realtime-switches-to-codex`)
  ["Realtime voice from Claude Code in the Whisper mode: one save with Codex, the Codex card and Vosk", async () => {
    await load("?engine=claude&whisper&notice_shown");
    const r = await page(`
      tab("config");
      const before = calls("save_settings").length;
      const option = $("assistant-mode").querySelector('option[value="realtime"]');
      const label = option.textContent, disabled = option.disabled;
      await pickMode("realtime");
      const saves = calls("save_settings").slice(before);
      return { label, disabled, saves: saves.map((c) => c.args.patch), stored: stored().assistant, engine: $("assistant-engine").value,
        title: $("engine-card-title").textContent, recognition: $("recognition-engine").value, wake: visible($("wake-model")),
        notice: claudeNoticeShown(), note: $("assistant-mode-note").textContent, rows: shownRows() };`);
    expect(r.label === "Realtime voice (Codex voice call, Vosk wake word)" && !r.disabled, `${r.label} disabled=${r.disabled}`);
    expect(r.saves.length === 1 && JSON.stringify(r.saves[0]) === JSON.stringify({ assistant: { mode: "realtime", engine: "codex" } }), JSON.stringify(r.saves));
    expect(r.stored.engine === "codex" && r.stored.mode === "realtime", JSON.stringify(r.stored));
    expect(r.engine === "codex" && r.title === "Codex", `${r.engine} ${r.title}`);
    expect(r.recognition === "Vosk", r.recognition);
    expect(r.wake, "the wake model row is hidden");
    expect(!r.notice && r.note === "", `notice ${r.notice}, note ${r.note}`);
    expect(r.rows.includes("codex-voice") && !r.rows.includes("claude-executable"), `rows ${r.rows}`);
  }],
  ["Realtime voice from OpenCode in the text answers mode keeps OpenCode's settings", async () => {
    await load("?engine=opencode&text_answers&whisper&notice_shown");
    const r = await page(`
      tab("config");
      await window.__TAURI__.core.invoke("save_settings", { patch: { opencode: { executable: "D:\\\\tools\\\\opencode.exe", model: "openai/gpt-5" } } });
      await sleep(300);
      const before = calls("save_settings").length;
      await pickMode("realtime");
      const saves = calls("save_settings").slice(before);
      return { saves: saves.map((c) => c.args.patch), stored: stored() };`);
    expect(r.saves.length === 1 && r.saves[0].assistant.engine === "codex" && r.saves[0].assistant.mode === "realtime" && !r.saves[0].opencode, JSON.stringify(r.saves));
    expect(r.stored.assistant.engine === "codex" && r.stored.assistant.mode === "realtime", JSON.stringify(r.stored.assistant));
    expect(r.stored.opencode.executable === "D:\\tools\\opencode.exe" && r.stored.opencode.model === "openai/gpt-5", JSON.stringify(r.stored.opencode));
  }],
  ["Claude Code again after the switch to the realtime voice: the Whisper mode and its card as set before", async () => {
    await load("?engine=claude&whisper&notice_shown");
    const r = await page(`
      tab("config");
      await window.__TAURI__.core.invoke("save_settings", { patch: { claude: { executable: "D:\\\\tools\\\\claude.exe", model: "sonnet" } } });
      await sleep(300);
      await pickMode("realtime");
      const before = calls("save_settings").length;
      await choose("claude");
      const saves = calls("save_settings").slice(before);
      return { saves: saves.map((c) => c.args.patch), stored: stored(), title: $("engine-card-title").textContent, executable: $("claude-executable").value };`);
    expect(r.saves.length === 1 && r.saves[0].assistant.engine === "claude" && r.saves[0].assistant.mode === "whisper", JSON.stringify(r.saves));
    expect(r.stored.claude.executable === "D:\\tools\\claude.exe" && r.stored.claude.model === "sonnet", JSON.stringify(r.stored.claude));
    expect(r.title === "Claude Code" && r.executable === "D:\\tools\\claude.exe", `${r.title} ${r.executable}`);
  }],
  ["Realtime voice with Codex already the engine saves the mode alone", async () => {
    await load("?whisper&notice_shown");
    const r = await page(`
      tab("config");
      const before = calls("save_settings").length;
      await pickMode("realtime");
      return { saves: calls("save_settings").slice(before).map((c) => c.args.patch), stored: stored().assistant };`);
    expect(r.saves.length === 1 && JSON.stringify(r.saves[0]) === JSON.stringify({ assistant: { mode: "realtime" } }), JSON.stringify(r.saves));
    expect(r.stored.engine === "codex" && r.stored.mode === "realtime", JSON.stringify(r.stored));
  }],
  ["Realtime voice from Claude Code with the save refused: Whisper and Claude Code stay, the reason shows", async () => {
    await load("?engine=claude&whisper&notice_shown");
    const r = await page(`
      tab("config");
      window.__mock.failNextSave("could not save the settings: the disk is full");
      await pickMode("realtime");
      return { shown: $("assistant-mode").value, stored: stored().assistant, engine: $("assistant-engine").value,
        message: document.querySelector('[data-for="assistant-mode"]').textContent, title: $("engine-card-title").textContent };`);
    expect(r.shown === "whisper", `the list shows ${r.shown}`);
    expect(r.stored.engine === "claude" && r.stored.mode === "whisper", JSON.stringify(r.stored));
    expect(r.engine === "claude" && r.title === "Claude Code", `${r.engine} ${r.title}`);
    expect(r.message.includes("the disk is full"), `message: ${r.message}`);
  }],
  // ---- discord-panel-controls "A mode button switches the assistant mode", scenario
  // "Unsaved edits in the window": a mode saved by the Discord panel, not by the window
  ["a mode changed from the Discord panel shows at once and keeps an unsaved edit of the Discord and Codex card", async () => {
    await load("?engine=claude&whisper&notice_shown");
    const r = await page(`
      tab("config");
      const field = $("config-guild-id");
      field.value = "123456789012345678"; field.dispatchEvent(new Event("input", { bubbles: true }));
      const before = calls("save_settings").length;
      window.__mock.saveFromOutside({ assistant: { mode: "text" } });
      await until(() => $("assistant-mode").value === "text", 4000);
      const text = { mode: $("assistant-mode").value, recognition: $("recognition-engine").value, engine: $("assistant-engine").value,
        title: $("engine-card-title").textContent };
      window.__mock.saveFromOutside({ assistant: { mode: "realtime", engine: "codex" } });
      await until(() => $("assistant-mode").value === "realtime", 4000);
      await sleep(300);
      return { text, realtime: { mode: $("assistant-mode").value, recognition: $("recognition-engine").value, engine: $("assistant-engine").value,
          title: $("engine-card-title").textContent, voice: shownRows().includes("codex-voice"), wake: visible($("wake-model")) },
        guild: field.value, feedback: $("config-feedback").textContent, saves: calls("save_settings").length - before,
        stored: stored().assistant, storedGuild: stored().discord.guild_id };`);
    expect(r.text.mode === "text" && r.text.recognition === "Whisper" && r.text.engine === "claude" && r.text.title === "Claude Code", `text: ${JSON.stringify(r.text)}`);
    expect(r.realtime.mode === "realtime" && r.realtime.recognition === "Vosk" && r.realtime.engine === "codex", `realtime: ${JSON.stringify(r.realtime)}`);
    expect(r.realtime.title === "Codex" && r.realtime.voice && r.realtime.wake, `the Codex card after the switch: ${JSON.stringify(r.realtime)}`);
    expect(r.guild === "123456789012345678" && r.feedback === "Unsaved changes.", `the unsaved edit: ${r.guild}, "${r.feedback}"`);
    expect(r.saves === 0, `the window saved ${r.saves} times`);
    expect(r.stored.mode === "realtime" && r.stored.engine === "codex" && r.storedGuild !== "123456789012345678", JSON.stringify(r.stored));
  }],
  // ---- settings-ui "The engine card follows the engine"
  ["an unsaved instruction edit survives a change of the engine", async () => {
    await load("?notice_shown&whisper");
    const r = await page(`
      tab("config");
      const field = $("codex-instructions");
      field.value = "Answer in one sentence."; field.dispatchEvent(new Event("input", { bubbles: true }));
      await choose("opencode");
      return { value: field.value, title: $("engine-card-title").textContent, path: field.dataset.path };`);
    expect(r.value === "Answer in one sentence.", `lost: ${r.value}`);
    expect(r.title === "OpenCode" && r.path === "assistant.instructions", JSON.stringify(r));
  }],
  // ---- settings-ui "Claude Code card" and "OpenCode card"
  ["the Claude Code card: the free entry before Check, the models after", async () => {
    await load("?engine=claude&whisper&notice_shown");
    const r = await page(`
      tab("config");
      const before = { input: visible($("claude-model")), placeholder: $("claude-model").placeholder, options: $("claude-model-select").options.length };
      $("claude-check").click();
      await until(() => $("claude-model-select").options.length > 0, 4000);
      return { before, after: [...$("claude-model-select").options].map((o) => o.textContent) };`);
    expect(r.before.input && r.before.placeholder === "Claude Code's default" && r.before.options === 0, JSON.stringify(r.before));
    expect(r.after[0] === "(Claude Code's default)" && r.after.some((o) => o.includes("haiku")), JSON.stringify(r.after));
  }],
  ["the OpenCode card lists provider/model and says where providers and searches go", async () => {
    await load("?engine=opencode&whisper&opencode_providers=openai");
    const r = await page(`
      tab("config");
      await until(() => $("opencode-model-select").options.length > 0, 4000);
      const help = $("opencode-model").closest("label").querySelector(".field-help").textContent;
      const note = [...document.querySelectorAll('#card-codex-body .form-note[data-engine="opencode"]')].map((n) => n.textContent).join(" ");
      return { options: [...$("opencode-model-select").options].map((o) => o.value), help, note };`);
    expect(r.options.includes("openai/gpt-5.4-mini"), JSON.stringify(r.options));
    expect(r.help.includes("opencode auth login"), r.help);
    expect(r.note.includes("web search service"), r.note);
  }],
  // ---- settings-ui "The Claude subscription notice in the window"
  ["the notice shows only in the Claude Code card, with both links, Hear only me and Hide, and goes with the owner-only mode", async () => {
    await load("?engine=claude&whisper&notice_shown");
    const r = await page(`
      tab("config");
      window.__mock.setClaudeSignIn("subscription");
      await sleep(600);
      const box = $("claude-notice-card");
      const notice = { shown: visible(box), card: box.closest(".card")?.dataset.card, title: $("engine-card-title").textContent,
        links: [...box.querySelectorAll("a")].map((a) => a.href), buttons: [...box.querySelectorAll("button")].map((b) => b.textContent),
        elsewhere: [...document.querySelectorAll("[data-claude-notice]")].filter((n) => n !== box).length,
        inAssistant: Boolean($("card-assistant-body").querySelector(".claude-notice")), inAccess: Boolean($("card-access-body").querySelector(".claude-notice")) };
      const s = $("access-mode"); s.value = "owner"; s.dispatchEvent(new Event("change", { bubbles: true }));
      await sleep(600);
      return { notice, after: visible(box) };`);
    expect(r.notice.shown && r.notice.card === "codex" && r.notice.title === "Claude Code", `notice: ${JSON.stringify(r.notice)}`);
    expect(r.notice.elsewhere === 0 && !r.notice.inAssistant && !r.notice.inAccess, `the notice is in another card too: ${JSON.stringify(r.notice)}`);
    expect(r.notice.links.includes("https://www.anthropic.com/legal/consumer-terms") && r.notice.links.includes("https://code.claude.com/docs/en/legal-and-compliance"), JSON.stringify(r.notice.links));
    expect(JSON.stringify(r.notice.buttons) === JSON.stringify(["Hear only me", "Hide"]), JSON.stringify(r.notice.buttons));
    expect(!r.after, "the notice stays with Only my Discord account");
  }],
  ["Hide: the notice goes for good, the flag is saved, the access mode stays, and neither a reload nor a switch to Codex and back brings it back", async () => {
    await load("?engine=claude&whisper&notice_shown");
    const r = await page(`
      tab("config");
      window.__mock.setClaudeSignIn("subscription"); await sleep(600);
      const before = visible($("claude-notice-card"));
      [...$("claude-notice-card").querySelectorAll("button")].find((b) => b.textContent === "Hide").click();
      await sleep(600);
      const after = { shown: visible($("claude-notice-card")), hidden: stored().claude.subscription_notice_hidden, access: stored().discord.access, links: calls("owner_link_start").length };
      await choose("codex"); await choose("claude"); await sleep(600);
      return { before, after, again: visible($("claude-notice-card")), dialog: dialogOpen() };`);
    expect(r.before, "no notice before Hide");
    expect(!r.after.shown && r.after.hidden === true && r.after.access === "everyone" && r.after.links === 0, JSON.stringify(r.after));
    expect(!r.again && !r.dialog, `back after a switch: ${JSON.stringify(r)}`);
    await load("?engine=claude&whisper&notice_shown&notice_hidden");
    const reload = await page(`tab("config"); window.__mock.setClaudeSignIn("subscription"); await sleep(600); return visible($("claude-notice-card"));`);
    expect(!reload, "the hidden notice came back after a restart");
  }],
  ["an API key or a cloud provider shows no notice", async () => {
    await load("?engine=claude&whisper&claude_signin=api_key");
    const r = await page(`
      tab("status"); $("claude-check").click(); await sleep(800);
      const key = visible($("claude-notice-card"));
      window.__mock.setClaudeSignIn("cloud"); await sleep(600);
      return { key, cloud: visible($("claude-notice-card")) };`);
    expect(!r.key && !r.cloud, JSON.stringify(r));
  }],
  // ---- settings-ui "The Claude subscription dialog"
  ["Hear only me with nobody linked: the owner-only mode and a link code in the Access card", async () => {
    await load("?whisper");
    const r = await page(`
      tab("config");
      await choose("claude");
      await until(dialogOpen, 3000);
      const focus = document.activeElement.id;
      const open = dialogOpen();
      $("claude-notice-owner").click();
      await until(() => !$("owner-code").hidden, 4000);
      return { open, focus, access: stored().discord.access, shown: stored().claude.subscription_notice_shown,
        code: $("owner-command").textContent, copy: !$("owner-copy").hidden, help: $("owner-copy-help").textContent,
        closed: !dialogOpen(), engine: stored().assistant.engine };`);
    expect(r.open && r.focus === "claude-notice-later", JSON.stringify(r));
    expect(r.access === "owner" && r.shown && r.closed && r.engine === "claude", JSON.stringify(r));
    expect(r.code === "K7QF-2M9X", r.code);
    expect(r.copy && r.help.includes("Press Copy"), `copy ${r.copy}, help ${r.help}`);
  }],
  ["Hear only me with an owner linked: the owner-only mode and no code", async () => {
    await load("?whisper&owner=linked");
    const r = await page(`
      tab("config");
      await choose("claude");
      await until(dialogOpen, 3000);
      $("claude-notice-owner").click();
      await sleep(900);
      return { access: stored().discord.access, code: !$("owner-code").hidden, links: calls("owner_link_start").length };`);
    expect(r.access === "owner" && !r.code && r.links === 0, JSON.stringify(r));
  }],
  ["Not now (or Escape) keeps the access mode and the notice, and is remembered", async () => {
    await load("?whisper");
    const r = await page(`
      tab("config");
      await choose("claude");
      await until(dialogOpen, 3000);
      $("claude-notice-dialog").dispatchEvent(new Event("cancel", { cancelable: true }));
      await sleep(700);
      return { closed: !dialogOpen(), access: stored().discord.access, shown: stored().claude.subscription_notice_shown, notice: visible($("claude-notice-card")) };`);
    expect(r.closed && r.access === "everyone" && r.shown && r.notice, JSON.stringify(r));
  }],
  ["asked once: Codex and then Claude Code again opens no dialog", async () => {
    await load("?whisper&notice_shown");
    const r = await page(`
      tab("config");
      await choose("claude"); await choose("codex"); await choose("claude");
      return { open: dialogOpen(), engine: stored().assistant.engine };`);
    expect(!r.open && r.engine === "claude", JSON.stringify(r));
  }],
  ["an API key sign-in opens no dialog", async () => {
    await load("?whisper&claude_signin=api_key");
    const r = await page(`
      tab("config");
      await choose("claude");
      await sleep(500);
      return { open: dialogOpen(), engine: stored().assistant.engine, shown: stored().claude.subscription_notice_shown };`);
    expect(!r.open && r.engine === "claude" && !r.shown, JSON.stringify(r));
  }],
  ["both dialogs: the subscription dialog opens, no download dialog follows it, and Config → Assistant says what is not downloaded", async () => {
    await load();
    const r = await page(`
      tab("config");
      await choose("claude");
      await until(dialogOpen, 3000);
      await sleep(1500);
      const first = { notice: dialogOpen(), files: filesDialog() };
      $("claude-notice-later").click();
      await sleep(2000);
      return { first, then: { notice: dialogOpen(), files: filesDialog(), note: !$("whisper-missing").hidden } };`);
    expect(r.first.notice && !r.first.files, `first ${JSON.stringify(r.first)}`);
    expect(!r.then.notice && !r.then.files && r.then.note, `then ${JSON.stringify(r.then)}`);
  }],
  // ---- status-view
  ["Status with Claude Code: its sign-in, its buttons, no Codex button and no call tile", async () => {
    await load("?engine=claude&whisper&notice_shown");
    const r = await page(`
      tab("status");
      $("claude-check").click();
      await until(() => $("status-codex").textContent.includes("Max"), 4000);
      return { engine: $("status-engine").textContent, label: $("status-account-label").textContent, value: $("status-codex").textContent,
        shown: ["codex-sign-in", "codex-check", "claude-sign-in", "claude-check", "opencode-check"].filter((id) => visible($(id))),
        call: visible($("status-call")), mute: $("mute-toggle").textContent };`);
    expect(r.engine.startsWith("Claude Code (2.1.318"), r.engine);
    expect(r.label === "Claude Code sign-in" && r.value === "Claude subscription (Max)", JSON.stringify(r));
    expect(JSON.stringify(r.shown) === JSON.stringify(["claude-sign-in", "claude-check"]), JSON.stringify(r.shown));
    expect(!r.call && r.mute === "Mute assistant", JSON.stringify(r));
  }],
  ["Claude Code missing: the winget offer, and Install runs it", async () => {
    await load("?engine=claude&whisper&claude_missing&notice_shown");
    const r = await page(`
      tab("status");
      await sleep(400);
      const text = $("claude-setup-text").textContent;
      const shown = visible($("claude-setup")) && visible($("claude-install"));
      $("claude-install").click();
      await until(() => !visible($("claude-setup")) || $("claude-setup-text").textContent.includes("not signed") || calls("claude_install").length && $("status-engine").textContent.includes("2.1.318"), 5000);
      return { text, shown, installs: calls("claude_install").length };`);
    expect(r.shown && r.text.includes("Anthropic.ClaudeCode") && r.text.includes("327 MB"), r.text);
    expect(r.installs === 1, `installs ${r.installs}`);
  }],
  ["Claude Code not signed in: the next step and Sign in, which runs Claude Code's own sign-in", async () => {
    await load("?engine=claude&whisper&claude_signin=none&notice_shown");
    const r = await page(`
      tab("status");
      $("claude-check").click();
      await until(() => visible($("claude-setup")), 4000);
      const text = $("claude-setup-text").textContent;
      $("claude-sign-in").click();
      await until(() => $("status-codex").textContent.includes("Max"), 5000);
      return { text, signIns: calls("claude_sign_in").length, value: $("status-codex").textContent, feedback: $("status-feedback").textContent };`);
    expect(r.text.includes("Next: Sign in to Claude Code"), r.text);
    expect(r.signIns === 1 && r.value === "Claude subscription (Max)", JSON.stringify(r));
  }],
  ["Status with OpenCode: no Sign in, the providers hint, and the install offer when missing", async () => {
    await load("?engine=opencode&whisper");
    const r = await page(`
      tab("status");
      await sleep(400);
      return { label: $("status-account-label").textContent, hint: $("opencode-setup-ways").textContent,
        shown: ["codex-sign-in", "claude-sign-in", "opencode-check"].filter((id) => visible($(id))) };`);
    expect(r.label === "OpenCode providers", r.label);
    expect(r.hint.includes("opencode auth login"), r.hint);
    expect(JSON.stringify(r.shown) === JSON.stringify(["opencode-check"]), JSON.stringify(r.shown));
    await load("?engine=opencode&whisper&opencode_missing");
    const missing = await page(`tab("status"); await sleep(400); return $("opencode-setup-text").textContent;`);
    expect(missing.includes("Anomaly.OpenCode"), missing);
  }],
  // ---- app-window "Tabs", assistant-chat-log "Chat tab"
  ["the Chat tab: its name, and Claude's entries labelled Claude", async () => {
    await load("?engine=claude&whisper&notice_shown");
    const r = await page(`
      window.__mock.emit("app-event", { event: "chat", id: 900, at: "12:00:05", guild_id: "1", kind: "tool", who: null, text: "Web search: London weather", engine: "Claude" });
      window.__mock.emit("app-event", { event: "chat", id: 901, at: "12:00:06", guild_id: "1", kind: "agent", who: null, text: "Sunny.", engine: "Claude" });
      await sleep(200);
      const rows = [...document.querySelectorAll("#chat-log .chat-entry")].slice(-2).map((row) => row.querySelector(".chat-who").textContent);
      return { tab: document.querySelector('[data-tab="chat"]').textContent, rows, filter: $("chat-filter-agent").nextElementSibling.textContent };`);
    expect(r.tab === "Chat", r.tab);
    expect(JSON.stringify(r.rows) === JSON.stringify(["Claude tool", "Claude"]), JSON.stringify(r.rows));
    expect(r.filter === "Agent work", r.filter);
  }],
  // ---- task 4.3: the System tab names the engine's share
  ["the System tab's memory names the engine in use", async () => {
    await load("?engine=claude&whisper&notice_shown");
    const r = await page(`
      await window.__TAURI__.core.invoke("save_settings", { patch: { diagnostics: { memory: true } } });
      tab("system");
      await until(() => $("memory-engine-name").textContent !== "Codex", 6000);
      return $("memory-engine-name").textContent;`);
    expect(r === "Claude Code", r);
  }],
];

let failed = false;
try {
  await connect();
  for (const [name, run] of scenarios) {
    try {
      pageErrors.length = 0;
      await run();
      expect(pageErrors.length === 0, `page errors: ${pageErrors.join("; ")}`);
      console.log(`ok   ${name}`);
    } catch (error) {
      console.log(`FAIL ${name}: ${error.message}`);
      failed = true;
      break;
    }
  }
} catch (error) {
  console.log(`FAIL setup: ${error.message}`);
  failed = true;
} finally {
  ws?.close();
  stop();
}
console.log(failed ? "the engine check failed" : `all ${scenarios.length} engine scenarios passed`);
process.exit(failed ? 1 : 0);
