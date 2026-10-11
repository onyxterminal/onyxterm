use super::islands::pane_group;
use super::Shell;
use crate::command::CommandId;
use crate::theme::icons::Icon;
use crate::theme::menu_chrome::{field_text, focused_field};
use domain::PanelId;
use gpui::{div, prelude::*, px, AnyView, App, Context, FontWeight, Hsla, MouseButton, SharedString, Stateful, Window};

const GEAR_TOOLTIP: &str = "Pane Options";
const CLOSE_TOOLTIP: &str = "Close Pane";
const HEADER_GAP_PX: f32 = 7.0;
const BUTTON_GAP_PX: f32 = 2.0;
const RENAME_PADDING_X_PX: f32 = 6.0;
const RENAME_RADIUS_PX: f32 = 5.0;

impl Shell {
    pub(super) fn render_pane_header(&self, panel: PanelId, is_focused: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let header = theme.pane_header;
        let group = pane_group(panel);
        let drag_handle = div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(HEADER_GAP_PX))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    shell.focus_pane(panel);
                    window.focus(&shell.focus_handle);
                    shell.commit_tab_field_on_click(cx);
                    shell.press_pane_title(panel, event.position);
                    cx.notify();
                }),
            )
            .child(self.status_dot(panel))
            .child(self.pane_title_element(panel, is_focused));
        let buttons = div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(BUTTON_GAP_PX))
            .when(!is_focused, |buttons| buttons.invisible().group_hover(group.clone(), |style| style.visible()))
            .child(self.gear_button(panel, cx))
            .child(self.close_button(panel, cx));
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(theme.layout.pane_header_height_px))
            .px(px(header.padding_x_px))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(HEADER_GAP_PX))
            .font_family(SharedString::from(theme.ui_font.family.clone()))
            .text_size(px(theme.ui_font.pane_title_px))
            .bg(theme.surface.canvas)
            .rounded_t(px(theme.layout.island_radius_px))
            .child(drag_handle)
            .child(buttons)
    }

    fn status_dot(&self, panel: PanelId) -> impl IntoElement {
        let theme = &self.state.theme;
        let color = match self.is_pane_busy(panel) {
            true => theme.status.running,
            false => theme.status.idle,
        };
        div().flex_none().size(px(theme.pane_header.status_dot_px)).rounded_full().bg(color)
    }

    fn pane_title_element(&self, panel: PanelId, is_focused: bool) -> Stateful<gpui::Div> {
        let theme = &self.state.theme;
        let id = SharedString::from(format!("pane-title-{panel}"));
        if let Some(field) = self.pane_field.as_ref().filter(|field| field.panel == panel) {
            return focused_field(theme)
                .id(id)
                .flex_shrink()
                .min_w(px(0.0))
                .px(px(RENAME_PADDING_X_PX))
                .rounded(px(RENAME_RADIUS_PX))
                .child(field_text(&field.input, &self.pane_display_title(panel), theme));
        }
        let (color, weight) = match is_focused {
            true => (theme.text.default, FontWeight::SEMIBOLD),
            false => (theme.text.subtle, FontWeight::MEDIUM),
        };
        div()
            .id(id)
            .flex_1()
            .min_w(px(0.0))
            .truncate()
            .text_color(color)
            .font_weight(weight)
            .tooltip(self.tooltip_builder(self.pane_title_tooltip(panel).into()))
            .child(self.pane_display_title(panel))
    }

    fn header_button(&self, id: String, tooltip: &'static str, icon: Icon) -> Stateful<gpui::Div> {
        let header = self.state.theme.pane_header;
        let hover_fill = self.state.theme.chrome.icon_hover_fill;
        div()
            .id(SharedString::from(id))
            .relative()
            .size(px(header.icon_button_px))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(header.icon_button_radius_px))
            .cursor_pointer()
            .hover(move |style| style.bg(hover_fill))
            .tooltip(self.tooltip_builder(SharedString::from(tooltip)))
            .child(icon.sized(header.icon_px))
    }

    fn tooltip_builder(&self, text: SharedString) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let theme = &self.state.theme;
        let style = TooltipStyle {
            fill: theme.surface.canvas,
            border: theme.border.default,
            text: theme.text.default,
            font: theme.ui_font.family.clone().into(),
            size_px: theme.ui_font.hint_px,
            radius_px: theme.top_bar.control_radius_px,
        };
        move |_window, cx| cx.new(|_| Tooltip { text: text.clone(), style: style.clone() }).into()
    }

    fn gear_button(&self, panel: PanelId, cx: &mut Context<Self>) -> impl IntoElement {
        self.header_button(format!("pane-gear-{panel}"), GEAR_TOOLTIP, Icon::Gear)
            .child(self.gear_bounds_probe(panel))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event, window, cx| {
                    cx.stop_propagation();
                    shell.toggle_pane_menu(panel, window, cx);
                }),
            )
    }

    fn close_button(&self, panel: PanelId, cx: &mut Context<Self>) -> impl IntoElement {
        self.header_button(format!("pane-close-{panel}"), CLOSE_TOOLTIP, Icon::Close).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, _event, window, cx| {
                cx.stop_propagation();
                shell.focus_pane(panel);
                window.focus(&shell.focus_handle);
                shell.run_command(CommandId::CloseActive, window, cx);
            }),
        )
    }
}

#[derive(Clone)]
struct TooltipStyle {
    fill: Hsla,
    border: Hsla,
    text: Hsla,
    font: SharedString,
    size_px: f32,
    radius_px: f32,
}

struct Tooltip {
    text: SharedString,
    style: TooltipStyle,
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let style = &self.style;
        div()
            .px_2()
            .py_1()
            .rounded(px(style.radius_px))
            .shadow_md()
            .bg(style.fill)
            .border_1()
            .border_color(style.border)
            .font_family(style.font.clone())
            .text_size(px(style.size_px))
            .text_color(style.text)
            .child(self.text.clone())
    }
}
