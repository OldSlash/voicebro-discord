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
// default, `interrupted`, or `failed` with an error). `fake/transcript` (params `{threadId,
// method, params}`) sends that notification with `threadId` added to its params, as the
// realtime call's transcript notifications arrive.
// Text turns (change `whisper-mode`): `turn/start` answers with the turn, then streams an
// interim agent message and the final one as `item/agentMessage/delta` pieces, each item
// completed, and `turn/completed`; `turn/interrupt` ends the running turn as `interrupted`.
// `fake/turnDelay` (params `{ms}`) waits that long between the pieces; `fake/lastTurnStart`
// returns the params of the last `turn/start`.
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

let turnDelay = 0;
let lastTurnStart = null;
const running = new Map();
// A text turn: the question echoed in the answer, streamed in pieces.
const textTurn = (threadId, turnId, question) => {
  const steps = [];
  const message = (n, text, phase) => {
    const itemId = `${turnId}-${n}`;
    const pieces = text.match(/.{1,12}/gs) || [];
    for (const delta of pieces) {
      steps.push(() => notify("item/agentMessage/delta", { threadId, turnId, itemId, delta }));
    }
    steps.push(() =>
      notify("item/completed", { threadId, turnId, item: { id: itemId, type: "agentMessage", text, phase } }));
  };
  steps.push(() => notify("turn/started", { threadId, turn: { id: turnId, items: [], status: "inProgress" } }));
  message(1, "Let me check that.", "commentary");
  message(2, `You asked: ${question} In Bangkok it is sunny, 33 degrees, with rain later. In London it is 9 degrees.`, "final_answer");
  steps.push(() => {
    running.delete(threadId);
    notify("turn/completed", { threadId, turn: { id: turnId, items: [], status: "completed", durationMs: 1200 } });
  });
  const state = { turnId, timer: null };
  running.set(threadId, state);
  const next = () => {
    const step = steps.shift();
    if (!step || running.get(threadId) !== state) return;
    step();
    if (steps.length) state.timer = setTimeout(next, turnDelay);
  };
  state.timer = setTimeout(next, 0);
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
    case "turn/start": {
      const params = message.params || {};
      lastTurnStart = params;
      turns += 1;
      const turnId = `fake-turn-${turns}`;
      const question = ((params.input || [])[0] || {}).text || "";
      reply(message.id, { turn: { id: turnId, items: [], status: "inProgress" } });
      textTurn(params.threadId, turnId, question);
      break;
    }
    case "turn/interrupt": {
      const params = message.params || {};
      const state = running.get(params.threadId);
      reply(message.id, {});
      if (state && state.turnId === params.turnId) {
        clearTimeout(state.timer);
        running.delete(params.threadId);
        notify("turn/completed", { threadId: params.threadId, turn: { id: state.turnId, items: [], status: "interrupted" } });
      }
      break;
    }
    case "fake/turnDelay":
      turnDelay = (message.params || {}).ms || 0;
      reply(message.id, {});
      break;
    case "fake/lastTurnStart":
      reply(message.id, lastTurnStart);
      break;
    case "fake/transcript": {
      const params = message.params || {};
      notify(params.method, { ...(params.params || {}), threadId: params.threadId });
      reply(message.id, {});
      break;
    }
    default:
      fail(message.id, `${message.method} is not in the fake`);
  }
});
lines.on("close", () => process.exit(0));
