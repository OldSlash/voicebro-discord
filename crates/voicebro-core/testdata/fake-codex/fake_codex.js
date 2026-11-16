// A stand-in for `codex app-server` in the unit tests. It answers the handshake, the
// account, the voices, the models, `thread/start` and `thread/realtime/start` over JSON-RPC
// on stdio, and exits when stdin closes. `--version` prints a version; `--signed-out`
// reports no account; `--reject-handoff` refuses a realtime start that carries an answer
// handoff field, as a Codex version without those fields would. A realtime start answers
// with its attempt number and the parameters it received. `--log <file>` appends the method
// of every request to that file, one per line; `--models <a,b>` names the models it lists
// (default `fake-model-a,fake-model-b`; `none` lists no model). `fake/delegate` (the fake's
// own method, params `{threadId, outcome}`) plays a delegated task on that thread as Codex
// CLI 0.167.0 sends it: `turn/started`, the task, an interim agent message, a web search, the
// final agent message and `turn/completed` with `outcome` as its status (`completed` by
// default, `interrupted`, or `failed` with an error).
const args = process.argv.slice(2);
if (args.includes("--version")) {
  console.log("codex-cli 0.0.0-fake");
  process.exit(0);
}
const option = (name) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};
const signedOut = args.includes("--signed-out");
const rejectHandoff = args.includes("--reject-handoff");
const log = option("--log");
const modelNames = option("--models") || "fake-model-a,fake-model-b";
const models = modelNames === "none" ? [] : modelNames.split(",").filter(Boolean);
let threads = 0;
let realtimeStarts = 0;

const reply = (id, result) => process.stdout.write(JSON.stringify({ id, result }) + "\n");
const fail = (id, message) =>
  process.stdout.write(JSON.stringify({ id, error: { code: -32601, message } }) + "\n");
const notify = (method, params) =>
  process.stdout.write(JSON.stringify({ method, params }) + "\n");

let turns = 0;
// A delegated task as the voice hands it to the agent, then the agent's work on it.
const delegate = (threadId, outcome) => {
  turns += 1;
  const turnId = `fake-turn-${turns}`;
  const item = (n, body) =>
    notify("item/completed", { threadId, turnId, completedAtMs: Date.now(), item: { id: `${turnId}-${n}`, ...body } });
  notify("turn/started", { threadId, turn: { id: turnId, items: [], status: "inProgress" } });
  item(1, {
    type: "userMessage",
    content: [{
      type: "text",
      text: `<realtime_delegation>\n  <input>Find the weather in Bangkok tomorrow (${turns})</input>\n  <transcript_delta>user: weather</transcript_delta>\n</realtime_delegation>`,
    }],
  });
  item(2, { type: "agentMessage", text: "I'll look that up.", phase: "commentary" });
  item(3, { type: "webSearch", query: "Bangkok weather tomorrow" });
  item(4, {
    type: "agentMessage",
    text: `**Bangkok, tomorrow (${turns})**: sunny, 33 °C.\n\n- Rain: 10 %\n- Source: [the forecast](https://example.com/forecast)`,
    phase: "final_answer",
  });
  const turn = { id: turnId, items: [], status: outcome, durationMs: 4200 };
  if (outcome === "failed") turn.error = { message: "the fake agent failed" };
  notify("turn/completed", { threadId, turn });
};

const lines = require("readline").createInterface({ input: process.stdin });
lines.on("line", (line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    return;
  }
  if (message.id === undefined || !message.method) return;
  if (log) require("fs").appendFileSync(log, `${message.method}\n`);
  switch (message.method) {
    case "initialize":
      reply(message.id, { userAgent: "fake-codex" });
      break;
    case "account/read":
      reply(message.id, {
        account: signedOut ? null : { type: "chatgpt", email: "test@example.invalid", planType: "plus" },
        requiresOpenaiAuth: true,
      });
      break;
    case "thread/realtime/listVoices":
      reply(message.id, { voices: { v3: ["fake-cove", "fake-juniper"], defaultV3: "fake-cove" } });
      break;
    case "model/list":
      reply(message.id, {
        data: models.map((id, index) => ({
          id,
          displayName: id.toUpperCase(),
          description: "",
          supportedReasoningEfforts: [{ reasoningEffort: "low" }, { reasoningEffort: "high" }],
          defaultReasoningEffort: "low",
          isDefault: index === 0,
          hidden: false,
        })),
        nextCursor: "",
      });
      break;
    case "thread/start":
      threads += 1;
      reply(message.id, { thread: { id: `fake-thread-${process.pid}-${threads}` } });
      break;
    case "thread/realtime/start": {
      realtimeStarts += 1;
      const params = message.params || {};
      const handoff = "codexResponsesAsItems" in params || "codexResponseHandoffMode" in params;
      if (rejectHandoff && handoff) {
        fail(message.id, "unknown field in thread/realtime/start");
        break;
      }
      reply(message.id, { attempt: realtimeStarts, received: params });
      break;
    }
    case "fake/delegate": {
      const params = message.params || {};
      reply(message.id, {});
      delegate(params.threadId, params.outcome || "completed");
      break;
    }
    default:
      fail(message.id, `${message.method} is not in the fake`);
  }
});
lines.on("close", () => process.exit(0));
