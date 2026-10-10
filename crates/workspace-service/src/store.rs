use crate::restore_policy::RestorePolicy;
use crate::writer::{SnapshotWriter, WriterCommand};
use domain::WorkspaceId;
use persistence::{LoadedSnapshot, PersistenceError, Repository, RestoreWarning, WorkspaceSnapshot};
use platform::PlatformError;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

const QUARANTINE_SUFFIX: &str = "corrupt";
const SQLITE_SIDECAR_SUFFIXES: [&str; 2] = ["-wal", "-shm"];

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("workspace store writer has stopped")]
    WriterStopped,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StoreWarning {
    DatabaseQuarantined { quarantined_to: PathBuf, reason: String },
    MissingDirectory { workspace: WorkspaceId, missing: PathBuf, fallback: PathBuf },
    LayoutDropped(RestoreWarning),
}

#[derive(Debug, Clone)]
pub struct RestoredState {
    pub snapshot: WorkspaceSnapshot,
    pub warnings: Vec<StoreWarning>,
}

pub struct WorkspaceStore {
    commands: Option<Sender<WriterCommand>>,
    writer: Option<JoinHandle<()>>,
    completed_writes: Arc<AtomicUsize>,
}

impl WorkspaceStore {
    pub fn open(path: &Path) -> Result<(Self, RestoredState), StoreError> {
        let home = platform::home_dir()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut warnings = Vec::new();
        let (repository, loaded) = open_or_quarantine(path, &home, &mut warnings)?;
        warnings.extend(loaded.warnings.into_iter().map(StoreWarning::LayoutDropped));
        let mut policy = RestorePolicy { home: &home, warnings: &mut warnings };
        let snapshot = policy.apply(loaded.snapshot);
        let store = Self::spawn_writer(repository)?;
        Ok((store, RestoredState { snapshot, warnings }))
    }

    pub fn save(&self, snapshot: WorkspaceSnapshot) {
        if self.send(WriterCommand::Save(snapshot)).is_err() {
            tracing::error!("workspace store writer has stopped; snapshot dropped");
        }
    }

    pub fn flush(&self) -> Result<(), StoreError> {
        let (ack, done) = mpsc::channel();
        self.send(WriterCommand::Flush(ack))?;
        done.recv().map_err(|_| StoreError::WriterStopped)??;
        Ok(())
    }

    pub fn completed_writes(&self) -> usize {
        self.completed_writes.load(Ordering::SeqCst)
    }

    fn spawn_writer(repository: Repository) -> Result<Self, StoreError> {
        let completed_writes = Arc::new(AtomicUsize::new(0));
        let writer = SnapshotWriter {
            repository,
            completed_writes: Arc::clone(&completed_writes),
        };
        let (commands, inbox) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("onyxterm-store-writer".into())
            .spawn(move || writer.run(inbox))?;
        Ok(Self {
            commands: Some(commands),
            writer: Some(handle),
            completed_writes,
        })
    }

    fn send(&self, command: WriterCommand) -> Result<(), StoreError> {
        let commands = self.commands.as_ref().ok_or(StoreError::WriterStopped)?;
        commands.send(command).map_err(|_| StoreError::WriterStopped)
    }
}

impl Drop for WorkspaceStore {
    fn drop(&mut self) {
        self.commands.take();
        if let Some(handle) = self.writer.take() {
            if handle.join().is_err() {
                tracing::error!("workspace store writer panicked");
            }
        }
    }
}

fn open_or_quarantine(
    path: &Path,
    home: &Path,
    warnings: &mut Vec<StoreWarning>,
) -> Result<(Repository, LoadedSnapshot), StoreError> {
    match open_and_load(path, home) {
        Ok(opened) => Ok(opened),
        Err(err @ PersistenceError::DowngradeForbidden { .. }) => Err(err.into()),
        Err(err) => {
            tracing::warn!(%err, path = %path.display(), "quarantining unreadable database");
            let quarantined_to = quarantine(path)?;
            warnings.push(StoreWarning::DatabaseQuarantined {
                quarantined_to,
                reason: err.to_string(),
            });
            Ok(open_and_load(path, home)?)
        }
    }
}

fn open_and_load(path: &Path, home: &Path) -> Result<(Repository, LoadedSnapshot), PersistenceError> {
    let repository = Repository::open(path)?;
    let loaded = repository.load_snapshot(home)?;
    Ok((repository, loaded))
}

fn quarantine(path: &Path) -> Result<PathBuf, io::Error> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    let quarantined_to = with_suffix(path, &format!(".{QUARANTINE_SUFFIX}-{stamp}"));
    std::fs::rename(path, &quarantined_to)?;
    for sidecar in SQLITE_SIDECAR_SUFFIXES {
        let sidecar_path = with_suffix(path, sidecar);
        if sidecar_path.exists() {
            std::fs::rename(&sidecar_path, with_suffix(&quarantined_to, sidecar))?;
        }
    }
    Ok(quarantined_to)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = path.as_os_str().to_owned();
    raw.push(suffix);
    PathBuf::from(raw)
}
