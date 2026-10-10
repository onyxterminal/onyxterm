#![deny(warnings)]
#![forbid(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("lock path invalid: {0}")]
    InvalidPath(String),
}

pub trait SingleInstanceLock: Send + Sync {
    fn acquire(&self) -> Result<LockGuard, PlatformError>;
}

pub struct LockGuard {
    file: File,
    path: PathBuf,
}

impl LockGuard {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if let Err(err) = fs2::FileExt::unlock(&self.file) {
            warn!(?err, path = %self.path.display(), "failed to release single-instance lock");
        }
    }
}

pub struct FileLock {
    path: PathBuf,
}

impl FileLock {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl SingleInstanceLock for FileLock {
    fn acquire(&self) -> Result<LockGuard, PlatformError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                PlatformError::InvalidPath(format!("{}: {}", parent.display(), e))
            })?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .read(true)
            .open(&self.path)
            .map_err(|e| PlatformError::InvalidPath(format!("{}: {}", self.path.display(), e)))?;
        fs2::FileExt::try_lock_exclusive(&file)?;
        Ok(LockGuard {
            file,
            path: self.path.clone(),
        })
    }
}

pub fn default_lock_path(profile: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("onyxterm");
    let safe_profile: String = profile
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    dir.join(format!("onyxterm-{}.lock", safe_profile))
}

pub fn warn_redaction(target: &str, reason: &str) {
    warn!(target = %target, reason = %reason, "redacting sensitive output");
}
