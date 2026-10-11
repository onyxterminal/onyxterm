use crate::buffer::Buffer;
use std::io::Write;
use std::path::Path;

const BINARY_SNIFF_BYTES: usize = 8192;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("binary file")]
    Binary,
    #[error("not UTF-8 text")]
    NotUtf8,
}

pub fn load(path: &Path) -> Result<Buffer, LoadError> {
    let bytes = std::fs::read(path)?;
    if bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0) {
        return Err(LoadError::Binary);
    }
    let text = String::from_utf8(bytes).map_err(|_| LoadError::NotUtf8)?;
    Ok(Buffer::from_text(&text))
}

pub fn save(path: &Path, buffer: &Buffer) -> std::io::Result<()> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let directory = target.parent().unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(buffer.to_saved_text().as_bytes())?;
    file.as_file().sync_all()?;
    if let Ok(metadata) = std::fs::metadata(&target) {
        std::fs::set_permissions(file.path(), metadata.permissions())?;
    }
    file.persist(&target).map_err(|err| err.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_and_save_round_trip_crlf() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "one\r\ntwo\r\n").expect("write");
        let buffer = load(&path).expect("load");
        assert_eq!(buffer.rope().to_string(), "one\ntwo\n");
        save(&path, &buffer).expect("save");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "one\r\ntwo\r\n");
    }

    #[test]
    fn binary_and_non_utf8_files_are_rejected() {
        let dir = TempDir::new().expect("tempdir");
        let binary = dir.path().join("bin");
        std::fs::write(&binary, [0x7f, 0x45, 0x00, 0x01]).expect("write");
        assert!(matches!(load(&binary), Err(LoadError::Binary)));
        let latin1 = dir.path().join("latin1");
        std::fs::write(&latin1, [0x63, 0x61, 0x66, 0xe9]).expect("write");
        assert!(matches!(load(&latin1), Err(LoadError::NotUtf8)));
    }

    #[cfg(unix)]
    #[test]
    fn save_through_symlink_writes_the_target() {
        let dir = TempDir::new().expect("tempdir");
        let target = dir.path().join("target.txt");
        let link = dir.path().join("link.txt");
        std::fs::write(&target, "old").expect("write");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        save(&link, &Buffer::from_text("new")).expect("save");
        assert!(std::fs::symlink_metadata(&link).expect("meta").file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "new");
    }

    #[cfg(unix)]
    #[test]
    fn save_keeps_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("script.sh");
        std::fs::write(&path, "echo").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        save(&path, &Buffer::from_text("echo hi")).expect("save");
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }

    #[cfg(unix)]
    #[test]
    fn failed_save_keeps_original_and_leaves_no_temp_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().expect("tempdir");
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).expect("mkdir");
        let path = locked.join("a.txt");
        std::fs::write(&path, "original").expect("write");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let result = save(&path, &Buffer::from_text("changed"));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "original");
        assert_eq!(std::fs::read_dir(&locked).expect("list").count(), 1);
    }
}
