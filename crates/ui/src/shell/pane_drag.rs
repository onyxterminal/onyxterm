use super::Shell;
use crate::motion::fade_in;
use crate::pane_drop::{has_left_dead_zone, pane_drop_target, tab_drop_target, DropTarget};
use crate::pane_motion::{duration_from_ms, ease_out_cubic, progress};
use domain::{DropZone, PanelId};
use gpui::{
    canvas, div, prelude::*, px, relative, AnyElement, Bounds, Context, DispatchPhase, MouseButton, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, SharedString, Window,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

const GHOST_WIDTH_PX: f32 = 240.0;
const GHOST_HEIGHT_PX: f32 = 150.0;
const GHOST_OPACITY: f32 = 0.6;
const GHOST_GRIP_INSET_PX: f32 = 16.0;
const DROP_HIGHLIGHT_ALPHA: f32 = 0.18;
const SWAP_OUTLINE_PX: f32 = 2.0;
const SWAP_LABEL: &str = "Swap";
const HALF: f32 = 0.5;
const DRAG_NOTIFY_FRAME_MS: u128 = 8;

fn should_notify_drag(last: &Cell<Option<Instant>>, now: Instant) -> bool {
    let ready = match last.get() {
        Some(previous) => now.duration_since(previous).as_millis() >= DRAG_NOTIFY_FRAME_MS,
        None => true,
    };
    if ready {
        last.set(Some(now));
    }
    ready
}

pub(super) type BoundsMap<K> = Rc<RefCell<HashMap<K, Bounds<Pixels>>>>;

pub(super) struct PaneDrag {
    panel: PanelId,
    press: Point<Pixels>,
    grab: Point<Pixels>,
    cursor: Point<Pixels>,
    is_active: bool,
    last_notify_at: Cell<Option<Instant>>,
}

pub(super) struct DroppedGhost {
    drag: PaneDrag,
    started: Instant,
}

pub(super) fn fraction(offset: Pixels, extent: Pixels) -> f32 {
    if extent <= px(0.0) {
        return 0.0;
    }
    f32::from(offset) / f32::from(extent)
}

pub(super) type DragMove = fn(&mut Shell, &MouseMoveEvent, &mut Context<Shell>);
pub(super) type DragRelease = fn(&mut Shell, &mut Context<Shell>);

pub(super) fn window_drag_listener(cx: &mut Context<Shell>, on_move: DragMove, on_release: DragRelease)
    -> impl IntoElement {
    let mover = cx.entity().downgrade();
    let releaser = mover.clone();
    canvas(|_bounds, _window, _cx| {}, move |_bounds, (), window, _cx| {
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
            if phase == DispatchPhase::Bubble {
                mover.update(cx, |shell, cx| on_move(shell, event, cx)).ok();
            }
        });
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                releaser.update(cx, on_release).ok();
            }
        });
    })
    .absolute()
    .size_full()
}

pub(super) fn bounds_probe<K>(sink: &BoundsMap<K>, key: K) -> impl IntoElement
where
    K: std::hash::Hash + Eq + 'static,
{
    let sink = sink.clone();
    canvas(
        move |bounds, _window, _cx| {
            sink.borrow_mut().insert(key, bounds);
        },
        |_bounds, _state, _window, _cx| {},
    )
    .absolute()
    .size_full()
}

impl Shell {
    pub(super) fn press_pane_title(&mut self, panel: PanelId, press: Point<Pixels>) {
        let origin = self.stage.island_origin(panel).unwrap_or(press);
        self.pane_drag = Some(PaneDrag {
            panel,
            press,
            grab: press - origin,
            cursor: press,
            is_active: false,
            last_notify_at: Cell::new(None),
        });
    }

    pub(super) fn is_dragging_pane(&self) -> bool {
        self.pane_drag.as_ref().is_some_and(|drag| drag.is_active)
    }

    pub(super) fn drag_pane(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(drag) = self.pane_drag.as_mut() else {
            return;
        };
        if event.pressed_button != Some(gpui::MouseButton::Left) {
            self.cancel_pane_drag(cx);
            return;
        }
        drag.cursor = event.position;
        drag.is_active = drag.is_active || has_left_dead_zone(drag.press, event.position);
        if !drag.is_active {
            return;
        }
        let (source, cursor) = (drag.panel, drag.cursor);
        let should_notify = should_notify_drag(&drag.last_notify_at, Instant::now());
        self.drop_target = self.hit_test(source, cursor);
        if should_notify {
            cx.notify();
        }
    }

    fn hit_test(&self, source: PanelId, cursor: Point<Pixels>) -> Option<DropTarget> {
        let source_tab = self.model.tabs().position_of(source)?;
        let tabs: Vec<(usize, Bounds<Pixels>)> = self.tab_bounds.borrow().iter().map(|(i, b)| (*i, *b)).collect();
        if let Some(index) = tab_drop_target(cursor, &tabs, source_tab) {
            return Some(DropTarget::Tab(index));
        }
        pane_drop_target(cursor, &self.stage.island_bounds(), source).map(|(panel, zone)| DropTarget::Pane(panel, zone))
    }

    pub(super) fn release_pane_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.pane_drag.take() else {
            return;
        };
        let target = self.drop_target.take();
        if !drag.is_active {
            return;
        }
        cx.notify();
        let panel = drag.panel;
        self.dropped_ghost = Some(DroppedGhost { drag, started: Instant::now() });
        let Some(target) = target else {
            return;
        };
        if self.apply_drop(panel, target) {
            window.focus(&self.focus_handle);
            self.persist();
        }
    }

    fn apply_drop(&mut self, source: PanelId, target: DropTarget) -> bool {
        let result = self.model.tabs_mut().drop_panel(source, target);
        result.inspect_err(|err| tracing::debug!(?err, "pane drop rejected")).is_ok()
    }

    pub(super) fn cancel_pane_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let was_active = self.is_dragging_pane();
        self.pane_drag = None;
        self.drop_target = None;
        cx.notify();
        was_active
    }

    pub(super) fn is_tab_drop_target(&self, index: usize) -> bool {
        self.drop_target == Some(DropTarget::Tab(index))
    }

    pub(super) fn render_pane_ghost(&self, now: Instant) -> Option<AnyElement> {
        if let Some(drag) = self.pane_drag.as_ref().filter(|drag| drag.is_active) {
            return Some(self.ghost_card(drag, GHOST_OPACITY));
        }
        let dropped = self.dropped_ghost.as_ref()?;
        let fade = ease_out_cubic(self.drop_fade_progress(dropped, now));
        Some(self.ghost_card(&dropped.drag, GHOST_OPACITY * (1.0 - fade)))
    }

    pub(super) fn is_drop_ghost_fading(&self, now: Instant) -> bool {
        self.dropped_ghost.as_ref().is_some_and(|dropped| self.drop_fade_progress(dropped, now) < 1.0)
    }

    pub(super) fn prune_dropped_ghost(&mut self, now: Instant) {
        if !self.is_drop_ghost_fading(now) {
            self.dropped_ghost = None;
        }
    }

    fn drop_fade_progress(&self, dropped: &DroppedGhost, now: Instant) -> f32 {
        progress(dropped.started, now, duration_from_ms(self.state.theme.layout.animation_ms))
    }

    fn ghost_card(&self, drag: &PaneDrag, opacity: f32) -> AnyElement {
        let theme = &self.state.theme;
        let grip_x = f32::from(drag.grab.x).clamp(GHOST_GRIP_INSET_PX, GHOST_WIDTH_PX - GHOST_GRIP_INSET_PX);
        let grip_y = theme.layout.pane_header_height_px * HALF;
        let title = SharedString::from(self.pane_display_title(drag.panel));
        let ghost = div()
            .absolute()
            .left(drag.cursor.x - px(grip_x))
            .top(drag.cursor.y - px(grip_y))
            .w(px(GHOST_WIDTH_PX))
            .h(px(GHOST_HEIGHT_PX))
            .opacity(opacity)
            .bg(theme.canvas())
            .border_1()
            .border_color(theme.accent.primary)
            .rounded(px(theme.layout.island_radius_px))
            .shadow_md()
            .child(
                div()
                    .h(px(theme.layout.pane_header_height_px))
                    .px_2()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(theme.border.muted)
                    .text_color(theme.text_default())
                    .child(title),
            );
        ghost.into_any_element()
    }

    pub(super) fn render_drop_highlight(&self, panel: PanelId) -> Option<AnyElement> {
        let Some(DropTarget::Pane(target, zone)) = self.drop_target else {
            return None;
        };
        if target != panel {
            return None;
        }
        let theme = &self.state.theme;
        let fill = theme.accent.primary.opacity(DROP_HIGHLIGHT_ALPHA);
        let radius = px(theme.layout.island_radius_px);
        let highlight = div().absolute();
        let highlight = match zone {
            DropZone::Left => highlight.bg(fill).rounded_l(radius).left_0().top_0().h_full().w(relative(HALF)),
            DropZone::Right => highlight.bg(fill).rounded_r(radius).right_0().top_0().h_full().w(relative(HALF)),
            DropZone::Top => highlight.bg(fill).rounded_t(radius).left_0().top_0().w_full().h(relative(HALF)),
            DropZone::Bottom => highlight.bg(fill).rounded_b(radius).left_0().bottom_0().w_full().h(relative(HALF)),
            DropZone::Center => highlight
                .inset_0()
                .border(px(SWAP_OUTLINE_PX))
                .border_color(theme.accent.primary)
                .rounded(radius)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .px_3()
                        .py_1()
                        .rounded(px(theme.layout.border_radius_px))
                        .bg(theme.accent.primary)
                        .text_color(theme.text.inverse)
                        .child(SWAP_LABEL),
                ),
        };
        let id = SharedString::from(format!("drop-{panel}-{zone:?}"));
        Some(fade_in(highlight, id, theme.layout.animation_ms))
    }
}
