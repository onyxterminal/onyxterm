use domain::{LayoutNode, Panel, PanelId, Tab, TabId, TerminalSessionId, Workspace, WorkspaceId};
use persistence::{PanelSnapshot, Repository, TabSnapshot, WorkspaceSnapshot};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn workspace_with_tab(name: &str, cwd: &str) -> (Workspace, TabSnapshot) {
    let mut workspace = Workspace::new(WorkspaceId::new(), name.to_string());
    let panel_id = PanelId::new();
    let tab = Tab::new(TabId::new(), workspace.id, LayoutNode::leaf(panel_id));
    workspace.tab_order = vec![tab.id];
    workspace.active_tab = Some(tab.id);
    let panel = Panel::new_terminal(panel_id, "Terminal".into(), TerminalSessionId::new());
    let panels = vec![PanelSnapshot { panel, cwd: PathBuf::from(cwd), name: Some(format!("{name} pane")), path: None }];
    (workspace, TabSnapshot { tab, panels })
}

#[test]
fn snapshot_roundtrip_replaces_previous_state() {
    let dir = TempDir::new().expect("tempdir");
    let repo = Repository::open(&dir.path().join("snap.db")).expect("open");
    let (first_ws, first_tab) = workspace_with_tab("First", "/first");
    let (second_ws, second_tab) = workspace_with_tab("Second", "/second");
    let full = WorkspaceSnapshot {
        workspaces: vec![first_ws.clone(), second_ws.clone()],
        tabs: vec![first_tab.clone(), second_tab],
        agents: Vec::new(),
        active_workspace: Some(second_ws.id),
    };
    repo.write_snapshot(&full).expect("write full");

    let trimmed = WorkspaceSnapshot {
        workspaces: vec![first_ws.clone()],
        tabs: vec![first_tab.clone()],
        agents: Vec::new(),
        active_workspace: Some(first_ws.id),
    };
    repo.write_snapshot(&trimmed).expect("write trimmed");

    let loaded = repo.load_snapshot(Path::new("/default")).expect("load");
    assert!(loaded.warnings.is_empty());
    assert_eq!(loaded.snapshot, trimmed);
}

#[test]
fn editor_panel_roundtrips_kind_and_path() {
    let dir = TempDir::new().expect("tempdir");
    let repo = Repository::open(&dir.path().join("snap.db")).expect("open");
    let mut workspace = Workspace::new(WorkspaceId::new(), "Code".to_string());
    let editor_id = PanelId::new();
    let tab = Tab::new(TabId::new(), workspace.id, LayoutNode::leaf(editor_id));
    workspace.tab_order = vec![tab.id];
    workspace.active_tab = Some(tab.id);
    let editor = PanelSnapshot {
        panel: Panel::new_editor(editor_id, "main.rs".into()),
        cwd: PathBuf::from("/code/src"),
        name: None,
        path: Some(PathBuf::from("/code/src/main.rs")),
    };
    let snapshot = WorkspaceSnapshot {
        workspaces: vec![workspace.clone()],
        tabs: vec![TabSnapshot { tab, panels: vec![editor.clone()] }],
        agents: Vec::new(),
        active_workspace: Some(workspace.id),
    };
    repo.write_snapshot(&snapshot).expect("write");

    let loaded = repo.load_snapshot(Path::new("/default")).expect("load");
    assert_eq!(loaded.snapshot.tabs[0].panels, vec![editor]);
}
