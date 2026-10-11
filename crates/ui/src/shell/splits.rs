use super::Shell;
use crate::pane_layout::Gap;
use crate::tabs::{clamp_split_ratio, SplitPath, SplitSide, SPLIT_RATIO};
use domain::Axis;
use gpui::{div, prelude::*, px, AnyElement, Context, CursorStyle, MouseButton, MouseMoveEvent};

const DOUBLE_CLICK: usize = 2;

pub(super) struct SplitDrag {
    path: SplitPath,
    axis: Axis,
}

fn resize_cursor(axis: Axis) -> CursorStyle {
    match axis {
        Axis::Horizontal => CursorStyle::ResizeLeftRight,
        Axis::Vertical => CursorStyle::ResizeUpDown,
    }
}

impl Shell {
    pub(super) fn render_gaps(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.stage.gaps().iter().map(|gap| self.render_split_handle(gap, cx)).collect()
    }

    pub(super) fn is_resizing_split(&self) -> bool {
        self.split_drag.is_some()
    }

    fn render_split_handle(&self, gap: &Gap, cx: &mut Context<Self>) -> AnyElement {
        let (path, axis, rect) = (gap.path.clone(), gap.axis, gap.rect);
        div()
            .absolute()
            .left(px(rect.x))
            .top(px(rect.y))
            .w(px(rect.width))
            .h(px(rect.height))
            .cursor(resize_cursor(axis))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, event: &gpui::MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    shell.start_split_drag(path.clone(), axis, event.click_count, cx);
                }),
            )
            .into_any_element()
    }

    fn start_split_drag(&mut self, path: SplitPath, axis: Axis, click_count: usize, cx: &mut Context<Self>) {
        if click_count == DOUBLE_CLICK {
            self.apply_split_ratio(&path, SPLIT_RATIO);
            self.persist();
            cx.notify();
            return;
        }
        self.split_drag = Some(SplitDrag { path, axis });
    }

    pub(super) fn drag_split(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(drag) = self.split_drag.as_ref() else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_split_drag(cx);
            return;
        }
        let Some(bounds) = self.stage.gap_container(&drag.path) else {
            return;
        };
        let local = event.position - bounds.origin;
        let (offset, extent) = match drag.axis {
            Axis::Horizontal => (local.x, bounds.size.width),
            Axis::Vertical => (local.y, bounds.size.height),
        };
        let ratio = clamp_split_ratio(f32::from(offset), f32::from(extent), self.state.theme.layout.min_pane_px);
        let path = drag.path.clone();
        self.apply_split_ratio(&path, ratio);
        cx.notify();
    }

    pub(super) fn end_split_drag(&mut self, cx: &mut Context<Self>) {
        if self.split_drag.take().is_some() {
            self.persist();
            cx.notify();
        }
    }

    pub(super) fn split_drag_cursor(&self) -> Option<CursorStyle> {
        self.split_drag.as_ref().map(|drag| resize_cursor(drag.axis))
    }

    fn apply_split_ratio(&mut self, path: &[SplitSide], ratio: f32) {
        if let Some(tab) = self.model.tabs_mut().active_mut() {
            tab.set_split_ratio(path, ratio);
        }
    }
}
