use crate::event::{GridSize, TerminalError};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtyPair, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const EXIT_STATUS_DEADLINE: Duration = Duration::from_millis(500);
const EXIT_STATUS_POLL: Duration = Duration::from_millis(10);

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundJob {
    pub process_group: i32,
    pub argv: Vec<String>,
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

    pub fn current_dir(&self) -> Option<PathBuf> {
        process_cwd(self.child_pid()?)
    }

    pub fn has_foreground_job(&self) -> bool {
        self.foreground_process_group().is_some()
    }

    pub fn foreground_process_group(&self) -> Option<i32> {
        let guard = self.inner.lock();
        let shell = guard.pid?;
        let leader = guard.master.process_group_leader()?;
        (leader as u32 != shell).then_some(leader)
    }

    pub fn foreground_job(&self) -> Option<ForegroundJob> {
        let process_group = self.foreground_process_group()?;
        let argv = process_argv(process_group as u32)?;
        Some(ForegroundJob { process_group, argv })
    }

    pub fn wait_exit_code(&self) -> Option<i32> {
        let end = Instant::now() + EXIT_STATUS_DEADLINE;
        loop {
            {
                let mut guard = self.inner.lock();
                let child = guard.child.as_mut()?;
                if let Ok(Some(status)) = child.try_wait() {
                    return Some(status.exit_code() as i32);
                }
            }
            if Instant::now() >= end {
                return None;
            }
            std::thread::sleep(EXIT_STATUS_POLL);
        }
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

#[cfg(target_os = "macos")]
fn process_cwd(pid: u32) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
    let written = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            &mut info as *mut _ as *mut libc::c_void,
            size,
        )
    };
    if written != size {
        return None;
    }
    let raw: Vec<u8> = info.pvi_cdir.vip_path.iter().flatten().map(|c| *c as u8).take_while(|b| *b != 0).collect();
    (!raw.is_empty()).then(|| PathBuf::from(std::ffi::OsStr::from_bytes(&raw)))
}

#[cfg(not(target_os = "macos"))]
fn process_cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

#[cfg(target_os = "macos")]
fn process_argv(pid: u32) -> Option<Vec<String>> {
    procargs_argv(pid).or_else(|| executable_path(pid).map(|path| vec![path]))
}

#[cfg(target_os = "macos")]
fn procargs_argv(pid: u32) -> Option<Vec<String>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
    let mut size: libc::size_t = 0;
    let null = std::ptr::null_mut();
    let sized = unsafe { libc::sysctl(mib.as_mut_ptr(), mib.len() as libc::c_uint, null, &mut size, null, 0) };
    if sized != 0 || size == 0 {
        return None;
    }
    let mut buffer = vec![0u8; size];
    let target = buffer.as_mut_ptr() as *mut libc::c_void;
    let read = unsafe { libc::sysctl(mib.as_mut_ptr(), mib.len() as libc::c_uint, target, &mut size, null, 0) };
    if read != 0 {
        return None;
    }
    buffer.truncate(size);
    parse_procargs(&buffer)
}

#[cfg(target_os = "macos")]
fn parse_procargs(buffer: &[u8]) -> Option<Vec<String>> {
    let (count, rest) = buffer.split_first_chunk::<4>()?;
    let argc = i32::from_ne_bytes(*count).max(0) as usize;
    let exec_path_end = rest.iter().position(|byte| *byte == 0)?;
    let after_path = &rest[exec_path_end..];
    let args_start = after_path.iter().position(|byte| *byte != 0)?;
    let argv: Vec<String> = after_path[args_start..]
        .split(|byte| *byte == 0)
        .take(argc)
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect();
    (!argv.is_empty()).then_some(argv)
}

#[cfg(target_os = "macos")]
fn executable_path(pid: u32) -> Option<String> {
    let mut buffer = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let target = buffer.as_mut_ptr() as *mut libc::c_void;
    let written = unsafe { libc::proc_pidpath(pid as libc::c_int, target, buffer.len() as u32) };
    if written <= 0 {
        return None;
    }
    buffer.truncate(written as usize);
    Some(String::from_utf8_lossy(&buffer).into_owned())
}

#[cfg(not(target_os = "macos"))]
fn process_argv(pid: u32) -> Option<Vec<String>> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let argv: Vec<String> = raw
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect();
    (!argv.is_empty()).then_some(argv)
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
