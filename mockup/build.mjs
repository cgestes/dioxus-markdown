import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";
import * as Y from "yjs";
import { examples } from "./src/notes.mjs";
import { buildCheckbox, checkboxNotices } from "./build-checkbox.mjs";

process.chdir(fileURLToPath(new URL(".", import.meta.url)));

// Reconstruct the original four-note seed to read pre-Loro browser notebooks.
const seed = new Y.Doc();
seed.clientID = 141889;
for (const { id, name, markdown } of examples.filter(n => n.id !== "departure")) {
  seed.getMap("files").set(id, name);
  seed.getText(id).insert(0, markdown);
}
const bundle = await build({
  entryPoints: ["src/main.js"], bundle: true, minify: true, write: false,
  format: "iife", target: ["safari17", "chrome119"], legalComments: "inline", metafile: true,
  define: {
    MARGIN_LEGACY_SEED: JSON.stringify(Buffer.from(Y.encodeStateAsUpdate(seed)).toString("base64")),
    MARGIN_CHECKBOX_WASM: JSON.stringify(buildCheckbox().toString("base64"))
  }
});
const icons = [
  "bold", "italic", "strikethrough", "ellipsis", "arrow-down-to-line", "arrow-up", "arrow-up-right", "book-open", "check",
  "chevron-down", "chevron-right", "code-xml", "file-text", "folder-open",
  "hard-drive", "list", "map-pin", "panel-left", "plus", "search", "sparkles",
  "square-pen", "table-2", "users", "x"
];
const symbols = await Promise.all(icons.map(async name => {
  const svg = await readFile(`node_modules/lucide-static/icons/${name}.svg`, "utf8");
  return `<symbol id="i-${name}" viewBox="0 0 24 24">${svg.match(/<svg\b[^>]*>([\s\S]*?)<\/svg>/)[1]}</symbol>`;
}));
// The typefaces assets/theme.css names (--sans, --mono), shared with the desktop app.
const fonts = [
  ["Geist", "geist", 400, "normal"], ["Geist", "geist", 400, "italic"],
  ["Geist", "geist", 500, "normal"], ["Geist", "geist", 600, "normal"],
  ["JetBrains Mono", "jetbrains-mono", 400, "normal"], ["JetBrains Mono", "jetbrains-mono", 400, "italic"],
  ["JetBrains Mono", "jetbrains-mono", 500, "normal"], ["JetBrains Mono", "jetbrains-mono", 600, "normal"]
];
const faces = await Promise.all(fonts.map(async ([family, slug, weight, style]) => {
  const bytes = await readFile(`../crates/margin-ui/assets/fonts/${slug}-latin-${weight}-${style}.woff2`);
  return `@font-face{font-family:"${family}";font-weight:${weight};font-style:${style};font-display:swap;src:url(data:font/woff2;base64,${bytes.toString("base64")}) format("woff2")}`;
}));
const fontNotices = await Promise.all([...new Set(fonts.map(f => f[1]))].map(async slug =>
  slug + "\n" + await readFile(`../crates/margin-ui/assets/fonts/${slug}-LICENSE.txt`, "utf8")));
// Keep redistribution notices inside the standalone copy as well as in npm packages.
const packages = new Set(["lucide-static"]);
for (const input of Object.keys(bundle.metafile.inputs)) {
  const match = input.match(/^node_modules\/((?:@[^/]+\/)?[^/]+)/);
  if (match) packages.add(match[1]);
}
const licenses = await Promise.all([...packages].map(async pkg => {
  if (pkg === "loro-protocol") return pkg + "\n" + await readFile("../assets/loro-protocol-LICENSE.txt","utf8");
  const directory = `node_modules/${pkg}/`;
  for (const name of ["LICENSE", "LICENSE.txt", "LICENSE.md", "OFL.txt", "COPYING"]) {
    try { return pkg + "\n" + await readFile(directory + name, "utf8"); }
    catch (error) { if (error.code !== "ENOENT") throw error; }
  }
  throw new Error("Missing distribution notice for " + pkg);
}));
let html = await readFile("src/shell.html", "utf8");
const css = (await Promise.all(["../crates/margin-ui/assets/theme.css", "../crates/margin-ui/assets/table.css", "src/editor.css"].map(path => readFile(path, "utf8")))).join("\n");
for (const [key, value] of Object.entries({
  FONTS: faces.join("\n"), CSS: css, ICONS: symbols.join("\n"),
  BUNDLE: bundle.outputFiles[0].text.replace(/<\/script/gi, "<\\/script"),
  LICENSES: [...licenses, ...fontNotices, ...checkboxNotices()].join("\n\n").replace(/<\/script/gi, "<\\/script")
})) html = html.replace("<!--" + key + "-->", () => value);
await writeFile("index.html", html);
