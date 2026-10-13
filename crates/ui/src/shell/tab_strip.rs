use super::pane_drag::bounds_probe;
use super::{FieldTarget, Shell, ENTER_KEY, ESCAPE_KEY};
use crate::command::CommandId;
use crate::pane_drop::{has_left_dead_zone, tab_drop_target};
use crate::tabs::numbered_tab_label;
use crate::theme::icons::Icon;
use crate::text_field::TextField;
use crate::theme::menu_chrome::field_text;
use crate::theme::{box_shadows, Theme};
use crate::workspace_menu::workspace_button_label;
use gpui::{
    div, img, prelude::*, px, AnyElement, Bounds, BoxShadow, Context, Div, FontWeight, Image, ImageFormat, Keystroke,
    MouseButton, MouseMoveEvent, MouseUpEvent, Pixels, Point, SharedString, Stateful, Window,
};
use std::sync::{Arc, LazyLock};

const DOUBLE_CLICK: usize = 2;
const LOGO_PNG: &[u8] = include_bytes!("../../../../assets/logo.png");
const WORKSPACE_BUTTON_PADDING_X_PX: f32 = 8.0;
const WORKSPACE_BUTTON_GAP_PX: f32 = 6.0;
const CHEVRON_PX: f32 = 10.0;
const NEW_TAB_GLYPH: &str = "+";
const NEW_TAB_GLYPH_PX: f32 = 17.0;
const PALETTE_BUTTON_PADDING_X_PX: f32 = 8.0;
const PALETTE_BUTTON_GAP_PX: f32 = 5.0;
const PALETTE_ICON_PX: f32 = 12.0;
const DROP_RING_PX: f32 = 1.5;
const TAB_GHOST_OPACITY: f32 = 0.7;
const TAB_GHOST_MIN_WIDTH_PX: f32 = 100.0;
const TAB_PILL_MIN_WIDTH_PX: f32 = 40.0;

static LOGO: LazyLock<Arc<Image>> =
    LazyLock::new(|| Arc::new(Image::from_bytes(ImageFormat::Png, LOGO_PNG.to_vec())));

enum TabLabel {
    Title(String),
    Editing(TextField),
}

struct TabPill {
    index: usize,
    label: TabLabel,
    is_active: bool,
}

pub(super) struct TabDrag {
    from: usize,
    press: Point<Pixels>,
    cursor: Point<Pixels>,
    is_active: bool,
}

impl Shell {
    pub(super) fn render_tab_strip(&self, is_fullscreen: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let layout = theme.layout;
        let leading_inset = theme.top_bar_leading_inset_px(is_fullscreen);
        div()
            .h(px(layout.tab_strip_height_px))
            .flex_none()
            .pl(px(leading_inset))
            .pr(px(layout.window_margin_px))
            .flex()
            .flex_row()
            .items_center()
            .font_family(SharedString::from(theme.ui_font.family.clone()))
            .text_size(px(theme.ui_font.tab_px))
            .on_mouse_down(MouseButton::Left, |event, window, _cx| {
                if event.click_count == DOUBLE_CLICK {
                    window.titlebar_double_click();
                }
            })
            .child(self.render_workspace_button(cx))
            .child(strip_separator(theme))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(theme.top_bar.tab_gap_px))
                    .children(self.tab_pills().into_iter().map(|pill| self.render_tab_pill(pill, cx))),
            )
            .child(self.render_new_tab_button(cx))
            .child(div().flex_1())
            .child(self.render_palette_button(cx))
    }

    fn tab_pills(&self) -> Vec<TabPill> {
        let active = self.model.tabs().active_index();
        self.model
            .tabs()
            .labels()
            .into_iter()
            .enumerate()
            .map(|(index, title)| {
                let label = match self.tab_field.as_ref().filter(|field| field.index == index) {
                    Some(field) => TabLabel::Editing(field.input.clone()),
                    None => TabLabel::Title(title),
                };
                TabPill { index, label, is_active: index == active }
            })
            .collect()
    }

    fn render_tab_pill(&self, pill: TabPill, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.state.theme;
        let top_bar = theme.top_bar;
        let index = pill.index;
        let hover_fill = theme.chrome.control_fill;
        let is_raised = pill.is_active || matches!(pill.label, TabLabel::Editing(_));
        let content = match pill.label {
            TabLabel::Editing(input) => field_text(&input, &numbered_tab_label(index), theme),
            TabLabel::Title(title) => div().child(title),
        };
        let mut shadows = match is_raised {
            true => box_shadows(&theme.shadow.tab),
            false => Vec::new(),
        };
        if self.is_tab_drop_target(index) || self.is_tab_drag_target(index) {
            shadows.push(drop_ring(theme));
        }
        div()
            .id(("tab", index))
            .relative()
            .h(px(top_bar.control_height_px))
            .min_w(px(TAB_PILL_MIN_WIDTH_PX))
            .px(px(top_bar.tab_padding_x_px))
            .flex()
            .items_center()
            .cursor_pointer()
            .rounded(px(top_bar.control_radius_px))
            .text_color(theme.text.muted)
            .shadow(shadows)
            .when(is_raised, |pill| pill.bg(theme.surface.canvas))
            .when(pill.is_active, |pill| pill.text_color(theme.text.default).font_weight(FontWeight::MEDIUM))
            .when(!is_raised, |pill| pill.hover(move |style| style.bg(hover_fill)))
            .when(pill.is_active, |pill| pill.border_b_2().border_color(theme.accent.primary))
            .child(bounds_probe(&self.tab_bounds, index))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, event: &gpui::MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    shell.press_tab_pill(index, event.position, cx);
                }),
            )
            .child(content)
            .into_any_element()
    }

    fn render_workspace_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let name = workspace_button_label(self.model.workspace_name(self.model.active_id())).to_string();
        strip_button("workspace-menu-button", theme)
            .px(px(WORKSPACE_BUTTON_PADDING_X_PX))
            .gap(px(WORKSPACE_BUTTON_GAP_PX))
            .bg(theme.chrome.control_fill)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _event, _window, cx| {
                    cx.stop_propagation();
                    shell.toggle_workspace_menu(cx);
                }),
            )
            .child(img(LOGO.clone()).size(px(theme.top_bar.logo_px)).flex_none())
            .child(div().font_weight(FontWeight::MEDIUM).text_color(theme.text.default).child(name))
            .child(Icon::Chevron.sized(CHEVRON_PX))
    }

    fn render_new_tab_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        strip_button("new-tab-button", theme)
            .w(px(theme.top_bar.control_height_px))
            .justify_center()
            .ml(px(theme.top_bar.tab_gap_px))
            .text_size(px(NEW_TAB_GLYPH_PX))
            .text_color(theme.text.subtle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _event, window, cx| {
                    cx.stop_propagation();
                    shell.run_command(CommandId::NewTab, window, cx);
                }),
            )
            .child(NEW_TAB_GLYPH)
    }

    fn render_palette_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let shortcut = self.state.keymap.combo_for(CommandId::OpenPalette).map(|combo| combo.glyphs());
        strip_button("palette-button", theme)
            .px(px(PALETTE_BUTTON_PADDING_X_PX))
            .gap(px(PALETTE_BUTTON_GAP_PX))
            .text_size(px(theme.ui_font.hint_px))
            .text_color(theme.text.subtle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _event, window, cx| {
                    cx.stop_propagation();
                    shell.run_command(CommandId::OpenPalette, window, cx);
                }),
            )
            .child(Icon::Magnifier.sized(PALETTE_ICON_PX))
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
        self.tab_drag = Some(TabDrag { from: index, press, cursor: press, is_active: false });
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
        if drag.is_active {
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
        let title = self
            .model
            .tabs()
            .iter()
            .nth(drag.from)
            .map(|tab| tab.label(drag.from))
            .unwrap_or_default();
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
                .px(px(theme.top_bar.tab_padding_x_px))
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
    let hover_fill = theme.chrome.icon_hover_fill;
    div()
        .id(id)
        .h(px(theme.top_bar.control_height_px))
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .cursor_pointer()
        .rounded(px(theme.top_bar.control_radius_px))
        .hover(move |style| style.bg(hover_fill))
}

fn strip_separator(theme: &Theme) -> Div {
    let top_bar = theme.top_bar;
    div()
        .flex_none()
        .w(px(top_bar.separator_width_px))
        .h(px(top_bar.separator_height_px))
        .mx(px(top_bar.item_gap_px))
        .bg(theme.border.default)
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
