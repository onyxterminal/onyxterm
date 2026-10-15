use ignore::WalkBuilder;
use std::path::{Component, Path};

pub fn list_files(root: &Path, limit: usize) -> Vec<String> {
    let walker = WalkBuilder::new(root).require_git(false).sort_by_file_path(|a, b| a.cmp(b)).build();
    walker
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(err) => {
                tracing::debug!(%err, "skipping unreadable entry while listing files");
                None
            }
        })
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .filter_map(|entry| relative_path(root, entry.path()))
        .take(limit)
        .collect()
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn lists_relative_files_respecting_gitignore_and_hidden_entries() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src/ui")).unwrap();
        fs::create_dir_all(root.path().join("target")).unwrap();
        fs::write(root.path().join(".gitignore"), "target/\n").unwrap();
        for file in ["Cargo.toml", "src/main.rs", "src/ui/shell.rs", "target/out.bin", ".env"] {
            fs::write(root.path().join(file), "").unwrap();
        }
        assert_eq!(list_files(root.path(), 100), ["Cargo.toml", "src/main.rs", "src/ui/shell.rs"]);
    }

    #[test]
    fn the_limit_caps_the_listing() {
        let root = tempfile::tempdir().unwrap();
        for file in ["a", "b", "c"] {
            fs::write(root.path().join(file), "").unwrap();
        }
        assert_eq!(list_files(root.path(), 2), ["a", "b"]);
    }
}
