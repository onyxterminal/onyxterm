use super::Shell;
use crate::tabs::{clamp_split_ratio, SplitPath, SplitSide, SPLIT_RATIO};
use domain::{Axis, LayoutNode};
use gpui::{
    canvas, div, prelude::*, px, relative, AnyElement, Bounds, Context, CursorStyle, MouseButton, MouseMoveEvent,
    Pixels,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const DOUBLE_CLICK: usize = 2;

pub(super) type SplitBounds = Rc<RefCell<HashMap<SplitPath, Bounds<Pixels>>>>;

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

fn child_path(path: &SplitPath, side: SplitSide) -> SplitPath {
    path.iter().copied().chain(std::iter::once(side)).collect()
}

impl Shell {
    pub(super) fn render_layout(&self, node: &LayoutNode, path: SplitPath, cx: &mut Context<Self>) -> AnyElement {
        let (axis, ratio, first, second) = match node {
            LayoutNode::Leaf { panel_id } => return self.render_pane(*panel_id, cx),
            LayoutNode::Split { axis, ratio, first, second } => (axis, ratio, first, second),
        };
        let first = self.render_layout(first, child_path(&path, SplitSide::First), cx);
        let second = self.render_layout(second, child_path(&path, SplitSide::Second), cx);
        let handle = self.render_split_handle(path.clone(), *axis, cx);
        let bounds_sink = self.split_bounds.clone();
        let bounds_probe = canvas(
            move |bounds, _window, _cx| {
                bounds_sink.borrow_mut().insert(path, bounds);
            },
            |_bounds, _state, _window, _cx| {},
        )
        .absolute()
        .size_full();
        let container = div().relative().size_full().flex().min_w(px(0.0)).min_h(px(0.0)).child(bounds_probe);
        match axis {
            Axis::Horizontal => container
                .flex_row()
                .child(div().h_full().w(relative(*ratio)).flex_none().child(first))
                .child(handle)
                .child(div().h_full().flex_1().min_w(px(0.0)).child(second)),
            Axis::Vertical => container
                .flex_col()
                .child(div().w_full().h(relative(*ratio)).flex_none().child(first))
                .child(handle)
                .child(div().w_full().flex_1().min_h(px(0.0)).child(second)),
        }
        .into_any_element()
    }

    fn render_split_handle(&self, path: SplitPath, axis: Axis, cx: &mut Context<Self>) -> impl IntoElement {
        let thickness = px(self.state.theme.layout.island_gap_px);
        let handle = div().flex_none().cursor(resize_cursor(axis));
        let handle = match axis {
            Axis::Horizontal => handle.w(thickness).h_full(),
            Axis::Vertical => handle.h(thickness).w_full(),
        };
        handle.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, event: &gpui::MouseDownEvent, _window, cx| {
                cx.stop_propagation();
                shell.start_split_drag(path.clone(), axis, event.click_count, cx);
            }),
        )
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
        let Some(bounds) = self.split_bounds.borrow().get(&drag.path).copied() else {
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
