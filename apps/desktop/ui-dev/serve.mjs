// Serves `apps/desktop` on http://127.0.0.1:5174 so the mock page runs in a browser that
// will not run scripts from `file://` (the Claude desktop browser pane is one). Generates
// `ui-dev/index.html` first. Local only; never shipped.
//
//   node apps/desktop/ui-dev/serve.mjs     then open http://127.0.0.1:5174/
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { dirname, extname, join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
execFileSync(process.execPath, [join(here, "make.mjs")], { stdio: "inherit" });

const TYPES = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".css": "text/css" };
const PORT = Number(process.env.PORT || 5174);

createServer(async (request, response) => {
  const path = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
  if (path === "/") {
    response.writeHead(302, { Location: "/ui-dev/index.html" });
    response.end();
    return;
  }
  const file = normalize(join(root, path));
  const allowed = [join(root, "ui") + sep, join(root, "ui-dev") + sep];
  if (!allowed.some((prefix) => file.startsWith(prefix))) {
    response.writeHead(404);
    response.end();
    return;
  }
  try {
    const body = await readFile(file);
    response.writeHead(200, { "Content-Type": TYPES[extname(file)] || "application/octet-stream", "Cache-Control": "no-store" });
    response.end(body);
  } catch {
    response.writeHead(404);
    response.end();
  }
}).listen(PORT, "127.0.0.1", () => console.log(`ui-dev on http://127.0.0.1:${PORT}/`));
