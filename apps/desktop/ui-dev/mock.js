// A stand-in for the app's Tauri commands and events, so the real UI (`../ui/main.js`,
// `../ui/styles.css`) can be checked in a browser without the app. It keeps a settings
// document in memory with the app's validation messages for the common cases, answers the
// commands the page calls with the same names and payload shapes, and emits status events
// every second. It is outside `frontendDist` and never ships.
//
// `window.__mock` exposes what a check needs: `calls` (every command with its arguments),
// `settings()` (the stored document), `changeCapabilities()` (the engine offers a new
// model list, as after a sign-in), `capabilities()`, `failNextCheck(message)` (the next
// Check Codex or Refresh lists fails) and `emit(name, payload)`. `?nolists` starts with no
// Codex lists ever read. `?disconnected` starts with the bot not connected
// (`setConnected(on)` changes it); `?post_answers=<value>&answers_channel=<id>` starts with
// that answer posting stored. The owner link (change `owner-only-access`): `?owner=linked`
// starts with Alex linked, `?owner_only` with the switch on; `completeLink(name)` sends the
// waiting code from Discord, `wrongCode()` a wrong one, `expireCode()` lets it expire, and
// `owner()` is the link state the status carries.
//
// The Whisper mode (change `whisper-mode`): `?whisper` starts in the Whisper mode with its
// files on disk and the bot in no channel (idle; add `&joined` for a bot in a channel),
// `?whisper_missing` in the Whisper mode with nothing downloaded, `?gpu_fallback` with
// Whisper loaded in a channel, the GPU asked for and the CPU used ("no CUDA device"), and one
// measured answer. Without them nothing of Whisper is on disk and the mode is realtime. Both
// statuses carry `missing` (what the selected model and device, or voice, still need) in both
// modes, and `whisper_download({model})` and `tts_download({voice})` work in both modes (change
// `whisper-config-layout`); in the realtime mode a download ends `off` with nothing missing. A
// `join()` in the Whisper mode with files missing is refused as the app refuses it: the bot
// stays out, the parts ask for the download, and the refusal is logged as an event.
// `whisperState()`, `ttsState()`, `whisperRequirements(model, device, voice)`,
// `failNextWhisperDownload()` (the next download stops at 40 % with a hash error),
// `completeWhisperDownload()` (everything the selection needs is on disk at once),
// `placeWhisperFiles(files)` (put there by hand), `setCuda(on)`, `whisperTurn()` (one
// measured answer now) and `haveFiles()` serve the checks.
(() => {
  const THEMES = ["dark", "darcula", "light", "intellij"];
  const clone = (value) => JSON.parse(JSON.stringify(value));
  let settings = {
    version: 2,
    discord: { guild_id: "1091401169441656933", owner_only: false },
    session: { mode: "wake_word", idle_timeout_seconds: 60, earcons: true, earcon_volume: 0.18 },
    audio: { input_gain: 1, output_gain: 1 },
    // Version 2: the phrases of each model under its id.
    wake: {
      engine: "vosk",
      model: "vosk-model-small-ru-0.22",
      phrases: { "vosk-model-small-ru-0.22": { activation: ["alpha"], deactivation: ["omega"] } },
    },
    // `post_answers` and `answers_channel` as the app fills them for a file without them;
    // `mode`, `whisper` and `tts` likewise (change `whisper-mode`).
    assistant: { engine: "codex", mode: "realtime", speaker_labels: "start", save_chat_history: false, post_answers: "off", answers_channel: "" },
    whisper: { model: "whisper-base-q8_0", device: "cpu", threads: 0, language: "auto", phrases: {} },
    tts: { voice: "supertonic3:F1", speed: 1.0 },
    // `handoff_mode` and `refresh_lists_at_start` as the app fills them for a file without
    // the fields: the defaults.
    codex: { executable: "", model: "", reasoning_effort: "", voice: "", instructions: "Answer briefly.", handoff_mode: "items", refresh_lists_at_start: false },
    hotkey: { mute: "Ctrl+Alt+M" },
    ui: { theme: "dark", font_size: 13, collapsed_cards: [] },
    // All off and libopus, as the app fills them for a file without the section.
    diagnostics: { performance: false, memory: false, verbose_log: false, opus_codec: "libopus" },
  };
  // The Opus codecs and their labels, as `OpusCodec::label` gives them.
  const CODEC_LABELS = { libopus: "libopus 1.5", "opus-rs": "opus-rs 0.1.42" };
  let tokenSet = true;
  let muted = false;
  // Codex as the status reports it, and the install offer: `__mock.setCodex(...)` switches.
  let codex = { installed: true, signedIn: true, accountRead: true, winget: true, nextExitCode: 0 };
  let install = { kind: "idle" };
  let capabilities = {
    models: [
      { id: "model-a", display_name: "Model A", description: "", reasoning_efforts: ["low", "medium", "high"], default_reasoning_effort: "medium", is_default: true },
      { id: "model-b", display_name: "Model B", description: "", reasoning_efforts: ["low", "high"], default_reasoning_effort: "low", is_default: false },
    ],
    voices: ["cove", "juniper", "maple"],
    default_voice: "cove",
    signed_in: true,
    // The saved lists were read on 2026-11-14 at 18:30 local time.
    lists_read_at_ms: new Date(2026, 10, 14, 18, 30).getTime(),
  };
  // `?nolists`: nothing was ever read, as on a PC without Codex (the lists are free text).
  if (new URLSearchParams(location.search).has("nolists")) {
    capabilities = { models: [], voices: [], default_voice: null, signed_in: false, lists_read_at_ms: null };
  }
  // `__mock.failNextCheck(message)`: the next Check Codex (or Refresh lists) fails with it.
  let failNextCheck = null;
  // `__mock.nextCheckAddsModel()`: Codex offers a new model at the next Check.
  let checkAddsModel = false;
  // As `wake_models` answers: the catalog with its download state, then the models folder.
  // `asset` is the mock's own: the label the app gives the model's archive.
  // Each model's default phrases (activation | deactivation), as the catalog carries them.
  const catalog = [
    ["vosk-model-small-en-us-0.15", "en-US", "English (US) 0.15", "English", 41205931, ["bro"], ["peace out"]],
    ["vosk-model-small-ru-0.22", "ru", "Russian 0.22", "Russian", 46220000, ["братан"], ["отбой"]],
    ["vosk-model-small-cs-0.4-rhasspy", "cs", "Czech 0.4", "Czech", 46088666, ["brácha"], ["čau"], "MIT"],
    ["vosk-model-small-nl-0.22", "nl", "Dutch 0.22", "Dutch", 40441176, ["maat"], ["doei doei"]],
    ["vosk-model-small-fr-0.22", "fr", "French 0.22", "French", 42233323, ["mon frère"], ["à la prochaine", "a la prochaine"]],
    ["vosk-model-small-de-0.15", "de", "German 0.15", "German", 46499967, ["bruder"], ["bis dann"]],
    ["vosk-model-small-it-0.22", "it", "Italian 0.22", "Italian", 49665141, ["fratello"], ["arrivederci"]],
    ["vosk-model-small-pl-0.22", "pl", "Polish 0.22", "Polish", 52979372, ["ziomek"], ["do widzenia"]],
    ["vosk-model-small-pt-0.3", "pt-BR", "Portuguese (Brazil) 0.3", "Portuguese (Brazil)", 32453112, ["irmão"], ["até mais"]],
    ["vosk-model-small-es-0.42", "es", "Spanish 0.42", "Spanish", 39817833, ["hermano"], ["hasta luego"]],
  ].map(([id, language, name, asset, bytes, activation, deactivation, license = "Apache-2.0"]) => ({
    id, engine: "vosk", language, display_name: `Vosk small ${name}`, license, source: "catalog",
    installed: false, bytes, default_activation: activation, default_deactivation: deactivation,
    asset: `Vosk ${asset} model`,
  }));
  const wakeModels = [
    ...catalog,
    // Catalan is no catalog language: no default phrases.
    { id: "local:vosk-model-small-ca-0.4", engine: "vosk", language: "ca", display_name: "vosk-model-small-ca-0.4", license: "", source: "local", installed: true, bytes: 0, default_activation: [], default_deactivation: [] },
  ];
  // `?fresh` in the page's address: a first start in an empty folder (English selected, no
  // phrases, nothing downloaded, the bot in no voice channel). Otherwise the Russian model
  // and the Vosk library are on disk and the bot sits in a channel.
  const FRESH = new URLSearchParams(location.search).has("fresh");
  let libraryInstalled = !FRESH;
  if (!FRESH) wakeModels[1].installed = true;
  if (FRESH) {
    settings.wake.model = "vosk-model-small-en-us-0.15";
    settings.wake.phrases = {};
  }
  // `?always`: the session mode starts as always listening.
  if (new URLSearchParams(location.search).has("always")) settings.session.mode = "always";
  // `?post_answers=<value>&answers_channel=<id>`: the stored answer posting, as after a restart.
  for (const field of ["post_answers", "answers_channel"]) {
    const value = new URLSearchParams(location.search).get(field);
    if (value !== null) settings.assistant[field] = value;
  }
  // The owner link as `status().owner` carries it; `?owner=linked` and `?owner_only`.
  const LINKED = { id: "1091401169441656999", name: "Alex", linked_at: "2026-11-16T12:00:00+00:00" };
  let owner = { linked: null, pending: null, code_ended: null, link_ended: null };
  if (new URLSearchParams(location.search).get("owner") === "linked") owner.linked = { ...LINKED };
  if (new URLSearchParams(location.search).has("owner_only")) settings.discord.owner_only = true;
  // As the app's status: a code past its time is no longer waiting.
  function ownerNow() {
    const now = clone(owner);
    if (now.pending && now.pending.expires_at_ms <= Date.now()) {
      now.pending = null;
      now.code_ended = "the code expired";
    }
    return now;
  }
  const ownerChanged = () => setTimeout(() => emit("app-event", { event: "owner", ...ownerNow() }), 0);
  // `?disconnected`: the token is stored but the bot is not connected to Discord;
  // `__mock.setConnected(on)` connects or disconnects it.
  let connected = !new URLSearchParams(location.search).has("disconnected");
  // As `discord_text_channels` answers while connected: the servers by name, each with its
  // text and announcement channels in Discord's order; the bot may not post in "rules".
  const TEXT_CHANNELS = [
    {
      guild_id: "1091401169441656933", guild_name: "Mock server", channels: [
        { id: "1091401169890443326", name: "general", can_post: true },
        { id: "1571000000000000001", name: "answers", can_post: true },
        { id: "1571000000000000002", name: "rules", can_post: false },
      ],
    },
    {
      guild_id: "1200000000000000000", guild_name: "Second server", channels: [
        { id: "1200000000000000001", name: "bot-spam", can_post: null },
      ],
    },
  ];
  const POST_ANSWERS = ["off", "voice_channel", "command_channel", "channel"];
  // `?collapsed=audio,hotkey`: the stored `ui.collapsed_cards`, as after a restart.
  const collapsedParam = new URLSearchParams(location.search).get("collapsed");
  if (collapsedParam) settings.ui.collapsed_cards = collapsedParam.split(",").filter(Boolean);
  // `?joined`: the bot is in a voice channel when the page loads, also with `?fresh` (as
  // after a reload of the window while the bot is in use).
  const PARAMS = new URLSearchParams(location.search);
  const WHISPER_START = PARAMS.has("whisper") || PARAMS.has("whisper_missing");
  const GPU_FALLBACK = PARAMS.has("gpu_fallback");
  let inVoice = (!FRESH && !WHISPER_START) || PARAMS.has("joined") || GPU_FALLBACK;
  // The codec a join and its call started with: a save changes neither, the next join does.
  let joinCodec = settings.diagnostics.opus_codec;
  // The wake engine as the app runs it: in wake-word mode the files are checked before the
  // phrases, a model that is not on disk asks first, and Download walks through the
  // progress. The consent covers one model.
  let consentFor = null;
  let download = null; // { model, items, index, percent, failed, installing }
  let failNextDownload = false;
  // Archives the user put into the assets folder by hand, by file name: Download checks
  // and unpacks them instead of downloading them.
  const placed = new Set();
  const MODELS_PAGE = "https://alphacephei.com/vosk/models";
  const LIBRARY = {
    label: "Vosk library", host: "github.com", bytes: 14900000,
    file: "vosk-win64-0.3.45.zip", page: "https://github.com/alphacep/vosk-api/releases/tag/v0.3.45",
  };
  // A list never set is the model's defaults; a stored list, even empty, is the user's.
  function phrasesOf(doc, id) {
    const entry = doc.wake.phrases[id] || {};
    const model = wakeModels.find((candidate) => candidate.id === id) || {};
    return {
      activation: entry.activation ?? model.default_activation ?? [],
      deactivation: entry.deactivation ?? model.default_deactivation ?? [],
    };
  }
  const selectedPhrases = () => phrasesOf(settings, settings.wake.model);
  // What choosing `id` needs to download, as `wake_requirements` answers.
  function requirements(id) {
    const model = wakeModels.find((entry) => entry.id === id);
    if (!model) throw `model «${id}» is neither in the catalog nor a local: model`;
    const items = [];
    if (!libraryInstalled) items.push(clone(LIBRARY));
    if (!model.installed) {
      items.push({ label: model.asset, host: "alphacephei.com", bytes: model.bytes, file: `${model.id}.zip`, page: MODELS_PAGE });
    }
    return items;
  }
  function wakeState() {
    // In the Whisper mode Vosk is stopped: Whisper spots the phrases (design D1).
    if (settings.assistant.mode === "whisper" || settings.session.mode !== "wake_word") return { kind: "off" };
    if (download && download.model === settings.wake.model) {
      const item = download.items[download.index];
      if (download.failed) {
        return { kind: "error", message: `${item.file} (downloaded) does not match the pinned SHA-256 (0000…); it was deleted` };
      }
      if (download.installing) return { kind: "installing", label: item.label };
      return { kind: "downloading", label: item.label, percent: download.percent };
    }
    const model = wakeModels.find((entry) => entry.id === settings.wake.model);
    if (!model) return { kind: "error", message: `the model folder C:\\VoiceBro\\models\\${settings.wake.model.slice(6)} is missing; put the model back or choose another one` };
    const items = requirements(model.id);
    if (items.length && consentFor !== settings.wake.model) return { kind: "needs_download", items };
    const phrases = selectedPhrases();
    if (!normalize(phrases.activation).length && !normalize(phrases.deactivation).length) return { kind: "no_phrases" };
    return { kind: "ready" };
  }
  function startDownload(id) {
    if (id !== settings.wake.model) throw `${id} is not the selected wake model (${settings.wake.model})`;
    const state = wakeState();
    if (state.kind !== "needs_download" && state.kind !== "error") throw "nothing is waiting for a download";
    consentFor = settings.wake.model;
    const model = wakeModels.find((entry) => entry.id === settings.wake.model);
    // A retry goes on with what is still missing.
    // Each file: downloaded in 20 % steps, then one step of checking and unpacking, then
    // installed. A file found in the assets folder starts at the unpacking step at once,
    // as the app reports it.
    const items = requirements(model.id);
    download = { model: model.id, items, index: 0, percent: 0, failed: false, installing: placed.has(items[0].file) };
    const step = () => {
      if (!download || download.model !== settings.wake.model) return;
      const item = download.items[download.index];
      if (download.installing) {
        if (item.label === LIBRARY.label) libraryInstalled = true;
        else model.installed = true;
        placed.delete(item.file);
        download.index += 1;
        download.percent = 0;
        if (download.index >= download.items.length) download = null;
        else download.installing = placed.has(download.items[download.index].file);
      } else if (failNextDownload && download.percent >= 40) {
        failNextDownload = false;
        download.failed = true;
        emit("app-event", { event: "wake" });
        return;
      } else {
        download.percent += 20;
        if (download.percent >= 100) download.installing = true;
      }
      emit("app-event", { event: "wake" });
      if (download) setTimeout(step, 300);
    };
    setTimeout(step, 300);
  }
  // ---- The Whisper mode (change `whisper-mode`): the catalogs as `whisper_models`,
  // `tts_voices` and `whisper_requirements` answer, with what is on disk by file name.
  const WHISPER_RELEASE = "https://github.com/ggml-org/whisper.cpp/releases/tag/b5454";
  const WHISPER_MODELS_PAGE = "https://huggingface.co/ggerganov/whisper.cpp/tree/5359861c739e955e79d9a303bcbc70fb988958b1";
  const CPU_RUNTIME = { label: "Whisper runtime (CPU)", host: "github.com", bytes: 8928640, file: "whisper-bin-x64.zip", page: WHISPER_RELEASE };
  const GPU_RUNTIME = { label: "Whisper runtime (NVIDIA GPU, CUDA 12.4)", host: "github.com", bytes: 684913404, file: "whisper-bin-win-cuda-12.4.0-x64.zip", page: WHISPER_RELEASE };
  const VAD_MODEL = {
    label: "Silero voice activity model", host: "huggingface.co", bytes: 885098, file: "ggml-silero-v6.2.0.bin",
    page: "https://huggingface.co/ggml-org/whisper-vad/tree/9ffd54a1e1ee413ddf265af9913beaf518d1639b",
  };
  const ggml = (name, file, bytes) => ({ label: `Whisper ${name} model`, host: "huggingface.co", bytes, file, page: WHISPER_MODELS_PAGE });
  const whisperCatalog = [
    ["whisper-tiny-q8_0", "tiny (q8_0)", "multilingual", "ggml-tiny-q8_0.bin", 43537433, "fastest, rough"],
    ["whisper-base-q8_0", "base (q8_0)", "multilingual", "ggml-base-q8_0.bin", 81768585, "fast; weak for Russian"],
    ["whisper-small-q8_0", "small (q8_0)", "multilingual", "ggml-small-q8_0.bin", 264464607, "balanced; on a CPU about 1–2 s"],
    ["whisper-medium-q5_0", "medium (q5_0)", "multilingual", "ggml-medium-q5_0.bin", 539212467, "accurate; GPU"],
    ["whisper-large-v3-turbo-q5_0", "large-v3-turbo (q5_0)", "multilingual", "ggml-large-v3-turbo-q5_0.bin", 574041195, "accurate and fast on a GPU (turbo)"],
    ["whisper-large-v3-turbo-q8_0", "large-v3-turbo (q8_0)", "multilingual", "ggml-large-v3-turbo-q8_0.bin", 874188075, "the same, closer to full quality"],
    ["whisper-large-v3-turbo", "large-v3-turbo", "multilingual", "ggml-large-v3-turbo.bin", 1624555275, "turbo at full precision"],
    ["whisper-large-v3-q5_0", "large-v3 (q5_0)", "multilingual", "ggml-large-v3-q5_0.bin", 1081140203, "most accurate, slower decoder; GPU"],
    ["whisper-base.en-q8_0", "base.en (q8_0)", "english_only", "ggml-base.en-q8_0.bin", 81781811, "fast, English only"],
    ["whisper-small.en-q8_0", "small.en (q8_0)", "english_only", "ggml-small.en-q8_0.bin", 264477561, "balanced, English only"],
  ].map(([id, name, group, file, bytes, note]) => ({ id, display_name: name, group, note, license: "MIT", asset: ggml(name, file, bytes) }))
    .concat([
      {
        id: "whisper-distil-large-v3.5", display_name: "distil-large-v3.5", group: "english_only", note: "distilled large-v3, English only; GPU", license: "MIT",
        asset: { label: "Whisper distil-large-v3.5 model", host: "huggingface.co", bytes: 1519521155, file: "ggml-distil-large-v3.5.bin", page: "https://huggingface.co/distil-whisper/distil-large-v3.5-ggml" },
      },
      {
        id: "whisper-large-v3-turbo-ru", display_name: "large-v3-turbo Russian (community)", group: "russian_tuned", note: "turbo fine-tuned on Common Voice 17 Russian; GPU", license: "MIT",
        asset: { label: "Whisper large-v3-turbo Russian model", host: "huggingface.co", bytes: 1624555275, file: "ggml-large-v3-turbo-russian.bin", page: "https://huggingface.co/dvislobokov/whisper-large-v3-turbo-russian" },
      },
      {
        id: "whisper-large-v3-ru-podlodka-q5_0", display_name: "large-v3 Russian Podlodka (q5_0, community)", group: "russian_tuned", note: "large-v3 fine-tuned for Russian; GPU", license: "Apache-2.0",
        asset: { label: "Whisper large-v3 Russian Podlodka model", host: "huggingface.co", bytes: 1081140203, file: "ggml-podlodka-large-v3-q5_0.bin", page: "https://huggingface.co/kdes70/whisper-large-v3-ru-podlodka-ggml" },
      },
      // The user's own file in the Whisper models folder: no download, as the app lists it.
      { id: "local:ggml-small-finetuned.bin", display_name: "ggml-small-finetuned.bin", group: "local", note: "your own model", license: "", asset: null },
    ]);
  const VOICES_PAGE = "https://github.com/k2-fsa/sherpa-onnx/releases/tag/tts-models";
  const VOICE_ENGINE = {
    label: "Voice engine (sherpa-onnx 2.0.1)", host: "github.com", bytes: 7899712,
    file: "sherpa-onnx-v2.0.1-win-x64-shared-MT-Release-lib.tar.bz2", page: "https://github.com/k2-fsa/sherpa-onnx/releases/tag/v2.0.1",
  };
  const PHONEMIZER = {
    label: "Phonemizer for Piper and Kokoro (piper-phonemize 1.5.9, GPL-3.0)", host: "github.com", bytes: 427015,
    file: "piper-phonemize-v1.5.9-windows-x64-shared-lib.tar.bz2", page: "https://github.com/csukuangfj/piper-phonemize/releases/tag/v1.5.9",
  };
  const pack = (label, file, bytes, license, phonemizer, rate) => ({
    asset: { label, host: "github.com", bytes, file, page: VOICES_PAGE }, license, needs_phonemizer: phonemizer, sample_rate: rate,
  });
  const SUPERTONIC = pack("Supertonic 3 voices (10 voices, 31 languages)", "sherpa-onnx-supertonic-3-tts-int8-2026-05-11.tar.bz2", 128774318, "BigScience OpenRAIL-M", false, 44100);
  const KOKORO = pack("Kokoro English voices (11 voices)", "kokoro-en-v0_19.tar.bz2", 319625534, "Apache-2.0", true, 24000);
  const piper = (voice, bytes, license) => pack(`Piper voice ${voice}`, `vits-piper-${voice}.tar.bz2`, bytes, license, true, 22050);
  const voiceCatalog = [
    ["supertonic3:F1", "Supertonic 3 F1 (female)", "multi", "supertonic", SUPERTONIC],
    ["supertonic3:F2", "Supertonic 3 F2 (female)", "multi", "supertonic", SUPERTONIC],
    ["supertonic3:M1", "Supertonic 3 M1 (male)", "multi", "supertonic", SUPERTONIC],
    ["supertonic3:M2", "Supertonic 3 M2 (male)", "multi", "supertonic", SUPERTONIC],
    ["piper:en_US-libritts_r-medium", "Piper English (US) LibriTTS-R", "en-US", "piper", piper("en_US-libritts_r-medium", 82038311, "CC BY 4.0")],
    ["piper:en_GB-cori-high", "Piper English (UK) Cori, female", "en-GB", "piper", piper("en_GB-cori-high", 115574061, "public domain")],
    ["piper:ru_RU-denis-medium", "Piper Russian Denis, male", "ru", "piper", piper("ru_RU-denis-medium", 67190991, "CC0")],
    ["piper:de_DE-thorsten-medium", "Piper German Thorsten, male", "de", "piper", piper("de_DE-thorsten-medium", 67214254, "CC0")],
    ["piper:pt_BR-faber-medium", "Piper Portuguese (Brazil) Faber, male", "pt-BR", "piper", piper("pt_BR-faber-medium", 67183065, "CC0")],
    ["kokoro-en:af_bella", "Kokoro English Bella (American, female)", "en-US", "kokoro", KOKORO],
    ["kokoro-en:bm_george", "Kokoro English George (British, male)", "en-GB", "kokoro", KOKORO],
    // The voices Windows has, listed at run time; nothing to download.
    ["windows:MSTTS_V110_enUS_DavidM", "Microsoft David", "en-US", "windows", null],
    ["windows:MSTTS_V110_enUS_ZiraM", "Microsoft Zira", "en-US", "windows", null],
  ].map(([id, display_name, language, engine, voicePack]) => ({ id, display_name, language, engine, pack: voicePack }));
  // whisper.cpp's languages in its order, as `whisper_languages` answers.
  const WHISPER_LANGUAGES = ("en:English,zh:Chinese,de:German,es:Spanish,ru:Russian,ko:Korean,fr:French,ja:Japanese,"
    + "pt:Portuguese,tr:Turkish,pl:Polish,ca:Catalan,nl:Dutch,ar:Arabic,sv:Swedish,it:Italian,id:Indonesian,hi:Hindi,"
    + "fi:Finnish,vi:Vietnamese,he:Hebrew,uk:Ukrainian,el:Greek,ms:Malay,cs:Czech,ro:Romanian,da:Danish,hu:Hungarian,"
    + "ta:Tamil,no:Norwegian,th:Thai,ur:Urdu,hr:Croatian,bg:Bulgarian,lt:Lithuanian,la:Latin,mi:Maori,ml:Malayalam,"
    + "cy:Welsh,sk:Slovak,te:Telugu,fa:Persian,lv:Latvian,bn:Bengali,sr:Serbian,az:Azerbaijani,sl:Slovenian,kn:Kannada,"
    + "et:Estonian,mk:Macedonian,br:Breton,eu:Basque,is:Icelandic,hy:Armenian,ne:Nepali,mn:Mongolian,bs:Bosnian,"
    + "kk:Kazakh,sq:Albanian,sw:Swahili,gl:Galician,mr:Marathi,pa:Punjabi,si:Sinhala,km:Khmer,sn:Shona,yo:Yoruba,"
    + "so:Somali,af:Afrikaans,oc:Occitan,ka:Georgian,be:Belarusian,tg:Tajik,sd:Sindhi,gu:Gujarati,am:Amharic,yi:Yiddish,"
    + "lo:Lao,uz:Uzbek,fo:Faroese,ht:Haitian Creole,ps:Pashto,tk:Turkmen,nn:Nynorsk,mt:Maltese,sa:Sanskrit,"
    + "lb:Luxembourgish,my:Myanmar,bo:Tibetan,tl:Tagalog,mg:Malagasy,as:Assamese,tt:Tatar,haw:Hawaiian,ln:Lingala,"
    + "ha:Hausa,ba:Bashkir,jw:Javanese,su:Sundanese,yue:Cantonese").split(",").map((pair) => pair.split(":"));
  const JOIN_NEEDS_DOWNLOADS = "The Whisper mode needs downloads: open VoiceBro → Config and press Download in the Assistant and Local voice cards";
  // The files on disk, by file name.
  const haveFiles = new Set();
  const have = (asset) => haveFiles.has(asset.file);
  let cuda = !GPU_FALLBACK;
  if (WHISPER_START || GPU_FALLBACK) settings.assistant.mode = "whisper";
  if (PARAMS.has("whisper")) {
    for (const asset of [CPU_RUNTIME, whisperCatalog[1].asset, VAD_MODEL, VOICE_ENGINE, SUPERTONIC.asset]) haveFiles.add(asset.file);
  }
  if (GPU_FALLBACK) {
    settings.whisper.device = "gpu";
    for (const asset of [GPU_RUNTIME, whisperCatalog[1].asset, VAD_MODEL, VOICE_ENGINE, SUPERTONIC.asset]) haveFiles.add(asset.file);
  }
  // What a model on a device needs, then what a voice needs, as the app lists them.
  function whisperMissing(model, device) {
    const entry = whisperCatalog.find((candidate) => candidate.id === model);
    if (!entry) throw `«${model}» is not a Whisper model`;
    if (device !== "cpu" && device !== "gpu") throw `whisper.device: «${device}» is not a device; use one of cpu, gpu`;
    const items = [];
    // A CPU load also runs from the GPU runtime when that one is on disk.
    const runtime = device === "gpu" ? have(GPU_RUNTIME) : have(CPU_RUNTIME) || have(GPU_RUNTIME);
    if (!runtime) items.push(device === "gpu" ? GPU_RUNTIME : CPU_RUNTIME);
    if (entry.asset && !have(entry.asset)) items.push(entry.asset);
    if (!have(VAD_MODEL)) items.push(VAD_MODEL);
    return items.map(clone);
  }
  function voiceAssets(voice) {
    if (!voice.pack) return [];
    return [VOICE_ENGINE].concat(voice.pack.needs_phonemizer ? [PHONEMIZER] : [], [voice.pack.asset]);
  }
  function ttsMissing(id) {
    const voice = voiceCatalog.find((candidate) => candidate.id === id);
    if (!voice) throw `«${id}» is not a voice`;
    return voiceAssets(voice).filter((asset) => !have(asset)).map(clone);
  }
  const selectionOf = (part) => (part === "whisper"
    ? `${settings.whisper.model}|${settings.whisper.device}`
    : settings.tts.voice);
  const missingOf = (part) => (part === "whisper"
    ? whisperMissing(settings.whisper.model, settings.whisper.device)
    : ttsMissing(settings.tts.voice));
  // A running download per part: { selection, items, index, percent, failed, installing }.
  const partDownloads = { whisper: null, tts: null };
  let failNextWhisperDownload = false;
  let whisperLoadingUntil = 0; // Whisper and the voice load at a join, a mode switch, a change
  // The state as the app reports it: a download runs in either mode (change
  // `whisper-config-layout`), and outside one the realtime mode is `off`.
  function partState(part) {
    const download = partDownloads[part];
    if (download && download.selection === selectionOf(part)) {
      const item = download.items[download.index];
      if (download.failed) {
        return { kind: "error", message: `${item.file} (downloaded) does not match the pinned SHA-256 (0000…); it was deleted` };
      }
      if (download.installing) return { kind: "installing", label: item.label };
      return { kind: "downloading", label: item.label, percent: download.percent };
    }
    if (settings.assistant.mode !== "whisper") return { kind: "off" };
    const items = missingOf(part);
    if (items.length) return { kind: "needs_download", items };
    if (!inVoice) return { kind: "idle" };
    return Date.now() < whisperLoadingUntil ? { kind: "loading" } : { kind: "ready" };
  }
  const safeState = (part) => {
    try {
      return partState(part);
    } catch (message) {
      return { kind: "error", message: String(message) };
    }
  };
  const whisperDeviceUsed = () => (settings.whisper.device === "gpu" && cuda && have(GPU_RUNTIME) ? "gpu" : "cpu");
  // The phrases in effect: the stored lists, or "bro" and "peace out" whatever the language
  // (change `whisper-config-layout`).
  function whisperPhrases(doc) {
    const own = doc.whisper.phrases || {};
    return { activation: own.activation ?? ["bro"], deactivation: own.deactivation ?? ["peace out"] };
  }
  // What the selected model and device, or the selected voice, still miss: in both modes.
  const safeMissing = (part) => {
    try {
      return missingOf(part);
    } catch {
      return [];
    }
  };
  function whisperStatus() {
    const state = safeState("whisper");
    const ready = state.kind === "ready";
    const used = ready ? whisperDeviceUsed() : null;
    const phrases = whisperPhrases(settings);
    const measured = turns.filter((turn) => turn.kind === "whisper").length;
    return {
      state, missing: safeMissing("whisper"), model: settings.whisper.model, device_requested: settings.whisper.device, device_used: used,
      fallback_reason: ready && settings.whisper.device === "gpu" && used === "cpu" ? "no CUDA device" : null,
      backend: !ready ? null : used === "gpu" ? "CUDA0: NVIDIA GeForce RTX 4070 Laptop GPU" : `CPU ggml-cpu-alderlake, ${settings.whisper.threads || 8} threads`,
      language: settings.whisper.language, load_ms: ready ? (used === "gpu" ? 640 : 410) : null,
      activation_phrases: normalize(phrases.activation), deactivation_phrases: normalize(phrases.deactivation),
      counters: {
        queued: 0, transcribed: measured, dropped_late: 0, dropped_noise: measured ? 1 : 0, dropped_hallucination: 0,
        last_transcribe_ms: measured ? 420 : null, last_audio_seconds: measured ? 2.4 : null,
      },
    };
  }
  function ttsStatus() {
    const state = safeState("tts");
    const voice = voiceCatalog.find((candidate) => candidate.id === settings.tts.voice);
    const ready = state.kind === "ready";
    return {
      state, missing: safeMissing("tts"), voice: settings.tts.voice, engine: voice?.engine || "",
      sample_rate: ready ? (voice?.pack?.sample_rate || 16000) : null, load_ms: ready ? 300 : null,
      last_first_audio_ms: ready && turns.some((turn) => turn.kind === "whisper") ? 180 : null,
      spoken: 0, skipped: 0,
    };
  }
  const emitPart = (part) => emit("app-event", { event: part, ...(part === "whisper" ? whisperStatus() : ttsStatus()) });
  // Loading after a join or a change while in a channel: loading, then ready.
  function loadWhisper() {
    whisperLoadingUntil = Date.now() + 400;
    setTimeout(() => {
      emitPart("whisper");
      emitPart("tts");
    }, 450);
  }
  // Each file in 20 % steps, then one step of checking and unpacking (at once for a file put
  // into the assets folder by hand); the two parts download side by side, as in the app.
  function startPartDownload(part) {
    const items = missingOf(part);
    if (!items.length) return;
    partDownloads[part] = { selection: selectionOf(part), items, index: 0, percent: 0, failed: false, installing: placed.has(items[0].file) };
    const step = () => {
      const download = partDownloads[part];
      if (!download || download.selection !== selectionOf(part) || download.failed) return;
      const item = download.items[download.index];
      if (download.installing) {
        haveFiles.add(item.file);
        placed.delete(item.file);
        download.index += 1;
        download.percent = 0;
        if (download.index >= download.items.length) {
          // The realtime mode loads nothing: the state goes back to `off`.
          partDownloads[part] = null;
          if (inVoice && settings.assistant.mode === "whisper") loadWhisper();
        } else download.installing = placed.has(download.items[download.index].file);
      } else if (failNextWhisperDownload && download.percent >= 40) {
        failNextWhisperDownload = false;
        download.failed = true;
        emitPart(part);
        return;
      } else {
        download.percent += 20;
        if (download.percent >= 100) download.installing = true;
      }
      emitPart(part);
      if (partDownloads[part]) setTimeout(step, 150);
    };
    setTimeout(step, 150);
  }
  // As `whisper_download({model})` and `tts_download({voice})` (change
  // `whisper-config-layout`): in either mode, for the selected item while it misses files or
  // after a failed download; anything else is refused.
  function partDownload(part, value) {
    const selected = part === "whisper" ? settings.whisper.model : settings.tts.voice;
    if (value !== selected) {
      throw part === "whisper"
        ? `${value} is not the selected Whisper model (${selected})`
        : `${value} is not the selected voice (${selected})`;
    }
    const kind = partState(part).kind;
    if (fetching(kind)) throw `a ${part === "whisper" ? "Whisper" : "voice"} download is already running`;
    if (!missingOf(part).length && kind !== "error") {
      throw part === "whisper" ? "nothing is waiting for a Whisper download" : "nothing is waiting for a voice download";
    }
    startPartDownload(part);
    setTimeout(() => emitPart(part), 0);
  }
  const fetching = (kind) => kind === "downloading" || kind === "installing";
  // A short tone as a 16 kHz mono 16-bit WAV, as `tts_preview` returns its bytes.
  function sampleWav(speed = 1) {
    const rate = 16000;
    const count = Math.round(rate * (0.4 / Math.max(0.5, Number(speed) || 1)));
    const view = new DataView(new ArrayBuffer(44 + count * 2));
    const text = (offset, value) => [...value].forEach((char, i) => view.setUint8(offset + i, char.charCodeAt(0)));
    text(0, "RIFF");
    view.setUint32(4, 36 + count * 2, true);
    text(8, "WAVE");
    text(12, "fmt ");
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true);
    view.setUint16(22, 1, true);
    view.setUint32(24, rate, true);
    view.setUint32(28, rate * 2, true);
    view.setUint16(32, 2, true);
    view.setUint16(34, 16, true);
    text(36, "data");
    view.setUint32(40, count * 2, true);
    for (let i = 0; i < count; i += 1) {
      const fade = Math.min(1, i / 400, (count - i) / 400);
      view.setInt16(44 + i * 2, Math.round(Math.sin((2 * Math.PI * 440 * i) / rate) * 4000 * fade), true);
    }
    return [...new Uint8Array(view.buffer)];
  }

  // A sample session for the documentation's screenshots (the page opened with `?sample`),
  // whose build stamp then reads like a release.
  const SAMPLE = new URLSearchParams(location.search).has("sample");
  const STAMP = SAMPLE ? "1.0.0 2026-11-11" : "1.0.0+mock 2026-11-11";
  const SAMPLE_CHAT = [
    ["20:14:02", "system", null, "Call started"],
    ["20:14:02", "context", null, "In the channel: Alex, Sam. Addressed by: Alex."],
    ["20:14:04", "user", "Alex", "Hey bro, how long is one day in Minecraft?"],
    ["20:14:06", "assistant", null, "A full day in Minecraft lasts twenty real minutes, about ten of them daylight."],
    ["20:15:31", "user", "Sam", "Brother, what's the weather in Berlin tomorrow?"],
    ["20:15:32", "assistant", null, "Let me check."],
    ["20:15:32", "task", null, "Weather forecast for Berlin tomorrow"],
    ["20:15:34", "tool", null, "Web search: Berlin weather forecast tomorrow"],
    ["20:15:39", "agent", null, "Tomorrow in Berlin: cloudy, 8 to 12 °C, light rain in the evening."],
    ["20:15:41", "assistant", null, "Cloudy tomorrow in Berlin, eight to twelve degrees, and some rain in the evening."],
    ["20:16:05", "user", "Alex", "Thanks, bro. Goodbye."],
    ["20:16:05", "system", null, "Call ended"],
  ].map(([at, kind, who, text], index) => ({ id: index + 1, at, guild_id: "1", kind, who, text }));
  const started = Date.now();
  const listeners = {};
  const calls = [];
  // A connected call: 50 packets a second each way; `setLoss` conceals a share of them.
  let lossRate = 0;
  let concealed = 0;
  let lastTick = Date.now();
  const callAudio = {
    frames_sent: 0, input_drops: 0, input_underruns: 0, packets_received: 0, late: 0, concealed: 0,
    concealed_fec: 0, playout_underruns: 0, output_drops: 0, decode_errors: 0,
    stretched_pause_ms: 0, stretched_speech_ms: 0, removed_ms: 0, bridged_frames: 0, speech_gaps: 0,
    depth_ms: 60, depth_max_ms: 60,
  };
  function advanceCall() {
    const now = Date.now();
    const packets = ((now - lastTick) / 1000) * 50;
    lastTick = now;
    callAudio.frames_sent += Math.round(packets);
    callAudio.packets_received += Math.round(packets);
    concealed += packets * lossRate;
    callAudio.concealed = Math.round(concealed);
    callAudio.concealed_fec = Math.floor(callAudio.concealed / 2);
    // A server a little slower than real time: the playout lengthens pauses to keep up.
    callAudio.stretched_pause_ms += Math.round(packets) % 3 === 0 ? 20 : 0;
    callAudio.depth_ms = 220;
    callAudio.depth_max_ms = 360;
  }
  const turns = [];
  function addTurn() {
    const n = turns.length + 1;
    // A turn of the Whisper mode: the four times of its latency line.
    if (safeState("whisper").kind === "ready") {
      const device = whisperDeviceUsed();
      const text = device === "gpu" ? 140 : 420;
      turns.push({
        at: new Date().toLocaleTimeString(), turn: n, kind: "whisper",
        first_speech_to_answer_ms: null, last_speech_to_answer_ms: text + 1300, answer_to_playback_ms: 180,
        first_speech_to_playback_ms: null,
        whisper: {
          speech_to_text_ms: text, audio_seconds: 2.4, model: settings.whisper.model, device,
          text_to_first_words_ms: 1300 + 10 * (n % 5), first_words_to_voice_ms: 180,
          speech_to_voice_ms: text + 1300 + 10 * (n % 5) + 180,
        },
      });
      if (turns.length > 50) turns.shift();
      return;
    }
    turns.push({
      at: new Date().toLocaleTimeString(), turn: n, kind: "response",
      first_speech_to_answer_ms: 3000 + 100 * (n % 5), last_speech_to_answer_ms: 1400 + 50 * (n % 3),
      answer_to_playback_ms: 60, first_speech_to_playback_ms: 3060 + 100 * (n % 5),
    });
    if (turns.length > 50) turns.shift();
  }

  function merge(target, patch) {
    for (const [key, value] of Object.entries(patch)) {
      if (value && typeof value === "object" && !Array.isArray(value) && target[key] && typeof target[key] === "object") {
        merge(target[key], value);
      } else {
        target[key] = value;
      }
    }
    return target;
  }
  // The matching form, as `phrase_words` makes it: lower case, "ё" as "е", every character
  // that is not a letter or a digit a space.
  const matchingForm = (phrase) =>
    String(phrase).toLowerCase().replace(/ё/g, "е").split(/[^\p{L}\p{N}]+/u).filter(Boolean).join(" ");
  // The phrases in effect as the status shows them: as typed, trimmed, the empty ones gone.
  const normalize = (phrases) => phrases.map((p) => String(p).trim()).filter((p) => matchingForm(p));
  // The app's messages, `<field>: <message>`, for the cases the checks use.
  function validate(doc) {
    const timeout = doc.session.idle_timeout_seconds;
    if (timeout !== 0 && !(timeout >= 5 && timeout <= 3600)) {
      throw `session.idle_timeout_seconds: ${timeout} is outside 0 or 5..3600`;
    }
    for (const model of Object.keys(doc.wake.phrases || {})) {
      const lists = phrasesOf(doc, model);
      const deactivation = lists.deactivation.filter((phrase) => matchingForm(phrase));
      for (const activation of lists.activation) {
        const form = matchingForm(activation);
        const shared = form && deactivation.find((phrase) => matchingForm(phrase) === form);
        if (shared) {
          throw `wake.phrases.${model}.deactivation: «${activation.trim()}» in the activation list and «${shared.trim()}» in the deactivation list of «${model}» are the same phrase`;
        }
      }
    }
    const local = doc.wake.model.startsWith("local:") ? doc.wake.model.slice(6) : null;
    const known = wakeModels.some((entry) => entry.source === "catalog" && entry.id === doc.wake.model);
    if (!known && (local === null || !local.trim() || /[\\/:]|\.\./.test(local))) {
      throw `wake.model: «${doc.wake.model}» is neither in the model catalog nor a local:<folder> model`;
    }
    for (const field of ["input_gain", "output_gain"]) {
      const gain = doc.audio[field];
      if (!(gain >= 0 && gain <= 4)) throw `audio.${field}: ${gain} is outside 0 to 4 (0 to 400 %)`;
    }
    if (!THEMES.includes(doc.ui.theme)) {
      throw `ui.theme: «${doc.ui.theme}» is not a theme; use one of ${THEMES.join(", ")}`;
    }
    if (!(doc.ui.font_size >= 11 && doc.ui.font_size <= 20)) {
      throw `ui.font_size: ${doc.ui.font_size} is outside 11 to 20`;
    }
    const modes = ["items", "commentary", "bem_tags", "thinking"];
    if (!modes.includes(doc.codex.handoff_mode)) {
      throw `codex.handoff_mode: «${doc.codex.handoff_mode}» is not an answer handoff mode; use one of ${modes.join(", ")}`;
    }
    if (!(doc.diagnostics.opus_codec in CODEC_LABELS)) {
      throw `diagnostics.opus_codec: «${doc.diagnostics.opus_codec}» is not an Opus codec; use one of ${Object.keys(CODEC_LABELS).join(", ")}`;
    }
    // The app rejects a wrong type when it reads the patched document.
    for (const field of ["performance", "memory", "verbose_log"]) {
      if (typeof doc.diagnostics[field] !== "boolean") {
        throw `invalid type: expected a boolean at diagnostics.${field}`;
      }
    }
    if (typeof doc.assistant.save_chat_history !== "boolean") {
      throw "invalid type: expected a boolean at assistant.save_chat_history";
    }
    // Read with the document in the app, so the message has the app's patch prefix.
    if (!POST_ANSWERS.includes(doc.assistant.post_answers)) {
      throw `the patch is not a settings document: assistant.post_answers: «${doc.assistant.post_answers}» is not a destination; use one of ${POST_ANSWERS.join(", ")}`;
    }
    const channel = String(doc.assistant.answers_channel).trim();
    if (!/^\d*$/.test(channel)) {
      throw `assistant.answers_channel: «${channel}» is not a channel id; a channel id is digits only`;
    }
    if (doc.assistant.post_answers === "channel" && !channel) {
      throw "assistant.answers_channel: pick the channel the answers go to";
    }
    if (typeof doc.discord.owner_only !== "boolean") {
      throw `the patch is not a settings document: discord.owner_only must be true or false, not ${JSON.stringify(doc.discord.owner_only)}`;
    }
    if (typeof doc.codex.refresh_lists_at_start !== "boolean") {
      throw "invalid type: expected a boolean at codex.refresh_lists_at_start";
    }
    if (!Array.isArray(doc.ui.collapsed_cards) || doc.ui.collapsed_cards.some((id) => typeof id !== "string")) {
      throw "invalid type: expected a sequence of strings at ui.collapsed_cards";
    }
    validateWhisper(doc);
  }
  // The `assistant.mode`, `whisper` and `tts` checks, with the app's messages.
  function validateWhisper(doc) {
    if (!["realtime", "whisper"].includes(doc.assistant.mode)) {
      throw `the patch is not a settings document: assistant.mode: «${doc.assistant.mode}» is not an assistant mode; use one of realtime, whisper`;
    }
    const model = String(doc.whisper.model);
    if (!whisperCatalog.some((entry) => entry.id === model && entry.group !== "local")) {
      const file = model.startsWith("local:") ? model.slice(6) : null;
      if (file !== null && (!file.trim() || /[\\/:]|\.\./.test(file))) {
        throw `whisper.model: «${model}»: a local model is a plain file name of the Whisper models folder (no \\, /, : or ..)`;
      }
      if (file === null) {
        throw `whisper.model: «${model}» is neither in the Whisper model catalog nor a local:<file> of the Whisper models folder`;
      }
    }
    if (!["cpu", "gpu"].includes(doc.whisper.device)) {
      throw `the patch is not a settings document: whisper.device: «${doc.whisper.device}» is not a device; use one of cpu, gpu`;
    }
    const threads = doc.whisper.threads;
    if (!Number.isInteger(threads) || threads < 0) {
      throw `the patch is not a settings document: invalid value: ${threads}, expected u32 at whisper.threads`;
    }
    if (threads > 64) throw `whisper.threads: ${threads} is outside 0 (automatic) to 64`;
    const language = doc.whisper.language;
    if (language !== "auto" && !WHISPER_LANGUAGES.some(([code]) => code === language)) {
      throw `whisper.language: «${language}» is not a language Whisper knows; use auto or an ISO 639-1 code such as en or ru`;
    }
    for (const list of ["activation", "deactivation"]) {
      const phrase = (doc.whisper.phrases?.[list] || []).find((item) => String(item).trim() && !matchingForm(item));
      if (phrase) throw `whisper.phrases.${list}: «${phrase.trim()}» has no letter or digit`;
    }
    const lists = whisperPhrases(doc);
    for (const activation of lists.activation) {
      const form = matchingForm(activation);
      const shared = form && lists.deactivation.find((phrase) => matchingForm(phrase) === form);
      if (shared) {
        throw `whisper.phrases.deactivation: «${activation.trim()}» in the activation list and «${shared.trim()}» in the deactivation list are the same phrase`;
      }
    }
    const voice = String(doc.tts.voice);
    const windows = voice.startsWith("windows:") && voice.slice(8).trim();
    if (!windows && !voiceCatalog.some((entry) => entry.id === voice)) {
      throw `tts.voice: «${voice}» is neither in the voice catalog nor a windows:<voice id> of Windows`;
    }
    if (!(doc.tts.speed >= 0.5 && doc.tts.speed <= 2)) throw `tts.speed: ${doc.tts.speed} is outside 0.5 to 2.0`;
  }

  function emit(name, payload) {
    for (const handler of listeners[name] || []) handler({ payload });
  }

  function status() {
    return {
      build_stamp: STAMP,
      data_dir: "C:\\VoiceBro",
      muted,
      hotkey: { shortcut: settings.hotkey.mute, registered: true, error: null },
      secrets: { discord_token: { is_set: tokenSet } },
      connection_errors: {},
      discord: {
        state: tokenSet && connected ? "connected" : "disconnected",
        user_name: "Mock bot",
        guild_count: 1,
        guilds: [{ id: "1091401169441656933", name: "Mock server" }],
        voice_channels: !inVoice ? {} : {
          "1091401169441656933": {
            guild_name: "Mock server",
            channel_name: "General",
            participants: ["Alex"],
            output: { pushed: 0, dropped: 0, played: 0, underruns: 0, silence_frames: 0, flushed: 0 },
            delivery: (() => {
              // About one receiver report a second from Discord, with a lossy second now and then.
              const seconds = Math.floor((Date.now() - started) / 1000);
              const packets = Math.floor((Date.now() - started) / 20);
              return {
                voice_server: "finland14098.discord.media", region: "finland",
                packets_encoded: packets, passed_through: packets, encode_errors: 0,
                expected_loss_percent: seconds < 1 ? 10 : 4,
                expected_loss_source: seconds < 1 ? "default" : "measured",
                reports: seconds, last_reported_loss_percent: seconds ? (seconds % 7 === 0 ? 5.9 : 0.4) : null,
                last_reported_jitter_ms: seconds ? 1.4 : null,
              };
            })(),
            opus_codec: CODEC_LABELS[joinCodec],
          },
        },
        last_error: null,
        counters: { ticks: Math.floor((Date.now() - started) / 20), speaker_frames: 0, malformed_frames: 0, unnamed_sources: 0 },
      },
      wake: {
        state: wakeState(), engine: settings.wake.engine, model: settings.wake.model,
        activation_phrases: normalize(selectedPhrases().activation), deactivation_phrases: normalize(selectedPhrases().deactivation),
        unknown_words: normalize(selectedPhrases().activation).includes("zzz") ? ["zzz"] : [], dropped_chunks: 0,
      },
      whisper: whisperStatus(),
      tts: ttsStatus(),
      gates: {
        "1091401169441656933": {
          guild_id: "1091401169441656933", mode: settings.session.mode, muted, active: false,
          listening_for_wake_word: !muted, session_wanted: false, assistant_ready: false,
          last_wake_phrase: null, replay_held: 0, counters: { input_dropped: 0, replay_dropped: 0, replay_timeouts: 0 },
        },
      },
      audio: {
        input: { gain: settings.audio.input_gain, peak: 0.12, clips: 0 },
        output: { gain: settings.audio.output_gain, peak: 0, clips: 0 },
      },
      assistant: {
        engine: "codex",
        summary: !codex.installed
          ? "codex not found: press Install Codex on the Status tab, or set the path of codex.exe in Config"
          : codex.signedIn ? "signed in (chatgpt)" : codex.accountRead ? "not signed in" : "codex found; it starts at /join",
        signed_in: codex.installed && codex.signedIn, account_type: codex.installed && codex.signedIn ? "chatgpt" : null,
        email: null, plan: "plus",
        executable: codex.installed ? "C:\\Users\\me\\AppData\\Local\\Microsoft\\WinGet\\Links\\codex.exe" : null,
        version: codex.installed ? "codex-cli 0.167.0" : null,
        running: codex.installed, error: null, model_in_use: null, reasoning_effort_in_use: null,
        calls: [{
          guild_id: "1091401169441656933", state: "live", connected: true, audio: (advanceCall(), clone(callAudio)),
          opus_codec: inVoice ? CODEC_LABELS[joinCodec] : null,
          thread_id: "mock-thread", last_user_text: null, last_assistant_text: null, last_error: null,
        }],
      },
      latency: { "1091401169441656933": clone(turns) },
      owner: ownerNow(),
      settings: clone(settings),
    };
  }

  const commands = {
    status: () => status(),
    get_settings: () => ({ settings: clone(settings), secrets: { discord_token: { is_set: tokenSet } } }),
    save_settings: ({ patch }) => {
      const next = merge(clone(settings), patch);
      validate(next);
      const wakeChanged = JSON.stringify(next.wake) !== JSON.stringify(settings.wake)
        || next.session.mode !== settings.session.mode || next.assistant.mode !== settings.assistant.mode;
      const whisperChanged = JSON.stringify([next.assistant.mode, next.whisper, next.tts])
        !== JSON.stringify([settings.assistant.mode, settings.whisper, settings.tts]);
      const reload = inVoice && next.assistant.mode === "whisper" && (next.assistant.mode !== settings.assistant.mode
        || next.whisper.model !== settings.whisper.model || next.whisper.device !== settings.whisper.device
        || next.tts.voice !== settings.tts.voice);
      settings = next;
      setTimeout(() => emit("app-event", { event: "settings_changed" }), 0);
      // The app's wake engine publishes its new state after a wake or session change.
      if (wakeChanged) setTimeout(() => emit("app-event", { event: "wake" }), 10);
      // Whisper and the voice check their files in the save (design D10) and publish.
      if (whisperChanged) {
        setTimeout(() => {
          emitPart("whisper");
          emitPart("tts");
        }, 10);
      }
      if (reload) loadWhisper();
      return clone(settings);
    },
    set_secret: ({ value }) => {
      if (!String(value).trim()) throw "the value is empty";
      tokenSet = true;
    },
    clear_secret: () => {
      tokenSet = false;
    },
    system_status: () => ({
      version: "1.0.0", build_stamp: STAMP, process_id: 4242,
      uptime_seconds: Math.floor((Date.now() - started) / 1000), runtime: "Rust / Tauri 2 / WebView2 (mock)",
      platform: "Windows (mock)", architecture: "x86_64", cpu_cores: 8,
    }),
    wake_models: () => wakeModels.map(({ asset, ...model }) => clone(model)),
    wake_requirements: ({ model }) => requirements(model),
    wake_download: ({ model }) => startDownload(model),
    open_models_folder: () => "C:\\VoiceBro\\models",
    // The Whisper mode (change `whisper-mode`).
    whisper_models: () => whisperCatalog.map(({ asset, ...model }) => ({
      ...clone(model), bytes: asset ? asset.bytes : 0, installed: asset ? have(asset) : true,
    })),
    // A catalog voice's `bytes` is what it still has to download.
    tts_voices: () => voiceCatalog.map((voice) => {
      const missing = voiceAssets(voice).filter((asset) => !have(asset));
      return {
        id: voice.id, display_name: voice.display_name, language: voice.language, engine: voice.engine,
        bytes: missing.reduce((sum, asset) => sum + asset.bytes, 0), installed: missing.length === 0,
        data_license: voice.pack ? voice.pack.license : "built into Windows",
        needs_phonemizer: Boolean(voice.pack?.needs_phonemizer),
      };
    }),
    whisper_languages: () => clone(WHISPER_LANGUAGES),
    whisper_requirements: ({ model, device, voice }) => {
      const items = whisperMissing(model, device);
      for (const item of ttsMissing(voice)) {
        if (!items.some((known) => known.file === item.file)) items.push(item);
      }
      return items;
    },
    whisper_download: ({ model }) => partDownload("whisper", model),
    tts_download: ({ voice }) => partDownload("tts", voice),
    tts_preview: ({ voice, speed }) => {
      const missing = ttsMissing(voice);
      if (missing.length) throw `${voice} needs a download first: ${missing.map((item) => item.label).join(", ")}`;
      return sampleWav(speed);
    },
    open_whisper_models_folder: () => "C:\\VoiceBro\\whisper-models",
    open_assets_folder: () => "C:\\VoiceBro\\assets",
    capabilities: () => clone(capabilities),
    // The text channels of the bot's servers, none while not connected, as the app answers.
    discord_text_channels: () => (tokenSet && connected ? clone(TEXT_CHANNELS) : []),
    // `?sample` in the page's address: a sample session for the screenshots in docs/images.
    chat_log: () => (SAMPLE ? clone(SAMPLE_CHAT) : [
      { id: 1, at: "12:00:01", guild_id: "1091401169441656933", kind: "user", who: "Alex", text: "What time is it?" },
      { id: 2, at: "12:00:03", guild_id: "1091401169441656933", kind: "assistant", who: null, text: "It is noon." },
    ]),
    log_history: () => ["[2026-11-11][12:00:00][INFO][voicebro] VoiceBro 1.0.0+mock (mock log)"],
    set_muted: ({ muted: next }) => {
      muted = next;
      return muted;
    },
    toggle_mute: () => (muted = !muted),
    discord_connect: () => {},
    discord_disconnect: () => {},
    // Like the app's Check: reads the account and, signed in, the lists with a new read time.
    // Answers after 300 ms, as the app-server takes a moment.
    codex_check: async () => {
      await new Promise((resolve) => setTimeout(resolve, 300));
      if (failNextCheck) {
        const message = failNextCheck;
        failNextCheck = null;
        throw message;
      }
      codex.accountRead = true;
      if (codex.installed && codex.signedIn && capabilities.models.length) {
        capabilities = { ...clone(capabilities), lists_read_at_ms: Date.now() };
        if (checkAddsModel) {
          checkAddsModel = false;
          capabilities.models.push({
            id: "model-c", display_name: "Model C", description: "", reasoning_efforts: ["minimal", "low"],
            default_reasoning_effort: "low", is_default: false,
          });
        }
      }
      return { signed_in: codex.signedIn, kind: codex.signedIn ? "chatgpt" : null, email: null, plan: codex.signedIn ? "plus" : null };
    },
    codex_install_info: () => ({
      winget: codex.winget, package: "OpenAI.Codex", source: "OpenAI's GitHub releases (github.com/openai/codex)",
      download_mb: 200, disk_mb: 370, state: clone(install),
    }),
    // The console runs for 1.5 s; exit code 0 installs Codex (signed out), anything else not.
    codex_install: () => {
      if (install.kind === "running") throw "an install is already running in its console window";
      install = { kind: "running" };
      setTimeout(() => {
        install = { kind: "finished", code: codex.nextExitCode };
        if (codex.nextExitCode === 0) {
          codex.installed = true;
          codex.signedIn = false;
          codex.accountRead = false;
        }
        emit("app-event", { event: "codex_install", state: clone(install) });
      }, 1500);
    },
    // Like the app: Check's account read (300 ms), then a code for 10 minutes.
    owner_link_start: async () => {
      await new Promise((resolve) => setTimeout(resolve, 300));
      codex.accountRead = true;
      if (!(codex.installed && codex.signedIn)) {
        throw "Codex is not signed in: use Sign in on the Status tab first, then press Link again";
      }
      owner.pending = { code: "K7QF-2M9X", expires_at_ms: Date.now() + 10 * 60 * 1000, wrong_tries: 0 };
      owner.code_ended = null;
      ownerChanged();
      return clone(owner.pending);
    },
    owner_link_cancel: () => {
      if (!owner.pending) return false;
      owner.pending = null;
      owner.code_ended = "the code was cancelled";
      ownerChanged();
      return true;
    },
    owner_unlink: () => {
      if (!owner.linked) return false;
      owner.linked = null;
      owner.link_ended = null;
      ownerChanged();
      return true;
    },
    codex_sign_in: () => "https://chatgpt.com/mock",
    codex_models: () => clone(capabilities.models),
    open_log_folder: () => "C:\\VoiceBro\\logs",
    app_info: () => ({ version: "1.0.0", build_stamp: STAMP, data_dir: "C:\\VoiceBro" }),
    // `null` while the switch is off, as the app answers when its reader is not running.
    memory_status: () => (settings.diagnostics.memory
      ? {
          total_mb: 172.4, app_mb: 61.2, webview_mb: 111.2, webview_processes: 5, codex_mb: null,
          metric: "private working set",
        }
      : null),
  };

  async function invoke(name, args = {}) {
    calls.push({ name, args: clone(args) });
    const command = commands[name];
    if (!command) {
      console.error(`mock: unknown command ${name}`);
      throw `unknown command ${name}`;
    }
    await new Promise((resolve) => setTimeout(resolve, 20));
    return command(args);
  }

  async function listen(name, handler) {
    (listeners[name] = listeners[name] || []).push(handler);
    return () => {
      listeners[name] = listeners[name].filter((h) => h !== handler);
    };
  }

  // Status events every second, as a connected app sends them; `quiet(true)` stops them
  // so the page's own polling can be counted.
  let tick = 0;
  let quietEvents = false;
  setInterval(() => {
    tick += 1;
    if (quietEvents) return;
    emit("app-event", { event: tick % 2 ? "discord" : "gate" });
    emit("app-log", `[2026-11-10][12:00:${String(tick % 60).padStart(2, "0")}][INFO][mock] status tick ${tick}`);
  }, 1000);

  window.__TAURI__ = { core: { invoke }, event: { listen } };
  // `?gpu_fallback`: one answer was measured before the page loaded.
  if (GPU_FALLBACK) addTurn();
  setInterval(addTurn, 4000);
  window.__mock = {
    calls,
    settings: () => clone(settings),
    emit,
    // How many times a command was called since the page loaded, or since `sinceIndex`
    // (a previous `calls.length`), for checking how often the page polls.
    callsOf: (name, sinceIndex = 0) => calls.slice(sinceIndex).filter((call) => call.name === name).length,
    quiet(on) {
      quietEvents = on;
    },
    // The share of received packets the call conceals from now on (0.03 = 3 %).
    setLoss(rate) {
      lossRate = rate;
    },
    // Codex: `setCodex({ installed, signedIn, accountRead, winget, nextExitCode })` changes
    // what the status reports and what the next Install Codex ends with; the install state
    // goes back to idle.
    setCodex(changes) {
      Object.assign(codex, changes);
      install = { kind: "idle" };
      emit("app-event", { event: "assistant" });
    },
    // The wake engine: `wakeState()` as the status reports it; `failNextDownload()` makes
    // the next Download stop at 40 % with a hash error; `resetDownloads()` puts the English
    // model and the library back to "not downloaded" and forgets the consent;
    // `setInstalled(ids, library)` marks those models (and the library) as on disk;
    // `placeArchives(ids, library)` puts their archives (and the library's) into the assets
    // folder, so the next Download checks and unpacks them without a download step.
    wakeState: () => wakeState(),
    requirements: (id) => requirements(id),
    failNextDownload() {
      failNextDownload = true;
    },
    resetDownloads(library = false) {
      wakeModels[0].installed = false;
      libraryInstalled = !library;
      consentFor = null;
      download = null;
      emit("app-event", { event: "wake" });
    },
    setInstalled(ids, library = true) {
      for (const model of wakeModels) if (ids.includes(model.id)) model.installed = true;
      libraryInstalled = library;
      emit("app-event", { event: "wake" });
    },
    placeArchives(ids, library = false) {
      for (const id of ids) placed.add(`${id}.zip`);
      if (library) placed.add(LIBRARY.file);
    },
    // The bot joins or leaves the mock server's voice channel, as `/join` and `/leave` do.
    // In the Whisper mode a join with files missing is refused (`false`): the bot stays out,
    // both parts ask for their files again (unless a download runs), and the app logs the
    // refusal, which the window takes as the trigger of its dialog.
    join() {
      if (settings.assistant.mode === "whisper") {
        if (missingOf("whisper").length || missingOf("tts").length) {
          for (const part of ["whisper", "tts"]) {
            if (partDownloads[part]?.failed) partDownloads[part] = null;
          }
          emitPart("whisper");
          emitPart("tts");
          const line = `join to guild 1091401169441656933 refused: ${JOIN_NEEDS_DOWNLOADS}`;
          emit("app-event", { event: "log", line });
          emit("app-log", `[2026-11-17][12:00:00][INFO][voicebro_core::runtime] ${line}`);
          return false;
        }
        loadWhisper();
      }
      inVoice = true;
      joinCodec = settings.diagnostics.opus_codec;
      emit("app-event", { event: "discord" });
      return true;
    },
    leave() {
      inVoice = false;
      emit("app-event", { event: "discord" });
      if (settings.assistant.mode === "whisper") {
        emitPart("whisper");
        emitPart("tts");
      }
    },
    // The Whisper mode (change `whisper-mode`); see the comment at the top.
    whisperState: () => safeState("whisper"),
    ttsState: () => safeState("tts"),
    whisperRequirements: (model, device, voice) => commands.whisper_requirements({ model, device, voice }),
    failNextWhisperDownload() {
      failNextWhisperDownload = true;
    },
    completeWhisperDownload() {
      for (const part of ["whisper", "tts"]) {
        for (const item of missingOf(part)) haveFiles.add(item.file);
        partDownloads[part] = null;
      }
      if (inVoice && settings.assistant.mode === "whisper") loadWhisper();
      emitPart("whisper");
      emitPart("tts");
    },
    placeWhisperFiles(files) {
      for (const file of files) placed.add(file);
    },
    setCuda(on) {
      cuda = on;
      if (inVoice && settings.assistant.mode === "whisper") loadWhisper();
    },
    whisperTurn() {
      addTurn();
      emitPart("whisper");
    },
    haveFiles: () => [...haveFiles],
    // The bot connects to Discord or disconnects, as Connect and Disconnect do.
    setConnected(on) {
      connected = on;
      emit("app-event", { event: "discord" });
    },
    // What `discord_text_channels` answers while connected.
    textChannels: () => clone(TEXT_CHANNELS),
    // The codec panicked during a join: the app makes the bot leave and tells the window,
    // as `handle_codec_failure` does. `where` names another server for a second failure.
    codecFailure(where = {}) {
      inVoice = false;
      emit("app-event", {
        event: "codec_failure",
        guild_name: where.guild_name || "Mock server",
        channel_name: where.channel_name || "General",
        path: where.path || "receive",
        activity: where.activity || "decoding the people in the channel",
        codec: CODEC_LABELS[joinCodec],
        message: where.message || "attempt to subtract with overflow",
      });
      emit("app-event", { event: "discord" });
    },
    // The owner link (change `owner-only-access`): the waiting code sent from Discord by
    // `name`, a wrong code (the fifth ends it), and the code's time running out.
    owner: () => ownerNow(),
    completeLink(name = "Alex") {
      if (!ownerNow().pending) return false;
      owner.linked = { ...LINKED, name };
      owner.pending = null;
      owner.code_ended = null;
      owner.link_ended = null;
      ownerChanged();
      return true;
    },
    wrongCode() {
      if (!ownerNow().pending) return false;
      owner.pending.wrong_tries += 1;
      if (owner.pending.wrong_tries >= 5) {
        owner.pending = null;
        owner.code_ended = "the code was ended after 5 wrong tries";
      }
      ownerChanged();
      return true;
    },
    expireCode() {
      if (owner.pending) owner.pending.expires_at_ms = Date.now() - 1;
      ownerChanged();
    },
    failNextCheck(message = "codex not found: press Install Codex on the Status tab, or set the path of codex.exe in Config") {
      failNextCheck = message;
    },
    nextCheckAddsModel() {
      checkAddsModel = true;
    },
    // What `capabilities` answers now (the lists and their read time).
    capabilities: () => clone(capabilities),
    changeCapabilities() {
      capabilities = clone(capabilities);
      capabilities.models.push({
        id: "model-c", display_name: "Model C", description: "", reasoning_efforts: ["minimal", "low"],
        default_reasoning_effort: "low", is_default: false,
      });
      emit("app-event", { event: "assistant" });
    },
    // One chat entry of each kind, as the app sends them: `event` names the event and
    // `kind` the entry.
    emitChatOfEveryKind() {
      const kinds = [
        ["user", "Alex", "Find where the rope gun is"],
        ["context", null, "In the channel: Alex, Sam"],
        ["assistant", null, "Let me check."],
        ["task", null, "Find the rope gun location in Sons of the Forest"],
        ["reasoning", null, "Searching the web for the location"],
        ["tool", null, "Web search: Sons of the Forest rope gun location"],
        ["agent", null, "It is in the maintenance bunker."],
        ["system", null, "Call started"],
        ["error", null, "The call ended unexpectedly"],
      ];
      kinds.forEach(([kind, who, text], index) => {
        emit("app-event", {
          event: "chat", id: 100 + index, guild_id: "1091401169441656933",
          at: `12:01:${String(index).padStart(2, "0")}`, kind, who, text,
        });
      });
    },
  };
})();
