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
        let dir_entry = dir_entry?;
        let name = dir_entry.file_name().to_string_lossy().into_owned();
        if name == GIT_DIR_NAME {
            continue;
        }
        let is_dir = dir_entry.path().is_dir();
        let is_ignored = ignore_rules
            .matched_path_or_any_parents(absolute_dir.join(&name), is_dir)
            .is_ignore();
        entries.push(Entry { path: dir_entry.path(), name, is_dir, is_ignored });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| natural_cmp(&a.name, &b.name)));
    Ok(entries)
}

fn build_ignore_rules(dir: &Path) -> Gitignore {
    let mut levels = Vec::new();
    for ancestor in dir.ancestors() {
        levels.push(ancestor);
        if ancestor.join(GIT_DIR_NAME).exists() {
            break;
        }
    }
    let root = levels.last().copied().unwrap_or(dir);
    let mut builder = GitignoreBuilder::new(root);
    for level in levels.iter().rev() {
        let file = level.join(GITIGNORE_FILE_NAME);
        if file.is_file() {
            if let Some(error) = builder.add(&file) {
                tracing::warn!(path = %file.display(), %error, "unreadable gitignore");
            }
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
