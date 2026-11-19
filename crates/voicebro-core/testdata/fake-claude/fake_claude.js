// A stand-in for Claude Code (`claude.exe`) in the unit tests (change
// `claude-and-opencode-engines`, design D14). It knows what VoiceBro runs:
//
// - `--version` prints a version.
// - `auth status` prints the JSON Claude Code 2.1.317 prints: a Max subscription by default,
//   `--kind=api_key` an API key, `--kind=none` no sign-in; with ANTHROPIC_API_KEY set it
//   reports the key, as the real CLI does.
// - `-p --input-format stream-json --output-format stream-json …` reads one JSON object per
//   stdin line. A `user` line is a question: answered in order, one at a time, a line that
//   comes meanwhile waits. Each answer is a `system/init`, an interim message ("Let me check
//   that."), a web search tool use, and the final message ("You asked: <question>. In
//   London it is 9 degrees."), each streamed as `stream_event` text deltas and then given
//   whole as an `assistant` message, and a `result`.
// - Control requests: `initialize` answers with models and the account; `interrupt` ends the
//   running answer with an `error_during_execution` result (`aborted_streaming`); the fake's
//   own `fake_delay {ms}` spaces the pieces, `fake_last_user` returns the last question as
//   received, `fake_exit` ends the process.
// - `--fake-log <file>` appends the arguments and every stdin line to that file.
const fs = require("fs");
const args = process.argv.slice(2);
const option = (name) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};
const kindArg = args.find((arg) => arg.startsWith("--kind="));
const kind = kindArg ? kindArg.slice("--kind=".length) : "subscription";
const logFile = option("--fake-log");
const log = (line) => {
  if (logFile) fs.appendFileSync(logFile, line + "\n");
};
log("RUN " + JSON.stringify(args.filter((arg) => arg !== "--fake-log" && arg !== logFile)));

if (args.includes("--version")) {
  console.log("0.0.0-fake (Claude Code)");
  process.exit(0);
}

if (args[0] === "auth" || args.includes("auth")) {
  let status;
  if (process.env.ANTHROPIC_API_KEY || kind === "api_key") {
    status = { loggedIn: true, authMethod: "api_key", apiProvider: "firstParty", apiKeySource: "ANTHROPIC_API_KEY" };
  } else if (kind === "none") {
    status = { loggedIn: false, authMethod: "none", apiProvider: "firstParty" };
  } else {
    status = {
      loggedIn: true, authMethod: "claude.ai", apiProvider: "firstParty",
      email: "fake@example.com", orgId: "org-fake", orgName: "Fake", subscriptionType: "max",
    };
  }
  console.log(JSON.stringify(status, null, 2));
  process.exit(status.loggedIn ? 0 : 1);
}

log("ARGS " + JSON.stringify(args));
const write = (message) => process.stdout.write(JSON.stringify(message) + "\n");
const session = "fake-session-" + process.pid;
let delay = 0;
let lastUser = null;
const queue = [];
let running = null; // { cancelled }

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function stream(turn, text) {
  write({ type: "stream_event", event: { type: "content_block_start", index: 0, content_block: { type: "text", text: "" } }, session_id: session });
  const words = text.split(/(?<= )/);
  for (const word of words) {
    if (turn.cancelled) return false;
    write({ type: "stream_event", event: { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: word } }, session_id: session });
    if (delay) await sleep(delay);
  }
  if (turn.cancelled) return false;
  write({ type: "assistant", message: { role: "assistant", content: [{ type: "text", text }] }, session_id: session });
  return true;
}

async function answer(question) {
  const turn = { cancelled: false };
  running = turn;
  write({ type: "system", subtype: "init", session_id: session, model: "fake-model", tools: ["WebFetch", "WebSearch"] });
  let done = await stream(turn, "Let me check that.");
  if (done) {
    write({ type: "assistant", message: { role: "assistant", content: [{ type: "tool_use", id: "tool-1", name: "WebSearch", input: { query: "London weather" } }] }, session_id: session });
    if (delay) await sleep(delay);
    done = !turn.cancelled && (await stream(turn, `You asked: ${question}. In London it is 9 degrees.`));
  }
  if (turn.cancelled || !done) {
    write({ type: "result", subtype: "error_during_execution", is_error: true, terminal_reason: "aborted_streaming", session_id: session });
  } else {
    write({ type: "result", subtype: "success", is_error: false, result: `You asked: ${question}. In London it is 9 degrees.`, terminal_reason: "completed", session_id: session });
  }
  running = null;
}

async function pump() {
  while (queue.length > 0) {
    if (running) return;
    const question = queue.shift();
    await answer(question);
  }
}

const respond = (id, response) =>
  write({ type: "control_response", response: { subtype: "success", request_id: id, response } });

let buffer = "";
process.stdin.on("data", (chunk) => {
  buffer += chunk.toString("utf8");
  let index;
  while ((index = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, index).trim();
    buffer = buffer.slice(index + 1);
    if (!line) continue;
    log("IN " + line);
    let message;
    try { message = JSON.parse(line); } catch { continue; }
    if (message.type === "user") {
      lastUser = message.message.content;
      queue.push(lastUser);
      pump();
    } else if (message.type === "control_request") {
      const request = message.request || {};
      switch (request.subtype) {
        case "initialize":
          respond(message.request_id, {
            models: [
              { value: "default", displayName: "Default (recommended)", description: "Fake default" },
              { value: "sonnet", displayName: "Sonnet", description: "Fake sonnet" },
            ],
            account: { email: "fake@example.com", subscriptionType: "Claude Max" },
          });
          break;
        case "interrupt":
          if (running) running.cancelled = true;
          respond(message.request_id, { still_queued: [] });
          break;
        case "fake_delay":
          delay = request.ms || 0;
          respond(message.request_id, {});
          break;
        case "fake_last_user":
          respond(message.request_id, { content: lastUser });
          break;
        case "fake_exit":
          respond(message.request_id, {});
          setTimeout(() => process.exit(0), 10);
          break;
        default:
          write({ type: "control_response", response: { subtype: "error", request_id: message.request_id, error: "unknown request" } });
      }
    }
  }
});
process.stdin.on("end", () => process.exit(0));
