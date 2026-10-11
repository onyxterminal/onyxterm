use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum OpError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("already exists: {}", .0.display())]
    AlreadyExists(PathBuf),
    #[error("invalid name: {0:?}")]
    InvalidName(String),
    #[error("could not move to trash: {0}")]
    Trash(String),
}

pub fn validate_name(name: &str) -> Result<&str, OpError> {
    let trimmed = name.trim();
    let is_reserved = trimmed.is_empty() || trimmed == "." || trimmed == "..";
    if is_reserved || trimmed.contains(['/', '\0']) {
        return Err(OpError::InvalidName(name.to_owned()));
    }
    Ok(trimmed)
}

pub fn create_file(dir: &Path, name: &str) -> Result<PathBuf, OpError> {
    let path = dir.join(validate_name(name)?);
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| classify_create_error(error, &path))?;
    Ok(path)
}

pub fn create_dir(dir: &Path, name: &str) -> Result<PathBuf, OpError> {
    let path = dir.join(validate_name(name)?);
    std::fs::create_dir(&path).map_err(|error| classify_create_error(error, &path))?;
    Ok(path)
}

pub fn rename(from: &Path, new_name: &str) -> Result<PathBuf, OpError> {
    let name = validate_name(new_name)?;
    let target = from.with_file_name(name);
    if target.symlink_metadata().is_ok() {
        return Err(OpError::AlreadyExists(target));
    }
    std::fs::rename(from, &target)?;
    Ok(target)
}

pub fn trash(path: &Path) -> Result<(), OpError> {
    trash::delete(path).map_err(|error| OpError::Trash(error.to_string()))
}

fn classify_create_error(error: std::io::Error, path: &Path) -> OpError {
    match error.kind() {
        ErrorKind::AlreadyExists => OpError::AlreadyExists(path.to_path_buf()),
        _ => OpError::Io(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn create_file_and_dir_inside_the_folder() {
        let root = tempfile::tempdir().unwrap();

        let file = create_file(root.path(), "a.txt").unwrap();
        let dir = create_dir(root.path(), "sub").unwrap();

        assert_eq!(file, root.path().join("a.txt"));
        assert!(file.is_file());
        assert_eq!(dir, root.path().join("sub"));
        assert!(dir.is_dir());
    }

    #[test]
    fn create_refuses_existing_name() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("a.txt"), "keep").unwrap();
        fs::create_dir(root.path().join("sub")).unwrap();

        assert!(matches!(create_file(root.path(), "a.txt"), Err(OpError::AlreadyExists(_))));
        assert!(matches!(create_dir(root.path(), "sub"), Err(OpError::AlreadyExists(_))));
        assert_eq!(fs::read_to_string(root.path().join("a.txt")).unwrap(), "keep");
    }

    #[test]
    fn rename_moves_within_the_same_parent() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("a.txt");
        fs::write(&original, "x").unwrap();

        let renamed = rename(&original, "b.txt").unwrap();

        assert_eq!(renamed, root.path().join("b.txt"));
        assert!(!original.exists());
        assert_eq!(fs::read_to_string(renamed).unwrap(), "x");
    }

    #[test]
    fn rename_refuses_to_overwrite() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("a.txt");
        let target = root.path().join("b.txt");
        fs::write(&source, "source").unwrap();
        fs::write(&target, "target").unwrap();

        assert!(matches!(rename(&source, "b.txt"), Err(OpError::AlreadyExists(_))));
        assert_eq!(fs::read_to_string(source).unwrap(), "source");
        assert_eq!(fs::read_to_string(target).unwrap(), "target");
    }

    #[test]
    fn invalid_names_are_rejected() {
        let root = tempfile::tempdir().unwrap();

        for name in ["", "  ", ".", "..", "a/b"] {
            assert!(matches!(validate_name(name), Err(OpError::InvalidName(_))), "{name:?}");
            assert!(matches!(create_file(root.path(), name), Err(OpError::InvalidName(_))), "{name:?}");
        }
        assert_eq!(validate_name("  ok.txt ").unwrap(), "ok.txt");
    }

    #[test]
    #[ignore = "moves a file into the real OS trash"]
    fn trash_moves_the_file_out_of_the_folder() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("doomed.txt");
        fs::write(&file, "bye").unwrap();

        trash(&file).unwrap();

        assert!(!file.exists());
    }
}
