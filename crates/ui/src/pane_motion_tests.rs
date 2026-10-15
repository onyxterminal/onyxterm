use crate::pane_layout::{compute_layout, first_extent, grid_changes, pin_widths, PaneLayout, Rect, Spacing};
use crate::pane_motion::{
    duration_from_ms, ease_out_cubic, progress, transition_for, tween, PaneMotion, Scene, Transition,
};
use crate::tabs::SplitSide;
use domain::{Axis, LayoutNode, PanelId, TabId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use terminal_core::GridSize;

const AREA: Rect = Rect { x: 0.0, y: 0.0, width: 1000.0, height: 600.0 };
const SPACING: Spacing = Spacing { gap: 6.0, min_pane: 120.0 };
const DURATION: Duration = Duration::from_millis(150);
const CELL_PX: f32 = 10.0;

fn split(axis: Axis, ratio: f32, first: LayoutNode, second: LayoutNode) -> LayoutNode {
    LayoutNode::Split { axis, ratio, first: Box::new(first), second: Box::new(second) }
}

fn rect_of(layout: &PaneLayout, panel: PanelId) -> Rect {
    layout.islands.iter().find(|island| island.panel == panel).expect("island").rect
}

fn approx(a: Rect, b: Rect) -> bool {
    let close = |x: f32, y: f32| (x - y).abs() < 1e-3;
    close(a.x, b.x) && close(a.y, b.y) && close(a.width, b.width) && close(a.height, b.height)
}

fn at(start: Instant, millis: u64) -> Instant {
    start + Duration::from_millis(millis)
}

#[test]
fn single_pane_fills_the_area_without_gaps() {
    let panel = PanelId::new();
    let layout = compute_layout(&LayoutNode::leaf(panel), AREA, SPACING);
    assert_eq!(layout.islands.len(), 1);
    assert_eq!(layout.islands[0].rect, AREA);
    assert_eq!(layout.islands[0].collapsed, AREA);
    assert!(layout.gaps.is_empty());
}

#[test]
fn pinned_leaves_keep_their_width_on_either_side_of_a_split() {
    let (tree_panel, editor, terminal) = (PanelId::new(), PanelId::new(), PanelId::new());
    let bundle = split(Axis::Horizontal, 0.5, LayoutNode::leaf(tree_panel), LayoutNode::leaf(editor));
    let outer = split(Axis::Horizontal, 0.4, LayoutNode::leaf(terminal), bundle);
    let pinned = |panel: PanelId| (panel == tree_panel).then_some(236.0);
    let layout = compute_layout(&pin_widths(&outer, AREA, SPACING, &pinned), AREA, SPACING);
    assert!((rect_of(&layout, tree_panel).width - 236.0).abs() < 1e-3);
    assert_eq!(rect_of(&layout, terminal).width, 400.0);
    let reversed = split(Axis::Horizontal, 0.5, LayoutNode::leaf(editor), LayoutNode::leaf(tree_panel));
    let layout = compute_layout(&pin_widths(&reversed, AREA, SPACING, &pinned), AREA, SPACING);
    assert!((rect_of(&layout, tree_panel).width - 236.0).abs() < 1e-3);
}

#[test]
fn pinning_leaves_vertical_splits_alone() {
    let (tree_panel, editor) = (PanelId::new(), PanelId::new());
    let stacked = split(Axis::Vertical, 0.5, LayoutNode::leaf(tree_panel), LayoutNode::leaf(editor));
    let pinned = |panel: PanelId| (panel == tree_panel).then_some(236.0);
    assert_eq!(pin_widths(&stacked, AREA, SPACING, &pinned), stacked);
}

#[test]
fn horizontal_split_places_gap_between_islands() {
    let (left, right) = (PanelId::new(), PanelId::new());
    let tree = split(Axis::Horizontal, 0.5, LayoutNode::leaf(left), LayoutNode::leaf(right));
    let layout = compute_layout(&tree, AREA, SPACING);
    assert_eq!(rect_of(&layout, left), Rect::new(0.0, 0.0, 500.0, 600.0));
    assert_eq!(rect_of(&layout, right), Rect::new(506.0, 0.0, 494.0, 600.0));
    assert_eq!(layout.gaps.len(), 1);
    assert_eq!(layout.gaps[0].rect, Rect::new(500.0, 0.0, 6.0, 600.0));
    assert_eq!(layout.gaps[0].container, AREA);
    assert_eq!(layout.islands[0].collapsed, Rect::new(0.0, 0.0, 0.0, 600.0));
    assert_eq!(layout.islands[1].collapsed, Rect::new(1000.0, 0.0, 0.0, 600.0));
}

#[test]
fn nested_splits_follow_ratios_and_paths() {
    let (left, top, bottom) = (PanelId::new(), PanelId::new(), PanelId::new());
    let column = split(Axis::Vertical, 0.25, LayoutNode::leaf(top), LayoutNode::leaf(bottom));
    let tree = split(Axis::Horizontal, 0.3, LayoutNode::leaf(left), column);
    let layout = compute_layout(&tree, AREA, SPACING);
    assert_eq!(rect_of(&layout, left), Rect::new(0.0, 0.0, 300.0, 600.0));
    assert_eq!(rect_of(&layout, top), Rect::new(306.0, 0.0, 694.0, 150.0));
    assert_eq!(rect_of(&layout, bottom), Rect::new(306.0, 156.0, 694.0, 444.0));
    let paths: Vec<Vec<SplitSide>> = layout.gaps.iter().map(|gap| gap.path.clone()).collect();
    assert_eq!(paths, vec![vec![], vec![SplitSide::Second]]);
    assert_eq!(layout.gaps[1].container, Rect::new(306.0, 0.0, 694.0, 600.0));
}

#[test]
fn ratios_are_clamped_to_the_minimum_pane_size() {
    assert_eq!(first_extent(1000.0, 0.05, SPACING), 120.0);
    assert_eq!(first_extent(1000.0, 0.99, SPACING), 874.0);
    assert_eq!(first_extent(200.0, 0.5, SPACING), 100.0);
    assert_eq!(first_extent(4.0, 0.5, SPACING), 0.0);
    let (left, right) = (PanelId::new(), PanelId::new());
    let tree = split(Axis::Horizontal, 0.01, LayoutNode::leaf(left), LayoutNode::leaf(right));
    let layout = compute_layout(&tree, AREA, SPACING);
    assert_eq!(rect_of(&layout, left).width, 120.0);
    assert_eq!(rect_of(&layout, right).width, 874.0);
}

#[test]
fn easing_and_tween_are_bounded_and_monotonic() {
    assert_eq!(ease_out_cubic(0.0), 0.0);
    assert_eq!(ease_out_cubic(1.0), 1.0);
    assert!((ease_out_cubic(0.5) - 0.875).abs() < 1e-6);
    assert_eq!(ease_out_cubic(2.0), 1.0);
    let samples: Vec<f32> = (0..=10).map(|step| ease_out_cubic(step as f32 / 10.0)).collect();
    assert!(samples.windows(2).all(|pair| pair[1] >= pair[0]));
    let from = Rect::new(0.0, 0.0, 100.0, 50.0);
    let to = Rect::new(100.0, 20.0, 300.0, 50.0);
    assert_eq!(tween(from, to, 0.0), from);
    assert_eq!(tween(from, to, 1.0), to);
    assert_eq!(tween(from, to, 0.5), Rect::new(50.0, 10.0, 200.0, 50.0));
}

#[test]
fn zero_duration_progress_is_complete() {
    let now = Instant::now();
    assert_eq!(progress(now, now, Duration::ZERO), 1.0);
    assert_eq!(progress(now, at(now, 75), DURATION), 0.5);
    assert_eq!(duration_from_ms(0.0), Duration::ZERO);
}

fn scene(tab: TabId, tree: &LayoutNode) -> Scene {
    Scene { tab, tree: tree.clone(), is_fresh_tab: false, is_live_resize: false }
}

#[test]
fn transitions_depend_on_what_changed() {
    let (tab, panel, other) = (TabId::new(), PanelId::new(), PanelId::new());
    let single = LayoutNode::leaf(panel);
    let pair = split(Axis::Horizontal, 0.5, LayoutNode::leaf(panel), LayoutNode::leaf(other));
    let fresh = Scene { is_fresh_tab: true, ..scene(tab, &single) };
    assert_eq!(transition_for(None, &scene(tab, &single)), Transition::Cut);
    assert_eq!(transition_for(None, &fresh), Transition::FadeIn);
    assert_eq!(transition_for(Some(&scene(TabId::new(), &single)), &scene(tab, &single)), Transition::Cut);
    assert_eq!(transition_for(Some(&scene(tab, &single)), &scene(tab, &single)), Transition::Follow);
    assert_eq!(transition_for(Some(&scene(tab, &single)), &scene(tab, &pair)), Transition::Glide);
    let dragging = Scene { is_live_resize: true, ..scene(tab, &pair) };
    assert_eq!(transition_for(Some(&scene(tab, &single)), &dragging), Transition::Follow);
}

struct SplitFixture {
    source: PanelId,
    added: PanelId,
    before: PaneLayout,
    after: PaneLayout,
}

fn split_fixture() -> SplitFixture {
    let (source, added) = (PanelId::new(), PanelId::new());
    let before = compute_layout(&LayoutNode::leaf(source), AREA, SPACING);
    let tree = split(Axis::Horizontal, 0.5, LayoutNode::leaf(source), LayoutNode::leaf(added));
    let after = compute_layout(&tree, AREA, SPACING);
    SplitFixture { source, added, before, after }
}

#[test]
fn new_split_grows_from_the_source_edge_and_fades_in() {
    let fixture = split_fixture();
    let start = Instant::now();
    let mut motion = PaneMotion::new(DURATION);
    motion.apply(Transition::Cut, &fixture.before, start);
    motion.apply(Transition::Glide, &fixture.after, start);
    let added = motion.frame(fixture.added, start).expect("added");
    assert_eq!(added.rect, Rect::new(1000.0, 0.0, 0.0, 600.0));
    assert_eq!(added.opacity, 0.0);
    assert_eq!(motion.frame(fixture.source, start).expect("source").rect, AREA);
    let midway = motion.frame(fixture.added, at(start, 75)).expect("midway");
    assert!(midway.rect.width > 0.0 && midway.rect.width < 494.0);
    assert!(midway.opacity > 0.0 && midway.opacity < 1.0);
    assert!(motion.is_animating(at(start, 75)));
    let done = at(start, 150);
    assert_eq!(motion.frame(fixture.added, done).expect("done").rect, rect_of(&fixture.after, fixture.added));
    assert_eq!(motion.frame(fixture.source, done).expect("done").rect, rect_of(&fixture.after, fixture.source));
    assert!(!motion.is_animating(done));
}

#[test]
fn closed_island_lingers_as_a_shrinking_ghost() {
    let fixture = split_fixture();
    let start = Instant::now();
    let mut motion = PaneMotion::new(DURATION);
    motion.apply(Transition::Cut, &fixture.after, start);
    motion.apply(Transition::Glide, &fixture.before, start);
    assert!(motion.frame(fixture.added, start).is_none());
    let ghosts = motion.ghosts(at(start, 75));
    assert_eq!(ghosts.len(), 1);
    assert_eq!(ghosts[0].0, fixture.added);
    assert!(ghosts[0].1.opacity < 1.0 && ghosts[0].1.rect.width < 494.0);
    let done = at(start, 150);
    assert!(motion.ghosts(done).is_empty());
    assert!(!motion.is_animating(done));
    motion.prune(done);
    assert!(motion.ghosts(start).is_empty());
}

#[test]
fn retarget_mid_tween_starts_from_the_displayed_rect() {
    let fixture = split_fixture();
    let start = Instant::now();
    let mut motion = PaneMotion::new(DURATION);
    motion.apply(Transition::Cut, &fixture.before, start);
    motion.apply(Transition::Glide, &fixture.after, start);
    let interrupt = at(start, 50);
    let shown = motion.frame(fixture.source, interrupt).expect("shown");
    motion.apply(Transition::Glide, &fixture.before, interrupt);
    let resumed = motion.frame(fixture.source, interrupt).expect("resumed");
    assert!(approx(shown.rect, resumed.rect));
    let ghost = motion.ghosts(interrupt);
    assert_eq!(ghost.len(), 1);
    assert!(approx(ghost[0].1.rect, added_rect_at(&fixture, interrupt)));
    assert_eq!(motion.frame(fixture.source, at(interrupt, 150)).expect("end").rect, AREA);
}

fn added_rect_at(fixture: &SplitFixture, interrupt: Instant) -> Rect {
    let start = interrupt - Duration::from_millis(50);
    let mut motion = PaneMotion::new(DURATION);
    motion.apply(Transition::Cut, &fixture.before, start);
    motion.apply(Transition::Glide, &fixture.after, start);
    motion.frame(fixture.added, interrupt).expect("added").rect
}

#[test]
fn follow_keeps_running_tweens_and_snaps_new_targets() {
    let fixture = split_fixture();
    let start = Instant::now();
    let mut motion = PaneMotion::new(DURATION);
    motion.apply(Transition::Cut, &fixture.before, start);
    motion.apply(Transition::Glide, &fixture.after, start);
    let midway = at(start, 75);
    let before = motion.frame(fixture.added, midway);
    motion.apply(Transition::Follow, &fixture.after, midway);
    assert_eq!(motion.frame(fixture.added, midway), before);
    let wider = compute_layout(
        &split(Axis::Horizontal, 0.5, LayoutNode::leaf(fixture.source), LayoutNode::leaf(fixture.added)),
        Rect::new(0.0, 0.0, 1200.0, 600.0),
        SPACING,
    );
    motion.apply(Transition::Follow, &wider, midway);
    assert_eq!(motion.frame(fixture.added, midway).expect("snapped").rect, rect_of(&wider, fixture.added));
}

#[test]
fn zero_duration_is_instant() {
    let fixture = split_fixture();
    let start = Instant::now();
    let mut motion = PaneMotion::new(duration_from_ms(0.0));
    motion.apply(Transition::Cut, &fixture.before, start);
    motion.apply(Transition::Glide, &fixture.after, start);
    assert_eq!(motion.frame(fixture.added, start).expect("added").rect, rect_of(&fixture.after, fixture.added));
    assert!(motion.ghosts(start).is_empty());
    assert!(!motion.is_animating(start));
}

#[test]
fn fresh_tab_fades_in_place() {
    let fixture = split_fixture();
    let start = Instant::now();
    let mut motion = PaneMotion::new(DURATION);
    motion.apply(Transition::FadeIn, &fixture.before, start);
    let first = motion.frame(fixture.source, start).expect("first");
    assert_eq!((first.rect, first.opacity), (AREA, 0.0));
    assert_eq!(motion.frame(fixture.source, at(start, 150)).expect("end").opacity, 1.0);
}

fn grid_for(rect: Rect) -> GridSize {
    GridSize::new((rect.width / CELL_PX) as u16, (rect.height / CELL_PX) as u16)
}

#[test]
fn terminal_resizes_once_per_layout_change_not_per_frame() {
    let fixture = split_fixture();
    let grids: RefCell<HashMap<PanelId, GridSize>> =
        RefCell::new([fixture.source, fixture.added].into_iter().map(|panel| (panel, grid_for(AREA))).collect());
    let mut resizes = 0;
    let start = Instant::now();
    let mut motion = PaneMotion::new(DURATION);
    motion.apply(Transition::Cut, &fixture.before, start);
    motion.apply(Transition::Glide, &fixture.after, start);
    for frame in 0..=15 {
        let now = at(start, frame * 10);
        motion.apply(Transition::Follow, &fixture.after, now);
        let changes = grid_changes(&fixture.after.islands, |panel| grids.borrow().get(&panel).copied(), grid_for);
        resizes += changes.len();
        grids.borrow_mut().extend(changes);
    }
    assert_eq!(resizes, 2);
    assert_eq!(grids.borrow()[&fixture.added], grid_for(rect_of(&fixture.after, fixture.added)));
}
