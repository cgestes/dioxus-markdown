// Checks the installed web app's offline copy (crates/margin-web/public/sw.js)
// against a release bundle: one online visit is enough to open the notebook
// offline, and the files of an earlier build are dropped.
//
//   dx bundle --web --release --package margin-web
//   npm --prefix mockup run test:web:offline               (or pass the bundle's directory)
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const bundle = process.argv[2] || fileURLToPath(new URL("../../target/dx/margin-web/release/web/public", import.meta.url));
const types = {
  ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".svg": "image/svg+xml",
  ".png": "image/png", ".woff2": "font/woff2", ".webmanifest": "application/manifest+json",
};
const server = createServer(async (request, response) => {
  const path = normalize(decodeURIComponent(new URL(request.url, "http://bundle").pathname));
  const file = path.endsWith("/") ? path + "index.html" : path;
  try {
    const body = await readFile(join(bundle, file));
    response.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" }).end(body);
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise(listening => server.listen(0, "127.0.0.1", listening));
const origin = `http://127.0.0.1:${server.address().port}/`;
const browser = await chromium.launch({ channel: process.env.MARGIN_CHROME_CHANNEL || "chrome" });

try {
  const page = await (await browser.newContext()).newPage();
  await page.goto(origin);
  await page.waitForSelector("#note-title");
  await page.evaluate(() => document.fonts.ready);

  // An earlier build left its wasm in the cache. The page registers the worker
  // only on https, so the test does it here.
  const wasmFile = (await readdir(join(bundle, "assets"))).find(name => /-dxh[0-9a-f]+\.wasm$/.test(name));
  const earlier = `/assets/${wasmFile.replace(/-dxh[0-9a-f]+\.wasm$/, "")}-dxh0123456789abcdef.wasm`;
  await page.evaluate(async earlier => {
    const cache = await caches.open("margin-shell-v3");
    await cache.put(earlier, new Response("an earlier build"));
    await navigator.serviceWorker.register("sw.js");
    await navigator.serviceWorker.ready;
  }, earlier);
  // The page tells the worker what it loaded before the worker was there: the
  // worker keeps this build's script, wasm and fonts, and drops the earlier wasm.
  const cached = () => page.evaluate(async () =>
    (await (await caches.open("margin-shell-v3")).keys()).map(request => new URL(request.url).pathname));
  let files = [];
  for (const deadline = Date.now() + 15000; Date.now() < deadline; await page.waitForTimeout(200)) {
    files = await cached();
    const wasm = files.filter(path => path.endsWith(".wasm"));
    if (wasm.length === 1 && wasm[0] !== earlier) break;
  }
  assert.deepEqual(files.filter(path => path.endsWith(".wasm") && path !== earlier).length, 1, `this build's wasm is kept: ${files}`);
  assert.ok(!files.includes(earlier), "the earlier build's wasm is dropped");
  for (const kind of [".js", ".woff2"]) assert.ok(files.some(path => path.endsWith(kind)), `a ${kind} file is kept: ${files}`);

  // With the server gone, the notebook still opens and edits.
  await new Promise(closed => { server.close(closed); server.closeAllConnections(); });
  await page.reload();
  await page.waitForSelector("#note-title");
  const body = await page.locator("#note-body").boundingBox();
  await page.mouse.click(body.x + body.width - 20, body.y + 12);
  await page.keyboard.type(" Offline");
  await page.waitForTimeout(300);
  assert.match(await page.inputValue("#note-body"), /Offline\s*$/);
  assert.ok(await page.evaluate(() => [...document.fonts].some(face => face.status === "loaded")), "the fonts come from the copy");
  console.log("Passed web offline: one visit caches the bundle, an earlier build is dropped, the notebook opens without the network.");
} finally {
  await browser.close();
  server.close();
}
