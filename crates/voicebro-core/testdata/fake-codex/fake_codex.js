// A stand-in for `codex app-server` in the unit tests. It answers the handshake, the
// account, the voices, the models, `thread/start` and `thread/realtime/start` over JSON-RPC
// on stdio, and exits when stdin closes. `--version` prints a version; `--signed-out`
// reports no account; `--reject-handoff` refuses a realtime start that carries an answer
// handoff field, as a Codex version without those fields would. A realtime start answers
// with its attempt number and the parameters it received. `--log <file>` appends the method
// of every request to that file, one per line; `--models <a,b>` names the models it lists
// (default `fake-model-a,fake-model-b`; `none` lists no model).
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
    default:
      fail(message.id, `${message.method} is not in the fake`);
  }
});
lines.on("close", () => process.exit(0));
