use crate::event::{GridSize, TerminalError, TerminalEvent};
use crate::pty::Pty;
use crate::shell_integration::ShellIntegration;
use crate::vt::{Terminal, TerminalHandle, DEFAULT_SCROLLBACK};
use async_trait::async_trait;
use domain::TerminalSessionId;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::broadcast;

#[async_trait]
pub trait TerminalService: Send + Sync {
    async fn spawn(
        &self,
        shell: PathBuf,
        cwd: PathBuf,
        size: GridSize,
    ) -> Result<TerminalSessionId, TerminalError>;

    async fn write(
        &self,
        session: TerminalSessionId,
        bytes: &[u8],
    ) -> Result<(), TerminalError>;

    async fn resize(
        &self,
        session: TerminalSessionId,
        size: GridSize,
    ) -> Result<(), TerminalError>;

    async fn subscribe_output(
        &self,
        session: TerminalSessionId,
    ) -> Result<(), TerminalError>;

    async fn terminate(&self, session: TerminalSessionId) -> Result<(), TerminalError>;

    async fn snapshot(
        &self,
        session: TerminalSessionId,
    ) -> Result<(), TerminalError>;
}

const TERMINAL_ENV: [(&str, &str); 2] = [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];
const ZDOTDIR_VAR: &str = "ZDOTDIR";

#[derive(Debug, Clone)]
pub struct ShellCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

pub struct InMemoryTerminalService {
    pty: Box<dyn Pty>,
    sessions: Mutex<HashMap<TerminalSessionId, TerminalHandle>>,
    scrollback: usize,
    integration: Option<ShellIntegration>,
}

impl InMemoryTerminalService {
    pub fn new(pty: Box<dyn Pty>) -> Self {
        Self {
            pty,
            sessions: Mutex::new(HashMap::new()),
            scrollback: DEFAULT_SCROLLBACK,
            integration: None,
        }
    }

    pub fn with_shell_integration(mut self, integration: ShellIntegration) -> Self {
        self.integration = Some(integration);
        self
    }

    pub fn with_scrollback(mut self, scrollback: usize) -> Self {
        self.scrollback = scrollback;
        self
    }

    pub fn events(
        &self,
        session: TerminalSessionId,
    ) -> Option<broadcast::Receiver<TerminalEvent>> {
        let guard = self.sessions.lock();
        guard.get(&session).map(|h| h.subscribe())
    }

    pub fn snapshot_grid(
        &self,
        session: TerminalSessionId,
    ) -> Option<crate::event::GridSnapshot> {
        let guard = self.sessions.lock();
        guard.get(&session).map(|h| h.snapshot_grid())
    }

    pub fn grid_size(&self, session: TerminalSessionId) -> Option<GridSize> {
        let guard = self.sessions.lock();
        guard.get(&session).map(|h| h.grid_size())
    }

    pub fn scrollback_len(&self, session: TerminalSessionId) -> Option<usize> {
        let guard = self.sessions.lock();
        guard.get(&session).map(|h| h.scrollback_len())
    }

    pub fn handle_for(&self, id: TerminalSessionId) -> Option<TerminalHandle> {
        let guard = self.sessions.lock();
        guard.get(&id).cloned()
    }

    pub fn active_session_count(&self) -> usize {
        self.sessions.lock().len()
    }

    pub fn spawn_command(
        &self,
        command: ShellCommand,
        size: GridSize,
    ) -> Result<TerminalSessionId, TerminalError> {
        let id = TerminalSessionId::new();
        let mut env: HashMap<String, String> = TERMINAL_ENV
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        if let Some(integration) = &self.integration {
            env.extend(integration.env_for(&command.program, std::env::var_os(ZDOTDIR_VAR)));
        }
        let pty_handle = self.pty.spawn(command.program, command.args, command.cwd, env, size)?;
        let terminal = Terminal::new(id, size, self.scrollback);
        let handle = TerminalHandle::attach(terminal, pty_handle)?;
        self.sessions.lock().insert(id, handle);
        Ok(id)
    }

    pub fn terminate_session(&self, session: TerminalSessionId) -> Result<(), TerminalError> {
        let handle = self
            .sessions
            .lock()
            .remove(&session)
            .ok_or_else(|| TerminalError::SessionNotFound(session.to_string()))?;
        handle.kill()
    }
}

#[async_trait]
impl TerminalService for InMemoryTerminalService {
    async fn spawn(
        &self,
        shell: PathBuf,
        cwd: PathBuf,
        size: GridSize,
    ) -> Result<TerminalSessionId, TerminalError> {
        let command = ShellCommand { program: shell, args: Vec::new(), cwd };
        self.spawn_command(command, size)
    }

    async fn write(
        &self,
        session: TerminalSessionId,
        bytes: &[u8],
    ) -> Result<(), TerminalError> {
        let guard = self.sessions.lock();
        let handle = guard
            .get(&session)
            .ok_or_else(|| TerminalError::SessionNotFound(session.to_string()))?;
        handle.write(bytes)
    }

    async fn resize(
        &self,
        session: TerminalSessionId,
        size: GridSize,
    ) -> Result<(), TerminalError> {
        let guard = self.sessions.lock();
        let handle = guard
            .get(&session)
            .ok_or_else(|| TerminalError::SessionNotFound(session.to_string()))?;
        handle.resize(size.rows, size.cols)
    }

    async fn subscribe_output(
        &self,
        session: TerminalSessionId,
    ) -> Result<(), TerminalError> {
        let guard = self.sessions.lock();
        guard
            .get(&session)
            .ok_or_else(|| TerminalError::SessionNotFound(session.to_string()))?;
        Ok(())
    }

    async fn terminate(&self, session: TerminalSessionId) -> Result<(), TerminalError> {
        self.terminate_session(session)
    }

    async fn snapshot(
        &self,
        session: TerminalSessionId,
    ) -> Result<(), TerminalError> {
        let guard = self.sessions.lock();
        let handle = guard
            .get(&session)
            .ok_or_else(|| TerminalError::SessionNotFound(session.to_string()))?;
        handle.snapshot_grid();
        Ok(())
    }
}
