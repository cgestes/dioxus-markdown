// Renders public/icon.svg into the PNG sizes the web manifest, iOS home screen and Xcode asset catalog use.
// Run `npm run icons` after changing the SVG; the PNGs are committed so builds do not need Chrome.
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

process.chdir(fileURLToPath(new URL(".", import.meta.url)));
const svg = await readFile("public/icon.svg", "utf8");
const targets = [
  ["public/icons/apple-touch-icon.png", 180, 1],
  ["public/icons/icon-192.png", 192, 1],
  ["public/icons/icon-512.png", 512, 1],
  // Maskable icons keep their content inside the central 80% circle.
  ["public/icons/icon-maskable-512.png", 512, 0.8],
  ["../ios/Margin/Assets.xcassets/AppIcon.appiconset/AppIcon.png", 1024, 1]
];
const browser = await chromium.launch({ channel: process.env.MARGIN_CHROME_CHANNEL || "chrome" });
try {
  for (const [path, size, scale] of targets) {
    const page = await browser.newPage({ viewport: { width: size, height: size } });
    await page.setContent(`<style>html,body{margin:0;background:#4342ad}svg{display:block;width:${size}px;height:${size}px;transform:scale(${scale})}</style>${svg}`);
    await page.screenshot({ path, omitBackground: false });
    await page.close();
  }
} finally { await browser.close(); }
