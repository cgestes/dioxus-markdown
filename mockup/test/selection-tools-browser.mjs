import assert from "node:assert/strict";

export async function exerciseSelectionTools(page) {
  await page.goto(new URL("/?room=selection-tools#walk",page.url()).href);
  await page.waitForSelector(".md-grid");
  const md = "# Field notes\n\nCafé by the lock\n\n| A | B | C | D |\n| --- | :--- | ---: | --- |\n| one | Café | 🌿 | keep |\n| two | a\\|b | c | last |\n\nAfter";
  await page.evaluate(async md=>{
    await document.fonts.ready;
    window.view=document.querySelector(".cm-content").cmTile.root.view;
    view.dispatch({changes:{from:0,to:view.state.doc.length,insert:md},selection:{anchor:2}});view.focus();
  },md);
  const content=()=>page.evaluate(()=>view.state.doc.toString());
  const cell=(r,c)=>page.locator(".md-grid tr").nth(r).locator("th,td").nth(c);
  const active=()=>page.locator(".grid-cell-input:focus").evaluate(i=>{const cell=i.closest("th,td");return [cell.parentElement.rowIndex,cell.cellIndex];});
  const count=()=>page.locator(".md-grid .range-cell").count();
  const cellInk=()=>page.locator(".grid-cell-input:focus").evaluate(input=>({
    input:getComputedStyle(input).opacity,
    label:getComputedStyle(input.parentElement.querySelector(".grid-value")).visibility
  }));
  const table=page.locator(".md-grid");
  const checkMenuPlacement=async()=>{
    const menu=page.getByRole("group",{name:"Table actions",exact:true});
    const box=await menu.boundingBox(), grid=await page.locator(".grid-scroll").boundingBox();
    const clip=await page.locator("main").boundingBox();
    assert.ok(box && (box.y>=grid.y+grid.height-1 || box.y+box.height<=grid.y+1),"actions leave the cells visible");
    assert.ok(box.x>=clip.x && box.y>=clip.y && box.x+box.width<=clip.x+clip.width+1 && box.y+box.height<=clip.y+clip.height+1,"all actions stay on screen");
  };
  const before=await table.boundingBox();
  await cell(2,2).click();
  await cell(1,1).click({modifiers:["Shift"]});
  assert.equal(await count(),4);
  assert.deepEqual(await cellInk(),{input:"0",label:"visible"});
  assert.equal(await page.locator(".cell-count:visible").innerText(),"4 cells");
  const after=await table.boundingBox();assert.deepEqual(after,before);
  await page.keyboard.press("Shift+F10");
  await checkMenuPlacement();
  assert.equal(await page.evaluate(()=>document.activeElement.textContent),"Clear selected cells");
  assert.deepEqual(await table.boundingBox(),before);
  await page.keyboard.press("Escape");
  assert.deepEqual(await active(),[1,1]);assert.equal(await count(),4);
  const format=page.getByRole("toolbar",{name:"Format selection"});
  assert.equal(await format.count(),0);
  await page.keyboard.press("Shift+Tab");assert.deepEqual(await active(),[2,2]);
  await page.keyboard.press("Tab");assert.deepEqual(await active(),[1,1]);
  await page.keyboard.press("Tab");assert.deepEqual(await active(),[1,2]);
  assert.equal(await content(),md);
  await page.context().grantPermissions(["clipboard-read","clipboard-write"]);
  await page.keyboard.press("ControlOrMeta+c");
  assert.equal(await page.evaluate(()=>navigator.clipboard.readText()),"Café\t🌿\na|b\tc");
  await page.evaluate(()=>navigator.clipboard.writeText("m|n\tζ\tEXCESS\nø\t42\tEXCESS\nEXCESS"));
  await page.keyboard.press("ControlOrMeta+v");
  assert.equal(await content(),md.replace("| Café | 🌿 |","| m\\|n | ζ |").replace("| a\\|b | c |","| ø | 42 |"));
  await page.keyboard.press("ControlOrMeta+z");
  assert.equal(await content(),md);
  await cell(1,1).click();
  await page.keyboard.press("Alt+Shift+ArrowRight");await page.keyboard.press("Shift+ArrowDown");
  assert.equal(await count(),4);
  await page.keyboard.press("Delete");
  assert.equal(await content(),md.replace("| Café | 🌿 |","|  |  |").replace("| a\\|b | c |","|  |  |"));
  await page.keyboard.press("ControlOrMeta+z");
  assert.equal(await content(),md);
  await cell(1,1).click();await cell(2,2).click({modifiers:["Shift"]});
  await page.getByRole("button",{name:"Table actions",exact:true}).click();
  await page.getByRole("button",{name:"Delete selected columns",exact:true}).click();
  assert.equal(await content(),md.replace("| A | B | C | D |","| A | D |").replace("| --- | :--- | ---: | --- |","| --- | --- |").replace(" Café | 🌿 |","").replace(" a\\|b | c |",""));
  await page.keyboard.press("ControlOrMeta+z");assert.equal(await content(),md);
  await cell(1,1).click();await cell(2,2).click({modifiers:["Shift"]});
  await page.keyboard.press("Tab");await page.keyboard.insertText("lake");
  assert.deepEqual(await cellInk(),{input:"1",label:"hidden"});
  await page.keyboard.press("Backspace");await page.keyboard.press("Tab");
  assert.equal(await content(),md.replace("| Café |","| lak |"));assert.deepEqual(await active(),[1,2]);
  await page.keyboard.press("Escape");assert.equal(await count(),0);
  const prose = page.locator(".cm-line").filter({hasText:"by the lock"});
  await prose.dblclick({position:{x:12,y:12}});
  await format.waitFor();
  const linesBefore=await prose.boundingBox();
  await format.getByRole("button",{name:"Bold",exact:true}).click();
  assert.ok((await content()).includes("**Café** by the lock"));
  await format.getByRole("button",{name:"Bold",exact:true,pressed:true}).waitFor();
  const linesAfter=await prose.boundingBox();
  assert.equal(linesAfter.y,linesBefore.y);
  await page.keyboard.press("ControlOrMeta+b");assert.ok((await content()).includes("Café by the lock"));
  await cell(1,3).dblclick();
  await format.getByRole("button",{name:"Italic",exact:true}).click();
  assert.ok((await content()).includes("| *keep* |"));
  assert.deepEqual(await active(),[1,3]);
  await page.keyboard.press("ControlOrMeta+i");assert.ok((await content()).includes("| keep |"));
  await page.keyboard.press("ArrowRight");await format.waitFor({state:"hidden"});
  await page.setViewportSize({width:405,height:740});
  await page.locator(".cm-line.md-h1").dblclick({position:{x:25,y:15}});
  await format.waitFor();const box=await format.boundingBox();assert.ok(box.x>=0 && box.x+box.width<=405);
  await cell(1,0).click();await cell(2,1).click({modifiers:["Shift"]});
  await page.getByRole("button",{name:"Table actions",exact:true}).click();
  await checkMenuPlacement();
  await page.keyboard.press("Escape");
  assert.deepEqual(await active(),[2,1]);assert.equal(await count(),4);
  await checkWrappedCell(page);
  console.log("Passed cell rectangles, Tab wrapping, clipboard, range deletion, selected-cell typing, floating formatting and desktop and narrow menu placement.");
}

async function checkWrappedCell(page) {
  const md="# Walk\n\n| Stop | Listen for | Take |\n| --- | --- | ---: |\n| Iron footbridge | Shoes on the hollow steel deck | 45 s |\n| Lock | Water | 60 s |";
  await page.setViewportSize({width:390,height:770});
  await page.evaluate(md=>{
    view.dispatch({changes:{from:0,to:view.state.doc.length,insert:md},selection:{anchor:2}});
    view.focus();
  },md);
  await page.waitForFunction(()=>{
    const [label,description,time]=document.querySelectorAll(".md-grid th");
    return description.offsetWidth>label.offsetWidth && label.offsetWidth>time.offsetWidth;
  });
  const label=page.locator(".md-grid tbody tr").first().locator(".grid-value").nth(1);
  const before=await label.boundingBox();
  const point=await label.evaluate(label=>{
    const text=label.firstChild, offset=text.data.indexOf("steel");
    const range=document.createRange();range.setStart(text,offset);range.collapse(true);
    const b=range.getBoundingClientRect();
    return {x:b.x+0.2,y:b.y+b.height/2,offset};
  });
  assert.ok(point.y>before.y+20,"the clicked word is on a wrapped line");
  await page.mouse.click(point.x,point.y);
  const input=page.locator(".grid-cell-input:focus");
  assert.deepEqual(await input.boundingBox(),before,"the editor occupies the typeset lines exactly");
  assert.deepEqual(await input.evaluate(i=>[i.selectionStart,i.selectionEnd]),[point.offset,point.offset]);
  assert.ok(await input.evaluate(i=>i.scrollWidth===i.clientWidth && i.scrollHeight===i.clientHeight));
  await page.keyboard.insertText("!");
  assert.equal(await page.evaluate(()=>view.state.doc.toString()),md.replace("steel","!steel"));
  await input.evaluate(i=>i.dispatchEvent(new InputEvent("beforeinput",{bubbles:true,cancelable:true,inputType:"insertLineBreak"})));
  assert.equal(await page.locator(".grid-cell-input:focus").inputValue(),"Water");
  assert.equal(await page.evaluate(()=>view.state.doc.toString()),md.replace("steel","!steel"));
}
