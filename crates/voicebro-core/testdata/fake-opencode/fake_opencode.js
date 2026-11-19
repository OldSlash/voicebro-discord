// A stand-in for OpenCode 2.x (`opencode.exe`) in the unit tests (change
// `claude-and-opencode-engines`, design D14). It serves what VoiceBro uses of the V2 server
// API, as read from OpenCode's source at tag v2.3.2:
//
// - `--version` prints a version.
// - `serve --hostname H --port P` listens there; every route needs HTTP Basic with the user
//   `opencode` and the password in OPENCODE_PASSWORD (401 otherwise). It prints
//   `server listening on http://H:P` and exits when its parent process is gone.
// - GET /api/info, /api/integration, /api/provider, /api/model, /api/model/default; POST /api/session,
//   /api/session/{id}/model, /api/session/{id}/prompt ({text, delivery: queue|steer}),
//   /api/session/{id}/interrupt; DELETE /api/session/{id}; GET /api/event (server-sent
//   events `data: {id, created, type, data}`).
// - An execution: `session.execution.started`, an interim text ("Let me check that."), a web
//   search (`session.tool.input.started`, `.called`, `.success`), the final text ("You
//   asked: <questions joined by />. In London it is 9 degrees." and an OpenAI citation
//   mark), each streamed as `session.text.delta` and closed with
//   `session.text.ended`, then `session.execution.succeeded`. An interrupt ends it with
//   `session.execution.interrupted`. A `queue` prompt while one runs starts the next
//   execution after it; a `steer` prompt joins the running one.
// - The environment: FAKE_OPENCODE_PROVIDERS (default `opencode,openai`; `none`),
//   FAKE_OPENCODE_MODELS (default `openai/gpt-5.4-mini,openai/gpt-5.4-mini-fast,
//   opencode/big-pickle`; `none`), FAKE_OPENCODE_FAIL_FIRST=<file>
//   (the first start exits at once and leaves the file, as a port that could not be bound).
// - The fake's own routes: POST /fake/delay {ms}, GET /fake/state (sessions, prompts, model
//   changes, interrupts, deletions, the environment it got and its config file), POST
//   /fake/exit.
const http = require("http");
const fs = require("fs");
const args = process.argv.slice(2);
const option = (name) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};

if (args.includes("--version")) {
  console.log("2.3.2-fake");
  process.exit(0);
}
if (args[0] !== "serve") {
  console.error("the fake only serves");
  process.exit(2);
}
const failFirst = process.env.FAKE_OPENCODE_FAIL_FIRST;
if (failFirst && !fs.existsSync(failFirst)) {
  fs.writeFileSync(failFirst, "failed once\n");
  process.exit(1);
}

const host = option("--hostname") || "127.0.0.1";
const port = Number(option("--port") || 4096);
const password = process.env.OPENCODE_PASSWORD || "";
const expected = "Basic " + Buffer.from(`opencode:${password}`).toString("base64");
const list = (name, fallback) => {
  const value = process.env[name] ?? fallback;
  return value === "none" ? [] : value.split(",").filter(Boolean);
};
const providers = list("FAKE_OPENCODE_PROVIDERS", "opencode,openai");
const models = list("FAKE_OPENCODE_MODELS", "openai/gpt-5.4-mini,openai/gpt-5.4-mini-fast,opencode/big-pickle");

const state = {
  sessions: [], prompts: [], modelChanges: [], interrupts: 0, deleted: [],
  env: {
    config: process.env.OPENCODE_CONFIG || null,
    disableClaudeCode: process.env.OPENCODE_DISABLE_CLAUDE_CODE || null,
    disableProjectConfig: process.env.OPENCODE_DISABLE_PROJECT_CONFIG || null,
    disableAutoupdate: process.env.OPENCODE_DISABLE_AUTOUPDATE || null,
  },
  configFile: null,
};
try {
  if (state.env.config) state.configFile = JSON.parse(fs.readFileSync(state.env.config, "utf8"));
} catch {}

let delay = 0;
let nextId = 1;
const clients = new Set();
const runs = new Map(); // session id -> { cancelled, questions, done }
const queued = new Map(); // session id -> [text]

function emit(type, data) {
  const frame = `data: ${JSON.stringify({ id: `evt_${nextId++}`, created: Date.now(), type, data })}\n\n`;
  for (const client of clients) client.write(frame);
}
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function text(session, run, message, ordinal, body) {
  for (const word of body.split(/(?<= )/)) {
    if (run.cancelled) return false;
    emit("session.text.delta", { sessionID: session, assistantMessageID: message, ordinal, delta: word });
    if (delay) await sleep(delay);
  }
  if (run.cancelled) return false;
  emit("session.text.ended", { sessionID: session, assistantMessageID: message, ordinal });
  return true;
}

async function execute(session, first) {
  const run = { cancelled: false, questions: [first] };
  runs.set(session, run);
  emit("session.execution.started", { sessionID: session });
  const message = `msg_${nextId++}`;
  let ok = await text(session, run, message, 0, "Let me check that.");
  if (ok) {
    // A web search as 2.3.2 reports it: the name first, the input with the call, then the result.
    const call = `call_${nextId++}`;
    emit("session.tool.input.started", { sessionID: session, assistantMessageID: message, id: call, name: "websearch" });
    emit("session.tool.input.ended", { sessionID: session, assistantMessageID: message, id: call, text: JSON.stringify({ query: "London weather" }) });
    emit("session.tool.called", { sessionID: session, assistantMessageID: message, id: call, executed: false, input: { query: "London weather" } });
    emit("session.tool.progress", { sessionID: session, assistantMessageID: message, id: call, metadata: { provider: "exa" } });
    emit("session.tool.success", { sessionID: session, assistantMessageID: message, id: call, content: [{ text: "London: 9 degrees" }] });
    if (delay) await sleep(delay);
    // The answer ends with the citation mark an OpenAI model leaves (2026-11-19).
    ok = !run.cancelled && await text(session, run, message, 1, `You asked: ${run.questions.join(" / ")}. In London it is 9 degrees. \u{e200}cite\u{e202}turn0search0\u{e201}`);
  }
  runs.delete(session);
  if (run.cancelled || !ok) {
    emit("session.execution.interrupted", { sessionID: session, reason: "user" });
  } else {
    emit("session.execution.succeeded", { sessionID: session });
  }
  const waiting = queued.get(session) || [];
  if (waiting.length) {
    queued.set(session, []);
    setTimeout(() => execute(session, waiting.join("\n")), 0);
  }
}

function body(request) {
  return new Promise((resolve) => {
    let data = "";
    request.on("data", (chunk) => (data += chunk));
    request.on("end", () => {
      try { resolve(data ? JSON.parse(data) : {}); } catch { resolve(null); }
    });
  });
}
function send(response, status, value) {
  response.writeHead(status, { "content-type": "application/json" });
  response.end(value === undefined ? "" : JSON.stringify(value));
}

const server = http.createServer(async (request, response) => {
  if (request.headers.authorization !== expected) {
    send(response, 401, { _tag: "UnauthorizedError", message: "Unauthorized" });
    return;
  }
  const url = new URL(request.url, `http://${host}:${port}`);
  const path = url.pathname;
  const match = path.match(/^\/api\/session\/([^/]+)(?:\/(\w+))?$/);
  if (request.method === "GET" && path === "/api/info") return send(response, 200, { version: "2.3.2-fake", pid: process.pid });
  // As 2.3.2 answered on 2026-11-19: integrations only from configuration files (none here),
  // the usable providers under /api/provider, and models whose `id` names them while
  // `modelID` is the provider's model several of them share.
  if (request.method === "GET" && path === "/api/integration") return send(response, 200, { data: [] });
  if (request.method === "GET" && path === "/api/provider") {
    return send(response, 200, { data: providers.map((id) => ({ id, integrationID: id, name: id, activation: id === "opencode" ? "enabled" : "auto", settings: {} })) });
  }
  if (request.method === "GET" && path === "/api/model") {
    return send(response, 200, { data: models.map((name) => { const [providerID, id] = name.split("/"); return { id, modelID: id.replace(/-fast$/, ""), providerID, name: id, enabled: true }; }) });
  }
  if (request.method === "GET" && path === "/api/model/default") {
    if (!models.length) return send(response, 200, { data: null });
    const [providerID, id] = models[0].split("/");
    return send(response, 200, { data: { id, modelID: id, providerID } });
  }
  if (request.method === "GET" && path === "/api/event") {
    response.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache" });
    clients.add(response);
    response.write(`data: ${JSON.stringify({ id: "evt_0", created: Date.now(), type: "server.connected", data: {} })}\n\n`);
    request.on("close", () => clients.delete(response));
    return;
  }
  if (request.method === "POST" && path === "/api/session") {
    const value = await body(request);
    const allowed = ["title", "agent", "model", "location", "permissions", "metadata", "id", "parentID"];
    if (!value || Object.keys(value).some((key) => !allowed.includes(key))) return send(response, 400, { message: "unknown field" });
    const id = `ses_${nextId++}`;
    state.sessions.push({ id, ...value });
    return send(response, 200, { data: { id, title: value.title } });
  }
  if (match && request.method === "DELETE" && !match[2]) {
    state.deleted.push(match[1]);
    return send(response, 204);
  }
  if (match && request.method === "POST" && match[2] === "model") {
    state.modelChanges.push({ session: match[1], ...(await body(request)) });
    return send(response, 204);
  }
  if (match && request.method === "POST" && match[2] === "prompt") {
    const value = await body(request);
    const allowed = ["text", "files", "agents", "skills", "metadata", "delivery", "resume", "id"];
    if (!value || typeof value.text !== "string" || Object.keys(value).some((key) => !allowed.includes(key))) {
      return send(response, 400, { message: "bad prompt" });
    }
    const session = match[1];
    const delivery = value.delivery || "steer";
    state.prompts.push({ session, text: value.text, delivery });
    const run = runs.get(session);
    if (run && delivery === "steer") run.questions.push(value.text);
    else if (run) queued.set(session, [...(queued.get(session) || []), value.text]);
    else setTimeout(() => execute(session, value.text), 0);
    return send(response, 200, { data: { id: `msg_${nextId++}` } });
  }
  if (match && request.method === "POST" && match[2] === "interrupt") {
    const run = runs.get(match[1]);
    state.interrupts += 1;
    if (run) run.cancelled = true;
    return send(response, 200, { interrupted: Boolean(run) });
  }
  if (request.method === "POST" && path === "/fake/delay") {
    delay = (await body(request))?.ms || 0;
    return send(response, 204);
  }
  if (request.method === "GET" && path === "/fake/state") return send(response, 200, state);
  if (request.method === "POST" && path === "/fake/exit") {
    send(response, 204);
    setTimeout(() => process.exit(0), 20);
    return;
  }
  send(response, 404, { message: `no route ${request.method} ${path}` });
});
server.listen(port, host, () => console.log(`server listening on http://${host}:${port}`));
// The app kills the process it started (here cmd.exe for the .cmd wrapper): leave with it.
setInterval(() => {
  try { process.kill(process.ppid, 0); } catch { process.exit(0); }
}, 300);
