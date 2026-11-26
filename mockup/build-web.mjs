// Assembles the deployable site in dist/: the self-contained notebook page (built by
// build.mjs) plus the web app manifest, icons and offline service worker in public/.
import { cp, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

process.chdir(fileURLToPath(new URL(".", import.meta.url)));
await rm("dist", { recursive: true, force: true });
await cp("public", "dist", { recursive: true });
await cp("index.html", "dist/index.html");
await writeFile("dist/.nojekyll", "");
