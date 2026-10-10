#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::error::AgentError;
use crate::event::AgentEvent;
use crate::profile::AdapterProfile;
use crate::registry::AdapterRegistry;
use async_trait::async_trait;
use domain::{Agent, AgentId, AgentState, WorkspaceId};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use terminal_core::{GridSize, TerminalService};
use tokio::sync::broadcast;
use tracing::debug;

pub const DEFAULT_GRACE_PERIOD: Duration = Duration::from_secs(5);
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[async_trait]
pub trait AgentService: Send + Sync {
    async fn launch(
        &self,
        workspace_id: WorkspaceId,
        adapter_id: &str,
        profile_name: &str,
        cwd: PathBuf,
    ) -> Result<Agent, AgentError>;

    async fn stop(&self, agent_id: AgentId, force: bool) -> Result<(), AgentError>;

    async fn status(&self, agent_id: AgentId) -> Result<AgentState, AgentError>;

    async fn list(&self, workspace_id: WorkspaceId) -> Vec<Agent>;

    async fn subscribe(&self, workspace_id: WorkspaceId) -> broadcast::Receiver<AgentEvent>;

    async fn set_state(&self, agent_id: AgentId, state: AgentState) -> Result<(), AgentError>;
}

pub struct InMemoryAgentService {
    adapters: Arc<AdapterRegistry>,
    profiles: Mutex<HashMap<String, AdapterProfile>>,
    agents: Mutex<HashMap<AgentId, Agent>>,
    terminal: Arc<dyn TerminalService>,
    event_tx: broadcast::Sender<AgentEvent>,
    grace_period: Duration,
    poll_interval: Duration,
}

impl InMemoryAgentService {
    pub fn new(
        adapters: Arc<AdapterRegistry>,
        terminal: Arc<dyn TerminalService>,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            adapters,
            profiles: Mutex::new(HashMap::new()),
            agents: Mutex::new(HashMap::new()),
            terminal,
            event_tx,
            grace_period: DEFAULT_GRACE_PERIOD,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }

    pub fn with_grace_period(mut self, period: Duration) -> Self {
        self.grace_period = period;
        self
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn seed_profile(&self, profile: AdapterProfile) {
        self.profiles
            .lock()
            .insert(profile.name.clone(), profile);
    }

    pub fn adapters(&self) -> Arc<AdapterRegistry> {
        Arc::clone(&self.adapters)
    }

    fn emit(&self, event: AgentEvent) {
        let _ = self.event_tx.send(event);
    }

    fn update_state(&self, id: AgentId, next: AgentState) -> Result<Agent, AgentError> {
        let (workspace, updated) = {
            let mut agents = self.agents.lock();
            let agent = agents
                .get_mut(&id)
                .ok_or_else(|| AgentError::AgentNotFound(id.to_string()))?;
            let next_agent = agent
                .clone()
                .transition(next)
                .map_err(|e| AgentError::InvalidTransition(e.to_string()))?;
            *agent = next_agent.clone();
            (agent.workspace, next_agent)
        };
        self.emit(AgentEvent::StateChanged {
            id,
            workspace,
            state: updated.state,
        });
        Ok(updated)
    }
}

#[async_trait]
impl AgentService for InMemoryAgentService {
    async fn launch(
        &self,
        workspace_id: WorkspaceId,
        adapter_id: &str,
        profile_name: &str,
        cwd: PathBuf,
    ) -> Result<Agent, AgentError> {
        let adapter = self
            .adapters
            .get(adapter_id)
            .ok_or_else(|| AgentError::AdapterNotFound(adapter_id.to_string()))?;
        let provider = adapter.id().to_string();

        let profile = self
            .profiles
            .lock()
            .get(profile_name)
            .cloned()
            .ok_or_else(|| AgentError::ProfileNotFound(profile_name.to_string()))?;

        profile.validate_cwd(&cwd)?;
        profile.validate()?;

        let spec = adapter.build_command(&profile, cwd.clone())?;

        let terminal_session = self
            .terminal
            .spawn(
                spec.executable.clone(),
                spec.cwd.clone(),
                GridSize::new(80, 24),
            )
            .await
            .map_err(|e| AgentError::Terminal(e.to_string()))?;

        let mut agent = Agent::new(
            AgentId::new(),
            provider.clone(),
            workspace_id,
            terminal_session,
            spec.cwd.clone(),
        );
        let starting = agent
            .clone()
            .transition(AgentState::Starting)
            .map_err(|e| AgentError::InvalidTransition(e.to_string()))?;
        let running = starting
            .transition(AgentState::Running)
            .map_err(|e| AgentError::InvalidTransition(e.to_string()))?;
        agent = running.clone();

        self.agents.lock().insert(agent.id, agent.clone());
        debug!(agent_id = %agent.id, provider = %provider, "agent launched");
        self.emit(AgentEvent::Launched {
            id: agent.id,
            workspace: workspace_id,
            provider,
            state: agent.state,
        });
        self.emit(AgentEvent::StateChanged {
            id: agent.id,
            workspace: workspace_id,
            state: agent.state,
        });
        Ok(agent)
    }

    async fn stop(&self, agent_id: AgentId, force: bool) -> Result<(), AgentError> {
        let session = {
            let agents = self.agents.lock();
            let agent = agents
                .get(&agent_id)
                .ok_or_else(|| AgentError::AgentNotFound(agent_id.to_string()))?;
            agent.terminal_session
        };

        if force {
            self.terminal
                .terminate(session)
                .await
                .map_err(|e| AgentError::Terminal(e.to_string()))?;
            let _ = self.update_state(agent_id, AgentState::Exited)?;
            return Ok(());
        }

        self.terminal
            .write(session, b"\x03")
            .await
            .map_err(|e| AgentError::Terminal(e.to_string()))?;

        let deadline = std::time::Instant::now() + self.grace_period;
        loop {
            let state = self.status(agent_id).await?;
            if matches!(state, AgentState::Exited | AgentState::Failed) {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                debug!(agent_id = %agent_id, "grace period expired");
                return Err(AgentError::StopTimeout);
            }
            tokio::time::sleep(self.poll_interval).await;
        }
    }

    async fn status(&self, agent_id: AgentId) -> Result<AgentState, AgentError> {
        self.agents
            .lock()
            .get(&agent_id)
            .map(|a| a.state)
            .ok_or_else(|| AgentError::AgentNotFound(agent_id.to_string()))
    }

    async fn list(&self, workspace_id: WorkspaceId) -> Vec<Agent> {
        self.agents
            .lock()
            .values()
            .filter(|a| a.workspace == workspace_id)
            .cloned()
            .collect()
    }

    async fn subscribe(&self, workspace_id: WorkspaceId) -> broadcast::Receiver<AgentEvent> {
        let _ = workspace_id;
        self.event_tx.subscribe()
    }

    async fn set_state(&self, agent_id: AgentId, state: AgentState) -> Result<(), AgentError> {
        let (workspace, final_state) = {
            let mut agents = self.agents.lock();
            let agent = agents
                .get_mut(&agent_id)
                .ok_or_else(|| AgentError::AgentNotFound(agent_id.to_string()))?;
            let next = agent
                .clone()
                .transition(state)
                .map_err(|e| AgentError::InvalidTransition(e.to_string()))?;
            *agent = next.clone();
            (agent.workspace, next.state)
        };
        self.emit(AgentEvent::StateChanged {
            id: agent_id,
            workspace,
            state: final_state,
        });
        if matches!(final_state, AgentState::Exited | AgentState::Failed) {
            self.emit(AgentEvent::Exited {
                id: agent_id,
                workspace,
                code: None,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use async_trait::async_trait;
    use domain::TerminalSessionId;
    use parking_lot::Mutex as PLMutex;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use terminal_core::{GridSize, TerminalError};
    use uuid::Uuid;

    #[derive(Default)]
    pub struct MockTerminalService {
        pub spawns: PLMutex<Vec<PathBuf>>,
        pub writes: PLMutex<Vec<(TerminalSessionId, Vec<u8>)>>,
        pub terminates: PLMutex<Vec<TerminalSessionId>>,
        pub resizes: PLMutex<Vec<(TerminalSessionId, GridSize)>>,
        pub snapshots: PLMutex<Vec<TerminalSessionId>>,
        pub subscribes: PLMutex<Vec<TerminalSessionId>>,
        next_id: AtomicU64,
        spawn_error: PLMutex<Option<String>>,
    }

    impl MockTerminalService {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn next_session(&self) -> TerminalSessionId {
            let n = self.next_id.fetch_add(1, Ordering::SeqCst);
            TerminalSessionId::from(Uuid::from_u64_pair(n, 0))
        }

        pub fn terminate_count(&self) -> usize {
            self.terminates.lock().len()
        }

        pub fn spawn_count(&self) -> usize {
            self.spawns.lock().len()
        }
    }

    #[async_trait]
    impl TerminalService for MockTerminalService {
        async fn spawn(
            &self,
            shell: PathBuf,
            _cwd: PathBuf,
            _size: GridSize,
        ) -> Result<TerminalSessionId, TerminalError> {
            if let Some(msg) = self.spawn_error.lock().clone() {
                return Err(TerminalError::PtySpawn(msg));
            }
            self.spawns.lock().push(shell);
            Ok(self.next_session())
        }

        async fn write(
            &self,
            session: TerminalSessionId,
            bytes: &[u8],
        ) -> Result<(), TerminalError> {
            self.writes.lock().push((session, bytes.to_vec()));
            Ok(())
        }

        async fn resize(
            &self,
            session: TerminalSessionId,
            size: GridSize,
        ) -> Result<(), TerminalError> {
            self.resizes.lock().push((session, size));
            Ok(())
        }

        async fn subscribe_output(
            &self,
            session: TerminalSessionId,
        ) -> Result<(), TerminalError> {
            self.subscribes.lock().push(session);
            Ok(())
        }

        async fn terminate(
            &self,
            session: TerminalSessionId,
        ) -> Result<(), TerminalError> {
            self.terminates.lock().push(session);
            Ok(())
        }

        async fn snapshot(
            &self,
            session: TerminalSessionId,
        ) -> Result<(), TerminalError> {
            self.snapshots.lock().push(session);
            Ok(())
        }
    }

    fn build_service() -> (
        InMemoryAgentService,
        Arc<AdapterRegistry>,
        Arc<MockTerminalService>,
    ) {
        let adapters = Arc::new(AdapterRegistry::with_defaults());
        let mock = Arc::new(MockTerminalService::new());
        let service = InMemoryAgentService::new(
            Arc::clone(&adapters) as Arc<AdapterRegistry>,
            Arc::clone(&mock) as Arc<dyn TerminalService>,
        );
        (service, adapters, mock)
    }

    fn default_profile() -> AdapterProfile {
        AdapterProfile::new("default", "claude")
            .with_args_template(vec!["--cwd".into(), "{cwd}".into()])
            .with_allowed_env_keys(vec!["PATH".into()])
    }

    #[tokio::test]
    async fn profile_validates_cwd_under_allowed_root() {
        let profile = AdapterProfile::new("p", "claude").with_allowed_cwd_roots(vec![
            PathBuf::from("/home/u/projects"),
        ]);
        assert!(profile.validate_cwd(Path::new("/home/u/projects/x")).is_ok());
        assert!(matches!(
            profile.validate_cwd(Path::new("/home/u/../etc")),
            Err(AgentError::CwdTraversal)
        ));
        assert!(matches!(
            profile.validate_cwd(Path::new("/etc/passwd")),
            Err(AgentError::CwdNotAllowed(_))
        ));
    }

    #[tokio::test]
    async fn profile_env_whitelist_rejects_unlisted_keys() {
        let profile = AdapterProfile::new("p", "claude").with_allowed_env_keys(vec![
            "PATH".into(),
        ]);
        assert!(profile.check_env_key("PATH").is_ok());
        assert!(matches!(
            profile.check_env_key("SECRET_TOKEN"),
            Err(AgentError::EnvKeyNotAllowed(_))
        ));
    }

    #[tokio::test]
    async fn agent_state_transitions_on_launch_and_exit() {
        let (service, _adapters, mock) = build_service();
        service.seed_profile(default_profile());
        let ws = WorkspaceId::new();
        let agent = service
            .launch(ws, "claude", "default", PathBuf::from("/repo"))
            .await
            .unwrap();
        assert_eq!(agent.state, AgentState::Running);
        assert!(agent.started_at.is_some());

        service
            .set_state(agent.id, AgentState::Exited)
            .await
            .unwrap();
        let state = service.status(agent.id).await.unwrap();
        assert_eq!(state, AgentState::Exited);

        assert_eq!(mock.spawn_count(), 1);
    }

    #[tokio::test]
    async fn stop_with_grace_returns_timeout_when_no_exit() {
        let adapters = Arc::new(AdapterRegistry::with_defaults());
        let mock = Arc::new(MockTerminalService::new());
        let service = InMemoryAgentService::new(
            Arc::clone(&adapters) as Arc<AdapterRegistry>,
            Arc::clone(&mock) as Arc<dyn TerminalService>,
        )
        .with_grace_period(Duration::from_millis(200))
        .with_poll_interval(Duration::from_millis(25));
        service.seed_profile(default_profile());

        let ws = WorkspaceId::new();
        let agent = service
            .launch(ws, "claude", "default", PathBuf::from("/repo"))
            .await
            .unwrap();

        let err = service.stop(agent.id, false).await.unwrap_err();
        assert!(matches!(err, AgentError::StopTimeout));
        let writes_snapshot = mock.writes.lock().clone();
        assert_eq!(writes_snapshot.len(), 1);
        assert_eq!(writes_snapshot[0].1, b"\x03".to_vec());
        drop(writes_snapshot);
        let state = service.status(agent.id).await.unwrap();
        assert_eq!(state, AgentState::Running);
    }

    #[tokio::test]
    async fn stop_force_kills_immediately() {
        let (service, _adapters, mock) = build_service();
        service.seed_profile(default_profile());

        let ws = WorkspaceId::new();
        let agent = service
            .launch(ws, "claude", "default", PathBuf::from("/repo"))
            .await
            .unwrap();

        let session = agent.terminal_session;
        service.stop(agent.id, true).await.unwrap();

        assert_eq!(mock.terminate_count(), 1);
        let terminates_snapshot = mock.terminates.lock().clone();
        assert_eq!(terminates_snapshot[0], session);
        drop(terminates_snapshot);
        let state = service.status(agent.id).await.unwrap();
        assert_eq!(state, AgentState::Exited);
    }

    #[tokio::test]
    async fn redact_env_masks_secrets() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("PATH".into(), "/usr/bin".into());
        env.insert("API_TOKEN".into(), "abc123".into());
        env.insert("DB_PASSWORD".into(), "pw".into());
        env.insert("OAUTH".into(), "tok".into());
        env.insert("CLIENT_SECRET".into(), "shh".into());
        env.insert("PRIVATE_KEY".into(), "-----".into());
        env.insert("CREDENTIAL_FILE".into(), "/creds".into());
        env.insert("NORMAL".into(), "ok".into());

        let redacted = crate::redact_env(&env);

        assert_eq!(redacted.get("PATH"), Some(&"/usr/bin".to_string()));
        assert_eq!(redacted.get("API_TOKEN"), Some(&"***".to_string()));
        assert_eq!(redacted.get("DB_PASSWORD"), Some(&"***".to_string()));
        assert_eq!(redacted.get("OAUTH"), Some(&"***".to_string()));
        assert_eq!(redacted.get("CLIENT_SECRET"), Some(&"***".to_string()));
        assert_eq!(redacted.get("PRIVATE_KEY"), Some(&"***".to_string()));
        assert_eq!(redacted.get("CREDENTIAL_FILE"), Some(&"***".to_string()));
        assert_eq!(redacted.get("NORMAL"), Some(&"ok".to_string()));
    }
}