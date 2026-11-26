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

async function open(viewport, { mobile = false, prepare, early } = {}) {
  const context = await browser.newContext({
    viewport, isMobile: mobile, hasTouch: mobile, acceptDownloads: true,
    permissions: ["clipboard-read", "clipboard-write"],
  });
  // Count the page's selection listeners, to see that a view leaves none behind.
  await context.addInitScript(() => {
    window.selectionListeners = 0;
    const { addEventListener, removeEventListener } = EventTarget.prototype;
    EventTarget.prototype.addEventListener = function (type, ...rest) {
      if (type === "selectionchange") window.selectionListeners++;
      return addEventListener.call(this, type, ...rest);
    };
    EventTarget.prototype.removeEventListener = function (type, ...rest) {
      if (type === "selectionchange") window.selectionListeners--;
      return removeEventListener.call(this, type, ...rest);
    };
  });
  await prepare?.(context);
  const page = await context.newPage();
  page.on("pageerror", e => errors.push(e.message));
  page.on("console", m => { if (m.type() === "error") errors.push(m.text()); });
  await page.goto(url);
  await page.waitForSelector("#note-title");
  await early?.(page);
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(500);
  return page;
}
const settle = page => page.waitForTimeout(250);
async function view(page, mode, field) {
  await page.click(`#${mode}-mode`);
  await page.waitForSelector(field);
  await page.waitForTimeout(150);
}
// The note's Markdown, read back through the Source view.
async function markdown(page) {
  await view(page, "source", "#markdown");
  const md = await page.inputValue("#markdown");
  await view(page, "live", "#note-title");
  return md;
}
// Replace the whole note, through the Source view.
async function write(page, md) {
  await view(page, "source", "#markdown");
  await page.click("#markdown");
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.insertText(md);
  await view(page, "live", "#note-title");
}
const active = page => page.evaluate(() => `${document.activeElement.id}@${document.activeElement.selectionStart}`);
const copy = (page, text) => page.evaluate(text => navigator.clipboard.writeText(text), text);
const box = (page, selector) => page.locator(selector).first().boundingBox();
const middle = b => [b.x + b.width / 2, b.y + b.height / 2];

try {
  // Desktop, mouse and keyboard.
  let page = await open({ width: 1000, height: 760 });
  const initial = await markdown(page);
  assert.match(initial, /^# Field notes\n/);
  assert.ok(await page.locator(".md-ink").count() > 0, "styled runs are painted over the fields");

  // Click places the caret; typing reaches the note.
  let body = await box(page, "#note-body");
  await page.mouse.click(body.x + body.width - 20, body.y + 12);
  await settle(page);
  assert.equal(await active(page), "note-body@40");
  await page.keyboard.type(" Hi");
  await settle(page);
  assert.equal((await markdown(page)).split("\n")[2], "Recorder packed [ ]. Rain cover ready [x]. Hi");
  for (let i = 0; i < 3; i++) await page.keyboard.press("ControlOrMeta+z");
  await settle(page);
  assert.equal(await markdown(page), initial, "undo restores the note");

  // Shortcuts are labelled for the keyboard in use.
  const apple = await page.evaluate(() => /^(Mac|iP)/.test(navigator.platform));
  assert.equal(await page.getAttribute("#undo", "title"), apple ? "Undo (⌘Z)" : "Undo (Ctrl+Z)");

  // Live mode shows a span's Markdown only while the caret is inside it.
  await page.mouse.click(...middle(await box(page, ".md-ink.strong")));
  await settle(page);
  assert.match(await page.evaluate(() => document.activeElement.value), /\*\*ten seconds\*\*/);

  // A click toggles a checkbox; the next click in the text lands where it was made.
  await page.mouse.click(...middle(await box(page, "[id^=checkbox-]")));
  await settle(page);
  assert.equal((await markdown(page)).split("\n")[2], "Recorder packed [x]. Rain cover ready [x].");

  // Cells: type, Tab to the next one.
  const cell = await box(page, "#cell-0-1-1");
  await page.mouse.click(cell.x + cell.width - 6, cell.y + cell.height / 2);
  await settle(page);
  await page.keyboard.type(" now");
  await page.keyboard.press("Tab");
  await settle(page);
  assert.equal(await page.evaluate(() => document.activeElement.id), "cell-0-1-2");
  assert.match(await markdown(page), /\| North lock \| Water through the gate now \| 45 s \|/);

  // Paste fills a selected range of cells, and wraps a selected word in a pasted link.
  await copy(page, "Dawn");
  await page.mouse.click(...middle(await box(page, "#cell-0-1-0")));
  await page.keyboard.down("Shift");
  await page.mouse.click(...middle(await box(page, "#cell-0-2-0")));
  await page.keyboard.up("Shift");
  await settle(page);
  await page.keyboard.press("ControlOrMeta+v");
  await settle(page);
  assert.match(await markdown(page), /\| Dawn \| Water through the gate now \| 45 s \|\n\| Dawn \| Steps/);
  await copy(page, "https://example.com/lock");
  body = await box(page, "#note-body");
  await page.mouse.dblclick(body.x + 30, body.y + 12);
  await settle(page);
  await page.keyboard.press("ControlOrMeta+v");
  await settle(page);
  assert.match((await markdown(page)).split("\n")[2], /^\[Recorder\]\(https:\/\/example\.com\/lock\) packed/);
  for (let i = 0; i < 2; i++) await page.keyboard.press("ControlOrMeta+z");
  await settle(page);

  // Enter continues a checklist.
  const lastBody = await page.evaluate(() => [...document.querySelectorAll("textarea.body-editor")].pop().id);
  const list = await box(page, "#" + lastBody);
  await page.mouse.click(list.x + list.width - 10, list.y + list.height - 8);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("New item");
  await settle(page);
  assert.equal((await markdown(page)).split("\n").pop(), "- [ ] New item");

  // Arrow keys cross fields: title, blank line, prose.
  const title = await box(page, "#note-title");
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
  body = await box(page, "#note-body");
  await page.mouse.click(body.x + 4, body.y + 12);
  await settle(page);
  for (let i = 0; i < 8; i++) await page.keyboard.press("Shift+ArrowRight");
  await settle(page);
  assert.ok(await page.locator("#format-bar").count(), "the format bar follows a selection");
  await page.keyboard.press("ControlOrMeta+b");
  await settle(page);
  assert.match((await markdown(page)).split("\n")[2], /^\*\*Recorder\*\* packed/);

  // A slash on an empty line opens the block menu.
  body = await box(page, "#note-body");
  await page.mouse.click(body.x + body.width - 20, body.y + 12);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("/");
  await settle(page);
  assert.match(await page.locator("#slash-menu").innerText(), /Heading 1/);
  await page.keyboard.press("Escape");

  // The inspector records inputs and saves its report as a download.
  await page.click("#toggle-debug");
  await page.waitForSelector("#save-inspection");
  await page.mouse.click(...middle(await box(page, "#note-title")));
  await page.keyboard.press("ArrowRight");
  await settle(page);
  assert.match(await page.locator("#inspect-status").innerText(), /"ArrowRight"/);
  const [download] = await Promise.all([page.waitForEvent("download"), page.click("#save-inspection")]);
  assert.match(download.suggestedFilename(), /^margin-report-\d+\.json$/);
  assert.match(await page.locator("#inspect-save-status").innerText(), /^Saved margin-report-\d+\.json/);
  await page.click("#toggle-debug");

  // Up and Down keep their column past a zero-width character.
  await write(page, "# Lines\n\nThe first line is a long one\nab​cdefghijklmnopqrstuvwxyz");
  body = await box(page, "#note-body");
  await page.mouse.click(body.x + body.width - 20, body.y + 10);
  await settle(page);
  await page.keyboard.press("ArrowDown");
  await settle(page);
  const [, column] = (await active(page)).split("@").map(Number);
  assert.ok(column > "The first line is a long one\nab​cdefghij".length, `Down lands under the caret, at ${column}`);

  // Source scrolls inside its field, and Live opens on the caret Source left.
  const long = "# Long\n\n" + Array.from({ length: 120 }, (_, i) => `Line ${i + 1} of one tall paragraph.`).join("\n");
  await write(page, long);
  await view(page, "source", "#markdown");
  await page.mouse.move(...middle(await box(page, "#markdown")));
  await page.mouse.wheel(0, 600);
  await settle(page);
  assert.ok(await page.evaluate(() => document.querySelector("#markdown").scrollTop) > 300, "the wheel scrolls the Markdown");
  await page.click("#markdown");
  await page.keyboard.press("ControlOrMeta+End");
  await view(page, "live", "#note-title");
  await settle(page);
  const caret = await box(page, ".caret-ink");
  assert.ok(caret && caret.y > 0 && caret.y + caret.height < 760, `Live scrolls to the caret: ${JSON.stringify(caret)}`);

  // Each view removes the listeners it added.
  for (let i = 0; i < 5; i++) {
    await view(page, "source", "#markdown");
    await view(page, "live", "#note-title");
  }
  assert.equal(await page.evaluate(() => window.selectionListeners), 1, "one selection listener in Live");
  await page.close();

  // Fonts that arrive after the text was first measured move the painted runs
  // with them. Rich mode sets prose in the proportional face, whose stand-in
  // has other widths.
  const runs = page => page.evaluate(() => [...document.querySelectorAll(".md-ink")].map(run => {
    const { x, width } = run.getBoundingClientRect();
    return [x, width].map(value => Math.round(value * 10) / 10);
  }));
  const rich = page => page.click("#rich-mode");
  page = await open({ width: 1000, height: 760 });
  await rich(page);
  await settle(page);
  const settled = await runs(page);
  await page.close();
  page = await open({ width: 1000, height: 760 }, {
    prepare: context => context.route("**/*.woff2", async route => {
      await new Promise(done => setTimeout(done, 1500));
      await route.continue();
    }),
    early: rich,
  });
  assert.deepEqual(await runs(page), settled, "styled runs sit where the loaded fonts put them");
  await page.close();

  // Phone, touch: taps place the caret and toggle checkboxes.
  page = await open({ width: 393, height: 560 }, { mobile: true });
  await page.touchscreen.tap(...middle(await box(page, "[id^=checkbox-]")));
  await settle(page);
  body = await box(page, "#note-body");
  await page.touchscreen.tap(body.x + 30, body.y + 12);
  await settle(page);
  assert.equal(await active(page), "note-body@4", "a tap after a checkbox lands where it was made");
  assert.equal((await markdown(page)).split("\n")[2], "Recorder packed [x]. Rain cover ready [x].");

  // The page is as wide as the screen, and a column's handle sits over its column.
  // (A phone widens its viewport to fit a wider page, so compare with the screen.)
  const [page_width, screen_width] = await page.evaluate(() => [document.documentElement.scrollWidth, screen.width]);
  assert.ok(page_width <= screen_width, `the page is ${page_width}px wide on a ${screen_width}px screen`);
  await page.touchscreen.tap(...middle(await box(page, "#cell-0-1-2")));
  await settle(page);
  const [handle] = middle(await box(page, "#col-menu-btn-0-2"));
  const [under] = middle(await box(page, "#slot-0-0-2"));
  assert.ok(Math.abs(handle - under) < 2, `handle at ${handle} over a column centred at ${under}`);

  // A finger that scrolls the page over the text leaves the caret where it was.
  body = await box(page, "#note-body");
  await page.touchscreen.tap(body.x + 30, body.y + 12);
  await settle(page);
  const touch = await page.context().newCDPSession(page);
  const finger = (type, y) => touch.send("Input.dispatchTouchEvent", {
    type, touchPoints: type === "touchEnd" ? [] : [{ x: Math.round(body.x + 75), y: Math.round(y) }],
  });
  await finger("touchStart", body.y + 12);
  for (let step = 1; step <= 8; step++) await finger("touchMove", body.y + 12 - step * 12);
  await finger("touchEnd");
  await settle(page);
  assert.ok(await page.evaluate(() => scrollY) > 0, "the drag scrolled the page");
  assert.equal(await active(page), "note-body@4", "scrolling over the text does not move the caret");
  await page.close();

  assert.deepEqual(errors, [], "no console errors");
  console.log("Passed web editor: typing, undo, Live spans, checkboxes, cells, paste, lists, arrows, formatting, slash menu, inspector, Source, late fonts and touch.");
} finally {
  await browser.close();
}
