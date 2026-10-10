use persistence::{PersistenceError, Repository, WorkspaceSnapshot};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const SAVE_DEBOUNCE: Duration = Duration::from_millis(300);

pub(crate) enum WriterCommand {
    Save(WorkspaceSnapshot),
    Flush(Sender<Result<(), PersistenceError>>),
}

struct PendingSave {
    snapshot: WorkspaceSnapshot,
    deadline: Instant,
}

pub(crate) struct SnapshotWriter {
    pub repository: Repository,
    pub completed_writes: Arc<AtomicUsize>,
}

impl SnapshotWriter {
    pub fn run(self, commands: Receiver<WriterCommand>) {
        let mut pending: Option<PendingSave> = None;
        loop {
            match next_command(&commands, pending.as_ref()) {
                Ok(WriterCommand::Save(snapshot)) => {
                    let deadline = pending
                        .as_ref()
                        .map_or_else(|| Instant::now() + SAVE_DEBOUNCE, |p| p.deadline);
                    pending = Some(PendingSave { snapshot, deadline });
                }
                Ok(WriterCommand::Flush(ack)) => {
                    let _ = ack.send(self.write(pending.take()));
                }
                Err(RecvTimeoutError::Timeout) => self.write_logging_failure(pending.take()),
                Err(RecvTimeoutError::Disconnected) => {
                    self.write_logging_failure(pending.take());
                    return;
                }
            }
        }
    }

    fn write(&self, pending: Option<PendingSave>) -> Result<(), PersistenceError> {
        let Some(pending) = pending else {
            return Ok(());
        };
        self.repository.write_snapshot(&pending.snapshot)?;
        self.completed_writes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn write_logging_failure(&self, pending: Option<PendingSave>) {
        if let Err(err) = self.write(pending) {
            tracing::error!(%err, "failed to persist workspace snapshot");
        }
    }
}

fn next_command(
    commands: &Receiver<WriterCommand>,
    pending: Option<&PendingSave>,
) -> Result<WriterCommand, RecvTimeoutError> {
    match pending {
        None => commands.recv().map_err(|_| RecvTimeoutError::Disconnected),
        Some(p) => commands.recv_timeout(p.deadline.saturating_duration_since(Instant::now())),
    }
}
