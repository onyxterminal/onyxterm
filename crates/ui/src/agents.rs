#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::theme::Theme;
use agent_core::{AdapterProfile, AdapterRegistry, AgentError, CommandSpec};
use chrono::{DateTime, Utc};
use domain::{Agent, AgentId, AgentState};
use gpui::Hsla;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const STOP_GRACE_SECS: i64 = 5;
const LAUNCH_PROFILE: &str = "default";
const EXECUTABLE_BITS: u32 = 0o111;
const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 3600;
const UNKNOWN_EXIT_CODE: i32 = -1;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LaunchError {
    #[error("Unknown agent provider \u{201c}{0}\u{201d}")]
    UnknownProvider(String),
    #[error("Directory {0} does not exist")]
    MissingDirectory(String),
    #[error("{name} is not installed: `{program}` was not found on PATH")]
    NotInstalled { name: String, program: String },
    #[error("{0}")]
    Rejected(String),
}

impl From<AgentError> for LaunchError {
    fn from(err: AgentError) -> Self {
        Self::Rejected(err.to_string())
    }
}

pub struct LaunchRequest<'a> {
    pub provider: &'a str,
    pub cwd: &'a Path,
    pub search_path: &'a OsStr,
}

pub fn prepare_launch(registry: &AdapterRegistry, request: LaunchRequest<'_>) -> Result<CommandSpec, LaunchError> {
    let adapter = registry.get(request.provider).ok_or_else(|| LaunchError::UnknownProvider(request.provider.into()))?;
    if !request.cwd.is_dir() {
        return Err(LaunchError::MissingDirectory(request.cwd.display().to_string()));
    }
    let profile = AdapterProfile::new(LAUNCH_PROFILE, adapter.id());
    profile.validate_cwd(request.cwd)?;
    let program = adapter.resolve_executable().unwrap_or_else(|| PathBuf::from(adapter.id()));
    let executable = find_executable(&program, request.search_path).ok_or_else(|| LaunchError::NotInstalled {
        name: adapter.display_name().to_string(),
        program: program.display().to_string(),
    })?;
    let spec = adapter.build_command(&profile, request.cwd.to_path_buf())?;
    Ok(CommandSpec { executable, ..spec })
}

pub fn find_executable(program: &Path, search_path: &OsStr) -> Option<PathBuf> {
    if program.components().count() > 1 {
        return is_executable(program).then(|| program.to_path_buf());
    }
    std::env::split_paths(search_path).map(|dir| dir.join(program)).find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & EXECUTABLE_BITS != 0)
}

#[derive(Debug, Default)]
pub struct AgentTracking {
    exit_codes: HashMap<AgentId, i32>,
    stop_requested: HashMap<AgentId, DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRow {
    pub id: AgentId,
    pub provider: String,
    pub cwd_label: String,
    pub state: AgentState,
    pub elapsed: String,
    pub exit_code: Option<i32>,
    pub can_stop: bool,
    pub can_force: bool,
}

impl AgentTracking {
    pub fn record_exit(&mut self, agent: &mut Agent, code: i32, classified: AgentState) {
        self.stop_requested.remove(&agent.id);
        if let Some(code) = known_exit_code(code) {
            self.exit_codes.insert(agent.id, code);
        }
        transition(agent, classified);
    }

    pub fn record_stop_request(&mut self, agent: &mut Agent, now: DateTime<Utc>) {
        self.stop_requested.insert(agent.id, now);
        transition(agent, AgentState::Stopping);
    }

    pub fn row(&self, agent: &Agent, home: &Path, now: DateTime<Utc>) -> AgentRow {
        let can_force = self
            .stop_requested
            .get(&agent.id)
            .is_some_and(|requested| (now - *requested).num_seconds() >= STOP_GRACE_SECS);
        AgentRow {
            id: agent.id,
            provider: agent.provider.clone(),
            cwd_label: short_label(&agent.cwd, home),
            state: agent.state,
            elapsed: elapsed_label(agent, now),
            exit_code: self.exit_codes.get(&agent.id).copied(),
            can_stop: agent.state == AgentState::Running,
            can_force: can_force && agent.state.is_alive(),
        }
    }
}

fn elapsed_label(agent: &Agent, now: DateTime<Utc>) -> String {
    let end = agent.ended_at.or_else(|| agent.state.is_alive().then_some(now));
    agent.started_at.zip(end).map(|(start, end)| format_elapsed(end - start)).unwrap_or_default()
}

pub fn mark_terminated(agent: &mut Agent) {
    if agent.state.is_alive() {
        transition(agent, AgentState::Exited);
    }
}

fn transition(agent: &mut Agent, next: AgentState) {
    match agent.clone().transition(next) {
        Ok(updated) => *agent = updated,
        Err(err) => tracing::debug!(%err, "agent transition ignored"),
    }
}

pub fn known_exit_code(code: i32) -> Option<i32> {
    (code != UNKNOWN_EXIT_CODE).then_some(code)
}

pub fn short_label(path: &Path, home: &Path) -> String {
    if path == home {
        return "~".to_string();
    }
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

pub fn format_elapsed(elapsed: chrono::Duration) -> String {
    let secs = elapsed.num_seconds().max(0);
    match secs {
        s if s < SECONDS_PER_MINUTE => format!("{s}s"),
        s if s < SECONDS_PER_HOUR => format!("{}m {:02}s", s / SECONDS_PER_MINUTE, s % SECONDS_PER_MINUTE),
        s => format!("{}h {:02}m", s / SECONDS_PER_HOUR, (s % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE),
    }
}

pub fn state_label(state: AgentState) -> &'static str {
    match state {
        AgentState::Created => "Created",
        AgentState::Starting => "Starting",
        AgentState::Running => "Running",
        AgentState::Exited => "Exited",
        AgentState::Failed => "Failed",
        AgentState::Stopping => "Stopping",
        AgentState::Unknown => "Unknown",
    }
}

pub fn status_color(theme: &Theme, state: AgentState) -> Hsla {
    let status = &theme.status;
    match state {
        AgentState::Created | AgentState::Starting => status.starting,
        AgentState::Running => status.running,
        AgentState::Exited => status.exited,
        AgentState::Failed => status.failed,
        AgentState::Stopping => status.stopping,
        AgentState::Unknown => status.unknown,
    }
}
