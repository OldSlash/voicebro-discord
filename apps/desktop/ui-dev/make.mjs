// Writes `ui-dev/index.html`: the real `ui/index.html` with the mock of the app's commands
// loaded before the real `../ui/main.js` and `../ui/styles.css`. Run it after the page
// changes, then open `ui-dev/index.html` from disk in a browser:
//
//   node apps/desktop/ui-dev/make.mjs
//
// The generated page is not committed (it would drift from `ui/index.html`).
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const page = readFileSync(join(here, "..", "ui", "index.html"), "utf8");
const dev = page
  .replace('href="styles.css"', 'href="../ui/styles.css"')
  .replace(
    '<script src="main.js"></script>',
    '<script src="mock.js"></script>\n    <script src="../ui/main.js"></script>',
  )
  .replace("<title>VoiceBro</title>", "<title>VoiceBro (mock)</title>");
if (dev === page || !dev.includes("mock.js") || !dev.includes("../ui/styles.css")) {
  throw new Error("ui/index.html no longer has the expected styles.css and main.js tags");
}
writeFileSync(join(here, "index.html"), dev);
console.log(`wrote ${join(here, "index.html")}`);
