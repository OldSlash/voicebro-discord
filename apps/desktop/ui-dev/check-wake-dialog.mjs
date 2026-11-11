// Runs every scenario of the wake install dialog against the command mock in a headless
// Edge and stops at the first failure with its name (exit code 1). Each scenario loads the
// page afresh. Needs Node 24 (built-in WebSocket) and Microsoft Edge; writes its browser
// profile under target\. Local only; never shipped.
//
//   node apps/desktop/ui-dev/check-wake-dialog.mjs
import { spawn } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..", "..");
const PAGE_PORT = 5181;
const DEBUG_PORT = 9231;
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
const profile = join(repo, "target", "ui-check-profile");
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
    const dialog = $("wake-install");
    const until = async (check, ms = 8000) => { const t = Date.now(); while (Date.now() - t < ms) { if (check()) return true; await sleep(100); } return false; };
    const items = () => [...document.querySelectorAll("#wake-install-items li")].map((li) => li.textContent);
    const choose = async (id, value) => { const s = $(id); s.value = value; s.dispatchEvent(new Event("input", { bubbles: true })); s.dispatchEvent(new Event("change", { bubbles: true })); await sleep(400); };
    const openDialogs = () => document.querySelectorAll("dialog[open]").length;
    const calls = (name) => window.__mock.calls.filter((c) => c.name === name);
  `;
  const result = await send("Runtime.evaluate", {
    expression: `(async () => { ${helpers} ${body} })()`,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  return result.result.value;
}
async function load(query) {
  await send("Page.navigate", { url: `http://127.0.0.1:${PAGE_PORT}/ui-dev/index.html${query}` });
  await waitFor(() => page(`return Boolean(window.__mock) && document.readyState === "complete";`), "the page");
  // The window's own start: settings, model list, first status.
  await sleep(1200);
}
async function pressEscape() {
  for (const type of ["keyDown", "keyUp"]) {
    await send("Input.dispatchKeyEvent", { type, key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  }
  await sleep(300);
}
async function pressEnter() {
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  await sleep(400);
}
function expect(condition, message) {
  if (!condition) throw new Error(message);
}

const scenarios = [
  ["start on a clean folder: the dialog lists the library and the English model", async () => {
    await load("?fresh");
    const r = await page(`return { open: dialog.open, items: items(), title: $("wake-install-title").textContent, text: $("wake-install-text").textContent, secondary: $("wake-install-secondary").textContent, downloads: calls("wake_download").length, dialogs: openDialogs() };`);
    expect(r.open, "the dialog is not open at start");
    expect(r.dialogs === 1, `${r.dialogs} dialogs open`);
    expect(r.title === "The wake word is not installed", r.title);
    expect(r.text.includes("Vosk small English (US) 0.15"), r.text);
    expect(JSON.stringify(r.items) === JSON.stringify(["Vosk library — 15 MB from github.com", "Vosk English model — 41 MB from alphacephei.com"]), JSON.stringify(r.items));
    expect(r.secondary === "Not now", r.secondary);
    expect(r.downloads === 0, "a download started without the OK");
  }],
  ["Not now: closes, downloads nothing, the notice stays", async () => {
    await load("?fresh");
    const r = await page(`$("wake-install-secondary").click(); await sleep(400); return { open: dialog.open, downloads: calls("wake_download").length, notice: !$("wake-download").hidden, state: window.__mock.wakeState().kind };`);
    expect(!r.open, "the dialog stayed open");
    expect(r.downloads === 0, "a download started");
    expect(r.notice, "the Assistant card's notice is gone");
    expect(r.state === "needs_download", r.state);
  }],
  ["Escape: the same as Not now", async () => {
    await load("?fresh");
    await pressEscape();
    const r = await page(`return { open: dialog.open, downloads: calls("wake_download").length };`);
    expect(!r.open, "Escape left the dialog open");
    expect(r.downloads === 0, "a download started");
  }],
  ["a stray Enter when the dialog comes up downloads nothing", async () => {
    await load("?fresh");
    const focused = await page(`return document.activeElement?.id;`);
    await pressEnter();
    const r = await page(`return { open: dialog.open, downloads: calls("wake_download").length };`);
    expect(focused === "wake-install-secondary", `the focus starts on ${focused}`);
    expect(r.downloads === 0, "Enter agreed to the download");
    expect(!r.open, "Enter on Not now left the dialog open");
  }],
  ["the bot joins with files missing: the dialog opens again", async () => {
    await load("?fresh");
    const r = await page(`$("wake-install-secondary").click(); await sleep(300); window.__mock.join(); await until(() => dialog.open, 3000); return { open: dialog.open, voice: $("footer-voice").textContent, secondary: $("wake-install-secondary").textContent };`);
    expect(r.open, "no dialog after the join");
    expect(r.voice === "1", `voice count ${r.voice}`);
    expect(r.secondary === "Not now", r.secondary);
  }],
  ["one dialog at most: a join while it is open opens no second one", async () => {
    await load("?fresh");
    const r = await page(`window.__mock.join(); await sleep(600); return { dialogs: openDialogs(), text: $("wake-install-text").textContent };`);
    expect(r.dialogs === 1, `${r.dialogs} dialogs open`);
  }],
  ["always listening: nothing at start, nothing at a join", async () => {
    await load("?fresh&always");
    const r = await page(`const atStart = dialog.open; window.__mock.join(); await sleep(700); return { atStart, afterJoin: dialog.open, state: window.__mock.wakeState().kind };`);
    expect(!r.atStart, "a dialog at start in always mode");
    expect(!r.afterJoin, "a dialog at a join in always mode");
    expect(r.state === "off", r.state);
  }],
  ["switched to wake word: the setting is saved and the dialog opens", async () => {
    await load("?fresh&always");
    const r = await page(`await choose("session-mode", "wake_word"); await until(() => dialog.open, 3000); return { open: dialog.open, mode: window.__mock.settings().session.mode, items: items().length };`);
    expect(r.mode === "wake_word", `mode ${r.mode}`);
    expect(r.open, "no dialog after the switch");
    expect(r.items === 2, `${r.items} items`);
  }],
  ["files on disk: no dialog at start", async () => {
    await load("");
    const r = await page(`return { open: dialog.open, state: window.__mock.wakeState().kind };`);
    expect(!r.open, "a dialog although the files are on disk");
    expect(r.state === "ready", r.state);
  }],
  ["Download: the progress in the dialog, closed when installed", async () => {
    await load("?fresh");
    const r = await page(`
      $("wake-install-primary").click();
      await until(() => !$("wake-install-progress").hidden && /Downloading/.test($("wake-install-progress").textContent), 4000);
      const during = { progress: $("wake-install-progress").textContent, secondary: $("wake-install-secondary").textContent, primaryHidden: $("wake-install-primary").hidden };
      const closed = await until(() => !dialog.open, 10000);
      await sleep(300);
      return { during, closed, state: window.__mock.wakeState().kind, label: $("wake-model").selectedOptions[0].textContent, download: calls("wake_download")[0]?.args };`);
    expect(/Downloading the Vosk (library|English model): \d+ %/.test(r.during.progress), r.during.progress);
    expect(r.during.secondary === "Hide" && r.during.primaryHidden, JSON.stringify(r.during));
    expect(r.closed, "the dialog did not close after the download");
    expect(r.state === "no_phrases", r.state);
    expect(r.label.endsWith("· downloaded"), r.label);
    expect(r.download?.model === "vosk-model-small-en-us-0.15", JSON.stringify(r.download));
  }],
  ["a failed download: Try again and Close, and Try again finishes it", async () => {
    await load("?fresh");
    const r = await page(`
      window.__mock.failNextDownload();
      $("wake-install-primary").click();
      await until(() => !$("wake-install-error").hidden, 6000);
      const failed = { error: $("wake-install-error").textContent, primary: $("wake-install-primary").textContent, secondary: $("wake-install-secondary").textContent };
      $("wake-install-primary").click();
      const closed = await until(() => !dialog.open, 10000);
      return { failed, closed, state: window.__mock.wakeState().kind };`);
    expect(r.failed.error.startsWith("The download failed:") && r.failed.error.includes("SHA-256"), r.failed.error);
    expect(r.failed.primary === "Try again" && r.failed.secondary === "Close", JSON.stringify(r.failed));
    expect(r.closed && r.state === "no_phrases", `closed ${r.closed}, state ${r.state}`);
  }],
  ["Hide during a download: the download goes on in the card's notice", async () => {
    await load("?fresh");
    const r = await page(`
      $("wake-install-primary").click();
      await until(() => $("wake-install-secondary").textContent === "Hide", 4000);
      $("wake-install-secondary").click(); await sleep(400);
      const hidden = { open: dialog.open, notice: $("wake-download").hidden ? "" : $("wake-download-text").textContent };
      await until(() => window.__mock.wakeState().kind === "no_phrases", 10000);
      return { hidden, state: window.__mock.wakeState().kind };`);
    expect(!r.hidden.open, "the dialog stayed open");
    expect(r.hidden.notice.startsWith("Downloading the"), `notice: ${r.hidden.notice}`);
    expect(r.state === "no_phrases", r.state);
  }],
  ["during a download the list does not change and a hidden Download does nothing", async () => {
    await load("?fresh");
    const r = await page(`
      $("wake-install-primary").click();
      await until(() => $("wake-install-primary").hidden, 3000);
      const downloads = calls("wake_download").length;
      await choose("wake-model", "vosk-model-small-fr-0.22");
      const list = { select: $("wake-model").value, stored: window.__mock.settings().wake.model };
      $("wake-install-primary").click(); await sleep(500);
      return { list, extra: calls("wake_download").length - downloads, error: $("wake-install-error").hidden ? "" : $("wake-install-error").textContent, open: dialog.open };`);
    expect(r.list.select === "vosk-model-small-en-us-0.15" && r.list.stored === "vosk-model-small-en-us-0.15", `the list moved under the dialog: ${JSON.stringify(r.list)}`);
    expect(r.extra === 0, "the hidden Download asked for a download");
    expect(!r.error, `the dialog shows: ${r.error}`);
  }],
  ["choice: a model not on disk asks first and lists only what is missing", async () => {
    await load("?fresh");
    const r = await page(`
      $("wake-install-secondary").click(); window.__mock.setInstalled(["vosk-model-small-en-us-0.15"], true); await sleep(400);
      await choose("wake-model", "vosk-model-small-de-0.15");
      return { open: dialog.open, items: items(), secondary: $("wake-install-secondary").textContent, note: $("wake-install-note").textContent, stored: window.__mock.settings().wake.model };`);
    expect(r.open, "no dialog for the German model");
    expect(JSON.stringify(r.items) === JSON.stringify(["Vosk German model — 46 MB from alphacephei.com"]), JSON.stringify(r.items));
    expect(r.secondary === "Cancel", r.secondary);
    expect(r.note.includes("Cancel keeps Vosk small English (US) 0.15"), r.note);
    expect(r.stored === "vosk-model-small-en-us-0.15", `saved before the OK: ${r.stored}`);
  }],
  ["choice Cancel: the list goes back, nothing is saved or downloaded", async () => {
    await load("?fresh");
    const r = await page(`
      $("wake-install-secondary").click(); window.__mock.setInstalled(["vosk-model-small-en-us-0.15"], true); await sleep(400);
      const saves = calls("save_settings").length;
      await choose("wake-model", "vosk-model-small-de-0.15");
      $("wake-install-secondary").click(); await sleep(400);
      return { open: dialog.open, select: $("wake-model").value, stored: window.__mock.settings().wake.model, saves: calls("save_settings").length - saves, downloads: calls("wake_download").length };`);
    expect(!r.open, "the dialog stayed open");
    expect(r.select === "vosk-model-small-en-us-0.15", `list on ${r.select}`);
    expect(r.stored === "vosk-model-small-en-us-0.15" && r.saves === 0, `stored ${r.stored}, ${r.saves} saves`);
    expect(r.downloads === 0, "a download started");
  }],
  ["choice Download: saves the model, then downloads it", async () => {
    await load("?fresh");
    const r = await page(`
      $("wake-install-secondary").click(); window.__mock.setInstalled(["vosk-model-small-en-us-0.15"], true); await sleep(400);
      await choose("wake-model", "vosk-model-small-de-0.15");
      $("wake-install-primary").click();
      const closed = await until(() => !dialog.open, 10000);
      const order = window.__mock.calls.filter((c) => c.name === "save_settings" || c.name === "wake_download").map((c) => c.name + ":" + (c.args.model || c.args.patch?.wake?.model || ""));
      return { closed, stored: window.__mock.settings().wake.model, order, state: window.__mock.wakeState().kind };`);
    expect(r.stored === "vosk-model-small-de-0.15", `stored ${r.stored}`);
    const save = r.order.indexOf("save_settings:vosk-model-small-de-0.15");
    const download = r.order.indexOf("wake_download:vosk-model-small-de-0.15");
    expect(save >= 0 && download > save, JSON.stringify(r.order));
    expect(r.closed && r.state === "no_phrases", `closed ${r.closed}, state ${r.state}`);
  }],
  ["choice of an installed model: saved at once, no dialog", async () => {
    await load("?fresh");
    const r = await page(`
      $("wake-install-secondary").click(); window.__mock.setInstalled(["vosk-model-small-ru-0.22"], true); await sleep(400);
      await choose("wake-model", "vosk-model-small-ru-0.22"); await sleep(300);
      return { open: dialog.open, stored: window.__mock.settings().wake.model };`);
    expect(!r.open, "a dialog for an installed model");
    expect(r.stored === "vosk-model-small-ru-0.22", `stored ${r.stored}`);
  }],
  ["choice of a local model without the library: only the library", async () => {
    await load("?fresh");
    const r = await page(`
      $("wake-install-secondary").click(); await sleep(300);
      await choose("wake-model", "local:vosk-model-small-ca-0.4");
      return { open: dialog.open, items: items(), text: $("wake-install-text").textContent };`);
    expect(r.open, "no dialog");
    expect(JSON.stringify(r.items) === JSON.stringify(["Vosk library — 15 MB from github.com"]), JSON.stringify(r.items));
    expect(r.text.includes("vosk-model-small-ca-0.4"), r.text);
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
console.log(failed ? "the wake dialog check failed" : `all ${scenarios.length} wake dialog scenarios passed`);
process.exit(failed ? 1 : 0);
