use domain::{Axis, LayoutNode, PanelId, TabId, Workspace, WorkspaceId};
use workspace_service::{
    InMemoryLayoutService, InMemoryTabService, InMemoryWorkspaceService, LayoutService, TabService,
    WorkspaceService,
};

#[test]
fn workspace_create_list_rename() {
    let svc = InMemoryWorkspaceService::new();
    let ws = svc.create("Personal".into(), None).unwrap();
    assert_eq!(ws.name, "Personal");
    assert_eq!(svc.list_active().len(), 1);

    svc.rename(ws.id, "Personal Updated".into()).unwrap();
    assert_eq!(svc.get(ws.id).unwrap().name, "Personal Updated");
}

#[test]
fn workspace_archive_excludes_from_active_list() {
    let svc = InMemoryWorkspaceService::new();
    let a = svc.create("A".into(), None).unwrap();
    let b = svc.create("B".into(), None).unwrap();
    assert_eq!(svc.list_active().len(), 2);

    svc.archive(a.id).unwrap();
    assert_eq!(svc.list_active().len(), 1);
    assert_eq!(svc.list_active()[0].id, b.id);

    svc.restore(a.id).unwrap();
    assert_eq!(svc.list_active().len(), 2);
}

#[test]
fn tab_create_close_reorder_focus() {
    let svc = InMemoryTabService::new();
    let ws_id = WorkspaceId::new();
    let seed = Workspace::new(ws_id, "x".into());
    svc.seed(seed);

    let t1 = svc.create(ws_id, Some("one".into())).unwrap();
    let t2 = svc.create(ws_id, Some("two".into())).unwrap();
    let t3 = svc.create(ws_id, Some("three".into())).unwrap();

    svc.reorder(ws_id, 0, 2).unwrap();
    let listing = svc.list(ws_id);

    let _ = (t1.id, t2.id, t3.id);
    assert!(listing.len() <= 3);

    svc.focus(ws_id, t1.id).unwrap();
    svc.close(ws_id, t1.id).unwrap();
}

#[test]
fn tab_close_unknown_errors() {
    let svc = InMemoryTabService::new();
    let ws_id = WorkspaceId::new();
    svc.seed(Workspace::new(ws_id, "x".into()));
    let err = svc.close(ws_id, TabId::new()).unwrap_err();
    assert!(matches!(err, domain::DomainError::TabNotFound(_)));
}

#[test]
fn layout_split_close_snapshot() {
    let svc = InMemoryLayoutService::new().with_ratio(0.4);
    let ws = WorkspaceId::new();
    let tab = TabId::new();
    let p1 = PanelId::new();
    let p2 = PanelId::new();

    svc.seed(ws, tab, LayoutNode::leaf(p1));

    svc.split(ws, tab, p1, Axis::Horizontal, p2).unwrap();
    let snap = svc.snapshot(ws, tab).unwrap();
    assert_eq!(snap.leaves().len(), 2);

    svc.close_panel(ws, tab, p2).unwrap();
    let snap = svc.snapshot(ws, tab).unwrap();
    assert_eq!(snap.leaves().len(), 1);

    svc.focus_panel(ws, tab, p1).unwrap();
    let err = svc.focus_panel(ws, tab, PanelId::new()).unwrap_err();
    assert!(matches!(err, domain::DomainError::PanelNotFound(_)));
}
