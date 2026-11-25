# Margin design system

Margin's windows (the notebook, its comments, the Topics window) are built from one set of components in `src/native/ui/`, styled by one stylesheet, `assets/ui.css`, from the tokens in `assets/theme.css`. `margin ui` (or `margin --gallery`) opens a window with every component in every state; `cargo test --features native --bin margin ui_gallery_shot -- --ignored` paints it to `target/ui-gallery.png`.

## Principles

These follow the product principles in `SPEC.md` §1.

1. **Calm paper.** A warm page, thin rules, one accent (pine). No heavy chrome: a control appears where it is used, and quiet actions stay quiet (ghost and link buttons) until hovered.
2. **One source for each look.** A colour, radius, space or type size is a token in `theme.css`. A button, card, row, checkbox or block of Markdown looks the same wherever it appears, because it is the same component.
3. **Markdown reads the same everywhere.** Read-only Markdown (a topic's page, a chat reply, a transcript, a comment) goes through `MarkdownView`, which uses the notebook's reading projection (`preview::document`) and the editor's table markup.
4. **Features compose; they don't restyle.** A feature's stylesheet (`topics.css`, `comments.css`, `editor.css`) holds layout and marks that exist only there. It has no button, card, row, Markdown or checkbox styles of its own, and no raw colour where a token exists.

## Tokens (`assets/theme.css`)

| Group | Tokens | Use |
| --- | --- | --- |
| Surfaces | `--page`, `--rail`, `--surface`, `--wash`, `--pale` | Window background, side rails, cards and fields, quiet fills, the accent's pale fill (hover, your messages) |
| Ink | `--ink`, `--muted`, `--faint`, `--on-accent` | Text, secondary text, hints and metadata, text on a filled accent |
| Rules | `--line`, `--line-strong` | Borders and separators; hovered or emphasised borders |
| Accent | `--accent`, `--accent-deep` | Primary actions, links, selection; their hover and pressed state |
| Status | `--ok`, `--info` / `--info-wash`, `--warn` / `--warn-ink` / `--warn-wash` / `--warn-line`, `--alert` / `--alert-wash` / `--alert-line` | Connected, information, needs attention (comment highlights), errors |
| Sources | `--telegram`, `--slack`, `--chat` and their `-wash` | Where a conversation comes from (`SourceGlyph`) |
| Tile hues | `--hue-{slate,rose,clay,ochre,plum,pine}` and their `-wash` | A topic's mark (`Tile`, `ui::Hue`) |
| Shape | `--radius-s` 4, `--radius` 7, `--radius-l` 10, `--radius-pill` | Badges; buttons and fields; cards; chips |
| Space | `--space-1` … `--space-5` (4, 8, 12, 16, 24) | Gaps and padding |
| Type scale | `--text-display` 34, `--text-title` 20, `--text-heading` 15, `--text-control` 12.5, `--text-body` 13, `--text-small` 11.5, `--text-label` 10.5, `--text-mono` 11; families `--sans`, `--display`, `--mono`, `--body` | Page titles; briefs; card headings; buttons; UI text; hints; small caps labels; metadata |
| Headings | `--font-h1` … `--font-h6` (34 / 24 / 19 / 16.5 / 15 / 13.5, weight 600) with `--track-h1` … `--track-h6` | The editor's title and heading fields **and** every read-only page (`.ui-md`): a page reads the same edited and rendered |
| Prose | `--font-prose` (Rich and read-only pages, 15.5/26), `--font-prose-mono` (Live, 14/25), `--font-source` (Source, Split, code blocks, 13.5/22), `--text-code` | Body text in each mode |
| Interface roles | `--font-ui` (menu items, fields, ghost buttons), `--font-control` (buttons), `--font-hint` (hints, status bar), `--font-label` + `--track-label` (uppercase section labels), `--font-mono-ui` (shortcuts, cell addresses, counts) | Every control and its text |
| Tables | `--font-table`, `--font-table-head`, `--font-table-label`, `--font-table-number`; `--ledger-*` in `table.css` derive from the accent ramp | A light header (`--wash`, `--muted` 600 labels, a `--line-strong` rule), rows divided by `--line`, accent only for focus and selection |
| Elevation | `--shadow-pop` (popovers), `--shadow-menu`, `--shadow-bar` (the floating toolbar), `--shadow-side` (the comments drawer on narrow windows) | Anything above the page |
| Editor | `--toolbar*`, `--selected-text`, `--ink-strong`, `--punct`, `--accent-soft`, `--accent-ring`, `--accent-a08/28` | The format bar, text selection, bold, Markdown punctuation, cell selection, focus rings, resolved comment ink |

## Components

Each is a Dioxus component (or a function) exported from `ui`. Props not listed are optional.

### Actions

| Component | When | API |
| --- | --- | --- |
| `Button` | Any action. `Default` for secondary actions, `Primary` for the one main action of a place (Save, Post, Pair), `Ghost` for quiet toolbar actions (Back, Resolve, Refresh), `Icon` for icon-only buttons (the editor's dock, table menus, toolbars), `Link` for "All →", "Open in editor →" and the status bar's "Edit as text". `Bare` has no look of its own (menu items, the comment quote). | `Button { label, text?, icon?, badge?, hint?, variant?, class?, id?, tip?, selected?, expanded?, disabled?, danger?, role?, stop?, keep_focus?, focusable?, onhover?, onclick }`. `label` is the accessible name and, without `text`, `icon` or `badge`, the caption. Enter (on press) and Space (on release) activate it. `stop` keeps the click from the parent; `keep_focus` keeps a press from taking focus from the text; `danger` turns its hover to `--alert`. |
| `Chip` | A filter (Discussions by source) or a quick reply (chat suggestions, "Shorten" in a comment) | `Chip { label, icon?, selected?, id?, stop?, onclick }` |
| `Segmented` | Mutually exclusive modes side by side | `Segmented { label, items: Vec<Segment { id, label, tip, selected }>, onselect(index) }` |
| `Tabs` | Views of one place, with counts | `Tabs { items: Vec<TabItem { label, count }>, selected, onselect(index) }` |
| `NavItem` | An entry of a side navigation | `NavItem { label, hint?, leading?, count?: (n, Tone), dot?, selected?, indent?, onclick }` |

### Popovers, menus, toolbars

| Component | When | API |
| --- | --- | --- |
| `Popover` | Any floating surface: a menu, a picker. Surface, `--line` border, `--radius-l`, `--shadow-pop`. The caller places it (`class` / `style`). | `Popover { id?, class?, style?, role?, label?, keep_focus? = true, contain? = true, children }`. `keep_focus` keeps the caret in the text; `contain` keeps presses from the page below. |
| `Menu` | A list of actions (`menu`) or of choices that follow typing (`listbox`, the slash menu) | `Menu { id?, class?, style?, role?, label?, children }` — a `Popover` with `.ui-menu` |
| `MenuItem` | One entry: icon or short badge (`H1`, `↑`), label, an optional mono hint (`#`, `- [ ]`) | `MenuItem { label, id?, icon?, badge?, hint?, active?, selected?, danger?, disabled?, option?, onhover?, onclick }`. `active` is the keyboard highlight (moved by `onhover`), `option` makes it a listbox option. |
| `MenuSection` / `MenuSeparator` / `MenuRow` | A small caps heading, a rule between groups, a row of icon buttons (alignment) | `MenuSection { label }`, `MenuSeparator {}`, `MenuRow { children }` |
| `Toolbar` | A row of buttons acting on the selection or the caret. `Dock` (the editor's top bar), `Floating` (dark, over the text: the format bar), `Inline` (quiet, next to the caret: empty-line tools, a table's selection) | `Toolbar { label, id?, class?, style?, look?: ToolbarLook, keep_focus?, contain?, children }`; `ToolbarSep {}` between groups |

### Text entry and checkboxes

| Component | When | API |
| --- | --- | --- |
| `TextField` | A field outside the notebook's text. Blitz draws no placeholder, so it overlays one while empty. Keys stay inside the field. | `TextField { value: Signal<String>, placeholder, multiline?, id?, onenter?, onescape? }` |
| `Composer` | A message box: field plus send. Holds its own draft, so typing re-renders only it. | `Composer { placeholder, onsend(String), multiline?, send_label?, field_id?, send_id?, onescape? }`. Without `send_label` the button is an arrow. |
| `Checkbox` | The notebook's 15px box (`.marker-face`), anywhere a checkbox is drawn | `Checkbox { checked }` |
| `CheckItem` | A checklist line outside the notebook (procedures, lists, shopping) | `CheckItem { text, checked, tag?, onclick }` |

### Content

| Component | When | API |
| --- | --- | --- |
| `Card` | A surface grouping one kind of content | `Card { class?, id?, onclick?, children }`. With `onclick` the card is a target and shows it on hover. |
| `CardTitle` | A card's small caps heading, with a note or an action at the end | `CardTitle { text, icon?, note?, action?, onaction? }` |
| `Rows` / `Row` | A list of items separated by rules | `Row { title, leading?, snippet?, sub?, trailing?, onclick?, dot?, children }`. With `onclick` the row is a button; children go under the text. |
| `SubItem` | One fact on a row's `sub` line | `SubItem { text, icon? }` |
| `Message` | One turn of a conversation: the chat panel (`Bubble`), a transcript (`Card`), a comment thread (`Thread`) | `Message { author, body, ai?, pending?, failed?, tools?, at?, style?, children }`. While `pending`, the body is plain text ("Thinking…" when empty), so a stream is never parsed as Markdown; finished bodies render with `MarkdownView`. |
| `MarkdownView` | Read-only Markdown | `MarkdownView { text, compact? }`. `page` (default) uses exactly the editor's heading, prose and mark styles, so a topic's README reads like the page in Rich mode; `compact` scales the same roles down for message bodies. Tables use the editor's `.ledger` markup; task boxes are the editor's checkbox. `runs(passage, ids)` gives the spans alone (the Split preview uses it with `ids`). |

### Marks

| Component | When | API |
| --- | --- | --- |
| `Icon` / `icon()` | Every glyph: the editor's tools and the Topics marks | `Icon::X.draw()` (14px, in buttons), `icon(Icon::X)` (16px `svg.i`), `Icon::X.svg(class)`; `Icon::ALL`, `Icon::for_source` |
| `Tile` | A topic's mark: an icon on a tinted square | `Tile { icon, hue: Hue, size?: Small \| Medium \| Large }` |
| `Badge` | A count, a kind (`PDF`), an id (`Q1`), the agent's name | `Badge { text, tone?: Tone }` |
| `Banner` | A message across a place: an error, a note | `Banner { text, tone? }` (`Alert` is announced) |
| `DateBox` | A calendar entry's date | `DateBox { top, day }`; `dates::chip(iso, today)` gives both |
| `SourceGlyph` | Where a conversation comes from | `SourceGlyph { source }` |
| `Avatar` | Who wrote a comment | `Avatar { ai?, initial? }` (used by `Message` in thread style) |

`Hue` (`Slate`, `Rose`, `Clay`, `Ochre`, `Plum`, `Pine`) and `Tone` (`Neutral`, `Accent`, `Ok`, `Info`, `Warn`, `Alert`) map to the tokens above.

## Where they are used

| Component | Used by |
| --- | --- |
| `Button` | Editor dock (insert table, undo, redo, comments, debug, restart), `PageBar` (Save), status bar ("Edit as text" / "Keep literal"), table header and row menu buttons, the table selection bar (Clear, Del rows, Del cols), the Split preview's add/delete edges, the inspector's Save report, comments pane (close, jump, resolve, delete, apply, undo, ask again, cancel), Topics (SilverBullet, Note, Reminder, Back, Refresh, Approve/Deny, Pair, inbox filing, "Open in editor →") |
| `Menu` / `MenuItem` / `MenuSeparator` / `MenuRow` | Slash menu (listbox), block menu, table column menu (alignment row + actions), table row menu |
| `Toolbar` / `ToolbarSep` | Format bar (`Floating`), empty-line tools and a table's selection bar (`Inline`) |
| `Chip` | Topics (Discussions filter, chat quick replies), comments (quick requests) |
| `Segmented` | Editor mode bar (Rich · Live · Split · Source), comments filter (Open · Resolved · All) |
| `Tabs` | Topic page |
| `NavItem` | Topics sidebar (Today, Inbox, topics, sub-topics) |
| `TextField` | Topics search, pairing code; inside every `Composer` |
| `Composer` | Topics chat, new comment, comment reply |
| `Checkbox` | Inside `CheckItem`; its `.marker-face` look is also the editor's own checkbox |
| `CheckItem` | Topics procedures, lists, Today's shopping |
| `Card` / `CardTitle` | Topics overview, tabs, Today, inbox, docs grid; each comment thread (`Card`) |
| `Row` / `Rows` / `SubItem` | Topics agenda, discussions, questions, docs, procedures, inbox |
| `Message` | Topics chat panel, transcripts, comment threads |
| `MarkdownView` | Topic overview page, chat replies, transcripts, comment bodies (through `Message`) |
| `Icon` | Everywhere |
| `Tile`, `DateBox`, `SourceGlyph` | Topics |
| `Badge` | Topics (counts, doc kinds, question ids), comments (open count, agent name) |
| `Banner` | Topics errors, comment file errors, tool approvals |

Still custom: the block drag handle (`⋮⋮`), whose press starts a drag rather than a click, and the inspector overlay's layout (a debugging tool). Everything else in the editor's chrome is built from these components.

## Rules

- Features compose `ui::` components. No per-feature button, card, row, Markdown or checkbox styles; a feature stylesheet holds layout and marks of its own (comment highlights, the docs grid, the chat column).
- A new look is a new variant or component in `ui/`, styled in `ui.css`, shown in the gallery, and listed here.
- Colours, radii, spacing and type sizes come from `theme.css`; add a token rather than a literal.
- `ui.css` rules are scoped by `ui-*` classes (or the reading projection's `read-*` classes inside `.ui-md`). The editor's bare `button` and `nav` rules are scoped under `.notebook` with `:where()`, so they never outrank a component.
