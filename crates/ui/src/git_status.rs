#![deny(warnings)]
#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const GIT_PROGRAM: &str = "git";
const BRANCH_HEADER: &str = "## ";
const UPSTREAM_SEPARATOR: &str = "...";
const NO_COMMITS_PREFIX: &str = "No commits yet on ";
const DETACHED_HEAD: &str = "HEAD (no branch)";
const DETACHED_LABEL: &str = "HEAD";
const UNMERGED_PAIRS: [&str; 7] = ["DD", "AU", "UD", "UA", "DU", "AA", "UU"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitMark {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Conflicted,
    ChangedInside,
}

impl GitMark {
    pub fn letter(self) -> &'static str {
        match self {
            GitMark::Modified => "M",
            GitMark::Added => "A",
            GitMark::Deleted => "D",
            GitMark::Renamed => "R",
            GitMark::Copied => "C",
            GitMark::Untracked => "U",
            GitMark::Conflicted => "!",
            GitMark::ChangedInside => "\u{2022}",
        }
    }

    pub fn is_addition(self) -> bool {
        matches!(self, GitMark::Added | GitMark::Untracked)
    }

    fn from_code(code: char) -> Option<Self> {
        match code {
            'M' | 'T' => Some(GitMark::Modified),
            'A' => Some(GitMark::Added),
            'D' => Some(GitMark::Deleted),
            'R' => Some(GitMark::Renamed),
            'C' => Some(GitMark::Copied),
            _ => None,
        }
    }

    fn from_pair(pair: &str) -> Option<Self> {
        if pair == "??" {
            return Some(GitMark::Untracked);
        }
        if UNMERGED_PAIRS.contains(&pair) {
            return Some(GitMark::Conflicted);
        }
        let mut codes = pair.chars();
        let (index, worktree) = (codes.next()?, codes.next()?);
        if index == 'A' {
            return Some(GitMark::Added);
        }
        GitMark::from_code(worktree).or_else(|| GitMark::from_code(index))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Porcelain {
    pub branch: Option<String>,
    pub marks: Vec<(String, GitMark)>,
}

pub fn parse_porcelain(output: &str) -> Porcelain {
    let mut porcelain = Porcelain::default();
    let mut records = output.split('\0').filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        if let Some(header) = record.strip_prefix(BRANCH_HEADER) {
            porcelain.branch = branch_name(header);
            continue;
        }
        let (Some(pair), Some(path)) = (record.get(..2), record.get(3..)) else {
            continue;
        };
        if pair.starts_with('R') || pair.starts_with('C') {
            records.next();
        }
        if let Some(mark) = GitMark::from_pair(pair) {
            porcelain.marks.push((path.to_string(), mark));
        }
    }
    porcelain
}

fn branch_name(header: &str) -> Option<String> {
    if header == DETACHED_HEAD {
        return Some(DETACHED_LABEL.to_string());
    }
    let local = header.strip_prefix(NO_COMMITS_PREFIX).unwrap_or(header);
    let name = local.split(UPSTREAM_SEPARATOR).next()?.split(' ').next()?;
    (!name.is_empty()).then(|| name.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitStatus {
    pub branch: Option<String>,
    marks: HashMap<PathBuf, GitMark>,
    changed_dirs: HashSet<PathBuf>,
}

impl GitStatus {
    pub fn from_porcelain(repo_root: &Path, porcelain: Porcelain) -> Self {
        let marks: HashMap<PathBuf, GitMark> =
            porcelain.marks.into_iter().map(|(path, mark)| (repo_root.join(path), mark)).collect();
        let changed_dirs = marks
            .keys()
            .flat_map(|path| path.ancestors().skip(1).take_while(|dir| dir.starts_with(repo_root)))
            .map(Path::to_path_buf)
            .collect();
        Self { branch: porcelain.branch, marks, changed_dirs }
    }

    pub fn file_mark(&self, path: &Path) -> Option<GitMark> {
        self.marks.get(path).copied()
    }

    pub fn dir_mark(&self, path: &Path, is_expanded: bool) -> Option<GitMark> {
        let has_changes = !is_expanded && self.changed_dirs.contains(path);
        has_changes.then_some(GitMark::ChangedInside)
    }
}

pub fn read_git_status(dir: &Path) -> Option<GitStatus> {
    let root = git_output(dir, &["rev-parse", "--show-toplevel"])?;
    let root = PathBuf::from(root.trim_end());
    let status = git_output(&root, &["status", "--porcelain=v1", "-z", "--branch", "--untracked-files=all"])?;
    Some(GitStatus::from_porcelain(&root_as_spelled(dir, &root), parse_porcelain(&status)))
}

fn root_as_spelled(dir: &Path, canonical_root: &Path) -> PathBuf {
    let depth = dir
        .canonicalize()
        .ok()
        .and_then(|canonical| canonical.strip_prefix(canonical_root).ok().map(|inner| inner.components().count()));
    match depth.and_then(|depth| dir.ancestors().nth(depth)) {
        Some(root) => root.to_path_buf(),
        None => canonical_root.to_path_buf(),
    }
}

fn git_output(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(GIT_PROGRAM).arg("-C").arg(dir).args(args).output();
    match output {
        Ok(output) if output.status.success() => String::from_utf8(output.stdout).ok(),
        Ok(_) => None,
        Err(err) => {
            tracing::debug!(%err, "git unavailable");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_reads_branch_and_file_marks() {
        let output = "## main...origin/main [ahead 1]\0 M crates/ui/src/shell.rs\0?? notes.md\0A  new.rs\0D  gone.rs\0";
        let parsed = parse_porcelain(output);
        assert_eq!(parsed.branch.as_deref(), Some("main"));
        assert_eq!(parsed.marks, vec![
            ("crates/ui/src/shell.rs".to_string(), GitMark::Modified),
            ("notes.md".to_string(), GitMark::Untracked),
            ("new.rs".to_string(), GitMark::Added),
            ("gone.rs".to_string(), GitMark::Deleted),
        ]);
    }

    #[test]
    fn porcelain_skips_the_original_path_of_renames() {
        let parsed = parse_porcelain("## feature\0R  to.rs\0from.rs\0 M other.rs\0");
        assert_eq!(parsed.branch.as_deref(), Some("feature"));
        assert_eq!(parsed.marks, vec![
            ("to.rs".to_string(), GitMark::Renamed),
            ("other.rs".to_string(), GitMark::Modified),
        ]);
    }

    #[test]
    fn porcelain_marks_conflicts_and_prefers_worktree_changes() {
        let parsed = parse_porcelain("UU merge.rs\0MM both.rs\0AM staged.rs\0");
        let marks: Vec<GitMark> = parsed.marks.into_iter().map(|(_, mark)| mark).collect();
        assert_eq!(marks, [GitMark::Conflicted, GitMark::Modified, GitMark::Added]);
    }

    #[test]
    fn branch_header_handles_fresh_and_detached_repositories() {
        assert_eq!(parse_porcelain("## No commits yet on trunk\0").branch.as_deref(), Some("trunk"));
        assert_eq!(parse_porcelain("## HEAD (no branch)\0").branch.as_deref(), Some("HEAD"));
        assert_eq!(parse_porcelain("## dev...origin/dev\0").branch.as_deref(), Some("dev"));
    }

    #[test]
    fn status_marks_files_and_collapsed_ancestors_of_changes() {
        let root = Path::new("/r");
        let porcelain = parse_porcelain(" M crates/ui/src/shell.rs\0?? README.md\0");
        let status = GitStatus::from_porcelain(root, porcelain);
        assert_eq!(status.file_mark(Path::new("/r/crates/ui/src/shell.rs")), Some(GitMark::Modified));
        assert_eq!(status.file_mark(Path::new("/r/README.md")), Some(GitMark::Untracked));
        assert_eq!(status.file_mark(Path::new("/r/Cargo.toml")), None);
        assert_eq!(status.dir_mark(Path::new("/r/crates/ui"), false), Some(GitMark::ChangedInside));
        assert_eq!(status.dir_mark(Path::new("/r/crates/ui"), true), None);
        assert_eq!(status.dir_mark(Path::new("/r/assets"), false), None);
        assert_eq!(status.dir_mark(Path::new("/r"), false), Some(GitMark::ChangedInside));
    }

    #[test]
    fn letters_and_addition_colors_follow_the_design() {
        assert_eq!(GitMark::Modified.letter(), "M");
        assert_eq!(GitMark::Untracked.letter(), "U");
        assert_eq!(GitMark::ChangedInside.letter(), "\u{2022}");
        assert!(GitMark::Untracked.is_addition());
        assert!(GitMark::Added.is_addition());
        assert!(!GitMark::Modified.is_addition());
    }

    #[test]
    fn a_symlinked_folder_keeps_its_spelling_for_the_repository_root() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(canonical.join("repo/src")).unwrap();
        let spelled = dir.path().join("repo/src");
        assert_eq!(root_as_spelled(&spelled, &canonical.join("repo")), dir.path().join("repo"));
    }

    #[test]
    fn a_directory_outside_any_repository_has_no_status() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_git_status(dir.path()), None);
    }
}
