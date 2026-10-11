use domain::{
    Agent, AgentId, AgentState, Axis, DomainError, Panel, PanelId, ResourceRef, Tab, TabId,
    TerminalSession, TerminalSessionId, Workspace, WorkspaceId,
};
use std::path::PathBuf;

#[test]
fn id_newtypes_are_unique_and_have_prefixes() {
    let a = WorkspaceId::new();
    let b = WorkspaceId::new();
    assert_ne!(a, b);
    assert!(a.to_string().starts_with("ws_"));

    let t = TabId::new();
    assert!(t.to_string().starts_with("tab_"));

    let p = PanelId::new();
    assert!(p.to_string().starts_with("panel_"));

    let s = TerminalSessionId::new();
    assert!(s.to_string().starts_with("term_"));

    let ag = AgentId::new();
    assert!(ag.to_string().starts_with("agent_"));
}

#[test]
fn panel_construction_resolves_resource() {
    let session = TerminalSessionId::new();
    let panel = Panel::new_terminal(PanelId::new(), "shell".into(), session);
    assert_eq!(panel.resource, ResourceRef::Terminal(session));

    let agent = AgentId::new();
    let panel = Panel::new_agent(PanelId::new(), "codex".into(), agent);
    assert_eq!(panel.resource, ResourceRef::Agent(agent));
}

#[test]
fn terminal_session_is_alive_until_exit() {
    let id = TerminalSessionId::new();
    let mut session = TerminalSession::new(id, PathBuf::from("/tmp"), PathBuf::from("/bin/zsh"));
    assert!(session.is_alive());
    session.process_status = domain::ProcessStatus::Exited(0);
    assert!(!session.is_alive());
}

#[test]
fn layout_leaf_collects_single_panel() {
    let node = domain::LayoutNode::leaf(PanelId::new());
    assert_eq!(node.leaves().len(), 1);
    node.validate().expect("leaf is valid");
}

#[test]
fn layout_split_balances_and_rejects_invalid_ratio() {
    let mut root = domain::LayoutNode::leaf(PanelId::new());
    let p2 = PanelId::new();
    root.split_with(root.leaves()[0], Axis::Horizontal, 0.5, p2)
        .expect("split ok");
    assert_eq!(root.leaves().len(), 2);
    root.validate().expect("valid");

    let err = root
        .split_with(p2, Axis::Vertical, 1.5, PanelId::new())
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidRatio(_)));

    let err = root
        .split_with(p2, Axis::Vertical, 0.0, PanelId::new())
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidRatio(_)));
}

#[test]
fn layout_close_leaf_collapses_parent() {
    let p1 = PanelId::new();
    let p2 = PanelId::new();
    let mut root = domain::LayoutNode::leaf(p1);
    root.split_with(p1, Axis::Horizontal, 0.4, p2).unwrap();
    assert_eq!(root.leaves().len(), 2);
    assert!(root.close_leaf(&mut |id| id == p2).unwrap());
    assert_eq!(root.leaves(), vec![p1]);
}

#[test]
fn layout_close_leaf_reaches_nested_second_branch() {
    let p1 = PanelId::new();
    let p2 = PanelId::new();
    let p3 = PanelId::new();
    let mut root = domain::LayoutNode::leaf(p1);
    root.split_with(p1, Axis::Horizontal, 0.5, p2).unwrap();
    root.split_with(p2, Axis::Vertical, 0.5, p3).unwrap();
    assert!(root.close_leaf(&mut |id| id == p3).unwrap());
    assert_eq!(root.leaves(), vec![p1, p2]);
}

#[test]
fn layout_validate_rejects_duplicate_panel() {
    let p = PanelId::new();
    let inner = domain::LayoutNode::Split {
        axis: Axis::Vertical,
        ratio: 0.5,
        first: Box::new(domain::LayoutNode::leaf(p)),
        second: Box::new(domain::LayoutNode::leaf(p)),
    };
    let err = inner.validate().unwrap_err();
    assert!(matches!(err, DomainError::LayoutCycle));
}

#[test]
fn workspace_reducer_add_remove_reorder() {
    let wid = WorkspaceId::new();
    let ws = Workspace::new(wid, "Personal".into());
    let mut reducer = domain::WorkspaceReducer::new(ws);

    let t1 = TabId::new();
    let t2 = TabId::new();
    let t3 = TabId::new();

    reducer
        .add_tab(Tab::new(t1, wid, domain::LayoutNode::leaf(PanelId::new())), None)
        .unwrap();
    reducer
        .add_tab(Tab::new(t2, wid, domain::LayoutNode::leaf(PanelId::new())), None)
        .unwrap();
    reducer
        .add_tab(Tab::new(t3, wid, domain::LayoutNode::leaf(PanelId::new())), None)
        .unwrap();

    assert_eq!(reducer.workspace().tab_order, vec![t1, t2, t3]);
    assert_eq!(reducer.workspace().active_tab, Some(t1));

    reducer.set_active_tab(Some(t2)).unwrap();
    assert_eq!(reducer.workspace().active_tab, Some(t2));

    reducer.reorder_tab(2, 0).unwrap();
    assert_eq!(reducer.workspace().tab_order, vec![t3, t1, t2]);

    reducer.remove_tab(t1).unwrap();
    assert_eq!(reducer.workspace().tab_order, vec![t3, t2]);

    let err = reducer
        .add_tab(
            Tab::new(TabId::new(), wid, domain::LayoutNode::leaf(PanelId::new())),
            None,
        )
        .unwrap();
    assert!(reducer.workspace().validate().is_ok());
    assert!(matches!(err, domain::WorkspaceReducerEvent::TabCreated(_)));
}

#[test]
fn workspace_reducer_rejects_duplicate_tab() {
    let wid = WorkspaceId::new();
    let ws = Workspace::new(wid, "x".into());
    let mut reducer = domain::WorkspaceReducer::new(ws);
    let tab = Tab::new(TabId::new(), wid, domain::LayoutNode::leaf(PanelId::new()));
    reducer.add_tab(tab.clone(), None).unwrap();
    let err = reducer.add_tab(tab, None).unwrap_err();
    assert!(matches!(err, DomainError::DuplicateTab));
}

#[test]
fn workspace_rejection_when_active_not_in_order() {
    let wid = WorkspaceId::new();
    let ws = Workspace::new(wid, "x".into());
    let mut reducer = domain::WorkspaceReducer::new(ws);
    let err = reducer.set_active_tab(Some(TabId::new())).unwrap_err();
    assert!(matches!(err, DomainError::InconsistentActiveTab));
}

#[test]
fn agent_state_transitions_follow_rules() {
    let ag = Agent::new(
        AgentId::new(),
        "claude".into(),
        WorkspaceId::new(),
        TerminalSessionId::new(),
        PathBuf::from("/work"),
    );
    let running = ag.clone().transition(AgentState::Running).unwrap();
    assert_eq!(running.state, AgentState::Running);
    assert!(running.started_at.is_some());

    let stopping = running.clone().transition(AgentState::Stopping).unwrap();
    let exited = stopping.transition(AgentState::Exited).unwrap();
    assert!(exited.ended_at.is_some());
    assert!(!exited.state.is_alive());
}

#[test]
fn agent_state_rejects_invalid_transitions() {
    let ag = Agent::new(
        AgentId::new(),
        "codex".into(),
        WorkspaceId::new(),
        TerminalSessionId::new(),
        PathBuf::from("/work"),
    );
    let started = ag.clone().transition(AgentState::Starting).unwrap();
    let running = started.transition(AgentState::Running).unwrap();
    let exited = running.transition(AgentState::Exited).unwrap();
    assert!(!exited.state.is_alive());

    let err = exited.transition(AgentState::Running).unwrap_err();
    assert!(matches!(err, DomainError::InvalidAgentTransition { .. }));
}

#[test]
fn unknown_state_recovers() {
    let ag = Agent::new(
        AgentId::new(),
        "claude".into(),
        WorkspaceId::new(),
        TerminalSessionId::new(),
        PathBuf::from("/work"),
    );
    let unknown = ag.transition(AgentState::Unknown).unwrap();
    let running = unknown.transition(AgentState::Running).unwrap();
    assert_eq!(running.state, AgentState::Running);
}

#[test]
fn axis_opposite_swaps() {
    assert_eq!(Axis::Horizontal.opposite(), Axis::Vertical);
    assert_eq!(Axis::Vertical.opposite(), Axis::Horizontal);
}

#[test]
fn layout_replace_leaf_swaps_panel() {
    let p1 = PanelId::new();
    let p2 = PanelId::new();
    let mut root = domain::LayoutNode::leaf(p1);
    let replaced = root
        .replace_leaf(&mut |id| id == p1, domain::LayoutNode::leaf(p2))
        .unwrap();
    assert!(replaced);
    assert_eq!(root.leaves(), vec![p2]);
}

mod pane_moves {
    use domain::{Axis, DropZone, LayoutNode, PanelId};

    fn two_up() -> (LayoutNode, PanelId, PanelId) {
        let (a, b) = (PanelId::new(), PanelId::new());
        let mut layout = LayoutNode::leaf(a);
        layout.split_with(a, Axis::Horizontal, 0.5, b).unwrap();
        (layout, a, b)
    }

    fn split(axis: Axis, first: PanelId, second: PanelId) -> LayoutNode {
        LayoutNode::Split {
            axis,
            ratio: 0.5,
            first: Box::new(LayoutNode::leaf(first)),
            second: Box::new(LayoutNode::leaf(second)),
        }
    }

    #[test]
    fn dropping_on_each_side_splits_the_target_and_collapses_the_source_parent() {
        for (zone, axis, source_first) in [
            (DropZone::Left, Axis::Horizontal, true),
            (DropZone::Right, Axis::Horizontal, false),
            (DropZone::Top, Axis::Vertical, true),
            (DropZone::Bottom, Axis::Vertical, false),
        ] {
            let (mut layout, a, b) = two_up();
            let c = PanelId::new();
            layout.split_with(b, Axis::Vertical, 0.5, c).unwrap();
            layout.move_leaf(c, a, zone).unwrap();
            let moved = match source_first {
                true => split(axis, c, a),
                false => split(axis, a, c),
            };
            let expected = LayoutNode::Split {
                axis: Axis::Horizontal,
                ratio: 0.5,
                first: Box::new(moved),
                second: Box::new(LayoutNode::leaf(b)),
            };
            assert_eq!(layout, expected, "{zone:?}");
            layout.validate().unwrap();
        }
    }

    #[test]
    fn dropping_on_the_center_swaps_two_panes() {
        let (mut layout, a, b) = two_up();
        layout.move_leaf(a, b, DropZone::Center).unwrap();
        assert_eq!(layout, split(Axis::Horizontal, b, a));
    }

    #[test]
    fn invalid_moves_leave_the_layout_untouched() {
        let (mut layout, a, _b) = two_up();
        let before = layout.clone();
        assert!(layout.move_leaf(a, a, DropZone::Left).is_err());
        assert!(layout.move_leaf(a, PanelId::new(), DropZone::Left).is_err());
        assert!(layout.move_leaf(PanelId::new(), a, DropZone::Center).is_err());
        assert_eq!(layout, before);
    }
}

mod pane_move_edge_cases {
    use domain::{Axis, DropZone, LayoutNode, PanelId};

    #[test]
    fn moving_a_sibling_around_its_target_keeps_a_valid_two_pane_tree() {
        for zone in [DropZone::Left, DropZone::Right, DropZone::Top, DropZone::Bottom, DropZone::Center] {
            let (a, b) = (PanelId::new(), PanelId::new());
            let mut layout = LayoutNode::leaf(a);
            layout.split_with(a, Axis::Horizontal, 0.5, b).unwrap();
            layout.move_leaf(b, a, zone).unwrap();
            layout.validate().unwrap();
            let mut leaves = layout.leaves();
            leaves.sort_by_key(|id| id.to_string());
            let mut expected = vec![a, b];
            expected.sort_by_key(|id| id.to_string());
            assert_eq!(leaves, expected, "{zone:?}");
            assert!(matches!(layout, LayoutNode::Split { .. }), "{zone:?}");
        }
    }

    #[test]
    fn moving_out_of_a_nested_split_collapses_only_that_parent() {
        let (a, b, c, d) = (PanelId::new(), PanelId::new(), PanelId::new(), PanelId::new());
        let mut layout = LayoutNode::leaf(a);
        layout.split_with(a, Axis::Horizontal, 0.5, b).unwrap();
        layout.split_with(b, Axis::Vertical, 0.5, c).unwrap();
        layout.split_with(c, Axis::Horizontal, 0.5, d).unwrap();
        layout.move_leaf(d, a, DropZone::Bottom).unwrap();
        layout.validate().unwrap();
        let expected = LayoutNode::Split {
            axis: Axis::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Split {
                axis: Axis::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::leaf(a)),
                second: Box::new(LayoutNode::leaf(d)),
            }),
            second: Box::new(LayoutNode::Split {
                axis: Axis::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::leaf(b)),
                second: Box::new(LayoutNode::leaf(c)),
            }),
        };
        assert_eq!(layout, expected);
    }
}
