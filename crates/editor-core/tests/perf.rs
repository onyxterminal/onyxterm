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
