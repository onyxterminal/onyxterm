use crate::buffer::line_len;
use crate::rope_compare::{common_prefix_bytes, common_suffix_bytes};
use ropey::{Rope, RopeSlice};
use std::collections::HashMap;
use std::ops::Range;

const MAX_EDIT_DISTANCE: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineMark {
    pub line: usize,
    pub kind: HunkKind,
}

impl Hunk {
    pub fn kind(&self) -> HunkKind {
        match (self.old.is_empty(), self.new.is_empty()) {
            (true, _) => HunkKind::Added,
            (false, true) => HunkKind::Deleted,
            (false, false) => HunkKind::Modified,
        }
    }
}

#[derive(Clone, Copy)]
pub struct DiffSource<'a> {
    pub saved: &'a Rope,
    pub current: &'a Rope,
    pub version: u64,
    pub saved_version: u64,
}

#[derive(Debug, Default)]
pub struct LineDiff {
    revision: Option<(u64, u64)>,
    hunks: Vec<Hunk>,
}

impl LineDiff {
    pub fn update(&mut self, source: DiffSource<'_>) {
        let revision = Some((source.version, source.saved_version));
        if self.revision == revision {
            return;
        }
        self.revision = revision;
        self.hunks = diff_lines(source.saved, source.current);
    }

    pub fn hunks(&self) -> &[Hunk] {
        &self.hunks
    }

    pub fn marks(&self, lines: Range<usize>) -> Vec<LineMark> {
        line_marks(&self.hunks, lines)
    }
}

pub fn line_marks(hunks: &[Hunk], lines: Range<usize>) -> Vec<LineMark> {
    let first = hunks.partition_point(|hunk| hunk.new.end.max(hunk.new.start + 1) <= lines.start);
    let mut marks = Vec::new();
    for hunk in hunks[first..].iter().take_while(|hunk| hunk.new.start <= lines.end) {
        let kind = hunk.kind();
        if kind == HunkKind::Deleted {
            marks.push(LineMark { line: hunk.new.start, kind });
            continue;
        }
        let start = hunk.new.start.max(lines.start);
        let end = hunk.new.end.min(lines.end);
        marks.extend((start..end).map(|line| LineMark { line, kind }));
    }
    marks
}

pub fn diff_lines(old: &Rope, new: &Rope) -> Vec<Hunk> {
    let (old_count, new_count) = (old.len_lines(), new.len_lines());
    let prefix_bytes = common_prefix_bytes(old, new);
    let prefix = old.byte_to_line(prefix_bytes).min(new.byte_to_line(prefix_bytes));
    let suffix_bytes = common_suffix_bytes(old, new).min(old.len_bytes().min(new.len_bytes()) - prefix_bytes);
    let suffix_start_line = old.byte_to_line(old.len_bytes() - suffix_bytes);
    let suffix = (old_count - 1 - suffix_start_line).min(old_count.min(new_count) - prefix);
    let old_middle = prefix..old_count - suffix;
    let new_middle = prefix..new_count - suffix;
    if old_middle.is_empty() && new_middle.is_empty() {
        return Vec::new();
    }
    let (old_ids, new_ids) = intern_lines(old, old_middle.clone(), new, new_middle.clone());
    let Some(script) = shortest_edit_script(&old_ids, &new_ids, MAX_EDIT_DISTANCE) else {
        return vec![Hunk { old: old_middle, new: new_middle }];
    };
    group_hunks(&script).into_iter().map(|hunk| shift_hunk(hunk, prefix)).collect()
}

fn line_text(rope: &Rope, line: usize) -> RopeSlice<'_> {
    rope.line(line).slice(..line_len(rope, line))
}

fn intern_lines(old: &Rope, old_lines: Range<usize>, new: &Rope, new_lines: Range<usize>) -> (Vec<u32>, Vec<u32>) {
    let mut ids: HashMap<String, u32> = HashMap::new();
    let mut id_of = |rope: &Rope, line: usize| {
        let next = ids.len() as u32;
        *ids.entry(line_text(rope, line).to_string()).or_insert(next)
    };
    let old_ids = old_lines.map(|line| id_of(old, line)).collect();
    let new_ids = new_lines.map(|line| id_of(new, line)).collect();
    (old_ids, new_ids)
}

struct EditScript {
    is_deleted: Vec<bool>,
    is_inserted: Vec<bool>,
}

fn shortest_edit_script(old: &[u32], new: &[u32], max_distance: usize) -> Option<EditScript> {
    let (old_len, new_len) = (old.len() as isize, new.len() as isize);
    let limit = max_distance.min(old.len() + new.len()) as isize;
    let offset = limit + 1;
    let mut furthest = vec![0isize; 2 * offset as usize + 1];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    for distance in 0..=limit {
        for diagonal in (-distance..=distance).step_by(2) {
            let index = (diagonal + offset) as usize;
            let is_down = diagonal == -distance || (diagonal != distance && furthest[index - 1] < furthest[index + 1]);
            let mut x = if is_down { furthest[index + 1] } else { furthest[index - 1] + 1 };
            let mut y = x - diagonal;
            while x < old_len && y < new_len && old[x as usize] == new[y as usize] {
                x += 1;
                y += 1;
            }
            furthest[index] = x;
            if x >= old_len && y >= new_len {
                trace.push(furthest[(offset - distance) as usize..=(offset + distance) as usize].to_vec());
                return Some(backtrack(&trace, old.len(), new.len()));
            }
        }
        trace.push(furthest[(offset - distance) as usize..=(offset + distance) as usize].to_vec());
    }
    None
}

fn backtrack(trace: &[Vec<isize>], old_len: usize, new_len: usize) -> EditScript {
    let mut script = EditScript { is_deleted: vec![false; old_len], is_inserted: vec![false; new_len] };
    let (mut x, mut y) = (old_len as isize, new_len as isize);
    for distance in (1..trace.len() as isize).rev() {
        let previous = &trace[distance as usize - 1];
        let at = |diagonal: isize| previous[(diagonal + distance - 1) as usize];
        let diagonal = x - y;
        let is_down = diagonal == -distance || (diagonal != distance && at(diagonal - 1) < at(diagonal + 1));
        let previous_diagonal = if is_down { diagonal + 1 } else { diagonal - 1 };
        let previous_x = at(previous_diagonal);
        let previous_y = previous_x - previous_diagonal;
        if is_down {
            script.is_inserted[previous_y as usize] = true;
        } else {
            script.is_deleted[previous_x as usize] = true;
        }
        x = previous_x;
        y = previous_y;
    }
    script
}

fn group_hunks(script: &EditScript) -> Vec<Hunk> {
    let (old_len, new_len) = (script.is_deleted.len(), script.is_inserted.len());
    let (mut old, mut new) = (0, 0);
    let mut hunks = Vec::new();
    while old < old_len || new < new_len {
        let is_deleted = |old: usize| old < old_len && script.is_deleted[old];
        let is_inserted = |new: usize| new < new_len && script.is_inserted[new];
        if !is_deleted(old) && !is_inserted(new) {
            old += 1;
            new += 1;
            continue;
        }
        let (old_start, new_start) = (old, new);
        while is_deleted(old) || is_inserted(new) {
            if is_deleted(old) {
                old += 1;
            } else {
                new += 1;
            }
        }
        hunks.push(Hunk { old: old_start..old, new: new_start..new });
    }
    hunks
}

fn shift_hunk(hunk: Hunk, lines: usize) -> Hunk {
    Hunk { old: hunk.old.start + lines..hunk.old.end + lines, new: hunk.new.start + lines..hunk.new.end + lines }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Buffer;

    fn diff(old: &str, new: &str) -> Vec<Hunk> {
        diff_lines(&Rope::from_str(old), &Rope::from_str(new))
    }

    fn hunk(old: Range<usize>, new: Range<usize>) -> Hunk {
        Hunk { old, new }
    }

    #[test]
    fn identical_text_has_no_hunks() {
        assert_eq!(diff("a\nb\n", "a\nb\n"), vec![]);
        assert_eq!(diff("", ""), vec![]);
    }

    #[test]
    fn inserted_lines_are_added() {
        let hunks = diff("a\nb\nc", "a\nx\ny\nb\nc");
        assert_eq!(hunks, vec![hunk(1..1, 1..3)]);
        assert_eq!(hunks[0].kind(), HunkKind::Added);
    }

    #[test]
    fn removed_lines_are_deleted_at_their_boundary() {
        let hunks = diff("a\nb\nc\nd", "a\nd");
        assert_eq!(hunks, vec![hunk(1..3, 1..1)]);
        assert_eq!(hunks[0].kind(), HunkKind::Deleted);
    }

    #[test]
    fn changed_lines_are_modified() {
        let hunks = diff("a\nb\nc", "a\nB\nc");
        assert_eq!(hunks, vec![hunk(1..2, 1..2)]);
        assert_eq!(hunks[0].kind(), HunkKind::Modified);
    }

    #[test]
    fn separate_edits_make_separate_hunks() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8";
        let new = "1\nX\n3\n4\n5\n6\n7\nnew\n8";
        assert_eq!(diff(old, new), vec![hunk(1..2, 1..2), hunk(7..7, 7..8)]);
    }

    #[test]
    fn interleaved_edits_inside_the_middle_are_found() {
        let old = "a\nb\nc\nd\ne";
        let new = "a\nc\nX\nd\nY\ne";
        assert_eq!(diff(old, new), vec![hunk(1..2, 1..1), hunk(3..3, 2..3), hunk(4..4, 4..5)]);
    }

    #[test]
    fn appending_a_trailing_newline_adds_an_empty_line() {
        assert_eq!(diff("a", "a\n"), vec![hunk(1..1, 1..2)]);
    }

    #[test]
    fn edits_beyond_the_distance_cap_fall_back_to_one_hunk() {
        let old: Vec<u32> = (0..3).collect();
        let new: Vec<u32> = (10..13).collect();
        assert!(shortest_edit_script(&old, &new, 5).is_none());
        let old_text: String = (0..3000).map(|n| format!("old {n}\n")).collect();
        let new_text: String = (0..3000).map(|n| format!("new {n}\n")).collect();
        let text = |body: &str| format!("head\n{body}tail");
        assert_eq!(diff(&text(&old_text), &text(&new_text)), vec![hunk(1..3001, 1..3001)]);
    }

    #[test]
    fn crlf_buffers_diff_their_normalized_lines() {
        let mut buffer = Buffer::from_text("a\r\nb\r\nc\r\n");
        buffer.replace(2..3, "B");
        assert_eq!(diff_lines(buffer.saved_rope(), buffer.rope()), vec![hunk(1..2, 1..2)]);
    }

    fn lines_of(text: &str) -> Vec<String> {
        let rope = Rope::from_str(text);
        (0..rope.len_lines()).map(|line| line_text(&rope, line).to_string()).collect()
    }

    fn rebuilt(old: &str, new: &str) -> Vec<String> {
        let (old_lines, new_lines) = (lines_of(old), lines_of(new));
        let mut result = Vec::new();
        let mut kept_through = 0;
        for hunk in diff(old, new) {
            result.extend_from_slice(&old_lines[kept_through..hunk.old.start]);
            result.extend_from_slice(&new_lines[hunk.new.clone()]);
            kept_through = hunk.old.end;
        }
        result.extend_from_slice(&old_lines[kept_through..]);
        result
    }

    #[test]
    fn applying_the_hunks_to_the_old_lines_yields_the_new_lines() {
        let mut seed: u64 = 7;
        let mut next = move |bound: u64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) % bound
        };
        for _ in 0..300 {
            let old: String = (0..next(12)).map(|_| format!("{}\n", next(4))).collect();
            let mut new = old.clone();
            for _ in 0..next(4) {
                let at = next(new.len() as u64 + 1) as usize;
                let end = (at + next(5) as usize).min(new.len());
                new.replace_range(at..end, ["x", "\n", "1\n", ""][next(4) as usize]);
            }
            assert_eq!(rebuilt(&old, &new), lines_of(&new), "{old:?} -> {new:?}");
        }
    }

    #[test]
    fn marks_cover_visible_lines_of_each_hunk() {
        let hunks = vec![hunk(1..1, 1..3), hunk(4..5, 5..6), hunk(7..9, 8..8), hunk(10..10, 10..14)];
        let marks = line_marks(&hunks, 2..12);
        let mark = |line, kind| LineMark { line, kind };
        assert_eq!(marks, vec![
            mark(2, HunkKind::Added),
            mark(5, HunkKind::Modified),
            mark(8, HunkKind::Deleted),
            mark(10, HunkKind::Added),
            mark(11, HunkKind::Added),
        ]);
    }

    #[test]
    fn deletion_at_the_end_marks_the_boundary_below_the_last_line() {
        let hunks = diff("a\nb\nc", "a");
        assert_eq!(line_marks(&hunks, 0..1), vec![LineMark { line: 1, kind: HunkKind::Deleted }]);
        assert_eq!(line_marks(&hunks, 2..5), vec![]);
    }

    #[test]
    fn the_cache_recomputes_only_when_a_version_changes() {
        let saved = Rope::from_str("a\nb");
        let mut current = Rope::from_str("a\nB");
        let mut cache = LineDiff::default();
        cache.update(DiffSource { saved: &saved, current: &current, version: 1, saved_version: 0 });
        assert_eq!(cache.hunks(), &[hunk(1..2, 1..2)]);
        current = Rope::from_str("a\nb");
        cache.update(DiffSource { saved: &saved, current: &current, version: 1, saved_version: 0 });
        assert_eq!(cache.hunks().len(), 1);
        cache.update(DiffSource { saved: &saved, current: &current, version: 2, saved_version: 0 });
        assert!(cache.hunks().is_empty());
    }
}
