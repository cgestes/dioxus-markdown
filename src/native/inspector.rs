//! Opt-in observation of the same Live fields and Loro note used for normal editing.
use super::{ByteSelection, Field, Writing};
use dioxus_native::prelude::*;
use serde::Serialize;
use std::collections::VecDeque;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const HISTORY_LIMIT: usize = 48;

#[derive(Clone, Default, Debug, PartialEq, Serialize)]
pub(super) struct State {
    pub focused: Option<String>,
    pub editor_field: Option<String>,
    /// Zero-based table index, fixed cell and moving cell. Each cell is [row, column].
    pub cell_selection: Option<(usize, [usize; 2], [usize; 2])>,
    pub local_bytes: Option<ByteSelection>,
    pub markdown_bytes: Option<ByteSelection>,
    /// One-based Markdown line and Unicode scalar column at the moving end.
    pub line_column: Option<[usize; 2]>,
    pub source_line: Option<String>,
    pub field_matches_source: Option<bool>,
    /// Logical viewport pixels, x/y followed by width/height.
    pub field_rect: Option<[f64; 4]>,
    pub caret_rect: Option<[f64; 4]>,
    pub viewport_px: [u32; 2],
    pub scale: f32,
}

pub(super) fn snapshot(writing: Writing) -> State {
    let mut state = State {
        editor_field: (*writing.active.peek()).map(Field::id),
        cell_selection: (*writing.cells.peek())
            .map(|(table, rect)| (table, rect.anchor, rect.head)),
        ..State::default()
    };
    let handle = writing.root.peek();
    let Some(handle) = handle.as_ref() else {
        return state;
    };
    let doc = handle.doc();
    let viewport = doc.viewport();
    state.viewport_px = [viewport.window_size.0, viewport.window_size.1];
    state.scale = viewport.scale();
    let Some(node) = doc.get_focussed_node_id().and_then(|id| doc.get_node(id)) else {
        return state;
    };
    let Some(element) = node.element_data() else {
        return state;
    };
    state.focused = element.id.as_deref().map(str::to_owned);
    let Some(field) = state.focused.as_deref().and_then(Field::from_input_id) else {
        return state;
    };
    let Some(input) = element.text_input_data() else {
        return state;
    };
    let selected = input.editor.raw_selection();
    if let Some(b) = doc.get_client_bounding_rect(node.id) {
        state.field_rect = Some([b.x, b.y, b.width, b.height]);
        if let Some(layout) = input.editor.try_layout() {
            let scale = f64::from(layout.scale());
            let c = selected.focus().geometry(layout, 1.5);
            let l = node.final_layout();
            let mut x = b.x + f64::from(l.padding.left + l.border.left) + c.x0 / scale;
            let mut y = b.y
                + f64::from(l.padding.top + l.border.top)
                + node.text_input_v_centering_offset(scale)
                + c.y0 / scale;
            if input.is_multiline {
                y -= f64::from(input.scroll_offset);
            } else {
                x -= f64::from(input.scroll_offset);
            }
            state.caret_rect = Some([x, y, c.width() / scale, c.height() / scale]);
        }
    }
    let local = [selected.anchor().index(), selected.focus().index()];
    state.local_bytes = Some(local);
    let md = writing.contents.read();
    let range = writing.range(field, &md);
    let Some(_raw) = md.get(range.clone()) else {
        return state;
    };
    let proj = writing.projection(field);
    let matches = proj.display == input.editor.raw_text();
    state.field_matches_source = Some(matches);
    let absolute = writing
        .span
        .peek()
        .unwrap_or_else(|| local.map(|offset| proj.display_to_source(offset)));
    state.markdown_bytes = Some(absolute);
    if let Some(before) = md.get(..absolute[1]) {
        let from = before.rfind('\n').map_or(0, |n| n + 1);
        let end = md[from..].find('\n').map_or(md.len(), |n| from + n);
        state.line_column = Some([
            before.bytes().filter(|b| *b == b'\n').count() + 1,
            before[from..].chars().count() + 1,
        ]);
        state.source_line = Some(md[from..end].trim_end_matches('\r').into());
    }
    state
}

#[derive(Clone, Serialize)]
struct Observation {
    number: usize,
    elapsed_ms: u128,
    inputs: Vec<String>,
    state: State,
}

pub(super) struct Trace {
    initial: String,
    started: std::time::Instant,
    frames: VecDeque<Observation>,
    dropped: usize,
    status: String,
}

impl Trace {
    pub(super) fn new(initial: &str) -> Self {
        Self {
            initial: initial.into(),
            started: std::time::Instant::now(),
            frames: VecDeque::new(),
            dropped: 0,
            status: String::new(),
        }
    }

    fn observe(&mut self, inputs: Vec<String>, state: State) {
        if self.frames.len() == HISTORY_LIMIT {
            self.frames.pop_front();
            self.dropped += 1;
        }
        let number = self.dropped + self.frames.len() + 1;
        self.frames.push_back(Observation {
            number,
            elapsed_ms: self.started.elapsed().as_millis(),
            inputs,
            state,
        });
    }

    pub(super) fn title_status(&self) -> String {
        let Some(frame) = self.frames.back() else {
            return "Inspector".into();
        };
        let field = frame.state.focused.as_deref().unwrap_or("no field");
        match frame.state.line_column {
            Some([line, col]) => format!("Inspector · {field} · {line}:{col}"),
            None => format!("Inspector · {field}"),
        }
    }

    fn report_bytes(&self, writing: Writing) -> io::Result<Vec<u8>> {
        let report = serde_json::json!({
            "format": 1, "editor": "Margin / Dioxus Native Live",
            "initial_markdown": self.initial, "current_markdown": writing.note.peek().markdown(),
            "current_state": snapshot(writing), "earlier_frames_omitted": self.dropped,
            "recent_frames": self.frames, "frames_observed": self.dropped + self.frames.len(),
        });
        Ok(serde_json::to_vec_pretty(&report)?)
    }
    pub(super) fn report(&self, writing: Writing, dir: &Path) -> io::Result<PathBuf> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let bytes = self.report_bytes(writing)?;
        if let Some(session) = writing.personal.peek().as_ref() {
            let blob = session
                .vault
                .attach(
                    &session.vault.inbox(),
                    "Editor report.json",
                    bytes.as_slice(),
                )
                .map_err(io::Error::other)?;
            return Ok(PathBuf::from(format!("encrypted inbox attachment {blob}")));
        }
        std::fs::create_dir_all(dir)?;
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let path = dir.join(format!("margin-{time}-{}.json", std::process::id()));
        let mut file = std::fs::File::create_new(&path)?;
        file.write_all(&bytes)?;
        Ok(path)
    }
}

pub(super) fn save(writing: Writing, mut trace: Signal<Trace>) {
    let dir = std::env::var_os("MARGIN_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("margin-reports"));
    let result = trace.peek().report(writing, &dir);
    let message = match result {
        Ok(path) => format!("Saved {}", path.display()),
        Err(error) => format!("Could not save report: {error}"),
    };
    eprintln!("{message}");
    trace.write().status = message;
}

#[component]
pub(super) fn Inspector(mut writing: Writing, trace: Signal<Trace>, desktop: bool) -> Element {
    let history = trace.read();
    let frame = history.frames.back();
    let state = frame
        .map(|f| f.state.clone())
        .unwrap_or_else(|| snapshot(writing));
    let field = state.focused.as_deref().unwrap_or("no field");
    let where_at = match (state.line_column, state.markdown_bytes) {
        (Some([line, col]), Some([a, b])) => format!("  Ln {line} Col {col}  bytes {a} → {b}"),
        _ => String::new(),
    };
    let last = frame
        .and_then(|f| f.inputs.last())
        .map(String::as_str)
        .unwrap_or("");
    let mismatch = if state.field_matches_source == Some(false) {
        "  FIELD ≠ MARKDOWN"
    } else {
        ""
    };
    let summary = format!("{field}{where_at}{mismatch}    {last}");
    let status = &history.status;
    rsx! {
        div { class: "inspect-strip",
        span { class: "inspect-label", "Inspector" }
        div { class: "inspect-status", id: "inspect-status", class: if !mismatch.is_empty() { "has-error" }, title: "{summary}  {status}",
            "{summary}"
            if !status.is_empty() { "   ·   " }
            span { id: "inspect-save-status", aria_live: "polite", "{status}" }
        }
        button { class: "inspect-save", id: "save-inspection", aria_label: "Save inspection report", tabindex: "-1",
            title: "Save Markdown, selection and recent inputs (F9)",
            onpointerdown: move |e| e.prevent_default(),
            onclick: move |_| {
                let prior = snapshot(writing);
                save(writing, trace);
                if let (Some(field), Some(local)) =
                    (prior.focused.as_deref().and_then(Field::from_input_id), prior.local_bytes) {
                    writing.focus.set(Some((field, local)));
                }
            }, "Save report" }
        }
        if desktop { WindowTrace { writing, trace } }
    }
}

#[component]
fn WindowTrace(writing: Writing, mut trace: Signal<Trace>) -> Element {
    use dioxus_native::winit::event::{Ime, WindowEvent};
    use dioxus_native::winit::keyboard::{Key as WindowKey, NamedKey as WindowNamed};
    let mut pending = vec!["Opened inspector".to_owned()];
    let mut modifiers = String::new();
    dioxus_native::use_window_event(move |event, _| {
        // AppKit can deliver input to an occluded window between redraws.
        // Flush the preceding input before F9 takes its observation.
        if let WindowEvent::KeyboardInput { event: key, .. } = event
            && key.state.is_pressed()
        {
            if !pending.is_empty() {
                trace
                    .write()
                    .observe(std::mem::take(&mut pending), snapshot(writing));
            }
            if matches!(
                key.logical_key,
                WindowKey::Named(WindowNamed::F8 | WindowNamed::F9)
            ) {
                return;
            }
        }
        let label = match event {
            WindowEvent::ModifiersChanged(m) => {
                let m = m.state();
                modifiers = [
                    (m.meta_key(), "Meta+"),
                    (m.control_key(), "Ctrl+"),
                    (m.alt_key(), "Alt+"),
                    (m.shift_key(), "Shift+"),
                ]
                .into_iter()
                .filter_map(|(on, name)| on.then_some(name))
                .collect();
                return;
            }
            WindowEvent::KeyboardInput { event: key, .. } if key.state.is_pressed() => format!(
                "{modifiers}{:?}{}",
                key.logical_key,
                if key.repeat { " (repeat)" } else { "" }
            ),
            WindowEvent::Ime(Ime::Commit(s)) => format!("IME commit {s:?}"),
            WindowEvent::Ime(Ime::Preedit(s, selection)) => {
                format!("IME preedit {s:?} {selection:?}")
            }
            WindowEvent::PointerButton {
                state,
                button,
                position,
                ..
            } => format!(
                "{modifiers}{button:?} {state:?} at {:.0},{:.0} px",
                position.x, position.y
            ),
            WindowEvent::MouseWheel { delta, .. } => format!("Scroll {delta:?}"),
            WindowEvent::SurfaceResized(size) => {
                format!("Resize {}×{} px", size.width, size.height)
            }
            WindowEvent::Focused(focused) => format!("Window focused {focused}"),
            WindowEvent::RedrawRequested if !pending.is_empty() => {
                let inputs = std::mem::take(&mut pending);
                trace.write().observe(inputs, snapshot(writing));
                return;
            }
            _ => return,
        };
        if pending.len() < HISTORY_LIMIT {
            pending.push(label);
        }
    });
    rsx! {}
}
