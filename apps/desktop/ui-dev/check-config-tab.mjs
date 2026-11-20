// Runs every scenario of the Settings and Config tabs (change `config-tab-and-codex-lists`)
// against the command mock in a headless Edge and stops at the first failure with its name
// (exit code 1): which cards each tab holds, the two ways of saving on the Config tab, the
// collapsible cards of both tabs, the switches and the Codex lists. Each scenario loads the page afresh.
// Needs Node 24 (built-in WebSocket) and Microsoft Edge; writes its browser profile under
// target\. Local only; never shipped.
//
//   node apps/desktop/ui-dev/check-config-tab.mjs
import { spawn } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..", "..");
const PAGE_PORT = 5182;
const DEBUG_PORT = 9232;
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
const profile = join(repo, "target", "ui-check-config-profile");
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
// Uncaught exceptions and console errors of the page; any of them fails the scenario.
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
// Evaluates an async function body in the page with helpers in scope; returns its value.
async function page(body) {
  const helpers = `
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    const $ = (id) => document.getElementById(id);
    const until = async (check, ms = 8000) => { const t = Date.now(); while (Date.now() - t < ms) { if (check()) return true; await sleep(100); } return false; };
    const calls = (name) => window.__mock.calls.filter((c) => c.name === name);
    const stored = () => window.__mock.settings();
    const tab = (name) => { document.querySelector('[data-tab="' + name + '"]').click(); };
    const titles = (name) => [...document.querySelector('[data-panel="' + name + '"]').querySelectorAll(".card")].map((card) => card.querySelector("h2").textContent.trim());
    const card = (id) => document.querySelector('[data-card="' + id + '"]');
    const collapsed = (id) => card(id).querySelector(".card-body").hidden && card(id).querySelector(".card-toggle").getAttribute("aria-expanded") === "false";
    const toggle = async (id) => { card(id).querySelector(".card-toggle").click(); await sleep(250); };
    const box = (node) => { const r = node.getBoundingClientRect(); return { top: r.top, mid: (r.top + r.bottom) / 2, width: r.width, height: r.height }; };
    const setValue = async (id, value, events = ["input", "change"]) => { const c = $(id); c.value = value; for (const e of events) c.dispatchEvent(new Event(e, { bubbles: true })); await sleep(300); };
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
  // The window's own start: settings, model lists, first status.
  await sleep(1200);
}
async function press(key) {
  const keys = {
    Enter: { key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" },
    Space: { key: " ", code: "Space", windowsVirtualKeyCode: 32, text: " " },
  }[key];
  await send("Input.dispatchKeyEvent", { type: "keyDown", ...keys });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: keys.key, code: keys.code, windowsVirtualKeyCode: keys.windowsVirtualKeyCode });
  await sleep(350);
}
function expect(condition, message) {
  if (!condition) throw new Error(message);
}
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
// Change `whisper-config-layout`: Local voice after Assistant, Whisper after Diagnostics.
const CARDS = ["discord", "access", "audio", "assistant", "local-voice", "codex", "hotkey"];
const SETTINGS_CARDS = ["appearance", "diagnostics", "whisper"];
const SWITCHES = [
  ["config", "earcons"], ["config", "one-speaker"], ["config", "codex-refresh-lists"], ["config", "save-chat-history"],
  ["settings", "diagnostics-performance"], ["settings", "diagnostics-memory"], ["settings", "diagnostics-verbose-log"],
  ["performance", "performance-monitoring"],
];

const scenarios = [
  // ---- Layout
  ["the Settings tab holds Appearance, Diagnostics, Whisper and Local voice, the Config tab the bot's six cards", async () => {
    await load();
    const r = await page(`
      const path = $("config-path");
      return { settings: titles("settings"), config: titles("config"), preview: Boolean($("config-preview")),
        pathText: path.textContent, pathInCard: Boolean(path.closest(".card")), pathTab: path.closest(".tab-panel").dataset.panel,
        pre: document.querySelectorAll('[data-panel="config"] pre').length };`);
    expect(same(r.settings, ["Appearance", "Diagnostics", "Whisper", "Local voice"]), `Settings: ${r.settings}`);
    expect(same(r.config, ["Discord and Codex", "Access", "Audio", "Assistant", "Codex", "Hotkey"]), `Config: ${r.config}`);
    expect(!r.preview && r.pre === 0, "the settings file's contents are still on the tab");
    expect(r.pathText === "C:\\VoiceBro\\settings.json", `path ${r.pathText}`);
    expect(!r.pathInCard && r.pathTab === "config", "the path is inside a card or not on the Config tab");
  }],
  ["each tab's feedback line names the saves of its own tab", async () => {
    await load();
    const r = await page(`
      const settingsLine = $("settings-feedback"); const configLine = $("config-autosave-feedback");
      const configBefore = configLine.textContent;
      tab("settings"); await setValue("setting-font-size", "15", ["input", "change"]);
      const afterFont = { settings: settingsLine.textContent, config: configLine.textContent, stored: stored().ui.font_size };
      tab("config"); await setValue("hotkey-mute", "Ctrl+Alt+K", ["input", "change"]);
      const afterHotkey = { settings: settingsLine.textContent, config: configLine.textContent, stored: stored().hotkey.mute };
      return { configBefore, afterFont, afterHotkey };`);
    expect(r.afterFont.stored === 15, `font size stored ${r.afterFont.stored}`);
    expect(r.afterFont.settings === "Saved: Font size", `Settings line: ${r.afterFont.settings}`);
    expect(r.afterFont.config === r.configBefore, `the Config line changed: ${r.afterFont.config}`);
    expect(r.afterHotkey.stored === "Ctrl+Alt+K", `hotkey stored ${r.afterHotkey.stored}`);
    expect(r.afterHotkey.config === "Saved: Mute hotkey", `Config line: ${r.afterHotkey.config}`);
    expect(r.afterHotkey.settings !== "Saved: Mute hotkey", "the Settings line named a Config save");
  }],
  ["two ways of saving on the Config tab: a gain at once, the guild id waits for Save changes", async () => {
    await load();
    const r = await page(`
      tab("config");
      await setValue("config-guild-id", "123", ["input"]);
      await setValue("output-gain", "150", ["input", "change"]);
      return { gain: stored().audio.output_gain, guild: stored().discord.guild_id, field: $("config-guild-id").value,
        dirty: $("config-guild-id").closest("label").classList.contains("is-dirty"), save: !$("config-save").disabled };`);
    expect(r.gain === 1.5, `the gain was not saved at once: ${r.gain}`);
    expect(r.guild === "1091401169441656933", `the guild id was saved without Save changes: ${r.guild}`);
    expect(r.field === "123" && r.dirty && r.save, `the unsaved guild id: ${JSON.stringify(r)}`);
  }],

  // ---- Collapsible cards
  ["first start: every card of the Settings and Config tabs is expanded", async () => {
    await load();
    const r = await page(`return ${JSON.stringify([...SETTINGS_CARDS, ...CARDS])}.map((id) => [id, Boolean(card(id)), collapsed(id)]);`);
    expect(r.every(([, found, folded]) => found && !folded), `at start: ${JSON.stringify(r)}`);
  }],
  ["a Settings card folds, is saved, comes back folded after a restart, and its switches still work", async () => {
    await load();
    const r = await page(`
      tab("settings");
      const below = card("diagnostics");
      const topBefore = box(below).top;
      await toggle("appearance");
      return { folded: collapsed("appearance"), moved: box(below).top < topBefore - 50, stored: stored().ui.collapsed_cards,
        feedbackOutside: !$("settings-feedback").closest(".card") };`);
    expect(r.folded && r.moved, `Appearance did not fold: ${JSON.stringify(r)}`);
    expect(same(r.stored, ["appearance"]), `stored ${JSON.stringify(r.stored)}`);
    expect(r.feedbackOutside, "the Settings feedback line is inside a card");
    await load(`?collapsed=${r.stored.join(",")}`);
    const again = await page(`
      const folded = [collapsed("appearance"), collapsed("diagnostics")];
      tab("settings"); await toggle("diagnostics"); await toggle("diagnostics");
      $("diagnostics-memory-label").click(); await sleep(400);
      return { folded, memory: stored().diagnostics.memory, list: stored().ui.collapsed_cards };`);
    expect(same(again.folded, [true, false]), `after a restart: ${again.folded}`);
    expect(again.memory === true, "a Diagnostics switch did not toggle after the card was expanded again");
    expect(same(again.list, ["appearance"]), `stored ${JSON.stringify(again.list)}`);
  }],
  ["a card collapses and expands from its title, and the state is saved", async () => {
    await load();
    const r = await page(`
      tab("config");
      const below = $("card-assistant-body").closest(".card");
      const topBefore = box(below).top;
      await toggle("audio");
      const folded = { collapsed: collapsed("audio"), stored: stored().ui.collapsed_cards, moved: box(below).top < topBefore - 50,
        saves: calls("save_settings").length };
      await toggle("audio");
      return { folded, open: !collapsed("audio"), stored: stored().ui.collapsed_cards, back: Math.abs(box(below).top - topBefore) < 1 };`);
    expect(r.folded.collapsed, "the Audio card did not collapse");
    expect(same(r.folded.stored, ["audio"]), `stored ${JSON.stringify(r.folded.stored)}`);
    expect(r.folded.moved, "the cards below did not move up");
    expect(r.open && same(r.stored, []) && r.back, `expanding: ${JSON.stringify(r)}`);
  }],
  ["keyboard: Enter and Space on the focused title collapse and expand the card", async () => {
    await load();
    await page(`tab("config"); card("hotkey").querySelector(".card-toggle").focus(); return true;`);
    await press("Enter");
    const afterEnter = await page(`return collapsed("hotkey");`);
    await press("Space");
    const afterSpace = await page(`return { collapsed: collapsed("hotkey"), focus: document.activeElement === card("hotkey").querySelector(".card-toggle") };`);
    expect(afterEnter, "Enter did not collapse the Hotkey card");
    expect(!afterSpace.collapsed, "Space did not expand it again");
    expect(afterSpace.focus, "the title lost the focus");
  }],
  ["after a restart the collapsed cards are still collapsed", async () => {
    await load();
    const list = await page(`tab("config"); await toggle("assistant"); await toggle("hotkey"); return stored().ui.collapsed_cards;`);
    expect(same(list, ["assistant", "hotkey"]), `stored ${JSON.stringify(list)}`);
    await load(`?collapsed=${list.join(",")}`);
    const r = await page(`return ${JSON.stringify(CARDS)}.map((id) => [id, collapsed(id)]);`);
    expect(same(r, [["discord", false], ["access", false], ["audio", false], ["assistant", true], ["local-voice", false], ["codex", false], ["hotkey", true]]), JSON.stringify(r));
  }],
  ["an id this window does not know is kept", async () => {
    await load("?collapsed=audio,later-card");
    const r = await page(`
      const audio = collapsed("audio");
      tab("config"); await toggle("hotkey");
      return { audio, stored: stored().ui.collapsed_cards };`);
    expect(r.audio, "the stored Audio card is not collapsed");
    expect(same(r.stored, ["later-card", "audio", "hotkey"]), `stored ${JSON.stringify(r.stored)}`);
  }],
  ["an unsaved guild id survives collapsing and expanding its card", async () => {
    await load();
    const r = await page(`
      tab("config");
      await setValue("config-guild-id", "4242", ["input"]);
      await toggle("discord"); const folded = collapsed("discord"); await toggle("discord");
      return { folded, value: $("config-guild-id").value, dirty: $("config-guild-id").closest("label").classList.contains("is-dirty"),
        stored: stored().discord.guild_id };`);
    expect(r.folded, "the card did not collapse");
    expect(r.value === "4242" && r.dirty, `the edit was lost: ${JSON.stringify(r)}`);
    expect(r.stored === "1091401169441656933", "the edit was saved");
  }],
  ["the settings file's location shows with every card collapsed", async () => {
    await load();
    const r = await page(`
      tab("config");
      for (const id of ${JSON.stringify(CARDS)}) await toggle(id);
      const path = $("config-path");
      return { all: ${JSON.stringify(CARDS)}.every((id) => collapsed(id)), shown: path.offsetParent !== null && box(path).height > 0, text: path.textContent };`);
    expect(r.all, "not every card collapsed");
    expect(r.shown && r.text.endsWith("settings.json"), `the location: ${JSON.stringify(r)}`);
  }],
  ["Not now in the install dialog with the Assistant card collapsed", async () => {
    await load("?fresh&collapsed=assistant");
    const r = await page(`
      const dialog = $("wake-install");
      window.__mock.join(); await until(() => dialog.open, 3000);
      const opened = dialog.open;
      $("wake-install-secondary").click(); await sleep(300);
      const still = collapsed("assistant");
      tab("config"); await toggle("assistant");
      return { opened, closed: !dialog.open, still, notice: !$("wake-download").hidden };`);
    expect(r.opened && r.closed, `the dialog: ${JSON.stringify(r)}`);
    expect(r.still, "the Assistant card expanded by itself");
    expect(r.notice, "the download notice is not in the Assistant card");
  }],

  // ---- Switches
  ["every switch is at least 44 x 24 px at the default font size", async () => {
    await load();
    const r = await page(`
      const out = [];
      for (const [name, id] of ${JSON.stringify(SWITCHES)}) { tab(name); await sleep(50); const b = box($(id)); out.push([id, Math.round(b.width), Math.round(b.height), $(id).getAttribute("role")]); }
      return { sizes: out, font: getComputedStyle(document.body).fontSize };`);
    expect(r.font === "13px", `font size ${r.font}`);
    for (const [id, width, height, role] of r.sizes) {
      expect(width >= 44 && height >= 24, `${id} is ${width} x ${height}`);
      expect(role === "switch", `${id} has role ${role}`);
    }
  }],
  ["a click on the Off text turns the switch on and saves it", async () => {
    await load();
    const r = await page(`
      tab("settings");
      const before = { checked: $("diagnostics-memory").checked, text: $("diagnostics-memory-label").textContent };
      $("diagnostics-memory-label").click(); await sleep(400);
      return { before, checked: $("diagnostics-memory").checked, text: $("diagnostics-memory-label").textContent, stored: stored().diagnostics.memory };`);
    expect(!r.before.checked && r.before.text === "Off", JSON.stringify(r.before));
    expect(r.checked && r.text === "On" && r.stored === true, JSON.stringify(r));
  }],
  ["Space on the focused switch toggles and saves it, with a visible focus mark", async () => {
    await load();
    await page(`tab("settings"); $("diagnostics-verbose-log").focus(); return true;`);
    await press("Space");
    const r = await page(`
      const s = getComputedStyle($("diagnostics-verbose-log"));
      return { checked: $("diagnostics-verbose-log").checked, stored: stored().diagnostics.verbose_log, outline: s.outlineStyle, outlineWidth: s.outlineWidth };`);
    expect(r.checked && r.stored === true, `Space: ${JSON.stringify(r)}`);
    expect(r.outline === "solid" && r.outlineWidth === "2px", `focus mark: ${r.outline} ${r.outlineWidth}`);
  }],
  ["the Earcons switch and the Earcon volume slider share their name line and their control line", async () => {
    await load();
    const r = await page(`
      tab("config");
      const nameTop = (id) => { const label = $(id).closest("label"); const range = document.createRange(); range.selectNodeContents([...label.childNodes].find((n) => n.nodeType === 3 && n.textContent.trim())); return range.getBoundingClientRect().top; };
      return { names: [nameTop("earcons"), nameTop("earcon-volume")], controls: [box($("earcons")).mid, box($("earcon-volume")).mid] };`);
    expect(Math.abs(r.names[0] - r.names[1]) < 1, `names ${r.names}`);
    expect(Math.abs(r.controls[0] - r.controls[1]) <= 3, `controls ${r.controls}`);
  }],
  ["the Performance tab's switch and the Diagnostics card stay mirrored", async () => {
    await load();
    const r = await page(`
      tab("performance"); $("performance-monitoring").click(); await sleep(400);
      return { card: $("diagnostics-performance").checked, text: $("diagnostics-performance-label").textContent,
        own: $("performance-monitoring-label").textContent, stored: stored().diagnostics.performance };`);
    expect(r.card && r.text === "On" && r.own === "On" && r.stored === true, JSON.stringify(r));
  }],

  // ---- Codex lists
  ["the saved lists fill the Model, Reasoning and Voice lists at load", async () => {
    await load();
    const r = await page(`
      return { models: [...$("codex-model-select").options].map((o) => o.value), model: $("codex-model-select").value,
        modelText: $("codex-model").style.display, voices: [...$("codex-voice-select").options].map((o) => o.value),
        efforts: $("codex-effort-select").options.length, read: $("codex-lists-read").textContent, checks: calls("codex_check").length };`);
    expect(same(r.models, ["", "model-a", "model-b"]), `models ${r.models}`);
    expect(r.model === "" && r.modelText === "none", `the stored (empty) model is not selected in a list: ${JSON.stringify(r)}`);
    expect(same(r.voices, ["", "cove", "juniper", "maple"]), `voices ${r.voices}`);
    expect(r.efforts > 1, "no reasoning list");
    expect(r.read === "Read 2026-11-14 18:30", `read time: ${r.read}`);
    expect(r.checks === 0, "the window read Codex at load");
  }],
  ["never read: free text and \"Never read\"", async () => {
    await load("?nolists");
    const r = await page(`return { options: $("codex-model-select").options.length, text: $("codex-model").style.display, read: $("codex-lists-read").textContent };`);
    expect(r.options === 0 && r.text === "", `the model is not free text: ${JSON.stringify(r)}`);
    expect(r.read === "Never read", `read: ${r.read}`);
  }],
  ["Refresh lists: the button waits, then the new time and the new model, the choice kept", async () => {
    await load();
    const r = await page(`
      tab("config");
      await setValue("codex-model-select", "model-b", ["change"]);
      window.__mock.nextCheckAddsModel();
      $("codex-lists-refresh").click(); await sleep(60);
      const during = { disabled: $("codex-lists-refresh").disabled, text: $("codex-lists-read").textContent };
      await until(() => !$("codex-lists-refresh").disabled, 4000); await sleep(200);
      return { during, stored: stored().codex.model, models: [...$("codex-model-select").options].map((o) => o.value),
        selected: $("codex-model-select").value, read: $("codex-lists-read").textContent, error: $("codex-lists-read").classList.contains("is-error"),
        checks: calls("codex_check").length };`);
    expect(r.stored === "model-b", `the choice was not saved: ${r.stored}`);
    expect(r.during.disabled && r.during.text === "Reading…", `while reading: ${JSON.stringify(r.during)}`);
    expect(r.checks === 1, `${r.checks} checks`);
    expect(r.models.includes("model-c"), `the new model is missing: ${r.models}`);
    expect(r.selected === "model-b", `the choice changed to ${r.selected}`);
    expect(/^Read \d{4}-\d\d-\d\d \d\d:\d\d$/.test(r.read) && r.read !== "Read 2026-11-14 18:30" && !r.error, `read time: ${r.read}`);
  }],
  ["a failed refresh shows why and keeps the lists", async () => {
    await load();
    const r = await page(`
      tab("config");
      window.__mock.failNextCheck();
      $("codex-lists-refresh").click();
      await until(() => !$("codex-lists-refresh").disabled && $("codex-lists-read").textContent !== "Reading…", 4000); await sleep(200);
      return { read: $("codex-lists-read").textContent, error: $("codex-lists-read").classList.contains("is-error"),
        models: $("codex-model-select").options.length, enabled: !$("codex-lists-refresh").disabled };`);
    expect(r.read.startsWith("Not read: codex not found") && r.error, `the error: ${r.read}`);
    expect(r.models === 3 && r.enabled, `the lists or the button: ${JSON.stringify(r)}`);
  }],
  ["turning the switch on saves it and reads nothing from Codex", async () => {
    await load();
    const r = await page(`
      tab("config");
      const since = window.__mock.calls.length;
      $("codex-refresh-lists").click(); await sleep(500);
      const names = [...new Set(window.__mock.calls.slice(since).map((c) => c.name))];
      return { names, stored: stored().codex.refresh_lists_at_start, text: $("codex-refresh-lists-label").textContent, feedback: $("config-autosave-feedback").textContent };`);
    expect(r.stored === true && r.text === "On", JSON.stringify(r));
    expect(!r.names.some((name) => ["codex_check", "codex_models", "codex_sign_in"].includes(name)), `called ${r.names}`);
    expect(r.feedback === "Saved: Refresh the lists at start", `feedback: ${r.feedback}`);
  }],

  // ---- Post answers to Discord (change `post-assistant-answers`)
  ["the Codex card holds its fields in order, post answers and its channel after Save chat history", async () => {
    await load();
    const r = await page(`
      const body = $("card-codex-body");
      // Change \`claude-and-opencode-engines\`: the rows of the other engines are hidden.
      const fields = [...body.querySelectorAll("input, select, textarea, button")]
        .filter((node) => !node.classList.contains("listed") && !node.closest("[data-engine][hidden]"))
        .map((node) => node.dataset.path || node.id);
      return { fields, values: { post: $("post-answers").value, history: $("save-chat-history").checked } };`);
    expect(same(r.fields, [
      "codex.refresh_lists_at_start", "codex-lists-refresh", "codex.model", "codex.reasoning_effort", "codex.voice",
      "codex.handoff_mode", "assistant.speaker_labels", "assistant.save_chat_history", "assistant.post_answers",
      "answers-channel", "assistant.instructions",
    ]), `fields: ${r.fields}`);
    expect(r.values.post === "off" && r.values.history === false, `values: ${JSON.stringify(r.values)}`);
  }],
  ["first start: Post answers shows Off and no channel list", async () => {
    await load("?fresh");
    const r = await page(`
      tab("config");
      return { value: $("post-answers").value, options: [...$("post-answers").options].map((o) => o.textContent),
        hidden: $("answers-channel-field").hidden, shown: box($("answers-channel-field")).height > 0,
        help: $("post-answers").closest("label").querySelector(".field-help").textContent };`);
    expect(r.value === "off", `value ${r.value}`);
    expect(same(r.options, ["Off", "The voice channel's chat", "The channel of /join", "A chosen channel"]), `options ${r.options}`);
    expect(r.hidden && !r.shown, "the channel list shows");
    expect(["final answer", "View Channel", "Send Messages", "Embed Links"].every((word) => r.help.includes(word)), `help: ${r.help}`);
  }],
  ["the voice channel's chat is saved without a button and shown after a reload", async () => {
    await load();
    const r = await page(`
      tab("config");
      const saves = calls("save_settings").length;
      await setValue("post-answers", "voice_channel");
      return { stored: stored().assistant, saves: calls("save_settings").length - saves, last: calls("save_settings").at(-1).args.patch,
        hidden: $("answers-channel-field").hidden, feedback: $("config-autosave-feedback").textContent, saveButton: $("config-save").disabled };`);
    expect(r.stored.post_answers === "voice_channel" && r.saves === 1, JSON.stringify(r));
    expect(same(r.last, { assistant: { post_answers: "voice_channel" } }), `patch ${JSON.stringify(r.last)}`);
    expect(r.hidden, "the channel list shows");
    expect(r.feedback === "Saved: Post answers to Discord" && r.saveButton, `feedback ${r.feedback}`);
    await load("?post_answers=voice_channel");
    const again = await page(`return { value: $("post-answers").value, hidden: $("answers-channel-field").hidden };`);
    expect(again.value === "voice_channel" && again.hidden, `after a restart: ${JSON.stringify(again)}`);
  }],
  ["A chosen channel with nothing stored saves nothing and asks to pick a channel", async () => {
    await load();
    const r = await page(`
      tab("config");
      const saves = calls("save_settings").length;
      await setValue("post-answers", "channel");
      return { saves: calls("save_settings").length - saves, stored: stored().assistant.post_answers,
        shown: !$("answers-channel-field").hidden, pick: !$("answers-channel-pick").hidden ? $("answers-channel-pick").textContent : null,
        first: $("answers-channel").options[0].textContent, value: $("answers-channel").value,
        dirty: $("post-answers").closest("label").classList.contains("is-dirty") };`);
    expect(r.saves === 0 && r.stored === "off", `something was saved: ${JSON.stringify(r)}`);
    expect(r.shown && r.pick && r.pick.startsWith("Pick a channel"), `the card does not ask: ${JSON.stringify(r)}`);
    expect(r.first === "Pick a channel…" && r.value === "" && r.dirty, JSON.stringify(r));
  }],
  ["picking a channel saves both fields together", async () => {
    await load();
    const r = await page(`
      tab("config");
      await setValue("post-answers", "channel");
      const saves = calls("save_settings").length;
      await setValue("answers-channel", "1571000000000000001", ["change"]);
      const option = $("answers-channel").selectedOptions[0];
      return { saves: calls("save_settings").length - saves, last: calls("save_settings").at(-1).args.patch, stored: stored().assistant,
        text: option.textContent, group: option.parentElement.label, pick: !$("answers-channel-pick").hidden,
        dirty: $("post-answers").closest("label").classList.contains("is-dirty"), feedback: $("config-autosave-feedback").textContent };`);
    expect(r.saves === 1, `${r.saves} saves`);
    expect(same(r.last, { assistant: { post_answers: "channel", answers_channel: "1571000000000000001" } }), `patch ${JSON.stringify(r.last)}`);
    expect(r.stored.post_answers === "channel" && r.stored.answers_channel === "1571000000000000001", JSON.stringify(r.stored));
    expect(r.text === "#answers" && r.group === "Mock server", `shown as ${r.text} under ${r.group}`);
    expect(!r.pick && !r.dirty && r.feedback === "Saved: Answers channel", JSON.stringify(r));
  }],
  ["a stored channel makes A chosen channel save at once and is kept while off", async () => {
    await load("?answers_channel=1571000000000000001");
    const r = await page(`
      tab("config");
      await setValue("post-answers", "channel");
      const chosen = { stored: stored().assistant, last: calls("save_settings").at(-1).args.patch };
      await setValue("post-answers", "off");
      return { chosen, off: stored().assistant, hidden: $("answers-channel-field").hidden };`);
    expect(same(r.chosen.last, { assistant: { post_answers: "channel", answers_channel: "1571000000000000001" } }), JSON.stringify(r.chosen));
    expect(r.off.post_answers === "off" && r.off.answers_channel === "1571000000000000001" && r.hidden, JSON.stringify(r));
  }],
  ["a channel the bot may not post in is marked, and the list is read again when the tab opens", async () => {
    await load("?post_answers=channel&answers_channel=1571000000000000001");
    const r = await page(`
      const before = calls("discord_text_channels").length;
      tab("config"); await sleep(300);
      const groups = [...$("answers-channel").querySelectorAll("optgroup")].map((g) => [g.label, [...g.children].map((o) => o.textContent)]);
      return { reads: calls("discord_text_channels").length - before, groups };`);
    expect(r.reads === 1, `${r.reads} reads when the tab opened`);
    expect(same(r.groups, [["Mock server", ["#general", "#answers", "#rules (can't post)"]], ["Second server", ["#bot-spam"]]]), JSON.stringify(r.groups));
  }],
  ["disconnected: the stored id and the note, then the names once the bot connects", async () => {
    await load("?disconnected&post_answers=channel&answers_channel=1571000000000000002");
    const r = await page(`
      tab("config");
      const off = { options: [...$("answers-channel").options].map((o) => o.textContent), value: $("answers-channel").value,
        note: $("answers-channel-note").textContent, shown: !$("answers-channel-field").hidden };
      window.__mock.setConnected(true);
      await until(() => $("answers-channel").querySelectorAll("optgroup").length > 0, 4000);
      return { off, selected: $("answers-channel").selectedOptions[0].textContent, value: $("answers-channel").value, saves: calls("save_settings").length };`);
    expect(r.off.shown && same(r.off.options, ["Channel 1571000000000000002"]) && r.off.value === "1571000000000000002", JSON.stringify(r.off));
    expect(r.off.note === "The channel names appear once the bot is connected.", `note: ${r.off.note}`);
    expect(r.selected === "#rules (can't post)" && r.value === "1571000000000000002", `connected: ${JSON.stringify(r)}`);
    expect(r.saves === 0, "the connection saved something");
  }],

  // ---- Assistant: one speaker at a time (change `speaker-turns`)
  ["one speaker at a time is off at first, saves at once, and its help names both modes", async () => {
    await load("?fresh");
    const r = await page(`
      tab("config");
      const before = { on: $("one-speaker").checked, label: $("one-speaker-label").textContent };
      const help = $("one-speaker").closest("label").querySelector(".field-help").textContent;
      const saves = calls("save_settings").length;
      $("one-speaker-label").click(); await sleep(400);
      return { before, help, stored: stored().assistant.one_speaker_at_a_time, saves: calls("save_settings").length - saves,
        last: calls("save_settings").at(-1).args.patch, label: $("one-speaker-label").textContent,
        inAssistant: Boolean($("card-assistant-body").querySelector("#one-speaker")) };`);
    expect(!r.before.on && r.before.label === "Off", JSON.stringify(r.before));
    expect(["realtime voice mode", "one person at a time", "Whisper mode", "waits", "deactivation phrase and Mute", "Default: off"].every((part) => r.help.includes(part)), `help: ${r.help}`);
    expect(r.stored === true && r.saves === 1 && same(r.last, { assistant: { one_speaker_at_a_time: true } }) && r.label === "On" && r.inAssistant, JSON.stringify(r));
  }],

  // ---- Access: who the assistant hears (changes `owner-only-access` and `voice-consent`)
  ["the Access card holds the mode, the policy URL, the link and the people who agreed, with help lines", async () => {
    await load();
    const r = await page(`
      const body = $("card-access-body");
      const fields = [...body.querySelectorAll("input, select, textarea, button")].map((node) => node.dataset.path || node.id);
      return { fields, options: [...$("access-mode").options].map((o) => [o.value, o.textContent]),
        modeHelp: $("access-mode").closest("label").querySelector(".field-help").textContent,
        urlHelp: $("privacy-url").closest("label").querySelector(".field-help").textContent,
        linkHelp: $("owner-link").querySelector(".field-help:not(.warning-text):not(#owner-code-ended)").textContent,
        listName: $("consent-list").querySelector(".field-name").textContent };`);
    expect(same(r.fields, ["discord.access", "discord.privacy_url", "owner-link-start", "owner-link-cancel", "owner-unlink"]), `fields: ${r.fields}`);
    expect(same(r.options, [["everyone", "Everyone"], ["agreed", "People who agreed (/optin)"], ["owner", "Only my Discord account"]]), `options: ${JSON.stringify(r.options)}`);
    expect(["/optin", "/optout", "linked account", "/join", "hears the answers", "Applies at once", "Default: Everyone"].every((part) => r.modeHelp.includes(part)), `help: ${r.modeHelp}`);
    expect(["https://", "PRIVACY-POLICY-TEMPLATE", "/privacy", "refused", "Developer Portal"].every((part) => r.urlHelp.includes(part)), `help: ${r.urlHelp}`);
    expect(["one-time code", "10 minutes", "/link", "encrypted"].every((part) => r.linkHelp.includes(part)), `help: ${r.linkHelp}`);
    expect(r.listName === "People who agreed", r.listName);
  }],
  ["first start: Everyone, no policy URL, nothing linked, nobody agreed, and Link is offered", async () => {
    await load("?fresh");
    const r = await page(`
      tab("config");
      return { mode: $("access-mode").value, url: $("privacy-url").value, state: $("owner-state").textContent,
        link: !$("owner-link-start").hidden, linkText: $("owner-link-start").textContent, unlink: !$("owner-unlink").hidden,
        code: !$("owner-code").hidden, note: $("owner-note").textContent, consentNote: $("consent-note").textContent,
        people: $("consent-people").children.length, empty: !$("consent-empty").hidden, emptyText: $("consent-empty").textContent };`);
    expect(r.mode === "everyone" && r.url === "", `mode ${JSON.stringify(r)}`);
    expect(r.state === "not linked" && r.link && r.linkText === "Link my Discord account", JSON.stringify(r));
    expect(!r.unlink && !r.code && r.note === "" && r.consentNote === "", JSON.stringify(r));
    expect(r.people === 0 && r.empty && r.emptyText.startsWith("Nobody yet"), JSON.stringify(r));
  }],
  ["People who agreed without a policy URL saves at once and warns that /optin is refused", async () => {
    await load();
    const r = await page(`
      tab("config");
      const saves = calls("save_settings").length;
      $("access-mode").value = "agreed"; $("access-mode").dispatchEvent(new Event("change")); await sleep(400);
      return { stored: stored().discord.access, saves: calls("save_settings").length - saves, last: calls("save_settings").at(-1).args.patch,
        note: $("consent-note").textContent, ownerNote: $("owner-note").textContent };`);
    expect(r.stored === "agreed" && r.saves === 1 && same(r.last, { discord: { access: "agreed" } }), JSON.stringify(r));
    expect(r.note.startsWith("No privacy policy URL: /optin is refused") && r.ownerNote === "", JSON.stringify(r));
  }],
  ["a privacy policy URL without https:// is not saved and the field says so; a good one clears the warning", async () => {
    await load("?access=agreed");
    const r = await page(`
      tab("config");
      const before = $("consent-note").textContent;
      $("privacy-url").value = "example.org/privacy"; $("privacy-url").dispatchEvent(new Event("change")); await sleep(400);
      const bad = { stored: stored().discord.privacy_url, message: document.querySelector('[data-for="privacy-url"]').textContent };
      $("privacy-url").value = " https://example.org/privacy "; $("privacy-url").dispatchEvent(new Event("change")); await sleep(400);
      return { before, bad, stored: stored().discord.privacy_url, shown: $("privacy-url").value, note: $("consent-note").textContent };`);
    expect(r.before !== "", "the warning before a URL");
    expect(r.bad.stored === "" && r.bad.message.includes("must start with https://"), JSON.stringify(r.bad));
    expect(r.stored === "https://example.org/privacy" && r.shown === "https://example.org/privacy" && r.note === "", JSON.stringify(r));
  }],
  ["an /optin appears in the list within a second, and Remove deletes it without a message to the person", async () => {
    await load("?access=agreed&privacy=https://example.org/p&consents=Alex");
    const r = await page(`
      tab("config");
      await until(() => $("consent-people").children.length === 1, 2000);
      const sent = Date.now();
      const id = window.__mock.optin("Sam");
      const shown = await until(() => $("consent-people").children.length === 2, 1000);
      const ms = Date.now() - sent;
      const rows = [...$("consent-people").children].map((li) => li.querySelector(".consent-name").textContent);
      const row = [...$("consent-people").children].find((li) => li.dataset.id === id);
      const time = row.querySelector(".consent-time").textContent;
      row.querySelector("button").click();
      await until(() => $("consent-people").children.length === 1, 2000);
      return { shown, ms, rows, time, after: [...$("consent-people").children].map((li) => li.querySelector(".consent-name").textContent),
        removes: calls("consent_remove").map((c) => c.args), records: window.__mock.consent().records.length, empty: !$("consent-empty").hidden };`);
    expect(r.shown && r.ms <= 1000, `not shown within a second: ${JSON.stringify(r)}`);
    expect(same(r.rows, ["Alex", "Sam"]) && r.time.startsWith("agreed "), JSON.stringify(r));
    expect(same(r.after, ["Alex"]) && r.removes.length === 1 && r.records === 1 && !r.empty, JSON.stringify(r));
  }],
  ["an /optout leaves the list, and an unreadable records file is named", async () => {
    await load("?consents=Alex");
    const r = await page(`
      tab("config");
      await until(() => $("consent-people").children.length === 1, 2000);
      const id = $("consent-people").children[0].dataset.id;
      window.__mock.optout(id);
      await until(() => $("consent-people").children.length === 0, 2000);
      const empty = !$("consent-empty").hidden;
      window.__mock.consentUnreadable();
      await until(() => $("consent-unreadable").textContent !== "", 2000);
      return { empty, unreadable: $("consent-unreadable").textContent };`);
    expect(r.empty, "Nobody yet after the /optout");
    expect(r.unreadable.startsWith("The consent records could not be read"), r.unreadable);
  }],
  ["linked: the owner's name and Unlink, which removes the link", async () => {
    await load("?owner=linked");
    const r = await page(`
      tab("config");
      const before = { state: $("owner-state").textContent, unlink: !$("owner-unlink").hidden, linkText: $("owner-link-start").textContent,
        code: !$("owner-code").hidden };
      $("owner-unlink").click();
      await until(() => $("owner-state").textContent === "not linked", 2000);
      return { before, after: $("owner-state").textContent, unlink: !$("owner-unlink").hidden, calls: calls("owner_unlink").length };`);
    expect(r.before.state === "Alex (1091401169441656999)" && r.before.unlink && !r.before.code, JSON.stringify(r.before));
    expect(r.before.linkText === "Link another account", `link button: ${r.before.linkText}`);
    expect(r.after === "not linked" && !r.unlink && r.calls === 1, JSON.stringify(r));
  }],
  ["Only my Discord account saves at once and, with nothing linked, says the assistant hears nobody", async () => {
    await load();
    const r = await page(`
      tab("config");
      const saves = calls("save_settings").length;
      $("access-mode").value = "owner"; $("access-mode").dispatchEvent(new Event("change")); await sleep(400);
      return { stored: stored().discord.access, saves: calls("save_settings").length - saves, last: calls("save_settings").at(-1).args.patch,
        note: $("owner-note").textContent, feedback: $("config-autosave-feedback").textContent,
        saveButton: $("config-save").disabled };`);
    expect(r.stored === "owner" && r.saves === 1 && same(r.last, { discord: { access: "owner" } }), JSON.stringify(r));
    expect(r.note === "No account is linked: the assistant hears nobody until you link one.", JSON.stringify(r));
    expect(r.feedback === "Saved: Who the assistant hears" && r.saveButton, `feedback ${r.feedback}`);
  }],
  ["Link shows the code, the command and the countdown; Cancel ends it", async () => {
    await load();
    const r = await page(`
      tab("config");
      $("owner-link-start").click();
      await until(() => !$("owner-code").hidden, 3000);
      const waiting = { command: $("owner-command").textContent, countdown: $("owner-countdown").textContent,
        cancel: !$("owner-link-cancel").hidden, link: !$("owner-link-start").hidden, unlink: !$("owner-unlink").hidden };
      $("owner-link-cancel").click();
      await until(() => $("owner-code").hidden, 2000);
      return { waiting, ended: $("owner-code-ended").textContent, link: !$("owner-link-start").hidden, state: $("owner-state").textContent,
        starts: calls("owner_link_start").length, cancels: calls("owner_link_cancel").length };`);
    expect(r.waiting.command === "/link K7QF-2M9X", `command ${r.waiting.command}`);
    expect(/^(10:00|9:5\\d) left$/.test(r.waiting.countdown), `countdown ${r.waiting.countdown}`);
    expect(r.waiting.cancel && !r.waiting.link && !r.waiting.unlink, JSON.stringify(r.waiting));
    expect(r.ended === "The code was cancelled." && r.link && r.state === "not linked", JSON.stringify(r));
    expect(r.starts === 1 && r.cancels === 1, JSON.stringify(r));
  }],
  ["the link completes in the card within a second of the code arriving from Discord", async () => {
    await load();
    const r = await page(`
      tab("config");
      $("owner-link-start").click();
      await until(() => !$("owner-code").hidden, 3000);
      const sent = Date.now();
      window.__mock.completeLink("Sam");
      const shown = await until(() => $("owner-state").textContent.startsWith("Sam"), 1000);
      return { shown, ms: Date.now() - sent, code: !$("owner-code").hidden, unlink: !$("owner-unlink").hidden, ended: $("owner-code-ended").textContent };`);
    expect(r.shown && r.ms <= 1000, `not shown within a second: ${JSON.stringify(r)}`);
    expect(!r.code && r.unlink && r.ended === "", JSON.stringify(r));
  }],
  ["an expired code and five wrong codes each say why the code ended", async () => {
    await load();
    const r = await page(`
      tab("config");
      $("owner-link-start").click();
      await until(() => !$("owner-code").hidden, 3000);
      window.__mock.expireCode();
      await until(() => $("owner-code").hidden, 2000);
      const expired = $("owner-code-ended").textContent;
      $("owner-link-start").click();
      await until(() => !$("owner-code").hidden, 3000);
      for (let i = 0; i < 5; i += 1) window.__mock.wrongCode();
      await until(() => $("owner-code").hidden, 2000);
      return { expired, tries: $("owner-code-ended").textContent, state: $("owner-state").textContent };`);
    expect(r.expired === "The code expired.", `expired: ${r.expired}`);
    expect(r.tries === "The code was ended after 5 wrong tries." && r.state === "not linked", JSON.stringify(r));
  }],
  ["Codex not signed in: Link shows no code and says why", async () => {
    await load();
    const r = await page(`
      tab("config");
      window.__mock.setCodex({ signedIn: false });
      $("owner-link-start").click();
      await until(() => $("owner-message").textContent.includes("not signed in"), 3000);
      return { message: $("owner-message").textContent, code: !$("owner-code").hidden, link: !$("owner-link-start").hidden };`);
    expect(r.message.startsWith("Codex is not signed in") && !r.code && r.link, JSON.stringify(r));
  }],
  ["a collapsed Access card keeps its waiting code", async () => {
    await load();
    const r = await page(`
      tab("config");
      $("owner-link-start").click();
      await until(() => !$("owner-code").hidden, 3000);
      await toggle("access"); const folded = collapsed("access"); await toggle("access");
      return { folded, open: !collapsed("access"), command: $("owner-command").textContent, shown: box($("owner-command")).height > 0,
        countdown: $("owner-countdown").textContent };`);
    expect(r.folded && r.open, JSON.stringify(r));
    expect(r.command === "/link K7QF-2M9X" && r.shown && r.countdown.endsWith("left"), JSON.stringify(r));
  }],

  // ---- Phrase fields (change `voice-recognition-rework`)
  ["phrases of several words between commas are saved in order and shown joined with commas", async () => {
    await load();
    const r = await page(`
      tab("config");
      await setValue("deactivation-phrases", "Отбой братан, Заканчивай,  Братан всё ");
      await setValue("activation-phrases", "братан\\nбро");
      const model = stored().wake.phrases["vosk-model-small-ru-0.22"];
      return { deactivation: model.deactivation, activation: model.activation,
        shown: $("deactivation-phrases").value, shownActivation: $("activation-phrases").value };`);
    expect(same(r.deactivation, ["Отбой братан", "Заканчивай", "Братан всё"]), `stored ${JSON.stringify(r.deactivation)}`);
    expect(r.shown === "Отбой братан, Заканчивай, Братан всё", `shown ${r.shown}`);
    expect(same(r.activation, ["братан", "бро"]) && r.shownActivation === "братан, бро", "a line break also separates");
  }],
  ["a first start shows the English model's default phrases and saves nothing for them", async () => {
    await load("?fresh");
    const r = await page(`
      tab("config");
      $("activation-phrases").focus(); $("activation-phrases").blur(); await sleep(300);
      return { activation: $("activation-phrases").value, deactivation: $("deactivation-phrases").value,
        stored: stored().wake.phrases, saves: calls("save_settings").length };`);
    expect(r.activation === "bro" && r.deactivation === "peace out", JSON.stringify(r));
    expect(same(r.stored, {}) && r.saves === 0, `an unchanged field saved: ${JSON.stringify(r)}`);
  }],
  ["an emptied field saves an empty list and the default does not come back", async () => {
    await load("?fresh");
    const r = await page(`
      tab("config");
      await setValue("deactivation-phrases", "");
      window.__mock.emit("app-event", { event: "settings_changed" }); await sleep(600);
      const english = stored().wake.phrases["vosk-model-small-en-us-0.15"];
      return { english, field: $("deactivation-phrases").value, activation: $("activation-phrases").value };`);
    expect(same(r.english, { deactivation: [] }), `stored ${JSON.stringify(r.english)}`);
    expect(r.field === "" && r.activation === "bro", JSON.stringify(r));
  }],
  ["switching the model shows its phrases: defaults the first time, the stored ones after", async () => {
    await load("?fresh");
    const r = await page(`
      tab("config");
      window.__mock.setInstalled(["vosk-model-small-en-us-0.15", "vosk-model-small-ru-0.22"], true); await sleep(400);
      const pick = async (id) => { const c = $("wake-model"); c.value = id; c.dispatchEvent(new Event("change", { bubbles: true })); await sleep(500); };
      await pick("vosk-model-small-ru-0.22");
      const russian = [$("activation-phrases").value, $("deactivation-phrases").value, document.querySelector("[data-model-name]").textContent];
      await setValue("deactivation-phrases", "Отбой братан, Заканчивай");
      await pick("vosk-model-small-en-us-0.15");
      const english = [$("activation-phrases").value, $("deactivation-phrases").value];
      await pick("vosk-model-small-ru-0.22");
      const back = [$("activation-phrases").value, $("deactivation-phrases").value];
      return { russian, english, back };`);
    expect(same(r.russian.slice(0, 2), ["братан", "отбой"]) && r.russian[2].includes("Russian"), JSON.stringify(r.russian));
    expect(same(r.english, ["bro", "peace out"]), JSON.stringify(r.english));
    expect(same(r.back, ["братан", "Отбой братан, Заканчивай"]), JSON.stringify(r.back));
  }],
  ["the help under the phrase fields says commas, several words, the vocabulary and the defaults", async () => {
    await load();
    const r = await page(`
      const help = (id) => $(id).closest("label").querySelector(".field-help").textContent;
      return [help("activation-phrases"), help("deactivation-phrases")];`);
    for (const text of r) {
      for (const part of ["commas", "several words", "vocabulary", "default"]) {
        expect(text.includes(part), `«${part}» missing in: ${text}`);
      }
      expect(!text.includes("Empty by default"), text);
    }
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
console.log(failed ? "the Config tab check failed" : `all ${scenarios.length} Config tab scenarios passed`);
process.exit(failed ? 1 : 0);
