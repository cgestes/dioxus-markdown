# Margin · a Rust Markdown notebook

Margin is a live Markdown notebook built with Dioxus Native and Loro. Run `cargo run --locked -p margin-editor` to open the desktop editor on *Field notes*, a sample note with an editable checklist, a listening table and two anchored discussions, kept in memory. Build with the `vault` feature (`cargo run --locked -p margin-editor --features vault`) to keep your notes encrypted on your device: Margin then reopens your selected note on the next launch. The same editor runs in a browser with Dioxus Web.

The editor and every read-only view use the same table stylesheet. A light header with small muted labels sits above a single rule, fine horizontal rules separate the rows, and the accent appears only for focus and selection. The first left-aligned column uses a medium weight, descriptions use the body sans, and right-aligned values use the mono font. Columns keep their widths while you type. Long cell text wraps and its row grows. [Table styles](crates/margin-ui/assets/table.css) · [Desktop](mockup/native-table.png)

- Run `cargo run --locked -p margin-editor -- --demo` for a fresh, in-memory copy of *Field notes*. Click the first letter of that title or any later ATX heading. At that position, **#** adds a heading level up to six, **Space** accepts the Markdown separator, and **Backspace** removes one level. Removing the last hash makes the line a paragraph. The left gutter shows the prefix while that heading is focused. Up and Down move one visual line at a time through headings, wrapped prose, blank lines and table rows. **Source** exposes the same Loro text and undo history. [Native table preview](mockup/native-table.png)

## Personal vault

The personal vault needs the editor's `vault` feature: `cargo run --locked -p margin-editor --features vault`. Without it, `margin` opens *Field notes* in memory, and its status bar says nothing is saved.

**Notes** lists your notes, creates a device-only note, attaches a file, and exports an attachment. The first time an older vault resumes, Margin opens its most recently created note or its existing inbox if you wrote there. If vault access fails, **Try again** retries the unlock. **Open Field notes demo** opens an in-memory copy for that session.

**Sync a copy of current text** starts a fresh Loro document with the current Markdown. Earlier drafts, existing files and private conversations stay on the original device. Files attached to the new copy sync with its edits. F9 saves private inspector reports as encrypted files in the device inbox. Reopen a specific note with `--note ID`. Give the GUI `--vault PATH` and the CLI `--dir PATH` to select the same nondefault device directory.

`cargo run --locked -p margin-editor -- --sync-file PATH` opens a standalone Loro history file. This explicit file mode reads and writes unencrypted local JSON, with or without the `vault` feature.

### Start a relay and pair devices

Build the standalone CLI with `cargo build --locked -p margin-vault-cli` (it writes `target/debug/margin-vault`). The examples below assume its directory is on your PATH. For a new owner vault, run the following in a private directory.

```sh
margin-vault init --recovery owner.recovery
margin-vault info
margin-vault serve --database relay.sqlite3 --owner PUBLIC_VAULT_ID
```

When upgrading an existing relay, restart it with the same database and owner ID. Paired devices reuse their keys and cursors.

The ID printed by `init` is public. If you already opened the desktop app, use `margin-vault backup owner.recovery` and the vault ID from `info`. The relay pins that ID and serves one vault. Leave it running, then use another terminal on the owner device.

```sh
margin-vault connect ws://127.0.0.1:8787
margin-vault invite second-device.invite
```

Transfer the invitation privately. Each invitation contains a shared vault key and a distinct device credential. On the receiving native device, run `margin-vault join second-device.invite`, then open Margin or run `margin-vault sync --watch`. `invite --qr` also prints a scannable code. For separate machines, configure a `wss://` endpoint with TLS termination in front of the loopback relay **before** issuing invitations. Restart an already-open desktop app after changing its relay or credentials.

### Keys, history and recovery

The desktop app keeps a wrapping key in the OS keychain. Headless use supports `--passphrase-file PATH` when creating and opening the device. It derives the wrapping key with Argon2id. Both use authenticated encryption. Keep the **owner recovery kit offline**. On a replacement device, run `recover owner.recovery`, connect to the original relay, and run `sync` to restore the shared history it retains. Device-only data needs a backup of that device’s encrypted storage and its unlock credentials.

After pairing, each device subscribes to the vault over a WebSocket. The relay replays records after the device’s saved cursor and pushes new edits as they arrive. Documents use Loro’s binary `%ELO` updates and acknowledgements. Records above the single-frame limit use Loro fragments. Margin carries the device signatures, vault cursor and encrypted attachment chunks alongside those frames. An interrupted upload stays in the local outbox until the relay confirms its durable write. A receiver advances its cursor after it verifies and saves a complete record. [Loro encrypted sync format](https://github.com/loro-dev/protocol/blob/main/protocol-e2ee.md)

A fresh random file key, name, and size live inside the encrypted document. The relay can see opaque room, file, writer and device IDs, causal counters, record sizes, traffic timing and client addresses. It receives a separate bearer credential. Devices reopen each full checkpoint and check its version before publishing it. The relay requests checkpoints after 64 active deltas by default, retains concurrent edits, and keeps superseded records for seven days. Creating a new checkpoint requires an online, unlocked device. A compacted full Loro snapshot still contains integrated editing history. Deleting superseded ciphertext records shrinks the relay journal. Document records have a 16 MiB limit. Files move in 128 KiB chunks.

Use `margin-vault compact NOTE_ID` to request compaction immediately. `revoke --database … --owner … DEVICE_ID` blocks that credential at the relay. If a device is lost, the owner can run `rotate --recovery NEW_FILE`, then issue fresh invitations and use `refresh` on each remaining device. Previously downloaded content and keys remain on those devices. [Upstream encrypted record format](https://github.com/loro-dev/protocol/blob/main/protocol-e2ee.md) · [Upstream history investigation](docs/loro-history-brief.md) · [Measured encrypted range recovery](docs/loro-history-results.md)

## Browse your agent's topics

`osom topics` (the Osom app) opens a three-pane window over a [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw) agent and the [SilverBullet](https://silverbullet.md) space it keeps, both reached over Tailscale. Topics are the space's folders (`car/`, `wedding/`) and person pages (`people/<name>`). Each topic page has an Overview (its README rendered), Discussions (the Telegram groups, Slack threads and Margin chats the space README routes to it), Calendar (its rows of `reminders.md`), Questions (`questions.md`), Docs and Lists. The right pane chats with the agent in the session `topic-<id>`. Docs open in the Live editor and save back with SilverBullet's `If-Match`, so an edit never overwrites a newer change by the agent. Comments on those pages are answered by a read-only agent that proposes replacements. Margin applies them, and threads are saved next to the page in `_margin/comments/`. [Comments backend](docs/comments-backend.md) [Design](docs/topics-design.md) · [Native window](mockup/topics-native.png) · [Design mockup](mockup/topics.html)

On **Today**, **Key dates** shows the next three weeks of entries from the `Key dates` section of `planning/README.md`. Give each list entry an ISO date such as `2031-04-08`. **Open Planning** opens the source for editing. **Coming up** shows open items from `reminders.md` by their next ping, or their deadline when no ping is set. Returning to Today or clicking **Refresh** rereads the space. ZeroClaw runs the notification jobs.

| Command | What it does |
| --- | --- |
| `cargo run --locked -p osom -- topics --demo` | The designed window with sample data. No network. |
| `osom setup <silverbullet-url> <zeroclaw-url> <agent>` | Writes `~/.config/margin/topics.json`. |
| `osom pair <code>` | Exchanges a code from `zeroclaw gateway get-paircode --new` (run on the agent's host) for a device token. Release builds keep it in the macOS Keychain; dev builds keep it in `~/.config/margin/token-<host>` (readable only by you), so rebuilding never triggers a Keychain prompt. `ZEROCLAW_TOKEN` overrides both. |
| `osom` (or `osom topics`) | The live window. `--open <topic\|today\|inbox>`, `--tab <name>` and `--doc <path>` choose where it starts. |
| `osom comment-test <page> "<quote>" "<request>"` | Asks the comment agent about a passage of a page, as a new comment thread would, and prints its reply and proposed edit. Uses a throwaway session. |
| `osom topics --print` | Lists topics with their reminder, discussion, question and doc counts. |
| `scripts/bundle-osom.sh` | Builds `target/Osom.app`, the Topics window as a double-clickable macOS app. |
| `margin ui` | The design system's gallery: every component in every state. `cargo test -p margin-ui ui_gallery_shot -- --ignored` paints it to `target/ui-gallery.png`. [Design system](docs/design-system.md) |
| `cargo test -p osom topics_shots -- --ignored` | Paints each view to `target/topics-shots`. Add `MARGIN_TOPICS_LIVE=1` for the live space. |

## Test native Live editing

Run `./scripts/test-editor --inspect` to open a fresh native note, or double-click the [macOS inspector launcher](scripts/inspect-editor.command). In an existing Live note, click in the text and press **F8** to toggle the inspector. On keyboards with media keys, use **Fn+F8** and **Fn+F9**. [Inspector preview](mockup/native-inspector.png)

The inspector colors headings, blank lines and prose, outlines table controls and paints the active caret magenta. The header shows the focused field, Markdown line and column, directed UTF-8 byte selection and latest input. The native window title keeps the position visible when the header scrolls away. **Save report** or **F9** captures a JSON report with the starting and current Markdown, focus, text selection, selected table corners, field and caret rectangles, window scale and the last 48 groups of inputs. A frame can contain several inputs between draws. The launcher writes reports under `target/editor-tests/reports`. In a personal vault the report is an encrypted inbox attachment. Otherwise, as in demo mode or any build without the `vault` feature, it is a plain JSON file readable only by you, in the system temporary directory under `margin-reports`. Set `MARGIN_REPORT_DIR` for demo or test reports. Existing plaintext exports and inspector files from earlier versions remain in their original locations.

| Command | What it exercises |
| --- | --- |
| `./scripts/test-editor` | Runs the portable Live workflows and, on macOS, a separate native window keyboard check. Opens a disposable note for the window check. |
| `./scripts/test-editor --headless` | Drives pointer, key-down, key-up and IME events through Dioxus, checks the Loro Markdown and native selection, and inspects rendered caret pixels at 1× and 2×. |
| `./scripts/test-editor --headless --seed 17` | Repeats the same sequence of 60 edits. The reference model checks the Markdown and caret after every input. The standard run uses four seeds. |
| `./scripts/test-editor --mac-window` | Drives arrows, Return, deletion, rectangular selection and forward/backward Tab in a new macOS window. Checks its Markdown, focus and selection after each physical key. Uses a logged-in macOS desktop, Orca’s background window driver and the Swift compiler. |

The runner writes a self-contained [HTML report](target/editor-tests/index.html) with the failed checks first. Expand **Passed checks** to inspect the other screenshots or **Inputs and Markdown** to see how a checkpoint was reached. Each run also has its own directory. A mismatch or an unavailable requested window check returns a nonzero exit status. The native window checks use the observed state before each key as their starting point, so an earlier failure does not change the expected behavior of the next gesture. Interactive F9 reports are observations. Seeded workflows can be replayed by seed.

## Run it in a browser and on iPhone

The notebook is one editor on two renderers. Blitz draws it on the desktop and on iPhone, and Dioxus Web runs the same code in a browser:

```sh
rustup target add wasm32-unknown-unknown
dx serve --web --package margin-editor
```

That is the notebook itself: Live and Rich editing, Split, Source, tables, checkboxes, the slash menu, comments and Loro undo. Only `crates/margin-editor/src/platform/` differs. Natively it re-exports Blitz's document and Parley's text layouts. In a browser it answers the same calls from the DOM, and measures text with a hidden mirror of each field. It is the build without the `vault` feature: the page opens *Field notes*, keeps it in memory and says **Not saved** in the status bar, so a reload starts over. The vault, the relay and the space need an operating system. Comments are answered by the stand-in agent. Pasting a table or a link inserts plain text, because a browser only gives clipboard text asynchronously. Append `?split` to open Split. [Platform layer](crates/margin-editor/src/platform/mod.rs) · [Browser side](crates/margin-editor/src/platform/web.rs)

`scripts/deploy-web` bundles a release build and publishes it to GitHub Pages from the `gh-pages` branch. `crates/margin-editor/public/` adds the web app manifest, icons and an offline worker, so **Share → Add to Home Screen** in Safari installs it full screen.

On iPhone it is the Dioxus Native app. Install the Dioxus CLI from the commit `dioxus-native` is pinned to, then boot a simulator and serve the `margin` binary:

```sh
cargo install --git https://github.com/DioxusLabs/dioxus --rev 0a7242a2a72b08975584bf8ecb05076a5ee0b9c7 dioxus-cli --locked
dx serve --ios --renderer native --package margin-editor
```

Add `--features vault` for the personal vault. Edits to `rsx!` reload in the running app. In `crates/margin-editor`, `Dioxus.toml` sets the bundle identifier, and `build.rs` embeds the simulator's Keychain entitlement so the encrypted vault can store its key. For your own phone, add `--device` once Xcode has an Apple Development certificate and a provisioning profile for `dev.margin.native`. Typing, Backspace, checkboxes and the encrypted vault work, and tables render. winit's iOS view only implements `UIKeyInput`. Autocorrect, dictation, input methods, selection handles, and arrow keys from a hardware keyboard are therefore missing. The layout doesn't resize for the on-screen keyboard. Attaching and exporting files needs a native file dialog, so those menu items are hidden on iOS.

## Try the Split design

Choose **Split** in the toolbar. For a new native session, run `cargo run --locked -p margin-editor --bin margin -- --split`. In a browser, append `?split` to the URL. [Native screenshot](mockup/native-split.png)

The left pane contains the full Markdown in one text field. Edit heading hashes directly, move through a blank line, or drag from a heading across table pipes and replace the selection. The right pane renders the same note. Clicking a formatted letter positions the source caret. Shift-click extends the selection. A table’s right and bottom margins target the nearest cell. The small **+** buttons append a column or row, and select the new header or cell. A missing cell in a short Markdown row only acquires pipes when you type into it.

The same Rust parser supplies the reading layout and the source range for each displayed run on every platform. Both panes share the note and undo history. Split leaves Markdown punctuation visible on the writing side and needs space for both panes. [Rust projection](src/preview.rs) · [Native source input](crates/margin-editor/src/notebook/split.rs) · [Native preview](crates/margin-editor/src/notebook/reading.rs)

## Try the editor

- Type Markdown directly. Headings, bold, italic, strikethrough, links, inline and fenced code, and blockquotes have live styling. The opening and closing punctuation hide independently until the caret reaches them. The text keeps its position. Focused headings show their prefix in the left margin. **Source** shows every delimiter.
- In prose and task lists, `[ ]`, `[x]`, and `[X]` occupy a fixed space. A checkbox is visible at rest. When the caret is immediately before it, inside it, or immediately after it, the checkbox reveals its Markdown brackets. Between the brackets, **Space** or **X** toggles once. At the outer edges, typing continues normally. Click a marker to toggle it, or Alt-click to place the caret inside. The footer offers **Edit as text** (F2) inside a marker. **Keep literal** writes a Markdown backslash before the opening bracket.
- Click a letter in a table cell to place the caret there. Double-click a word to select it. Tab and Shift+Tab move horizontally. Enter and Shift+Enter move vertically. Tab at the last cell or Enter in the last row creates the next row. Left and Right cross into neighboring cells at the ends of their text. Up and Down keep the current column and leave the grid at the top or bottom. Ctrl/⌘+Home and End reach the first and last cells. Escape returns to the document. Type a pipe header and its alignment line, then press Enter to start the first data row. **Insert table** adds a two-column Markdown table at the current paragraph. **Source** shows its pipes.
- Hover a grid or focus a cell to reveal its two **+** buttons. The right edge adds a column and the bottom edge adds a row. Clicking beside either button puts the caret in the nearest visible edge cell. A click in the following paragraph places it there. A new row opens in its first cell. A new column opens with its header selected. Paste tab- or newline-separated spreadsheet values into a cell to fill a rectangle and expand the table. One Undo reverses the paste or structural command. Click **Table actions** below the left edge or press **Shift+F10** in a cell to delete a row or column. The menu shows the affected address and opens outside the grid when space permits. **Escape** closes it and restores the cell focus. Undo restores the contents and alignment.
- Drag across cells or click one cell and Shift-click another to select a rectangle. Option/Alt+Shift+Arrow starts a range from the keyboard. Shift+Arrow changes its moving corner. Tab and Shift+Tab cycle inside the selected rectangle, and typing edits the active cell. Copy exports tab-separated values. Delete clears the cells. Pasting one value fills the selection, and larger clipboard blocks are clipped at its edges. **Table actions** can remove all selected rows or columns in one undoable edit. The header and final column are protected. Escape returns to editing one cell.
- Select words in a heading, paragraph or cell to open the floating formatting bar. Bold, italic, strikethrough and inline code edit Markdown and preserve your selection. The shortcuts use ⌘ on macOS and Ctrl elsewhere, with B, I, Shift+X and E respectively.
- In a bullet, numbered item, or checklist, Enter continues the list. A new task starts unchecked. Tab nests an item and its children under its previous sibling. Shift+Tab outdents it. Enter on an empty top-level item starts a paragraph. Backspace at the first letter outdents a nested item or removes a top-level list prefix.
- Select a sentence and choose **Ask inline** or press ⌘J / Ctrl+J. Try **Make a checklist**, ask a question, or type `Replace with:` followed by your own Markdown. Review the original and proposed text, then **Apply edit** or **Discard**. The local demo uses deterministic replies. Accepted edits can be undone in one step.

## How a checkbox edit works

1. **Find the source range.** The Rust parser recognizes GFM task markers and scans ordinary Markdown text for the same three-character tokens. This second rule is a notebook extension. Links, code, and escaped brackets keep their Markdown meaning. [Rust parser and command](src/checkbox.rs) · [GFM task-list syntax](https://github.github.com/gfm/#task-list-items-extension-)
2. **Consume the input once.** Both caret positions inside the brackets and a selection of just the middle character are eligible. Space, x, and X request the opposite value. Key repeat and other text edits inside a structured marker leave it alone. Arrow keys navigate normally. A selection spanning a bracket uses ordinary text semantics. Literal editing is local to the view. A saved `\[x]` travels with the document. [Native input handling](crates/margin-editor/src/notebook/writing.rs)
3. **Commit to Loro.** Each note has a `LoroText` named `markdown`. A structured toggle applies the Boolean `markdown.checked` attribute to the existing middle character with expansion disabled. Two disconnected peers checking the same box target the same character. Ordinary source edits splice text. Reading Source or exporting projects the attribute onto that one ASCII character, producing plain `[x]` or `[ ]`. Use `Note::markdown()` for native output. [Rust document model](src/note.rs) · [Loro text and marks](https://www.loro.dev/docs/tutorial/text)
4. **Preserve position.** Parser ranges are UTF-8 byte offsets and the native model uses Loro’s byte-indexed mark and edit APIs. In a browser, the platform layer converts the DOM's UTF-16 offsets at its boundary. Peer selections use Loro cursors, and undo belongs to the local author. [Boundary conversion](crates/margin-editor/src/platform/web.rs) · [LoroText positions](https://docs.rs/loro/1.18.0/loro/struct.LoroText.html)

The interaction follows SilverBullet’s live preview. A writer can read a typeset paragraph, put the caret into it, and edit its punctuation. Tables keep their grid during cell entry. Each cell maps to a range in the pipe source, so a single keystroke can update another writer’s view without rebuilding the focused input. The column widths stay steady during typing. **Edit Markdown pipes** and **Source** give direct access to the delimiters. Peer presence marks the active cell and leaves local focus alone. [SilverBullet live preview](https://github.com/silverbulletmd/silverbullet/blob/main/docs/Live%20Preview.md) · [GFM tables](https://github.github.com/gfm/#tables-extension-)

## Editing interaction review

Dioxus Native and Loro can support this editor. The weak point in the first prototype was selection ownership. A table rail could receive a click without placing a caret, a heading could receive Enter without splitting, and a local input could select only its own piece of the note. A directed Markdown text selection and a rectangular table selection give the pointer, keyboard and clipboard a shared editing model. This is the same design principle CodeMirror uses for document state, selection and transactions. [CodeMirror editor state](https://github.com/codemirror/state/blob/main/src/state.ts)

| Gesture | Expected result and implementation |
| --- | --- |
| Click or drag | Native inputs handle clicks on their letters, word selection and IME. The paper resolves its gutters to the nearest shaped caret. The right table rail belongs to the visible cell in its row and the bottom rail to the cell above the pointer. Only the button activates growth. [Native caret after a rail click](mockup/table-caret.png) · [Native pointer handling](crates/margin-editor/src/notebook/pointer.rs) |
| Select and replace | A selection has a fixed anchor and a moving head in the Markdown. Shift+arrows, Shift+click, drag, Select All, copy, cut and replacement can cross headings, prose and table cells. Dragging within one grid selects cells by row and column. Copy writes their tab-separated values. A text selection crossing into prose includes the intervening Markdown. Parley paints the selected text in the native fields. [Selection projection](crates/margin-editor/src/notebook/selection.rs) |
| Navigate and join | Up and Down follow the rendered lines and keep the caret in view. Horizontal arrows cross field boundaries. Enter splits any heading at the caret. At its end, Enter writes one Markdown newline and places the caret on the new line. Backspace at the start of the following paragraph rejoins it. Tab from the note title jumps to the existing body. [Navigation](crates/margin-editor/src/notebook/navigation.rs) · [Document commands](crates/margin-editor/src/notebook/commands.rs) |
| Toggle syntax | The drawn checkbox toggles on click. Clicking its visible bracket text places a caret. Space and X toggle inside, and F2 edits the literal characters. Heading and inline punctuation use the same source positions before and after reveal. |
| Undo | Native undo items capture both selection ends as Loro cursors. Undo after a replacement can restore the original directed selection in Live or Source. Structural edits and rectangular pastes each make one commit. [Loro undo integration](src/note.rs) |

Live uses multiple native text inputs. Split provides a baseline with one native input for the same document. Compare both on dragging beyond the viewport, marked-text composition with a real IME, and selection announcements through VoiceOver before choosing a default. The automated suite covers native IME events, a long 2× viewport, Unicode, CRLF, escaped pipes and backward selections. Native two-window editing and typing latency on large tables remain unverified.

Run `cargo test --locked --workspace --all-features` for the Rust input and storage tests; `--all-features` includes the personal vault. For the browser build, run `npm --prefix mockup ci` once and keep `dx serve --web --package margin-editor --port 8741` running. With Chrome installed, `npm --prefix mockup run test:web` then drives real pointer gestures, touch, selection, typing and undo in the same editor. `npm --prefix mockup test` runs the Loro history recovery study.

## Native architecture

| Layer | Implementation |
| --- | --- |
| Interface | Dioxus Native with Blitz owns the title, prose fields, grid cells, and Source view. Live selections map to UTF-8 offsets in Loro. In a browser the same components run on Dioxus Web, and `platform/` answers the layout queries from the DOM. |
| Shared note | Rust and JavaScript use Loro 1.18, the same text container name, and the same checkbox attribute. A snapshot containing an inline checked box and emoji was opened and edited in Rust, then read back in JavaScript with both checkbox values intact. The Rust tests also exercise disconnected peers and duplicate updates. |
| Editing surface | Each standalone ATX heading receives prefix edits at its first letter. Parley’s glyph positions locate checkboxes and Markdown styling without changing line wraps. Top-level GFM tables have stable cell inputs and byte ranges for their values, padding, and delimiters. Nested or incomplete table syntax stays editable in prose. A structural command commits its source edits together. [Native text styling](crates/margin-editor/src/notebook/markdown.rs) · [Table source model](src/table.rs) |
| Files and sync | Native vaults store encrypted Loro records and file chunks in SQLite. A durable outbox and cursor resume interrupted transfers. Devices provide full checkpoints and the relay prunes the covered history. |
| Inline agent | Keep the prompt and proposed diff private until acceptance. Resolve relative positions and compare the original passage before committing one undoable edit. |

The native study pins Dioxus main at `0a7242a` with Blitz `0.3.0-beta.4`. This revision applies DOM mutations after Dioxus finishes rendering, so a component can access the native editor from an effect to restore its selection. When updating the pin, run `cargo test --locked --workspace --all-features` to exercise pointer, keyboard, and text input across the two views. [Pinned Dioxus renderer](https://github.com/DioxusLabs/dioxus/tree/0a7242a2a72b08975584bf8ecb05076a5ee0b9c7/packages/native) · [Upstream mutation handling change](https://github.com/DioxusLabs/dioxus/pull/5943)

### Crates

A crate depends only on crates listed above it, and each heavy dependency enters at the lowest crate that needs it. An edit rebuilds its crate and the crates that depend on it.

| Crate | Holds | Heavy dependencies |
| --- | --- | --- |
| `margin-editor-core` (root) | Markdown editing commands, the preview model, the topic and comment formats. No I/O. It builds for native and for wasm. | none |
| `crates/margin-note` | `Note` (a Loro document) and the standalone sync file | Loro |
| `crates/margin-vault` | The encrypted vault: keys, SQLite storage, relay and sync. `margin` uses it only with `--features vault`. | crypto, SQLite, tokio |
| `crates/margin-hub` | SilverBullet and ZeroClaw clients, the device token | HTTP, WebSocket |
| `crates/margin-ui` | The design system: tokens, components, fonts, the gallery | Dioxus |
| `crates/margin-editor` | The notebook editor (`margin`) and the API a host embeds it with | Dioxus Native, Blitz |
| `crates/osom` | The Topics window (`osom`) | |
| `crates/margin-vault-cli` | `margin-vault`, the vault from the terminal | |

Loro is the document model in every build: it merges your edits with an agent's and gives undo that restores the selection. [Why Loro stays](docs/decisions/0001-loro-is-the-document-model.md)

`margin-ui` depends only on the pure core, so vault, sync or agent changes never rebuild it. `margin --help`, `osom --help` and `margin-vault --help` list each binary's options.

## Build order

1. **Prove native typing on one file.** Open a real `.md`, edit a wrapped paragraph, select across lines, paste, undo, and save it. Start on macOS. Exercise accented input, emoji, marked IME text, arrow navigation, and selection near soft wraps. Inspect the platform accessibility tree. Confirm the native surface can return caret and range geometry for decorations. If the stock input cannot expose these, prototype a custom text surface before porting the notebook shell. A read–save round trip should preserve untouched bytes, including line endings and the final newline.
2. **Extend native lists and tables.** The native editor supports list continuation, indentation, cell navigation, adding rows and columns, and rectangular paste. Exercise native cell focus during incoming edits. Measure key-to-paint latency on a 100-row table with two writers.
3. **Connect two native editors.** Apply local input immediately, persist incremental edits, and exchange Loro updates and version vectors on join or reconnect. Use per-author undo and expiring peer presence. Verify identical Markdown after concurrent same-position inserts, disconnected edits, reordered and repeated updates, and an application restart. Exercise adding a column while a peer types and removing the cell that holds a peer’s caret. Apply an external file change against the last saved version. If it overlaps unsaved local edits, retain both versions for review. [Loro synchronization and undo](https://docs.rs/loro/1.18.0/loro/struct.UndoManager.html)
4. **Port the inline review with the fake agent.** Support questions, an exact replacement, and checklist conversion. Run three two-writer cases. An edit before the target moves the proposal with its text. An edit inside the target marks it stale. Undoing an accepted proposal preserves the other writer’s changes. A model provider can later implement the same proposal interface.

The next native milestone is opening and saving a real Markdown file with the same editing surface.
