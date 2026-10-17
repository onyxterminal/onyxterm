use super::Shell;
use crate::island_look::{IslandLook, IslandLooks, IslandRole};
use crate::file_tree_view::TREE_WIDTH_PX;
use crate::pane_layout::{compute_layout, grid_changes, pin_widths, Gap, PaneLayout, Rect, Spacing};
use crate::pane_motion::{duration_from_ms, transition_for, IslandFrame, PaneMotion, Scene};
use crate::tabs::SplitSide;
use crate::terminal_view::island_grid;
use crate::theme::{LayoutTokens, Theme};
use domain::{LayoutNode, PanelId, TabId};
use gpui::{
    canvas, div, point, prelude::*, px, size, AnyElement, Bounds, ContentMask, Context, Div, Pixels, Point, SharedString, Size,
    PathBuilder, Window,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

const OPAQUE: f32 = 1.0;

pub(super) struct Stage {
    motion: PaneMotion,
    scene: Option<Scene>,
    fresh_tab: Option<TabId>,
    layout: PaneLayout,
    looks: IslandLooks,
    origin: Rc<Cell<Point<Pixels>>>,
}

impl Stage {
    pub(super) fn new(tokens: &LayoutTokens) -> Self {
        let origin = point(px(tokens.window_margin_px), px(tokens.tab_strip_height_px));
        Self {
            motion: PaneMotion::new(duration_from_ms(tokens.animation_ms)),
            scene: None,
            fresh_tab: None,
            layout: PaneLayout::default(),
            looks: IslandLooks::default(),
            origin: Rc::new(Cell::new(origin)),
        }
    }

    pub(super) fn mark_fresh(&mut self, tab: TabId) {
        self.fresh_tab = Some(tab);
    }

    pub(super) fn island_bounds(&self) -> Vec<(PanelId, Bounds<Pixels>)> {
        let origin = self.origin.get();
        self.layout.islands.iter().map(|island| (island.panel, island.rect.to_bounds(origin))).collect()
    }

    pub(super) fn island_origin(&self, panel: PanelId) -> Option<Point<Pixels>> {
        let island = self.layout.islands.iter().find(|island| island.panel == panel)?;
        Some(island.rect.to_bounds(self.origin.get()).origin)
    }

    pub(super) fn gap_container(&self, path: &[SplitSide]) -> Option<Bounds<Pixels>> {
        let gap = self.layout.gaps.iter().find(|gap| gap.path == path)?;
        Some(gap.container.to_bounds(self.origin.get()))
    }

    pub(super) fn gaps(&self) -> &[Gap] {
        &self.layout.gaps
    }

    fn area(&self, viewport: Size<Pixels>, margin: f32) -> Rect {
        let top = f32::from(self.origin.get().y);
        let width = (f32::from(viewport.width) - margin * 2.0).max(0.0);
        let height = (f32::from(viewport.height) - top - margin).max(0.0);
        Rect::new(0.0, 0.0, width, height)
    }

    fn advance(&mut self, scene: Scene, layout: PaneLayout, now: Instant) {
        self.motion.prune(now);
        self.motion.apply(transition_for(self.scene.as_ref(), &scene), &layout, now);
        if scene.is_fresh_tab {
            self.fresh_tab = None;
        }
        self.scene = Some(scene);
        self.layout = layout;
    }

    fn remember_looks(&mut self, current: HashMap<PanelId, IslandLook>, now: Instant) {
        let ghosts: Vec<PanelId> = self.motion.ghosts(now).into_iter().map(|(panel, _)| panel).collect();
        self.looks.remember(current, &ghosts);
    }

    fn origin_probe(&self) -> impl IntoElement {
        let origin = self.origin.clone();
        canvas(
            move |bounds, window, _cx| {
                if origin.get() != bounds.origin {
                    origin.set(bounds.origin);
                    window.request_animation_frame();
                }
            },
            |_bounds, _state, _window, _cx| {},
        )
        .absolute()
        .size_full()
    }
}

fn placed(frame: IslandFrame) -> Div {
    let rect = frame.rect;
    div()
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.width))
        .h(px(rect.height))
        .when(frame.opacity < OPAQUE, |el| el.opacity(frame.opacity))
}

pub(super) fn pane_group(panel: PanelId) -> SharedString {
    SharedString::from(format!("pane-{panel}"))
}

pub(super) fn island_card(theme: &Theme) -> Div {
    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .rounded(px(theme.layout.island_radius_px))
        .bg(theme.canvas())
}

pub(super) fn island_content(content: impl IntoElement) -> Div {
    div().size_full().child(content)
}

pub(super) fn island_edge(theme: &Theme, look: IslandLook) -> Div {
    div()
        .absolute()
        .inset_0()
        .rounded(px(theme.layout.island_radius_px))
        .border(px(look.edge_px))
        .border_color(look.edge_color)
}

impl Shell {
    pub(super) fn render_tab_connection(&self, now: Instant, background: bool) -> impl IntoElement {
        let tabs = self.tab_bounds.clone();
        let active = self.model.tabs().active_index();
        let origin = self.stage.origin.clone();
        let theme = &self.state.theme;
        let color = theme.canvas();
        let radius = px(theme.layout.island_radius_px);
        let top_radius = px(theme.top_bar.control_height_px / 2.0);
        let edge = px(theme.island.focus_ring_px.max(theme.island.unfocused_border_px));
        let tab_border = theme.border.default;
        let panes: Vec<_> = self.stage.layout.islands.iter().filter_map(|island| {
            let frame = self.stage.motion.frame(island.panel, now)?;
            let look = self.island_look(island.panel);
            (frame.rect.y.abs() < 0.5).then_some((frame.rect, look.edge_color.opacity(look.opacity)))
        }).collect();
        canvas(|_bounds, _window, _cx| {}, move |bounds, (), window, _cx| {
            let tabs = tabs.borrow();
            let Some(tab) = tabs.get(&active).copied() else { return };
            let touching: Vec<_> = panes.iter()
                .map(|(rect, color)| (rect.to_bounds(origin.get()), *color))
                .filter(|(pane, _)| tab_overlaps_pane(tab, *pane))
                .collect();
            let pane_bounds: Vec<_> = touching.iter().map(|(pane, _)| *pane).collect();
            let polygon = tab_bubble_polygon(tab, &pane_bounds);
            let trace = |path: &mut PathBuilder| round_bubble(path, &polygon, tab.top(), top_radius, radius);
            let join_mask = Bounds::new(
                point(tab.left() - radius, tab.bottom()),
                size(tab.size.width + radius * 2.0, radius + edge * 2.0),
            );
            let mask = if background {
                Bounds::new(bounds.origin, size(bounds.size.width, tab.bottom() - bounds.top()))
            } else {
                join_mask
            };
            let mut silhouette = PathBuilder::fill();
            trace(&mut silhouette);
            if let Ok(path) = silhouette.build() {
                if background || !touching.is_empty() {
                    window.with_content_mask(Some(ContentMask { bounds: mask }), |window| window.paint_path(path, color));
                }
            }
            if !background {
                let mut outline = PathBuilder::stroke(edge);
                trace(&mut outline);
                if let Ok(path) = outline.build() {
                    let header_mask = Bounds::new(bounds.origin, size(bounds.size.width, tab.bottom() - bounds.top()));
                    window.with_content_mask(Some(ContentMask { bounds: header_mask }), |window| window.paint_path(path, tab_border));
                }
                if !touching.is_empty() {
                    let mut outline = PathBuilder::stroke(edge);
                    trace(&mut outline);
                    if let Ok(path) = outline.build() {
                        window.with_content_mask(Some(ContentMask { bounds: join_mask }), |window| window.paint_path(path, tab_border));
                    }
                }
                // Every pane touched by the tab shares the bubble. Preserve the
                // focus outline of each lobe while rounding the gap between them.
                for (pane, border) in touching {
                    let left = join_mask.left().max(pane.left() - edge);
                    let right = join_mask.right().min(pane.right() + edge);
                    let mask = Bounds::new(point(left, join_mask.top()), size(right - left, join_mask.size.height));
                    let mut outline = PathBuilder::stroke(edge);
                    trace(&mut outline);
                    if let Ok(path) = outline.build() {
                        window.with_content_mask(Some(ContentMask { bounds: mask }), |window| window.paint_path(path, border));
                    }
                }
            }
        })
        .absolute()
        .size_full()
    }

    pub(super) fn render_pane_area(
        &mut self,
        window: &mut Window,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let scene = self.current_scene()?;
        let layout = self.target_layout(&scene.tree, window.viewport_size());
        self.sync_grids(&layout, window);
        self.stage.advance(scene, layout, now);
        self.stage.remember_looks(self.current_looks(), now);
        self.prune_dropped_ghost(now);
        let area = div()
            .relative()
            .size_full()
            .child(self.stage.origin_probe())
            .children(self.render_island_ghosts(now))
            .children(self.render_islands(now, window.viewport_size(), cx))
            .children(self.render_gaps(cx));
        Some(area.into_any_element())
    }

    pub(super) fn island_look(&self, panel: PanelId) -> IslandLook {
        let tab = self.model.tabs().active();
        let role = IslandRole { is_focused: tab.is_some_and(|tab| tab.focused() == panel) };
        IslandLook::for_role(&self.state.theme, role)
    }

    fn current_looks(&self) -> HashMap<PanelId, IslandLook> {
        self.stage.layout.islands.iter().map(|island| (island.panel, self.island_look(island.panel))).collect()
    }

    pub(super) fn needs_motion_frame(&self, now: Instant) -> bool {
        self.stage.motion.is_animating(now) || self.is_drop_ghost_fading(now)
    }

    pub(super) fn mark_active_tab_fresh(&mut self) {
        if let Some(tab) = self.model.tabs().active().map(|tab| tab.id()) {
            self.stage.mark_fresh(tab);
        }
    }

    fn current_scene(&self) -> Option<Scene> {
        let tab = self.model.tabs().active()?;
        let tree = match tab.is_maximized() {
            true => LayoutNode::leaf(tab.focused()),
            false => tab.layout().clone(),
        };
        Some(Scene {
            tab: tab.id(),
            tree,
            is_fresh_tab: self.stage.fresh_tab == Some(tab.id()),
            is_live_resize: self.is_resizing_split(),
        })
    }

    fn target_layout(&self, tree: &LayoutNode, viewport: Size<Pixels>) -> PaneLayout {
        let tokens = &self.state.theme.layout;
        let spacing = Spacing { gap: tokens.island_gap_px, min_pane: tokens.min_pane_px };
        let area = self.stage.area(viewport, tokens.window_margin_px);
        let tree_width = TREE_WIDTH_PX * self.state.settings.ui_scale;
        let pinned = |panel: PanelId| self.tree_pane(panel).map(|_| tree_width);
        compute_layout(&pin_widths(tree, area, spacing, &pinned), area, spacing)
    }

    fn sync_grids(&self, layout: &PaneLayout, window: &Window) {
        let cell = self.style.cell_size(window);
        let cell_pixels = self.style.cell_pixels(window);
        let current = |panel| self.panes.get(&panel).map(|pane| pane.terminal.handle.grid_size());
        let grid_for = |rect: Rect| island_grid(size(px(rect.width), px(rect.height)), &self.style, cell);
        for (panel, grid) in grid_changes(&layout.islands, current, grid_for) {
            let Some(pane) = self.panes.get(&panel) else {
                continue;
            };
            tracing::debug!(%panel, rows = grid.rows, cols = grid.cols, "pty resize");
            pane.terminal.handle.terminal().set_cell_pixels(cell_pixels);
            if let Err(err) = pane.terminal.handle.resize(grid.rows, grid.cols) {
                tracing::warn!(?err, "terminal resize failed");
            }
        }
    }

    fn render_islands(&self, now: Instant, viewport: Size<Pixels>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let origin = self.stage.origin.get();
        let viewport_bounds = Bounds::new(point(px(0.0), px(0.0)), viewport);
        self.stage
            .layout
            .islands
            .iter()
            .filter_map(|island| {
                let frame = self.stage.motion.frame(island.panel, now)?;
                if !frame.rect.to_bounds(origin).intersects(&viewport_bounds) {
                    return None;
                }
                Some(placed(frame).child(self.render_pane(island.panel, cx)).into_any_element())
            })
            .collect()
    }

    fn render_island_ghosts(&self, now: Instant) -> Vec<AnyElement> {
        self.stage
            .motion
            .ghosts(now)
            .into_iter()
            .map(|(panel, frame)| placed(frame).child(self.render_ghost_island(panel)).into_any_element())
            .collect()
    }

    fn render_ghost_island(&self, panel: PanelId) -> impl IntoElement {
        let theme = &self.state.theme;
        let look = self.stage.looks.get(panel).unwrap_or_else(|| self.island_look(panel));
        island_card(theme).opacity(look.opacity).child(island_edge(theme, look))
    }
}

fn tab_overlaps_pane(tab: Bounds<Pixels>, pane: Bounds<Pixels>) -> bool {
    (pane.top() - tab.bottom()).abs() < px(1.0)
        && tab.right().min(pane.right()) > tab.left().max(pane.left())
}

// Trace the union of axis-aligned rectangles. Using one contour makes the
// shared tab work across arbitrary split positions and pane widths.
fn tab_bubble_polygon(tab: Bounds<Pixels>, panes: &[Bounds<Pixels>]) -> Vec<Point<Pixels>> {
    let rects: Vec<_> = std::iter::once(tab).chain(panes.iter().copied()).collect();
    let mut xs: Vec<_> = rects.iter().flat_map(|rect| [rect.left(), rect.right()]).collect();
    let mut ys: Vec<_> = rects.iter().flat_map(|rect| [rect.top(), rect.bottom()]).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.dedup();
    ys.dedup();
    let width = xs.len() - 1;
    let height = ys.len() - 1;
    let mut cells = vec![false; width * height];
    for y in 0..height {
        for x in 0..width {
            let center = point((xs[x] + xs[x + 1]) * 0.5, (ys[y] + ys[y + 1]) * 0.5);
            cells[y * width + x] = rects.iter().any(|rect| rect.contains(&center));
        }
    }
    let mut edges = HashMap::new();
    for y in 0..height {
        for x in 0..width {
            if !cells[y * width + x] { continue; }
            if y == 0 || !cells[(y - 1) * width + x] { edges.insert((x, y), (x + 1, y)); }
            if x + 1 == width || !cells[y * width + x + 1] { edges.insert((x + 1, y), (x + 1, y + 1)); }
            if y + 1 == height || !cells[(y + 1) * width + x] { edges.insert((x + 1, y + 1), (x, y + 1)); }
            if x == 0 || !cells[y * width + x - 1] { edges.insert((x, y + 1), (x, y)); }
        }
    }
    let Some(start) = edges.keys().min_by_key(|(x, y)| (*y, *x)).copied() else { return Vec::new() };
    let mut cursor = start;
    let mut points = Vec::new();
    while let Some(next) = edges.remove(&cursor) {
        points.push(point(xs[cursor.0], ys[cursor.1]));
        cursor = next;
        if cursor == start { break; }
    }
    (0..points.len()).filter_map(|i| {
        let prev = points[(i + points.len() - 1) % points.len()];
        let here = points[i];
        let next = points[(i + 1) % points.len()];
        ((prev.x != here.x || here.x != next.x) && (prev.y != here.y || here.y != next.y)).then_some(here)
    }).collect()
}

fn round_bubble(path: &mut PathBuilder, polygon: &[Point<Pixels>], tab_top: Pixels, top_radius: Pixels, radius: Pixels) {
    if polygon.is_empty() { return; }
    let corners: Vec<_> = (0..polygon.len()).map(|i| {
        let prev = polygon[(i + polygon.len() - 1) % polygon.len()];
        let here = polygon[i];
        let next = polygon[(i + 1) % polygon.len()];
        let incoming = (prev.x - here.x).abs() + (prev.y - here.y).abs();
        let outgoing = (next.x - here.x).abs() + (next.y - here.y).abs();
        let desired = if here.y == tab_top { top_radius } else { radius };
        let r = desired.min(incoming * 0.5).min(outgoing * 0.5);
        let entry = here + (prev - here) * (f32::from(r) / f32::from(incoming));
        let exit = here + (next - here) * (f32::from(r) / f32::from(outgoing));
        (entry, here, exit)
    }).collect();
    path.move_to(corners.last().unwrap().2);
    for (entry, corner, exit) in corners {
        path.line_to(entry);
        path.curve_to(exit, corner);
    }
    path.close();
}

#[cfg(test)]
mod tab_connection_tests {
    use super::*;

    fn bounds(x: f32, y: f32, width: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(100.0)))
    }

    #[test]
    fn a_tab_spanning_a_split_joins_both_panes_in_one_contour() {
        let tab = Bounds::new(point(px(210.0), px(8.0)), size(px(75.0), px(26.0)));
        let panes = [bounds(8.0, 34.0, 220.0), bounds(234.0, 34.0, 400.0)];
        assert!(panes.iter().all(|pane| tab_overlaps_pane(tab, *pane)));
        let polygon = tab_bubble_polygon(tab, &panes);
        for pane in panes {
            assert!(polygon.contains(&pane.bottom_left()));
            assert!(polygon.contains(&pane.bottom_right()));
        }
        // The gap remains open below the shared tab.
        assert!(polygon.contains(&point(px(228.0), px(34.0))));
        assert!(polygon.contains(&point(px(234.0), px(34.0))));
    }

    #[test]
    fn panes_beside_or_below_the_tab_keep_their_own_outline() {
        let tab = Bounds::new(point(px(100.0), px(8.0)), size(px(75.0), px(26.0)));
        assert!(tab_overlaps_pane(tab, bounds(8.0, 34.0, 400.0)));
        assert!(!tab_overlaps_pane(tab, bounds(200.0, 34.0, 400.0)));
        assert!(!tab_overlaps_pane(tab, bounds(8.0, 100.0, 400.0)));
    }

    #[test]
    fn a_tab_without_a_pane_still_has_a_closed_bubble() {
        let tab = Bounds::new(point(px(100.0), px(8.0)), size(px(75.0), px(26.0)));
        assert_eq!(tab_bubble_polygon(tab, &[]).len(), 4);
    }
}
