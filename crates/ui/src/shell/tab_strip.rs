use super::pane_drag::bounds_probe;
use super::{FieldTarget, Shell, ENTER_KEY, ESCAPE_KEY};
use crate::command::CommandId;
use crate::pane_drop::{has_left_dead_zone, tab_drop_target};
use crate::theme::icons::{logo_mark, Icon};
use crate::text_field::TextField;
use crate::theme::menu_chrome::{field_text, kbd_chip};
use crate::theme::{box_shadows, Theme};
use crate::workspace_menu::workspace_button_label;
use gpui::{
    div, prelude::*, px, AnyElement, Bounds, BoxShadow, Context, Div, FontWeight, Keystroke,
    MouseButton, MouseMoveEvent, MouseUpEvent, Pixels, Point, SharedString, Stateful, Window,
};
use std::cell::Cell;
use std::time::Instant;

const DOUBLE_CLICK: usize = 2;
const WORKSPACE_BUTTON_PADDING_X_PX: f32 = 8.0;
const WORKSPACE_BUTTON_GAP_PX: f32 = 7.0;
const CHEVRON_PX: f32 = 10.0;
const CHEVRON_OPACITY: f32 = 0.55;
const SEPARATOR_MARGIN_X_PX: f32 = 4.0;
const NEW_TAB_ICON_PX: f32 = 13.0;
const PALETTE_BUTTON_WIDTH_PX: f32 = 230.0;
const PALETTE_BUTTON_MIN_WIDTH_PX: f32 = 120.0;
const PALETTE_BUTTON_PADDING_LEFT_PX: f32 = 10.0;
const PALETTE_BUTTON_PADDING_RIGHT_PX: f32 = 6.0;
const PALETTE_BUTTON_GAP_PX: f32 = 8.0;
const PALETTE_BUTTON_RADIUS_PX: f32 = 7.0;
const PALETTE_ICON_PX: f32 = 13.0;
const PALETTE_LABEL: &str = "Search commands";
const DROP_RING_PX: f32 = 1.5;
const TAB_GHOST_OPACITY: f32 = 0.7;
const TAB_GHOST_MIN_WIDTH_PX: f32 = 100.0;
const TAB_MIN_WIDTH_PX: f32 = 70.0;
const TAB_MAX_WIDTH_PX: f32 = 180.0;
const TAB_CONTENT_GAP_PX: f32 = 6.0;
const TAB_DOT_PX: f32 = 6.0;
const TAB_CLOSE_PX: f32 = 18.0;
const TAB_CLOSE_INSET_PX: f32 = 4.0;
const TAB_CLOSE_RADIUS_PX: f32 = 4.0;
const TAB_CLOSE_ICON_PX: f32 = 9.0;
const INACTIVE_TAB_CLOSE_OPACITY: f32 = 0.5;
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

enum TabLabel {
    Title(String),
    Editing(TextField),
}

struct TabPill {
    index: usize,
    label: TabLabel,
    is_active: bool,
    is_busy: bool,
}

pub(super) struct TabDrag {
    from: usize,
    press: Point<Pixels>,
    cursor: Point<Pixels>,
    is_active: bool,
    last_notify_at: Cell<Option<Instant>>,
}

impl Shell {
    pub(super) fn render_tab_strip(&self, is_fullscreen: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let layout = theme.layout;
        let active = (self.model.active_id(), self.model.tabs().active_index());
        if self.visible_tab.replace(Some(active)) != Some(active) {
            self.tab_scroll.scroll_to_item(active.1);
        }
        let leading_inset = theme.top_bar_leading_inset_px(is_fullscreen);
        div()
            .h(px(layout.tab_strip_height_px))
            .flex_none()
            .pl(px(leading_inset))
            .pr(px(layout.window_margin_px))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(theme.top_bar.item_gap_px))
            .font_family(SharedString::from(theme.ui_font.family.clone()))
            .text_size(px(theme.ui_font.tab_px))
            .on_mouse_down(MouseButton::Left, |event, window, _cx| {
                if event.click_count == DOUBLE_CLICK {
                    window.titlebar_double_click();
                }
            })
            .child(self.render_workspace_button(cx))
            .child(self.render_separator())
            .child(self.render_tab_list(cx))
            .child(div().flex_1())
            .child(self.render_palette_button(cx))
    }

    fn render_separator(&self) -> impl IntoElement {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        div()
            .flex_none()
            .w(px(theme.top_bar.separator_width_px))
            .h(px(theme.top_bar.separator_height_px))
            .mx(px(SEPARATOR_MARGIN_X_PX * scale))
            .bg(theme.border.default)
    }

    fn render_tab_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let gap = px(self.state.theme.top_bar.tab_gap_px);
        div()
            .min_w(px(0.0))
            .flex_shrink()
            .flex()
            .flex_row()
            .items_center()
            .gap(gap)
            .child(
                div()
                    .id("workspace-tabs")
                    .min_w(px(0.0))
                    .flex_shrink()
                    .overflow_x_scroll()
                    .track_scroll(&self.tab_scroll)
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(gap)
                    .children(self.tab_pills().into_iter().map(|pill| self.render_tab_pill(pill, cx))),
            )
            .child(self.render_new_tab_button(cx))
    }

    fn tab_pills(&self) -> Vec<TabPill> {
        let active = self.model.tabs().active_index();
        self.model
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let label = match self.tab_field.as_ref().filter(|field| field.index == index) {
                    Some(field) => TabLabel::Editing(field.input.clone()),
                    None => TabLabel::Title(self.tab_title(tab)),
                };
                TabPill { index, label, is_active: index == active, is_busy: self.is_tab_busy(tab) }
            })
            .collect()
    }

    fn render_tab_pill(&self, pill: TabPill, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.state.theme;
        let top_bar = theme.top_bar;
        let scale = self.state.settings.ui_scale;
        let index = pill.index;
        let is_raised = pill.is_active || matches!(pill.label, TabLabel::Editing(_));
        let title = match pill.label {
            TabLabel::Editing(input) => field_text(&input, "", theme),
            TabLabel::Title(title) => div().min_w(px(0.0)).truncate().child(title),
        };
        let mut shadows = match is_raised {
            true => box_shadows(&theme.shadow.tab),
            false => Vec::new(),
        };
        if self.is_tab_drop_target(index) || self.is_tab_drag_target(index) {
            shadows.push(drop_ring(theme));
        }
        let status_dot = pill.is_busy.then(|| {
            div().flex_none().size(px(TAB_DOT_PX * scale)).rounded_full().bg(theme.status.running)
        });
        let pill_body = div()
            .h(px(top_bar.control_height_px))
            .min_w(px(0.0))
            .w_full()
            .pl(px(top_bar.tab_padding_left_px))
            .pr(px(top_bar.tab_padding_right_px))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(TAB_CONTENT_GAP_PX * scale))
            .rounded(px(top_bar.control_radius_px))
            .text_color(theme.text.muted)
            .shadow(shadows)
            .when(is_raised, |body| body.bg(theme.surface.raised))
            .when(pill.is_active, |body| body.text_color(theme.text.default).font_weight(FontWeight::MEDIUM))
            .children(status_dot)
            .child(title);
        div()
            .id(("tab", index))
            .relative()
            .flex_none()
            .min_w(px(TAB_MIN_WIDTH_PX * scale))
            .max_w(px(TAB_MAX_WIDTH_PX * scale))
            .flex()
            .items_center()
            .cursor_pointer()
            .child(bounds_probe(&self.tab_bounds, index))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, event: &gpui::MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    shell.press_tab_pill(index, event.position, cx);
                }),
            )
            .child(pill_body)
            .child(self.render_tab_close(index, pill.is_active, cx))
            .into_any_element()
    }

    fn render_tab_close(&self, index: usize, is_active: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let opacity = if is_active { 1.0 } else { INACTIVE_TAB_CLOSE_OPACITY };
        div()
            .id(("tab-close", index))
            .absolute()
            .right(px(TAB_CLOSE_INSET_PX * scale))
            .size(px(TAB_CLOSE_PX * scale))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(TAB_CLOSE_RADIUS_PX * scale))
            .opacity(opacity)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event, _window, cx| {
                    cx.stop_propagation();
                    shell.request_close_tab_at(index, cx);
                    cx.notify();
                }),
            )
            .child(Icon::CloseBold.sized(TAB_CLOSE_ICON_PX * scale, theme.text.subtle))
    }

    fn render_workspace_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let name = workspace_button_label(self.model.workspace_name(self.model.active_id())).to_string();
        let chevron_tint = theme.text.default.opacity(CHEVRON_OPACITY);
        strip_button("workspace-menu-button", theme)
            .px(px(WORKSPACE_BUTTON_PADDING_X_PX * scale))
            .gap(px(WORKSPACE_BUTTON_GAP_PX * scale))
            .max_w(px(TAB_MAX_WIDTH_PX * scale))
            .text_size(px(theme.ui_font.workspace_px))
            .text_color(theme.text.default)
            .when(self.is_workspace_menu_open, |el| el.bg(theme.chrome.control_fill))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _event, _window, cx| {
                    cx.stop_propagation();
                    shell.toggle_workspace_menu(cx);
                }),
            )
            .child(logo_mark(theme.top_bar.logo_px, theme.accent.primary, theme.chrome.logo_ink))
            .child(div().min_w(px(0.0)).truncate().font_weight(FontWeight::SEMIBOLD).child(name))
            .child(Icon::Chevron.sized(CHEVRON_PX * scale, chevron_tint))
    }

    fn render_new_tab_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        strip_button("new-tab-button", theme)
            .w(px(theme.top_bar.control_height_px))
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _event, window, cx| {
                    cx.stop_propagation();
                    shell.run_command(CommandId::NewTab, window, cx);
                }),
            )
            .child(Icon::Plus.sized(NEW_TAB_ICON_PX * self.state.settings.ui_scale, theme.text.muted))
    }

    fn render_palette_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let shortcut = self.state.keymap.combo_for(CommandId::OpenPalette).map(|combo| kbd_chip(combo.glyphs(), theme));
        strip_button("palette-button", theme)
            .flex_shrink()
            .w(px(PALETTE_BUTTON_WIDTH_PX * scale))
            .min_w(px(PALETTE_BUTTON_MIN_WIDTH_PX * scale))
            .pl(px(PALETTE_BUTTON_PADDING_LEFT_PX * scale))
            .pr(px(PALETTE_BUTTON_PADDING_RIGHT_PX * scale))
            .gap(px(PALETTE_BUTTON_GAP_PX * scale))
            .rounded(px(PALETTE_BUTTON_RADIUS_PX * scale))
            .bg(theme.chrome.control_fill)
            .text_size(px(theme.ui_font.button_px))
            .text_color(theme.text.subtle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _event, window, cx| {
                    cx.stop_propagation();
                    shell.run_command(CommandId::OpenPalette, window, cx);
                }),
            )
            .child(Icon::MagnifierBold.sized(PALETTE_ICON_PX * scale, theme.text.subtle))
            .child(div().min_w(px(0.0)).truncate().child(PALETTE_LABEL))
            .child(div().flex_1())
            .children(shortcut)
    }

    fn press_tab_pill(&mut self, index: usize, press: Point<Pixels>, cx: &mut Context<Self>) {
        if self.tab_field.as_ref().is_some_and(|field| field.index == index) {
            return;
        }
        if index >= self.model.tabs().len() {
            return;
        }
        self.commit_tab_field();
        self.tab_drag = Some(TabDrag {
            from: index,
            press,
            cursor: press,
            is_active: false,
            last_notify_at: Cell::new(None),
        });
        cx.notify();
    }

    pub(super) fn drag_tab(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(drag) = self.tab_drag.as_mut() else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.cancel_tab_drag(cx);
            return;
        }
        drag.cursor = event.position;
        drag.is_active = drag.is_active || has_left_dead_zone(drag.press, event.position);
        if drag.is_active && should_notify_drag(&drag.last_notify_at, Instant::now()) {
            cx.notify();
        }
    }

    pub(super) fn release_tab_drag(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.tab_drag.take() else {
            return;
        };
        cx.notify();
        if !drag.is_active {
            self.activate_tab(drag.from, window, cx);
            if event.click_count == DOUBLE_CLICK {
                self.start_rename_tab(drag.from);
            }
            return;
        }
        let bounds: Vec<(usize, Bounds<Pixels>)> =
            self.tab_bounds.borrow().iter().map(|(index, bounds)| (*index, *bounds)).collect();
        let Some(target) = tab_drop_target(event.position, &bounds, drag.from) else {
            return;
        };
        if target != drag.from {
            self.model.tabs_mut().reorder(drag.from, target);
            self.persist();
        }
    }

    pub(super) fn cancel_tab_drag(&mut self, cx: &mut Context<Self>) {
        if self.tab_drag.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn is_dragging_tab(&self) -> bool {
        self.tab_drag.as_ref().is_some_and(|drag| drag.is_active)
    }

    fn is_tab_drag_target(&self, index: usize) -> bool {
        let Some(drag) = self.tab_drag.as_ref() else {
            return false;
        };
        if !drag.is_active || drag.from == index {
            return false;
        }
        self.tab_bounds.borrow().get(&index).is_some_and(|bounds| bounds.contains(&drag.cursor))
    }

    pub(super) fn render_tab_ghost(&self) -> Option<AnyElement> {
        let drag = self.tab_drag.as_ref().filter(|drag| drag.is_active)?;
        let theme = &self.state.theme;
        let title = self.model.tabs().iter().nth(drag.from).map(|tab| self.tab_title(tab)).unwrap_or_default();
        let from_bounds = self.tab_bounds.borrow().get(&drag.from).copied();
        let width = from_bounds
            .map(|bounds| bounds.size.width)
            .unwrap_or(px(TAB_GHOST_MIN_WIDTH_PX));
        let height = from_bounds
            .map(|bounds| bounds.size.height)
            .unwrap_or(px(theme.top_bar.control_height_px));
        let origin_x = drag.cursor.x - width * 0.5;
        let origin_y = drag.cursor.y - height * 0.5;
        Some(
            div()
                .absolute()
                .left(origin_x)
                .top(origin_y)
                .w(width)
                .h(height)
                .opacity(TAB_GHOST_OPACITY)
                .bg(theme.surface.canvas)
                .border_1()
                .border_color(theme.accent.primary)
                .rounded(px(theme.top_bar.control_radius_px))
                .flex()
                .items_center()
                .pl(px(theme.top_bar.tab_padding_left_px))
                .pr(px(theme.top_bar.tab_padding_right_px))
                .text_color(theme.text.default)
                .child(title)
                .into_any_element(),
        )
    }

    pub(super) fn start_rename_tab(&mut self, index: usize) {
        let Some(tab) = self.model.tabs().iter().nth(index) else {
            return;
        };
        let input = TextField::new(tab.name().unwrap_or_default());
        self.tab_field = Some(TabField { index, input });
    }

    pub(super) fn handle_tab_field_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        match keystroke.key.as_str() {
            ESCAPE_KEY => self.tab_field = None,
            ENTER_KEY => self.commit_tab_field(),
            _ => return self.type_into_field(FieldTarget::TabName, keystroke, cx),
        }
        cx.notify();
        true
    }

    pub(super) fn commit_tab_field(&mut self) {
        let Some(field) = self.tab_field.take() else {
            return;
        };
        let Some(tab) = self.model.tabs_mut().get_mut(field.index) else {
            return;
        };
        tab.rename(Some(field.input.text().to_string()));
        self.persist();
    }

    pub(super) fn commit_tab_field_on_click(&mut self, cx: &mut Context<Self>) {
        if self.tab_field.is_some() || self.pane_field.is_some() {
            self.commit_tab_field();
            self.commit_pane_field();
            cx.notify();
        }
    }

    fn activate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.model.tabs_mut().activate(index);
        self.persist();
        window.focus(&self.focus_handle);
        cx.notify();
    }
}

fn strip_button(id: &'static str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(theme.top_bar.control_height_px))
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .cursor_pointer()
        .rounded(px(theme.top_bar.control_radius_px))
}

fn drop_ring(theme: &Theme) -> BoxShadow {
    BoxShadow {
        color: theme.accent.primary,
        offset: gpui::point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(DROP_RING_PX),
    }
}

pub(super) struct TabField {
    pub(super) index: usize,
    pub(super) input: TextField,
}
