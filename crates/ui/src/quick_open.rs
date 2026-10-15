#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::git_status::GitStatus;
use crate::pane_menu::step_selection;
use crate::palette::PaletteRow;
use crate::text_field::TextField;
use std::path::PathBuf;
use std::sync::Arc;

pub const LISTED_FILE_LIMIT: usize = 200_000;
pub const RESULT_LIMIT: usize = 200;
const MATCH_SCORE: i64 = 16;
const CONSECUTIVE_BONUS: i64 = 24;
const BOUNDARY_BONUS: i64 = 20;
const GAP_PENALTY: i64 = 1;
const NAME_MATCH_BONUS: i64 = 400;
const NAME_PREFIX_BONUS: i64 = 200;
const SEGMENT_BOUNDARIES: [char; 5] = ['/', '_', '-', '.', ' '];

pub fn rank_paths(paths: &[String], query: &str, limit: usize) -> Vec<usize> {
    let needle: Vec<char> = query.to_lowercase().chars().filter(|ch| !ch.is_whitespace()).collect();
    if needle.is_empty() {
        return (0..paths.len().min(limit)).collect();
    }
    let mut scored: Vec<(i64, usize)> =
        paths.iter().enumerate().filter_map(|(index, path)| Some((score_path(path, &needle)?, index))).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(paths[a.1].len().cmp(&paths[b.1].len())).then(a.1.cmp(&b.1)));
    scored.into_iter().take(limit).map(|(_, index)| index).collect()
}

fn score_path(path: &str, needle: &[char]) -> Option<i64> {
    let lower: Vec<char> = path.to_lowercase().chars().collect();
    let name_start = lower.iter().rposition(|ch| *ch == '/').map_or(0, |slash| slash + 1);
    name_score(&lower, name_start, needle).or_else(|| subsequence_score(&lower, 0, needle))
}

fn name_score(lower: &[char], name_start: usize, needle: &[char]) -> Option<i64> {
    let split = needle.iter().rposition(|ch| *ch == '/');
    let (folder_query, name_query) = match split {
        Some(slash) => (&needle[..slash], &needle[slash + 1..]),
        None => (&needle[..0], needle),
    };
    if name_query.is_empty() {
        return None;
    }
    let folder = if folder_query.is_empty() { 0 } else { subsequence_score(&lower[..name_start], 0, folder_query)? };
    let name = subsequence_score(lower, name_start, name_query)?;
    let is_prefix = lower[name_start..].starts_with(name_query);
    Some(folder + name + NAME_MATCH_BONUS + if is_prefix { NAME_PREFIX_BONUS } else { 0 })
}

fn subsequence_score(text: &[char], from: usize, needle: &[char]) -> Option<i64> {
    let mut score = 0;
    let mut cursor = from;
    let mut previous: Option<usize> = None;
    for ch in needle {
        let found = cursor + text[cursor..].iter().position(|candidate| candidate == ch)?;
        score += MATCH_SCORE;
        let is_boundary = found == 0 || SEGMENT_BOUNDARIES.contains(&text[found - 1]);
        if is_boundary {
            score += BOUNDARY_BONUS;
        }
        match previous {
            Some(last) if last + 1 == found => score += CONSECUTIVE_BONUS,
            Some(last) => score -= (found - last - 1) as i64 * GAP_PENALTY,
            None => score -= (found - from) as i64 * GAP_PENALTY,
        }
        previous = Some(found);
        cursor = found + 1;
    }
    Some(score)
}

#[derive(Debug)]
pub struct QuickOpen {
    pub root: PathBuf,
    pub query: TextField,
    pub files: Arc<Vec<String>>,
    pub results: Arc<Vec<usize>>,
    pub selection: usize,
    pub git: Option<Arc<GitStatus>>,
}

impl QuickOpen {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            query: TextField::default(),
            files: Arc::default(),
            results: Arc::default(),
            selection: 0,
            git: None,
        }
    }

    pub fn move_selection(&mut self, step: isize) {
        self.selection = step_selection(self.selection, step, self.results.len());
    }

    pub fn accept_results(&mut self, results: Arc<Vec<usize>>) {
        self.results = results;
        self.selection = 0;
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        let index = *self.results.get(self.selection)?;
        Some(self.root.join(self.files.get(index)?))
    }

    pub fn rows(&self) -> Vec<PaletteRow> {
        self.results.iter().filter_map(|index| self.files.get(*index)).map(|path| self.row(path)).collect()
    }

    fn row(&self, path: &str) -> PaletteRow {
        let (directory, name) = path.rsplit_once('/').unwrap_or(("", path));
        let mark = self.git.as_ref().and_then(|git| git.file_mark(&self.root.join(path)));
        PaletteRow {
            title: name.to_string(),
            category: directory.to_string(),
            shortcut: mark.map(|mark| mark.letter().to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_status::parse_porcelain;
    use std::path::Path;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|path| path.to_string()).collect()
    }

    fn ranked<'a>(list: &'a [String], query: &str) -> Vec<&'a str> {
        rank_paths(list, query, RESULT_LIMIT).into_iter().map(|index| list[index].as_str()).collect()
    }

    const PROJECT: [&str; 6] = [
        "crates/ui/src/settings.rs",
        "crates/ui/src/shell/splits.rs",
        "crates/ui/src/shell.rs",
        "assets/themes/onyx-dark.toml",
        "README.md",
        "crates/ui/src/keymap.rs",
    ];

    #[test]
    fn file_name_matches_outrank_scattered_path_matches() {
        let list = paths(&PROJECT);
        assert_eq!(ranked(&list, "shell")[..2], ["crates/ui/src/shell.rs", "crates/ui/src/shell/splits.rs"]);
        assert_eq!(ranked(&list, "sett")[0], "crates/ui/src/settings.rs");
        assert_eq!(ranked(&list, "km")[0], "crates/ui/src/keymap.rs");
    }

    #[test]
    fn queries_match_as_case_insensitive_subsequences() {
        let list = paths(&PROJECT);
        assert_eq!(ranked(&list, "READ"), ["README.md"]);
        assert_eq!(ranked(&list, "ui/sp")[0], "crates/ui/src/shell/splits.rs");
        assert!(ranked(&list, "zzz").is_empty());
    }

    #[test]
    fn an_empty_query_lists_files_in_order_up_to_the_limit() {
        let list = paths(&PROJECT);
        assert_eq!(rank_paths(&list, "  ", 3), [0, 1, 2]);
    }

    #[test]
    fn ranking_fifty_thousand_paths_stays_interactive() {
        let list: Vec<String> = (0..50_000).map(|index| format!("crates/module_{index}/src/file_{index}.rs")).collect();
        let started = std::time::Instant::now();
        let found = rank_paths(&list, "mod42file", RESULT_LIMIT);
        assert!(started.elapsed() < std::time::Duration::from_secs(1), "took {:?}", started.elapsed());
        assert_eq!(found.len(), RESULT_LIMIT);
    }

    #[test]
    fn rows_split_name_and_folder_and_carry_git_letters() {
        let mut quick_open = QuickOpen::new(PathBuf::from("/r"));
        quick_open.files = Arc::new(paths(&["crates/ui/src/shell.rs", "README.md"]));
        quick_open.accept_results(Arc::new(vec![0, 1]));
        let porcelain = parse_porcelain(" M crates/ui/src/shell.rs\0");
        quick_open.git = Some(Arc::new(GitStatus::from_porcelain(Path::new("/r"), porcelain)));
        let rows = quick_open.rows();
        let shortcut = Some("M".to_string());
        let shell = PaletteRow { title: "shell.rs".into(), category: "crates/ui/src".into(), shortcut };
        assert_eq!(rows[0], shell);
        assert_eq!(rows[1], PaletteRow { title: "README.md".into(), category: String::new(), shortcut: None });
        quick_open.move_selection(1);
        assert_eq!(quick_open.selected_path(), Some(PathBuf::from("/r/README.md")));
    }
}
