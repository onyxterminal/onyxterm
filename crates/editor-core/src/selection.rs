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
