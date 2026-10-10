use std::path::PathBuf;

use async_trait::async_trait;
use domain::{PanelId, TerminalSessionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
}

impl GridSize {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("pty spawn failed: {0}")]
    PtySpawn(String),
}

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
    async fn terminate(
        &self,
        session: TerminalSessionId,
    ) -> Result<(), TerminalError>;
    async fn snapshot(
        &self,
        session: TerminalSessionId,
    ) -> Result<(), TerminalError>;
}

pub fn panel_to_session(_panel: PanelId) -> Option<TerminalSessionId> {
    None
}
