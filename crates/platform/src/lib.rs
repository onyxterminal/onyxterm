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
    #[error("environment variable {0} is not set")]
    MissingEnv(&'static str),
}

pub const PROFILE_DIR_ENV: &str = "ONYXTERM_PROFILE_DIR";
pub const DATABASE_FILE_NAME: &str = "onyxterm.db";

pub fn home_dir() -> Result<PathBuf, PlatformError> {
    non_empty_env("HOME").ok_or(PlatformError::MissingEnv("HOME"))
}

pub fn profile_dir(profile: &str) -> Result<PathBuf, PlatformError> {
    if let Some(dir) = non_empty_env(PROFILE_DIR_ENV) {
        return Ok(dir);
    }
    Ok(data_root()?.join(profile))
}

pub fn database_path(profile: &str) -> Result<PathBuf, PlatformError> {
    Ok(profile_dir(profile)?.join(DATABASE_FILE_NAME))
}

fn non_empty_env(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn data_root() -> Result<PathBuf, PlatformError> {
    Ok(home_dir()?.join("Library/Application Support/Onyxterm"))
}

#[cfg(not(target_os = "macos"))]
fn data_root() -> Result<PathBuf, PlatformError> {
    let base = match non_empty_env("XDG_DATA_HOME") {
        Some(dir) => dir,
        None => home_dir()?.join(".local/share"),
    };
    Ok(base.join("onyxterm"))
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
