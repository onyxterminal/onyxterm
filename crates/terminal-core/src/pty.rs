use crate::event::{GridSize, TerminalError};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtyPair, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub trait Pty: Send + Sync {
    fn spawn(
        &self,
        shell: PathBuf,
        args: Vec<String>,
        cwd: PathBuf,
        env: HashMap<String, String>,
        size: GridSize,
    ) -> Result<PtyHandle, TerminalError>;
}

#[derive(Default)]
pub struct NativePty;

impl NativePty {
    pub fn new() -> Self {
        Self
    }
}

impl Pty for NativePty {
    fn spawn(
        &self,
        shell: PathBuf,
        args: Vec<String>,
        cwd: PathBuf,
        env: HashMap<String, String>,
        size: GridSize,
    ) -> Result<PtyHandle, TerminalError> {
        PtyHandle::spawn(shell, args, cwd, env, size)
    }
}

pub struct PtyHandle {
    inner: Arc<Mutex<PtyInner>>,
}

struct PtyInner {
    master: Box<dyn MasterPty + Send>,
    child: Option<Box<dyn Child + Send + Sync>>,
    writer: Option<Box<dyn Write + Send>>,
    reader_slot: Option<Box<dyn Read + Send>>,
    pid: Option<u32>,
}

impl Clone for PtyHandle {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl PtyHandle {
    pub fn spawn(
        shell: PathBuf,
        args: Vec<String>,
        cwd: PathBuf,
        env: HashMap<String, String>,
        size: GridSize,
    ) -> Result<Self, TerminalError> {
        let system = native_pty_system();
        let pair: PtyPair = system
            .openpty(to_pty_size(size))
            .map_err(|e| TerminalError::PtySpawn(e.to_string()))?;

        let command = build_command(&shell, &args, &cwd, &env);

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| TerminalError::PtySpawn(e.to_string()))?;
        let pid = child.process_id();

        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| TerminalError::PtySpawn(format!("take_writer failed: {e}")))?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| TerminalError::PtySpawn(format!("try_clone_reader failed: {e}")))?;
        let master = pair.master;

        Ok(Self {
            inner: Arc::new(Mutex::new(PtyInner {
                master,
                child: Some(child),
                writer: Some(writer),
                reader_slot: Some(reader),
                pid,
            })),
        })
    }

    pub fn write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        let mut guard = self.inner.lock();
        let writer = guard.writer.as_mut().ok_or(TerminalError::Terminated)?;
        writer
            .write_all(bytes)
            .map_err(|e| TerminalError::PtyWrite(e.to_string()))?;
        writer.flush().map_err(|e| TerminalError::PtyWrite(e.to_string()))?;
        Ok(())
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        let guard = self.inner.lock();
        guard
            .master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| TerminalError::PtyResize(e.to_string()))
    }

    pub fn take_reader(&self) -> Option<Box<dyn Read + Send>> {
        self.inner.lock().reader_slot.take()
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.inner.lock().pid
    }

    pub fn child_is_alive(&self) -> bool {
        let mut guard = self.inner.lock();
        if let Some(child) = guard.child.as_mut() {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    pub fn kill(&self) -> Result<(), TerminalError> {
        self.kill_with_deadline(Duration::from_secs(1))
    }

    pub fn kill_with_deadline(&self, deadline: Duration) -> Result<(), TerminalError> {
        let pid = { self.inner.lock().pid };
        let mut sent_sigterm = false;
        if let Some(pid) = pid {
            let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
            if rc == 0 {
                sent_sigterm = true;
            }
        }

        if !sent_sigterm {
            return self.force_kill_child();
        }

        let end = Instant::now() + deadline;
        loop {
            {
                let mut guard = self.inner.lock();
                if let Some(child) = guard.child.as_mut() {
                    match child.try_wait() {
                        Ok(Some(_)) => {
                            guard.child = None;
                            return Ok(());
                        }
                        Ok(None) if Instant::now() >= end => {
                            drop(guard);
                            return self.force_kill_child();
                        }
                        Ok(None) => {}
                        Err(_) => {
                            drop(guard);
                            return self.force_kill_child();
                        }
                    }
                } else {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn force_kill_child(&self) -> Result<(), TerminalError> {
        let mut guard = self.inner.lock();
        if let Some(child) = guard.child.as_mut() {
            child.kill().map_err(|e| TerminalError::PtyWrite(e.to_string()))?;
        }
        if let Some(child) = guard.child.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {
                    guard.child = None;
                    return Ok(());
                }
                _ => {}
            }
        }
        if let Some(pid) = guard.pid {
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGKILL);
            }
        }
        guard.writer = None;
        Ok(())
    }
}

fn to_pty_size(size: GridSize) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn build_command(
    shell: &PathBuf,
    args: &[String],
    cwd: &PathBuf,
    env: &HashMap<String, String>,
) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(shell);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.cwd(cwd);
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd
}
