use editor_core::syntax::{HighlightSource, Language, LineStates};
use editor_core::{diff_lines, Buffer, Editor, EditorAction, Rope, Selection, Selections};
use std::time::{Duration, Instant};

const TARGET_BYTES: usize = 10 * 1024 * 1024;
const EDITS: u32 = 1000;
const LOAD_BUDGET: Duration = Duration::from_millis(100);
const EDIT_BUDGET: Duration = Duration::from_millis(1);
const SAMPLE_LINE: &str = "fn example_function(argument: usize) -> usize { argument * 2 }\n";
const HIGHLIGHTED_LINES: usize = 20_000;
const HIGHLIGHT_BUDGET: Duration = Duration::from_millis(100);
const REHIGHLIGHT_BUDGET: Duration = Duration::from_millis(5);
const LINES_FROM_END: usize = 10;
const DIFFED_LINES: usize = 20_000;
const DIFF_BUDGET: Duration = Duration::from_millis(5);
const RUST_SAMPLE: &str = r#"/// Documented item with a block comment below.
/* multi-line
   block comment */
pub fn render(frame: &Frame, count: usize) -> Result<Vec<Span>, Error> {
    let label = format!("{count} items, escaped \" quote");
    if count > 0x1F && frame.is_ready() { return Ok(Vec::new()); }
    let raw = r"raw \ text";
    'outer: for index in 0..count { println!("{index}"); }
    Err(Error::Empty)
}
"#;

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

fn rust_source(rope: &Rope, version: u64) -> HighlightSource<'_> {
    HighlightSource { rope, version, language: Some(Language::Rust) }
}

#[test]
#[ignore]
fn highlighting_twenty_thousand_rust_lines_stays_within_budget() {
    let text = RUST_SAMPLE.repeat(HIGHLIGHTED_LINES / RUST_SAMPLE.lines().count());
    let mut rope = Rope::from_str(&text);
    let last_line = rope.len_lines() - 1;
    let mut states = LineStates::default();

    let full_started = Instant::now();
    states.update(rust_source(&rope, 0), last_line);
    let full_time = full_started.elapsed();
    assert!(full_time < HIGHLIGHT_BUDGET, "full highlight took {full_time:?}");

    rope.insert(rope.line_to_char(last_line - LINES_FROM_END), "x");
    let edit_started = Instant::now();
    states.update(rust_source(&rope, 1), last_line);
    let edit_time = edit_started.elapsed();
    assert!(edit_time < REHIGHLIGHT_BUDGET, "rehighlight after an edit took {edit_time:?}");
}

#[test]
#[ignore]
fn diffing_twenty_thousand_lines_after_a_small_edit_stays_within_budget() {
    let saved = Rope::from_str(&SAMPLE_LINE.repeat(DIFFED_LINES));
    let mut current = saved.clone();
    current.insert(current.line_to_char(DIFFED_LINES / 2), "x");
    let started = Instant::now();
    let hunks = diff_lines(&saved, &current);
    let diff_time = started.elapsed();
    assert_eq!(hunks.len(), 1);
    assert!(diff_time < DIFF_BUDGET, "diff took {diff_time:?}");
}
