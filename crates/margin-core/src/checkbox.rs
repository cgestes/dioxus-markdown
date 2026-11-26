use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Marker {
    pub open: usize,
    pub checked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Structured,
    Literal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Type(char),
    OtherTextEdit,
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    PassThrough,
    Consume,
    SetChecked { at: usize, checked: bool },
}

impl Marker {
    /// The caller supplies a token identified by the Markdown parser.
    pub fn from_token(open: usize, source: &str) -> Option<Self> {
        let checked = match source.as_bytes() {
            b"[ ]" => false,
            b"[x]" | b"[X]" => true,
            _ => return None,
        };
        Some(Self { open, checked })
    }

    /// Both caret stops around the middle character are inside the control.
    /// A selection covering either bracket belongs to ordinary text editing.
    pub fn edit(self, selection: Range<usize>, mode: Mode, input: Input) -> Decision {
        let at = self.open + 1;
        if mode == Mode::Literal
            || selection.start > selection.end
            || selection.start < at
            || selection.end > at + 1
        {
            return Decision::PassThrough;
        }
        match input {
            Input::Type(' ' | 'x' | 'X') => Decision::SetChecked {
                at,
                checked: !self.checked,
            },
            Input::Type(_) | Input::OtherTextEdit | Input::Repeat => Decision::Consume,
        }
    }
}

/// Reveal the source at either outside edge. Only the middle accepts toggle keys.
pub fn marker_beside(markers: &[Marker], selection: Range<usize>) -> Option<Marker> {
    if selection.end < selection.start {
        return None;
    }
    let index = markers
        .partition_point(|m| m.open <= selection.start)
        .checked_sub(1)?;
    let marker = markers[index];
    let inside = selection.start > marker.open && selection.end <= marker.open + 2;
    (inside || selection.is_empty() && selection.start <= marker.open + 3).then_some(marker)
}

/// Index GFM checklist markers and the notebook's extension for bare markers
/// in prose. Every offset is a UTF-8 byte position in the original source.
pub fn markdown_markers(markdown: &str) -> Vec<Marker> {
    use pulldown_cmark::{Event, Options, Parser, TagEnd};
    use std::borrow::Cow;
    // Treat an empty final task like the same line followed by a newline.
    // The sentinel is outside every reported range and never enters the note.
    let source = if markdown.ends_with(']') {
        Cow::Owned(format!("{markdown}\n"))
    } else {
        Cow::Borrowed(markdown)
    };
    let options = Options::ENABLE_TASKLISTS
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_WIKILINKS;
    let mut markers = Vec::new();
    let mut text_run: Option<Range<usize>> = None;
    let mut excluded = 0;
    let excludes_prose = |tag| {
        matches!(
            tag,
            TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::Table
        )
    };
    for (event, span) in Parser::new_ext(&source, options).into_offset_iter() {
        if excluded == 0 && matches!(event, Event::Text(_)) {
            if let Some(run) = &mut text_run
                && run.end == span.start
            {
                run.end = span.end;
            } else {
                if let Some(run) = text_run.take() {
                    scan_prose(&source, run, &mut markers);
                }
                text_run = Some(span);
            }
            continue;
        }
        if let Some(run) = text_run.take() {
            scan_prose(&source, run, &mut markers);
        }
        match event {
            Event::Start(tag) if excludes_prose(tag.to_end()) => excluded += 1,
            Event::End(tag) if excludes_prose(tag) => excluded -= 1,
            Event::TaskListMarker(_) if excluded == 0 => {
                let open = span.end - 3;
                if let Some(marker) = Marker::from_token(open, &source[open..span.end]) {
                    markers.push(marker);
                }
            }
            _ => {}
        }
    }
    if let Some(run) = text_run {
        scan_prose(&source, run, &mut markers);
    }
    markers
}

fn scan_prose(source: &str, run: Range<usize>, markers: &mut Vec<Marker>) {
    for (offset, _) in source[run.clone()].match_indices('[') {
        let open = run.start + offset;
        if open + 3 > run.end {
            break;
        }
        let Some(token) = source
            .get(open..open + 3)
            .and_then(|s| Marker::from_token(open, s))
        else {
            continue;
        };
        let slashes = source.as_bytes()[..open]
            .iter()
            .rev()
            .take_while(|&&b| b == b'\\')
            .count();
        if slashes % 2 == 0 {
            markers.push(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_and_x_toggle_on_either_side_of_the_state_character() {
        for (source, target) in [("[ ]", true), ("[x]", false), ("[X]", false)] {
            let marker = Marker::from_token(12, source).unwrap();
            for range in [13..13, 14..14, 13..14] {
                for key in [' ', 'x', 'X'] {
                    assert_eq!(
                        marker.edit(range.clone(), Mode::Structured, Input::Type(key)),
                        Decision::SetChecked {
                            at: 13,
                            checked: target
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn raw_text_and_selections_crossing_a_bracket_use_normal_editing() {
        let marker = Marker::from_token(9, "[x]").unwrap();
        for range in [9..9, 12..12, 9..10, 11..12, 9..12] {
            assert_eq!(
                marker.edit(range, Mode::Structured, Input::Type(' ')),
                Decision::PassThrough
            );
        }
        assert_eq!(
            marker.edit(10..10, Mode::Literal, Input::Type('x')),
            Decision::PassThrough
        );
        assert!(Marker::from_token(9, "[xx]").is_none());
    }

    #[test]
    fn other_input_and_auto_repeat_leave_a_structured_marker_alone() {
        let marker = Marker::from_token(0, "[ ]").unwrap();
        for input in [
            Input::Type('a'),
            Input::Type('é'),
            Input::OtherTextEdit,
            Input::Repeat,
        ] {
            assert_eq!(
                marker.edit(1..2, Mode::Structured, input),
                Decision::Consume
            );
        }
    }

    #[test]
    fn gfm_index_uses_byte_offsets_and_preserves_literal_contexts() {
        let md = "🎧 é\r\n\r\n- [ ] first\r\n  - [X] nested\r\n\r\n> 1. [x] quoted\r\n\r\n- \\[x] escaped\n\n`[x]` and [x](url)\n\n~~~md\n- [ ] example\n~~~\n";
        let spans = markdown_markers(md);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].open, md.find("[ ] first").unwrap());
        assert_eq!(
            spans[1],
            Marker {
                open: md.find("[X]").unwrap(),
                checked: true
            }
        );
        assert_eq!(spans[2].open, md.find("[x] quoted").unwrap());
        let mut edited = md.to_owned();
        edited.replace_range(spans[0].open + 1..spans[0].open + 2, "x");
        assert_eq!(edited, md.replacen("[ ] first", "[x] first", 1));
    }

    #[test]
    fn empty_final_and_padded_list_items_retain_the_exact_marker_range() {
        let md = "-    [x] padded\n- [ ]\n> 1. [X]";
        let markers = markdown_markers(md);
        assert_eq!(markers.len(), 3);
        assert_eq!(markers[0].open, md.find("[x]").unwrap());
        assert_eq!(markers[1].open, md.find("[ ]").unwrap());
        assert_eq!(markers[2].open, md.find("[X]").unwrap());
    }

    #[test]
    fn standalone_markers_in_prose_use_source_offsets_and_respect_markdown_context() {
        let source = concat!(
            "🎧 Ready [ ] and **sent [X]**. A separate marker follows.\n\n[x]\n\n",
            "\\[x] escaped, `[ ]` inline code, [x](url), ![x](img), [X][ref], [[x]], &#91;x].\n\n",
            "[ref]: /notes\n\n~~~md\n[ ]\n~~~\n\n    [x] indented code\n"
        );
        let offsets: Vec<_> = markdown_markers(source).iter().map(|m| m.open).collect();
        assert_eq!(
            offsets,
            [
                source.find("[ ]").unwrap(),
                source.find("[X]").unwrap(),
                source.find("\n[x]\n").unwrap() + 1
            ]
        );
        assert!(markdown_markers("[x] referenced\n\n[x]: /somewhere").is_empty());
        assert_eq!(markdown_markers("[ ][x][X]").len(), 3);
    }
}
