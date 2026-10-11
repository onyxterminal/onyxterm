use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(50);

pub struct Watcher {
    inner: RecommendedWatcher,
    _debouncer: JoinHandle<()>,
}

impl Watcher {
    pub fn new(debounce: Duration) -> Result<(Watcher, Receiver<Vec<PathBuf>>), notify::Error> {
        let (raw_sender, raw_receiver) = channel();
        let (batch_sender, batch_receiver) = channel();
        let inner = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            forward_event(result, &raw_sender);
        })?;
        let debouncer = std::thread::spawn(move || run_debouncer(&raw_receiver, &batch_sender, debounce));
        Ok((Watcher { inner, _debouncer: debouncer }, batch_receiver))
    }

    pub fn watch(&mut self, path: &Path, recursive: bool) -> Result<(), notify::Error> {
        let mode = if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
        self.inner.watch(path, mode)
    }

    pub fn unwatch(&mut self, path: &Path) {
        if let Err(error) = self.inner.unwatch(path) {
            tracing::warn!(path = %path.display(), %error, "unwatch failed");
        }
    }
}

fn coalesce(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    paths.into_iter().collect::<BTreeSet<_>>().into_iter().collect()
}

fn forward_event(result: notify::Result<notify::Event>, sender: &Sender<Vec<PathBuf>>) {
    match result {
        Ok(event) if matches!(event.kind, EventKind::Access(_)) => {}
        Ok(event) => {
            let _ = sender.send(event.paths);
        }
        Err(error) => tracing::warn!(%error, "watch event error"),
    }
}

fn run_debouncer(raw: &Receiver<Vec<PathBuf>>, batches: &Sender<Vec<PathBuf>>, debounce: Duration) {
    while let Ok(first) = raw.recv() {
        let mut collected = first;
        let deadline = Instant::now() + debounce;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match raw.recv_timeout(remaining) {
                Ok(more) => collected.extend(more),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        if batches.send(coalesce(collected)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;
    use std::time::Instant;
    use std::sync::mpsc::RecvTimeoutError;

    const BURST_FILE_COUNT: usize = 20;
    const FIRST_BATCH_TIMEOUT: Duration = Duration::from_secs(5);
    const BURST_DEADLINE: Duration = Duration::from_secs(10);
    const QUIET_WINDOW: Duration = Duration::from_millis(600);
    const MAX_BATCHES_PER_BURST: usize = 2;
    const SILENCE_WINDOW: Duration = Duration::from_millis(800);

    fn path(value: &str) -> PathBuf {
        PathBuf::from(value)
    }

    fn is_burst_file(path: &Path) -> bool {
        path.file_name().is_some_and(|name| {
            let name = name.to_string_lossy();
            name.starts_with("file") && name.ends_with(".txt")
        })
    }

    fn collect_burst(receiver: &Receiver<Vec<PathBuf>>) -> (usize, BTreeSet<String>) {
        let deadline = Instant::now() + BURST_DEADLINE;
        let mut burst_batches = 0;
        let mut seen = BTreeSet::new();
        let mut last_burst_at = Instant::now();
        while Instant::now() < deadline {
            let all_arrived = seen.len() == BURST_FILE_COUNT;
            if all_arrived && last_burst_at.elapsed() > QUIET_WINDOW {
                break;
            }
            let Ok(batch) = receiver.recv_timeout(QUIET_WINDOW) else { continue };
            let names: Vec<String> = batch
                .iter()
                .filter(|path| is_burst_file(path))
                .filter_map(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .collect();
            if names.is_empty() {
                continue;
            }
            burst_batches += 1;
            last_burst_at = Instant::now();
            seen.extend(names);
        }
        (burst_batches, seen)
    }

    #[test]
    fn coalesce_dedups_and_sorts() {
        let merged = coalesce([path("/b"), path("/a"), path("/b"), path("/c"), path("/a")]);

        assert_eq!(merged, [path("/a"), path("/b"), path("/c")]);
    }

    #[test]
    fn bursts_coalesce_into_one_batch() {
        let root = tempfile::tempdir().unwrap();
        let (mut watcher, receiver) = Watcher::new(DEFAULT_DEBOUNCE).unwrap();
        watcher.watch(root.path(), true).unwrap();

        for index in 0..BURST_FILE_COUNT {
            fs::write(root.path().join(format!("file{index}.txt")), "x").unwrap();
        }
        let (burst_batches, seen) = collect_burst(&receiver);

        assert!(burst_batches <= MAX_BATCHES_PER_BURST, "{burst_batches} batches");
        for index in 0..BURST_FILE_COUNT {
            assert!(seen.contains(&format!("file{index}.txt")), "missing file{index}.txt");
        }
    }

    #[test]
    fn dropping_the_watcher_disconnects_the_receiver() {
        let (watcher, receiver) = Watcher::new(DEFAULT_DEBOUNCE).unwrap();

        drop(watcher);

        assert_eq!(receiver.recv_timeout(FIRST_BATCH_TIMEOUT), Err(RecvTimeoutError::Disconnected));
    }

    #[test]
    fn unwatched_path_sends_nothing() {
        let root = tempfile::tempdir().unwrap();
        let (mut watcher, receiver) = Watcher::new(DEFAULT_DEBOUNCE).unwrap();
        watcher.watch(root.path(), true).unwrap();
        watcher.unwatch(root.path());

        fs::write(root.path().join("quiet.txt"), "x").unwrap();

        assert_eq!(receiver.recv_timeout(SILENCE_WINDOW), Err(RecvTimeoutError::Timeout));
    }
}
