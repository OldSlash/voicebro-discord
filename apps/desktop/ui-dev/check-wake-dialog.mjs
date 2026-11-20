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
    const manual = (id) => ({ shown: !$(id).hidden, files: [...$(id).querySelectorAll("code")].map((c) => c.textContent), links: [...$(id).querySelectorAll("a")].map((a) => [a.getAttribute("href"), a.target, a.rel]) });
    const joinAndWait = async () => { window.__mock.join(); return until(() => dialog.open, 3000); };
    const noticeItems = () => $("wake-download").hidden ? [] : [...document.querySelectorAll("#wake-download-items li")].map((li) => li.textContent);
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

const LIBRARY_PAGE = "https://github.com/alphacep/vosk-api/releases/tag/v0.3.45";
const MODELS_PAGE = "https://alphacephei.com/vosk/models";
const ENGLISH_FILES = ["vosk-win64-0.3.45.zip", "vosk-model-small-en-us-0.15.zip"];
const ENGLISH_LINKS = [[LIBRARY_PAGE, "_blank", "noopener"], [MODELS_PAGE, "_blank", "noopener"]];
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

const scenarios = [
  ["start on a clean folder: no dialog, the notice lists the files with their links", async () => {
    await load("?fresh");
    const r = await page(`return { open: dialog.open, dialogs: openDialogs(), notice: !$("wake-download").hidden, manual: manual("wake-download-manual"), downloads: calls("wake_download").length };`);
    expect(!r.open && r.dialogs === 0, "a dialog opened because the window started");
    expect(r.notice, "no notice in the Assistant card");
    expect(r.manual.shown, "the notice has no download-by-hand part");
    expect(same(r.manual.files, ENGLISH_FILES), JSON.stringify(r.manual.files));
    expect(same(r.manual.links, ENGLISH_LINKS), JSON.stringify(r.manual.links));
    expect(r.downloads === 0, "a download started without the OK");
  }],
  ["the bot joins with files missing: the join is refused and the dialog lists the library and the English model", async () => {
    await load("?fresh");
    const r = await page(`const joined = window.__mock.join(); const opened = await until(() => dialog.open, 3000); await sleep(1200); return { joined, opened, reason: dialog.dataset.reason, items: items(), title: $("wake-install-title").textContent, text: $("wake-install-text").textContent, secondary: $("wake-install-secondary").textContent, note: $("wake-install-note").textContent, downloads: calls("wake_download").length, dialogs: openDialogs(), voice: $("footer-voice").textContent, manual: manual("wake-install-manual"), log: document.getElementById("console-output")?.textContent || "" };`);
    expect(r.joined === false, "the join was not refused");
    expect(r.opened, "no dialog after the refused join");
    expect(r.reason === "join", r.reason);
    expect(r.voice === "0", `the bot is in a channel: voice count ${r.voice}`);
    expect(r.note.includes("Run /join again"), r.note);
    expect(r.dialogs === 1, `${r.dialogs} dialogs open`);
    expect(r.title === "The wake word is not installed", r.title);
    expect(r.text.includes("Vosk small English (US) 0.15"), r.text);
    expect(same(r.items, ["Vosk library — 15 MB from github.com", "Vosk English model — 41 MB from alphacephei.com"]), JSON.stringify(r.items));
    expect(r.secondary === "Not now", r.secondary);
    expect(r.manual.shown && same(r.manual.files, ENGLISH_FILES) && same(r.manual.links, ENGLISH_LINKS), JSON.stringify(r.manual));
    expect(r.downloads === 0, "a download started without the OK");
  }],
  ["Not now: closes, downloads nothing, the notice stays", async () => {
    await load("?fresh");
    const r = await page(`await joinAndWait(); $("wake-install-secondary").click(); await sleep(400); return { open: dialog.open, downloads: calls("wake_download").length, notice: !$("wake-download").hidden, state: window.__mock.wakeState().kind };`);
    expect(!r.open, "the dialog stayed open");
    expect(r.downloads === 0, "a download started");
    expect(r.notice, "the Assistant card's notice is gone");
    expect(r.state === "needs_download", r.state);
  }],
  ["Escape: the same as Not now", async () => {
    await load("?fresh");
    await page(`return joinAndWait();`);
    await pressEscape();
    const r = await page(`return { open: dialog.open, downloads: calls("wake_download").length };`);
    expect(!r.open, "Escape left the dialog open");
    expect(r.downloads === 0, "a download started");
  }],
  ["a stray Enter when the dialog comes up downloads nothing", async () => {
    await load("?fresh");
    await page(`return joinAndWait();`);
    const focused = await page(`return document.activeElement?.id;`);
    await pressEnter();
    const r = await page(`return { open: dialog.open, downloads: calls("wake_download").length };`);
    expect(focused === "wake-install-secondary", `the focus starts on ${focused}`);
    expect(r.downloads === 0, "Enter agreed to the download");
    expect(!r.open, "Enter on Not now left the dialog open");
  }],
  ["every join asks again: Not now, join", async () => {
    await load("?fresh");
    const r = await page(`await joinAndWait(); $("wake-install-secondary").click(); await sleep(1300); const closedBetween = !dialog.open; const joined = window.__mock.join(); const opened = await until(() => dialog.open, 3000); return { closedBetween, joined, opened, secondary: $("wake-install-secondary").textContent };`);
    expect(r.closedBetween, "the dialog opened again by itself");
    expect(r.joined === false, "the second join was not refused");
    expect(r.opened, "no dialog at the second join");
    expect(r.secondary === "Not now", r.secondary);
  }],
  ["a reload with the bot in a voice channel: no dialog", async () => {
    await load("?fresh&joined");
    const r = await page(`await sleep(2500); return { open: dialog.open, dialogs: openDialogs(), notice: !$("wake-download").hidden };`);
    expect(!r.open && r.dialogs === 0, "a dialog opened without a refused join");
    expect(r.notice, "no notice in the Assistant card");
  }],
  ["one dialog at most: a join while it is open opens no second one", async () => {
    await load("?fresh");
    const r = await page(`await joinAndWait(); const reason = dialog.dataset.reason; window.__mock.join(); await sleep(1300); return { dialogs: openDialogs(), reason };`);
    expect(r.reason === "join", r.reason);
    expect(r.dialogs === 1, `${r.dialogs} dialogs open`);
  }],
  ["always listening: nothing at start, the bot joins without Vosk", async () => {
    await load("?fresh&always");
    const r = await page(`const atStart = dialog.open; const joined = window.__mock.join(); await sleep(1300); return { atStart, joined, afterJoin: dialog.open, state: window.__mock.wakeState().kind, voice: $("footer-voice").textContent };`);
    expect(!r.atStart, "a dialog at start in always mode");
    expect(r.joined === true && r.voice === "1", `joined ${r.joined}, voice ${r.voice}`);
    expect(!r.afterJoin, "a dialog at a join in always mode");
    expect(r.state === "off", r.state);
  }],
  ["switched to wake word: the setting is saved, no dialog, the notice lists the files", async () => {
    await load("?fresh&always");
    const r = await page(`await choose("session-mode", "wake_word"); await sleep(1500); return { open: dialog.open, mode: window.__mock.settings().session.mode, notice: noticeItems() };`);
    expect(r.mode === "wake_word", `mode ${r.mode}`);
    expect(!r.open, "a dialog opened at the switch");
    expect(r.notice.length === 2, `notice: ${JSON.stringify(r.notice)}`);
  }],
  ["files on disk: no dialog at start or at a join, and the bot joins", async () => {
    await load("");
    const r = await page(`const atStart = dialog.open; window.__mock.leave(); await sleep(1300); const joined = window.__mock.join(); await sleep(1300); return { atStart, joined, afterJoin: dialog.open, state: window.__mock.wakeState().kind };`);
    expect(!r.atStart, "a dialog at start although the files are on disk");
    expect(r.joined === true, "the join was refused although the files are on disk");
    expect(!r.afterJoin, "a dialog at a join although the files are on disk");
    expect(r.state === "ready", r.state);
  }],
  ["a link: the page stays on the app and the dialog stays open", async () => {
    await load("?fresh");
    const r = await page(`
      await joinAndWait();
      const before = location.href;
      $("wake-install-manual").querySelector("a").click();
      await sleep(500);
      const afterDialog = { href: location.href, open: dialog.open };
      $("wake-download-manual").querySelector("a").click();
      await sleep(500);
      return { before, afterDialog, afterNotice: location.href, saves: calls("save_settings").length };`);
    expect(r.afterDialog.href === r.before, `the dialog's link navigated the page to ${r.afterDialog.href}`);
    expect(r.afterDialog.open, "the dialog closed at the link");
    expect(r.afterNotice === r.before, `the notice's link navigated the page to ${r.afterNotice}`);
    expect(r.saves === 0, "a link changed a setting");
  }],
  ["Open assets folder: the folder opens, the dialog stays open", async () => {
    await load("?fresh");
    const r = await page(`
      await joinAndWait();
      $("wake-install-assets").click(); await sleep(300);
      const fromDialog = { calls: calls("open_assets_folder").length, open: dialog.open };
      $("wake-install-secondary").click(); await sleep(300);
      $("wake-download-assets").click(); await sleep(300);
      return { fromDialog, total: calls("open_assets_folder").length, feedback: $("config-autosave-feedback").textContent };`);
    expect(r.fromDialog.calls === 1 && r.fromDialog.open, JSON.stringify(r.fromDialog));
    expect(r.total === 2, `${r.total} calls`);
    expect(r.feedback === "Opened C:\\VoiceBro\\assets", r.feedback);
  }],
  ["Download: the progress in the dialog, the links hidden, closed when installed", async () => {
    await load("?fresh");
    const r = await page(`
      await joinAndWait();
      $("wake-install-primary").click();
      await until(() => !$("wake-install-progress").hidden && /Downloading/.test($("wake-install-progress").textContent), 4000);
      const during = { progress: $("wake-install-progress").textContent, secondary: $("wake-install-secondary").textContent, primaryHidden: $("wake-install-primary").hidden, manual: !$("wake-install-manual").hidden, notice: !$("wake-download-manual").hidden };
      const unpacking = await until(() => /^Checking and unpacking the Vosk (library|English model)…$/.test($("wake-install-progress").textContent), 6000);
      const closed = await until(() => !dialog.open, 10000);
      await sleep(400);
      return { during, unpacking, closed, state: window.__mock.wakeState().kind, label: $("wake-model").selectedOptions[0].textContent, download: calls("wake_download")[0]?.args };`);
    expect(/Downloading the Vosk (library|English model): \d+ %/.test(r.during.progress), r.during.progress);
    expect(r.during.secondary === "Hide" && r.during.primaryHidden, JSON.stringify(r.during));
    expect(!r.during.manual && !r.during.notice, `the links show during the download: ${JSON.stringify(r.during)}`);
    expect(r.unpacking, "no checking-and-unpacking step");
    expect(r.closed, "the dialog did not close after the download");
    expect(r.state === "ready", r.state);
    expect(r.label.endsWith("· downloaded"), r.label);
    expect(r.download?.model === "vosk-model-small-en-us-0.15", JSON.stringify(r.download));
  }],
  ["a failed download: Try again and Close, the links back, and Try again finishes it", async () => {
    await load("?fresh");
    const r = await page(`
      await joinAndWait();
      window.__mock.failNextDownload();
      $("wake-install-primary").click();
      await until(() => !$("wake-install-error").hidden, 6000);
      await sleep(300);
      const failed = { error: $("wake-install-error").textContent, primary: $("wake-install-primary").textContent, secondary: $("wake-install-secondary").textContent, manual: manual("wake-install-manual"), notice: manual("wake-download-manual") };
      $("wake-install-primary").click();
      const closed = await until(() => !dialog.open, 10000);
      return { failed, closed, state: window.__mock.wakeState().kind };`);
    expect(r.failed.error.startsWith("The download failed:") && r.failed.error.includes("SHA-256"), r.failed.error);
    expect(r.failed.primary === "Try again" && r.failed.secondary === "Close", JSON.stringify(r.failed));
    expect(r.failed.manual.shown && same(r.failed.manual.files, ENGLISH_FILES), `dialog: ${JSON.stringify(r.failed.manual)}`);
    expect(r.failed.notice.shown && same(r.failed.notice.files, ENGLISH_FILES), `notice: ${JSON.stringify(r.failed.notice)}`);
    expect(r.closed && r.state === "ready", `closed ${r.closed}, state ${r.state}`);
  }],
  ["archives put there by hand: checked and unpacked, never Downloading", async () => {
    await load("?fresh");
    const r = await page(`
      await joinAndWait();
      window.__mock.placeArchives(["vosk-model-small-en-us-0.15"], true);
      const texts = new Set();
      const watch = setInterval(() => texts.add($("wake-install-progress").textContent), 20);
      $("wake-install-primary").click();
      const closed = await until(() => !dialog.open, 6000);
      clearInterval(watch);
      return { closed, texts: [...texts], state: window.__mock.wakeState().kind, downloads: calls("wake_download").length };`);
    expect(r.closed, "the dialog did not close");
    expect(!r.texts.some((text) => /download/i.test(text)), `the dialog spoke of a download: ${JSON.stringify(r.texts)}`);
    expect(r.texts.includes("Checking and unpacking the Vosk library…") && r.texts.includes("Checking and unpacking the Vosk English model…"), JSON.stringify(r.texts));
    expect(r.state === "ready" && r.downloads === 1, `state ${r.state}, ${r.downloads} calls`);
  }],
  ["Hide during a download: the download goes on in the card's notice", async () => {
    await load("?fresh");
    const r = await page(`
      await joinAndWait();
      $("wake-install-primary").click();
      await until(() => $("wake-install-secondary").textContent === "Hide", 4000);
      $("wake-install-secondary").click(); await sleep(400);
      const hidden = { open: dialog.open, notice: $("wake-download").hidden ? "" : $("wake-download-text").textContent };
      await until(() => window.__mock.wakeState().kind === "ready", 10000);
      return { hidden, state: window.__mock.wakeState().kind };`);
    expect(!r.hidden.open, "the dialog stayed open");
    expect(r.hidden.notice.startsWith("Downloading the"), `notice: ${r.hidden.notice}`);
    expect(r.state === "ready", r.state);
  }],
  ["during a download the list does not change and a hidden Download does nothing", async () => {
    await load("?fresh");
    const r = await page(`
      await joinAndWait();
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
  ["choice: a model not on disk is saved at once, no dialog, the notice lists only what is missing", async () => {
    await load("?fresh");
    const r = await page(`
      window.__mock.setInstalled(["vosk-model-small-en-us-0.15"], true); await sleep(400);
      await choose("wake-model", "vosk-model-small-de-0.15"); await sleep(1300);
      return { open: dialog.open, dialogs: openDialogs(), notice: noticeItems(), manual: manual("wake-download-manual"), stored: window.__mock.settings().wake.model, downloads: calls("wake_download").length };`);
    expect(!r.open && r.dialogs === 0, "a dialog opened for the German model");
    expect(r.stored === "vosk-model-small-de-0.15", `stored ${r.stored}`);
    expect(same(r.notice, ["Vosk German model — 46 MB from alphacephei.com"]), JSON.stringify(r.notice));
    expect(same(r.manual.files, ["vosk-model-small-de-0.15.zip"]) && same(r.manual.links, [[MODELS_PAGE, "_blank", "noopener"]]), JSON.stringify(r.manual));
    expect(r.downloads === 0, "a download started without the OK");
  }],
  ["choice Cancel: picking the installed model again removes the notice, nothing is downloaded", async () => {
    await load("?fresh");
    const r = await page(`
      window.__mock.setInstalled(["vosk-model-small-en-us-0.15"], true); await sleep(400);
      await choose("wake-model", "vosk-model-small-de-0.15"); await sleep(1300);
      const before = noticeItems().length;
      await choose("wake-model", "vosk-model-small-en-us-0.15"); await sleep(1300);
      return { before, after: noticeItems().length, open: dialog.open, stored: window.__mock.settings().wake.model, downloads: calls("wake_download").length };`);
    expect(r.before === 1, `notice before: ${r.before}`);
    expect(r.after === 0, `notice after: ${r.after}`);
    expect(!r.open, "a dialog opened");
    expect(r.stored === "vosk-model-small-en-us-0.15", `stored ${r.stored}`);
    expect(r.downloads === 0, "a download started");
  }],
  ["choice Download: the notice's Download downloads the picked model", async () => {
    await load("?fresh");
    const r = await page(`
      window.__mock.setInstalled(["vosk-model-small-en-us-0.15"], true); await sleep(400);
      await choose("wake-model", "vosk-model-small-de-0.15"); await sleep(1300);
      $("wake-download-button").click();
      const ready = await until(() => window.__mock.wakeState().kind === "ready", 10000);
      await sleep(1300);
      return { ready, open: dialog.open, notice: !$("wake-download").hidden, download: calls("wake_download")[0]?.args };`);
    expect(r.download?.model === "vosk-model-small-de-0.15", JSON.stringify(r.download));
    expect(r.ready, "the German model was not installed");
    expect(!r.open, "a dialog opened");
    expect(!r.notice, "the notice stayed after the download");
  }],
  ["choice of an installed model: saved at once, no dialog", async () => {
    await load("?fresh");
    const r = await page(`
      window.__mock.setInstalled(["vosk-model-small-ru-0.22"], true); await sleep(400);
      await choose("wake-model", "vosk-model-small-ru-0.22"); await sleep(300);
      return { open: dialog.open, stored: window.__mock.settings().wake.model };`);
    expect(!r.open, "a dialog for an installed model");
    expect(r.stored === "vosk-model-small-ru-0.22", `stored ${r.stored}`);
  }],
  ["choice of a local model without the library: saved, the notice lists only the library", async () => {
    await load("?fresh");
    const r = await page(`
      await choose("wake-model", "local:vosk-model-small-ca-0.4"); await sleep(1300);
      return { open: dialog.open, notice: noticeItems(), stored: window.__mock.settings().wake.model };`);
    expect(!r.open, "a dialog opened");
    expect(r.stored === "local:vosk-model-small-ca-0.4", `stored ${r.stored}`);
    expect(same(r.notice, ["Vosk library — 15 MB from github.com"]), JSON.stringify(r.notice));
  }],
  ["a refused join for a local model without the library: the dialog lists only the library", async () => {
    await load("?fresh");
    const r = await page(`
      await choose("wake-model", "local:vosk-model-small-ca-0.4"); await sleep(600);
      const joined = window.__mock.join(); await until(() => dialog.open, 3000);
      return { joined, open: dialog.open, items: items(), text: $("wake-install-text").textContent };`);
    expect(r.joined === false && r.open, `joined ${r.joined}, open ${r.open}`);
    expect(same(r.items, ["Vosk library — 15 MB from github.com"]), JSON.stringify(r.items));
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
