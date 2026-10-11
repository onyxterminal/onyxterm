use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::cmp::Ordering;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::str::Chars;

pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub is_ignored: bool,
}

const GIT_DIR_NAME: &str = ".git";
const GITIGNORE_FILE_NAME: &str = ".gitignore";

pub fn list_dir(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let absolute_dir = std::path::absolute(dir)?;
    let ignore_rules = build_ignore_rules(&absolute_dir);
    let mut entries = Vec::new();
    for dir_entry in std::fs::read_dir(dir)? {
        let dir_entry = match dir_entry {
            Ok(dir_entry) => dir_entry,
            Err(error) => {
                tracing::warn!(%error, "skipping entry that vanished during listing");
                continue;
            }
        };
        let name = dir_entry.file_name().to_string_lossy().into_owned();
        if name == GIT_DIR_NAME {
            continue;
        }
        let is_dir = dir_entry.path().is_dir();
        let is_ignored = is_ignored(&ignore_rules, &absolute_dir.join(&name), is_dir);
        entries.push(Entry { path: dir_entry.path(), name, is_dir, is_ignored });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| natural_cmp(&a.name, &b.name)));
    Ok(entries)
}

fn is_ignored(rules_deepest_first: &[Gitignore], path: &Path, is_dir: bool) -> bool {
    rules_deepest_first
        .iter()
        .map(|rules| rules.matched_path_or_any_parents(path, is_dir))
        .find(|decision| !decision.is_none())
        .is_some_and(|decision| decision.is_ignore())
}

fn build_ignore_rules(dir: &Path) -> Vec<Gitignore> {
    let mut rules = Vec::new();
    for level in dir.ancestors() {
        rules.push(build_level_rules(level));
        if level.join(GIT_DIR_NAME).exists() {
            break;
        }
    }
    rules
}

fn build_level_rules(level: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(level);
    let file = level.join(GITIGNORE_FILE_NAME);
    if file.is_file() {
        if let Some(error) = builder.add(&file) {
            tracing::warn!(path = %file.display(), %error, "unreadable gitignore");
        }
    }
    builder.build().unwrap_or_else(|error| {
        tracing::warn!(%error, "gitignore rules rejected");
        Gitignore::empty()
    })
}

pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        let ordering = match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(l), Some(r)) if l.is_ascii_digit() && r.is_ascii_digit() => {
                compare_digit_runs(&take_digits(&mut left), &take_digits(&mut right))
            }
            (Some(l), Some(r)) => {
                left.next();
                right.next();
                l.to_lowercase().cmp(r.to_lowercase())
            }
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
}

fn take_digits(chars: &mut Peekable<Chars>) -> String {
    let mut digits = String::new();
    while let Some(digit) = chars.next_if(char::is_ascii_digit) {
        digits.push(digit);
    }
    digits
}

fn compare_digit_runs(a: &str, b: &str) -> Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    #[test]
    fn lists_dirs_first_then_files_in_natural_order() {
        let root = tempfile::tempdir().unwrap();
        for file in ["b.txt", "a10.txt", "a2.txt"] {
            fs::write(root.path().join(file), "").unwrap();
        }
        for dir in ["zdir", "Adir"] {
            fs::create_dir(root.path().join(dir)).unwrap();
        }

        let entries = list_dir(root.path()).unwrap();

        assert_eq!(names(&entries), ["Adir", "zdir", "a2.txt", "a10.txt", "b.txt"]);
    }

    #[test]
    fn skips_dot_git_and_marks_ignored() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".git")).unwrap();
        fs::write(root.path().join(".gitignore"), "target/\n*.log").unwrap();
        fs::create_dir(root.path().join("target")).unwrap();
        fs::write(root.path().join("x.log"), "").unwrap();
        fs::create_dir(root.path().join("src")).unwrap();

        let entries = list_dir(root.path()).unwrap();
        let ignored = |name: &str| entries.iter().find(|entry| entry.name == name).unwrap().is_ignored;

        assert!(!names(&entries).contains(&".git"));
        assert!(ignored("target"));
        assert!(ignored("x.log"));
        assert!(!ignored("src"));
        assert!(!ignored(".gitignore"));
    }

    #[test]
    fn missing_directory_is_an_error() {
        let root = tempfile::tempdir().unwrap();

        assert!(list_dir(&root.path().join("missing")).is_err());
    }

    fn ignored_names(dir: &Path) -> Vec<String> {
        list_dir(dir)
            .unwrap()
            .into_iter()
            .filter(|entry| entry.is_ignored)
            .map(|entry| entry.name)
            .collect()
    }

    #[test]
    fn nested_gitignore_anchors_to_its_own_directory() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".git")).unwrap();
        let sub = root.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join(".gitignore"), "/dist\ndocs/*.md").unwrap();
        fs::create_dir(sub.join("dist")).unwrap();
        fs::create_dir(sub.join("docs")).unwrap();
        fs::create_dir(root.path().join("dist")).unwrap();

        assert_eq!(ignored_names(&sub), ["dist"]);
        assert!(ignored_names(root.path()).is_empty());
        assert_eq!(ignored_names(&sub.join("docs")), Vec::<String>::new());
        fs::write(sub.join("docs").join("a.md"), "").unwrap();
        assert_eq!(ignored_names(&sub.join("docs")), ["a.md"]);
    }

    #[test]
    fn negation_whitelists_a_file() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".git")).unwrap();
        fs::write(root.path().join(".gitignore"), "*.log\n!keep.log").unwrap();
        fs::write(root.path().join("a.log"), "").unwrap();
        fs::write(root.path().join("keep.log"), "").unwrap();

        assert_eq!(ignored_names(root.path()), ["a.log"]);
    }

    #[test]
    fn ancestor_gitignore_applies_in_subdirectory() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".git")).unwrap();
        fs::write(root.path().join(".gitignore"), "*.tmp").unwrap();
        let sub = root.path().join("a").join("b");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("x.tmp"), "").unwrap();
        fs::write(sub.join("y.txt"), "").unwrap();

        assert_eq!(ignored_names(&sub), ["x.tmp"]);
    }

    #[test]
    fn deeper_negation_overrides_ancestor_ignore() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".git")).unwrap();
        fs::write(root.path().join(".gitignore"), "*.log").unwrap();
        let sub = root.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join(".gitignore"), "!keep.log").unwrap();
        fs::write(sub.join("keep.log"), "").unwrap();
        fs::write(sub.join("drop.log"), "").unwrap();

        assert_eq!(ignored_names(&sub), ["drop.log"]);
    }

    #[test]
    fn root_ignore_applies_two_levels_down_and_inside_ignored_directories() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".git")).unwrap();
        fs::write(root.path().join(".gitignore"), "target/").unwrap();
        let deep = root.path().join("a").join("b");
        let ignored_dir = deep.join("target");
        fs::create_dir_all(&ignored_dir).unwrap();
        fs::write(ignored_dir.join("out.bin"), "").unwrap();

        assert_eq!(ignored_names(&deep), ["target"]);
        assert_eq!(ignored_names(&ignored_dir), ["out.bin"]);
    }

    #[test]
    fn natural_cmp_leading_zeros_are_consistent() {
        assert_ne!(natural_cmp("a01", "a1"), Ordering::Equal);
        assert_eq!(natural_cmp("a01", "a1"), natural_cmp("a1", "a01").reverse());
        assert_eq!(natural_cmp("a01", "a2"), Ordering::Less);
    }

    #[test]
    fn natural_cmp_handles_very_long_digit_runs() {
        let small = format!("a{}", "9".repeat(30));
        let large = format!("a1{}", "0".repeat(30));

        assert_eq!(natural_cmp(&small, &large), Ordering::Less);
        assert_eq!(natural_cmp(&large, &small), Ordering::Greater);
    }

    #[test]
    fn natural_cmp_orders_numbers_by_value() {
        assert_eq!(natural_cmp("a2", "a10"), Ordering::Less);
        assert_eq!(natural_cmp("img_9.png", "img_10.png"), Ordering::Less);
    }

    #[test]
    fn natural_cmp_falls_back_to_byte_order_for_case() {
        assert_eq!(natural_cmp("A", "a"), Ordering::Less);
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
    }
}
