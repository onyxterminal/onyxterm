use super::Shell;
use crate::motion::fade_in;
use crate::tabs::drop_zone_at;
use domain::{DropZone, PanelId};
use gpui::{div, prelude::*, px, relative, AnyElement, Context, DragMoveEvent, Hsla, SharedString, Window};

const GHOST_WIDTH_PX: f32 = 240.0;
const GHOST_HEIGHT_PX: f32 = 150.0;
const GHOST_OPACITY: f32 = 0.6;
const DROP_HIGHLIGHT_ALPHA: f32 = 0.18;
const HALF: f32 = 0.5;

#[derive(Clone)]
pub(super) struct DraggedPane {
    pub(super) panel: PanelId,
    title: SharedString,
}

impl DraggedPane {
    pub(super) fn new(panel: PanelId, title: SharedString) -> Self {
        Self { panel, title }
    }
}

#[derive(Clone, Copy)]
pub(super) struct GhostStyle {
    colors: (Hsla, Hsla, Hsla),
    radius: f32,
}

pub(super) struct PaneGhost {
    title: SharedString,
    style: GhostStyle,
}

impl PaneGhost {
    pub(super) fn new(dragged: &DraggedPane, style: GhostStyle) -> Self {
        Self { title: dragged.title.clone(), style }
    }
}

impl Render for PaneGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (bg, border, fg) = self.style.colors;
        div()
            .w(px(GHOST_WIDTH_PX))
            .h(px(GHOST_HEIGHT_PX))
            .opacity(GHOST_OPACITY)
            .bg(bg)
            .border_1()
            .border_color(border)
            .rounded(px(self.style.radius))
            .shadow_md()
            .px_2()
            .py_1()
            .text_color(fg)
            .child(self.title.clone())
    }
}

impl Shell {
    pub(super) fn ghost_style(&self) -> GhostStyle {
        let theme = &self.state.theme;
        GhostStyle {
            colors: (theme.canvas(), theme.accent.primary, theme.text_default()),
            radius: theme.layout.island_radius_px,
        }
    }

    pub(super) fn track_pane_drag(
        &mut self,
        panel: PanelId,
        event: &DragMoveEvent<DraggedPane>,
        cx: &mut Context<Self>,
    ) {
        let bounds = event.bounds;
        let target = match bounds.contains(&event.event.position) {
            true => {
                let local = event.event.position - bounds.origin;
                let zone = drop_zone_at(local.x / bounds.size.width, local.y / bounds.size.height);
                (event.drag(cx).panel != panel).then_some((panel, zone))
            }
            false => self.drop_target.filter(|(current, _)| *current != panel),
        };
        if target != self.drop_target {
            self.drop_target = target;
            cx.notify();
        }
    }

    pub(super) fn drop_pane(&mut self, source: PanelId, cx: &mut Context<Self>) {
        let Some((target, zone)) = self.drop_target.take() else {
            return;
        };
        let moved = self.model.tabs_mut().active_mut().map(|tab| tab.move_panel(source, target, zone));
        if let Some(Err(err)) = moved {
            tracing::debug!(?err, "pane move rejected");
        }
        self.persist();
        cx.notify();
    }

    pub(super) fn move_pane_to_tab(&mut self, source: PanelId, index: usize, cx: &mut Context<Self>) {
        self.drop_target = None;
        if let Err(err) = self.model.tabs_mut().move_panel_to_tab(source, index) {
            tracing::debug!(?err, "pane move into tab rejected");
        }
        self.persist();
        cx.notify();
    }

    pub(super) fn cancel_pane_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.drop_target = None;
        let was_dragging = cx.stop_active_drag(window);
        cx.notify();
        was_dragging
    }

    pub(super) fn clear_drop_target(&mut self, cx: &mut Context<Self>) {
        if self.drop_target.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn render_drop_highlight(&self, panel: PanelId) -> Option<AnyElement> {
        let (_, zone) = self.drop_target.filter(|(target, _)| *target == panel)?;
        let theme = &self.state.theme;
        let highlight = div().absolute().bg(theme.accent.primary.opacity(DROP_HIGHLIGHT_ALPHA));
        let highlight = match zone {
            DropZone::Left => highlight.left_0().top_0().h_full().w(relative(HALF)),
            DropZone::Right => highlight.right_0().top_0().h_full().w(relative(HALF)),
            DropZone::Top => highlight.left_0().top_0().w_full().h(relative(HALF)),
            DropZone::Bottom => highlight.left_0().bottom_0().w_full().h(relative(HALF)),
            DropZone::Center => highlight.inset_0(),
        };
        let id = SharedString::from(format!("drop-{panel}-{zone:?}"));
        Some(fade_in(highlight, id, theme.layout.animation_ms))
    }
}
