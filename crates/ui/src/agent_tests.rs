#![allow(clippy::disallowed_methods)]

use crate::agents::{
    find_executable, format_elapsed, prepare_launch, short_label, state_label, status_color, AgentTracking,
    LaunchError, LaunchRequest, STOP_GRACE_SECS,
};
use crate::confirm::stop_agent_copy;
use crate::theme::Theme;
use agent_core::AdapterRegistry;
use chrono::{Duration, Utc};
use domain::{Agent, AgentId, AgentState, TerminalSessionId, WorkspaceId};
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;

fn fake_bin(dir: &Path, name: &str, mode: u32) {
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\n").expect("write bin");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

fn running_agent(cwd: &Path) -> Agent {
    let agent = Agent::new(AgentId::new(), "claude".into(), WorkspaceId::new(), TerminalSessionId::new(), cwd.into());
    agent.transition(AgentState::Running).expect("running")
}

#[test]
fn launch_resolves_provider_on_path_into_argv_without_shell() {
    let bin = TempDir::new().expect("bin");
    let work = TempDir::new().expect("work");
    fake_bin(bin.path(), "claude", 0o755);
    let registry = AdapterRegistry::with_defaults();
    let search_path = OsString::from(bin.path());
    let request = LaunchRequest { provider: "claude", cwd: work.path(), search_path: &search_path };
    let spec = prepare_launch(&registry, request).expect("launch spec");
    assert_eq!(spec.executable, bin.path().join("claude"));
    assert!(spec.args.is_empty());
    assert_eq!(spec.cwd, work.path());
}

#[test]
fn launch_reports_missing_cli_directory_and_provider_without_panicking() {
    let bin = TempDir::new().expect("bin");
    let work = TempDir::new().expect("work");
    fake_bin(bin.path(), "codex", 0o644);
    let registry = AdapterRegistry::with_defaults();
    let search_path = OsString::from(bin.path());
    let launch = |provider, cwd| prepare_launch(&registry, LaunchRequest { provider, cwd, search_path: &search_path });

    let missing = launch("codex", work.path()).expect_err("non-executable is not installed");
    assert!(matches!(missing, LaunchError::NotInstalled { .. }));
    assert!(missing.to_string().contains("Codex CLI is not installed"));
    let gone = work.path().join("gone");
    assert!(matches!(launch("claude", &gone), Err(LaunchError::MissingDirectory(_))));
    assert!(matches!(launch("gemini", work.path()), Err(LaunchError::UnknownProvider(_))));
}

#[test]
fn find_executable_accepts_explicit_paths() {
    let bin = TempDir::new().expect("bin");
    fake_bin(bin.path(), "tool", 0o755);
    let explicit = bin.path().join("tool");
    assert_eq!(find_executable(&explicit, &OsString::new()), Some(explicit.clone()));
    assert_eq!(find_executable(Path::new("tool"), &OsString::new()), None);
}

#[test]
fn rows_map_state_elapsed_and_exit_code_from_os_events() {
    let home = TempDir::new().expect("home");
    let repo = home.path().join("repo");
    let mut tracking = AgentTracking::default();
    let mut agent = running_agent(&repo);
    let start = agent.started_at.expect("started");

    let row = tracking.row(&agent, home.path(), start + Duration::seconds(75));
    assert_eq!((row.state, row.elapsed.as_str(), row.cwd_label.as_str()), (AgentState::Running, "1m 15s", "repo"));
    assert!(row.can_stop && !row.can_force && row.exit_code.is_none());

    tracking.record_exit(&mut agent, 2, AgentState::Failed);
    let row = tracking.row(&agent, home.path(), Utc::now() + Duration::hours(3));
    assert_eq!(row.state, AgentState::Failed);
    assert_eq!(row.exit_code, Some(2));
    assert!(!row.can_stop);
    assert!(!row.elapsed.starts_with('3'));
}

#[test]
fn stop_offers_force_kill_only_after_grace_period() {
    let home = TempDir::new().expect("home");
    let mut tracking = AgentTracking::default();
    let mut agent = running_agent(home.path());
    let requested = Utc::now();
    tracking.record_stop_request(&mut agent, requested);
    assert_eq!(agent.state, AgentState::Stopping);
    assert!(!tracking.row(&agent, home.path(), requested + Duration::seconds(1)).can_force);
    let late = tracking.row(&agent, home.path(), requested + Duration::seconds(STOP_GRACE_SECS));
    assert!(late.can_force && !late.can_stop);
    assert_eq!(late.cwd_label, "~");

    tracking.record_exit(&mut agent, -1, AgentState::Failed);
    let row = tracking.row(&agent, home.path(), requested + Duration::seconds(30));
    assert_eq!(row.exit_code, None);
    assert!(!row.can_force);
}

#[test]
fn restored_agents_without_end_time_show_no_running_clock() {
    let home = TempDir::new().expect("home");
    let mut agent = running_agent(home.path());
    agent.state = AgentState::Exited;
    let row = AgentTracking::default().row(&agent, home.path(), Utc::now() + Duration::hours(1));
    assert_eq!(row.elapsed, "");
    assert!(!row.can_stop);
}

#[test]
fn labels_colors_and_formatting() {
    let theme = Theme::load_default().expect("theme");
    assert_eq!(status_color(&theme, AgentState::Running), theme.status.running);
    assert_eq!(status_color(&theme, AgentState::Starting), theme.status.starting);
    assert_eq!(status_color(&theme, AgentState::Failed), theme.status.failed);
    assert_eq!(state_label(AgentState::Stopping), "Stopping");
    assert_eq!(format_elapsed(Duration::seconds(42)), "42s");
    assert_eq!(format_elapsed(Duration::seconds(3_725)), "1h 02m");
    assert_eq!(short_label(Path::new("/"), Path::new("/home/me")), "/");
    assert!(stop_agent_copy("claude", STOP_GRACE_SECS).body.contains("5s"));
}
