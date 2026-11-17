// Runs every scenario of the Whisper mode's fields on the Config and Settings tabs (change
// `whisper-config-layout`: the settings-ui and app-window deltas) against the command mock in
// a headless Edge and stops at the first failure with its name (exit code 1):
// - no Whisper tab, and the cards of both tabs in order;
// - Config → Assistant: the assistant mode first, the grey Recognition field, the rows of the
//   mode shown, the Whisper model's notice and the phrase fields of each mode;
// - Config → Local voice: the mode note, the voice, Preview and its notice in both modes;
// - Settings → Whisper: the On/Off label, the fields, the notice of the model chosen for the
//   Whisper mode while the realtime mode is on (Download before the switch), the CUDA
//   runtime's notice in the Whisper mode, the GPU fallback and the last answer's timings;
// - the install dialog's Whisper subject: what it lists and when it opens;
// - the Codex card's realtime-only help, the Status tab's tiles, and the folding cards.
// Each scenario loads the page afresh. Needs Node 24 (built-in WebSocket) and Microsoft Edge;
// writes its browser profile under target\. Local only; never shipped.
//
//   node apps/desktop/ui-dev/check-whisper-config.mjs
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
    const setValue = async (id, value, events = ["input", "change"]) => { const c = $(id); c.value = value; for (const e of events) c.dispatchEvent(new Event(e, { bubbles: true })); await sleep(450); };
    const items = () => [...document.querySelectorAll("#wake-install-items li")].map((li) => li.textContent);
    const openDialogs = () => document.querySelectorAll("dialog[open]").length;
    const manual = (id) => ({ shown: !$(id).hidden, files: [...$(id).querySelectorAll("code")].map((c) => c.textContent), links: [...$(id).querySelectorAll("a")].map((a) => a.getAttribute("href")) });
    const notice = (id) => ({ shown: !$(id + "-download").hidden && $(id + "-download").offsetParent !== null, text: $(id + "-download-text").textContent,
      items: [...$(id + "-download-items").children].map((li) => li.textContent), manual: manual(id + "-download-manual"), button: !$(id + "-download-button").hidden });
    const help = (id) => $(id).closest("label, .field").querySelector(".field-help").textContent;
    const shown = (id) => { const label = $(id).closest("label, .wide"); return !label.hidden && $(id).offsetParent !== null; };
    const optionText = (id, value) => [...$(id).options].find((o) => o.value === value)?.textContent;
    const SETTINGS = "settings-whisper";
    const configLine = () => $("config-autosave-feedback").textContent;
    const settingsLine = () => $("settings-feedback").textContent;
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

// What an empty PC misses for the Whisper mode: Whisper's three files, then the voice's two.
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
const CUDA = "Whisper runtime (NVIDIA GPU, CUDA 12.4) — 685 MB from github.com";
const DOWNLOADS = `calls("whisper_download").length + calls("tts_download").length`;

const scenarios = [
  // ---- Tabs and cards (app-window, settings-ui)
  ["no Whisper tab: the tab bar ends with Config", async () => {
    await load("?whisper");
    const r = await page(`return { tabs: [...document.querySelectorAll(".tab-button")].map((b) => b.textContent.trim()), panel: Boolean(document.querySelector('[data-panel="whisper"]')) };`);
    expect(same(r.tabs, ["Console", "Codex chat", "Status", "Performance", "System", "Settings", "Config"]), `tabs: ${r.tabs}`);
    expect(!r.panel, "the Whisper tab's panel is still in the page");
  }],
  ["the cards: Config holds Access, Audio, Assistant, Local voice, Codex and Hotkey; Settings Appearance, Diagnostics and Whisper", async () => {
    await load();
    const r = await page(`return { config: titles("config"), settings: titles("settings"), ids: [card("local-voice")?.closest(".tab-panel").dataset.panel, card("whisper")?.closest(".tab-panel").dataset.panel],
      path: $("config-path").textContent, pathInCard: Boolean($("config-path").closest(".card")) };`);
    expect(same(r.config, ["Discord and Codex", "Access", "Audio", "Assistant", "Local voice", "Codex", "Hotkey"]), `Config: ${r.config}`);
    expect(same(r.settings, ["Appearance", "Diagnostics", "Whisper"]), `Settings: ${r.settings}`);
    expect(same(r.ids, ["config", "settings"]), `card ids: ${r.ids}`);
    expect(r.path.endsWith("settings.json") && !r.pathInCard, `the settings file's line: ${r.path}`);
  }],

  // ---- Config → Assistant (settings-ui)
  ["the Assistant card: the mode first, the grey Recognition field, then the Vosk model, its phrases and Activation below", async () => {
    await load();
    const r = await page(`
      tab("config");
      const body = $("card-assistant-body");
      const order = [...body.querySelectorAll("input, select, textarea")].filter((n) => n.offsetParent !== null).map((n) => n.dataset.path || n.id);
      const rec = $("recognition-engine");
      return { order, rec: { value: rec.value, readOnly: rec.readOnly, grey: getComputedStyle(rec).backgroundColor !== getComputedStyle($("wake-model")).backgroundColor },
        engine: Boolean($("wake-engine")), mode: $("assistant-mode").value, options: [...$("assistant-mode").options].map((o) => o.value), help: help("assistant-mode"),
        note: Boolean($("assistant-whisper-note")) };`);
    expect(same(r.order, ["assistant.mode", "recognition-engine", "assistant.engine", "wake.model", "wake.phrases", "wake.phrases", "session.mode", "session.idle_timeout_seconds"]), `order: ${r.order}`);
    expect(r.rec.value === "Vosk" && r.rec.readOnly && r.rec.grey, `Recognition: ${JSON.stringify(r.rec)}`);
    expect(!r.engine && !r.note, "the wake engine select or the old note is still there");
    expect(r.mode === "realtime" && same(r.options, ["realtime", "whisper"]), `mode: ${r.mode}`);
    for (const part of ["Realtime voice", "OpenAI", "only text goes to OpenAI", "on this PC"]) {
      expect(r.help.includes(part), `the mode help lacks «${part}»: ${r.help}`);
    }
  }],
  ["switching to Whisper on the Config tab: saved at once, Recognition says Whisper, and the Whisper rows replace the Vosk rows; back again shows the Vosk rows with their values", async () => {
    await load();
    const r = await page(`
      window.__mock.completeWhisperDownload(); await sleep(300);
      tab("config");
      await setValue("assistant-mode", "whisper");
      const whisper = { stored: stored().assistant.mode, rec: $("recognition-engine").value, line: configLine(), dialogs: openDialogs(),
        shown: ["whisper-model", "whisper-activation", "whisper-deactivation", "session-mode"].map(shown),
        hidden: ["wake-model", "activation-phrases", "deactivation-phrases", "open-models-folder"].map((id) => !shown(id)),
        phrases: [$("whisper-activation").value, $("whisper-deactivation").value],
        disabled: ["wake-model", "activation-phrases"].map((id) => $(id).disabled),
        order: [...$("card-assistant-body").querySelectorAll("input, select, textarea")].filter((n) => n.offsetParent !== null).map((n) => n.id) };
      await setValue("assistant-mode", "realtime");
      return { whisper, back: { stored: stored().assistant.mode, rec: $("recognition-engine").value,
        values: [$("activation-phrases").value, $("deactivation-phrases").value, $("wake-model").value], shown: ["wake-model", "activation-phrases"].map(shown),
        hidden: ["whisper-model", "whisper-activation"].map((id) => !shown(id)) } };`);
    expect(r.whisper.stored === "whisper" && r.whisper.rec === "Whisper" && r.whisper.line === "Saved: Assistant mode", `switch: ${JSON.stringify(r.whisper)}`);
    expect(r.whisper.dialogs === 0, "a dialog opened although every file is on disk");
    expect(r.whisper.shown.every(Boolean) && r.whisper.hidden.every(Boolean), `rows: ${JSON.stringify(r.whisper)}`);
    expect(same(r.whisper.phrases, ["bro", "peace out"]), `Whisper phrases: ${r.whisper.phrases}`);
    expect(!r.whisper.disabled.some(Boolean), "the Vosk rows were disabled instead of hidden");
    expect(same(r.whisper.order, ["assistant-mode", "recognition-engine", "assistant-engine", "whisper-model", "whisper-activation", "whisper-deactivation", "session-mode", "idle-timeout"]), `order: ${r.whisper.order}`);
    expect(r.back.stored === "realtime" && r.back.rec === "Vosk", `back: ${JSON.stringify(r.back)}`);
    expect(same(r.back.values, ["alpha", "omega", "vosk-model-small-ru-0.22"]) && r.back.shown.every(Boolean) && r.back.hidden.every(Boolean), `back: ${JSON.stringify(r.back)}`);
  }],
  ["phrases per mode: \"hey bro\" set in the Whisper mode is kept, and the realtime mode shows the Vosk model's phrases", async () => {
    await load("?whisper");
    const r = await page(`
      tab("config");
      const helps = [help("whisper-activation"), help("whisper-deactivation"), help("activation-phrases")];
      await setValue("whisper-activation", "hey bro");
      const saved = { whisper: stored().whisper.phrases, wake: stored().wake.phrases, patch: calls("save_settings").at(-1).args.patch };
      await setValue("assistant-mode", "realtime");
      const realtime = [$("activation-phrases").value, $("deactivation-phrases").value, shown("activation-phrases"), stored().whisper.phrases];
      await setValue("assistant-mode", "whisper");
      return { helps, saved, realtime, again: $("whisper-activation").value };`);
    expect(r.helps[0].includes("Whisper mode's own phrases") && r.helps[0].includes('"bro"') && r.helps[1].includes('"peace out"'), `Whisper help: ${r.helps.slice(0, 2)}`);
    expect(r.helps[2].includes("realtime voice mode"), `Vosk help: ${r.helps[2]}`);
    expect(same(r.saved.whisper, { activation: ["hey bro"] }) && same(r.saved.patch, { whisper: { phrases: { activation: ["hey bro"] } } }), `saved: ${JSON.stringify(r.saved)}`);
    expect(same(r.saved.wake["vosk-model-small-ru-0.22"], { activation: ["alpha"], deactivation: ["omega"] }), `the Vosk phrases changed: ${JSON.stringify(r.saved.wake)}`);
    expect(same(r.realtime, ["alpha", "omega", true, { activation: ["hey bro"] }]), `realtime: ${JSON.stringify(r.realtime)}`);
    expect(r.again === "hey bro", `back in the Whisper mode: ${r.again}`);
  }],
  ["the Whisper phrases default to bro and peace out whatever the language", async () => {
    await load("?whisper");
    const r = await page(`
      tab("settings"); await setValue("whisper-language", "de");
      tab("config");
      window.__mock.emit("app-event", { event: "settings_changed" }); await sleep(500);
      return { phrases: [$("whisper-activation").value, $("whisper-deactivation").value], stored: stored().whisper, line: settingsLine() };`);
    expect(r.stored.language === "de" && r.line === "Saved: Language", `language: ${JSON.stringify(r)}`);
    expect(same(r.phrases, ["bro", "peace out"]) && same(r.stored.phrases, {}), `phrases: ${JSON.stringify(r)}`);
  }],
  ["a rejected value shows the app's message under its field and stays to be corrected", async () => {
    await load("?whisper");
    const r = await page(`
      tab("settings"); await setValue("whisper-threads", "65");
      const threads = { message: document.querySelector('.field-message[data-for="whisper-threads"]').textContent,
        invalid: $("whisper-threads").closest("label").classList.contains("is-invalid"), value: $("whisper-threads").value, stored: stored().whisper.threads };
      tab("config"); await setValue("whisper-deactivation", "Bro!");
      return { threads, phrase: document.querySelector('.field-message[data-for="whisper-deactivation"]').textContent, value: $("whisper-deactivation").value, stored: stored().whisper.phrases };`);
    expect(r.threads.message === "whisper.threads: 65 is outside 0 (automatic) to 64" && r.threads.invalid && r.threads.value === "65" && r.threads.stored === 0, JSON.stringify(r.threads));
    expect(r.phrase.startsWith("whisper.phrases.deactivation:") && r.phrase.includes("same phrase"), `phrases: ${r.phrase}`);
    expect(r.value === "Bro!" && same(r.stored, {}), JSON.stringify(r));
  }],
  ["the Whisper model list: four groups, each entry with its size, state and note", async () => {
    await load("?whisper");
    const r = await page(`
      return { groups: [...$("whisper-model").querySelectorAll("optgroup")].map((g) => g.label), base: optionText("whisper-model", "whisper-base-q8_0"),
        small: optionText("whisper-model", "whisper-small-q8_0"), local: optionText("whisper-model", "local:ggml-small-finetuned.bin"), help: help("whisper-model") };`);
    expect(same(r.groups, ["Multilingual", "English only", "Russian-tuned", "Your models"]), `groups: ${r.groups}`);
    expect(r.base === "base (q8_0) · 82 MB · downloaded · fast; weak for Russian", `base: ${r.base}`);
    expect(r.small === "small (q8_0) · 264 MB · to download · balanced; on a CPU about 1–2 s", `small: ${r.small}`);
    expect(r.local.includes("your own model"), `local: ${r.local}`);
    expect(r.help.includes("Settings → Whisper"), `help: ${r.help}`);
  }],
  ["start in the Whisper mode with files missing: no dialog, and the notices of Assistant and Local voice list what is missing", async () => {
    await load("?whisper_missing");
    await sleep(1000);
    const r = await page(`tab("config"); await sleep(200);
      return { dialogs: openDialogs(), whisper: notice("whisper"), tts: notice("tts"), downloads: ${DOWNLOADS}, settings: !$(SETTINGS + "-download").hidden };`);
    expect(r.dialogs === 0, "a dialog opened because the window started");
    expect(!r.settings, "Settings → Whisper repeats the model's notice in the Whisper mode");
    expect(r.whisper.shown && r.whisper.button && same(r.whisper.items, MISSING.slice(0, 3)), `Assistant notice: ${JSON.stringify(r.whisper)}`);
    expect(r.whisper.manual.shown && same(r.whisper.manual.files, MISSING_FILES.slice(0, 3)), `by hand: ${JSON.stringify(r.whisper.manual)}`);
    expect(r.tts.shown && r.tts.button && same(r.tts.items, MISSING.slice(3)), `Local voice notice: ${JSON.stringify(r.tts)}`);
    expect(r.whisper.text.includes("Nothing is downloaded until you press Download"), r.whisper.text);
    expect(r.downloads === 0, "a download started without the OK");
  }],
  ["the Assistant card's Download fetches Whisper's files only", async () => {
    await load("?whisper_missing");
    const r = await page(`
      tab("config");
      $("whisper-download-button").click();
      const progress = await until(() => /^(Downloading|Checking)/.test($("whisper-download-text").textContent), 3000);
      await until(() => window.__mock.whisperState().kind === "idle", 10000); await sleep(600);
      return { progress, calls: [calls("whisper_download").map((c) => c.args), calls("tts_download").length], states: [window.__mock.whisperState().kind, window.__mock.ttsState().kind],
        whisper: notice("whisper"), tts: notice("tts"), label: $("whisper-model").selectedOptions[0].textContent, dialogs: openDialogs() };`);
    expect(r.progress, "no progress under the model");
    expect(same(r.calls, [[{ model: "whisper-base-q8_0" }], 0]), `calls: ${JSON.stringify(r.calls)}`);
    expect(same(r.states, ["idle", "needs_download"]), `states: ${r.states}`);
    expect(!r.whisper.shown && r.tts.shown, `notices: ${JSON.stringify([r.whisper.shown, r.tts.shown])}`);
    expect(r.label.includes("· downloaded") && r.dialogs === 0, `label ${r.label}, ${r.dialogs} dialogs`);
  }],

  // ---- Config → Local voice (settings-ui)
  ["the Local voice card: its note follows the mode, and its fields and Download work in the realtime mode", async () => {
    await load();
    const r = await page(`
      tab("config");
      const note = $("local-voice-note").textContent;
      const before = notice("tts");
      const texts = new Set(); const watch = setInterval(() => texts.add($("tts-download-text").textContent), 30);
      $("tts-download-button").click();
      await until(() => !window.__mock.ttsState().kind.match(/downloading|installing/) && calls("tts_download").length > 0 && $("tts-download").hidden, 10000);
      clearInterval(watch); await sleep(500);
      const after = { state: window.__mock.ttsState().kind, notice: notice("tts").shown,
        label: $("tts-voice").selectedOptions[0].textContent, calls: [calls("tts_download").map((c) => c.args), calls("whisper_download").length], dialogs: openDialogs() };
      await setValue("assistant-mode", "whisper"); await until(() => dialog.open, 2000);
      return { note, before, texts: [...texts], after, whisperNote: $("local-voice-note").textContent };`);
    expect(r.note.includes("Not used in the realtime voice mode") && r.note.includes("Codex → Voice"), `realtime note: ${r.note}`);
    expect(r.before.shown && same(r.before.items, MISSING.slice(3)), `before: ${JSON.stringify(r.before)}`);
    expect(r.texts.some((text) => text.startsWith("Downloading the Voice engine")), `no progress: ${JSON.stringify(r.texts)}`);
    expect(r.after.state === "off" && !r.after.notice && r.after.label === "Supertonic 3 F1 (female) · downloaded", `after: ${JSON.stringify(r.after)}`);
    expect(same(r.after.calls, [[{ voice: "supertonic3:F1" }], 0]) && r.after.dialogs === 0, `calls: ${JSON.stringify(r.after)}`);
    expect(r.whisperNote === "This voice reads Codex's answers into the voice channel.", `Whisper note: ${r.whisperNote}`);
  }],
  ["a voice choice in the realtime mode saves at once and its card lists what it misses", async () => {
    await load();
    const r = await page(`
      tab("config");
      await setValue("tts-voice", "piper:de_DE-thorsten-medium"); await sleep(300);
      return { stored: stored().tts.voice, dialogs: openDialogs(), line: configLine(), tts: notice("tts"), info: $("tts-voice-info").textContent, asked: calls("whisper_requirements").length };`);
    expect(r.stored === "piper:de_DE-thorsten-medium" && r.dialogs === 0 && r.line === "Saved: Voice", JSON.stringify(r));
    expect(same(r.tts.items, [
      "Voice engine (sherpa-onnx 2.0.1) — 8 MB from github.com",
      "Phonemizer for Piper and Kokoro (piper-phonemize 1.5.9, GPL-3.0) — 0.4 MB from github.com",
      "Piper voice de_DE-thorsten-medium — 67 MB from github.com",
    ]), `items: ${JSON.stringify(r.tts.items)}`);
    expect(r.tts.manual.links.includes("https://github.com/csukuangfj/piper-phonemize/releases/tag/v1.5.9"), `links: ${r.tts.manual.links}`);
    expect(r.info.includes("piper-phonemize (GPL-3.0)") && r.info.includes("CC0") && r.asked === 0, `info: ${r.info}`);
  }],
  ["Preview plays the sample in the window only", async () => {
    await load("?whisper");
    const r = await page(`
      tab("config");
      await setValue("tts-speed", "1.5");
      const since = window.__mock.calls.length;
      $("tts-preview").click();
      const played = await until(() => { const a = $("tts-preview-audio"); return a.ended || (a.currentTime > 0 && !a.paused); }, 4000);
      const audio = $("tts-preview-audio");
      return { played, src: audio.src, error: audio.error?.message || "", args: calls("tts_preview")[0]?.args,
        names: window.__mock.calls.slice(since).map((c) => c.name).filter((name) => name !== "status"), line: $("tts-preview-state").textContent };`);
    expect(r.src.startsWith("blob:") && !r.error && r.played, `the sample did not play: ${JSON.stringify(r)}`);
    expect(same(r.args, { voice: "supertonic3:F1", speed: 1.5 }), `args ${JSON.stringify(r.args)}`);
    expect(same(r.names, ["tts_preview"]) && r.line.includes("window only"), `at Preview: ${JSON.stringify(r)}`);
  }],
  ["Preview with a voice not on this PC says why", async () => {
    await load();
    const r = await page(`
      tab("config");
      $("tts-preview").click(); await sleep(500);
      return { text: $("tts-preview-state").textContent, error: $("tts-preview-state").classList.contains("is-error"), audio: $("tts-preview-audio").hidden };`);
    expect(r.text.startsWith("Preview failed: supertonic3:F1 needs a download first") && r.error && r.audio, JSON.stringify(r));
  }],

  // ---- Settings → Whisper (settings-ui)
  ["Settings → Whisper is Off in the realtime mode, says where to switch it on, and its fields still save", async () => {
    await load();
    const r = await page(`
      tab("settings");
      const label = $("whisper-onoff").textContent;
      await setValue("whisper-threads", "4"); const threads = [stored().whisper.threads, settingsLine()];
      await setValue("whisper-language", "fr"); const language = [stored().whisper.language, settingsLine()];
      return { label, threads, language, now: $("whisper-now-recognition").textContent, rows: [...document.querySelectorAll("[data-whisper-row]")].every((row) => row.hidden),
        help: [help("whisper-device"), help("whisper-threads"), help("whisper-language")].every((text) => text.length > 30) };`);
    expect(r.label === "Off: turn it on in Config → Assistant → Assistant mode.", `label: ${r.label}`);
    expect(same(r.threads, [4, "Saved: CPU threads"]) && same(r.language, ["fr", "Saved: Language"]), JSON.stringify(r));
    expect(r.now.startsWith("Off: nothing is loaded") && r.rows && r.help, `in use: ${r.now}`);
  }],
  ["Settings → Whisper is On in the Whisper mode", async () => {
    await load("?whisper");
    const r = await page(`return { label: $("whisper-onoff").textContent, on: $("whisper-onoff").classList.contains("is-on") };`);
    expect(r.label === "On: the assistant runs in the Whisper mode." && r.on, JSON.stringify(r));
  }],
  ["the realtime mode with nothing of Whisper downloaded: the card names base (q8_0) and lists the runtime, the model and the VAD model; Download fetches them, and the mode stays realtime with nothing loaded", async () => {
    await load();
    const r = await page(`
      tab("settings");
      const before = { ...notice(SETTINGS), total: $("settings-whisper-download-total").textContent, downloads: ${DOWNLOADS} };
      $("settings-whisper-download-button").click();
      const progress = await until(() => /^(Downloading|Checking)/.test($("settings-whisper-download-text").textContent), 3000);
      await until(() => $("settings-whisper-download").hidden, 10000); await sleep(400);
      const after = { shown: notice(SETTINGS).shown, mode: stored().assistant.mode, state: window.__mock.whisperState().kind,
        now: $("whisper-now-recognition").textContent, calls: [calls("whisper_download").map((c) => c.args), calls("tts_download").length],
        files: window.__mock.haveFiles(), voice: notice("tts") };
      tab("config"); await setValue("assistant-mode", "whisper");
      await until(() => dialog.open, 3000);
      return { before, progress, after, switched: { open: dialog.open, items: items(), whisperNotice: notice("whisper").shown } };`);
    expect(r.before.shown && r.before.text.startsWith("The Whisper mode needs these files for base (q8_0), the model chosen for it (choose another in Config → Assistant once the mode is Whisper)."), `text: ${r.before.text}`);
    expect(same(r.before.items, MISSING.slice(0, 3)) && r.before.total === "92 MB in total." && r.before.button, `items: ${JSON.stringify(r.before)}`);
    expect(r.before.manual.shown && same(r.before.manual.files, MISSING_FILES.slice(0, 3)) && r.before.downloads === 0, `by hand: ${JSON.stringify(r.before.manual)}`);
    expect(r.progress && !r.after.shown, `the notice did not go: ${JSON.stringify(r.after)}`);
    expect(r.after.mode === "realtime" && r.after.state === "off" && r.after.now.startsWith("Off: nothing is loaded"), `after: ${JSON.stringify(r.after)}`);
    expect(same(r.after.calls, [[{ model: "whisper-base-q8_0" }], 0]) && MISSING_FILES.slice(0, 3).every((file) => r.after.files.includes(file)), `calls: ${JSON.stringify(r.after)}`);
    expect(same(r.switched.items, MISSING.slice(3)) && !r.switched.whisperNotice, `the later switch asked for Whisper's files again: ${JSON.stringify(r.switched)}`);
  }],
  ["NVIDIA GPU chosen with only the CPU runtime: the card shows the CUDA runtime, 685 MB from github.com, and nothing is fetched until Download", async () => {
    await load();
    const r = await page(`
      window.__mock.completeWhisperDownload(); await sleep(300);
      tab("settings");
      const cpu = notice(SETTINGS).shown;
      await setValue("whisper-device", "gpu"); await sleep(300);
      const gpu = { stored: stored().whisper.device, dialogs: openDialogs(), cuda: notice(SETTINGS), downloads: ${DOWNLOADS} };
      $("settings-whisper-download-button").click();
      const progress = await until(() => $("settings-whisper-download-text").textContent.startsWith("Downloading the Whisper runtime (NVIDIA GPU"), 3000);
      await until(() => $("settings-whisper-download").hidden, 10000); await sleep(300);
      return { cpu, gpu, progress, after: { shown: notice(SETTINGS).shown, state: window.__mock.whisperState().kind, calls: calls("whisper_download").map((c) => c.args),
        missing: window.__mock.whisperRequirements("whisper-base-q8_0", "gpu", "supertonic3:F1").length } };`);
    expect(!r.cpu, "a notice shows while nothing is missing");
    expect(r.gpu.stored === "gpu" && r.gpu.dialogs === 0, `the choice: ${JSON.stringify(r.gpu)}`);
    expect(r.gpu.cuda.shown && same(r.gpu.cuda.items, [CUDA]) && r.gpu.cuda.button, `CUDA notice: ${JSON.stringify(r.gpu.cuda)}`);
    expect(r.gpu.downloads === 0, "something was fetched before Download");
    expect(r.progress && !r.after.shown && r.after.state === "off" && r.after.missing === 0, `after Download: ${JSON.stringify(r.after)}`);
    expect(same(r.after.calls, [{ model: "whisper-base-q8_0" }]), `calls: ${JSON.stringify(r.after.calls)}`);
  }],
  ["in the Whisper mode the card shows only the CUDA runtime under the device, and the model's notice is in Config → Assistant", async () => {
    await load();
    const r = await page(`
      window.__mock.completeWhisperDownload(); await sleep(300);
      tab("settings"); await setValue("whisper-device", "gpu");
      const realtime = notice(SETTINGS).text;
      tab("config"); await setValue("assistant-mode", "whisper");
      await until(() => dialog.open, 3000); const asked = items();
      $("wake-install-secondary").click(); await sleep(300);
      const assistant = notice("whisper");
      tab("settings"); await sleep(200);
      const cuda = notice(SETTINGS);
      await setValue("whisper-device", "cpu"); await sleep(300);
      return { realtime, asked, assistant, cuda, cpu: notice(SETTINGS).shown, notices: document.querySelectorAll('[data-panel="settings"] .download-notice:not([hidden])').length };`);
    expect(r.realtime.startsWith("The Whisper mode needs these files for base (q8_0)"), `realtime: ${r.realtime}`);
    expect(same(r.asked, [CUDA]), `dialog: ${JSON.stringify(r.asked)}`);
    expect(r.assistant.shown && same(r.assistant.items, [CUDA]), `Assistant notice: ${JSON.stringify(r.assistant)}`);
    expect(r.cuda.shown && same(r.cuda.items, [CUDA]) && r.cuda.text.startsWith("NVIDIA GPU needs Whisper's CUDA runtime"), `Settings notice: ${JSON.stringify(r.cuda)}`);
    expect(!r.cpu && r.notices === 0, `with the CPU: ${JSON.stringify([r.cpu, r.notices])}`);
  }],
  ["GPU fallback: the card's line says GPU requested, CPU used, with the reason, and the last answer's four timings", async () => {
    await load("?gpu_fallback");
    const r = await page(`tab("settings"); return { device: $("whisper-now-device").textContent, recognition: $("whisper-now-recognition").textContent,
      voice: $("whisper-now-voice").textContent, last: $("whisper-now-last").textContent, tile: $("status-whisper-device").textContent };`);
    expect(r.device.startsWith("GPU requested, CPU used: no CUDA device"), `device: ${r.device}`);
    expect(r.recognition.startsWith("base (q8_0): loaded") && r.voice.startsWith("Supertonic 3 F1 (female): ready"), JSON.stringify(r));
    for (const part of ["speech end → text 420 ms", "2.4 s of audio", "text → Codex first words", "first words → voice 180 ms", "speech end → voice"]) {
      expect(r.last.includes(part), `«${part}» missing: ${r.last}`);
    }
    expect(r.tile === "CPU (GPU requested: no CUDA device)", `Status tile: ${r.tile}`);
  }],
  ["the Status tab in the Whisper mode on the GPU: mode, model, GPU (CUDA), the voice and the wake tile", async () => {
    await load("?gpu_fallback");
    const r = await page(`
      window.__mock.setCuda(true); await sleep(900);
      return { mode: $("status-mode").textContent, whisper: $("status-whisper").textContent, device: $("status-whisper-device").textContent,
        tts: $("status-tts").textContent, wake: $("status-wake").textContent };`);
    expect(same(r, { mode: "Whisper", whisper: "base (q8_0), ready", device: "GPU (CUDA)", tts: "Supertonic 3 F1 (female), ready", wake: "off: the Whisper mode spots the phrases" }), JSON.stringify(r));
  }],
  ["the Codex card's Voice and Answer handoff say realtime voice mode only", async () => {
    await load();
    const r = await page(`
      const all = (id) => [...$(id).closest("label").querySelectorAll(".field-help")].map((n) => n.textContent).join(" ");
      return { voice: all("codex-voice"), handoff: all("codex-handoff") };`);
    expect(r.voice.includes("realtime voice mode only") && !r.voice.includes("Whisper tab"), `Voice: ${r.voice}`);
    expect(r.handoff.includes("Realtime voice mode only"), `Answer handoff: ${r.handoff}`);
  }],

  // ---- The install dialog's Whisper subject (app-window)
  ["a refused /join: the dialog lists the runtime, the model, the VAD model and the voice with sizes and the total, and no text names a Whisper tab", async () => {
    await load("?whisper_missing");
    const r = await page(`
      const joined = window.__mock.join();
      const opened = await until(() => dialog.open, 3000);
      return { joined, opened, subject: dialog.dataset.subject, reason: dialog.dataset.reason, items: items(), total: $("wake-install-total").textContent,
        text: $("wake-install-text").textContent, note: $("wake-install-note").textContent, secondary: $("wake-install-secondary").textContent,
        focus: document.activeElement?.id, voice: $("footer-voice").textContent, manual: manual("wake-install-manual"), downloads: ${DOWNLOADS},
        whisperTab: /Whisper tab/i.test(document.body.textContent) };`);
    expect(r.joined === false && r.voice === "0", `the bot joined: ${JSON.stringify([r.joined, r.voice])}`);
    expect(r.opened && r.subject === "whisper" && r.reason === "join", `dialog: ${JSON.stringify(r)}`);
    expect(same(r.items, MISSING) && r.total === "228 MB in total.", `items: ${JSON.stringify([r.items, r.total])}`);
    expect(r.note.includes("Config → Assistant") && r.note.includes("Config → Local voice"), `note: ${r.note}`);
    expect(r.secondary === "Not now" && r.focus === "wake-install-secondary", `buttons: ${r.secondary}, focus ${r.focus}`);
    expect(r.manual.shown && same(r.manual.files, MISSING_FILES) && r.downloads === 0, JSON.stringify(r.manual));
    expect(!r.whisperTab, "a text still names the Whisper tab");
  }],
  ["Not now and Escape close it, download nothing, and the notices stay", async () => {
    await load("?whisper_missing");
    const r = await page(`
      window.__mock.join(); await until(() => dialog.open, 3000);
      $("wake-install-secondary").click(); await sleep(400);
      const notNow = { open: dialog.open, notices: [notice("whisper").shown, notice("tts").shown] };
      tab("config"); await sleep(200);
      const shownOnTab = [notice("whisper").shown, notice("tts").shown];
      window.__mock.join(); await until(() => dialog.open, 3000);
      return { notNow, shownOnTab, again: dialog.open };`);
    expect(!r.notNow.open && same(r.shownOnTab, [true, true]) && r.again, JSON.stringify(r));
    await press("Escape");
    const after = await page(`return { open: dialog.open, downloads: ${DOWNLOADS} };`);
    expect(!after.open && after.downloads === 0, `Escape: ${JSON.stringify(after)}`);
  }],
  ["Download in the dialog asks for both parts, shows both progresses, and closes when installed", async () => {
    await load("?whisper_missing");
    const r = await page(`
      window.__mock.join(); await until(() => dialog.open, 3000);
      const texts = new Set(); const watch = setInterval(() => texts.add($("wake-install-progress").textContent), 30);
      $("wake-install-primary").click();
      await until(() => $("wake-install-primary").hidden, 3000);
      const secondary = $("wake-install-secondary").textContent;
      const closed = await until(() => !dialog.open, 12000);
      clearInterval(watch); await sleep(600);
      tab("config"); await sleep(200);
      return { secondary, closed, texts: [...texts], calls: [calls("whisper_download").map((c) => c.args), calls("tts_download").map((c) => c.args)],
        states: [window.__mock.whisperState().kind, window.__mock.ttsState().kind], notices: [notice("whisper").shown, notice("tts").shown],
        labels: [$("whisper-model").selectedOptions[0].textContent, $("tts-voice").selectedOptions[0].textContent] };`);
    expect(r.secondary === "Hide" && r.closed, `dialog: ${JSON.stringify([r.secondary, r.closed])}`);
    expect(same(r.calls, [[{ model: "whisper-base-q8_0" }], [{ voice: "supertonic3:F1" }]]), `calls: ${JSON.stringify(r.calls)}`);
    expect(r.texts.some((text) => /^Downloading the Whisper runtime \(CPU\): \d+ %\nDownloading the Voice engine/.test(text)), `progress: ${JSON.stringify(r.texts.slice(0, 5))}`);
    expect(same(r.states, ["idle", "idle"]) && same(r.notices, [false, false]), `after: ${JSON.stringify(r)}`);
    expect(r.labels[0].includes("· downloaded") && r.labels[1].endsWith("· downloaded"), `lists: ${r.labels}`);
  }],
  ["a failed download: Try again and Close, the links back, and Try again finishes it", async () => {
    await load("?whisper_missing");
    const r = await page(`
      window.__mock.join(); await until(() => dialog.open, 3000);
      window.__mock.failNextWhisperDownload();
      $("wake-install-primary").click();
      await until(() => !$("wake-install-error").hidden, 8000); await sleep(300);
      const failed = { error: $("wake-install-error").textContent, primary: $("wake-install-primary").textContent, secondary: $("wake-install-secondary").textContent,
        manual: manual("wake-install-manual") };
      $("wake-install-primary").click();
      const closed = await until(() => !dialog.open, 12000);
      return { failed, closed, states: [window.__mock.whisperState().kind, window.__mock.ttsState().kind] };`);
    expect(r.failed.error.startsWith("The download failed:") && r.failed.error.includes("SHA-256"), r.failed.error);
    expect(r.failed.primary === "Try again" && r.failed.secondary === "Close", JSON.stringify(r.failed));
    expect(r.failed.manual.shown && r.failed.manual.files.length > 0 && r.failed.manual.files.every((file) => MISSING_FILES.includes(file)), `links: ${JSON.stringify(r.failed.manual)}`);
    expect(r.closed && same(r.states, ["idle", "idle"]), `closed ${r.closed}, states ${r.states}`);
  }],
  ["the start never opens it, also with the bot in a channel", async () => {
    await load("?whisper_missing&joined");
    await sleep(1500);
    const r = await page(`return { open: dialog.open, state: window.__mock.whisperState().kind };`);
    expect(!r.open && r.state === "needs_download", JSON.stringify(r));
  }],
  ["switching to Whisper with files missing: saved, then the dialog lists both parts' files", async () => {
    await load();
    const r = await page(`
      tab("config");
      await setValue("assistant-mode", "whisper");
      const opened = await until(() => dialog.open, 3000);
      return { stored: stored().assistant.mode, opened, reason: dialog.dataset.reason, items: items(), total: $("wake-install-total").textContent };`);
    expect(r.stored === "whisper" && r.opened && r.reason === "mode", JSON.stringify(r));
    expect(same(r.items, MISSING) && r.total === "228 MB in total.", `items: ${JSON.stringify(r.items)}`);
  }],
  ["a model choice in the Whisper mode asks first; Cancel keeps the stored model", async () => {
    await load("?whisper");
    const r = await page(`
      tab("config");
      await setValue("whisper-model", "whisper-small-q8_0");
      const asked = { open: dialog.open, reason: dialog.dataset.reason, items: items(), total: $("wake-install-total").textContent, note: $("wake-install-note").textContent,
        stored: stored().whisper.model, saves: calls("save_settings").length, downloads: ${DOWNLOADS} };
      $("wake-install-secondary").click(); await sleep(400);
      return { asked, open: dialog.open, value: $("whisper-model").value, stored: stored().whisper.model };`);
    expect(r.asked.open && r.asked.reason === "choice", `dialog: ${JSON.stringify(r.asked)}`);
    expect(same(r.asked.items, ["Whisper small (q8_0) model — 264 MB from huggingface.co"]) && r.asked.total === "264 MB in total.", JSON.stringify(r.asked));
    expect(r.asked.note.includes("Cancel keeps base (q8_0)") && r.asked.stored === "whisper-base-q8_0" && r.asked.saves === 0 && r.asked.downloads === 0, JSON.stringify(r.asked));
    expect(!r.open && r.value === "whisper-base-q8_0" && r.stored === "whisper-base-q8_0", `Cancel: ${JSON.stringify(r)}`);
  }],
  ["choice Download: saved first, then downloaded, closed when done", async () => {
    await load("?whisper");
    const r = await page(`
      tab("config");
      await setValue("whisper-model", "whisper-small-q8_0");
      $("wake-install-primary").click();
      const closed = await until(() => !dialog.open, 12000);
      const order = window.__mock.calls.filter((c) => ["save_settings", "whisper_download"].includes(c.name))
        .map((c) => c.name + ":" + (c.args.model || c.args.patch?.whisper?.model || ""));
      return { closed, order, stored: stored().whisper.model, state: window.__mock.whisperState().kind };`);
    const save = r.order.indexOf("save_settings:whisper-small-q8_0");
    const download = r.order.indexOf("whisper_download:whisper-small-q8_0");
    expect(save >= 0 && download > save, `order: ${JSON.stringify(r.order)}`);
    expect(r.closed && r.stored === "whisper-small-q8_0" && r.state === "idle", JSON.stringify(r));
  }],
  ["a device choice in the Whisper mode asks for the GPU runtime; a first Piper voice for the phonemizer and the voice", async () => {
    await load("?whisper");
    const gpu = await page(`
      tab("settings"); await setValue("whisper-device", "gpu");
      const r = { open: dialog.open, items: items(), text: $("wake-install-text").textContent, stored: stored().whisper.device };
      $("wake-install-secondary").click(); await sleep(300);
      return { ...r, value: $("whisper-device").value };`);
    expect(gpu.open && same(gpu.items, [CUDA]) && gpu.text.includes("NVIDIA GPU") && gpu.stored === "cpu" && gpu.value === "cpu", JSON.stringify(gpu));
    const voice = await page(`
      tab("config"); await setValue("tts-voice", "piper:de_DE-thorsten-medium");
      return { open: dialog.open, items: items(), stored: stored().tts.voice };`);
    expect(voice.open && same(voice.items, [
      "Phonemizer for Piper and Kokoro (piper-phonemize 1.5.9, GPL-3.0) — 0.4 MB from github.com",
      "Piper voice de_DE-thorsten-medium — 67 MB from github.com",
    ]) && voice.stored === "supertonic3:F1", JSON.stringify(voice));
  }],
  ["in the realtime mode no choice opens the dialog: each saves at once", async () => {
    await load();
    const r = await page(`
      tab("settings"); await setValue("whisper-device", "gpu");
      tab("config"); await setValue("tts-voice", "kokoro-en:af_bella");
      return { dialogs: openDialogs(), stored: [stored().whisper.device, stored().tts.voice], asked: calls("whisper_requirements").length, downloads: ${DOWNLOADS} };`);
    expect(r.dialogs === 0 && same(r.stored, ["gpu", "kokoro-en:af_bella"]) && r.asked === 0 && r.downloads === 0, JSON.stringify(r));
  }],

  // ---- Folding cards and tabs (settings-ui, app-window)
  ["Local voice and Settings → Whisper fold and come back folded; the removed tab's card ids are harmless", async () => {
    await load();
    const list = await page(`tab("config"); await toggle("local-voice"); tab("settings"); await toggle("whisper"); return stored().ui.collapsed_cards;`);
    expect(same(list, ["whisper", "local-voice"]), `stored ${JSON.stringify(list)}`);
    await load("?collapsed=whisper-voice,whisper-mode,local-voice,whisper");
    const r = await page(`
      const state = ["local-voice", "whisper", "assistant", "appearance"].map((id) => collapsed(id));
      tab("config"); await toggle("hotkey");
      return { state, stored: stored().ui.collapsed_cards };`);
    expect(same(r.state, [true, true, false, false]), `after a restart: ${r.state}`);
    expect(same(r.stored, ["whisper-voice", "whisper-mode", "whisper", "local-voice", "hotkey"]), `stored ${JSON.stringify(r.stored)}`);
  }],
  ["switching tabs keeps the Settings tab's scroll position and an unsaved value", async () => {
    await load();
    const r = await page(`
      tab("settings");
      const panel = document.querySelector('[data-panel="settings"]');
      $("whisper-threads").value = "7"; $("whisper-threads").dispatchEvent(new Event("input", { bubbles: true }));
      panel.scrollTop = 250; await sleep(100); const top = panel.scrollTop;
      tab("status"); await sleep(1200); tab("settings"); await sleep(200);
      return { top, back: panel.scrollTop, value: $("whisper-threads").value, dirty: $("whisper-threads").closest("label").classList.contains("is-dirty"), stored: stored().whisper.threads };`);
    expect(r.top > 0 && r.back === r.top, `scroll ${r.top} → ${r.back}`);
    expect(r.value === "7" && r.dirty && r.stored === 0, JSON.stringify(r));
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
console.log(failed ? "the Whisper config check failed" : `all ${scenarios.length} Whisper config scenarios passed`);
process.exit(failed ? 1 : 0);
