# Native code editor and file tree — design

Date: 2026-10-11
Status: Draft, awaiting review

## Intent

Onyxterm gets a native code editor and a folder manager that live inside the existing pane system, look like the rest of the app (same theme tokens, island cards, JetBrains Mono, minimal chrome), and feel as fast as Zed. LSP is out of scope; language servers arrive later as extensions (manifest + grammar + LSP binary).

What the user asked for:
- Native Rust editor, performance is a must.
- No LSP now; language tooling later as extensions.
- Same UI as the app, minimalistic and modern.
- A folder manager.
- v1 includes: syntax highlighting, multi-cursor, find/replace in file, Cmd-P fuzzy open.
- Both standard and vim key handling, switchable in settings.
- File tree is a pane, never a sidebar (standing rule: no side rails).

Assumptions (correct these if wrong):
- UTF-8 text only. Binary or non-UTF-8 files show a "can't open this file" pane rather than being edited.
- No soft wrap in v1; long lines scroll horizontally.
- Cursor and scroll position are not restored across restarts in v1; open files are.
- The editor is always compiled in for v1. The "install later" toggle arrives with the extension system, not now.

## Blueprint deviation

`docs/blueprint.md:353,373` schedules the editor for Phase 3 and forbids editors in Phase 1. The user has explicitly pulled it forward. Step 1 of the plan adds `docs/adr/0002-editor-and-file-tree-panels.md`, recording:
- The editor and file tree move ahead of the blueprint's phase.
- New panel kinds `Editor` and `FileTree` (the blueprint's "future registered kinds").
- Schema migration v4 (see Persistence), satisfying the ADR-0001 rule that a schema change needs a migration plus an ADR addendum.
- New dependencies and why each is needed.

## Performance targets

These sit inside the blueprint budgets (`docs/blueprint.md:225-237`):

| Metric | Target |
|---|---|
| Keystroke → painted frame | < 8 ms p95 on a 10 MB file |
| Open a 10 MB file | < 100 ms to first paint (load off the UI thread) |
| Scroll a 100k-line file | no frame > 16 ms |
| Idle editor or file tree | no added CPU beyond the watcher's OS events |
| Re-highlight after an edit | incremental; never blocks the UI thread |
| Cmd-P over 100k files | results update < 16 ms per keystroke |

Rules that keep the targets reachable:
- Work is proportional to the viewport, never to file size: only visible lines are sliced, highlighted, shaped and painted.
- The UI thread never does filesystem I/O, full-file parses or directory walks (`docs/blueprint.md:191`).
- Tracing spans on load, save, parse and paint, matching ADR-0001's observability rule.

## Architecture

```
crates/editor-core   pure Rust, no gpui
  buffer.rs          Rope (ropey), version counter, saved_version, line ending
  selection.rs       Vec<Selection{anchor, head}> in char offsets, sorted + merged
  edit.rs            multi-range edits applied back-to-front; transactions
  history.rs         undo/redo of transactions, grouped by time and boundary
  motion.rs          word/line/document motions shared by standard + vim
  keys_standard.rs   Keystroke-shaped input → EditorAction
  vim.rs             mode + count + operator state machine → EditorAction
  search.rs          find/replace over the rope (regex-cursor), case/regex toggles
  syntax.rs          tree-sitter parse tree + highlight spans for a byte range
  languages.rs       built-in grammars + highlight queries, picked by extension

crates/fs-service    background threads, no gpui
  tree.rs            lazy directory listing via ignore (respects .gitignore)
  watch.rs           notify watcher → coalesced change events
  index.rs           file index + nucleo fuzzy matcher for Cmd-P
  io.rs              load file to Rope, atomic save (temp file + rename), trash

crates/ui
  src/editor/        EditorPane state, painter (canvas), key/IME routing
  src/file_tree/     FileTreePane state, uniform_list rendering
  src/shell/*        Pane becomes an enum over content kinds
```

`editor-core` and `fs-service` depend only on `domain` (if at all) and their own deps, so they compile and test without gpui. This keeps the blueprint's dependency direction (`docs/blueprint.md:271-275`).

### Dependencies

All pinned with `=` like the rest of the workspace; the plan picks exact versions at least two weeks old.

| Crate | Purpose |
|---|---|
| `ropey` | Rope buffer, O(log n) edits and line lookup |
| `tree-sitter` | Incremental parsing |
| `tree-sitter-rust`, `-typescript`, `-javascript`, `-json`, `-md`, `-toml-ng` | Built-in grammars |
| `regex-cursor` | Regex search directly over rope chunks, no full-string copy |
| `ignore` | Gitignore-aware directory walking |
| `notify` | Filesystem watching |
| `nucleo` | Fast fuzzy matching for Cmd-P |
| `trash` | Delete from the file tree goes to the OS trash, not `rm` |

Built-in grammars are linked natively in v1. Loading `.wasm` grammars from downloaded extensions comes with the extension system.

## Step 1 — Pane-kind seam

Today `Pane` in `ui/src/shell.rs:62` is terminal-only, and about 15 `Shell` methods assume a PTY.

- Domain: `PanelKind` gains `Editor` and `FileTree`. `ResourceRef` gains `Document(DocumentId)`; the path is `PanelSnapshot.path: Option<PathBuf>`. Constructors `Panel::new_editor(path)` and `Panel::new_file_tree(root)`.
- UI: non-terminal panes live in `Shell.documents: HashMap<PanelId, DocumentPane>`; terminal code paths are unchanged. Every terminal-only method in the scout list (`spawn_restored`, `sync_live_cwds`, `send_key_to_terminal`, `drop_paths`, `focused_handle`, close/confirm, `panel_cwd`, `discard_pane`, `apply_batch`, `render_pane`, `islands::sync_grids`, `pane_status`, `copy_mode`, `text_input`) matches on the kind and either handles it or skips non-terminals.
- Key routing: `Shell::consume_key` keeps its order (dialogs → menus → palette → keymap → copy mode); after the global keymap, the key goes to the focused pane's kind instead of straight to the terminal.
- IME: `Shell`'s `EntityInputHandler` delegates to the focused pane's kind; the editor supplies its own `bounds_for_range` from the cursor.
- Closing an editor with unsaved changes uses the existing confirm dialog (`ui/src/confirm.rs`) with Save / Discard / Cancel. Quit with unsaved buffers asks the same.
- `restore_policy` fallbacks (`fall_back_missing_panel_cwds`, `fill_missing_panels`, `fresh_terminal`) apply only to terminals. An editor whose file no longer exists restores as a "file not found" editor pane; a file tree whose root is gone shows a 'not found' placeholder.

Done when: every existing test passes, terminals behave identically, and a stub editor pane can be split, focused, moved, closed and restored.

## Persistence

- Migration v4 adds `panels.path TEXT NULL`.
- `ResourceRef::Document(id)` stores `resource_kind = 'Document'` and the document id; the path is stored in `panels.path`.
- The codec (`persistence/src/codec.rs:41-79`) gains `editor` / `file_tree` kind strings and the `path` resource.
- `PanelSnapshot` carries the path through `write_snapshot` / `load_snapshot`.
- Restore reopens the file from disk; unsaved edits are never persisted (quit already confirmed them).

## Step 2 — Editor core and view

**Buffer.** `Rope` plus `version: u64` (bumped on each edit) and `saved_version`. `is_dirty = version != saved_version`. Line endings are detected on load and preserved on save.

**Selections.** Char-offset `anchor` and `head` pairs, always sorted and merged after an edit. A single cursor is just a single selection. Each selection keeps a preferred column for vertical moves.

**Edits.** An `Edit` is a list of `(range, text)` pairs applied back-to-front so offsets stay valid. Typing with N cursors is one edit with N ranges. Each edit is also turned into a tree-sitter `InputEdit`.

**Undo.** Transactions group consecutive typing within 300 ms or until a cursor jump; each stores the inverse edit and the selections before and after.

**Loading.** `fs-service::io` reads on a background thread into a `Rope` (`Rope::from_reader`). Files that are binary or not UTF-8 are rejected with a message. The pane shows the theme background until loaded; no spinner unless loading takes over 150 ms.

**Saving (Cmd-S).** Atomic write: temp file in the same directory, `fsync`, rename. On error the buffer stays dirty and the pane header shows the error.

**External changes.** The watcher reports changes to open files. A clean buffer reloads silently, keeping cursors clamped. A dirty buffer gets a "changed on disk" badge in the pane header (same style as the COPY badge, `shell/pane_header.rs:68-78`); Reload and Keep mine are in the pane menu.

**View and painter.** Built like `terminal_canvas` (`ui/src/terminal_view.rs:213`): a `canvas(prepaint, paint)`.
- Prepaint works out the visible line range from scroll offset and line height, slices just those lines from the rope, and builds `TextRun`s from highlight spans.
- Paint order: background, current-line highlight, selections, text (one `shape_line` per visible line, relying on GPUI's line layout cache like the terminal does), cursors, then the gutter.
- Gutter: right-aligned line numbers in `text.muted`; the current line uses `text.primary`.
- Cursors: a bar in standard mode and vim insert, a block in vim normal and visual. Blinking follows the terminal's cursor setting.
- Scrolling: smooth pixel scrolling with trackpad, horizontal scroll for long lines, and the cursor kept in view with 3 lines of margin.
- Mouse: click places the cursor; drag selects; double-click selects a word; triple-click selects a line; Alt-click adds a cursor; Alt-drag makes a column selection.
- Colors: new `editor` token group in `Theme` (gutter, current line, selection, cursor, plus syntax highlight names) with defaults in every `assets/themes/*.toml`. Syntax colors map tree-sitter capture names (`keyword`, `string`, `function`, `type`, `comment`, `number`, `constant`, `property`, `punctuation`, `operator`, `variable`) to tokens, falling back to the ANSI palette.

**Standard keys.** Arrows; Alt+arrows for word moves; Cmd+arrows for line and document ends; Shift adds selection to any of these; Cmd-A, Cmd-C/X/V (through the global commands), Cmd-Z, Cmd-Shift-Z; Tab and Shift-Tab indent and outdent; Enter keeps the current indentation; Backspace and Delete with Alt or Cmd for word and line; Cmd-/ toggles line comments using the language's comment token.

Pane header shows the file name and a dirty dot. `Pane::title` is the file name.

## Step 3 — File tree pane

- State: root path, set of expanded directories, and a flat `Vec<Row>` of what's visible (depth, name, is_dir, path). It's rendered with `uniform_list`, so 100k visible rows cost the same as 30.
- Listing: directories load on expand through `fs-service::tree` on a background thread. `.gitignore` is respected; `.git` is hidden. Directories are listed first, then files, in natural sort order.
- Watching: one `notify` watcher per tree root. Events are coalesced over 50 ms and only refresh expanded directories.
- Keys: j/k and arrows move; Enter or l opens a file or expands a folder; h collapses or goes to the parent; `a` creates a new file and `A` a new folder (inline `TextField`, `ui/src/text_field.rs`); `r` renames inline; `d` deletes after a confirm and sends the item to the trash; `y` copies the path. Mouse click selects; double-click opens.
- Opening a file: if the tab already has an editor showing that file, focus it. Otherwise reuse the most recently focused editor in the tab if it is clean. Otherwise split a new editor pane to the right of the tree.
- Visuals: rows use `menu_row`-style spacing, chevrons from `theme/icons.rs`, file names in `text.primary`, ignored or hidden files in `text.muted`, and the selected row in the accent wash used by palette selection.

Commands and bindings, added to `CommandId`, `default_commands()` and the keymap:

| Command | Binding |
|---|---|
| Open Folder… (native picker, then a file tree pane split left) | Cmd-Shift-O |
| Toggle File Tree (workspace root or focused pane's cwd) | Cmd-Shift-B |
| Quick Open | Cmd-P |
| Save | Cmd-S |
| Find | Cmd-F |
| Find and Replace | Cmd-Alt-F |
| Add next occurrence | Cmd-Alt-D |
| Select all occurrences | Cmd-Alt-L |
| Toggle Vim Mode | palette only |

Every existing global binding keeps its meaning in every pane (Cmd-D split, Cmd-Shift-E equalize, Cmd-O workspace menu, Cmd-h/j/k/l focus, Cmd-Shift-h/j/k/l swap, Cmd-Ctrl-h/j/k/l resize). The new combos above are free today. The existing global `SelectAll`, `Copy` and `Paste` commands dispatch to the focused pane's kind, so in an editor they act on the buffer. Editor-only keys (Cmd-Z, Cmd-Shift-Z, Cmd-X, Cmd-/, Cmd+arrows, Alt+arrows) are handled by the editor after the global keymap misses.

## Step 4 — Syntax highlighting

- `languages.rs` maps file extensions to a grammar, highlight query and comment token for Rust, TypeScript/TSX, JavaScript/JSX, JSON, Markdown and TOML. Other files show as plain text.
- After each edit the tree is updated with `InputEdit`, and parsing runs on a background thread on a cloned rope (ropey clones are O(1)). Results come back tagged with the buffer version; stale results are dropped. Until a new tree arrives, painting uses the last tree with spans shifted by the edit.
- Highlights: `QueryCursor` limited to the visible byte range, so cost is proportional to the viewport. Spans are cached per visible line and keyed by tree version.

## Step 5 — Multi-cursor and find/replace

- Cmd-Alt-D adds the next occurrence of the selection, or of the word under the cursor. Cmd-Alt-L selects every occurrence. Alt-click and Alt-drag come from the step 2 mouse work. Esc collapses back to one cursor.
- Find bar: a slim strip at the top of the editor pane, inside the island, built from `TextField`. It has toggles for case, whole word and regex, shows "3 of 41", and has Enter / Shift-Enter for next and previous. Replace adds a second field with Replace and Replace All (one undo transaction).
- Matching runs on a background thread for buffers over 1 MB and is cancelled whenever the query changes. Match highlights paint only in the visible range.

## Step 6 — Vim mode

- Setting `editor.vim_mode` (bool) in `app_settings` / `ui/src/settings.rs`, toggled from the palette. Default off.
- `vim.rs` is a pure state machine: `Vim { mode, count, operator, pending }` takes keystrokes and produces `EditorAction`s, so it is unit-testable without gpui. It reuses `copy_mode::typed_text` (`ui/src/copy_mode.rs:45`) for key normalisation.
- Modes: Normal, Insert, Visual, Visual Line, Visual Block (backed by multi-cursor).
- Motions: h j k l, w b e, W B E, 0 ^ $, gg G, f F t T ; ,, %, { }, Ctrl-u/d.
- Operators d c y > < with motions and text objects iw aw i" a" i( a( i{ a{; counts; doubled operators (dd, yy, cc); x, p, P, u, Ctrl-r, `.` repeat, o O, A I, J, r.
- `/` and `?` use the find engine, with n and N.
- Registers: unnamed only, backed by the system clipboard.
- Mode badge: NORMAL / INSERT / VISUAL in the pane header, same style as the COPY badge.

## Step 7 — Cmd-P fuzzy open

- `fs-service::index` walks the root with `ignore` on a background thread, then keeps the list current from watcher events. The root is the workspace root, else the focused file tree's root, else the focused pane's cwd.
- Matching uses `nucleo`, which runs incrementally on its own threads. The UI shows the top 50 results.
- UI reuses the palette overlay (`ui/src/palette.rs:52` `build_overlay`), showing the file name with matched characters highlighted and the dimmed relative directory. Enter opens the file using the step 3 rules.

## Error handling

- Every filesystem error (permission denied, file vanished, disk full) shows in the pane header's status line or a toast. Nothing panics, and the buffer is never lost.
- Saves are atomic. A failed save leaves the old file intact and the buffer dirty.
- Delete goes to the trash and asks for confirmation first.
- Closing or quitting with dirty buffers always asks.
- A grammar or query failure falls back to plain text, logged with `tracing::warn`.

## Testing

- `editor-core`: unit tests for edits with many cursors, selection merging, undo grouping, every motion, a vim table test (keys in, buffer and cursors out), search and replace including regex and Replace All, and highlight spans for each language. These are the bulk of the tests.
- `fs-service`: temp-dir tests for listing order, gitignore, atomic save, watcher coalescing and index updates.
- `ui`: gpui `test-support` tests for pane-kind routing (keys reach the right pane, terminal unaffected), split/close/restore of editor and tree panes, and the dirty-close confirm.
- `persistence`: migration v3 → v4 round trip; snapshots with editor and file tree panels.
- Performance: an `#[ignore]` timing test in `editor-core` that loads a generated 10 MB file, applies 1,000 edits and highlights the viewport, asserting it stays under the targets. Run with `cargo test -- --ignored`.
- Verification for every step: `cargo build`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, plus manual `cargo run -p app`.

## Out of scope

LSP, project-wide search, git gutter or diff, soft wrap, minimap, code folding, autocompletion, bracket auto-pairing, split views of one buffer, cursor and scroll restore, extension loading, WASM grammars, and remote files.
