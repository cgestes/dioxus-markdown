// Drives the Dioxus Web build of the notebook in Chrome: the same editor as
// the native app, so this is the quickest place to exercise it end to end.
//
//   dx serve --web --package margin-editor --port 8741     (in another terminal)
//   npm --prefix mockup run test:web                       (MARGIN_WEB_URL overrides the address)
import assert from "node:assert/strict";
import { chromium } from "playwright-core";

const url = process.env.MARGIN_WEB_URL || "http://127.0.0.1:8741/?demo";
const browser = await chromium.launch({ channel: process.env.MARGIN_CHROME_CHANNEL || "chrome" });
const errors = [];

async function open(viewport, mobile = false) {
  const page = await (await browser.newContext({ viewport, isMobile: mobile, hasTouch: mobile })).newPage();
  page.on("pageerror", e => errors.push(e.message));
  page.on("console", m => { if (m.type() === "error") errors.push(m.text()); });
  await page.goto(url);
  await page.waitForSelector("#note-title");
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(500);
  return page;
}
// The note's Markdown, read back through the Source view.
async function markdown(page) {
  await page.click("#source-mode");
  await page.waitForSelector("#markdown");
  const md = await page.inputValue("#markdown");
  await page.click("#live-mode");
  await page.waitForSelector("#note-title");
  await page.waitForTimeout(150);
  return md;
}
const active = page => page.evaluate(() => `${document.activeElement.id}@${document.activeElement.selectionStart}`);
const settle = page => page.waitForTimeout(250);

try {
  // Desktop, mouse and keyboard.
  let page = await open({ width: 1000, height: 760 });
  const initial = await markdown(page);
  assert.match(initial, /^# Field notes\n/);
  assert.ok(await page.locator(".md-ink").count() > 0, "styled runs are painted over the fields");

  // Click places the caret; typing reaches the note.
  let body = await page.locator("#note-body").boundingBox();
  await page.mouse.click(body.x + body.width - 20, body.y + 12);
  await settle(page);
  assert.equal(await active(page), "note-body@40");
  await page.keyboard.type(" Hi");
  await settle(page);
  assert.equal((await markdown(page)).split("\n")[2], "Recorder packed [ ]. Rain cover ready [x]. Hi");
  for (let i = 0; i < 3; i++) await page.keyboard.press("ControlOrMeta+z");
  await settle(page);
  assert.equal(await markdown(page), initial, "undo restores the note");

  // Live mode shows a span's Markdown only while the caret is inside it.
  const strong = await page.locator(".md-ink.strong").first().boundingBox();
  await page.mouse.click(strong.x + strong.width / 2, strong.y + strong.height / 2);
  await settle(page);
  assert.match(await page.evaluate(() => document.activeElement.value), /\*\*ten seconds\*\*/);

  // A click toggles a checkbox; the next click in the text lands where it was made.
  const box = await page.locator("[id^=checkbox-]").first().boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await settle(page);
  assert.equal((await markdown(page)).split("\n")[2], "Recorder packed [x]. Rain cover ready [x].");

  // Cells: type, Tab to the next one.
  const cell = await page.locator("#cell-0-1-1").boundingBox();
  await page.mouse.click(cell.x + cell.width - 6, cell.y + cell.height / 2);
  await settle(page);
  await page.keyboard.type(" now");
  await page.keyboard.press("Tab");
  await settle(page);
  assert.equal(await page.evaluate(() => document.activeElement.id), "cell-0-1-2");
  assert.match(await markdown(page), /\| North lock \| Water through the gate now \| 45 s \|/);

  // Enter continues a checklist.
  const lastBody = await page.evaluate(() => [...document.querySelectorAll("textarea.body-editor")].pop().id);
  const list = await page.locator("#" + lastBody).boundingBox();
  await page.mouse.click(list.x + list.width - 10, list.y + list.height - 8);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("New item");
  await settle(page);
  assert.equal((await markdown(page)).split("\n").pop(), "- [ ] New item");

  // Arrow keys cross fields: title, blank line, prose.
  const title = await page.locator("#note-title").boundingBox();
  await page.mouse.click(title.x + 60, title.y + title.height / 2);
  await settle(page);
  const stops = [];
  for (let i = 0; i < 6; i++) {
    await page.keyboard.press("ArrowDown");
    await page.waitForTimeout(120);
    stops.push((await active(page)).split("@")[0]);
  }
  assert.deepEqual([...new Set(stops)].slice(0, 4), ["title-gap", "note-body", "note-heading-0", "note-body-1"]);

  // Selecting text opens the format bar; ⌘B/Ctrl+B writes Markdown.
  body = await page.locator("#note-body").boundingBox();
  await page.mouse.click(body.x + 4, body.y + 12);
  await settle(page);
  for (let i = 0; i < 8; i++) await page.keyboard.press("Shift+ArrowRight");
  await settle(page);
  assert.ok(await page.locator("#format-bar").count(), "the format bar follows a selection");
  await page.keyboard.press("ControlOrMeta+b");
  await settle(page);
  assert.match((await markdown(page)).split("\n")[2], /^\*\*Recorder\*\* packed/);

  // A slash on an empty line opens the block menu.
  body = await page.locator("#note-body").boundingBox();
  await page.mouse.click(body.x + body.width - 20, body.y + 12);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("/");
  await settle(page);
  assert.match(await page.locator("#slash-menu").innerText(), /Heading 1/);
  await page.keyboard.press("Escape");

  // Split: the preview follows the source.
  await page.click("#split-mode");
  await page.waitForSelector("#reading-page");
  await page.click("#markdown");
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("\n\nSplit line.");
  await settle(page);
  assert.match(await page.locator("#reading-page").innerText(), /Split line\./);
  await page.close();

  // Phone, touch: taps place the caret and toggle checkboxes.
  page = await open({ width: 393, height: 852 }, true);
  const tapBox = await page.locator("[id^=checkbox-]").first().boundingBox();
  await page.touchscreen.tap(tapBox.x + tapBox.width / 2, tapBox.y + tapBox.height / 2);
  await settle(page);
  body = await page.locator("#note-body").boundingBox();
  await page.touchscreen.tap(body.x + 30, body.y + 12);
  await settle(page);
  assert.equal(await active(page), "note-body@4", "a tap after a checkbox lands where it was made");
  assert.equal((await markdown(page)).split("\n")[2], "Recorder packed [x]. Rain cover ready [x].");
  await page.close();

  assert.deepEqual(errors, [], "no console errors");
  console.log("Passed web editor: typing, undo, Live spans, checkboxes, cells, lists, arrows, formatting, slash menu, Split and touch.");
} finally {
  await browser.close();
}
