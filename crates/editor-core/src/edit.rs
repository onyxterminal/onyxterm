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
        changes.sort_by_key(|change| (change.range.start, change.range.end));
        let mut merged: Vec<Change> = Vec::with_capacity(changes.len());
        for change in changes {
            match merged.last_mut() {
                Some(last) if overlaps(last, &change) => {
                    last.range.end = last.range.end.max(change.range.end);
                    last.text.push_str(&change.text);
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

    pub fn apply(&self, buffer: &mut Buffer) -> Edit {
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

fn overlaps(earlier: &Change, later: &Change) -> bool {
    later.range.start < earlier.range.end
        || (later.range.start == earlier.range.start && later.range.end > later.range.start)
}

fn offset_by(offset: usize, delta: isize) -> usize {
    offset.checked_add_signed(delta).unwrap_or(0)
}

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

    #[test]
    fn merging_keeps_both_texts_regardless_of_input_order() {
        let expected = [change(1..3, "ba")];
        assert_eq!(Edit::new(vec![change(1..3, "a"), change(1..1, "b")]).changes(), &expected);
        assert_eq!(Edit::new(vec![change(1..1, "b"), change(1..3, "a")]).changes(), &expected);
    }

    #[test]
    fn insert_inside_a_replaced_range_is_kept() {
        let edit = Edit::new(vec![change(0..5, "x"), change(2..2, "Y")]);
        assert_eq!(edit.changes(), &[change(0..5, "xY")]);
    }
}
