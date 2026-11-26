//! Native: the editor reads Blitz's document and Parley's text layouts
//! directly. These are the real types under the names `platform/web.rs`
//! implements over the browser's DOM, so the notebook's code is the same on both.
use super::FieldEvent;
use crate::prelude::*;

pub use blitz_dom::{BaseDocument as Document, BoundingRect, ScrollBehavior};
pub use dioxus_native::{NodeHandle, NodeId};

/// A text field's laid-out text: lines, glyph clusters, carets and selections.
pub mod text {
    pub use parley::{Affinity, Cluster, Cursor, Selection};
    pub type Layout = parley::Layout<blitz_dom::node::TextBrush>;
}

/// The document an element was mounted into.
pub fn mounted(event: &MountedEvent) -> Option<NodeHandle> {
    event.downcast::<NodeHandle>().cloned()
}

/// Keep the focused input's internal scroll offset aligned with its caret,
/// ensuring a full 2px CSS margin on the right edge of single-line inputs
/// so the 1.5px caret is never clipped at 1× or 2×.
pub fn keep_caret_in_view(doc: &mut Document, id: NodeId) {
    let Some(node) = doc.get_node_mut(id) else {
        return;
    };
    let width = node.final_layout().content_box_width();
    let height = node.final_layout().content_box_height();
    let Some(input) = node
        .element_data_mut()
        .and_then(|e| e.text_input_data_mut())
    else {
        return;
    };
    input.clamp_scroll_offset(width, height);
    if !input.is_multiline
        && width > 0.0
        && let Some(layout) = input.editor.try_layout()
        && let Some(caret) = input.editor.cursor_geometry(1.5)
    {
        let scale = layout.scale();
        let caret_x = caret.x0 as f32 / scale;
        let caret_pad = 2.0_f32;
        let caret_end = caret_x + caret_pad;
        let content = layout.full_width() / scale;
        let max_offset = (content.max(caret_end) - width).max(0.0);
        let mut offset = input.scroll_offset;
        if caret_end > offset + width {
            offset = caret_end - width;
        }
        if caret_x < offset {
            offset = caret_x;
        }
        input.scroll_offset = offset.clamp(0.0, max_offset);
    }
}

/// Whether the field under a press already has its caret there. Blitz places
/// it as soon as a mouse or a finger presses.
pub fn press_places_caret(_press: &PointerData) -> bool {
    true
}

/// Whether moving this pressed pointer drags a selection. In Blitz every
/// pointer does.
pub fn move_selects(_pointer: &PointerData) -> bool {
    true
}

/// Whether a click follows this release. Blitz drops the click after a touch
/// whose press the notebook cancelled (the move it synthesises before a finger
/// press starts a pan).
pub fn click_follows_release(release: &PointerData) -> bool {
    release.pointer_type() != "touch"
}

/// Whether the window is as narrow as a phone's screen.
pub fn phone_sized() -> bool {
    cfg!(target_os = "ios")
}

/// Whether shortcuts use ⌘: an Apple keyboard.
pub fn apple_keys() -> bool {
    cfg!(target_vendor = "apple")
}

/// Open a link in the system browser.
pub fn open_url(url: &str) {
    let _ = webbrowser::open(url);
}

/// The app's window, for its title. Absent when a test hosts the document.
#[derive(Clone)]
pub struct Window(std::sync::Arc<dyn dioxus_native::winit::window::Window>);

impl Window {
    pub fn set_title(&self, title: &str) {
        self.0.set_title(title);
    }
}

pub fn use_window() -> Option<Window> {
    use_hook(|| try_consume_context::<std::sync::Arc<dyn dioxus_native::winit::window::Window>>().map(Window))
}

/// Tell the notebook when the focused field's caret or geometry moves, so its
/// overlays follow.
pub fn use_field_watch(root: Signal<Option<NodeHandle>>, mut on_event: impl FnMut(FieldEvent) + 'static) {
    use dioxus_native::winit::event::{Ime, WindowEvent};
    let mut resized = true;
    let mut wheeled = false;
    let mut previous = None;
    let mut prev_scroll = (0u64, 0u64, None::<(u64, u64)>);
    // Window hooks run before Blitz. On the following redraw its text editor
    // has the updated IME/selection geometry. Idle redraws need no note scan.
    dioxus_native::use_window_event(move |event, _| match event {
        WindowEvent::Ime(Ime::Preedit(text, _)) if !text.is_empty() => on_event(FieldEvent::Composing),
        WindowEvent::SurfaceResized(_) | WindowEvent::ScaleFactorChanged { .. } => resized = true,
        WindowEvent::MouseWheel { .. } => wheeled = true,
        WindowEvent::RedrawRequested => {
            let Some(handle) = root.peek().clone() else {
                return;
            };
            let changed_size = std::mem::take(&mut resized);
            let did_wheel = std::mem::take(&mut wheeled);
            if changed_size {
                handle.doc_mut().resolve(0.0);
            }
            let (current, cur_scroll) = {
                let doc = handle.doc();
                let vs = doc.viewport_scroll();
                let cur = doc.get_focussed_node_id().and_then(|id| {
                    let text = doc.get_node(id)?.element_data()?.text_input_data()?;
                    Some((id, text.editor.generation(), text.scroll_offset.to_bits()))
                });
                let rect_pos = cur
                    .and_then(|(id, _, _)| doc.get_client_bounding_rect(id))
                    .map(|r| (r.x.to_bits(), r.y.to_bits()));
                (cur, (vs.x.to_bits(), vs.y.to_bits(), rect_pos))
            };
            let gen_changed = match (current, previous) {
                (Some((id, rev, _)), Some((pid, prev, _))) => id != pid || rev != prev,
                (Some(_), None) => true,
                _ => false,
            };
            if gen_changed && let Some((id, _, _)) = current {
                keep_caret_in_view(&mut handle.doc_mut(), id);
            }
            let (settled, settled_scroll) = {
                let doc = handle.doc();
                let vs = doc.viewport_scroll();
                let cur = doc.get_focussed_node_id().and_then(|id| {
                    let text = doc.get_node(id)?.element_data()?.text_input_data()?;
                    Some((id, text.editor.generation(), text.scroll_offset.to_bits()))
                });
                let rect_pos = cur
                    .and_then(|(id, _, _)| doc.get_client_bounding_rect(id))
                    .map(|r| (r.x.to_bits(), r.y.to_bits()));
                (cur, (vs.x.to_bits(), vs.y.to_bits(), rect_pos))
            };
            if changed_size
                || did_wheel
                || settled != previous
                || settled_scroll != prev_scroll
                || cur_scroll != prev_scroll
            {
                let viewport_moved = settled_scroll.0 != prev_scroll.0
                    || settled_scroll.1 != prev_scroll.1;
                previous = settled;
                prev_scroll = settled_scroll;
                if changed_size || viewport_moved {
                    on_event(FieldEvent::ViewportChanged);
                }
                on_event(FieldEvent::Changed);
            }
        }
        _ => {}
    });
}

/// Report what the user did, for the inspector: each redraw's key presses,
/// composition, pointer buttons, scrolling and resizing, as short labels.
pub fn use_input_trace(mut on_inputs: impl FnMut(Vec<String>) + 'static) {
    use dioxus_native::winit::event::{Ime, WindowEvent};
    use dioxus_native::winit::keyboard::{Key as WindowKey, NamedKey as WindowNamed};
    let mut pending = vec!["Opened inspector".to_owned()];
    let mut modifiers = String::new();
    dioxus_native::use_window_event(move |event, _| {
        // AppKit can deliver input to an occluded window between redraws.
        // Report the preceding input before a key takes its own observation.
        if let WindowEvent::KeyboardInput { event: key, .. } = event
            && key.state.is_pressed()
        {
            if !pending.is_empty() {
                on_inputs(std::mem::take(&mut pending));
            }
            // The inspector's own keys are not inputs to the note.
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
                on_inputs(std::mem::take(&mut pending));
                return;
            }
            _ => return,
        };
        if pending.len() < super::TRACE_BATCH {
            pending.push(label);
        }
    });
}

/// Save an inspection report as a file only its owner can read (it holds the
/// note's text), in `MARGIN_REPORT_DIR` or the system's temporary directory.
/// Says where it went.
pub fn save_report(bytes: &[u8]) -> std::io::Result<String> {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    let dir = std::env::var_os("MARGIN_REPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("margin-reports"));
    std::fs::create_dir_all(&dir)?;
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_nanos();
    let path = dir.join(format!("margin-{time}-{}.json", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(&path)?.write_all(bytes)?;
    Ok(path.display().to_string())
}

/// Blitz gets its fonts from the font context and needs no page defaults.
#[component]
pub fn Head() -> Element {
    rsx! {}
}

/// Open a window titled `title` whose content is `root`.
pub fn launch(title: &str, size: (f64, f64), root: fn() -> Element) {
    let window = dioxus_native::WindowAttributes::default().with_title(title);
    // On iPhone and iPad the window is the screen; a requested size would crop it.
    let window = if cfg!(target_os = "ios") {
        window
    } else {
        window.with_surface_size(dioxus_native::LogicalSize::new(size.0, size.1))
    };
    #[cfg(target_os = "macos")]
    super::macos::launch(window, root);
    #[cfg(not(target_os = "macos"))]
    {
        let config = dioxus_native::Config::new()
            .with_window_attributes(window)
            .with_font_ctx(margin_ui::typefaces::notebook_fonts());
        dioxus_native::launch_cfg(root, vec![], vec![Box::new(config)]);
    }
}
