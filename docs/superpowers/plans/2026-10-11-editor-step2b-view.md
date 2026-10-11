# Editor Step 2b: Editor View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An editor pane is a real editor. It loads the file off the UI thread and paints only the visible lines, with a gutter, selections and cursors. You can type (including IME), move, select with the mouse, scroll, copy/cut/paste, undo and save (Cmd-S). The header shows a dirty dot, and closing a pane, closing a tab, quitting or deleting a workspace with unsaved changes asks first.

**Architecture:**
- `DocumentPane` becomes `enum { Editor(EditorPane), FileTree(FileTreePane) }`.
- `EditorPane` holds an `EditorState { Loading, Ready(editor_core::Editor), Failed(String) }` and an `Rc<RefCell<EditorView>>` with the scroll offsets and the last frame's layout. The layout is used for hit-testing and IME.
- The view is a GPUI `canvas`, built like `terminal_canvas`. Prepaint shapes the visible lines and stores the layout; paint draws them.
- Keys reach the editor from `Shell::consume_key`. Typed text arrives through the existing `EntityInputHandler`. Mouse events reach it through a callback holding `Entity<Shell>`.
- All file I/O runs on `cx.background_executor()`.

**Tech Stack:** Rust 1.85, GPUI 0.2.2, `editor-core` (plan 2a).

**Spec:** `docs/superpowers/specs/2026-10-11-code-editor-design.md` (Step 2, including "Carried from Step 1"; Performance targets; Error handling)

**Prerequisite:** plan `2026-10-11-editor-step2a-core.md` is complete. Its public API: `Buffer`, `Editor`, `EditorAction`, `Granularity`, `Motion`, `Selection`, `Selections`, `KeyInput`, `standard_action`, `display_column`, `TAB_WIDTH`, `line_len`, `io::{load, save, LoadError}`, and `Rope` re-exported.

## Global Constraints

- No code comments of any kind, including doc comments.
- Keep `#![deny(warnings)]` and `#![forbid(unsafe_code)]`. The only new dependency edge is `ui → editor-core` (path dependency).
- The UI thread never reads or writes files. Load and save run on `cx.background_executor()`, and no `path.exists()` or other stat happens per render.
- Paint cost scales with the viewport, never with file size. Shape only visible lines; never call `rope.to_string()` on the whole buffer in render or paint.
- Magic numbers become named `const`s at the top of the file.
- Match the existing patterns: `impl Shell` blocks in `crates/ui/src/shell/*.rs` with `pub(super)`, painting like `crates/ui/src/terminal_view.rs`, and confirms through `crates/ui/src/confirm.rs`.
- GPUI signatures quoted here come from gpui 0.2.2 (`~/.cargo/registry/src/*/gpui-0.2.2`). If one differs, follow the source and note it in the report.
- Commits: `TYPE(SCOPE): MESSAGE`, single line, no `Co-Authored-By` trailer, never `--no-verify`. Work on `main`.
- Verification for every task: `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`.

## Spec deviations (decided while planning)

- **Two-choice dirty confirm.** Enter discards and Esc cancels; save first with Cmd-S. The existing dialog has exactly two actions, and a third button is new UI for little gain.
- **No cursor blink.** The terminal cursor doesn't blink either; this matches it.
- **No current-line highlight in an unfocused editor, and no cursor drawn when unfocused.** This keeps an unfocused pane calm, like the terminal's dimming.
- **Dragging selects by character,** even when it starts from a word or line click.

## Review Focus

1. **A save is still running when the user keeps typing.** The buffer must stay dirty, so only the version that was actually written is marked saved. Test: Task 1 `mark_saved_at_an_old_version_keeps_the_buffer_dirty`.
2. **A huge file.** Opening a 10 MB file must not freeze the window. Load runs in the background, and the pane shows the empty theme background until it's ready. Test: Task 3 `editor_pane_starts_loading_and_finishes_ready`, plus the manual check in Task 8.
3. **IME composition** (option-e then e, Japanese input): the composing text shows at the cursor and is committed once, not twice. The key-down path must return `false` for composed text, so only `replace_text_in_range` inserts. Test: Task 5 `composed_text_is_left_to_the_input_handler`.
4. **Losing work.** Cmd-W on a dirty editor, closing its tab, Cmd-Q and deleting its workspace must all confirm first. Tests: Task 7 copy and decision tests.
5. **Tabs and wide glyphs.** The cursor sits exactly after a tab or an emoji, because x positions come from the shaped line's `x_for_index` on tab-expanded text. Test: Task 4 `display_line_maps_chars_to_bytes_through_tabs_and_emoji`.

---

### Task 1: editor-core: click selection and versioned save marking

**Files:**
- Modify: `crates/editor-core/src/buffer.rs` (add `pub(crate) fn mark_saved_at`)
- Modify: `crates/editor-core/src/editor.rs` (add `ClickMode`, `Editor::click`, `Editor::mark_saved_at`)
- Modify: `crates/editor-core/src/motion.rs` (add `pub(crate) fn word_range`)
- Modify: `crates/editor-core/src/lib.rs` (export `ClickMode`)

**Interfaces:**
- Produces:
  - `ClickMode { Place, Extend, AddCursor, Word, Line }`
  - `Editor::click(&mut self, offset: usize, mode: ClickMode)`
  - `Editor::mark_saved_at(&mut self, version: u64)`

- [ ] **Step 1: Write the failing tests** (add to `editor.rs` tests):

```rust
    #[test]
    fn mark_saved_at_an_old_version_keeps_the_buffer_dirty() {
        let mut editor = editor_with("a", &[1]);
        editor.apply(EditorAction::InsertText("b".into()), Instant::now());
        let saved_version = editor.buffer().version();
        editor.apply(EditorAction::InsertText("c".into()), Instant::now() + std::time::Duration::from_secs(1));
        editor.mark_saved_at(saved_version);
        assert!(editor.buffer().is_dirty());
        editor.mark_saved_at(editor.buffer().version());
        assert!(!editor.buffer().is_dirty());
    }

    #[test]
    fn clicks_place_extend_add_and_select_words_and_lines() {
        let mut editor = editor_with("foo bar\nbaz", &[0]);
        editor.click(5, ClickMode::Place);
        assert_eq!(editor.selections().as_slice(), &[Selection::cursor(5)]);
        editor.click(2, ClickMode::Extend);
        assert_eq!(editor.selections().as_slice(), &[Selection::new(5, 2)]);
        editor.click(9, ClickMode::AddCursor);
        assert_eq!(editor.selections().len(), 2);
        editor.click(5, ClickMode::Word);
        assert_eq!(editor.selections().as_slice(), &[Selection::new(4, 7)]);
        editor.click(1, ClickMode::Line);
        assert_eq!(editor.selections().as_slice(), &[Selection::new(0, 8)]);
        editor.click(10, ClickMode::Line);
        assert_eq!(editor.selections().as_slice(), &[Selection::new(8, 11)]);
    }

    #[test]
    fn word_click_on_whitespace_selects_the_whitespace_run() {
        let mut editor = editor_with("a   b", &[0]);
        editor.click(2, ClickMode::Word);
        assert_eq!(editor.selections().as_slice(), &[Selection::new(1, 4)]);
    }
```

- [ ] **Step 2: Run and confirm failure.** `cargo test -p editor-core` → compile errors.

- [ ] **Step 3: Implement.**

`buffer.rs`:

```rust
    pub(crate) fn mark_saved_at(&mut self, version: u64) {
        self.saved_version = version;
    }
```

`motion.rs` (reuses the private `class`):

```rust
pub(crate) fn word_range(rope: &Rope, offset: usize) -> std::ops::Range<usize> {
    let len = rope.len_chars();
    let probe = if offset < len { offset } else { offset.saturating_sub(1) };
    let Some(target) = (len > 0).then(|| class(rope.char(probe))) else {
        return offset..offset;
    };
    let mut start = probe;
    while start > 0 && class(rope.char(start - 1)) == target {
        start -= 1;
    }
    let mut end = probe;
    while end < len && class(rope.char(end)) == target {
        end += 1;
    }
    start..end
}
```

`editor.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickMode {
    Place,
    Extend,
    AddCursor,
    Word,
    Line,
}
```

```rust
    pub fn mark_saved_at(&mut self, version: u64) {
        self.buffer.mark_saved_at(version);
    }

    pub fn click(&mut self, offset: usize, mode: ClickMode) {
        let rope = self.buffer.rope();
        let offset = offset.min(rope.len_chars());
        self.selections = match mode {
            ClickMode::Place => Selections::single(Selection::cursor(offset)),
            ClickMode::Extend => Selections::single(Selection::new(self.selections.newest().anchor, offset)),
            ClickMode::AddCursor => self.selections.with(Selection::cursor(offset)),
            ClickMode::Word => {
                let range = word_range(rope, offset);
                Selections::single(Selection::new(range.start, range.end))
            }
            ClickMode::Line => {
                let start = line_start(rope, offset);
                let end = (line_end(rope, offset) + 1).min(rope.len_chars());
                Selections::single(Selection::new(start, end))
            }
        };
    }
```

Add `word_range` to the `crate::motion` import, and `ClickMode` to `lib.rs`'s `pub use editor::{...}`.

- [ ] **Step 4: Run.** `cargo test -p editor-core` → all pass; clippy clean.

- [ ] **Step 5: Commit.**

```bash
git add crates/editor-core
git commit -m "feat(editor-core): add click selection modes and versioned save marking"
```

---

### Task 2: Theme editor tokens

**Files:**
- Modify: `crates/ui/src/theme.rs` (new `EditorTokens`, a field on `Theme` and `RawTheme`, `from_raw`, `scale_ui`)
- Modify all six: `assets/themes/{apple-light,dracula,nord,onyx-dark,solarized-dark,solarized-light}.toml`
- Test: `crates/ui/src/tests.rs`

**Interfaces:**
- Produces: `Theme.editor: EditorTokens` with:
  - colours (`Hsla`): `gutter_text`, `gutter_text_active`, `current_line`, `selection`, `cursor`
  - sizes (`f32`): `gutter_padding_px`, `cursor_width_px`

- [ ] **Step 1: Write the failing test** (`crates/ui/src/tests.rs`, next to the existing theme tests; reuse their way of iterating `ThemeKind`):

```rust
#[test]
fn every_builtin_theme_has_editor_tokens() {
    for kind in ThemeKind::builtin() {
        let theme = Theme::load(kind).expect("theme loads");
        assert!(theme.editor.cursor_width_px > 0.0);
        assert!(theme.editor.gutter_padding_px > 0.0);
        assert!(theme.editor.selection.a > 0.0);
    }
}
```

If `ThemeKind` has no list of built-in kinds, iterate the same way the existing "all themes load" test does, or list the six variants explicitly.

- [ ] **Step 2: Run and confirm failure.**

- [ ] **Step 3: Implement.** In `theme.rs`, following the `ChromeTokens` pattern:

```rust
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct EditorTokens {
    #[serde(deserialize_with = "color")]
    pub gutter_text: Hsla,
    #[serde(deserialize_with = "color")]
    pub gutter_text_active: Hsla,
    #[serde(deserialize_with = "color")]
    pub current_line: Hsla,
    #[serde(deserialize_with = "color")]
    pub selection: Hsla,
    #[serde(deserialize_with = "color")]
    pub cursor: Hsla,
    pub gutter_padding_px: f32,
    pub cursor_width_px: f32,
}
```

Add `pub editor: EditorTokens` to `Theme`, an `editor: EditorTokens` field to `RawTheme`, and pass it through in `from_raw`. Add `editor.gutter_padding_px` and `editor.cursor_width_px` to the `scale_ui` list. `ThemeKind::Custom` loads onyx-dark, so it inherits the section automatically.

In each theme TOML, add an `[editor]` section after `[pane_header]`:
- Dark themes (onyx-dark, dracula, nord, solarized-dark):
  - `gutter_text = "rgba(255,255,255,0.28)"`, `gutter_text_active = "rgba(255,255,255,0.72)"`
  - `current_line = "rgba(255,255,255,0.04)"`
  - `selection` = that theme's `accent.primary` at alpha 0.30, written as `rgba(...)`
  - `cursor` = that theme's `accent.primary` hex
- Light themes (apple-light, solarized-light): the same, using `rgba(0,0,0,...)` for the gutter and current-line values.
- All themes: `gutter_padding_px = 12.0` and `cursor_width_px = 2.0`.

Example for onyx-dark. Read its real `[accent] primary` value and substitute it; this example assumes `#0A84FF`:

```toml
[editor]
gutter_text = "rgba(255,255,255,0.28)"
gutter_text_active = "rgba(255,255,255,0.72)"
current_line = "rgba(255,255,255,0.04)"
selection = "rgba(10,132,255,0.30)"
cursor = "#0A84FF"
gutter_padding_px = 12.0
cursor_width_px = 2.0
```

- [ ] **Step 4: Run.** `cargo test -p ui` → passes, including every existing theme test.

- [ ] **Step 5: Commit.**

```bash
git add crates/ui/src/theme.rs crates/ui/src/tests.rs assets/themes
git commit -m "feat(theme): add editor gutter, selection and cursor tokens"
```

---

### Task 3: Document model: editor and file-tree panes, background load

**Files:**
- Modify: `crates/ui/Cargo.toml` (add `editor-core = { path = "../editor-core" }`)
- Modify: `crates/ui/src/document.rs` (enum, states, pure helpers, render for non-ready states)
- Create: `crates/ui/src/editor_view.rs` (only `EditorView` with scroll fields in this task; Task 4 extends it)
- Modify: `crates/ui/src/lib.rs` (`pub mod editor_view;`)
- Modify: `crates/ui/src/shell.rs` (`restore_document` takes `cx` and starts the load; `render_pane` passes through)
- Create: `crates/ui/src/shell/editor_io.rs` (`impl Shell`: `load_editor`, `finish_load`)

**Interfaces:**
- Consumes: `editor_core::{Editor, io::{load, LoadError}}`.
- Produces:
  - `DocumentPane::{Editor(EditorPane), FileTree(FileTreePane)}`, `DocumentPane::from_snapshot(&PanelSnapshot) -> Option<DocumentPane>`
  - `DocumentPane::is_dirty(&self) -> bool`, `DocumentPane::path(&self) -> &Path`
  - `EditorPane { path: PathBuf, state: EditorState, view: Rc<RefCell<EditorView>> }` with `ready(&self) -> Option<&Editor>` and `ready_mut(&mut self) -> Option<&mut Editor>`
  - `EditorState::{Loading, Ready(Editor), Failed(String)}`
  - `FileTreePane { root: PathBuf, exists: bool }`
  - `load_error_text(&LoadError) -> String`
  - `Shell::focused_editor_mut(&mut self) -> Option<(PanelId, &mut EditorPane)>`

- [ ] **Step 1: Write the failing tests** (`document.rs` tests):

```rust
    #[test]
    fn editor_pane_starts_loading_and_finishes_ready() {
        let record = editor_record();
        let Some(DocumentPane::Editor(mut pane)) = DocumentPane::from_snapshot(&record) else {
            panic!("expected an editor pane");
        };
        assert!(matches!(pane.state, EditorState::Loading));
        pane.state = EditorState::Ready(Editor::new(Buffer::from_text("x")));
        assert!(pane.ready().is_some());
        assert!(!DocumentPane::Editor(pane).is_dirty());
    }

    #[test]
    fn load_errors_read_as_plain_words() {
        let missing = LoadError::Io(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert_eq!(load_error_text(&missing), "not found");
        assert_eq!(load_error_text(&LoadError::Binary), "binary file");
        assert_eq!(load_error_text(&LoadError::NotUtf8), "not UTF-8 text");
    }
```

`editor_record()` already exists in this test module. Add `use editor_core::{io::LoadError, Buffer, Editor};` to it.

- [ ] **Step 2: Run and confirm failure.**

- [ ] **Step 3: Implement `document.rs`.** This replaces the `DocumentPane` struct; keep `document_cwd`, `explicit_title` and `document_title` unchanged.

```rust
pub enum DocumentPane {
    Editor(EditorPane),
    FileTree(FileTreePane),
}

pub struct EditorPane {
    pub path: PathBuf,
    pub state: EditorState,
    pub view: Rc<RefCell<EditorView>>,
}

pub enum EditorState {
    Loading,
    Ready(Editor),
    Failed(String),
}

pub struct FileTreePane {
    pub root: PathBuf,
    pub exists: bool,
}

impl DocumentPane {
    pub fn from_snapshot(record: &PanelSnapshot) -> Option<Self> {
        let path = record.path.clone()?;
        match record.panel.kind {
            PanelKind::Editor => Some(Self::Editor(EditorPane {
                path,
                state: EditorState::Loading,
                view: Rc::new(RefCell::new(EditorView::default())),
            })),
            PanelKind::FileTree => Some(Self::FileTree(FileTreePane { exists: path.is_dir(), root: path })),
            PanelKind::Terminal | PanelKind::Agent => None,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Editor(pane) => &pane.path,
            Self::FileTree(pane) => &pane.root,
        }
    }

    pub fn is_dirty(&self) -> bool {
        matches!(self, Self::Editor(pane) if pane.ready().is_some_and(|editor| editor.buffer().is_dirty()))
    }
}

impl EditorPane {
    pub fn ready(&self) -> Option<&Editor> {
        match &self.state {
            EditorState::Ready(editor) => Some(editor),
            EditorState::Loading | EditorState::Failed(_) => None,
        }
    }

    pub fn ready_mut(&mut self) -> Option<&mut Editor> {
        match &mut self.state {
            EditorState::Ready(editor) => Some(editor),
            EditorState::Loading | EditorState::Failed(_) => None,
        }
    }
}

pub fn load_error_text(error: &LoadError) -> String {
    match error {
        LoadError::Io(err) if err.kind() == std::io::ErrorKind::NotFound => NOT_FOUND.to_string(),
        other => other.to_string(),
    }
}
```

Use `const NOT_FOUND: &str = "not found";`, and change `NOT_FOUND_SUFFIX` to `format!("{name} {NOT_FOUND}")` style so there is one source for the words. `FileTreePane.exists` is the only stat, done once at restore or open. Remove `pane.path.exists()` from render.

`render_document(pane, theme)` now handles only the non-ready states. Task 4 sends `Ready` to the canvas.
- Editor `Loading`: an empty `div().size_full()`.
- Editor `Failed(message)`: the existing centred label, reading `"{name} — {message}"`.
- `FileTree`: the existing centred label with the name, or `"{name} not found"` when `!exists`.
- Editor `Ready`: in this task, the centred name. Task 4 replaces this.

`editor_view.rs` (initial):

```rust
#[derive(Debug, Default)]
pub struct EditorView {
    pub scroll_top: f32,
    pub scroll_left: f32,
}
```

`shell/editor_io.rs`:

```rust
use super::Shell;
use crate::document::{load_error_text, DocumentPane, EditorState};
use domain::PanelId;
use editor_core::{io, Buffer, Editor};
use gpui::Context;
use std::path::PathBuf;

impl Shell {
    pub(super) fn load_editor(&mut self, panel: PanelId, path: PathBuf, cx: &mut Context<Self>) {
        let task = cx.background_executor().spawn(async move { io::load(&path) });
        cx.spawn(async move |shell, cx| {
            let loaded = task.await;
            let updated = shell.update(cx, |shell, cx| {
                shell.finish_load(panel, loaded);
                cx.notify();
            });
            if let Err(err) = updated {
                tracing::debug!(?err, "shell dropped before load finished");
            }
        })
        .detach();
    }

    fn finish_load(&mut self, panel: PanelId, loaded: Result<Buffer, io::LoadError>) {
        let Some(DocumentPane::Editor(pane)) = self.documents.get_mut(&panel) else {
            return;
        };
        pane.state = match loaded {
            Ok(buffer) => EditorState::Ready(Editor::new(buffer)),
            Err(err) => EditorState::Failed(load_error_text(&err)),
        };
    }

    pub(super) fn focused_editor_mut(&mut self) -> Option<(PanelId, &mut crate::document::EditorPane)> {
        let panel = self.model.focused_document()?;
        match self.documents.get_mut(&panel)? {
            DocumentPane::Editor(pane) => Some((panel, pane)),
            DocumentPane::FileTree(_) => None,
        }
    }
}
```

Register `mod editor_io;` in `shell.rs`. `restore_document(panel, cx)` inserts the pane, and when it is `DocumentPane::Editor`, calls `self.load_editor(panel, path, cx)`. Update both callers (`show_active_workspace`, `open_document`), which both already have `cx`.

- [ ] **Step 4: Run.** `cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings` → green. Run `cargo run -p app`, Cmd-E, open a file through the palette (Open File…): it shows its name, and a binary file shows "— binary file".

- [ ] **Step 5: Commit.**

```bash
git add crates/ui
git commit -m "feat(editor): load editor documents in the background"
```

---

### Task 4: Editor painter

**Files:**
- Modify: `crates/ui/src/editor_view.rs` (style, layout cache, pure helpers, `editor_canvas`)
- Modify: `crates/ui/src/document.rs` (`Ready` goes to the canvas through a new `render_editor` call in shell)
- Modify: `crates/ui/src/shell.rs` (`render_pane` builds the editor canvas for a ready editor)

**Interfaces:**
- Consumes: `Theme.editor`, `TerminalStyle` (font family, size, line height), `ContentInsets` / `content_bounds`, `PaneFocus`, `editor_core::{Rope, Selections, display_column, TAB_WIDTH, line_len}`.
- Produces:
  - `EditorStyle::from_theme(&Theme, font_family: SharedString) -> EditorStyle`
  - `EditorFrame { rope: Rope, selections: Selections, is_focused: bool, marked_text: Option<String> }`
  - `editor_canvas(frame: EditorFrame, style: EditorStyle, view: Rc<RefCell<EditorView>>, focus: Option<PaneFocus>, on_pointer: PointerHandler) -> impl IntoElement`
  - `EditorView` fields: `scroll_top: f32`, `scroll_left: f32`, `layout: Option<FrameLayout>`, `is_selecting: bool`
  - `FrameLayout { text_origin: Point<Pixels>, line_height: Pixels, first_line: usize, lines: Vec<LaidLine>, viewport: Size<Pixels>, cursor_bounds: Option<Bounds<Pixels>> }`
  - `LaidLine { line: usize, start_char: usize, shaped: ShapedLine, display: DisplayLine }`
  - `FrameLayout::visible_line_count(&self) -> usize`
  - Pure helpers: `display_line(&str) -> DisplayLine`, `DisplayLine { text: String, byte_of_char: Vec<usize> }`, `DisplayLine::char_at_byte(&self, usize) -> usize`, `visible_range(scroll_top, viewport_height, line_height, line_count) -> Range<usize>`, `max_scroll_top(line_count, line_height, viewport_height) -> f32`, `scroll_to_reveal(scroll_top, line, line_height, viewport_height) -> f32`, `gutter_columns(line_count) -> usize`
  - `PointerHandler = Rc<dyn Fn(PointerEvent, &mut Window, &mut App)>`, `PointerEvent::{Down { offset: usize, click_count: usize, shift: bool, alt: bool }, Drag { offset: usize }, Up, Scroll { dx: f32, dy: f32 }}`. Task 6 wires the handler; this task defines the type and passes a no-op handler from shell.

- [ ] **Step 1: Write the failing tests** (`editor_view.rs` tests):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_line_maps_chars_to_bytes_through_tabs_and_emoji() {
        let line = display_line("\ta\u{1F600}b\n");
        assert_eq!(line.text, "    a\u{1F600}b");
        assert_eq!(line.byte_of_char, vec![0, 4, 5, 9, 10]);
        assert_eq!(line.char_at_byte(9), 3);
        assert_eq!(line.char_at_byte(2), 0);
    }

    #[test]
    fn tab_after_text_expands_to_the_next_stop() {
        assert_eq!(display_line("ab\tc").text, "ab  c");
    }

    #[test]
    fn visible_range_covers_partial_lines_and_clamps() {
        assert_eq!(visible_range(0.0, 100.0, 20.0, 1000), 0..6);
        assert_eq!(visible_range(30.0, 100.0, 20.0, 1000), 1..7);
        assert_eq!(visible_range(0.0, 100.0, 20.0, 3), 0..3);
    }

    #[test]
    fn scrolling_clamps_and_reveals_the_cursor_with_margin() {
        assert_eq!(max_scroll_top(100, 20.0, 200.0), 1980.0);
        assert_eq!(max_scroll_top(1, 20.0, 200.0), 0.0);
        assert_eq!(scroll_to_reveal(0.0, 2, 20.0, 200.0), 0.0);
        assert_eq!(scroll_to_reveal(0.0, 20, 20.0, 200.0), 280.0);
        assert_eq!(scroll_to_reveal(400.0, 19, 20.0, 200.0), 320.0);
    }

    #[test]
    fn gutter_fits_the_widest_line_number() {
        assert_eq!(gutter_columns(9), 2);
        assert_eq!(gutter_columns(12345), 5);
    }
}
```

Expected values:
- `scroll_to_reveal` keeps the cursor line at least `REVEAL_MARGIN_LINES = 3` lines inside the viewport.
- Line 20 at line height 20 means its top is at 400 and its bottom at 420. With a 3-line margin, the bottom limit is `420 + 60 - 200 = 280`.
- Line 19 when scrolled to 400: its top is 380, minus 60 is 320.
- `max_scroll_top` lets the last line scroll to the top: `(100 - 1) * 20 = 1980`.
- `gutter_columns` is at least `MIN_GUTTER_COLUMNS = 2`.

- [ ] **Step 2: Run and confirm failure.**

- [ ] **Step 3: Implement the pure helpers:**

```rust
const REVEAL_MARGIN_LINES: f32 = 3.0;
const MIN_GUTTER_COLUMNS: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayLine {
    pub text: String,
    pub byte_of_char: Vec<usize>,
}

impl DisplayLine {
    pub fn char_at_byte(&self, byte: usize) -> usize {
        self.byte_of_char.iter().rposition(|start| *start <= byte).unwrap_or(0)
    }
}

pub fn display_line(line: &str) -> DisplayLine {
    let mut text = String::with_capacity(line.len());
    let mut byte_of_char = Vec::with_capacity(line.len() + 1);
    let mut column = 0;
    for ch in line.chars().take_while(|ch| *ch != '\n') {
        byte_of_char.push(text.len());
        if ch == '\t' {
            let width = TAB_WIDTH - column % TAB_WIDTH;
            text.extend(std::iter::repeat_n(' ', width));
            column += width;
        } else {
            text.push(ch);
            column += 1;
        }
    }
    byte_of_char.push(text.len());
    DisplayLine { text, byte_of_char }
}

pub fn visible_range(scroll_top: f32, viewport_height: f32, line_height: f32, line_count: usize) -> Range<usize> {
    let first = (scroll_top / line_height).floor().max(0.0) as usize;
    let last = ((scroll_top + viewport_height) / line_height).ceil() as usize;
    first.min(line_count)..last.min(line_count)
}

pub fn max_scroll_top(line_count: usize, line_height: f32, viewport_height: f32) -> f32 {
    let _ = viewport_height;
    line_count.saturating_sub(1) as f32 * line_height
}

pub fn scroll_to_reveal(scroll_top: f32, line: usize, line_height: f32, viewport_height: f32) -> f32 {
    let margin = REVEAL_MARGIN_LINES * line_height;
    let top = line as f32 * line_height;
    let lowest = top + line_height + margin - viewport_height;
    let highest = top - margin;
    scroll_top.max(lowest).min(highest.max(0.0)).max(0.0)
}

pub fn gutter_columns(line_count: usize) -> usize {
    line_count.max(1).to_string().len().max(MIN_GUTTER_COLUMNS)
}
```

`std::iter::repeat_n` was stabilised in 1.82, so it's fine on 1.85. `max_scroll_top` ignores the viewport height on purpose. Drop the parameter and update the test if clippy objects; don't keep `let _`.

Check the hand-computed reveal values:
- `(0.0, 20, ..)`: highest = 400 − 60 = 340, lowest = 280. `0.max(280).min(340) = 280`. ✓
- `(400.0, 19, ..)`: highest = 320, lowest = 260. `400.max(260).min(320) = 320`. ✓
- `(0.0, 2, ..)`: highest = 40 − 60 → clamped to 0, lowest = −80. Result 0. ✓

- [ ] **Step 4: Implement the canvas.** Mirror `terminal_canvas` (`terminal_view.rs:213`). The steps:
1. **Prepaint** `(bounds, window, cx)`:
   - Insert a hitbox and compute `content = content_bounds(bounds, style.insets)`.
   - Cell width is the `'m'` advance, the same as `TerminalStyle::cell_size`.
   - `gutter_width = gutter_columns(line_count) * cell_width + 2 * gutter_padding`.
   - The text area is `content` minus the gutter on the left.
   - Clamp `view.scroll_top` to `[0, max_scroll_top]` and `scroll_left` to `>= 0`.
   - For each line in `visible_range`:
     - `display_line(&rope.line(i).to_string())`
     - shape it with `window.text_system().shape_line(display.text.into(), font_size, &[one TextRun of the whole line in theme.text.default], None)`
     - record a `LaidLine` with `start_char = rope.line_to_char(i)`.
   - `text_origin = text_area.origin + point(-scroll_left, -(scroll_top % line_height))`.
   - Compute `cursor_bounds` for the newest selection's head when its line is visible.
   - Store `FrameLayout` in `view.layout` and return `(hitbox, gutter rect, text rect)` for paint.
2. **Paint**, under `window.with_content_mask(Some(ContentMask { bounds: text_rect }))` for the text layers:
   - If focused, the current-line quad (`theme.editor.current_line`) for every line that holds an empty selection's head, spanning the full text-area width.
   - Selection quads per visible line. Byte x comes from `laid.shaped.x_for_index(laid.display.byte_of_char[col])`. When a selection continues past the line end, add one `cell_width` for the newline.
   - Text: `laid.shaped.paint(origin_of_line, line_height, window, cx)`, logging any error with `tracing::warn!` like the terminal does.
   - Cursors when focused: `fill(Bounds::new(point(x, y), size(px(cursor_width_px), line_height)), theme.editor.cursor)` for every selection head.
   - Marked text when `frame.marked_text` is `Some`: shape it underlined, as in `FramePainter::paint_marked_text`, paint a background quad over it, and paint it at the newest cursor.
3. **Gutter**, outside the text mask: for each laid line, shape `(line + 1).to_string()` and paint it right-aligned at `gutter_right - padding - width`. Use `gutter_text_active` for lines holding a selection head, `gutter_text` otherwise.
4. If `focus` is `Some`, call `(focus.register_input)(content, window, cx)`.
5. Register the mouse handlers (Task 6). In this task, register nothing and accept the `on_pointer` handler unused. To avoid a dead-code failure under `deny(warnings)`, hold `on_pointer` in the closure and wire it in Task 6, or do Tasks 4 and 6 back to back and commit them together.

`EditorStyle`:

```rust
#[derive(Clone)]
pub struct EditorStyle {
    pub font_family: SharedString,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub text: Hsla,
    pub tokens: EditorTokens,
    pub insets: ContentInsets,
}

impl EditorStyle {
    pub fn from_theme(theme: &Theme, font_family: SharedString) -> Self {
        Self {
            font_family,
            font_size: px(theme.font.size_px),
            line_height: px((theme.font.size_px * theme.font.line_height).round()),
            text: theme.text.default,
            tokens: theme.editor,
            insets: ContentInsets::from_theme(theme),
        }
    }
}
```

In `shell.rs` `render_pane`, when the document is a ready editor, build the content:

```rust
let frame = EditorFrame {
    rope: editor.buffer().rope().clone(),
    selections: editor.selections().clone(),
    is_focused,
    marked_text: is_focused.then(|| self.marked_text.clone()).flatten(),
};
editor_canvas(frame, EditorStyle::from_theme(&self.state.theme, self.style.font_family.clone()),
    pane.view.clone(), is_focused.then(|| self.pane_focus(cx)), self.editor_pointer_handler(panel, cx))
    .into_any_element()
```

Task 6 implements `editor_pointer_handler`. In this task it returns `Rc::new(|_, _, _| {})`. `Rope::clone` is O(1).

- [ ] **Step 5: Run.** The pure tests pass and the workspace is green. Manual check with `cargo run -p app`: open this repo's `Cargo.toml` through Open File…. The text renders in the terminal font, line numbers are on the left, and the cursor sits at 1:1.

- [ ] **Step 6: Commit** (or hold for Task 6 if `on_pointer` would be dead code):

```bash
git add crates/ui
git commit -m "feat(editor): paint visible lines with gutter, selections and cursors"
```

---

### Task 5: Keyboard, clipboard and save

**Files:**
- Create: `crates/ui/src/shell/editor_input.rs` (`impl Shell`: `send_key_to_document`, `apply_editor_action`, `reveal_cursor`, `copy_from_editor`, `paste_into_editor`, `save_focused_editor`, `finish_save`)
- Create: `crates/ui/src/editor_keys.rs` (pure: `key_input(&Keystroke) -> KeyInput<'_>`, `should_defer_to_input_handler(&Keystroke, is_held: bool) -> bool`)
- Modify: `crates/ui/src/shell.rs` (`consume_key` routes to `send_key_to_document`; `run_command` adds the editor arms for Copy, Paste, SelectAll and Save)
- Modify: `crates/ui/src/command.rs` (`CommandId::Save`, "Save", category "Files")
- Modify: `crates/ui/src/keymap.rs` (Cmd-S → `Save`)
- Modify: `crates/ui/src/menus.rs` (Save in the Files menu, after Open Editor Here)
- Test: `crates/ui/src/editor_keys.rs`, `crates/ui/src/tests.rs`

**Interfaces:**
- Consumes: `standard_action`, `KeyInput`, `EditorAction::SelectAll`, `Editor::{apply, copy_text, paste, mark_saved_at}`, `io::save`, `scroll_to_reveal`, `FrameLayout::visible_line_count`.
- Produces: `CommandId::Save`; key routing for focused editors.

- [ ] **Step 1: Write the failing tests.** In `editor_keys.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Keystroke, Modifiers};

    fn stroke(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke { modifiers, key: key.into(), key_char: key_char.map(str::to_string) }
    }

    #[test]
    fn composed_text_is_left_to_the_input_handler() {
        assert!(should_defer_to_input_handler(&stroke("a", Some("a"), Modifiers::default()), false));
        assert!(!should_defer_to_input_handler(&stroke("a", Some("a"), Modifiers::default()), true));
        assert!(!should_defer_to_input_handler(&stroke("left", None, Modifiers::default()), false));
        assert!(!should_defer_to_input_handler(&stroke("z", Some("z"), Modifiers::command()), false));
    }

    #[test]
    fn key_input_carries_modifiers_and_text() {
        let keystroke = stroke("left", None, Modifiers { alt: true, shift: true, ..Modifiers::default() });
        let input = key_input(&keystroke);
        assert_eq!(input.key, "left");
        assert!(input.alt && input.shift && !input.cmd && !input.ctrl);
    }
}
```

If `Keystroke` has more fields in gpui 0.2.2, fill them with their defaults, using `..Default::default()` if it implements `Default`. Check `gpui-0.2.2/src/platform/keystroke.rs`.

In `tests.rs`, next to `cmd_e_opens_editor_here`:

```rust
#[test]
fn cmd_s_saves() {
    let keymap = Keymap::default();
    assert_eq!(keymap.resolve(&Combo::new(Modifiers::command(), "s")), Some(CommandId::Save));
}
```

- [ ] **Step 2: Run and confirm failure.**

- [ ] **Step 3: Implement `editor_keys.rs`:**

```rust
use crate::input::is_composed_text;
use editor_core::KeyInput;
use gpui::Keystroke;

pub fn key_input(keystroke: &Keystroke) -> KeyInput<'_> {
    KeyInput {
        key: &keystroke.key,
        text: keystroke.key_char.as_deref(),
        cmd: keystroke.modifiers.platform,
        alt: keystroke.modifiers.alt,
        shift: keystroke.modifiers.shift,
        ctrl: keystroke.modifiers.control,
    }
}

pub fn should_defer_to_input_handler(keystroke: &Keystroke, is_held: bool) -> bool {
    is_composed_text(keystroke, false) && !is_held
}
```

Register `mod editor_keys;` in `lib.rs` (private is fine; tests are inline).

`shell/editor_input.rs`:

```rust
use super::Shell;
use crate::editor_keys::{key_input, should_defer_to_input_handler};
use crate::editor_view::scroll_to_reveal;
use domain::PanelId;
use editor_core::{io, standard_action, EditorAction};
use gpui::{ClipboardItem, Context, KeyDownEvent};
use std::time::Instant;

const DEFAULT_PAGE_LINES: usize = 30;

impl Shell {
    pub(super) fn send_key_to_document(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if should_defer_to_input_handler(&event.keystroke, event.is_held) {
            return false;
        }
        let Some((panel, pane)) = self.focused_editor_mut() else {
            return false;
        };
        let page_lines = pane.view.borrow().layout.as_ref().map_or(DEFAULT_PAGE_LINES, |layout| layout.visible_line_count());
        let Some(action) = standard_action(&key_input(&event.keystroke), page_lines) else {
            return false;
        };
        self.apply_editor_action(panel, action, cx);
        true
    }

    pub(super) fn apply_editor_action(&mut self, panel: PanelId, action: EditorAction, cx: &mut Context<Self>) {
        let Some((focused, pane)) = self.focused_editor_mut() else {
            return;
        };
        if focused != panel {
            return;
        }
        let Some(editor) = pane.ready_mut() else {
            return;
        };
        if let Some(text) = editor.apply(action, Instant::now()) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        self.reveal_cursor(panel);
        cx.notify();
    }
}
```

The held-key case: when `event.is_held` and the keystroke is composed text, `standard_action` receives `text = key_char` and returns `InsertText`. That matches the terminal's `repeat_text` behaviour.

`reveal_cursor(panel)` reads the newest head's line (`rope.char_to_line(head)`) and the last layout's viewport height and line height, then sets `view.scroll_top = scroll_to_reveal(...)`. If there is no layout yet, it does nothing. Horizontal reveal: if the head's x, taken from the laid line when visible, falls outside `[scroll_left, scroll_left + text_width]`, shift `scroll_left` so it's inside, with one cell width of margin.

`copy_from_editor`, `paste_into_editor` and select-all call `editor.copy_text()` → `cx.write_to_clipboard`, `cx.read_from_clipboard().and_then(|item| item.text())` → `editor.paste(&text, Instant::now())`, and `apply_editor_action(panel, EditorAction::SelectAll, cx)`.

Save:

```rust
    pub(super) fn save_focused_editor(&mut self, cx: &mut Context<Self>) {
        let Some((panel, pane)) = self.focused_editor_mut() else {
            return;
        };
        let path = pane.path.clone();
        let Some(buffer) = pane.ready().map(|editor| editor.buffer().clone()) else {
            return;
        };
        let version = buffer.version();
        let task = cx.background_executor().spawn(async move { io::save(&path, &buffer) });
        cx.spawn(async move |shell, cx| {
            let result = task.await;
            let updated = shell.update(cx, |shell, cx| shell.finish_save(panel, version, result, cx));
            if let Err(err) = updated {
                tracing::debug!(?err, "shell dropped before save finished");
            }
        })
        .detach();
    }

    fn finish_save(&mut self, panel: PanelId, version: u64, result: std::io::Result<()>, cx: &mut Context<Self>) {
        let Some(crate::document::DocumentPane::Editor(pane)) = self.documents.get_mut(&panel) else {
            return;
        };
        match result {
            Ok(()) => {
                if let Some(editor) = pane.ready_mut() {
                    editor.mark_saved_at(version);
                }
            }
            Err(err) => {
                let name = crate::document::document_title(&pane.path);
                tracing::warn!(%err, "save failed");
                self.notice = Some(format!("{SAVE_FAILED_PREFIX} {name}: {err}"));
            }
        }
        cx.notify();
    }
```

Use `const SAVE_FAILED_PREFIX: &str = "Couldn't save";`.

`shell.rs`:
- In `consume_key`, replace `if self.model.focused_document().is_some() { return false; }` with `if self.model.focused_document().is_some() { return self.send_key_to_document(event, cx); }`.
- In `run_command`, add these arms after the field arm and before the terminal arms. Use whatever `self.focused_editor_mut().is_some()` guard compiles against the borrow checker; a small `has_focused_editor(&self) -> bool` helper in `editor_io.rs` is fine.

```rust
CommandId::Copy if self.has_focused_editor() => self.copy_from_editor(cx),
CommandId::Paste if self.has_focused_editor() => self.paste_into_editor(cx),
CommandId::SelectAll if self.has_focused_editor() => self.select_all_in_editor(cx),
CommandId::Save => self.save_focused_editor(cx),
```

`command.rs`: add `Save` to `CommandId` and `Command::builder(Save, "Save", "Files", no_op).build()`. `keymap.rs`: `map.bind(Combo::new(cmd, "s"), Save);`, after checking that Cmd-S is free (it is today).

- [ ] **Step 4: Run.** Tests green. Manual check: open a file, then type, arrow, option-arrow, cmd-arrow, shift-select, Backspace, Enter keeping indent, Cmd-Z, Cmd-Shift-Z, Cmd-C/X/V, Cmd-A, and Cmd-S writes the file (check with `git diff`).

- [ ] **Step 5: Commit.**

```bash
git add crates/ui
git commit -m "feat(editor): route keys, clipboard and cmd-s save to the focused editor"
```

---

### Task 6: IME, mouse and scrolling

**Files:**
- Modify: `crates/ui/src/shell/text_input.rs` (an editor branch in `replace_text_in_range`, `replace_and_mark_text_in_range` and `bounds_for_range`)
- Modify: `crates/ui/src/editor_view.rs` (register mouse handlers, plus the pure `offset_at`)
- Modify: `crates/ui/src/shell/editor_input.rs` (`editor_pointer_handler`, `handle_editor_pointer`)
- Test: `crates/ui/src/editor_view.rs`

**Interfaces:**
- Consumes: `FrameLayout`, `PointerEvent`, `Editor::click`, `ClickMode`.
- Produces:
  - `offset_at(layout: &FrameLayout, position: Point<Pixels>) -> usize`
  - `line_and_x_at(layout_geometry: (text_origin_y: f32, line_height: f32, first_line: usize, line_count: usize), y: f32) -> usize`, the pure part, tested.

- [ ] **Step 1: Write the failing test** for the pure line lookup:

```rust
    #[test]
    fn y_positions_map_to_lines_and_clamp_to_the_buffer() {
        assert_eq!(line_at_y(100.0, 20.0, 5, 50, 105.0), 5);
        assert_eq!(line_at_y(100.0, 20.0, 5, 50, 141.0), 7);
        assert_eq!(line_at_y(100.0, 20.0, 5, 50, 10.0), 5);
        assert_eq!(line_at_y(100.0, 20.0, 5, 8, 400.0), 7);
    }
```

The signature is `line_at_y(origin_y, line_height, first_line, line_count, y) -> usize`. Clicks above the first visible line map to it, and clicks below the buffer map to the last line.

- [ ] **Step 2: Run and confirm failure.**

- [ ] **Step 3: Implement.**

```rust
pub fn line_at_y(origin_y: f32, line_height: f32, first_line: usize, line_count: usize, y: f32) -> usize {
    let row = ((y - origin_y) / line_height).floor().max(0.0) as usize;
    (first_line + row).min(line_count.saturating_sub(1))
}
```

`offset_at(layout, rope, position)`:
1. Compute the line with `line_at_y`.
2. If that line is laid out, take `laid.shaped.closest_index_for_x(position.x - layout.text_origin.x)` → byte → `laid.display.char_at_byte(byte)`, then add `laid.start_char` and clamp to the line length.
3. If not laid out (dragged above or below the viewport), use the line start or end.

In the canvas paint closure, register `window.on_mouse_event` handlers exactly like `register_mouse_handlers` in `terminal_view.rs:272-311`, with the same phase and hitbox checks:
- **Mouse down, left button:**
  - Call `on_pointer(PointerEvent::Down { offset, click_count: event.click_count, shift: event.modifiers.shift, alt: event.modifiers.alt })`.
  - Set `view.is_selecting = true`.
- **Mouse move with left pressed while selecting:** call `on_pointer(PointerEvent::Drag { offset })`.
- **Mouse up:** clear `is_selecting`, then call `on_pointer(PointerEvent::Up)`.
- **Scroll wheel:** call `on_pointer(PointerEvent::Scroll { dx, dy })`, taking dx and dy from `event.delta.pixel_delta(line_height)`.

`editor_pointer_handler(panel, cx)` returns `Rc::new(move |event, window, cx| { shell.update(cx, |shell, cx| shell.handle_editor_pointer(panel, event, cx)); window.refresh(); })`, where `shell = cx.entity()`; check how `input_registration` obtains it.

`handle_editor_pointer`:
- **Down:** `click_count` 1 gives `ClickMode::Extend` if shift, `AddCursor` if alt, else `Place`; 2 gives `Word`; 3 or more gives `Line`.
- **Drag:** `ClickMode::Extend`.
- **Scroll:**
  - `view.scroll_top = (view.scroll_top - dy).clamp(0.0, max_scroll_top(...))`
  - `view.scroll_left = (view.scroll_left - dx).max(0.0)`
- After a Down or Drag, call `cx.notify()`. Don't reveal the cursor on mouse actions, so the view doesn't jump under the pointer.

IME (`text_input.rs`): after each `with_active_field` early return and before the terminal fallback, add the editor branch:
- `replace_text_in_range`:

  ```rust
  if let Some(panel) = self.model.focused_document() {
      self.marked_text = None;
      if !text.is_empty() {
          self.apply_editor_action(panel, EditorAction::InsertText(text.to_string()), cx);
      }
      cx.notify();
      return;
  }
  ```

- `replace_and_mark_text_in_range`: keep the existing fallback (`self.marked_text = Some(...)`). The editor canvas already paints `frame.marked_text`, so no change is needed. Confirm it reaches the frame.
- `bounds_for_range`: if a document is focused, return the focused editor's `view.layout.cursor_bounds`.

- [ ] **Step 4: Run.** Tests green. Manual check:
  - Click places the cursor; shift-click extends; option-click adds a cursor; double-click selects a word; triple-click selects a line; dragging selects.
  - The trackpad scrolls smoothly both ways.
  - Option-e then e types é once.
  - Typing with a Japanese IME shows the underlined composition at the cursor.

- [ ] **Step 5: Commit.**

```bash
git add crates/ui
git commit -m "feat(editor): add mouse selection, scrolling and ime input"
```

---

### Task 7: Unsaved-changes protection and dirty dot

**Files:**
- Modify: `crates/ui/src/confirm.rs` (new variants and copy)
- Modify: `crates/ui/src/shell.rs`:
  - `request_close_focused`, `request_close_tab`
  - `accept_confirmation`, `confirmation_copy`
  - `request_quit`, `live_session_count` callers
- Modify: `crates/ui/src/shell/pane_header.rs` (`status_dot`)
- Test: `crates/ui/src/tests.rs` (find the existing confirm-copy tests and add next to them)

**Interfaces:**
- Produces:
  - `Confirmation::DiscardPane(PanelId)`, `Confirmation::DiscardTab(TabId)`, `Confirmation::Quit { sessions: usize, unsaved: usize }`
  - `discard_pane_copy(name: &str) -> ConfirmCopy`, `discard_tab_copy(unsaved: usize) -> ConfirmCopy`
  - `quit_copy(sessions: usize, unsaved: usize) -> ConfirmCopy`, `delete_workspace_copy(name, sessions, unsaved) -> ConfirmCopy`
  - `Shell::unsaved_count(&self, panels: impl Iterator<Item = PanelId>) -> usize`

- [ ] **Step 1: Write the failing tests:**

```rust
#[test]
fn discard_copy_names_the_file_and_is_destructive() {
    let copy = discard_pane_copy("main.rs");
    assert!(copy.body.contains("main.rs"));
    assert!(copy.is_destructive);
    assert_eq!(copy.action, "Discard");
}

#[test]
fn quit_copy_mentions_unsaved_files() {
    assert!(quit_copy(0, 2).body.contains("2 unsaved files"));
    assert!(quit_copy(1, 1).body.contains("1 unsaved file"));
    assert!(!quit_copy(1, 0).body.contains("unsaved"));
}

#[test]
fn delete_workspace_copy_mentions_unsaved_files() {
    assert!(delete_workspace_copy("Code", 0, 3).body.contains("3 unsaved files"));
}
```

- [ ] **Step 2: Run and confirm failure.**

- [ ] **Step 3: Implement.** Copy, in the tone of the existing copy functions:

```rust
pub fn discard_pane_copy(name: &str) -> ConfirmCopy {
    ConfirmCopy {
        title: "Discard unsaved changes?".to_string(),
        body: format!("{name} has changes that haven't been saved."),
        action: "Discard",
        is_destructive: true,
    }
}

pub fn discard_tab_copy(unsaved: usize) -> ConfirmCopy {
    ConfirmCopy {
        title: "Discard unsaved changes?".to_string(),
        body: format!("This tab has {}.", unsaved_files(unsaved)),
        action: "Close tab",
        is_destructive: true,
    }
}

fn unsaved_files(count: usize) -> String {
    match count {
        1 => "1 unsaved file".to_string(),
        n => format!("{n} unsaved files"),
    }
}
```

`quit_copy(sessions, unsaved)` and `delete_workspace_copy(name, sessions, unsaved)` keep their current text and append `" and {unsaved_files(n)}"`, or a sentence, whenever `unsaved > 0`. When `sessions == 0 && unsaved > 0`, the body names only the files. Update every caller.

Shell:
- `unsaved_count(panels)` counts the panels whose `self.documents.get(..).is_some_and(DocumentPane::is_dirty)`.
- **`request_close_focused`:** if the focused document is dirty, set `pending = Some(Confirmation::DiscardPane(panel))` and return. Otherwise run the existing busy-terminal logic.
- **`request_close_tab`:** if `unsaved_count(leaves) > 0`, set `pending = Some(Confirmation::DiscardTab(id))`. Otherwise run the existing logic.
- **`accept_confirmation`:** `DiscardPane(panel)` goes to `close_pane(panel, cx)`, and `DiscardTab(id)` to `close_tab(id, cx)`.
- **`confirmation_copy`:** `DiscardPane(panel)` uses `discard_pane_copy(&document_title(path))`, and `DiscardTab(id)` uses `discard_tab_copy(unsaved in that tab)`. `DeleteWorkspace` passes `unsaved_count(workspace panels)`.
- **`request_quit`:** compute `sessions = self.panes.len()` and `unsaved = unsaved_count(self.documents.keys().copied())`. If both are 0, quit; otherwise set `pending = Some(Confirmation::Quit { sessions, unsaved })`.
- **`close_pane`** already clears a matching `pending` for `ClosePane`. Extend it to clear `DiscardPane(panel)` too.

`pane_header.rs` `status_dot`: when `self.documents.get(&panel).is_some_and(DocumentPane::is_dirty)`, use `theme.accent.primary`; otherwise keep the existing busy and idle colours.

- [ ] **Step 4: Run.** Tests green. Manual check:
  - Edit a file and the header dot turns accent-coloured; Cmd-S turns it back.
  - With unsaved changes: Cmd-W asks first, Cmd-Shift-W asks first, and Cmd-Q lists the unsaved files.
  - With no unsaved changes, Cmd-W closes immediately.

- [ ] **Step 5: Commit.**

```bash
git add crates/ui
git commit -m "feat(editor): confirm before discarding unsaved changes and show dirty dot"
```

---

### Task 8: Manual verification (user)

- [ ] `cargo run --release -p app`. In a terminal, `cd` into a project and press Cmd-E. Then use Open File… on a source file.
- [ ] Generate a 10 MB file (`yes "fn example_function(argument: usize) -> usize { argument * 2 }" | head -c 10000000 > /tmp/big.rs`) and open it. The window must not freeze; type at the end and scroll fast.
- [ ] The editor follows a theme switch (Settings).
- [ ] Quit and relaunch: the editor pane restores and reloads the file.
