use domain::{Axis, LayoutNode, Panel, PanelId, Tab, TabId, TerminalSessionId, Workspace, WorkspaceId};
use persistence::{
    PersistenceError, Repository, TabRepo, WorkspaceRepo, CURRENT_VERSION,
};
use rusqlite::params;
use std::path::PathBuf;
use tempfile::TempDir;

fn fresh_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("test.db");
    let repo = Repository::open(&path).expect("open");
    (dir, repo)
}

trait OptionalResult<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalResult<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn read_max_version(repo: &Repository) -> i64 {
    repo.pool()
        .with_conn(|c| {
            c.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
        })
        .expect("read version")
}

fn count_migrations(repo: &Repository) -> i64 {
    repo.pool()
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0)))
        .expect("count")
}

#[test]
fn migrations_apply_once() {
    let (dir, repo) = fresh_repo();
    assert_eq!(read_max_version(&repo), CURRENT_VERSION);
    drop(repo);

    let path = dir.path().join("test.db");
    let repo2 = Repository::open(&path).expect("reopen");
    assert_eq!(read_max_version(&repo2), CURRENT_VERSION);
    assert_eq!(
        count_migrations(&repo2),
        CURRENT_VERSION,
        "each migration must be applied once"
    );
}

#[test]
fn migrations_reject_downgrade() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("downgrade.db");
    let repo = Repository::open(&path).expect("open");
    repo.pool()
        .with_conn(|c| {
            c.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                params![CURRENT_VERSION + 1, "2025-01-01T00:00:00Z"],
            )
        })
        .expect("seed future version");
    drop(repo);

    match Repository::open(&path) {
        Err(PersistenceError::DowngradeForbidden { from, to }) => {
            assert_eq!(from, CURRENT_VERSION + 1);
            assert_eq!(to, CURRENT_VERSION);
        }
        Err(other) => panic!("expected downgrade forbidden, got error: {:?}", other),
        Ok(_) => panic!("expected downgrade forbidden, got success"),
    }
}

#[test]
fn workspace_repo_roundtrip() {
    let (_dir, repo) = fresh_repo();
    let workspace_id = WorkspaceId::new();
    let ws = Workspace {
        id: workspace_id,
        name: "Personal".into(),
        root: Some(PathBuf::from("/Users/personal/work")),
        tab_order: vec![],
        active_tab: None,
        archived: false,
    };
    repo.workspaces().insert(ws.clone()).expect("insert");
    let read = repo.workspaces().get(workspace_id).expect("get");
    assert_eq!(read, Some(ws));
    let all = repo.workspaces().list().expect("list");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "Personal");
}

#[test]
fn tab_repo_roundtrip() {
    let (_dir, repo) = fresh_repo();
    let ws_id = WorkspaceId::new();
    let tab_id = TabId::new();
    let panel_id = PanelId::new();
    let session_id = TerminalSessionId::new();
    let workspace = Workspace {
        id: ws_id,
        name: "W".into(),
        root: None,
        tab_order: vec![tab_id],
        active_tab: Some(tab_id),
        archived: false,
    };
    repo.workspaces().insert(workspace).expect("ws");
    let tab = Tab {
        id: tab_id,
        workspace_id: ws_id,
        title: Some("shell".into()),
        layout: LayoutNode::leaf(panel_id),
        focused_panel: Some(panel_id),
    };
    let panel = Panel::new_terminal(panel_id, "shell".into(), session_id);
    repo.tabs()
        .upsert(ws_id, tab.clone(), std::slice::from_ref(&panel))
        .expect("upsert tab");
    let read = repo
        .tabs()
        .get(ws_id, tab_id)
        .expect("get")
        .expect("present");
    assert_eq!(read, tab);
    let panels = repo.tabs().list_panels(ws_id, tab_id).expect("panels");
    assert_eq!(panels.len(), 1);
    assert_eq!(panels[0].id, panel.id);
    assert_eq!(panels[0].title, panel.title);
}

#[test]
fn layout_node_survives_json_serde() {
    let p1 = PanelId::new();
    let p2 = PanelId::new();
    let mut root = LayoutNode::leaf(p1);
    root.split_with(p1, Axis::Horizontal, 0.4, p2)
        .expect("split");
    let json = serde_json::to_string(&root).expect("encode");
    let decoded: LayoutNode = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, root);
    assert_eq!(decoded.leaves().len(), 2);
}

#[test]
fn atomic_write_rolls_back_on_error() {
    let (_dir, repo) = fresh_repo();
    let ws_id = WorkspaceId::new();
    let tab_id = TabId::new();
    let panel_id = PanelId::new();
    let workspace = Workspace {
        id: ws_id,
        name: "atomic".into(),
        root: None,
        tab_order: vec![tab_id],
        active_tab: Some(tab_id),
        archived: false,
    };
    repo.workspaces().insert(workspace).expect("ws");

    let outcome: Result<(), PersistenceError> = repo.pool().transaction(|tx| {
        tx.execute(
            "INSERT INTO tabs (id, workspace_id, title, focused_panel) VALUES (?1, ?2, ?3, ?4)",
            params![
                tab_id.to_string(),
                ws_id.to_string(),
                Some("tx".to_string()),
                Some(panel_id.to_string()),
            ],
        )?;
        tx.execute(
            "INSERT INTO layouts (tab_id, tree_json) VALUES (?1, ?2)",
            params!["definitely_nonexistent_tab_id".to_string(), "[]"],
        )?;
        Ok(())
    });
    assert!(outcome.is_err(), "expected FK constraint violation");

    let read: Option<String> = repo
        .pool()
        .with_conn(|c| {
            c.query_row(
                "SELECT id FROM tabs WHERE id = ?1",
                [tab_id.to_string()],
                |row| row.get(0),
            )
            .optional()
        })
        .expect("query");
    assert!(
        read.is_none(),
        "tab row must not exist after rolled-back transaction"
    );

    let _ = panel_id;
}

#[test]
fn restore_repairs_invalid_layout() {
    let (_dir, repo) = fresh_repo();
    let ws_id = WorkspaceId::new();
    let tab_id = TabId::new();
    let panel_id = PanelId::new();
    let workspace = Workspace {
        id: ws_id,
        name: "corrupt".into(),
        root: None,
        tab_order: vec![tab_id],
        active_tab: Some(tab_id),
        archived: false,
    };
    repo.workspaces().insert(workspace).expect("ws");

    let panel = Panel::new_terminal(panel_id, "p".into(), TerminalSessionId::new());
    repo.tabs()
        .upsert(
            ws_id,
            Tab {
                id: tab_id,
                workspace_id: ws_id,
                title: Some("dup".into()),
                layout: LayoutNode::Split {
                    axis: Axis::Horizontal,
                    ratio: 0.5,
                    first: Box::new(LayoutNode::leaf(panel_id)),
                    second: Box::new(LayoutNode::leaf(panel_id)),
                },
                focused_panel: Some(panel_id),
            },
            std::slice::from_ref(&panel),
        )
        .expect("seed tab with duplicate leaf");

    let snapshot = repo.restore().expect("restore");
    assert_eq!(snapshot.workspaces.len(), 1);
    assert_eq!(snapshot.tabs.len(), 1);
    let restored_tab = &snapshot.tabs[0];
    assert_eq!(restored_tab.id, tab_id);
    assert!(
        restored_tab.layout.validate().is_ok(),
        "repaired layout must validate"
    );
    assert_eq!(
        snapshot.warnings.len(),
        0,
        "duplicate-leaf repair should not drop the tab"
    );
    assert_eq!(restored_tab.layout.leaves().len(), 1);
    assert!(
        snapshot.panels.iter().any(|p| p.id == panel_id),
        "kept panels should reference remaining layout leaves"
    );
}



