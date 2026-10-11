use super::Shell;
use crate::pane_layout::{compute_layout, grid_changes, Gap, PaneLayout, Rect, Spacing};
use crate::pane_motion::{duration_from_ms, transition_for, IslandFrame, PaneMotion, Scene};
use crate::tabs::SplitSide;
use crate::terminal_view::{island_grid, terminal_preview, ISLAND_BORDER_PX};
use crate::theme::LayoutTokens;
use domain::{LayoutNode, PanelId, TabId};
use gpui::{
    canvas, div, point, prelude::*, px, size, AnyElement, Bounds, Context, Div, Pixels, Point, Size, Window,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

const OPAQUE: f32 = 1.0;

pub(super) struct Stage {
    motion: PaneMotion,
    scene: Option<Scene>,
    fresh_tab: Option<TabId>,
    layout: PaneLayout,
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

impl Shell {
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
        self.prune_dropped_ghost(now);
        let area = div()
            .relative()
            .size_full()
            .child(self.stage.origin_probe())
            .children(self.render_island_ghosts(now))
            .children(self.render_islands(now, cx))
            .children(self.render_gaps(cx));
        Some(area.into_any_element())
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
        compute_layout(tree, self.stage.area(viewport, tokens.window_margin_px), spacing)
    }

    fn sync_grids(&self, layout: &PaneLayout, window: &Window) {
        let cell = self.style.cell_size(window);
        let current = |panel| self.panes.get(&panel).map(|pane| pane.terminal.handle.grid_size());
        let grid_for = |rect: Rect| island_grid(size(px(rect.width), px(rect.height)), &self.style, cell);
        for (panel, grid) in grid_changes(&layout.islands, current, grid_for) {
            let Some(pane) = self.panes.get(&panel) else {
                continue;
            };
            tracing::debug!(%panel, rows = grid.rows, cols = grid.cols, "pty resize");
            if let Err(err) = pane.terminal.handle.resize(grid.rows, grid.cols) {
                tracing::warn!(?err, "terminal resize failed");
            }
        }
    }

    fn render_islands(&self, now: Instant, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let panels: Vec<PanelId> = self.stage.layout.islands.iter().map(|island| island.panel).collect();
        panels
            .into_iter()
            .filter_map(|panel| {
                let frame = self.stage.motion.frame(panel, now)?;
                Some(placed(frame).child(self.render_pane(panel, cx)).into_any_element())
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
        let preview =
            self.panes.get(&panel).map(|pane| terminal_preview(pane.terminal.handle.clone(), self.style.clone()));
        div()
            .size_full()
            .overflow_hidden()
            .rounded(px(theme.layout.island_radius_px))
            .border(px(ISLAND_BORDER_PX))
            .border_color(theme.border_default())
            .bg(theme.canvas())
            .children(preview)
    }
}
