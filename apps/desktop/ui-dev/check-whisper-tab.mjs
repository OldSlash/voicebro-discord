// Runs every scenario of the Whisper tab (change `whisper-mode`: the settings-ui and
// app-window deltas) against the command mock in a headless Edge and stops at the first
// failure with its name (exit code 1): the tab and its four cards, the fields with their
// help and message lines, the grouped lists, the install dialog's Whisper subject and its
// triggers, the download notices, the GPU fallback and the timings on the Mode card,
// Preview, the Config tab in the Whisper mode and the Status tab's tiles. Each scenario
// loads the page afresh. Needs Node 24 (built-in WebSocket) and Microsoft Edge; writes its
// browser profile under target\. Local only; never shipped.
//
//   node apps/desktop/ui-dev/check-whisper-tab.mjs
import { spawn } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..", "..");
const PAGE_PORT = 5183;
const DEBUG_PORT = 9233;
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
const profile = join(repo, "target", "ui-check-whisper-profile");
mkdirSync(profile, { recursive: true });
// Preview plays from a script's click here; in the app the user's click allows it.
const edge = spawn(EDGE, [
  "--headless=new", `--remote-debugging-port=${DEBUG_PORT}`, `--user-data-dir=${profile}`,
  "--window-size=1280,800", "--no-first-run", "--disable-extensions",
  "--autoplay-policy=no-user-gesture-required", "about:blank",
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
    const calls = (name) => window.__mock.calls.filter((c) => c.name === name);
    const stored = () => window.__mock.settings();
    const tab = (name) => { document.querySelector('[data-tab="' + name + '"]').click(); };
    const titles = (name) => [...document.querySelector('[data-panel="' + name + '"]').querySelectorAll(".card")].map((card) => card.querySelector("h2").textContent.trim());
    const card = (id) => document.querySelector('[data-card="' + id + '"]');
    const collapsed = (id) => card(id).querySelector(".card-body").hidden && card(id).querySelector(".card-toggle").getAttribute("aria-expanded") === "false";
    const toggle = async (id) => { card(id).querySelector(".card-toggle").click(); await sleep(250); };
    const setValue = async (id, value, events = ["input", "change"]) => { const c = $(id); c.value = value; for (const e of events) c.dispatchEvent(new Event(e, { bubbles: true })); await sleep(400); };
    const items = () => [...document.querySelectorAll("#wake-install-items li")].map((li) => li.textContent);
    const openDialogs = () => document.querySelectorAll("dialog[open]").length;
    const manual = (id) => ({ shown: !$(id).hidden, files: [...$(id).querySelectorAll("code")].map((c) => c.textContent), links: [...$(id).querySelectorAll("a")].map((a) => a.getAttribute("href")) });
    const notice = (part) => ({ shown: !$(part + "-download").hidden, text: $(part + "-download-text").textContent, items: [...$(part + "-download-items").children].map((li) => li.textContent), manual: manual(part + "-download-manual") });
    const help = (id) => $(id).closest("label, .field").querySelector(".field-help").textContent;
    const feedback = () => $("whisper-feedback").textContent;
    const optionText = (id, value) => [...$(id).options].find((o) => o.value === value)?.textContent;
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
  // The window's own start: settings, model and voice lists, first status.
  await sleep(1300);
}
async function press(key) {
  const keys = {
    Enter: { key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" },
    Escape: { key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 },
  }[key];
  await send("Input.dispatchKeyEvent", { type: "keyDown", ...keys });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: keys.key, code: keys.code, windowsVirtualKeyCode: keys.windowsVirtualKeyCode });
  await sleep(350);
}
function expect(condition, message) {
  if (!condition) throw new Error(message);
}
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

const CARDS = ["whisper-mode", "whisper-recognition", "whisper-phrases", "whisper-voice"];
// The fields of the tab, each saved on change with its help and message lines.
const FIELDS = [
  "assistant-mode", "whisper-model", "whisper-device", "whisper-threads", "whisper-language",
  "whisper-activation", "whisper-deactivation", "whisper-session-mode", "tts-voice", "tts-speed",
];
// What a first join on an empty PC needs: Whisper's three files, then the voice's two.
const MISSING = [
  "Whisper runtime (CPU) — 9 MB from github.com",
  "Whisper base (q8_0) model — 82 MB from huggingface.co",
  "Silero voice activity model — 0.9 MB from huggingface.co",
  "Voice engine (sherpa-onnx 2.0.1) — 8 MB from github.com",
  "Supertonic 3 voices (10 voices, 31 languages) — 129 MB from github.com",
];
const MISSING_FILES = [
  "whisper-bin-x64.zip", "ggml-base-q8_0.bin", "ggml-silero-v6.2.0.bin",
  "sherpa-onnx-v2.0.1-win-x64-shared-MT-Release-lib.tar.bz2", "sherpa-onnx-supertonic-3-tts-int8-2026-05-11.tar.bz2",
];

const scenarios = [
  // ---- The tab and its cards (app-window, settings-ui)
  ["the tabs end with Config and Whisper, and the Whisper tab holds Mode, Speech recognition, Phrases and Voice", async () => {
    await load();
    const r = await page(`
      return { tabs: [...document.querySelectorAll(".tab-button")].map((b) => b.textContent.trim()), titles: titles("whisper"),
        cards: [...document.querySelectorAll('[data-panel="whisper"] .collapsible')].map((c) => c.dataset.card),
        feedbackOutside: !$("whisper-feedback").closest(".card") && $("whisper-feedback").closest(".tab-panel").dataset.panel };`);
    expect(same(r.tabs, ["Console", "Codex chat", "Status", "Performance", "System", "Settings", "Config", "Whisper"]), `tabs: ${r.tabs}`);
    expect(same(r.titles, ["Mode", "Speech recognition", "Phrases", "Voice"]), `cards: ${r.titles}`);
    expect(same(r.cards, CARDS), `card ids: ${r.cards}`);
    expect(r.feedbackOutside === "whisper", "the tab has no feedback line of its own");
  }],
  ["every field has its help line and its message line, and the help says what the specs ask", async () => {
    await load();
    const r = await page(`
      const out = ${JSON.stringify(FIELDS)}.map((id) => { const label = $(id).closest("label");
        return [id, Boolean(label), (label?.querySelector(".field-help")?.textContent || "").length, Boolean(document.querySelector('.field-message[data-for="' + id + '"]'))]; });
      return { out, mode: help("assistant-mode"), activation: help("whisper-activation"), session: help("whisper-session-mode"),
        preview: help("tts-preview"), voice: help("tts-voice"), device: help("whisper-device") };`);
    for (const [id, label, helpLength, message] of r.out) {
      expect(label && helpLength > 20 && message, `${id}: label ${label}, help ${helpLength} characters, message line ${message}`);
    }
    for (const part of ["Realtime voice", "only text goes to OpenAI", "Model and Reasoning", "low reasoning level"]) {
      expect(r.mode.includes(part), `the mode help lacks «${part}»: ${r.mode}`);
    }
    for (const part of ["any word", "commas or line breaks", "ignores case and punctuation", "default phrases"]) {
      expect(r.activation.includes(part), `the phrases help lacks «${part}»: ${r.activation}`);
    }
    expect(r.session.includes("everything said in the channel goes to Codex"), `activation help: ${r.session}`);
    expect(r.preview.includes("in the window only") && r.preview.includes("never in Discord"), `preview help: ${r.preview}`);
    expect(r.voice.includes("GPL-3.0") && r.voice.includes("built into Windows"), `voice help: ${r.voice}`);
    expect(r.device.includes("CPU"), `device help: ${r.device}`);
  }],
  ["the lists: models in four groups with size, state and note; languages Automatic, the ten, the rest; voices by engine and language", async () => {
    await load("?whisper");
    const r = await page(`
      const groups = (id) => [...$(id).querySelectorAll("optgroup")].map((g) => g.label);
      const language = [...$("whisper-language").options].map((o) => o.value);
      return { modelGroups: groups("whisper-model"), base: optionText("whisper-model", "whisper-base-q8_0"),
        small: optionText("whisper-model", "whisper-small-q8_0"), local: optionText("whisper-model", "local:ggml-small-finetuned.bin"),
        model: $("whisper-model").value, language: language.slice(0, 11), languageText: optionText("whisper-language", "auto"),
        rest: language[11], languageGroups: groups("whisper-language"), voiceGroups: groups("tts-voice"),
        supertonic: optionText("tts-voice", "supertonic3:F1"), piper: optionText("tts-voice", "piper:de_DE-thorsten-medium"),
        windows: optionText("tts-voice", "windows:MSTTS_V110_enUS_DavidM"), info: $("tts-voice-info").textContent,
        device: [...$("whisper-device").options].map((o) => o.textContent) };`);
    expect(same(r.modelGroups, ["Multilingual", "English only", "Russian-tuned", "Your models"]), `model groups: ${r.modelGroups}`);
    expect(r.base === "base (q8_0) · 82 MB · downloaded · fast; weak for Russian", `base: ${r.base}`);
    expect(r.small === "small (q8_0) · 264 MB · to download · balanced; on a CPU about 1–2 s", `small: ${r.small}`);
    expect(r.local.includes("ggml-small-finetuned.bin") && r.local.includes("your own model"), `local: ${r.local}`);
    expect(r.model === "whisper-base-q8_0", `selected ${r.model}`);
    expect(same(r.language, ["auto", "en", "ru", "cs", "nl", "fr", "de", "it", "pl", "pt", "es"]) && r.languageText === "Automatic", `languages: ${r.language}`);
    expect(r.rest === "zh" && same(r.languageGroups, ["Languages with default phrases", "Other languages"]), `the rest: ${r.rest} ${r.languageGroups}`);
    for (const group of ["Supertonic 3 — 31 languages", "Piper — English (US)", "Piper — German", "Piper — Portuguese (Brazil)", "Kokoro — English (UK)", "Windows — English (US)"]) {
      expect(r.voiceGroups.includes(group), `voice group «${group}» missing: ${r.voiceGroups}`);
    }
    expect(r.supertonic === "Supertonic 3 F1 (female) · downloaded", `supertonic: ${r.supertonic}`);
    expect(r.piper === "Piper German Thorsten, male · 68 MB · to download · needs the GPL-3.0 phonemizer", `piper: ${r.piper}`);
    expect(r.windows === "Microsoft David · built into Windows", `windows: ${r.windows}`);
    expect(r.info.includes("BigScience OpenRAIL-M"), `voice info: ${r.info}`);
    expect(same(r.device, ["CPU", "NVIDIA GPU (CUDA)"]), `devices: ${r.device}`);
  }],
  ["every field saves on change, named in the tab's own feedback line", async () => {
    await load("?whisper");
    const r = await page(`
      tab("whisper");
      const configLine = $("config-autosave-feedback").textContent;
      const lines = {};
      await setValue("whisper-threads", "4"); lines.threads = feedback();
      await setValue("whisper-language", "de"); lines.language = feedback();
      await setValue("tts-speed", "1.25"); lines.speed = feedback();
      await setValue("whisper-activation", "hey bro, yo"); lines.activation = feedback();
      await setValue("whisper-session-mode", "always"); lines.session = feedback();
      await setValue("whisper-model", "local:ggml-small-finetuned.bin"); lines.model = feedback();
      await setValue("tts-voice", "windows:MSTTS_V110_enUS_ZiraM"); lines.voice = feedback();
      await setValue("whisper-deactivation", "bye now"); lines.deactivation = feedback();
      await setValue("assistant-mode", "realtime"); lines.mode = feedback();
      await setValue("whisper-device", "gpu"); lines.device = feedback();
      const s = stored();
      return { s: { mode: s.assistant.mode, session: s.session.mode, whisper: s.whisper, tts: s.tts }, lines,
        speedLabel: $("tts-speed-label").textContent, config: { line: $("config-autosave-feedback").textContent, before: configLine, session: $("session-mode").value },
        dialogs: openDialogs() };`);
    expect(r.s.whisper.threads === 4 && r.lines.threads === "Saved: CPU threads", `threads: ${JSON.stringify([r.s.whisper.threads, r.lines.threads])}`);
    expect(r.s.whisper.language === "de" && r.lines.language === "Saved: Language", `language: ${r.lines.language}`);
    expect(r.s.tts.speed === 1.25 && r.lines.speed === "Saved: Speed" && r.speedLabel === "1.25×", `speed: ${JSON.stringify([r.s.tts.speed, r.lines.speed, r.speedLabel])}`);
    expect(same(r.s.whisper.phrases.activation, ["hey bro", "yo"]) && r.lines.activation === "Saved: Activation phrases", `activation: ${JSON.stringify(r.s.whisper.phrases)}`);
    expect(same(r.s.whisper.phrases.deactivation, ["bye now"]) && r.lines.deactivation === "Saved: Deactivation phrases", `deactivation: ${JSON.stringify(r.s.whisper.phrases)}`);
    expect(r.s.session === "always" && r.lines.session === "Saved: Activation", `session: ${r.s.session} ${r.lines.session}`);
    expect(r.s.whisper.model === "local:ggml-small-finetuned.bin" && r.lines.model === "Saved: Model", `model: ${r.s.whisper.model} ${r.lines.model}`);
    expect(r.s.tts.voice === "windows:MSTTS_V110_enUS_ZiraM" && r.lines.voice === "Saved: Voice", `voice: ${r.s.tts.voice}`);
    expect(r.s.mode === "realtime" && r.lines.mode === "Saved: Assistant mode", `mode: ${r.s.mode}`);
    expect(r.s.whisper.device === "gpu" && r.lines.device === "Saved: Device", `device in the realtime mode: ${r.s.whisper.device}`);
    expect(r.config.line === r.config.before, `the Config line named a Whisper save: ${r.config.line}`);
    expect(r.config.session === "always", `the Config tab's Activation did not follow: ${r.config.session}`);
    expect(r.dialogs === 0, "a dialog opened");
  }],
  ["a rejected value shows the app's message under its field and stays to be corrected", async () => {
    await load("?whisper");
    const r = await page(`
      tab("whisper");
      await setValue("whisper-threads", "65");
      const threads = { message: document.querySelector('.field-message[data-for="whisper-threads"]').textContent,
        invalid: $("whisper-threads").closest("label").classList.contains("is-invalid"), value: $("whisper-threads").value, stored: stored().whisper.threads };
      await setValue("whisper-deactivation", "Bro!");
      const phrases = { message: document.querySelector('.field-message[data-for="whisper-deactivation"]').textContent,
        value: $("whisper-deactivation").value, stored: stored().whisper.phrases };
      await setValue("whisper-threads", "8");
      return { threads, phrases, fixed: { message: document.querySelector('.field-message[data-for="whisper-threads"]').textContent, stored: stored().whisper.threads } };`);
    expect(r.threads.message === "whisper.threads: 65 is outside 0 (automatic) to 64", `message: ${r.threads.message}`);
    expect(r.threads.invalid && r.threads.value === "65" && r.threads.stored === 0, JSON.stringify(r.threads));
    expect(r.phrases.message.startsWith("whisper.phrases.deactivation:") && r.phrases.message.includes("same phrase"), `phrases: ${r.phrases.message}`);
    expect(r.phrases.value === "Bro!" && same(r.phrases.stored, {}), JSON.stringify(r.phrases));
    expect(r.fixed.message === "" && r.fixed.stored === 8, `corrected: ${JSON.stringify(r.fixed)}`);
  }],
  ["phrases: the language's defaults until changed, an emptied list stays empty, another language shows its own", async () => {
    await load("?whisper");
    const r = await page(`
      tab("whisper");
      const start = [$("whisper-activation").value, $("whisper-deactivation").value, calls("save_settings").length];
      await setValue("whisper-language", "de");
      const german = [$("whisper-activation").value, $("whisper-deactivation").value];
      await setValue("whisper-deactivation", "");
      window.__mock.emit("app-event", { event: "settings_changed" }); await sleep(600);
      return { start, german, emptied: $("whisper-deactivation").value, activation: $("whisper-activation").value, phrases: stored().whisper.phrases };`);
    expect(same(r.start, ["bro", "peace out", 0]), `at start: ${JSON.stringify(r.start)}`);
    expect(same(r.german, ["bruder", "bis dann"]), `German: ${JSON.stringify(r.german)}`);
    expect(r.emptied === "" && r.activation === "bruder" && same(r.phrases, { deactivation: [] }), JSON.stringify(r));
  }],

  // ---- Collapsible cards (settings-ui)
  ["first start: every card of the Whisper tab is expanded", async () => {
    await load();
    const r = await page(`return ${JSON.stringify(CARDS)}.map((id) => [id, collapsed(id)]);`);
    expect(r.every(([, folded]) => !folded), JSON.stringify(r));
  }],
  ["the Voice card folds, is saved, and is folded after a restart; Enter folds the Mode card", async () => {
    await load();
    const list = await page(`tab("whisper"); await toggle("whisper-voice"); return { folded: collapsed("whisper-voice"), stored: stored().ui.collapsed_cards };`);
    expect(list.folded && same(list.stored, ["whisper-voice"]), JSON.stringify(list));
    await load("?collapsed=whisper-voice");
    const r = await page(`return ${JSON.stringify(CARDS)}.map((id) => [id, collapsed(id)]);`);
    expect(same(r, [["whisper-mode", false], ["whisper-recognition", false], ["whisper-phrases", false], ["whisper-voice", true]]), JSON.stringify(r));
    await page(`tab("whisper"); card("whisper-mode").querySelector(".card-toggle").focus(); return true;`);
    await press("Enter");
    const keyboard = await page(`return { folded: collapsed("whisper-mode"), stored: stored().ui.collapsed_cards };`);
    expect(keyboard.folded && same(keyboard.stored, ["whisper-mode", "whisper-voice"]), `Enter: ${JSON.stringify(keyboard)}`);
  }],
  ["switching tabs keeps the Whisper tab's scroll position and an unsaved value", async () => {
    await load("?whisper");
    const r = await page(`
      tab("whisper");
      const panel = document.querySelector('[data-panel="whisper"]');
      $("whisper-threads").value = "7"; $("whisper-threads").dispatchEvent(new Event("input", { bubbles: true }));
      panel.scrollTop = 300; await sleep(100); const top = panel.scrollTop;
      tab("status"); await sleep(1200); tab("whisper"); await sleep(200);
      return { top, back: panel.scrollTop, value: $("whisper-threads").value, dirty: $("whisper-threads").closest("label").classList.contains("is-dirty"), stored: stored().whisper.threads };`);
    expect(r.top > 0 && r.back === r.top, `scroll ${r.top} → ${r.back}`);
    expect(r.value === "7" && r.dirty && r.stored === 0, JSON.stringify(r));
  }],

  // ---- The install dialog's Whisper subject (app-window) and the notices
  ["start in the Whisper mode with files missing: no dialog, and the notices list what is missing", async () => {
    await load("?whisper_missing");
    await sleep(1200);
    const r = await page(`return { open: dialog.open, dialogs: openDialogs(), whisper: notice("whisper"), tts: notice("tts"),
      downloads: calls("whisper_download").length, now: $("whisper-now-recognition").textContent };`);
    expect(!r.open && r.dialogs === 0, "a dialog opened because the window started");
    expect(r.whisper.shown && same(r.whisper.items, MISSING.slice(0, 3)), `recognition notice: ${JSON.stringify(r.whisper)}`);
    expect(r.tts.shown && same(r.tts.items, MISSING.slice(3)), `voice notice: ${JSON.stringify(r.tts)}`);
    expect(r.whisper.manual.shown && same(r.whisper.manual.files, MISSING_FILES.slice(0, 3)), `by hand: ${JSON.stringify(r.whisper.manual)}`);
    expect(r.whisper.text.includes("nothing is downloaded until you press Download"), r.whisper.text);
    expect(r.downloads === 0, "a download started without the OK");
    expect(r.now.includes("not on this PC"), `Mode card: ${r.now}`);
  }],
  ["a /join refused for missing files: the dialog lists the runtime, the model, the VAD model and the voice with sizes and the total", async () => {
    await load("?whisper_missing");
    const r = await page(`
      const joined = window.__mock.join();
      const opened = await until(() => dialog.open, 3000);
      return { joined, opened, subject: dialog.dataset.subject, reason: dialog.dataset.reason, title: $("wake-install-title").textContent,
        items: items(), total: $("wake-install-total").textContent, secondary: $("wake-install-secondary").textContent, focus: document.activeElement?.id,
        voice: $("footer-voice").textContent, manual: manual("wake-install-manual"), downloads: calls("whisper_download").length, dialogs: openDialogs() };`);
    expect(r.joined === false && r.voice === "0", `the bot joined: ${JSON.stringify([r.joined, r.voice])}`);
    expect(r.opened && r.subject === "whisper" && r.reason === "join" && r.dialogs === 1, `dialog: ${JSON.stringify(r)}`);
    expect(r.title === "The Whisper mode is not installed", r.title);
    expect(same(r.items, MISSING), `items: ${JSON.stringify(r.items)}`);
    expect(r.total === "228 MB in total.", `total: ${r.total}`);
    expect(r.secondary === "Not now" && r.focus === "wake-install-secondary", `buttons: ${r.secondary}, focus ${r.focus}`);
    expect(r.manual.shown && same(r.manual.files, MISSING_FILES), `by hand: ${JSON.stringify(r.manual)}`);
    expect(r.downloads === 0, "a download started without the OK");
  }],
  ["Not now and Escape close it, download nothing, and the notices stay", async () => {
    await load("?whisper_missing");
    const r = await page(`
      window.__mock.join(); await until(() => dialog.open, 3000);
      $("wake-install-secondary").click(); await sleep(400);
      const notNow = { open: dialog.open, notices: [notice("whisper").shown, notice("tts").shown] };
      window.__mock.join(); await until(() => dialog.open, 3000);
      return { notNow, again: dialog.open };`);
    expect(!r.notNow.open && same(r.notNow.notices, [true, true]), `Not now: ${JSON.stringify(r.notNow)}`);
    expect(r.again, "the second refused join asked nothing");
    await press("Escape");
    const after = await page(`return { open: dialog.open, downloads: calls("whisper_download").length, state: window.__mock.whisperState().kind };`);
    expect(!after.open && after.downloads === 0 && after.state === "needs_download", `Escape: ${JSON.stringify(after)}`);
  }],
  ["Download: both parts' progress in the dialog, closed when installed, the lists say downloaded", async () => {
    await load("?whisper_missing");
    const r = await page(`
      window.__mock.join(); await until(() => dialog.open, 3000);
      const texts = new Set();
      const watch = setInterval(() => texts.add($("wake-install-progress").textContent), 30);
      $("wake-install-primary").click();
      await until(() => $("wake-install-primary").hidden, 3000);
      const during = { secondary: $("wake-install-secondary").textContent, manual: !$("wake-install-manual").hidden };
      const closed = await until(() => !dialog.open, 12000);
      clearInterval(watch); await sleep(600);
      return { during, closed, texts: [...texts], states: [window.__mock.whisperState().kind, window.__mock.ttsState().kind],
        download: calls("whisper_download")[0]?.args, model: $("whisper-model").selectedOptions[0].textContent, voice: $("tts-voice").selectedOptions[0].textContent,
        notices: [notice("whisper").shown, notice("tts").shown] };`);
    expect(r.during.secondary === "Hide" && !r.during.manual, `during: ${JSON.stringify(r.during)}`);
    expect(r.texts.some((text) => /^Downloading the Whisper runtime \(CPU\): \d+ %\nDownloading the Voice engine/.test(text)), `no side-by-side progress: ${JSON.stringify(r.texts.slice(0, 6))}`);
    expect(r.texts.some((text) => text.includes("Checking and unpacking the Silero voice activity model…")), "no unpacking step");
    expect(r.closed && same(r.states, ["idle", "idle"]), `closed ${r.closed}, states ${r.states}`);
    expect(same(r.download, { model: "whisper-base-q8_0", voice: "supertonic3:F1" }), `download ${JSON.stringify(r.download)}`);
    expect(r.model.includes("· downloaded") && r.voice.endsWith("· downloaded"), `lists: ${r.model} / ${r.voice}`);
    expect(same(r.notices, [false, false]), "a notice stayed after the download");
  }],
  ["a failed download: Try again and Close, the links back, and Try again finishes it", async () => {
    await load("?whisper_missing");
    const r = await page(`
      window.__mock.join(); await until(() => dialog.open, 3000);
      window.__mock.failNextWhisperDownload();
      $("wake-install-primary").click();
      await until(() => !$("wake-install-error").hidden, 8000); await sleep(400);
      const failed = { error: $("wake-install-error").textContent, primary: $("wake-install-primary").textContent, secondary: $("wake-install-secondary").textContent,
        manual: manual("wake-install-manual") };
      await until(() => window.__mock.ttsState().kind === "idle", 6000);
      $("wake-install-primary").click();
      const closed = await until(() => !dialog.open, 12000);
      return { failed, closed, states: [window.__mock.whisperState().kind, window.__mock.ttsState().kind] };`);
    expect(r.failed.error.startsWith("The download failed:") && r.failed.error.includes("SHA-256"), r.failed.error);
    expect(r.failed.primary === "Try again" && r.failed.secondary === "Close", JSON.stringify(r.failed));
    expect(r.failed.manual.shown && r.failed.manual.files.length > 0 && r.failed.manual.files.every((file) => MISSING_FILES.includes(file)), `links: ${JSON.stringify(r.failed.manual)}`);
    expect(r.closed && same(r.states, ["idle", "idle"]), `closed ${r.closed}, states ${r.states}`);
  }],
  ["Hide during a download: the notices on the tab show the progress", async () => {
    await load("?whisper_missing");
    const r = await page(`
      window.__mock.join(); await until(() => dialog.open, 3000);
      $("wake-install-primary").click();
      await until(() => $("wake-install-secondary").textContent === "Hide", 3000);
      $("wake-install-secondary").click(); await sleep(300);
      const hidden = { open: dialog.open, whisper: notice("whisper").text, tts: notice("tts").text, button: $("whisper-download-button").hidden };
      await until(() => window.__mock.whisperState().kind === "idle" && window.__mock.ttsState().kind === "idle", 12000);
      return { hidden, states: [window.__mock.whisperState().kind, window.__mock.ttsState().kind] };`);
    expect(!r.hidden.open, "the dialog stayed open");
    expect(/^(Downloading|Checking)/.test(r.hidden.whisper) && /^(Downloading|Checking)/.test(r.hidden.tts) && r.hidden.button, `notices: ${JSON.stringify(r.hidden)}`);
    expect(same(r.states, ["idle", "idle"]), `states ${r.states}`);
  }],
  ["the notice's Download gets everything without the dialog, and Open Whisper models folder opens it", async () => {
    await load("?whisper_missing");
    const r = await page(`
      tab("whisper");
      $("whisper-download-button").click();
      await until(() => window.__mock.whisperState().kind === "idle" && window.__mock.ttsState().kind === "idle", 12000);
      const done = { dialogs: openDialogs(), download: calls("whisper_download")[0]?.args, notices: [notice("whisper").shown, notice("tts").shown] };
      $("open-whisper-models-folder").click(); await sleep(300);
      return { done, folder: calls("open_whisper_models_folder").length, feedback: feedback() };`);
    expect(r.done.dialogs === 0 && same(r.done.download, { model: "whisper-base-q8_0", voice: "supertonic3:F1" }), JSON.stringify(r.done));
    await sleep(300);
    expect(r.folder === 1 && r.feedback === "Opened C:\\VoiceBro\\whisper-models", `folder: ${r.folder} ${r.feedback}`);
  }],
  ["the start never opens it, also with the bot in a channel", async () => {
    await load("?whisper_missing&joined");
    await sleep(1500);
    const r = await page(`return { open: dialog.open, state: window.__mock.whisperState().kind };`);
    expect(!r.open && r.state === "needs_download", JSON.stringify(r));
  }],
  ["switching the mode to Whisper with files missing: saved, the dialog opens, and the Config tab's Vosk fields are disabled", async () => {
    await load();
    const r = await page(`
      tab("whisper");
      await setValue("assistant-mode", "whisper");
      const opened = await until(() => dialog.open, 3000);
      const shown = { reason: dialog.dataset.reason, subject: dialog.dataset.subject, items: items(), total: $("wake-install-total").textContent };
      $("wake-install-secondary").click(); await sleep(300);
      tab("config");
      return { stored: stored().assistant.mode, opened, shown,
        disabled: ["activation-phrases", "deactivation-phrases", "wake-engine", "wake-model"].map((id) => $(id).disabled),
        note: $("assistant-whisper-note").hidden ? "" : $("assistant-whisper-note").textContent, others: [$("session-mode").disabled, $("idle-timeout").disabled] };`);
    expect(r.stored === "whisper", `mode ${r.stored}`);
    expect(r.opened && r.shown.reason === "mode" && r.shown.subject === "whisper", `dialog: ${JSON.stringify(r.shown)}`);
    expect(same(r.shown.items, MISSING) && r.shown.total === "228 MB in total.", `items: ${JSON.stringify(r.shown)}`);
    expect(r.disabled.every(Boolean), `Vosk fields: ${r.disabled}`);
    expect(r.note === "Whisper mode: the phrases are on the Whisper tab.", `note: ${r.note}`);
    expect(!r.others.some(Boolean), "fields of both modes were disabled");
  }],
  ["switching back to realtime enables the Vosk fields with their stored values", async () => {
    await load("?whisper");
    const r = await page(`
      tab("config");
      const before = { disabled: $("wake-model").disabled, note: !$("assistant-whisper-note").hidden };
      tab("whisper"); await setValue("assistant-mode", "realtime"); await sleep(400);
      tab("config");
      return { before, disabled: ["activation-phrases", "deactivation-phrases", "wake-engine", "wake-model"].map((id) => $(id).disabled),
        note: !$("assistant-whisper-note").hidden, values: [$("activation-phrases").value, $("deactivation-phrases").value, $("wake-model").value, $("wake-engine").value],
        wake: window.__mock.wakeState().kind, dialogs: openDialogs() };`);
    expect(r.before.disabled && r.before.note, `in the Whisper mode: ${JSON.stringify(r.before)}`);
    expect(!r.disabled.some(Boolean) && !r.note, `after the switch: ${JSON.stringify(r)}`);
    expect(same(r.values, ["alpha", "omega", "vosk-model-small-ru-0.22", "vosk"]), `values: ${r.values}`);
    expect(r.wake === "ready" && r.dialogs === 0, `wake ${r.wake}, ${r.dialogs} dialogs`);
  }],
  ["choosing a model that is not downloaded: the dialog lists what it needs and the total, nothing saved or fetched", async () => {
    await load("?whisper");
    const r = await page(`
      tab("whisper");
      const saves = calls("save_settings").length;
      await setValue("whisper-model", "whisper-small-q8_0");
      return { open: dialog.open, subject: dialog.dataset.subject, reason: dialog.dataset.reason, items: items(), total: $("wake-install-total").textContent,
        text: $("wake-install-text").textContent, note: $("wake-install-note").textContent, secondary: $("wake-install-secondary").textContent,
        focus: document.activeElement?.id, manual: manual("wake-install-manual"), stored: stored().whisper.model, saves: calls("save_settings").length - saves,
        downloads: calls("whisper_download").length };`);
    expect(r.open && r.subject === "whisper" && r.reason === "choice", `dialog: ${JSON.stringify(r)}`);
    expect(same(r.items, ["Whisper small (q8_0) model — 264 MB from huggingface.co"]) && r.total === "264 MB in total.", `items: ${JSON.stringify([r.items, r.total])}`);
    expect(r.text.includes("small (q8_0)"), r.text);
    expect(r.note.includes("Cancel keeps base (q8_0)") && r.secondary === "Cancel" && r.focus === "wake-install-secondary", `note: ${r.note}`);
    expect(same(r.manual.files, ["ggml-small-q8_0.bin"]), JSON.stringify(r.manual));
    expect(r.stored === "whisper-base-q8_0" && r.saves === 0 && r.downloads === 0, `saved or fetched before Download: ${JSON.stringify(r)}`);
  }],
  ["choice Cancel: the list goes back, nothing saved or downloaded", async () => {
    await load("?whisper");
    const r = await page(`
      tab("whisper");
      await setValue("tts-voice", "kokoro-en:af_bella");
      const opened = dialog.open;
      $("wake-install-secondary").click(); await sleep(400);
      return { opened, open: dialog.open, value: $("tts-voice").value, info: $("tts-voice-info").textContent, stored: stored().tts.voice,
        saves: calls("save_settings").length, downloads: calls("whisper_download").length };`);
    expect(r.opened && !r.open, "the dialog did not open or did not close");
    expect(r.value === "supertonic3:F1" && r.stored === "supertonic3:F1" && r.info.includes("Supertonic"), JSON.stringify(r));
    expect(r.saves === 0 && r.downloads === 0, `saves ${r.saves}, downloads ${r.downloads}`);
  }],
  ["choice Download: saved first, then downloaded, closed when done", async () => {
    await load("?whisper");
    const r = await page(`
      tab("whisper");
      await setValue("whisper-model", "whisper-small-q8_0");
      $("wake-install-primary").click();
      const closed = await until(() => !dialog.open, 12000);
      const order = window.__mock.calls.filter((c) => c.name === "save_settings" || c.name === "whisper_download")
        .map((c) => c.name + ":" + (c.args.model || c.args.patch?.whisper?.model || ""));
      await sleep(500);
      return { closed, order, stored: stored().whisper.model, state: window.__mock.whisperState().kind, label: $("whisper-model").selectedOptions[0].textContent };`);
    const save = r.order.indexOf("save_settings:whisper-small-q8_0");
    const download = r.order.indexOf("whisper_download:whisper-small-q8_0");
    expect(save >= 0 && download > save, `order: ${JSON.stringify(r.order)}`);
    expect(r.closed && r.stored === "whisper-small-q8_0" && r.state === "idle", JSON.stringify(r));
    expect(r.label.startsWith("small (q8_0) · 264 MB · downloaded"), `label: ${r.label}`);
  }],
  ["NVIDIA GPU asks for the GPU runtime before it is saved", async () => {
    await load("?whisper");
    const r = await page(`
      tab("whisper");
      await setValue("whisper-device", "gpu");
      return { open: dialog.open, items: items(), text: $("wake-install-text").textContent, note: $("wake-install-note").textContent, stored: stored().whisper.device };`);
    expect(r.open && same(r.items, ["Whisper runtime (NVIDIA GPU, CUDA 12.4) — 685 MB from github.com"]), `dialog: ${JSON.stringify(r)}`);
    expect(r.text.includes("NVIDIA GPU") && r.note.includes("Cancel keeps CPU") && r.stored === "cpu", JSON.stringify(r));
  }],
  ["a first Piper voice lists the voice and the GPL-3.0 phonemizer as two items, with its source", async () => {
    await load("?whisper");
    const r = await page(`
      tab("whisper");
      await setValue("tts-voice", "piper:de_DE-thorsten-medium");
      return { open: dialog.open, items: items(), manual: manual("wake-install-manual"), info: $("tts-voice-info").textContent };`);
    expect(r.open && same(r.items, [
      "Phonemizer for Piper and Kokoro (piper-phonemize 1.5.9, GPL-3.0) — 0.4 MB from github.com",
      "Piper voice de_DE-thorsten-medium — 67 MB from github.com",
    ]), `items: ${JSON.stringify(r.items)}`);
    expect(r.manual.links.includes("https://github.com/csukuangfj/piper-phonemize/releases/tag/v1.5.9"), `links: ${r.manual.links}`);
    expect(r.info.includes("piper-phonemize (GPL-3.0)") && r.info.includes("CC0"), `info: ${r.info}`);
  }],
  ["a choice on disk saves at once, and in the realtime mode no choice asks", async () => {
    await load("?whisper");
    const onDisk = await page(`
      tab("whisper");
      await setValue("whisper-model", "local:ggml-small-finetuned.bin");
      await setValue("tts-voice", "windows:MSTTS_V110_enUS_DavidM");
      return { open: dialog.open, model: stored().whisper.model, voice: stored().tts.voice, info: $("tts-voice-info").textContent };`);
    expect(!onDisk.open && onDisk.model === "local:ggml-small-finetuned.bin" && onDisk.voice === "windows:MSTTS_V110_enUS_DavidM", JSON.stringify(onDisk));
    expect(onDisk.info.includes("built into Windows"), `info: ${onDisk.info}`);
    await load();
    const realtime = await page(`
      tab("whisper");
      await setValue("whisper-model", "whisper-medium-q5_0");
      await setValue("tts-voice", "piper:pt_BR-faber-medium");
      return { open: dialog.open, model: stored().whisper.model, voice: stored().tts.voice, asked: calls("whisper_requirements").length,
        now: $("whisper-now-recognition").textContent, notices: [notice("whisper").shown, notice("tts").shown] };`);
    expect(!realtime.open && realtime.model === "whisper-medium-q5_0" && realtime.voice === "piper:pt_BR-faber-medium", JSON.stringify(realtime));
    expect(realtime.asked === 0 && same(realtime.notices, [false, false]), `the realtime mode asked: ${JSON.stringify(realtime)}`);
    expect(realtime.now.includes("realtime"), `Mode card: ${realtime.now}`);
  }],

  // ---- Status lines and tiles (settings-ui, status-view)
  ["GPU fallback: the Mode card says GPU requested, CPU used, with the reason, and so does the Status tab", async () => {
    await load("?gpu_fallback");
    const r = await page(`
      tab("whisper");
      return { device: $("whisper-now-device").textContent, recognition: $("whisper-now-recognition").textContent, voice: $("whisper-now-voice").textContent,
        tile: $("status-whisper-device").textContent };`);
    expect(r.device.startsWith("GPU requested, CPU used: no CUDA device"), `Mode card: ${r.device}`);
    expect(r.recognition.startsWith("base (q8_0): loaded"), `recognition: ${r.recognition}`);
    expect(r.voice.startsWith("Supertonic 3 F1 (female): ready"), `voice: ${r.voice}`);
    expect(r.tile === "CPU (GPU requested: no CUDA device)", `tile: ${r.tile}`);
  }],
  ["the last answer's four timings are on the Mode card", async () => {
    await load("?gpu_fallback");
    const r = await page(`return $("whisper-now-last").textContent;`);
    for (const part of ["speech end → text 420 ms", "2.4 s of audio", "text → Codex first words", "first words → voice 180 ms", "speech end → voice"]) {
      expect(r.includes(part), `«${part}» missing: ${r}`);
    }
  }],
  ["the Status tab in the Whisper mode on the GPU: mode, model, GPU (CUDA), the voice, and the wake tile", async () => {
    await load("?gpu_fallback");
    const r = await page(`
      window.__mock.setCuda(true); await sleep(900);
      const tile = (id) => ({ shown: !$(id).closest("p").hidden, text: $(id).textContent });
      return { mode: $("status-mode").textContent, whisper: tile("status-whisper"), device: tile("status-whisper-device"), tts: tile("status-tts"),
        wake: $("status-wake").textContent, now: $("whisper-now-device").textContent };`);
    expect(r.mode === "Whisper", `mode: ${r.mode}`);
    expect(r.whisper.shown && r.whisper.text === "base (q8_0), ready", `Whisper: ${JSON.stringify(r.whisper)}`);
    expect(r.device.shown && r.device.text === "GPU (CUDA)", `device: ${JSON.stringify(r.device)}`);
    expect(r.tts.shown && r.tts.text === "Supertonic 3 F1 (female), ready", `voice: ${JSON.stringify(r.tts)}`);
    expect(r.wake === "off: the Whisper mode spots the phrases", `wake: ${r.wake}`);
    expect(r.now.startsWith("GPU (CUDA): CUDA0"), `Mode card: ${r.now}`);
    await load();
    const realtime = await page(`return { mode: $("status-mode").textContent, hidden: [...document.querySelectorAll("[data-whisper-only]")].every((p) => p.hidden), wake: $("status-wake").textContent };`);
    expect(realtime.mode === "Realtime voice" && realtime.hidden && realtime.wake.startsWith("ready"), `realtime: ${JSON.stringify(realtime)}`);
  }],

  // ---- Preview (settings-ui)
  ["Preview plays the sample in the window only", async () => {
    await load("?whisper");
    const r = await page(`
      tab("whisper");
      await setValue("tts-speed", "1.5");
      const since = window.__mock.calls.length;
      $("tts-preview").click();
      const played = await until(() => { const a = $("tts-preview-audio"); return a.ended || (a.currentTime > 0 && !a.paused); }, 4000);
      const audio = $("tts-preview-audio");
      const names = window.__mock.calls.slice(since).map((c) => c.name).filter((name) => name !== "status");
      return { played, src: audio.src, error: audio.error?.message || "", duration: audio.duration, shown: !audio.hidden,
        args: calls("tts_preview")[0]?.args, names, line: $("tts-preview-state").textContent };`);
    expect(r.src.startsWith("blob:") && r.shown, `audio: ${JSON.stringify(r)}`);
    expect(!r.error && r.duration > 0 && r.played, `the sample did not play: ${JSON.stringify(r)}`);
    expect(same(r.args, { voice: "supertonic3:F1", speed: 1.5 }), `args ${JSON.stringify(r.args)}`);
    expect(same(r.names, ["tts_preview"]), `other commands at Preview: ${r.names}`);
    expect(r.line.includes("window only"), `line: ${r.line}`);
  }],
  ["Preview with a voice not on this PC says why", async () => {
    await load();
    const r = await page(`
      tab("whisper");
      await setValue("tts-voice", "piper:de_DE-thorsten-medium");
      $("tts-preview").click(); await sleep(500);
      const line = $("tts-preview-state");
      return { text: line.textContent, error: line.classList.contains("is-error"), audio: $("tts-preview-audio").hidden, button: $("tts-preview").disabled };`);
    expect(r.text.startsWith("Preview failed: piper:de_DE-thorsten-medium needs a download first") && r.error, `line: ${r.text}`);
    expect(r.audio && !r.button, JSON.stringify(r));
  }],

  // ---- The Codex card (settings-ui)
  ["the Codex card's Voice and Answer handoff say realtime mode only", async () => {
    await load();
    const r = await page(`
      const all = (id) => [...$(id).closest("label").querySelectorAll(".field-help")].map((n) => n.textContent).join(" ");
      return { voice: all("codex-voice"), handoff: all("codex-handoff") };`);
    expect(/realtime mode only/i.test(r.voice), `Voice: ${r.voice}`);
    expect(/realtime mode only/i.test(r.handoff), `Answer handoff: ${r.handoff}`);
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
console.log(failed ? "the Whisper tab check failed" : `all ${scenarios.length} Whisper tab scenarios passed`);
process.exit(failed ? 1 : 0);
