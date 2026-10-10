#![allow(clippy::disallowed_methods)]

use crate::tabs::{Tab, Tabs};
use crate::workspaces::{fresh_terminal, restore_notice, WorkspaceModel};
use domain::{Agent, AgentId, Axis, LayoutNode, Panel, PanelId, TabId, TerminalSessionId, WorkspaceId};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use workspace_service::{PanelSnapshot, StoreWarning, WorkspaceSnapshot, WorkspaceStore};

fn split(axis: Axis, ratio: f32, first: LayoutNode, second: LayoutNode) -> LayoutNode {
    LayoutNode::Split { axis, ratio, first: Box::new(first), second: Box::new(second) }
}

fn titled(panel: PanelId, title: &str, cwd: &Path) -> PanelSnapshot {
    PanelSnapshot {
        panel: Panel::new_terminal(panel, title.to_string(), TerminalSessionId::new()),
        cwd: cwd.to_path_buf(),
    }
}

fn subdir(root: &TempDir, name: &str) -> PathBuf {
    let path = root.path().join(name);
    std::fs::create_dir_all(&path).expect("create dir");
    path
}

fn seeded_model(home: &Path) -> WorkspaceModel {
    let mut model = WorkspaceModel::from_snapshot(WorkspaceSnapshot::default(), home);
    for name in ["Infrastructure", "Project"] {
        model.create_workspace(name.to_string());
    }
    model
}

#[test]
fn user_workspaces_round_trip_through_store_with_layout_focus_and_cwd() {
    let dir = TempDir::new().expect("tempdir");
    let database = dir.path().join("profile").join("onyxterm.db");
    let home = subdir(&dir, "home");
    let (api, web, logs) = (subdir(&dir, "api"), subdir(&dir, "web"), subdir(&dir, "logs"));
    let (store, restored) = WorkspaceStore::open(&database).expect("open");
    let mut model = WorkspaceModel::from_snapshot(restored.snapshot, &home);
    let seeded: Vec<WorkspaceId> = model.entries().iter().map(|entry| entry.workspace.id).collect();

    let backend = model.create_workspace("Backend".to_string());
    model.switch_to(backend);
    let (a, b, c, d) = (PanelId::new(), PanelId::new(), PanelId::new(), PanelId::new());
    model.add_panel(titled(a, "server", &api));
    model.add_panel(titled(b, "vim", &web));
    model.add_panel(titled(c, "tail", &logs));
    model.add_panel(titled(d, "htop", &home));
    let nested = split(
        Axis::Horizontal,
        0.3,
        LayoutNode::leaf(a),
        split(Axis::Vertical, 0.72, LayoutNode::leaf(b), LayoutNode::leaf(c)),
    );
    let first_tab = Tab::restore(TabId::new(), nested.clone(), Some(c));
    let second_tab = Tab::restore(TabId::new(), LayoutNode::leaf(d), None);
    let tab_ids = [first_tab.id(), second_tab.id()];
    *model.tabs_mut() = Tabs::restore(vec![first_tab, second_tab], 0);
    model.rename_workspace(seeded[0], "Home".to_string());
    model.delete_workspace(seeded[1]);

    store.save(model.snapshot());
    store.flush().expect("flush");
    drop(store);

    let (_store, reopened) = WorkspaceStore::open(&database).expect("reopen");
    assert!(reopened.warnings.is_empty(), "{:?}", reopened.warnings);
    let restored = WorkspaceModel::from_snapshot(reopened.snapshot, &home);
    assert_eq!(restored.snapshot(), model.snapshot());

    let names: Vec<&str> = restored.entries().iter().map(|entry| entry.workspace.name.as_str()).collect();
    assert_eq!(names, ["Home", "Project", "Backend"]);
    assert_eq!(restored.active_id(), backend);
    let tabs = restored.tabs();
    assert_eq!(tabs.iter().map(Tab::id).collect::<Vec<_>>(), tab_ids);
    assert_eq!(tabs.active_index(), 0);
    let first = tabs.active().expect("active tab");
    assert_eq!(first.layout(), &nested);
    assert_eq!(first.focused(), c);
    let titles: Vec<String> = restored.snapshot().tabs.iter().filter_map(|t| t.tab.title.clone()).collect();
    assert!(titles.ends_with(&["tail".to_string(), "htop".to_string()]), "{titles:?}");
    for (panel, cwd) in [(a, &api), (b, &web), (c, &logs), (d, &home)] {
        assert_eq!(&restored.panel(panel).expect("panel").cwd, cwd);
    }
}

#[test]
fn only_the_active_workspace_needs_spawning_and_switching_keeps_live_panels() {
    let dir = TempDir::new().expect("tempdir");
    let mut model = seeded_model(dir.path());
    let ids: Vec<WorkspaceId> = model.entries().iter().map(|entry| entry.workspace.id).collect();
    for id in &ids {
        model.switch_to(*id);
        let panel = PanelId::new();
        model.add_panel(fresh_terminal(panel, dir.path().to_path_buf()));
        model.tabs_mut().open(panel);
    }
    model.switch_to(ids[0]);
    let first = model.panels_to_spawn(|_| false);
    assert_eq!(first.len(), 1);

    let live = first.clone();
    model.switch_to(ids[1]);
    let second = model.panels_to_spawn(|panel| live.contains(&panel));
    assert_eq!(second.len(), 1);
    assert_ne!(second, first);

    let live: Vec<PanelId> = first.iter().chain(&second).copied().collect();
    model.switch_to(ids[0]);
    assert!(model.panels_to_spawn(|panel| live.contains(&panel)).is_empty());
    assert!(model.panel(first[0]).is_some());
    assert_eq!(model.locate(second[0]), Some((ids[1], 0)));
}

#[test]
fn closing_a_hidden_workspace_panel_leaves_the_active_workspace_alone() {
    let dir = TempDir::new().expect("tempdir");
    let mut model = seeded_model(dir.path());
    let ids: Vec<WorkspaceId> = model.entries().iter().map(|entry| entry.workspace.id).collect();
    model.switch_to(ids[1]);
    let hidden = PanelId::new();
    model.add_panel(fresh_terminal(hidden, dir.path().to_path_buf()));
    model.tabs_mut().open(hidden);
    model.switch_to(ids[0]);
    let visible = PanelId::new();
    model.add_panel(fresh_terminal(visible, dir.path().to_path_buf()));
    model.tabs_mut().open(visible);

    model.remove_panel(hidden).expect("remove hidden");
    assert_eq!(model.active_id(), ids[0]);
    assert_eq!(model.tabs().len(), 1);
    assert!(model.panel(hidden).is_none());
}

#[test]
fn delete_workspace_drops_its_panels_and_agents_but_never_the_last_one() {
    let dir = TempDir::new().expect("tempdir");
    let mut model = seeded_model(dir.path());
    let ids: Vec<WorkspaceId> = model.entries().iter().map(|entry| entry.workspace.id).collect();
    model.switch_to(ids[1]);
    let panel = PanelId::new();
    model.add_panel(fresh_terminal(panel, dir.path().to_path_buf()));
    model.tabs_mut().open(panel);
    let agent = Agent::new(AgentId::new(), "claude".into(), ids[1], TerminalSessionId::new(), dir.path().into());
    model.agents_mut().push(agent);

    assert_eq!(model.workspace_panels(ids[1]), vec![panel]);
    model.delete_workspace(ids[1]);
    assert!(model.panel(panel).is_none());
    assert!(model.agents().is_empty());
    assert_eq!(model.active_id(), ids[0]);

    model.delete_workspace(ids[2]);
    model.delete_workspace(ids[0]);
    assert_eq!(model.entries().len(), 1);
    assert!(!model.can_delete_workspace());
}

#[test]
fn restored_agent_panels_become_fresh_terminals() {
    let dir = TempDir::new().expect("tempdir");
    let mut model = seeded_model(dir.path());
    let panel = PanelId::new();
    model.add_panel(PanelSnapshot {
        panel: Panel::new_agent(panel, "Claude Code".into(), AgentId::new()),
        cwd: dir.path().to_path_buf(),
    });
    model.tabs_mut().open(panel);
    let restored = WorkspaceModel::from_snapshot(model.snapshot(), dir.path());
    let record = restored.panel(panel).expect("panel");
    assert_eq!(record.panel.kind, domain::PanelKind::Terminal);
    assert_eq!(record.cwd, dir.path());
}

#[test]
fn restore_notice_lists_each_warning() {
    assert_eq!(restore_notice(&[]), None);
    let notice = restore_notice(&[StoreWarning::MissingDirectory {
        workspace: WorkspaceId::new(),
        missing: PathBuf::from("/gone"),
        fallback: PathBuf::from("/home/me"),
    }])
    .expect("notice");
    assert!(notice.contains("/gone") && notice.contains("/home/me"));
}
