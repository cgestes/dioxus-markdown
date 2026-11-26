//! Opt-in observation of the same Live fields and Loro note used for normal editing.
use super::{ByteSelection, Field, Writing};
use crate::prelude::*;
use serde::Serialize;
use std::collections::VecDeque;
use std::io;

const HISTORY_LIMIT: usize = 48;

#[derive(Clone, Default, Debug, PartialEq, Serialize)]
pub(crate) struct State {
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

pub(crate) fn snapshot(writing: Writing) -> State {
    let mut state = State {
        editor_field: (*writing.active.peek()).map(Field::id),
        cell_selection: (*writing.grid.cells.peek())
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
    started: web_time::Instant,
    frames: VecDeque<Observation>,
    dropped: usize,
    status: String,
}

impl Trace {
    pub(super) fn new(initial: &str) -> Self {
        Self {
            initial: initial.into(),
            started: web_time::Instant::now(),
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
    /// Save the report and say where it went: into the vault with a personal
    /// session, otherwise wherever the platform keeps a file.
    fn save(&self, writing: Writing) -> io::Result<String> {
        let bytes = self.report_bytes(writing)?;
        if let Some(kept) = writing.store.peek().as_ref().and_then(|s| s.attach_report("Editor report.json", &bytes)) {
            return kept.map_err(io::Error::other);
        }
        crate::platform::save_report(&bytes)
    }
}

pub(super) fn save(writing: Writing, mut trace: Signal<Trace>) {
    let result = trace.peek().save(writing);
    let message = match result {
        Ok(place) => format!("Saved {place}"),
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
    let status_color = if !mismatch.is_empty() { "text-alert" } else { "" };
    rsx! {
        div { class: "absolute inset-x-0 top-0 h-7 flex items-center gap-3 px-7 max-[650px]:gap-[7px] max-[650px]:px-[14px] bg-wash border-0 border-b border-solid border-line text-accent-deep",
        span { class: "type-label", "Inspector" }
        div { class: "flex-1 min-w-0 overflow-hidden whitespace-nowrap type-mono-ui {status_color}", id: "inspect-status", title: "{summary}  {status}",
            "{summary}"
            if !status.is_empty() { "   ·   " }
            span { id: "inspect-save-status", aria_live: "polite", "{status}" }
        }
        crate::ui::Button { class: "!flex-none !h-6 !px-2 !text-accent-deep", id: "save-inspection", variant: crate::ui::Variant::Ghost, label: "Save inspection report",
            text: "Save report", tip: "Save Markdown, selection and recent inputs (F9)", keep_focus: true, focusable: false,
            onclick: move |_| {
                let prior = snapshot(writing);
                save(writing, trace);
                if let (Some(field), Some(local)) =
                    (prior.focused.as_deref().and_then(Field::from_input_id), prior.local_bytes) {
                    writing.focus.set(Some((field, local)));
                }
            }
        }
        }
        if desktop { WindowTrace { writing, trace } }
    }
}

/// Records what the user does while the inspector is open, with the state each input left.
#[component]
fn WindowTrace(writing: Writing, mut trace: Signal<Trace>) -> Element {
    crate::platform::use_input_trace(move |inputs| trace.write().observe(inputs, snapshot(writing)));
    rsx! {}
}
