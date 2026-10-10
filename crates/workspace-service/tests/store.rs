use domain::{Agent, AgentId, AgentState, PanelKind, TerminalSessionId};
use persistence::{PersistenceError, Repository};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use workspace_service::{
    new_terminal_tab, RestoredState, StoreError, StoreWarning, WorkspaceSnapshot, WorkspaceStore,
    SAVE_DEBOUNCE, SEEDED_WORKSPACE_NAMES,
};

fn db_path(dir: &TempDir) -> PathBuf {
    dir.path().join("profile").join("onyxterm.db")
}

fn open(path: &Path) -> (WorkspaceStore, RestoredState) {
    WorkspaceStore::open(path).expect("open store")
}

fn home() -> PathBuf {
    platform::home_dir().expect("home")
}

fn renamed(snapshot: &WorkspaceSnapshot, name: &str) -> WorkspaceSnapshot {
    let mut next = snapshot.clone();
    next.workspaces[0].name = name.to_string();
    next
}

#[test]
fn first_run_seeds_three_workspaces_with_one_terminal_tab_each() {
    let dir = TempDir::new().expect("tempdir");
    let (_store, restored) = open(&db_path(&dir));
    let snapshot = restored.snapshot;

    let names: Vec<&str> = snapshot.workspaces.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, SEEDED_WORKSPACE_NAMES);
    assert!(restored.warnings.is_empty());
    assert_eq!(snapshot.active_workspace, Some(snapshot.workspaces[0].id));
    for workspace in &snapshot.workspaces {
        let tabs: Vec<_> = snapshot.tabs.iter().filter(|t| t.tab.workspace_id == workspace.id).collect();
        assert_eq!(tabs.len(), 1);
        assert_eq!(workspace.tab_order, vec![tabs[0].tab.id]);
        assert_eq!(workspace.active_tab, Some(tabs[0].tab.id));
        assert_eq!(tabs[0].panels.len(), 1);
        assert_eq!(tabs[0].panels[0].panel.kind, PanelKind::Terminal);
        assert_eq!(tabs[0].panels[0].cwd, home());
    }
}

#[test]
fn save_drop_reopen_roundtrips_state() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir);
    let (store, restored) = open(&path);
    let mut changed = renamed(&restored.snapshot, "Renamed");
    let workspace = changed.workspaces[1].id;
    let extra = new_terminal_tab(workspace, dir.path().to_path_buf());
    changed.workspaces[1].tab_order.push(extra.tab.id);
    changed.workspaces[1].active_tab = Some(extra.tab.id);
    changed.tabs.push(extra);
    changed.active_workspace = Some(workspace);
    store.save(changed.clone());
    drop(store);

    let (_store, reopened) = open(&path);
    assert!(reopened.warnings.is_empty());
    assert_eq!(reopened.snapshot.workspaces, changed.workspaces);
    assert_eq!(reopened.snapshot.active_workspace, Some(workspace));
    for tab in &changed.tabs {
        assert!(reopened.snapshot.tabs.contains(tab), "missing tab {}", tab.tab.id);
    }
}

#[test]
fn rapid_saves_coalesce_into_one_write_of_the_latest_snapshot() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir);
    let (store, restored) = open(&path);
    for index in 0..50 {
        store.save(renamed(&restored.snapshot, &format!("Name {index}")));
    }
    store.flush().expect("flush");
    assert_eq!(store.completed_writes(), 1);
    drop(store);

    let (_store, reopened) = open(&path);
    assert_eq!(reopened.snapshot.workspaces[0].name, "Name 49");
}

#[test]
fn pending_save_is_written_once_the_debounce_window_elapses() {
    let dir = TempDir::new().expect("tempdir");
    let (store, restored) = open(&db_path(&dir));
    store.save(renamed(&restored.snapshot, "Debounced"));
    assert_eq!(store.completed_writes(), 0);
    std::thread::sleep(SAVE_DEBOUNCE * 3);
    assert_eq!(store.completed_writes(), 1);
}

#[test]
fn flush_blocks_until_pending_snapshot_is_written() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir);
    let (store, restored) = open(&path);
    store.save(renamed(&restored.snapshot, "Flushed"));
    store.flush().expect("flush");

    let reader = Repository::open(&path).expect("second connection");
    let loaded = reader.load_snapshot(&home()).expect("load");
    assert_eq!(loaded.snapshot.workspaces[0].name, "Flushed");
}

#[test]
fn corrupt_database_is_quarantined_and_replaced_with_fresh_state() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(&path, b"this is definitely not a sqlite database file").expect("garbage");

    let (_store, restored) = open(&path);
    let quarantined_to = match restored.warnings.as_slice() {
        [StoreWarning::DatabaseQuarantined { quarantined_to, .. }] => quarantined_to.clone(),
        other => panic!("expected single quarantine warning, got {other:?}"),
    };
    let quarantined_name = quarantined_to.file_name().expect("name").to_string_lossy().into_owned();
    assert!(quarantined_name.starts_with("onyxterm.db.corrupt-"), "{quarantined_name}");
    assert_eq!(
        std::fs::read(&quarantined_to).expect("quarantined bytes"),
        b"this is definitely not a sqlite database file"
    );
    assert_eq!(restored.snapshot.workspaces.len(), SEEDED_WORKSPACE_NAMES.len());
}

#[test]
fn newer_schema_version_errors_without_touching_the_file() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir);
    drop(open(&path));
    let repository = Repository::open(&path).expect("raw open");
    repository
        .pool()
        .with_conn(|c| {
            c.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (999, 'future')",
                (),
            )
        })
        .expect("bump version");
    drop(repository);

    let result = WorkspaceStore::open(&path);
    assert!(matches!(
        result,
        Err(StoreError::Persistence(PersistenceError::DowngradeForbidden { .. }))
    ));
    let siblings = std::fs::read_dir(path.parent().expect("parent")).expect("list");
    assert!(siblings
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
        .all(|name| !name.contains("corrupt")));
}

#[test]
fn missing_workspace_root_falls_back_to_home_with_warning() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir);
    let gone = dir.path().join("deleted-project");
    std::fs::create_dir_all(&gone).expect("mkdir root");
    let (store, restored) = open(&path);
    let mut changed = restored.snapshot.clone();
    changed.workspaces[0].root = Some(gone.clone());
    let workspace = changed.workspaces[0].id;
    store.save(changed);
    drop(store);
    std::fs::remove_dir_all(&gone).expect("remove root");

    let (_store, reopened) = open(&path);
    assert_eq!(reopened.snapshot.workspaces[0].root, Some(home()));
    assert!(reopened.warnings.contains(&StoreWarning::MissingDirectory {
        workspace,
        missing: gone,
        fallback: home(),
    }));
}

#[test]
fn running_agent_is_restored_as_exited_and_never_resumed() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir);
    let (store, restored) = open(&path);
    let mut changed = restored.snapshot.clone();
    let mut agent = Agent::new(
        AgentId::new(),
        "claude".into(),
        changed.workspaces[0].id,
        TerminalSessionId::new(),
        home(),
    );
    agent.state = AgentState::Running;
    changed.agents.push(agent.clone());
    store.save(changed);
    drop(store);

    let (_store, reopened) = open(&path);
    assert_eq!(reopened.snapshot.agents.len(), 1);
    assert_eq!(reopened.snapshot.agents[0].id, agent.id);
    assert_eq!(reopened.snapshot.agents[0].state, AgentState::Exited);
}
