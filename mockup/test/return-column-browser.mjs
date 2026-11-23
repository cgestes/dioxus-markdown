import assert from "node:assert/strict";

export async function exerciseReturnAndColumns(page) {
  await page.setViewportSize({width:1000,height:700});
  await page.goto(new URL("/?room=return-and-columns#walk",page.url()).href);
  await page.waitForSelector(".cm-content");
  await page.evaluate(async()=>{await document.fonts.ready;window.view=document.querySelector(".cm-content").cmTile.root.view;});
  const load=(md,pos=0)=>page.evaluate(([s,anchor])=>{
    view.dispatch({changes:{from:0,to:view.state.doc.length,insert:s},selection:{anchor},scrollIntoView:true});view.focus();
  },[md,pos]);
  const content=()=>page.evaluate(()=>view.state.doc.toString());
  // Test the actual painted cursor before any further key, click or pointer move.
  for (const mode of ["live","split","source"]) {
    await page.locator("#"+mode).click();
    for (const tail of ["","\nNext"]) {
      const start="# Title\n\n"+"Line of text.\n".repeat(25)+"**Café**";
      await load(start+tail,start.length);
      await page.keyboard.press("Enter");
      const at=start.length+1;
      assert.equal(await content(),start+"\n"+tail);
      await page.waitForFunction(pos=>{
        const c=document.querySelector(".cm-cursor-primary");
        if (!c || view.state.selection.main.head!==pos || document.activeElement!==view.contentDOM) return false;
        const cursor=c.getBoundingClientRect(), stop=view.coordsAtPos(pos);
        if (!stop || !cursor.height || !cursor.width || getComputedStyle(c).visibility!=="visible") return false;
        const editor=document.querySelector("#editor").getBoundingClientRect(),clip=document.querySelector("#scroller").getBoundingClientRect();
        return Math.abs(cursor.top-stop.top)<2 && cursor.top>=Math.max(0,clip.top,editor.top)-1 &&
          cursor.bottom<=Math.min(innerHeight,clip.bottom,editor.bottom)+1;
      },at,{timeout:4000});
      const height=await page.locator(".cm-cursor-primary").evaluate(e=>e.getBoundingClientRect().height);
      await page.keyboard.insertText("z");
      assert.equal(await content(),start+"\nz"+tail);
      await page.waitForFunction(h=>Math.abs(document.querySelector(".cm-cursor-primary").getBoundingClientRect().height-h)<2,height,{timeout:3000});
    }
  }
  const first="| Keep | Time |\n| --- | --- |\n| 1 | 2 |\n\n";
  const md=first+"| A | B | C |\n| --- | ---: | :---: |\n| a\\|b | 🌿 | third |\n| Only |";
  for (const split of [false,true]) {
    await page.locator(split?"#split":"#live").click();
    await load(md);
    const table=page.locator(split?"#reading table":".md-grid").nth(1);
    const shell=table.locator(split?"xpath=ancestor::div[contains(@class,'split-table')][1]":"xpath=ancestor::div[contains(@class,'md-grid-wrap')][1]");
    const column=shell.getByRole("button",{name:"Delete column",exact:true});
    assert.equal(await column.count(),0);
    await table.locator("tr").nth(1).locator("td").nth(1).click();
    const action=shell.getByRole("button",{name:"Table actions",exact:true});
    const minus=await (split ? column : action).boundingBox();
    await page.mouse.click(minus.x-1,minus.y+minus.height/2);
    assert.equal(await content(),md,"click beside the minus preserves the table");
    await table.locator("tr").nth(1).locator("td").nth(1).click();
    if (!split) await action.click();
    await column.click();
    const expected=first+"| A | C |\n| --- | :---: |\n| a\\|b | third |\n| Only |";
    assert.equal(await content(),expected);
    if (split) assert.equal(await page.evaluate(()=>{const s=view.state.selection.main;return view.state.sliceDoc(s.from,s.to);}),"third");
    else assert.equal(await page.locator(".grid-cell-input:focus").inputValue(),"third");
    if (!split) await shell.getByRole("button",{name:"Table actions",exact:true}).click();
    await column.click();
    assert.equal(await table.locator("th").count(),1,(split?"Split":"Live")+" after removing the last selected column: "+await content());
    if (split) assert.equal(await column.count(),0);
    else {
      await shell.getByRole("button",{name:"Table actions",exact:true}).click();
      assert.ok(await column.isDisabled());
      await page.keyboard.press("Escape");
      await table.locator("tr").nth(1).locator("td").first().click();
    }
    await page.keyboard.press("ControlOrMeta+z");assert.equal(await content(),expected);
    await page.keyboard.press("ControlOrMeta+z");assert.equal(await content(),md);
  }
  // Undoing the local deletion must retain a peer's edit to a surviving column.
  const borderless="Place | Time\n:--- | ---:\nQuai | 40";
  await load(borderless);
  const peer=await page.context().newPage();
  try {
    const url=new URL(page.url());url.searchParams.set("editor","b");
    await peer.goto(url.href);
    const peerSees=s=>peer.waitForFunction(md=>document.querySelector(".cm-content")?.cmTile?.root.view.state.doc.toString()===md,s);
    await peerSees(borderless);
    await page.locator("#reading table tr").last().locator("td").nth(1).click();
    await page.getByRole("button",{name:"Delete column",exact:true}).click();
    const reduced="| Place |\n| :--- |\n| Quai |";
    await peerSees(reduced);
    await peer.evaluate(()=>{
      const v=document.querySelector(".cm-content").cmTile.root.view;
      v.dispatch({changes:{from:v.state.doc.toString().indexOf("Quai")+4,insert:" vert"},userEvent:"input.type"});
    });
    await page.waitForFunction(md=>view.state.doc.toString()===md,reduced.replace("Quai","Quai vert"));
    await page.locator(".cm-content").focus();
    await page.keyboard.press("ControlOrMeta+z");
    await peerSees(borderless.replace("Quai","Quai vert"));
    assert.equal(await content(),borderless.replace("Quai","Quai vert"));
  } finally { await peer.close(); }
  console.log("Passed Return caret painting and scrolling in all three views, scoped column deletion and two-step undo in both table views.");
}
