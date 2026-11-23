// Run against the built, self-contained preview in an ordinary Chrome browser.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { exerciseSelectionTools } from "./selection-tools-browser.mjs";
import { exerciseReturnAndColumns } from "./return-column-browser.mjs";
import { exerciseSplit } from "./split-browser.mjs";
import { chromium } from "playwright-core";

const html = await readFile(new URL("../index.html", import.meta.url));
const server = createServer((_, res) => { res.setHeader("Content-Type", "text/html; charset=utf-8"); res.end(html); });
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
let browser;
try {
  browser = await chromium.launch({ channel: process.env.MARGIN_CHROME_CHANNEL || "chrome" });
  const context = await browser.newContext({ viewport: { width: 1100, height: 780 } });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/?room=editing-audit#walk`);
  await page.waitForSelector(".md-grid");
  await page.evaluate(async () => {
    await document.fonts.ready;
    window.view = document.querySelector(".cm-content").cmTile.root.view;
  });
  const md = "# Top\n\nParagraph\n\n## Café noir\n\n| Place | Time |\n| --- | ---: |\n| **Écho** gate | Noon |\n\nTail";
  await page.evaluate(md => { view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: md }, selection: { anchor: 2 } }); view.focus(); }, md);
  const content = () => page.evaluate(() => view.state.doc.toString());
  const selection = () => page.evaluate(() => { const s = view.state.selection.main; return [s.anchor,s.head]; });
  const grid = page.locator(".md-grid"), input = page.locator(".grid-cell-input");
  await grid.scrollIntoViewIfNeeded();
  const rect = await grid.boundingBox();
  await page.mouse.click(rect.x + rect.width + 9, rect.y + 6);
  assert.equal(await input.getAttribute("aria-label"), "Header, column 2");
  assert.deepEqual(await input.evaluate(e => [e.selectionStart,e.selectionEnd]), [4,4]);
  await page.mouse.click(rect.x + rect.width * 0.74, rect.y + rect.height + 4);
  assert.equal(await input.getAttribute("aria-label"), "Row 1, column 2");
  for (const [label,side] of [["Add column","right"],["Add row","bottom"]]) {
    const b = await page.getByRole("button",{name:label,exact:true}).boundingBox();
    const points = side === "right"
      ? [[b.x+b.width/2,b.y-1],[b.x+b.width/2,b.y+b.height+1]]
      : [[b.x-1,b.y+b.height/2],[b.x+b.width+1,b.y+b.height/2]];
    for (const [x,y] of points) {
      await page.mouse.click(x,y);
      assert.equal(await input.evaluate(e => e === document.activeElement),true);
      assert.equal(await content(),md);
    }
  }
  assert.equal(await content(), md);

  const glyph = async (selector, offset) => page.locator(selector).evaluate((el, n) => {
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    // The active cell's sizing label is hidden from assistive technology.
    // Measure its glyphs, skipping the Markdown delimiters inside the label.
    while (walker.nextNode()) {
      const hidden = walker.currentNode.parentElement.closest('[aria-hidden="true"]');
      if (!hidden || hidden === el || !el.contains(hidden)) break;
    }
    const r = document.createRange(); r.setStart(walker.currentNode,n); r.collapse(true);
    const b = r.getBoundingClientRect(); return { x: b.x + 0.15, y: (b.top+b.bottom)/2 };
  }, offset);
  const click = p => page.mouse.click(p.x,p.y);
  const e = await glyph(".md-grid td strong",2);
  await click(e);
  assert.deepEqual(await input.evaluate(el=>[el.selectionStart,el.selectionEnd]),[4,4]);
  await page.keyboard.insertText("!");
  const changed = md.replace("Écho","Éc!ho");
  assert.equal(await content(),changed);
  await page.keyboard.press("ArrowLeft");
  const absolute = changed.indexOf("Éc!ho") + 2;
  await page.getByRole("button",{ name:"Source",exact:true }).click();
  assert.deepEqual(await selection(),[absolute,absolute]);
  await page.getByRole("button",{ name:"Live edit",exact:true }).click();

  assert.deepEqual(await input.evaluate(el=>[el.selectionStart,el.selectionEnd]),[4,4]);
  await page.getByRole("cell",{name:"Noon",exact:true}).click();
  await grid.scrollIntoViewIfNeeded();
  const first = await glyph(".md-grid td strong",1);
  const last = await glyph(".md-grid tr:last-child td:last-child .grid-value",2);
  await page.mouse.move(first.x, first.y); await page.mouse.down();
  await page.mouse.move(last.x,last.y,{steps:12}); await page.mouse.up();
  const b = changed.indexOf("Noon")+2;
  assert.equal(await grid.locator(".range-cell").count(),2,"dragging across two cells selects their rectangle");
  await page.keyboard.press("Escape");
  const prose = await glyph(".cm-line:text-is(\"Paragraph\")",3);
  await page.mouse.move(prose.x,prose.y); await page.mouse.down();
  await page.mouse.move(last.x,last.y,{steps:12}); await page.mouse.up();
  assert.deepEqual(await selection(),[changed.indexOf("Paragraph")+3,b]);
  await page.getByRole("cell",{name:"Noon",exact:true}).click();
  await input.press("Home");
  await page.keyboard.press("Shift+ArrowLeft");
  const left = changed.indexOf(" gate") + " gate".length;
  assert.deepEqual(await selection(),[changed.indexOf("Noon"),left]);
  await page.keyboard.press("Shift+ArrowLeft");
  assert.deepEqual(await selection(),[changed.indexOf("Noon"),left-1]);
  await page.getByRole("cell",{name:"Noon",exact:true}).dblclick();
  assert.deepEqual(await input.evaluate(el=>[el.selectionStart,el.selectionEnd]),[0,4]);
  await page.keyboard.press("ControlOrMeta+a");
  assert.deepEqual(await selection(),[0,changed.length]);
  await page.keyboard.insertText("A new note");
  assert.equal(await content(),"A new note");
  await page.keyboard.press("ControlOrMeta+z");
  assert.equal(await content(),changed);
  const tableOnly = "| A | B |\n| --- | --- |\n| one | two |";
  await page.evaluate(md => { view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: md }, selection: { anchor: 0 } }); view.focus(); },tableOnly);
  await page.getByRole("cell",{name:"two",exact:true}).click();
  await page.keyboard.press("Escape");
  assert.equal(await content(),tableOnly);
  await page.keyboard.insertText("After");
  assert.equal(await content(),tableOnly+"\n\nAfter");
  await page.keyboard.press("ControlOrMeta+z");
  assert.equal(await content(),tableOnly);
  const checklist = "# Walk\n\n- Parent\n  - [x] Child";
  await page.evaluate(md => { view.dispatch({ changes:{from:0,to:view.state.doc.length,insert:md},selection:{anchor:md.indexOf("Child")} }); view.focus(); },checklist);
  await page.keyboard.press("Backspace"); await page.keyboard.press("Backspace");
  assert.equal(await content(),"# Walk\n\n- Parent\nChild");

  const shared = "# Team\n\n| Name | Time |\n| --- | --- |\n| Al | Noon |\n\nEnd";
  await page.evaluate(md => { view.dispatch({ changes:{from:0,to:view.state.doc.length,insert:md},selection:{anchor:2} }); view.focus(); },shared);
  const peer = await page.context().newPage();
  await peer.goto(await page.locator("#open-peer").getAttribute("href"));
  await peer.waitForFunction(md => document.querySelector(".cm-content")?.cmTile.root.view.state.doc.toString() === md,shared);
  const noon = await glyph(".md-grid tr:last-child td:last-child .grid-value",2);
  await click(noon);
  await peer.locator(".md-h1").click(); await peer.keyboard.press("Home"); await peer.keyboard.insertText("Our ");
  await page.waitForFunction(() => view.state.doc.toString().startsWith("# Our Team"));
  assert.deepEqual(await input.evaluate(i=>[i.selectionStart,i.selectionEnd]),[2,2]);
  await page.keyboard.insertText("!");
  const converged = shared.replace("Team","Our Team").replace("Noon","No!on");
  await peer.waitForFunction(md=>document.querySelector(".cm-content").cmTile.root.view.state.doc.toString()===md,converged);
  assert.equal(await content(),converged);
  await page.keyboard.press("ControlOrMeta+z");
  const remoteOnly = shared.replace("Team","Our Team");
  await peer.waitForFunction(md=>document.querySelector(".cm-content").cmTile.root.view.state.doc.toString()===md,remoteOnly);
  assert.equal(await content(),remoteOnly);
  await peer.close();
  await exerciseSplit(page,context,"http://127.0.0.1:"+server.address().port);
  await exerciseReturnAndColumns(page);
  await exerciseSelectionTools(page);
  assert.deepEqual(errors,[]);
  console.log("Passed browser gestures, Markdown selection, tables, lists and two-editor typing and undo.");
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
