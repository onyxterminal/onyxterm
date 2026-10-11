# Editor Step 2a: Editor Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A pure-Rust crate, `crates/editor-core`, that holds a text document and edits it the way a modern editor does: multi-cursor, grapheme- and word-aware motions, typing grouped into undo steps, standard macOS editing keys, and atomic load/save. It has no GUI. Plan 2b puts a GPUI view on top of it.

**Architecture:**
- `Buffer` wraps a `ropey::Rope` and normalises line endings to `\n` internally; save restores CRLF.
- An `Edit` is a sorted set of `Change { range, text }` in char offsets, applied back to front. Applying it returns its inverse, which drives undo.
- `Selections` is a sorted, merged list of `Selection { anchor, head, goal_column }`.
- `Editor` ties these together and exposes one entry point, `apply(EditorAction, now) -> Option<String>`. The returned `String` is text for the clipboard, from Cut.
- `standard_action` maps a GUI-neutral `KeyInput` to an `EditorAction`.

**Tech Stack:** Rust 1.85, `ropey`, `unicode-segmentation`, `tempfile`, `thiserror`.

**Spec:** `docs/superpowers/specs/2026-10-11-code-editor-design.md` (Step 2 sections "Buffer", "Selections", "Edits", "Undo", "Loading", "Saving", "Standard keys"; Performance targets)

## Global Constraints

- No code comments of any kind, including doc comments.
- The crate root starts with `#![deny(warnings)]` and `#![forbid(unsafe_code)]`.
- `editor-core` must not depend on `gpui` or any other workspace crate.
- Dependencies are pinned exactly with `=`: `ropey = "=1.6.1"`, `unicode-segmentation = "=1.13.3"`, `tempfile = "=3.20.0"`, `thiserror = { workspace = true }`.
- Offsets are char indices into the rope, never byte indices.
- Magic numbers become named `const`s at the top of the file.
- Tests are inline `#[cfg(test)] mod tests` at the bottom of each module (they need crate-private access), except the perf test in `crates/editor-core/tests/perf.rs`.
- Commits: `TYPE(SCOPE): MESSAGE`, single line, no `Co-Authored-By` trailer, never `--no-verify`. Work on `main`.
- Verification for every task: `cargo build --workspace`, `cargo test -p editor-core`, `cargo clippy --workspace -- -D warnings`.
- Performance (spec): keystroke to painted frame under 8 ms p95 on a 10 MB file. The core's share is under 1 ms per edit and under 100 ms to load 10 MB, both in release.

## Spec deviations (decided while planning)

- Load and save live in `editor-core::io`, not in a separate `fs-service` crate. `fs-service` arrives in step 3, when the tree, watcher and index need it. Rationale: no crate exists only to hold two functions.
- External-change detection (the "changed on disk" badge) needs a watcher, so it moves to step 3 alongside the tree's `notify` watcher.
- Cmd-/ (toggle comment) needs a language's comment token, so it moves to step 4 (languages).
- The dirty-close confirm in 2b uses the existing two-choice dialog: Enter discards, Esc cancels, and Cmd-S saves beforehand. There is no three-button Save / Discard / Cancel dialog.

## Known ceilings (accepted)

- After an edit, undoing back to the saved text still shows the buffer as dirty, because the dirty check compares versions, not content. The upgrade path is to record the undo depth at save time.
- Mixed line endings in one file are unified to the file's majority style on save. The file counts as CRLF if any CRLF is present.
- Grapheme motion reads the current line as a `String`, so arrow keys on a single huge line (such as minified JSON) cost O(line length).

## Review Focus

1. **Two cursors deleting toward each other** (cursors at 1 and 2 in `"abc"`, Backspace) must not panic or corrupt the text. Overlapping changes are merged in `Edit::new`. Test: Task 5 `backspace_with_adjacent_cursors_merges_changes`.
2. **Emoji and combining marks.** Left, Right and Backspace move and delete a whole grapheme cluster, never half of one. Test: Task 4 `right_moves_over_a_whole_grapheme_cluster`; Task 5 `backspace_deletes_a_whole_grapheme_cluster`.
3. **Saving through a symlink** keeps the symlink and writes the target. A naive rename would replace the link with a plain file. Test: Task 7 `save_through_symlink_writes_the_target`.
4. **A save that fails must leave the original file intact,** with no stray temp files. Test: Task 7 `failed_save_keeps_original_and_leaves_no_temp_file`.
5. **Up and Down through tab-indented lines** keep the visual column, so the goal column is in display columns with `TAB_WIDTH` 4. Test: Task 4 `vertical_motion_keeps_display_goal_column_across_tabs`.

---

### Task 1: Crate scaffold and `Buffer`

**Files:**
- Modify: `Cargo.toml` (root): add `"crates/editor-core"` to `[workspace] members`
- Create: `crates/editor-core/Cargo.toml`
- Create: `crates/editor-core/src/lib.rs`
- Create: `crates/editor-core/src/buffer.rs`

**Interfaces:**
- Produces:
  - `editor_core::{Buffer, LineEnding, line_len}`
  - `Buffer::from_text(&str) -> Buffer`, `rope(&self) -> &Rope`
  - `version(&self) -> u64`, `is_dirty(&self) -> bool`, `mark_saved(&mut self)`
  - `line_ending(&self) -> LineEnding`, `len_chars(&self) -> usize`, `line_count(&self) -> usize`
  - `to_saved_text(&self) -> String`, and `pub(crate) replace(&mut self, Range<usize>, &str)`
  - `pub fn line_len(rope: &Rope, line: usize) -> usize`, the line's length in chars without its `\n`.

- [ ] **Step 1: Scaffold.** `crates/editor-core/Cargo.toml`:

```toml
[package]
name = "editor-core"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
ropey = "=1.6.1"
unicode-segmentation = "=1.13.3"
tempfile = "=3.20.0"
thiserror = { workspace = true }
```

`crates/editor-core/src/lib.rs`:

```rust
#![deny(warnings)]
#![forbid(unsafe_code)]

mod buffer;

pub use buffer::{line_len, Buffer, LineEnding};
pub use ropey::Rope;
```

- [ ] **Step 2: Write the failing tests.** Create `crates/editor-core/src/buffer.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_text_is_normalized_and_restored_on_save() {
        let buffer = Buffer::from_text("a\r\nb\r\n");
        assert_eq!(buffer.line_ending(), LineEnding::CrLf);
        assert_eq!(buffer.rope().to_string(), "a\nb\n");
        assert_eq!(buffer.to_saved_text(), "a\r\nb\r\n");
    }

    #[test]
    fn lf_text_stays_lf() {
        let buffer = Buffer::from_text("a\nb");
        assert_eq!(buffer.line_ending(), LineEnding::Lf);
        assert_eq!(buffer.to_saved_text(), "a\nb");
    }

    #[test]
    fn replace_marks_dirty_until_saved() {
        let mut buffer = Buffer::from_text("abc");
        assert!(!buffer.is_dirty());
        buffer.replace(1..2, "XY");
        assert_eq!(buffer.rope().to_string(), "aXYc");
        assert!(buffer.is_dirty());
        buffer.mark_saved();
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn line_len_excludes_the_newline() {
        let rope = Rope::from_str("ab\ncde");
        assert_eq!(line_len(&rope, 0), 2);
        assert_eq!(line_len(&rope, 1), 3);
    }
}
```

- [ ] **Step 3: Run and confirm failure.** `cargo test -p editor-core` → compile errors (`Buffer` not found).

- [ ] **Step 4: Implement** above the test module in `buffer.rs`:

```rust
use ropey::Rope;
use std::ops::Range;

const CRLF: &str = "\r\n";
const LF: &str = "\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

#[derive(Debug, Clone)]
pub struct Buffer {
    rope: Rope,
    version: u64,
    saved_version: u64,
    line_ending: LineEnding,
}

impl Buffer {
    pub fn from_text(text: &str) -> Self {
        let line_ending = if text.contains(CRLF) { LineEnding::CrLf } else { LineEnding::Lf };
        Self { rope: Rope::from_str(&text.replace(CRLF, LF)), version: 0, saved_version: 0, line_ending }
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn is_dirty(&self) -> bool {
        self.version != self.saved_version
    }

    pub fn mark_saved(&mut self) {
        self.saved_version = self.version;
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn to_saved_text(&self) -> String {
        let text = self.rope.to_string();
        match self.line_ending {
            LineEnding::Lf => text,
            LineEnding::CrLf => text.replace(LF, CRLF),
        }
    }

    pub(crate) fn replace(&mut self, range: Range<usize>, text: &str) {
        self.rope.remove(range.clone());
        self.rope.insert(range.start, text);
        self.version += 1;
    }
}

pub fn line_len(rope: &Rope, line: usize) -> usize {
    let slice = rope.line(line);
    let len = slice.len_chars();
    if len > 0 && slice.char(len - 1) == '\n' {
        len - 1
    } else {
        len
    }
}
```

If `replace` is unused outside tests at this point and `deny(warnings)` fails on dead code, mark it `#[cfg_attr(not(test), allow(dead_code))]`. Don't do that: Task 2 uses it, so commit Tasks 1 and 2 together instead (see Task 2, Step 5).

- [ ] **Step 5: Run.** `cargo test -p editor-core` → 4 passed. Don't commit yet.

---

### Task 2: `Edit`: multi-range changes, inverse, offset mapping

**Files:**
- Create: `crates/editor-core/src/edit.rs`
- Modify: `crates/editor-core/src/lib.rs` (add `mod edit;` and `pub use edit::{Bias, Change, Edit};`)

**Interfaces:**
- Consumes: `Buffer::replace`, `Buffer::rope`.
- Produces:
  - `Bias { Left, Right }`, `Change { pub range: Range<usize>, pub text: String }`
  - `Edit::new(Vec<Change>) -> Edit`: sorts the changes and merges overlapping ones.
  - `Edit::changes(&self) -> &[Change]`, `Edit::is_empty(&self) -> bool`
  - `pub(crate) Edit::apply(&self, &mut Buffer) -> Edit`: returns the inverse, whose ranges are in post-edit coordinates.
  - `Edit::map(&self, offset: usize, bias: Bias) -> usize`

- [ ] **Step 1: Write the failing tests** (test module in `edit.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn change(range: Range<usize>, text: &str) -> Change {
        Change { range, text: text.to_string() }
    }

    #[test]
    fn multi_range_edit_applies_and_inverts() {
        let mut buffer = Buffer::from_text("abc def");
        let edit = Edit::new(vec![change(4..7, "ghij"), change(0..0, "X")]);
        let inverse = edit.apply(&mut buffer);
        assert_eq!(buffer.rope().to_string(), "Xabc ghij");
        assert_eq!(inverse.changes(), &[change(0..1, ""), change(5..9, "def")]);
        inverse.apply(&mut buffer);
        assert_eq!(buffer.rope().to_string(), "abc def");
    }

    #[test]
    fn overlapping_changes_are_merged() {
        let edit = Edit::new(vec![change(3..5, ""), change(2..4, "")]);
        assert_eq!(edit.changes(), &[change(2..5, "")]);
    }

    #[test]
    fn inserts_at_the_same_offset_are_kept_in_order() {
        let mut buffer = Buffer::from_text("ab");
        Edit::new(vec![change(1..1, "X"), change(1..1, "Y")]).apply(&mut buffer);
        assert_eq!(buffer.rope().to_string(), "aXYb");
    }

    #[test]
    fn map_shifts_offsets_and_respects_bias() {
        let edit = Edit::new(vec![change(2..2, "xy"), change(5..7, "")]);
        assert_eq!(edit.map(1, Bias::Right), 1);
        assert_eq!(edit.map(2, Bias::Left), 2);
        assert_eq!(edit.map(2, Bias::Right), 4);
        assert_eq!(edit.map(4, Bias::Left), 6);
        assert_eq!(edit.map(6, Bias::Right), 7);
        assert_eq!(edit.map(9, Bias::Left), 9);
    }
}
```

- [ ] **Step 2: Run and confirm failure.** `cargo test -p editor-core edit` → compile error.

- [ ] **Step 3: Implement** in `edit.rs`:

```rust
use crate::buffer::Buffer;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bias {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub range: Range<usize>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Edit {
    changes: Vec<Change>,
}

impl Edit {
    pub fn new(mut changes: Vec<Change>) -> Self {
        changes.sort_by_key(|change| change.range.start);
        let mut merged: Vec<Change> = Vec::with_capacity(changes.len());
        for change in changes {
            match merged.last_mut() {
                Some(last) if change.range.start < last.range.end => {
                    last.range.end = last.range.end.max(change.range.end);
                }
                _ => merged.push(change),
            }
        }
        Self { changes: merged }
    }

    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub(crate) fn apply(&self, buffer: &mut Buffer) -> Edit {
        let removed: Vec<String> =
            self.changes.iter().map(|change| buffer.rope().slice(change.range.clone()).to_string()).collect();
        for change in self.changes.iter().rev() {
            buffer.replace(change.range.clone(), &change.text);
        }
        let mut delta: isize = 0;
        let changes = self
            .changes
            .iter()
            .zip(removed)
            .map(|(change, old)| {
                let start = offset_by(change.range.start, delta);
                let inserted = change.text.chars().count();
                delta += inserted as isize - change.range.len() as isize;
                Change { range: start..start + inserted, text: old }
            })
            .collect();
        Edit { changes }
    }

    pub fn map(&self, offset: usize, bias: Bias) -> usize {
        let mut delta: isize = 0;
        for change in &self.changes {
            let (start, end) = (change.range.start, change.range.end);
            if offset < start || (offset == start && bias == Bias::Left) {
                break;
            }
            let inserted = change.text.chars().count();
            let is_after = offset > end || (offset == end && start != end) || start == end;
            if is_after {
                delta += inserted as isize - (end - start) as isize;
                continue;
            }
            let landing = if bias == Bias::Right { start + inserted } else { start };
            return offset_by(landing, delta);
        }
        offset_by(offset, delta)
    }
}

fn offset_by(offset: usize, delta: isize) -> usize {
    offset.checked_add_signed(delta).unwrap_or(0)
}
```

- [ ] **Step 4: Run.** `cargo test -p editor-core` → all pass. `cargo clippy --workspace -- -D warnings` → clean.

- [ ] **Step 5: Commit Tasks 1 and 2.**

```bash
git add Cargo.toml Cargo.lock crates/editor-core
git commit -m "feat(editor-core): add rope buffer and multi-range edits"
```

---

### Task 3: `Selections`

**Files:**
- Create: `crates/editor-core/src/selection.rs`
- Modify: `crates/editor-core/src/lib.rs` (add `mod selection;` and `pub use selection::{Selection, Selections};`)

**Interfaces:**
- Produces:
  - `Selection { pub anchor: usize, pub head: usize, pub goal_column: Option<usize> }`
  - `Selection::cursor(usize)`, `Selection::new(anchor, head)`
  - `range(&self) -> Range<usize>`, `is_empty(&self) -> bool`, `is_reversed(&self) -> bool`
  - `Selections::single(Selection)`, `Selections::new(Vec<Selection>, newest: usize)`: sorts and merges, and keeps track of which selection is newest.
  - `as_slice(&self) -> &[Selection]`, `newest(&self) -> Selection`, `len(&self) -> usize`
  - `with(&self, Selection) -> Selections`: adds a selection, which becomes the newest.
  - `map(&self, impl FnMut(Selection) -> Selection) -> Selections`

- [ ] **Step 1: Write the failing tests:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_selections_merge_and_touching_ones_do_not() {
        let selections = Selections::new(vec![Selection::new(4, 8), Selection::new(0, 5), Selection::new(8, 10)], 0);
        assert_eq!(selections.as_slice(), &[Selection::new(0, 8), Selection::new(8, 10)]);
    }

    #[test]
    fn identical_cursors_collapse_to_one() {
        let selections = Selections::new(vec![Selection::cursor(3), Selection::cursor(3)], 1);
        assert_eq!(selections.len(), 1);
    }

    #[test]
    fn newest_follows_the_added_selection_after_sorting() {
        let selections = Selections::single(Selection::cursor(10)).with(Selection::cursor(2));
        assert_eq!(selections.as_slice(), &[Selection::cursor(2), Selection::cursor(10)]);
        assert_eq!(selections.newest(), Selection::cursor(2));
    }

    #[test]
    fn merged_reversed_selections_stay_reversed() {
        let selections = Selections::new(vec![Selection::new(5, 0), Selection::new(8, 3)], 0);
        assert_eq!(selections.as_slice(), &[Selection::new(8, 0)]);
    }

    #[test]
    fn empty_input_falls_back_to_a_cursor_at_zero() {
        assert_eq!(Selections::new(Vec::new(), 0).as_slice(), &[Selection::cursor(0)]);
    }
}
```

- [ ] **Step 2: Run and confirm failure.** `cargo test -p editor-core selection` → compile error.

- [ ] **Step 3: Implement:**

```rust
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
    pub goal_column: Option<usize>,
}

impl Selection {
    pub fn cursor(offset: usize) -> Self {
        Self::new(offset, offset)
    }

    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head, goal_column: None }
    }

    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn is_reversed(&self) -> bool {
        self.head < self.anchor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selections {
    items: Vec<Selection>,
    newest: usize,
}

impl Selections {
    pub fn single(selection: Selection) -> Self {
        Self { items: vec![selection], newest: 0 }
    }

    pub fn new(items: Vec<Selection>, newest: usize) -> Self {
        let Some(newest_head) = items.get(newest).or(items.last()).map(|selection| selection.head) else {
            return Self::single(Selection::cursor(0));
        };
        let mut sorted = items;
        sorted.sort_by_key(|selection| (selection.range().start, selection.range().end));
        let mut merged: Vec<Selection> = Vec::with_capacity(sorted.len());
        for selection in sorted {
            match merged.last_mut() {
                Some(last) if overlaps(last, &selection) => *last = union(last, &selection),
                _ => merged.push(selection),
            }
        }
        let newest = merged
            .iter()
            .position(|selection| selection.range().contains(&newest_head) || selection.range().end == newest_head)
            .unwrap_or(merged.len() - 1);
        Self { items: merged, newest }
    }

    pub fn as_slice(&self) -> &[Selection] {
        &self.items
    }

    pub fn newest(&self) -> Selection {
        self.items[self.newest]
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn with(&self, selection: Selection) -> Self {
        let mut items = self.items.clone();
        items.push(selection);
        let newest = items.len() - 1;
        Self::new(items, newest)
    }

    pub fn map(&self, transform: impl FnMut(Selection) -> Selection) -> Self {
        Self::new(self.items.iter().copied().map(transform).collect(), self.newest)
    }
}

fn overlaps(earlier: &Selection, later: &Selection) -> bool {
    let (earlier, later) = (earlier.range(), later.range());
    later.start < earlier.end || later.start == earlier.start
}

fn union(first: &Selection, second: &Selection) -> Selection {
    let start = first.range().start.min(second.range().start);
    let end = first.range().end.max(second.range().end);
    if first.is_reversed() && second.is_reversed() {
        Selection::new(end, start)
    } else {
        Selection::new(start, end)
    }
}
```

`is_empty` exists only to satisfy clippy's `len_without_is_empty` lint. It always returns false in practice, because the list is never empty.

- [ ] **Step 4: Run.** `cargo test -p editor-core` → all pass, and clippy is clean.

- [ ] **Step 5: Commit.**

```bash
git add crates/editor-core
git commit -m "feat(editor-core): add sorted merged multi-cursor selections"
```

---

### Task 4: Motions

**Files:**
- Create: `crates/editor-core/src/motion.rs`
- Modify: `crates/editor-core/src/lib.rs` (add `mod motion;` and `pub use motion::{char_column, display_column, move_selection, Motion, TAB_WIDTH};`)

**Interfaces:**
- Consumes: `line_len`, `Selection`.
- Produces:
  - `TAB_WIDTH: usize = 4`
  - `Motion { Left, Right, Up, Down, WordLeft, WordRight, LineStart, LineEnd, DocumentStart, DocumentEnd, PageUp(usize), PageDown(usize) }`
  - `move_selection(rope: &Rope, selection: Selection, motion: Motion, extend: bool) -> Selection`
  - `display_column(line: &str, char_column: usize) -> usize`
  - `char_column(line: &str, display_column: usize) -> usize`
  - `pub(crate)` helpers `prev_grapheme`, `next_grapheme`, `word_left`, `word_right`, `line_start`, `line_end`, used by Task 5.

- [ ] **Step 1: Write the failing tests:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn moved(text: &str, head: usize, motion: Motion) -> usize {
        move_selection(&Rope::from_str(text), Selection::cursor(head), motion, false).head
    }

    #[test]
    fn right_moves_over_a_whole_grapheme_cluster() {
        let family = "a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}b";
        assert_eq!(moved(family, 1, Motion::Right), 6);
        assert_eq!(moved(family, 6, Motion::Left), 1);
        assert_eq!(moved("e\u{301}x", 0, Motion::Right), 2);
    }

    #[test]
    fn left_and_right_cross_line_breaks() {
        assert_eq!(moved("ab\ncd", 3, Motion::Left), 2);
        assert_eq!(moved("ab\ncd", 2, Motion::Right), 3);
        assert_eq!(moved("ab", 0, Motion::Left), 0);
        assert_eq!(moved("ab", 2, Motion::Right), 2);
    }

    #[test]
    fn word_motions_stop_at_class_boundaries() {
        let text = "foo.bar  baz";
        assert_eq!(moved(text, 0, Motion::WordRight), 3);
        assert_eq!(moved(text, 3, Motion::WordRight), 4);
        assert_eq!(moved(text, 4, Motion::WordRight), 7);
        assert_eq!(moved(text, 7, Motion::WordRight), 12);
        assert_eq!(moved(text, 12, Motion::WordLeft), 9);
        assert_eq!(moved(text, 9, Motion::WordLeft), 4);
    }

    #[test]
    fn line_start_toggles_between_indent_and_column_zero() {
        assert_eq!(moved("    xy", 6, Motion::LineStart), 4);
        assert_eq!(moved("    xy", 4, Motion::LineStart), 0);
        assert_eq!(moved("ab\ncd\n", 3, Motion::LineEnd), 5);
    }

    #[test]
    fn vertical_motion_keeps_display_goal_column_across_tabs() {
        let rope = Rope::from_str("\tab\nabcdefgh\nx");
        let start = Selection::cursor(3);
        let down = move_selection(&rope, start, Motion::Down, false);
        assert_eq!(down.head, 10);
        let down_again = move_selection(&rope, down, Motion::Down, false);
        assert_eq!(down_again.head, 14);
        let up = move_selection(&rope, down_again, Motion::Up, false);
        assert_eq!(up.head, 10);
    }

    #[test]
    fn up_on_first_line_goes_to_start_and_down_on_last_goes_to_end() {
        assert_eq!(moved("abc\ndef", 2, Motion::Up), 0);
        assert_eq!(moved("abc\ndef", 5, Motion::Down), 7);
    }

    #[test]
    fn plain_left_collapses_a_selection_to_its_start() {
        let rope = Rope::from_str("abcdef");
        let moved = move_selection(&rope, Selection::new(1, 4), Motion::Left, false);
        assert_eq!(moved, Selection::cursor(1));
    }

    #[test]
    fn extending_keeps_the_anchor() {
        let rope = Rope::from_str("abcdef");
        let moved = move_selection(&rope, Selection::cursor(2), Motion::WordRight, true);
        assert_eq!((moved.anchor, moved.head), (2, 6));
    }

    #[test]
    fn display_and_char_columns_round_trip_through_tabs() {
        assert_eq!(display_column("\tab", 2), 5);
        assert_eq!(char_column("\tab", 5), 2);
        assert_eq!(char_column("\tab", 2), 0);
    }
}
```

- [ ] **Step 2: Run and confirm failure.**

- [ ] **Step 3: Implement:**

```rust
use crate::buffer::line_len;
use crate::selection::Selection;
use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

pub const TAB_WIDTH: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
    PageUp(usize),
    PageDown(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Whitespace,
    Word,
    Punctuation,
}

pub fn move_selection(rope: &Rope, selection: Selection, motion: Motion, extend: bool) -> Selection {
    if !extend && !selection.is_empty() {
        match motion {
            Motion::Left => return Selection::cursor(selection.range().start),
            Motion::Right => return Selection::cursor(selection.range().end),
            _ => {}
        }
    }
    let (head, goal_column) = match motion {
        Motion::Up => vertical(rope, selection, -1),
        Motion::Down => vertical(rope, selection, 1),
        Motion::PageUp(lines) => vertical(rope, selection, -(lines as isize)),
        Motion::PageDown(lines) => vertical(rope, selection, lines as isize),
        other => (horizontal(rope, selection.head, other), None),
    };
    let anchor = if extend { selection.anchor } else { head };
    Selection { anchor, head, goal_column }
}

pub fn display_column(line: &str, char_column: usize) -> usize {
    line.chars().take(char_column).fold(0, advance)
}

pub fn char_column(line: &str, display_column: usize) -> usize {
    let mut column = 0;
    for (index, ch) in line.chars().enumerate() {
        let next = advance(column, ch);
        if next > display_column {
            return index;
        }
        column = next;
    }
    line.chars().count()
}

pub(crate) fn prev_grapheme(rope: &Rope, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let line = rope.char_to_line(offset);
    let start = rope.line_to_char(line);
    if offset == start {
        return offset - 1;
    }
    let column = offset - start;
    grapheme_boundaries(&line_text(rope, line))
        .into_iter()
        .take_while(|boundary| *boundary < column)
        .last()
        .map_or(offset - 1, |boundary| start + boundary)
}

pub(crate) fn next_grapheme(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len_chars() {
        return rope.len_chars();
    }
    let line = rope.char_to_line(offset);
    let start = rope.line_to_char(line);
    let column = offset - start;
    if column >= line_len(rope, line) {
        return offset + 1;
    }
    grapheme_boundaries(&line_text(rope, line))
        .into_iter()
        .find(|boundary| *boundary > column)
        .map_or(offset + 1, |boundary| start + boundary)
}

pub(crate) fn word_right(rope: &Rope, offset: usize) -> usize {
    let mut chars = rope.chars_at(offset).peekable();
    let mut position = offset;
    while chars.peek().is_some_and(|ch| class(*ch) == CharClass::Whitespace) {
        chars.next();
        position += 1;
    }
    let Some(first) = chars.peek().map(|ch| class(*ch)) else {
        return position;
    };
    while chars.peek().is_some_and(|ch| class(*ch) == first) {
        chars.next();
        position += 1;
    }
    position
}

pub(crate) fn word_left(rope: &Rope, offset: usize) -> usize {
    let mut chars = rope.chars_at(offset);
    let mut position = offset;
    let mut previous = chars.prev();
    while previous.is_some_and(|ch| class(ch) == CharClass::Whitespace) {
        position -= 1;
        previous = chars.prev();
    }
    let Some(first) = previous.map(class) else {
        return position;
    };
    while previous.is_some_and(|ch| class(ch) == first) {
        position -= 1;
        previous = chars.prev();
    }
    position
}

pub(crate) fn line_start(rope: &Rope, offset: usize) -> usize {
    rope.line_to_char(rope.char_to_line(offset))
}

pub(crate) fn line_end(rope: &Rope, offset: usize) -> usize {
    let line = rope.char_to_line(offset);
    rope.line_to_char(line) + line_len(rope, line)
}

fn horizontal(rope: &Rope, head: usize, motion: Motion) -> usize {
    match motion {
        Motion::Left => prev_grapheme(rope, head),
        Motion::Right => next_grapheme(rope, head),
        Motion::WordLeft => word_left(rope, head),
        Motion::WordRight => word_right(rope, head),
        Motion::LineStart => smart_line_start(rope, head),
        Motion::LineEnd => line_end(rope, head),
        Motion::DocumentStart => 0,
        Motion::DocumentEnd => rope.len_chars(),
        Motion::Up | Motion::Down | Motion::PageUp(_) | Motion::PageDown(_) => head,
    }
}

fn vertical(rope: &Rope, selection: Selection, lines: isize) -> (usize, Option<usize>) {
    let line = rope.char_to_line(selection.head);
    let goal = selection
        .goal_column
        .unwrap_or_else(|| display_column(&line_text(rope, line), selection.head - rope.line_to_char(line)));
    let target = line as isize + lines;
    if target < 0 {
        return (0, Some(goal));
    }
    let target = target as usize;
    if target >= rope.len_lines() {
        return (rope.len_chars(), Some(goal));
    }
    let column = char_column(&line_text(rope, target), goal).min(line_len(rope, target));
    (rope.line_to_char(target) + column, Some(goal))
}

fn smart_line_start(rope: &Rope, head: usize) -> usize {
    let line = rope.char_to_line(head);
    let start = rope.line_to_char(line);
    let indent = rope.line(line).chars().take_while(|ch| *ch == ' ' || *ch == '\t').count();
    let first_text = start + indent;
    if head == first_text {
        start
    } else {
        first_text
    }
}

fn line_text(rope: &Rope, line: usize) -> String {
    rope.line(line).to_string()
}

fn grapheme_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = vec![0];
    let mut chars = 0;
    for grapheme in text.graphemes(true) {
        chars += grapheme.chars().count();
        boundaries.push(chars);
    }
    boundaries
}

fn class(ch: char) -> CharClass {
    if ch.is_whitespace() {
        CharClass::Whitespace
    } else if ch.is_alphanumeric() || ch == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

fn advance(column: usize, ch: char) -> usize {
    if ch == '\t' {
        column + TAB_WIDTH - column % TAB_WIDTH
    } else {
        column + 1
    }
}
```

`line_start` and `word_left` are `pub(crate)` and unused until Task 5. If `deny(warnings)` fails on dead code, commit Tasks 4 and 5 together. Don't add `allow` attributes.

- [ ] **Step 4: Run.** `cargo test -p editor-core motion` → all pass.

- [ ] **Step 5: Commit** (or hold for Task 5 if dead-code warnings fire):

```bash
git add crates/editor-core
git commit -m "feat(editor-core): add grapheme, word, line and vertical motions"
```

---

### Task 5: `Editor`: typing, newline, delete, undo/redo, move

**Files:**
- Create: `crates/editor-core/src/history.rs`
- Create: `crates/editor-core/src/editor.rs`
- Modify: `crates/editor-core/src/lib.rs` (add `mod history; mod editor;` and `pub use editor::{Editor, EditorAction, Granularity};`)

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces:
  - `Granularity { Char, Word, Line }`
  - `EditorAction { Move { motion: Motion, extend: bool }, InsertText(String), Newline, DeleteBackward(Granularity), DeleteForward(Granularity), Indent, Outdent, SelectAll, Undo, Redo, Cut }`
  - `Editor::new(Buffer) -> Editor`, `buffer(&self) -> &Buffer`, `selections(&self) -> &Selections`
  - `set_selections(&mut self, Selections)` (clamps offsets), `mark_saved(&mut self)`
  - `apply(&mut self, EditorAction, now: Instant) -> Option<String>`
  - `copy_text(&self) -> Option<String>`, `paste(&mut self, text: &str, now: Instant)`

  Indent, Outdent, Cut and paste are implemented in Task 6. In this task, their `apply` arms call the private methods Task 6 fills in, and those methods start out as `todo!()`. Don't do that, because a `todo!()` would ship if Task 6 were skipped. Write the `EditorAction` enum with every variant now, implement the Task 5 arms, and implement the Task 6 arms in Task 6. Tasks 5 and 6 commit together.

- [ ] **Step 1: `history.rs`** (no separate test, since `Editor`'s tests exercise it):

```rust
use crate::buffer::Buffer;
use crate::edit::Edit;
use crate::selection::Selections;
use std::time::{Duration, Instant};

const TYPING_GROUP_WINDOW: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditKind {
    Typing,
    Other,
}

#[derive(Debug, Clone)]
struct Transaction {
    steps: Vec<(Edit, Edit)>,
    before: Selections,
    after: Selections,
    kind: EditKind,
    at: Instant,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
}

pub(crate) struct Record {
    pub forward: Edit,
    pub inverse: Edit,
    pub before: Selections,
    pub after: Selections,
    pub kind: EditKind,
    pub at: Instant,
}

impl History {
    pub(crate) fn record(&mut self, record: Record) {
        self.redo.clear();
        if let Some(last) = self.undo.last_mut() {
            let is_continuation = record.kind == EditKind::Typing
                && last.kind == EditKind::Typing
                && record.at.duration_since(last.at) <= TYPING_GROUP_WINDOW
                && last.after == record.before;
            if is_continuation {
                last.steps.push((record.forward, record.inverse));
                last.after = record.after;
                last.at = record.at;
                return;
            }
        }
        self.undo.push(Transaction {
            steps: vec![(record.forward, record.inverse)],
            before: record.before,
            after: record.after,
            kind: record.kind,
            at: record.at,
        });
    }

    pub(crate) fn undo(&mut self, buffer: &mut Buffer) -> Option<Selections> {
        let transaction = self.undo.pop()?;
        for (_, inverse) in transaction.steps.iter().rev() {
            inverse.apply(buffer);
        }
        let selections = transaction.before.clone();
        self.redo.push(transaction);
        Some(selections)
    }

    pub(crate) fn redo(&mut self, buffer: &mut Buffer) -> Option<Selections> {
        let transaction = self.redo.pop()?;
        for (forward, _) in &transaction.steps {
            forward.apply(buffer);
        }
        let selections = transaction.after.clone();
        self.undo.push(transaction);
        Some(selections)
    }
}
```

- [ ] **Step 2: Write the failing editor tests** (test module in `editor.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn editor_with(text: &str, cursors: &[usize]) -> Editor {
        let mut editor = Editor::new(Buffer::from_text(text));
        let selections = cursors.iter().map(|offset| Selection::cursor(*offset)).collect();
        editor.set_selections(Selections::new(selections, 0));
        editor
    }

    fn text(editor: &Editor) -> String {
        editor.buffer().rope().to_string()
    }

    fn heads(editor: &Editor) -> Vec<usize> {
        editor.selections().as_slice().iter().map(|selection| selection.head).collect()
    }

    #[test]
    fn typing_at_three_cursors_inserts_at_each() {
        let mut editor = editor_with("a\nb\nc", &[1, 3, 5]);
        editor.apply(EditorAction::InsertText("!".into()), Instant::now());
        assert_eq!(text(&editor), "a!\nb!\nc!");
        assert_eq!(heads(&editor), vec![2, 5, 8]);
    }

    #[test]
    fn typing_replaces_a_selection() {
        let mut editor = Editor::new(Buffer::from_text("hello world"));
        editor.set_selections(Selections::single(Selection::new(0, 5)));
        editor.apply(EditorAction::InsertText("bye".into()), Instant::now());
        assert_eq!(text(&editor), "bye world");
        assert_eq!(heads(&editor), vec![3]);
    }

    #[test]
    fn quick_typing_is_one_undo_step_and_a_pause_starts_another() {
        let mut editor = editor_with("", &[0]);
        let start = Instant::now();
        editor.apply(EditorAction::InsertText("a".into()), start);
        editor.apply(EditorAction::InsertText("b".into()), start + Duration::from_millis(100));
        editor.apply(EditorAction::InsertText("c".into()), start + Duration::from_millis(1000));
        editor.apply(EditorAction::Undo, start + Duration::from_millis(1100));
        assert_eq!(text(&editor), "ab");
        editor.apply(EditorAction::Undo, start + Duration::from_millis(1200));
        assert_eq!(text(&editor), "");
        editor.apply(EditorAction::Redo, start + Duration::from_millis(1300));
        assert_eq!(text(&editor), "ab");
        assert_eq!(heads(&editor), vec![2]);
    }

    #[test]
    fn a_cursor_jump_starts_a_new_undo_step() {
        let mut editor = editor_with("xy", &[0]);
        let now = Instant::now();
        editor.apply(EditorAction::InsertText("a".into()), now);
        editor.apply(EditorAction::Move { motion: Motion::DocumentEnd, extend: false }, now);
        editor.apply(EditorAction::InsertText("b".into()), now);
        editor.apply(EditorAction::Undo, now);
        assert_eq!(text(&editor), "axy");
    }

    #[test]
    fn newline_keeps_the_current_indentation() {
        let mut editor = editor_with("    foo", &[7]);
        editor.apply(EditorAction::Newline, Instant::now());
        assert_eq!(text(&editor), "    foo\n    ");
        assert_eq!(heads(&editor), vec![12]);
    }

    #[test]
    fn backspace_with_adjacent_cursors_merges_changes() {
        let mut editor = editor_with("abc", &[1, 2]);
        editor.apply(EditorAction::DeleteBackward(Granularity::Char), Instant::now());
        assert_eq!(text(&editor), "c");
        assert_eq!(heads(&editor), vec![0]);
    }

    #[test]
    fn backspace_deletes_a_whole_grapheme_cluster() {
        let mut editor = editor_with("e\u{301}x", &[2]);
        editor.apply(EditorAction::DeleteBackward(Granularity::Char), Instant::now());
        assert_eq!(text(&editor), "x");
    }

    #[test]
    fn word_and_line_deletion() {
        let mut editor = editor_with("let value = 1", &[9]);
        editor.apply(EditorAction::DeleteBackward(Granularity::Word), Instant::now());
        assert_eq!(text(&editor), "let  = 1");
        editor.apply(EditorAction::DeleteBackward(Granularity::Line), Instant::now());
        assert_eq!(text(&editor), " = 1");
        editor.apply(EditorAction::DeleteForward(Granularity::Line), Instant::now());
        assert_eq!(text(&editor), "");
    }

    #[test]
    fn line_backspace_at_column_zero_joins_lines() {
        let mut editor = editor_with("ab\ncd", &[3]);
        editor.apply(EditorAction::DeleteBackward(Granularity::Line), Instant::now());
        assert_eq!(text(&editor), "abcd");
    }

    #[test]
    fn select_all_then_type_replaces_everything() {
        let mut editor = editor_with("abc\ndef", &[0]);
        editor.apply(EditorAction::SelectAll, Instant::now());
        editor.apply(EditorAction::InsertText("z".into()), Instant::now());
        assert_eq!(text(&editor), "z");
    }

    #[test]
    fn set_selections_clamps_to_the_buffer() {
        let editor = editor_with("ab", &[99]);
        assert_eq!(heads(&editor), vec![2]);
    }
}
```

- [ ] **Step 3: Run and confirm failure.**

- [ ] **Step 4: Implement `editor.rs`** (Task 6 adds the indent, outdent, cut and paste bodies):

```rust
use crate::buffer::Buffer;
use crate::edit::{Bias, Change, Edit};
use crate::history::{EditKind, History, Record};
use crate::motion::{line_end, line_start, move_selection, next_grapheme, prev_grapheme, word_left, word_right, Motion};
use crate::selection::{Selection, Selections};
use ropey::Rope;
use std::time::Instant;

const NEWLINE: &str = "\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Char,
    Word,
    Line,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    Move { motion: Motion, extend: bool },
    InsertText(String),
    Newline,
    DeleteBackward(Granularity),
    DeleteForward(Granularity),
    Indent,
    Outdent,
    SelectAll,
    Undo,
    Redo,
    Cut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    AfterInsert,
    Mapped,
}

pub struct Editor {
    buffer: Buffer,
    selections: Selections,
    history: History,
}

impl Editor {
    pub fn new(buffer: Buffer) -> Self {
        Self { buffer, selections: Selections::single(Selection::cursor(0)), history: History::default() }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn selections(&self) -> &Selections {
        &self.selections
    }

    pub fn set_selections(&mut self, selections: Selections) {
        let len = self.buffer.len_chars();
        self.selections = selections.map(|selection| Selection {
            anchor: selection.anchor.min(len),
            head: selection.head.min(len),
            goal_column: selection.goal_column,
        });
    }

    pub fn mark_saved(&mut self) {
        self.buffer.mark_saved();
    }

    pub fn apply(&mut self, action: EditorAction, now: Instant) -> Option<String> {
        match action {
            EditorAction::Move { motion, extend } => {
                let rope = self.buffer.rope();
                self.selections = self.selections.map(|selection| move_selection(rope, selection, motion, extend));
            }
            EditorAction::InsertText(text) => self.replace_selections(&text, EditKind::Typing, now),
            EditorAction::Newline => self.insert_newline(now),
            EditorAction::DeleteBackward(granularity) => self.delete(granularity, true, now),
            EditorAction::DeleteForward(granularity) => self.delete(granularity, false, now),
            EditorAction::Indent => self.indent(now),
            EditorAction::Outdent => self.outdent(now),
            EditorAction::SelectAll => {
                self.selections = Selections::single(Selection::new(0, self.buffer.len_chars()));
            }
            EditorAction::Undo => {
                if let Some(selections) = self.history.undo(&mut self.buffer) {
                    self.selections = selections;
                }
            }
            EditorAction::Redo => {
                if let Some(selections) = self.history.redo(&mut self.buffer) {
                    self.selections = selections;
                }
            }
            EditorAction::Cut => return self.cut(now),
        }
        None
    }

    fn replace_selections(&mut self, text: &str, kind: EditKind, now: Instant) {
        let changes = self
            .selections
            .as_slice()
            .iter()
            .map(|selection| Change { range: selection.range(), text: text.to_string() })
            .collect();
        self.commit(Edit::new(changes), kind, now, Placement::AfterInsert);
    }

    fn insert_newline(&mut self, now: Instant) {
        let rope = self.buffer.rope();
        let changes = self
            .selections
            .as_slice()
            .iter()
            .map(|selection| {
                let start = line_start(rope, selection.range().start);
                let indent: String = rope
                    .slice(start..selection.range().start)
                    .chars()
                    .take_while(|ch| *ch == ' ' || *ch == '\t')
                    .collect();
                Change { range: selection.range(), text: format!("{NEWLINE}{indent}") }
            })
            .collect();
        self.commit(Edit::new(changes), EditKind::Other, now, Placement::AfterInsert);
    }

    fn delete(&mut self, granularity: Granularity, is_backward: bool, now: Instant) {
        let rope = self.buffer.rope();
        let changes = self
            .selections
            .as_slice()
            .iter()
            .map(|selection| {
                if !selection.is_empty() {
                    return Change { range: selection.range(), text: String::new() };
                }
                let target = deletion_target(rope, selection.head, granularity, is_backward);
                Change { range: target.min(selection.head)..target.max(selection.head), text: String::new() }
            })
            .filter(|change| !change.range.is_empty())
            .collect();
        self.commit(Edit::new(changes), EditKind::Typing, now, Placement::AfterInsert);
    }

    fn commit(&mut self, edit: Edit, kind: EditKind, now: Instant, placement: Placement) {
        if edit.is_empty() {
            return;
        }
        let before = self.selections.clone();
        let inverse = edit.apply(&mut self.buffer);
        let after = match placement {
            Placement::AfterInsert => {
                let newest_head = edit.map(before.newest().head, Bias::Right);
                let cursors: Vec<Selection> =
                    inverse.changes().iter().map(|change| Selection::cursor(change.range.end)).collect();
                let newest = cursors.iter().position(|cursor| cursor.head == newest_head).unwrap_or(cursors.len() - 1);
                Selections::new(cursors, newest)
            }
            Placement::Mapped => before.map(|selection| {
                Selection::new(edit.map(selection.anchor, Bias::Right), edit.map(selection.head, Bias::Right))
            }),
        };
        self.history.record(Record { forward: edit, inverse, before, after: after.clone(), kind, at: now });
        self.selections = after;
    }
}

fn deletion_target(rope: &Rope, head: usize, granularity: Granularity, is_backward: bool) -> usize {
    match (granularity, is_backward) {
        (Granularity::Char, true) => prev_grapheme(rope, head),
        (Granularity::Char, false) => next_grapheme(rope, head),
        (Granularity::Word, true) => word_left(rope, head),
        (Granularity::Word, false) => word_right(rope, head),
        (Granularity::Line, true) => {
            let start = line_start(rope, head);
            if start == head {
                head.saturating_sub(1)
            } else {
                start
            }
        }
        (Granularity::Line, false) => {
            let end = line_end(rope, head);
            if end == head {
                next_grapheme(rope, head)
            } else {
                end
            }
        }
    }
}
```

The test `word_and_line_deletion` deletes forward with Line from offset 0 of `" = 1"`. `line_end` there is 4, so the result is `""`.

- [ ] **Step 5: Run.** The tests compile only once Task 6's `indent`, `outdent` and `cut` exist. Go straight on to Task 6, then run both test sets.

---

### Task 6: Indent, outdent, copy, cut, paste

**Files:**
- Modify: `crates/editor-core/src/editor.rs`

**Interfaces:**
- Produces: `Editor::copy_text(&self) -> Option<String>`, `Editor::paste(&mut self, &str, Instant)`, and the private `indent`, `outdent` and `cut` used by `apply`.

- [ ] **Step 1: Add the failing tests** to `editor.rs`'s test module:

```rust
    #[test]
    fn tab_inserts_indent_at_a_cursor() {
        let mut editor = editor_with("ab", &[1]);
        editor.apply(EditorAction::Indent, Instant::now());
        assert_eq!(text(&editor), "a    b");
    }

    #[test]
    fn tab_with_a_multiline_selection_indents_each_line_and_shift_tab_reverts() {
        let mut editor = Editor::new(Buffer::from_text("a\nb\nc"));
        editor.set_selections(Selections::single(Selection::new(0, 3)));
        editor.apply(EditorAction::Indent, Instant::now());
        assert_eq!(text(&editor), "    a\n    b\nc");
        editor.apply(EditorAction::Outdent, Instant::now());
        assert_eq!(text(&editor), "a\nb\nc");
    }

    #[test]
    fn outdent_removes_a_leading_tab() {
        let mut editor = editor_with("\tx", &[2]);
        editor.apply(EditorAction::Outdent, Instant::now());
        assert_eq!(text(&editor), "x");
    }

    #[test]
    fn copy_joins_selections_and_ignores_empty_ones() {
        let mut editor = Editor::new(Buffer::from_text("one two three"));
        editor.set_selections(Selections::new(
            vec![Selection::new(0, 3), Selection::cursor(5), Selection::new(8, 13)],
            0,
        ));
        assert_eq!(editor.copy_text().as_deref(), Some("one\nthree"));
        editor.set_selections(Selections::single(Selection::cursor(1)));
        assert_eq!(editor.copy_text(), None);
    }

    #[test]
    fn cut_returns_the_text_and_removes_it() {
        let mut editor = Editor::new(Buffer::from_text("hello world"));
        editor.set_selections(Selections::single(Selection::new(5, 11)));
        let cut = editor.apply(EditorAction::Cut, Instant::now());
        assert_eq!(cut.as_deref(), Some(" world"));
        assert_eq!(text(&editor), "hello");
    }

    #[test]
    fn paste_distributes_lines_across_matching_cursors() {
        let mut editor = editor_with("a\nb", &[1, 3]);
        editor.paste("1\n2", Instant::now());
        assert_eq!(text(&editor), "a1\nb2");
    }

    #[test]
    fn paste_inserts_whole_text_when_counts_differ() {
        let mut editor = editor_with("ab", &[1]);
        editor.paste("x\ny", Instant::now());
        assert_eq!(text(&editor), "ax\nyb");
        assert_eq!(heads(&editor), vec![4]);
    }
```

- [ ] **Step 2: Implement.** Add these to `impl Editor`, plus the const `INDENT` at the top of the file:

```rust
const INDENT: &str = "    ";
```

```rust
    pub fn copy_text(&self) -> Option<String> {
        let rope = self.buffer.rope();
        let parts: Vec<String> = self
            .selections
            .as_slice()
            .iter()
            .filter(|selection| !selection.is_empty())
            .map(|selection| rope.slice(selection.range()).to_string())
            .collect();
        (!parts.is_empty()).then(|| parts.join(NEWLINE))
    }

    pub fn paste(&mut self, text: &str, now: Instant) {
        let lines: Vec<&str> = text.split(NEWLINE).collect();
        if self.selections.len() > 1 && lines.len() == self.selections.len() {
            let changes = self
                .selections
                .as_slice()
                .iter()
                .zip(lines)
                .map(|(selection, line)| Change { range: selection.range(), text: line.to_string() })
                .collect();
            self.commit(Edit::new(changes), EditKind::Other, now, Placement::AfterInsert);
            return;
        }
        self.replace_selections(text, EditKind::Other, now);
    }

    fn cut(&mut self, now: Instant) -> Option<String> {
        let text = self.copy_text()?;
        let changes = self
            .selections
            .as_slice()
            .iter()
            .filter(|selection| !selection.is_empty())
            .map(|selection| Change { range: selection.range(), text: String::new() })
            .collect();
        self.commit(Edit::new(changes), EditKind::Other, now, Placement::AfterInsert);
        Some(text)
    }

    fn indent(&mut self, now: Instant) {
        if self.selections.as_slice().iter().all(Selection::is_empty) {
            self.replace_selections(INDENT, EditKind::Typing, now);
            return;
        }
        let rope = self.buffer.rope();
        let changes = touched_lines(rope, &self.selections)
            .into_iter()
            .map(|line| {
                let start = rope.line_to_char(line);
                Change { range: start..start, text: INDENT.to_string() }
            })
            .collect();
        self.commit(Edit::new(changes), EditKind::Other, now, Placement::Mapped);
    }

    fn outdent(&mut self, now: Instant) {
        let rope = self.buffer.rope();
        let changes = touched_lines(rope, &self.selections)
            .into_iter()
            .filter_map(|line| {
                let start = rope.line_to_char(line);
                let spaces = rope.line(line).chars().take(INDENT.len()).take_while(|ch| *ch == ' ').count();
                let removable = if spaces == 0 && rope.line(line).chars().next() == Some('\t') { 1 } else { spaces };
                (removable > 0).then(|| Change { range: start..start + removable, text: String::new() })
            })
            .collect();
        self.commit(Edit::new(changes), EditKind::Other, now, Placement::Mapped);
    }
```

Then add this as a free function below `deletion_target`:

```rust
fn touched_lines(rope: &Rope, selections: &Selections) -> Vec<usize> {
    let mut lines: Vec<usize> = selections
        .as_slice()
        .iter()
        .flat_map(|selection| {
            let range = selection.range();
            let first = rope.char_to_line(range.start);
            let mut last = rope.char_to_line(range.end);
            if last > first && range.end == rope.line_to_char(last) {
                last -= 1;
            }
            first..=last
        })
        .collect();
    lines.dedup();
    lines
}
```

In `tab_with_a_multiline_selection_indents_each_line_and_shift_tab_reverts`, the selection `0..3` ends at the start of line 1 (char 2 is `b`). Is line 1 touched? Line 1 starts at char 2, and `range.end` = 3 is not equal to 2, so line 1 is included. Line 2 starts at char 4 and isn't reached. The expected text `"    a\n    b\nc"` matches.

- [ ] **Step 3: Run Tasks 5 and 6 together.** `cargo test -p editor-core` → all pass. `cargo clippy --workspace -- -D warnings` → clean.

- [ ] **Step 4: Commit.**

```bash
git add crates/editor-core
git commit -m "feat(editor-core): add editor with multi-cursor editing and grouped undo"
```

---

### Task 7: Standard key mapping, load and save

**Files:**
- Create: `crates/editor-core/src/keys.rs`
- Create: `crates/editor-core/src/io.rs`
- Modify: `crates/editor-core/src/lib.rs` (add `mod keys; pub mod io;` and `pub use keys::{standard_action, KeyInput};`)

**Interfaces:**
- Produces:
  - `KeyInput<'a> { pub key: &'a str, pub text: Option<&'a str>, pub cmd: bool, pub alt: bool, pub shift: bool, pub ctrl: bool }`
  - `standard_action(&KeyInput, page_lines: usize) -> Option<EditorAction>`
  - `io::load(&Path) -> Result<Buffer, io::LoadError>`
  - `io::save(&Path, &Buffer) -> std::io::Result<()>`
  - `io::LoadError { Io(std::io::Error), Binary, NotUtf8 }`

  Plan 2b's GPUI layer builds a `KeyInput` from `gpui::Keystroke`: `key` → `key`, `key_char` → `text`, and `modifiers.platform` / `alt` / `shift` / `control` → the flags.

- [ ] **Step 1: Write the failing tests.** In `keys.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str) -> KeyInput<'_> {
        KeyInput { key, text: None, cmd: false, alt: false, shift: false, ctrl: false }
    }

    #[test]
    fn arrows_map_to_motions_with_modifiers() {
        let movement = |motion, extend| Some(EditorAction::Move { motion, extend });
        assert_eq!(standard_action(&key("left"), 30), movement(Motion::Left, false));
        assert_eq!(standard_action(&KeyInput { shift: true, ..key("right") }, 30), movement(Motion::Right, true));
        assert_eq!(standard_action(&KeyInput { alt: true, ..key("left") }, 30), movement(Motion::WordLeft, false));
        assert_eq!(standard_action(&KeyInput { cmd: true, ..key("right") }, 30), movement(Motion::LineEnd, false));
        assert_eq!(standard_action(&KeyInput { cmd: true, ..key("up") }, 30), movement(Motion::DocumentStart, false));
        assert_eq!(standard_action(&key("pagedown"), 30), movement(Motion::PageDown(30), false));
    }

    #[test]
    fn deletion_granularity_follows_modifiers() {
        assert_eq!(standard_action(&key("backspace"), 30), Some(EditorAction::DeleteBackward(Granularity::Char)));
        assert_eq!(
            standard_action(&KeyInput { alt: true, ..key("backspace") }, 30),
            Some(EditorAction::DeleteBackward(Granularity::Word))
        );
        assert_eq!(
            standard_action(&KeyInput { cmd: true, ..key("delete") }, 30),
            Some(EditorAction::DeleteForward(Granularity::Line))
        );
    }

    #[test]
    fn editing_commands() {
        assert_eq!(standard_action(&key("enter"), 30), Some(EditorAction::Newline));
        assert_eq!(standard_action(&key("tab"), 30), Some(EditorAction::Indent));
        assert_eq!(standard_action(&KeyInput { shift: true, ..key("tab") }, 30), Some(EditorAction::Outdent));
        assert_eq!(standard_action(&KeyInput { cmd: true, ..key("z") }, 30), Some(EditorAction::Undo));
        assert_eq!(standard_action(&KeyInput { cmd: true, shift: true, ..key("z") }, 30), Some(EditorAction::Redo));
        assert_eq!(standard_action(&KeyInput { cmd: true, ..key("x") }, 30), Some(EditorAction::Cut));
    }

    #[test]
    fn printable_text_inserts_and_command_chords_do_not() {
        assert_eq!(
            standard_action(&KeyInput { text: Some("a"), ..key("a") }, 30),
            Some(EditorAction::InsertText("a".into()))
        );
        assert_eq!(
            standard_action(&KeyInput { text: Some("#"), alt: true, ..key("3") }, 30),
            Some(EditorAction::InsertText("#".into()))
        );
        assert_eq!(standard_action(&KeyInput { text: Some("q"), cmd: true, ..key("q") }, 30), None);
        assert_eq!(standard_action(&KeyInput { text: Some("\u{1b}"), ..key("escape") }, 30), None);
    }
}
```

In `io.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_and_save_round_trip_crlf() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "one\r\ntwo\r\n").expect("write");
        let buffer = load(&path).expect("load");
        assert_eq!(buffer.rope().to_string(), "one\ntwo\n");
        save(&path, &buffer).expect("save");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "one\r\ntwo\r\n");
    }

    #[test]
    fn binary_and_non_utf8_files_are_rejected() {
        let dir = TempDir::new().expect("tempdir");
        let binary = dir.path().join("bin");
        std::fs::write(&binary, [0x7f, 0x45, 0x00, 0x01]).expect("write");
        assert!(matches!(load(&binary), Err(LoadError::Binary)));
        let latin1 = dir.path().join("latin1");
        std::fs::write(&latin1, [0x63, 0x61, 0x66, 0xe9]).expect("write");
        assert!(matches!(load(&latin1), Err(LoadError::NotUtf8)));
    }

    #[cfg(unix)]
    #[test]
    fn save_through_symlink_writes_the_target() {
        let dir = TempDir::new().expect("tempdir");
        let target = dir.path().join("target.txt");
        let link = dir.path().join("link.txt");
        std::fs::write(&target, "old").expect("write");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        save(&link, &Buffer::from_text("new")).expect("save");
        assert!(std::fs::symlink_metadata(&link).expect("meta").file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "new");
    }

    #[cfg(unix)]
    #[test]
    fn save_keeps_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("script.sh");
        std::fs::write(&path, "echo").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        save(&path, &Buffer::from_text("echo hi")).expect("save");
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }

    #[cfg(unix)]
    #[test]
    fn failed_save_keeps_original_and_leaves_no_temp_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().expect("tempdir");
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).expect("mkdir");
        let path = locked.join("a.txt");
        std::fs::write(&path, "original").expect("write");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let result = save(&path, &Buffer::from_text("changed"));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "original");
        assert_eq!(std::fs::read_dir(&locked).expect("list").count(), 1);
    }
}
```

- [ ] **Step 2: Run and confirm failure.**

- [ ] **Step 3: Implement `keys.rs`:**

```rust
use crate::editor::{EditorAction, Granularity};
use crate::motion::Motion;

pub struct KeyInput<'a> {
    pub key: &'a str,
    pub text: Option<&'a str>,
    pub cmd: bool,
    pub alt: bool,
    pub shift: bool,
    pub ctrl: bool,
}

pub fn standard_action(input: &KeyInput<'_>, page_lines: usize) -> Option<EditorAction> {
    let movement = |motion| Some(EditorAction::Move { motion, extend: input.shift });
    match input.key {
        "left" if input.cmd => movement(Motion::LineStart),
        "right" if input.cmd => movement(Motion::LineEnd),
        "up" if input.cmd => movement(Motion::DocumentStart),
        "down" if input.cmd => movement(Motion::DocumentEnd),
        "left" if input.alt => movement(Motion::WordLeft),
        "right" if input.alt => movement(Motion::WordRight),
        "left" => movement(Motion::Left),
        "right" => movement(Motion::Right),
        "up" => movement(Motion::Up),
        "down" => movement(Motion::Down),
        "home" => movement(Motion::LineStart),
        "end" => movement(Motion::LineEnd),
        "pageup" => movement(Motion::PageUp(page_lines)),
        "pagedown" => movement(Motion::PageDown(page_lines)),
        "backspace" => Some(EditorAction::DeleteBackward(granularity(input))),
        "delete" => Some(EditorAction::DeleteForward(granularity(input))),
        "enter" => Some(EditorAction::Newline),
        "tab" if input.shift => Some(EditorAction::Outdent),
        "tab" => Some(EditorAction::Indent),
        "z" if input.cmd && input.shift => Some(EditorAction::Redo),
        "z" if input.cmd => Some(EditorAction::Undo),
        "x" if input.cmd => Some(EditorAction::Cut),
        _ if input.cmd || input.ctrl => None,
        _ => input
            .text
            .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
            .map(|text| EditorAction::InsertText(text.to_string())),
    }
}

fn granularity(input: &KeyInput<'_>) -> Granularity {
    if input.cmd {
        Granularity::Line
    } else if input.alt {
        Granularity::Word
    } else {
        Granularity::Char
    }
}
```

`io.rs`:

```rust
use crate::buffer::Buffer;
use std::io::Write;
use std::path::Path;

const BINARY_SNIFF_BYTES: usize = 8192;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("binary file")]
    Binary,
    #[error("not UTF-8 text")]
    NotUtf8,
}

pub fn load(path: &Path) -> Result<Buffer, LoadError> {
    let bytes = std::fs::read(path)?;
    if bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0) {
        return Err(LoadError::Binary);
    }
    let text = String::from_utf8(bytes).map_err(|_| LoadError::NotUtf8)?;
    Ok(Buffer::from_text(&text))
}

pub fn save(path: &Path, buffer: &Buffer) -> std::io::Result<()> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let directory = target.parent().unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(buffer.to_saved_text().as_bytes())?;
    file.as_file().sync_all()?;
    if let Ok(metadata) = std::fs::metadata(&target) {
        std::fs::set_permissions(file.path(), metadata.permissions())?;
    }
    file.persist(&target).map_err(|err| err.error)?;
    Ok(())
}
```

- [ ] **Step 4: Run.** `cargo test -p editor-core` → all pass, and clippy is clean. If `failed_save_keeps_original_and_leaves_no_temp_file` fails only because the tests run as root (CI containers), mark it `#[ignore]` with a test name ending `_unless_root`. Don't use a comment for this. Locally on macOS it must pass.

- [ ] **Step 5: Commit.**

```bash
git add crates/editor-core
git commit -m "feat(editor-core): add standard key mapping and atomic load and save"
```

---

### Task 8: Performance check

**Files:**
- Create: `crates/editor-core/tests/perf.rs`

**Interfaces:**
- Consumes: the public API only.

- [ ] **Step 1: Write the test:**

```rust
use editor_core::{Buffer, Editor, EditorAction, Selection, Selections};
use std::time::{Duration, Instant};

const TARGET_BYTES: usize = 10 * 1024 * 1024;
const EDITS: u32 = 1000;
const LOAD_BUDGET: Duration = Duration::from_millis(100);
const EDIT_BUDGET: Duration = Duration::from_millis(1);
const SAMPLE_LINE: &str = "fn example_function(argument: usize) -> usize { argument * 2 }\n";

#[test]
#[ignore]
fn ten_megabyte_buffer_stays_within_budget() {
    let text = SAMPLE_LINE.repeat(TARGET_BYTES / SAMPLE_LINE.len());
    let load_started = Instant::now();
    let buffer = Buffer::from_text(&text);
    let load_time = load_started.elapsed();
    assert!(load_time < LOAD_BUDGET, "load took {load_time:?}");

    let middle = buffer.len_chars() / 2;
    let mut editor = Editor::new(buffer);
    editor.set_selections(Selections::single(Selection::cursor(middle)));
    let edits_started = Instant::now();
    for _ in 0..EDITS {
        editor.apply(EditorAction::InsertText("x".into()), Instant::now());
    }
    let per_edit = edits_started.elapsed() / EDITS;
    assert!(per_edit < EDIT_BUDGET, "edit took {per_edit:?}");
}
```

- [ ] **Step 2: Run in release.** `cargo test --release -p editor-core --test perf -- --ignored`. Expected: PASS. Record the measured times in the report: add `-- --nocapture` and print them with `eprintln!` temporarily, or read them from a deliberately failing assert during development. Don't commit any print statement.

- [ ] **Step 3: Full verification and commit.**

```bash
cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings
git add crates/editor-core/tests/perf.rs
git commit -m "test(editor-core): add 10 MB edit and load budget check"
```
