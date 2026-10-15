#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::pane_menu::step_selection;
use crate::text_field::TextField;
use domain::{DocumentId, PanelId};
use std::ops::Range;
use std::sync::Arc;
use terminal_core::TextMatch;

pub const MATCH_LIMIT: usize = 10_000;
const NO_RESULTS: &str = "No results";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentVersion {
    pub document: DocumentId,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindHits {
    None,
    Editor { searched: DocumentVersion, ranges: Arc<Vec<Range<usize>>> },
    Terminal(Arc<Vec<TextMatch>>),
}

impl FindHits {
    pub fn len(&self) -> usize {
        match self {
            FindHits::None => 0,
            FindHits::Editor { ranges, .. } => ranges.len(),
            FindHits::Terminal(matches) => matches.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn searched_document(&self) -> Option<DocumentVersion> {
        match self {
            FindHits::Editor { searched, .. } => Some(*searched),
            FindHits::None | FindHits::Terminal(_) => None,
        }
    }
}

#[derive(Debug)]
pub struct FindBar {
    pub panel: PanelId,
    pub input: TextField,
    pub is_focused: bool,
    pub hits: FindHits,
    pub current: usize,
}

impl FindBar {
    pub fn new(panel: PanelId) -> Self {
        Self { panel, input: TextField::default(), is_focused: true, hits: FindHits::None, current: 0 }
    }

    pub fn reopen(&mut self, panel: PanelId) {
        if self.panel != panel {
            *self = Self::new(panel);
            return;
        }
        self.is_focused = true;
        self.current = 0;
    }

    pub fn step(&mut self, step: isize) {
        self.current = step_selection(self.current, step, self.hits.len());
    }

    pub fn accept(&mut self, hits: FindHits) {
        self.current = self.current.min(hits.len().saturating_sub(1));
        self.hits = hits;
    }

    pub fn count_label(&self) -> String {
        count_label(self.input.is_empty(), self.current, self.hits.len())
    }

    pub fn current_range(&self) -> Option<Range<usize>> {
        match &self.hits {
            FindHits::Editor { ranges, .. } => ranges.get(self.current).cloned(),
            FindHits::None | FindHits::Terminal(_) => None,
        }
    }

    pub fn current_match(&self) -> Option<TextMatch> {
        match &self.hits {
            FindHits::Terminal(matches) => matches.get(self.current).copied(),
            FindHits::None | FindHits::Editor { .. } => None,
        }
    }
}

pub fn count_label(is_query_empty: bool, current: usize, total: usize) -> String {
    if is_query_empty {
        return String::new();
    }
    if total == 0 {
        return NO_RESULTS.to_string();
    }
    format!("{} of {total}", current + 1)
}

pub fn hits_on_line(ranges: &[Range<usize>], line: Range<usize>) -> impl Iterator<Item = (usize, Range<usize>)> + '_ {
    let first = ranges.partition_point(|range| range.end <= line.start);
    ranges[first..]
        .iter()
        .enumerate()
        .take_while(move |(_, range)| range.start < line.end)
        .map(move |(offset, range)| (first + offset, range.start.max(line.start)..range.end.min(line.end)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use terminal_core::MatchPoint;

    fn editor_hits(ranges: Vec<Range<usize>>) -> FindHits {
        let searched = DocumentVersion { document: DocumentId::new(), version: 0 };
        FindHits::Editor { searched, ranges: Arc::new(ranges) }
    }

    #[test]
    fn count_reads_n_of_m_no_results_or_nothing() {
        assert_eq!(count_label(false, 2, 7), "3 of 7");
        assert_eq!(count_label(false, 0, 0), "No results");
        assert_eq!(count_label(true, 0, 0), "");
    }

    #[test]
    fn stepping_wraps_in_both_directions() {
        let mut bar = FindBar::new(PanelId::new());
        bar.accept(editor_hits(vec![0..1, 4..5, 9..10]));
        bar.step(-1);
        assert_eq!(bar.current, 2);
        bar.step(1);
        assert_eq!(bar.current, 0);
        bar.step(1);
        assert_eq!(bar.current_range(), Some(4..5));
    }

    #[test]
    fn fewer_results_clamp_the_current_match() {
        let mut bar = FindBar::new(PanelId::new());
        bar.accept(editor_hits(vec![0..1, 4..5, 9..10]));
        bar.step(-1);
        bar.accept(editor_hits(std::iter::once(0..1).collect()));
        assert_eq!(bar.current, 0);
        bar.accept(FindHits::None);
        assert_eq!(bar.count_label(), "");
    }

    #[test]
    fn reopening_the_same_pane_keeps_the_query_and_restarts_at_the_first_match() {
        let panel = PanelId::new();
        let mut bar = FindBar::new(panel);
        bar.input = TextField::new("side");
        bar.current = 2;
        bar.is_focused = false;
        bar.reopen(panel);
        assert_eq!((bar.input.text(), bar.current, bar.is_focused), ("side", 0, true));
        bar.reopen(PanelId::new());
        assert!(bar.input.is_empty());
    }

    #[test]
    fn terminal_hits_expose_the_current_match() {
        let mut bar = FindBar::new(PanelId::new());
        let hit = TextMatch { start: MatchPoint { line: -3, col: 0 }, end: MatchPoint { line: -3, col: 4 } };
        bar.accept(FindHits::Terminal(Arc::new(vec![hit])));
        assert_eq!(bar.current_match(), Some(hit));
        assert_eq!(bar.current_range(), None);
    }

    #[test]
    fn hits_on_a_line_are_clipped_to_it_with_their_index() {
        let ranges = vec![0..3, 8..14, 20..22];
        let found: Vec<(usize, Range<usize>)> = hits_on_line(&ranges, 10..21).collect();
        assert_eq!(found, vec![(1, 10..14), (2, 20..21)]);
        assert_eq!(hits_on_line(&ranges, 3..8).count(), 0);
    }
}
