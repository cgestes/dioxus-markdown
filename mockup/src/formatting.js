import { ViewPlugin } from "@codemirror/view";
import { gridSession, setGrid } from "./grid.js";
import { mode } from "./live-markdown.js";
import { undoGroup } from "./inline-agent.js";
import { button, el, icon } from "./ui.js";

const actions = [
  ["Bold", "bold", "B"], ["Italic", "italic", "I"],
  ["Strikethrough", "strikethrough", "Shift+X"], ["Inline code", "code-xml", "E"]
];

// Single-line inputs cannot provide DOM Ranges. Use the input's own font, scroll
// and alignment to anchor to the selected letters inside a wide or clipped cell.
function cellBounds(input) {
  const box = input.getBoundingClientRect(), css = getComputedStyle(input);
  const ctx = document.createElement("canvas").getContext("2d");
  ctx.font = css.font || `${css.fontWeight} ${css.fontSize} ${css.fontFamily}`;
  const width = ctx.measureText(input.value).width;
  const pad = parseFloat(css.paddingLeft), room = box.width - pad - parseFloat(css.paddingRight);
  const offset = Math.max(0,room-width) * (css.textAlign === "right" ? 1 : css.textAlign === "center" ? .5 : 0);
  const x = box.left + pad + offset - input.scrollLeft;
  const mid = x + (ctx.measureText(input.value.slice(0,input.selectionStart)).width + ctx.measureText(input.value.slice(0,input.selectionEnd)).width)/2;
  return { x: Math.min(box.right,Math.max(box.left,mid)), top:box.top, bottom:box.bottom };
}

export function floatingFormatting(format) {
  return ViewPlugin.fromClass(class {
    constructor(view) {
      this.view = view;
      this.bar = el("div","format-bar");
      this.bar.hidden = true;
      this.bar.setAttribute("role","toolbar"); this.bar.setAttribute("aria-label","Format selection");
      const mod = /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl+";
      actions.forEach(([label,glyph,chord],i) => {
        const control = button("","",() => this.apply(i));
        control.title = `${label} (${mod}${chord})`; control.setAttribute("aria-label",label);
        control.append(icon(glyph)); this.bar.append(control);
      });
      document.body.append(this.bar);
      this.events = new AbortController(); const signal = this.events.signal;
      this.bar.addEventListener("mousedown",e => e.preventDefault(),{signal});
      this.bar.addEventListener("keydown", e => {
        if (e.key === "Escape") { e.preventDefault(); this.restore(); this.dismissed = this.key; this.bar.hidden = true; }
        if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
          e.preventDefault(); const controls = [...this.bar.children];
          controls[(controls.indexOf(e.target)+(e.key === "ArrowRight" ? 1 : 3))%4].focus();
        }
      },{signal});
      view.dom.addEventListener("keydown",e => {
        if (e.isComposing || view.state.field(mode) !== "live") return;
        if (e.altKey && e.key === "F10" && !this.bar.hidden) { e.preventDefault(); this.bar.firstChild.focus(); return; }
        if (e.key === "Escape" && !this.bar.hidden && !view.state.field(gridSession)) {
          this.dismissed = this.key; this.bar.hidden = true; return;
        }
        if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
        const chord = (e.shiftKey ? "Shift+" : "")+e.key.toUpperCase();
        const idx = actions.findIndex(([, ,key]) => key === chord);
        if (idx >= 0 && this.apply(idx)) { e.preventDefault(); e.stopPropagation(); }
      },{capture:true,signal});
      view.dom.addEventListener("pointerdown",()=>{ this.dragging=true; this.bar.hidden=true; this.dismissed=null; },{capture:true,signal});
      for (const name of ["pointerup","pointercancel"]) window.addEventListener(name,()=>{this.dragging=false; this.schedule();},{capture:true,signal});
      for (const name of ["scroll","resize","focusin","focusout","selectionchange","select"]) window.addEventListener(name,()=>this.schedule(),{capture:true,passive:true,signal});
      this.schedule();
    }
    update() { this.schedule(); }
    schedule() { if (!this.frame) this.frame = requestAnimationFrame(()=>{this.frame=0; this.paint();}); }
    target(explicit = false) {
      const {view} = this, state = view.state;
      if (state.field(mode) !== "live" || view.composing) return null;
      const session = state.field(gridSession), input = view.dom.querySelector(".grid-cell-input");
      const active = document.activeElement;
      if (active !== view.contentDOM && (!input || active !== input) && !this.bar.contains(active)) return null;
      if (session?.rect || (session && (!input || input.compositionBase != null || !explicit && !session.textSelected))) return null;
      let {anchor,head} = state.selection.main;
      if (session) {
        const back = input.selectionDirection === "backward";
        anchor = session.from + (back ? input.selectionEnd : input.selectionStart);
        head = session.from + (back ? input.selectionStart : input.selectionEnd);
      }
      return anchor === head ? null : {anchor,head,session,input};
    }
    restore() { (this.view.dom.querySelector(".grid-cell-input") || this.view.contentDOM).focus({preventScroll:true}); }
    apply(idx) {
      const target = this.target(true); if (!target) return false;
      const {view} = this, {anchor,head,session} = target;
      const plan = format(view.state.doc,anchor,head,idx); if (!plan) return false;
      const changes = view.state.changes(plan.changes);
      const effects = session ? setGrid.of({...session, from:changes.mapPos(session.from,-1), to:changes.mapPos(session.to,1)}) : [];
      const manager = view.state.facet(undoGroup);
      manager.stopCapturing();
      view.dispatch({changes,selection:{anchor:plan.selection[0],head:plan.selection[1]},effects,userEvent:"input.format"});
      manager.stopCapturing();
      this.restore();
      if (session) {
        const base = view.state.field(gridSession).from;
        const [a,b] = plan.selection.map(n=>n-base);
        view.dom.querySelector(".grid-cell-input").setSelectionRange(Math.min(a,b),Math.max(a,b),a>b?"backward":"forward");
      }
      this.schedule(); return true;
    }
    paint() {
      const t = this.target();
      if (this.dragging || !t) { this.bar.hidden=true; this.key=null; this.dismissed=null; return; }
      const {view} = this, {anchor,head,input,session} = t;
      const key = `${anchor}/${head}`;
      if (key !== this.key || this.doc !== view.state.doc) {
        this.status = format(view.state.doc,anchor,head);
        this.key=key; this.doc=view.state.doc; this.dismissed=null;
      }
      if (!this.status || this.dismissed===key) {this.bar.hidden=true; return;}
      let bounds;
      if (session) {
        bounds=cellBounds(input);
        const grid=input.closest(".grid-frame").getBoundingClientRect();
        // Outside the grid, the palette leaves every neighboring cell clickable.
        if (grid.top >= 46) bounds.top=grid.top;
        else if (grid.bottom+46 <= innerHeight) { bounds.top=0; bounds.bottom=grid.bottom; }
      }
      else {
        const a = view.coordsAtPos(anchor), b = view.coordsAtPos(head);
        if (!a || !b) {this.bar.hidden=true; return;}
        bounds = { x: Math.abs(a.top-b.top)<2 ? (a.left+b.left)/2 : b.left, top:b.top, bottom:b.bottom };
      }
      const clip = view.dom.closest("#scroller")?.getBoundingClientRect();
      const top = Math.max(0,clip?.top ?? 0), bottom = Math.min(innerHeight,clip?.bottom ?? innerHeight);
      if (bounds.bottom < top || bounds.top > bottom) { this.bar.hidden=true; return; }
      [...this.bar.children].forEach((b,i)=>b.setAttribute("aria-pressed",String(this.status.active[i])));
      this.bar.hidden=false;
      const w=this.bar.offsetWidth,h=this.bar.offsetHeight;
      this.bar.style.left=Math.round(Math.max(6,Math.min(innerWidth-w-6,bounds.x-w/2)))+"px";
      this.bar.style.top=Math.round(bounds.top-h-7>=top ? bounds.top-h-7 : Math.min(innerHeight-h-6,bounds.bottom+7))+"px";
    }
    destroy() { this.events.abort(); cancelAnimationFrame(this.frame); this.bar.remove(); }
  });
}
