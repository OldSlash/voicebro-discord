// A stand-in for `codex app-server` in the unit tests. It answers the handshake, the
// account, the voices, the models and `thread/start` over JSON-RPC on stdio, and exits when
// stdin closes. `--version` prints a version; `--signed-out` reports no account.
const args = process.argv.slice(2);
if (args.includes("--version")) {
  console.log("codex-cli 0.0.0-fake");
  process.exit(0);
}
const signedOut = args.includes("--signed-out");
let threads = 0;

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
    default:
      fail(message.id, `${message.method} is not in the fake`);
  }
});
lines.on("close", () => process.exit(0));
