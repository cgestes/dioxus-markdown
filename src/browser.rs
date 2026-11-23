//! The browser owns a synchronous call into Rust. The buffers stay owned by
//! Rust and are borrowed by JavaScript only between successive calls.
use crate::checkbox::markdown_markers;
use std::cell::RefCell;

#[derive(Default)]
struct Buffers {
    source: Vec<u8>,
    positions: Vec<u32>,
    preview: Vec<u8>,
}

thread_local! {
    static BUFFERS: RefCell<Buffers> = RefCell::default();
}

#[unsafe(no_mangle)]
pub extern "C" fn marker_source(len: usize) -> *mut u8 {
    BUFFERS.with_borrow_mut(|buffers| {
        buffers.source.resize(len, 0);
        buffers.source.as_mut_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn index_markers() -> usize {
    BUFFERS.with_borrow_mut(
        |Buffers {
             source, positions, ..
         }| {
            positions.clear();
            if let Ok(markdown) = std::str::from_utf8(source) {
                let mut previous = 0;
                let mut utf16 = 0;
                for marker in markdown_markers(markdown) {
                    utf16 += markdown[previous..marker.open].encode_utf16().count();
                    positions.push(utf16 as u32);
                    previous = marker.open;
                }
            }
            positions.len()
        },
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn marker_positions() -> *const u32 {
    BUFFERS.with_borrow(|buffers| buffers.positions.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn render_preview() -> usize {
    BUFFERS.with_borrow_mut(
        |Buffers {
             source, preview, ..
         }| {
            preview.clear();
            if let Ok(md) = std::str::from_utf8(source) {
                serde_json::to_writer(&mut *preview, &crate::preview::document(md))
                    .expect("writing to a vector");
            }
            preview.len()
        },
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn preview_bytes() -> *const u8 {
    BUFFERS.with_borrow(|buffers| buffers.preview.as_ptr())
}

// The public JS boundary uses UTF-16 positions, including positions in the edited document.
#[unsafe(no_mangle)]
pub extern "C" fn inline_format(anchor: usize, head: usize, action: usize) -> usize {
    use crate::format::{InlineSelection, Style};
    BUFFERS.with_borrow_mut(|Buffers { source, preview, .. }| {
        preview.clear();
        let Ok(md) = std::str::from_utf8(source) else { return 0 };
        fn byte(md: &str, offset: usize) -> usize {
            let mut units = 0;
            for (at,ch) in md.char_indices() {
                if units >= offset { return at; }
                units += ch.len_utf16();
            }
            md.len()
        }
        let Some(selected) = InlineSelection::read(md,[byte(md,anchor),byte(md,head)]) else { return 0 };
        let active: Vec<_> = Style::ALL.iter().map(|style| selected.active(*style)).collect();
        let mut result = serde_json::json!({ "active":active });
        if let Some(style) = Style::ALL.get(action) {
            let edit = selected.toggle(md,*style);
            let js = |at| md[..at].encode_utf16().count();
            result["changes"] = edit.changes.iter().map(|e| serde_json::json!({"from":js(e.range.start),"to":js(e.range.end),"insert":e.text})).collect();
            let mut after = md.to_owned();
            for e in edit.changes.into_iter().rev() { after.replace_range(e.range,&e.text); }
            result["selection"] = serde_json::json!(edit.selection.map(|at| after[..at].encode_utf16().count()));
        }
        serde_json::to_writer(&mut *preview, &result).expect("writing to a vector");
        preview.len()
    })
}
