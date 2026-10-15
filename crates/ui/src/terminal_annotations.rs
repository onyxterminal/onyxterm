use crate::file_links::{file_links, may_contain_link, FileLink};
use std::ops::Range;
use std::time::Duration;
use terminal_core::{BlockId, BlockOutcome, BlockStatus, GridSnapshot, Terminal};

const RUNNING_LABEL: &str = "running";
const INTERRUPTED_LABEL: &str = "interrupted";
const SHOWN_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowLink {
    pub row: usize,
    pub link: FileLink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockLabel {
    pub text: String,
    pub is_failure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowLabel {
    pub row: usize,
    pub label: BlockLabel,
}

#[derive(Debug, Default)]
pub struct Annotations {
    pub links: Vec<RowLink>,
    pub labels: Vec<RowLabel>,
    pub ghost: Option<String>,
}

impl Annotations {
    pub fn read(snapshot: &GridSnapshot, terminal: &Terminal, is_focused: bool) -> Self {
        let block_rows = first_block_rows(snapshot);
        let labels = match block_rows.is_empty() {
            true => Vec::new(),
            false => terminal.with_command_blocks(|blocks| {
                block_rows
                    .iter()
                    .filter_map(|(row, id)| {
                        let label = block_label(blocks.block(*id)?.status)?;
                        Some(RowLabel { row: *row, label })
                    })
                    .collect()
            }),
        };
        let ghost = is_focused.then(|| terminal.autosuggestion()).flatten();
        Self { links: row_links(snapshot), labels, ghost }
    }

    pub fn link_at(&self, row: usize, col: usize) -> Option<&RowLink> {
        self.links.iter().find(|found| found.row == row && found.link.columns.contains(&col))
    }

    pub fn link_columns_on(&self, row: usize) -> impl Iterator<Item = Range<usize>> + '_ {
        self.links.iter().filter(move |found| found.row == row).map(|found| found.link.columns.clone())
    }
}

pub fn block_label(status: BlockStatus) -> Option<BlockLabel> {
    let outcome = match status {
        BlockStatus::Running => return Some(quiet_label(RUNNING_LABEL.to_string())),
        BlockStatus::Finished(outcome) => outcome,
    };
    if outcome.is_interrupted() {
        return Some(quiet_label(INTERRUPTED_LABEL.to_string()));
    }
    if outcome.is_failure() {
        return Some(failure_label(outcome));
    }
    (outcome.duration >= SHOWN_DURATION).then(|| quiet_label(seconds(outcome.duration)))
}

fn quiet_label(text: String) -> BlockLabel {
    BlockLabel { text, is_failure: false }
}

fn failure_label(outcome: BlockOutcome) -> BlockLabel {
    let code = outcome.exit_code.unwrap_or_default();
    BlockLabel { text: format!("exit {code} · {}", seconds(outcome.duration)), is_failure: true }
}

fn seconds(duration: Duration) -> String {
    format!("{:.1}s", duration.as_secs_f64())
}

fn first_block_rows(snapshot: &GridSnapshot) -> Vec<(usize, BlockId)> {
    let cols = usize::from(snapshot.cols.max(1));
    let mut rows: Vec<(usize, BlockId)> = Vec::new();
    for (row, cells) in snapshot.cells.chunks(cols).enumerate() {
        for id in cells.iter().filter_map(|cell| cell.block) {
            if rows.iter().all(|(_, seen)| *seen != id) {
                rows.push((row, id));
            }
        }
    }
    rows
}

fn row_links(snapshot: &GridSnapshot) -> Vec<RowLink> {
    let cols = usize::from(snapshot.cols.max(1));
    let mut links = Vec::new();
    for (row, cells) in snapshot.cells.chunks(cols).enumerate() {
        if !may_contain_link(cells.iter().map(|cell| cell.ch)) {
            continue;
        }
        let chars: Vec<char> = cells.iter().map(|cell| cell.ch).collect();
        links.extend(file_links(&chars).into_iter().map(|link| RowLink { row, link }));
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use terminal_core::{Cell, GridSize};

    fn finished(exit_code: Option<i32>, millis: u64) -> BlockStatus {
        BlockStatus::Finished(BlockOutcome { exit_code, duration: Duration::from_millis(millis) })
    }

    fn label(text: &str, is_failure: bool) -> Option<BlockLabel> {
        Some(BlockLabel { text: text.into(), is_failure })
    }

    #[test]
    fn labels_follow_the_design_wording() {
        assert_eq!(block_label(BlockStatus::Running), label("running", false));
        assert_eq!(block_label(finished(Some(1), 2140)), label("exit 1 · 2.1s", true));
        assert_eq!(block_label(finished(Some(127), 40)), label("exit 127 · 0.0s", true));
        assert_eq!(block_label(finished(Some(130), 900)), label("interrupted", false));
        assert_eq!(block_label(finished(Some(0), 3810)), label("3.8s", false));
        assert_eq!(block_label(finished(Some(0), 999)), None);
        assert_eq!(block_label(finished(None, 40)), None);
    }

    fn snapshot_of(rows: &[&str], cols: u16) -> GridSnapshot {
        let mut snapshot = GridSnapshot::empty(GridSize::new(cols, rows.len() as u16));
        for (row, text) in rows.iter().enumerate() {
            for (col, ch) in text.chars().enumerate() {
                snapshot.cells[row * usize::from(cols) + col] = Cell { ch, ..Cell::empty() };
            }
        }
        snapshot
    }

    #[test]
    fn links_are_found_per_visible_row() {
        let snapshot = snapshot_of(&["running 42 tests", "  at crates/ui/src/settings.rs:15:9"], 40);
        let links = row_links(&snapshot);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].row, 1);
        assert_eq!(links[0].link.columns, 5..35);
        let annotations = Annotations { links, ..Annotations::default() };
        assert!(annotations.link_at(1, 5).is_some());
        assert!(annotations.link_at(1, 35).is_none());
        assert_eq!(annotations.link_columns_on(1).collect::<Vec<_>>(), vec![5..35]);
    }

    #[test]
    fn every_block_is_found_on_its_first_row_even_when_rows_share_ids() {
        let mut snapshot = snapshot_of(&["$ cargo", "test", "$ ls"], 10);
        for index in [2, 3, 10, 11] {
            snapshot.cells[index].block = Some(BlockId(4));
        }
        snapshot.cells[22].block = Some(BlockId(5));
        snapshot.cells[24].block = Some(BlockId(6));
        let expected = vec![(0, BlockId(4)), (2, BlockId(5)), (2, BlockId(6))];
        assert_eq!(first_block_rows(&snapshot), expected);
    }
}
