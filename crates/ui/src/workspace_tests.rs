#![allow(clippy::disallowed_methods)]

use crate::tabs::{Tab, Tabs};
use crate::workspaces::{fresh_terminal, restore_notice, WorkspaceModel};
use domain::{Axis, LayoutNode, Panel, PanelId, TabId, TerminalSessionId, WorkspaceId};
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
        name: None,
        path: None,
    }
}

fn subdir(root: &TempDir, name: &str) -> PathBuf {
    let path = root.path().join(name);
    std::fs::create_dir_all(&path).expect("create dir");
    path
}

fn seeded_model(home: &Path) -> WorkspaceModel {
    let mut model = WorkspaceModel::from_snapshot(WorkspaceSnapshot::default(), home);
    for name in ["Personal", "Infrastructure", "Project"] {
        model.create_workspace(name.to_string());
    }
    model
}

fn workspace_ids(model: &WorkspaceModel) -> Vec<WorkspaceId> {
    model.entries().iter().filter(|entry| !entry.is_scratch).map(|entry| entry.workspace.id).collect()
}

fn open_terminal_tab(model: &mut WorkspaceModel, cwd: &Path) -> PanelId {
    let panel = PanelId::new();
    model.add_panel(fresh_terminal(panel, cwd.to_path_buf()));
    model.tabs_mut().open(panel);
    panel
}

#[test]
fn user_workspaces_round_trip_through_store_with_layout_focus_and_cwd() {
    let dir = TempDir::new().expect("tempdir");
    let database = dir.path().join("profile").join("onyxterm.db");
    let home = subdir(&dir, "home");
    let (api, web, logs) = (subdir(&dir, "api"), subdir(&dir, "web"), subdir(&dir, "logs"));
    let (store, restored) = WorkspaceStore::open(&database).expect("open");
    let mut model = WorkspaceModel::from_snapshot(restored.snapshot, &home);
    let seeded = [model.create_workspace("Home".to_string()), model.create_workspace("Infra".to_string())];
    for id in seeded {
        model.switch_to(id);
        open_terminal_tab(&mut model, &home);
    }

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
    let mut second_tab = Tab::restore(TabId::new(), LayoutNode::leaf(d), None);
    second_tab.rename(Some("  Monitoring ".to_string()));
    let tab_ids = [first_tab.id(), second_tab.id()];
    *model.tabs_mut() = Tabs::restore(vec![first_tab, second_tab], 0);
    model.rename_workspace(seeded[0], "Home Base".to_string());
    model.delete_workspace(seeded[1]);

    store.save(model.snapshot());
    store.flush().expect("flush");
    drop(store);

    let (_store, reopened) = WorkspaceStore::open(&database).expect("reopen");
    assert!(reopened.warnings.is_empty(), "{:?}", reopened.warnings);
    let restored = WorkspaceModel::from_snapshot(reopened.snapshot, &home);
    assert_eq!(restored.snapshot(), model.snapshot());

    let names: Vec<&str> = restored.entries().iter().map(|entry| entry.workspace.name.as_str()).collect();
    assert_eq!(names, ["Home Base", "Backend"]);
    assert!(restored.entries().iter().all(|entry| !entry.is_scratch));
    assert_eq!(restored.active_id(), backend);
    let tabs = restored.tabs();
    assert_eq!(tabs.iter().map(Tab::id).collect::<Vec<_>>(), tab_ids);
    assert_eq!(tabs.active_index(), 0);
    let first = tabs.active().expect("active tab");
    assert_eq!(first.layout(), &nested);
    assert_eq!(first.focused(), c);
    let names: Vec<Option<&str>> = tabs.iter().map(Tab::name).collect();
    assert_eq!(names, [None, Some("Monitoring")]);
    let pane_titles: Vec<&str> = [a, b, c, d]
        .iter()
        .map(|panel| restored.panel(*panel).expect("panel").panel.title.as_str())
        .collect();
    assert_eq!(pane_titles, ["server", "vim", "tail", "htop"]);
    for (panel, cwd) in [(a, &api), (b, &web), (c, &logs), (d, &home)] {
        assert_eq!(&restored.panel(panel).expect("panel").cwd, cwd);
    }
}

#[test]
fn only_the_active_workspace_needs_spawning_and_switching_keeps_live_panels() {
    let dir = TempDir::new().expect("tempdir");
    let mut model = seeded_model(dir.path());
    let ids = workspace_ids(&model);
    for id in &ids {
        model.switch_to(*id);
        open_terminal_tab(&mut model, dir.path());
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
    let ids = workspace_ids(&model);
    model.switch_to(ids[1]);
    let hidden = open_terminal_tab(&mut model, dir.path());
    model.switch_to(ids[0]);
    open_terminal_tab(&mut model, dir.path());

    model.remove_panel(hidden).expect("remove hidden");
    assert_eq!(model.active_id(), ids[0]);
    assert_eq!(model.tabs().len(), 1);
    assert!(model.panel(hidden).is_none());
}

#[test]
fn delete_workspace_drops_its_panels_and_falls_back_to_scratch_when_last() {
    let dir = TempDir::new().expect("tempdir");
    let mut model = seeded_model(dir.path());
    let ids = workspace_ids(&model);
    model.switch_to(ids[1]);
    let panel = PanelId::new();
    model.add_panel(fresh_terminal(panel, dir.path().to_path_buf()));
    model.tabs_mut().open(panel);

    assert_eq!(model.workspace_panels(ids[1]), vec![panel]);
    model.delete_workspace(ids[1]);
    assert!(model.panel(panel).is_none());
    assert_eq!(model.active_id(), ids[0]);

    model.delete_workspace(ids[2]);
    model.delete_workspace(ids[0]);
    assert_eq!(model.entries().len(), 1);
    assert!(model.entries()[0].is_scratch);
    assert!(model.snapshot().workspaces.is_empty());
}

#[test]
fn deleting_the_last_workspace_falls_back_to_scratch() {
    let dir = TempDir::new().expect("tempdir");
    let mut model = WorkspaceModel::from_snapshot(WorkspaceSnapshot::default(), dir.path());
    open_terminal_tab(&mut model, dir.path());
    let scratch_panel = model.tabs().active().expect("scratch tab").focused();
    model.remove_panel(scratch_panel).expect("close scratch");
    let mine = model.create_workspace("Mine".to_string());
    model.switch_to(mine);
    open_terminal_tab(&mut model, dir.path());

    model.delete_workspace(mine);
    assert_eq!(model.entries().len(), 1);
    assert!(model.entries()[0].is_scratch);
    assert!(model.tabs().is_empty());
    model.delete_workspace(model.active_id());
    assert_eq!(model.entries().len(), 1);
}

#[test]
fn first_run_is_a_single_unpersisted_scratch_session() {
    let dir = TempDir::new().expect("tempdir");
    let database = dir.path().join("profile").join("onyxterm.db");
    let (store, restored) = WorkspaceStore::open(&database).expect("open");
    assert!(restored.snapshot.workspaces.is_empty());
    let mut model = WorkspaceModel::from_snapshot(restored.snapshot, dir.path());
    assert_eq!(model.entries().len(), 1);
    assert!(model.entries()[0].is_scratch);
    assert_eq!(model.entries()[0].workspace.name, crate::workspaces::SCRATCH_NAME);
    open_terminal_tab(&mut model, dir.path());

    let snapshot = model.snapshot();
    assert!(snapshot.workspaces.is_empty() && snapshot.tabs.is_empty());
    assert_eq!(snapshot.active_workspace, None);
    store.save(snapshot);
    store.flush().expect("flush");
    drop(store);
    let (_store, reopened) = WorkspaceStore::open(&database).expect("reopen");
    assert!(reopened.snapshot.workspaces.is_empty());
}

#[test]
fn scratch_disappears_when_closed_once_a_workspace_exists() {
    let dir = TempDir::new().expect("tempdir");
    let mut model = WorkspaceModel::from_snapshot(WorkspaceSnapshot::default(), dir.path());
    let scratch_panel = open_terminal_tab(&mut model, dir.path());
    let mine = model.create_workspace("Mine".to_string());
    assert_eq!(model.entries().len(), 2);
    assert!(model.entries()[0].is_scratch);

    model.remove_panel(scratch_panel).expect("close scratch");
    assert_eq!(model.entries().len(), 1);
    assert_eq!(model.active_id(), mine);
}

#[test]
fn created_workspace_is_exactly_what_comes_back_after_reopen() {
    let dir = TempDir::new().expect("tempdir");
    let database = dir.path().join("profile").join("onyxterm.db");
    let (store, restored) = WorkspaceStore::open(&database).expect("open");
    let mut model = WorkspaceModel::from_snapshot(restored.snapshot, dir.path());
    open_terminal_tab(&mut model, dir.path());
    let mine = model.create_workspace("Mine".to_string());
    model.switch_to(mine);
    let panel = open_terminal_tab(&mut model, dir.path());
    store.save(model.snapshot());
    store.flush().expect("flush");
    drop(store);

    let (_store, reopened) = WorkspaceStore::open(&database).expect("reopen");
    let restored = WorkspaceModel::from_snapshot(reopened.snapshot, dir.path());
    let names: Vec<&str> = restored.entries().iter().map(|entry| entry.workspace.name.as_str()).collect();
    assert_eq!(names, ["Mine"]);
    assert_eq!(restored.active_id(), mine);
    assert_eq!(restored.tabs().active().map(|tab| tab.focused()), Some(panel));
    assert_eq!(restored.snapshot(), model.snapshot());
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

#[test]
fn moving_panes_keeps_their_sessions_and_persists_the_layout() {
    use domain::DropZone;
    let root = TempDir::new().unwrap();
    let mut model = seeded_model(root.path());
    model.switch_to(workspace_ids(&model)[0]);
    let first = open_terminal_tab(&mut model, root.path());
    let second = open_terminal_tab(&mut model, root.path());
    let session_of = |model: &WorkspaceModel, panel: PanelId| model.panel(panel).unwrap().panel.resource;
    let (first_session, second_session) = (session_of(&model, first), session_of(&model, second));
    let tab_count = model.tabs().len();

    model.tabs_mut().move_panel_to_tab(second, tab_count - 2).unwrap();
    assert_eq!(model.tabs().len(), tab_count - 1);
    let tab = model.tabs().active().unwrap();
    assert_eq!(tab.layout().leaves(), vec![first, second]);
    assert_eq!(tab.focused(), second);

    model.tabs_mut().active_mut().unwrap().move_panel(second, first, DropZone::Top).unwrap();
    let expected = split(Axis::Vertical, 0.5, LayoutNode::leaf(second), LayoutNode::leaf(first));
    assert_eq!(model.tabs().active().unwrap().layout(), &expected);
    assert_eq!(session_of(&model, first), first_session);
    assert_eq!(session_of(&model, second), second_session);

    let snapshot = model.snapshot();
    let restored = WorkspaceModel::from_snapshot(snapshot, root.path());
    assert_eq!(restored.tabs().active().unwrap().layout(), &expected);
}

#[test]
fn moving_a_pane_into_its_own_tab_or_a_missing_tab_is_rejected() {
    let root = TempDir::new().unwrap();
    let mut model = seeded_model(root.path());
    let panel = open_terminal_tab(&mut model, root.path());
    let own = model.tabs().position_of(panel).unwrap();
    assert!(model.tabs_mut().move_panel_to_tab(panel, own).is_err());
    assert!(model.tabs_mut().move_panel_to_tab(panel, 99).is_err());
    assert!(model.tabs().iter().any(|tab| tab.contains(panel)));
}

#[test]
fn workspaces_resolve_by_index_and_neighbors_wrap_in_entry_order() {
    let dir = TempDir::new().expect("tempdir");
    let mut model = seeded_model(dir.path());
    let order: Vec<WorkspaceId> = model.entries().iter().map(|entry| entry.workspace.id).collect();
    assert_eq!(model.workspace_id_at(1), Some(order[1]));
    assert_eq!(model.workspace_id_at(order.len()), None);
    model.switch_to(order[0]);
    assert_eq!(model.neighbor_workspace_id(1), order[1]);
    assert_eq!(model.neighbor_workspace_id(-1), order[order.len() - 1]);
    model.switch_to(order[order.len() - 1]);
    assert_eq!(model.neighbor_workspace_id(1), order[0]);
}

fn editor_snapshot(panel: PanelId, file: &Path) -> PanelSnapshot {
    PanelSnapshot {
        panel: Panel::new_editor(panel, "main.rs".to_string()),
        cwd: file.parent().map(Path::to_path_buf).unwrap_or_default(),
        name: None,
        path: Some(file.to_path_buf()),
    }
}

#[test]
fn document_panels_are_not_spawned_as_terminals() {
    let home = TempDir::new().expect("home");
    let mut model = seeded_model(home.path());
    let terminal = open_terminal_tab(&mut model, home.path());
    let editor = PanelId::new();
    model.add_panel(editor_snapshot(editor, &home.path().join("main.rs")));
    model.tabs_mut().active_mut().expect("tab").split_focused(Axis::Horizontal, editor).expect("split");

    assert_eq!(model.terminals_to_spawn(|_| false), vec![terminal]);
    assert_eq!(model.documents_to_restore(|_| false), vec![editor]);
    assert!(model.documents_to_restore(|panel| panel == editor).is_empty());
}

#[test]
fn focused_document_has_no_terminal_handle() {
    let home = TempDir::new().expect("home");
    let mut model = seeded_model(home.path());
    open_terminal_tab(&mut model, home.path());
    let editor = PanelId::new();
    model.add_panel(editor_snapshot(editor, &home.path().join("main.rs")));
    model.tabs_mut().active_mut().expect("tab").split_focused(Axis::Horizontal, editor).expect("split");

    assert_eq!(model.focused_document(), Some(editor));
    model.tabs_mut().active_mut().expect("tab").focus_next();
    assert_eq!(model.focused_document(), None);
}

fn file_tree_snapshot(panel: PanelId, root: &Path) -> PanelSnapshot {
    PanelSnapshot {
        panel: Panel::new_file_tree(panel, "proj".to_string()),
        cwd: root.to_path_buf(),
        name: None,
        path: Some(root.to_path_buf()),
    }
}

#[test]
fn file_tree_in_active_tab_finds_the_tree_rooted_at_the_folder() {
    let home = TempDir::new().expect("home");
    let mut model = seeded_model(home.path());
    open_terminal_tab(&mut model, home.path());
    let tree = PanelId::new();
    model.add_panel(file_tree_snapshot(tree, home.path()));
    model.tabs_mut().active_mut().expect("tab").split_focused(Axis::Horizontal, tree).expect("split");

    assert_eq!(model.file_tree_in_active_tab(home.path()), Some(tree));
}

#[test]
fn file_tree_in_active_tab_ignores_other_folders() {
    let home = TempDir::new().expect("home");
    let mut model = seeded_model(home.path());
    open_terminal_tab(&mut model, home.path());
    let tree = PanelId::new();
    model.add_panel(file_tree_snapshot(tree, home.path()));
    model.tabs_mut().active_mut().expect("tab").split_focused(Axis::Horizontal, tree).expect("split");

    assert_eq!(model.file_tree_in_active_tab(&home.path().join("other")), None);
}

#[test]
fn any_file_tree_in_active_tab_finds_a_tree_whatever_its_root() {
    let home = TempDir::new().expect("home");
    let mut model = seeded_model(home.path());
    open_terminal_tab(&mut model, home.path());
    assert_eq!(model.any_file_tree_in_active_tab(), None);

    let tree = PanelId::new();
    model.add_panel(file_tree_snapshot(tree, &home.path().join("elsewhere")));
    model.tabs_mut().active_mut().expect("tab").split_focused(Axis::Horizontal, tree).expect("split");

    assert_eq!(model.any_file_tree_in_active_tab(), Some(tree));
}
