import assert from "node:assert/strict";

export async function exerciseSplit(page, context, origin) {
  const url=origin+"/?view=split&room=split-interactions#walk";
  await page.goto(url);
  await page.waitForSelector("#reading .reading-cell");
  await page.evaluate(async()=>{await document.fonts.ready; window.view=document.querySelector(".cm-content").cmTile.root.view;});
  const input=page.locator(".cm-content");
  const heading=page.locator("#reading h2");
  const md="# Étape\n\n## Deux\n\n**Départ** au quai.\n\n| A | B | C |\n| --- | --- | ---: |\n| 🌿 | **Écho** | a\\|b |\n| Court |\n\n- [ ] Tâche\n\nFin";
  const load=markdown=>page.evaluate(s=>{view.dispatch({changes:{from:0,to:view.state.doc.length,insert:s},selection:{anchor:0}}); view.focus();},markdown);
  const selection=()=>page.evaluate(()=>[view.state.selection.main.anchor,view.state.selection.main.head]);
  const place=(anchor,head=anchor)=>page.evaluate(([a,b])=>{view.dispatch({selection:{anchor:a,head:b},scrollIntoView:true});view.focus();},[anchor,head]);
  const text=()=>page.evaluate(()=>view.state.doc.toString());
  const caretPoint=pos=>page.evaluate(i=>{const r=view.coordsAtPos(i);return {x:r.left+.1,y:(r.top+r.bottom)/2};},pos);
  const glyphPoint=(locator,offset)=>locator.evaluate((el,n)=>{const range=document.createRange();range.setStart(el.firstChild,n);range.collapse(true);const b=range.getBoundingClientRect();return {x:b.left+.1,y:(b.top+b.bottom)/2};},offset);
  const click=p=>page.mouse.click(p.x,p.y);
  await load(md);
  assert.equal(await heading.textContent(),"Deux");
  assert.equal(await page.locator(".cm-content").count(),1);
  assert.equal(await page.locator("#reading input,#reading textarea,#reading [contenteditable=true],.grid-cell-input").count(),0);
  assert.equal(await page.locator("#reading .strong").count(),2);
  await click(await glyphPoint(heading.locator("span").first(),2));
  assert.deepEqual(await selection(),[md.indexOf("Deux")+2,md.indexOf("Deux")+2]);
  const echo=page.locator("#reading .reading-cell .strong");
  await click(await glyphPoint(echo,2));
  assert.deepEqual(await selection(),[md.indexOf("Écho")+2,md.indexOf("Écho")+2]);
  await page.keyboard.insertText("!");
  assert.equal(await echo.textContent(),"Éc!ho");
  await page.keyboard.press("ControlOrMeta+z");
  assert.equal(await text(),md);

  await place(0);
  await page.keyboard.press("ArrowDown");assert.deepEqual(await selection(),[8,8]);
  await page.keyboard.press("ArrowDown");assert.deepEqual(await selection(),[9,9]);
  await page.keyboard.press("ArrowUp");assert.deepEqual(await selection(),[8,8]);
  const a=md.indexOf("Deux")+1,b=md.indexOf("Écho")+2;
  const start=await caretPoint(a),end=await caretPoint(b);
  await page.mouse.move(start.x,start.y);await page.mouse.down();await page.mouse.move(end.x,end.y,{steps:12});await page.mouse.up();
  assert.deepEqual(await selection(),[a,b]);
  await place(b,a);
  await page.locator("#source").click();await page.locator("#split").click();
  assert.deepEqual(await selection(),[b,a]);
  await page.keyboard.insertText("Remplacé");
  assert.equal(await text(),md.slice(0,a)+"Remplacé"+md.slice(b));
  await page.keyboard.press("ControlOrMeta+z");assert.equal(await text(),md);

  const table=page.locator("#reading table").first();
  const rowCol=table.locator("tr").nth(1).locator(".reading-cell").last();
  assert.equal(await rowCol.innerText(),"a|b");
  const read=await table.boundingBox();
  const firstHeight=(await table.locator("tr").first().boundingBox()).height;
  await page.mouse.click(read.x+read.width+5,read.y+firstHeight/2);
  assert.deepEqual(await selection(),[md.indexOf(" C |")+2,md.indexOf(" C |")+2]);
  for (const [label,side] of [["Add column","right"],["Add row","bottom"]]) {
    const rect=await page.getByRole("button",{name:label,exact:true}).boundingBox();
    const points=side==="right"?[[rect.x+rect.width/2,rect.y-1],[rect.x+rect.width/2,rect.y+rect.height+1]]:[[rect.x-1,rect.y+rect.height/2],[rect.x+rect.width+1,rect.y+rect.height/2]];
    for(const [x,y] of points){await page.mouse.click(x,y);assert.equal(await input.evaluate(n=>n===document.activeElement),true);assert.equal(await text(),md);}
  }
  await table.locator("tr").last().locator("td").last().click();
  assert.equal(await text(),md);
  await page.keyboard.insertText("nuit");
  assert.match(await table.locator("tr").last().locator("td").last().textContent(),/^nuit$/);
  await page.keyboard.press("Tab");
  assert.equal(await table.locator("tr").count(),4);
  await page.getByRole("button",{name:"Add column",exact:true}).click();
  assert.equal(await table.locator("th").count(),4);
  const [lo,hi]=await selection();
  assert.equal((await text()).slice(lo,hi),"Column 4");
  await page.keyboard.insertText("Durée");
  assert.equal(await table.locator("th").last().innerText(),"Durée");
  await page.getByRole("button",{name:"Add row",exact:true}).click();
  assert.equal(await table.locator("tr").count(),5);
  await page.keyboard.press("ControlOrMeta+z");
  assert.equal(await table.locator("tr").count(),4);

  await load(md);
  const task=md.indexOf("[ ]")+1;
  await place(task);await page.keyboard.press("x");await page.keyboard.insertText("q");
  assert.ok((await text()).includes("[x]"));
  await page.keyboard.press("F2");await page.keyboard.insertText("q");
  assert.ok((await text()).includes("[qx]"));
  await load(md);await place(md.indexOf("Tâche")+5);await page.keyboard.press("Enter");
  assert.ok((await text()).includes("Tâche\n- [ ] "));
  await load(md);await place(md.indexOf("Écho")+2);
  await input.evaluate(n=>{const data=new DataTransfer();data.setData("text/plain","un\tdeux\ntrois\tquatre");n.dispatchEvent(new ClipboardEvent("paste",{clipboardData:data,bubbles:true,cancelable:true}));});
  assert.equal(await table.locator("tr").nth(2).locator("td").nth(2).textContent(),"quatre");
  await page.keyboard.press("ControlOrMeta+z");assert.equal(await text(),md);

  const passage=md.indexOf("Départ");
  await place(passage,passage+6);
  await page.locator("#ask").click();
  const agent=page.locator(".agent-panel");
  await agent.locator("input").fill("Replace with: Retour");
  await agent.locator("input").press("Enter");
  assert.equal(await text(),md);
  await agent.getByRole("button",{name:"Apply",exact:false}).click();
  assert.equal(await page.locator("#reading .strong").first().textContent(),"Retour");
  await page.keyboard.press("ControlOrMeta+z");
  assert.equal(await text(),md);

  // Edits delivered by the other Loro writer also move reading-pane targets.
  const peer=await context.newPage();
  try {
    await peer.goto(url.replace("room=split-interactions","editor=b&room=split-interactions"));
    await peer.waitForFunction(s=>document.querySelector(".cm-content")?.cmTile?.root.view.state.doc.toString()===s,md);
    await peer.evaluate(()=>{const v=document.querySelector(".cm-content").cmTile.root.view;v.dispatch({changes:{from:2,insert:"Notre "},userEvent:"input.type"});});
    await page.waitForFunction(()=>document.querySelector("#reading h1")?.textContent==="Notre Étape");
    await click(await glyphPoint(echo,2));
    assert.deepEqual(await selection(),[md.indexOf("Écho")+8,md.indexOf("Écho")+8]);
  } finally {await peer.close();}
  await page.setViewportSize({width:390,height:844});
  await page.waitForFunction(()=>document.querySelector("#reading").getBoundingClientRect().top>=document.querySelector("#editor").getBoundingClientRect().bottom);
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
  console.log("Passed Split source drags, directional selections, blank lines, glyph and rail clicks, table growth, list and checkbox keys, undo, peer updates and narrow layout.");
}
