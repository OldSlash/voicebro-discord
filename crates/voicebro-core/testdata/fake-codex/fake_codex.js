// A stand-in for `codex app-server` in the unit tests. It answers the handshake, the
// account, the voices, the models, `thread/start` and `thread/realtime/start` over JSON-RPC
// on stdio, and exits when stdin closes. `--version` prints a version; `--signed-out`
// reports no account; `--reject-handoff` refuses a realtime start that carries an answer
// handoff field, as a Codex version without those fields would. A realtime start answers
// with its attempt number and the parameters it received.
const args = process.argv.slice(2);
if (args.includes("--version")) {
  console.log("codex-cli 0.0.0-fake");
  process.exit(0);
}
const signedOut = args.includes("--signed-out");
const rejectHandoff = args.includes("--reject-handoff");
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
      reply(message.id, { voices: [] });
      break;
    case "model/list":
      reply(message.id, { data: [] });
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
