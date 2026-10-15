use super::Shell;
use crate::command::CommandId;
use crate::document::DocumentPane;
use crate::theme::icons::Icon;
use crate::theme::menu_chrome::field_text;
use domain::PanelId;
use gpui::{
    div, prelude::*, px, AnyView, App, Context, FontWeight, Hsla, MouseButton, SharedString, Stateful, Svg, Window,
};

const COPY_MODE_LABEL: &str = "Copy mode";
const MAXIMIZED_LABEL: &str = "Maximized";
const HEADER_GAP_PX: f32 = 8.0;
const UNFOCUSED_BUTTON_OPACITY: f32 = 0.45;
const RENAME_WIDTH_PX: f32 = 180.0;
const RENAME_HEIGHT_PX: f32 = 22.0;
const RENAME_PADDING_X_PX: f32 = 6.0;
const RENAME_RADIUS_PX: f32 = 5.0;
const PILL_PADDING_X_PX: f32 = 7.0;
const PILL_PADDING_Y_PX: f32 = 2.0;
const PILL_RADIUS_PX: f32 = 9.0;
const PILL_LINE_HEIGHT_PX: f32 = 12.5;

struct PillPaint {
    fill: Hsla,
    text: Hsla,
    weight: FontWeight,
}

impl Shell {
    pub(super) fn render_pane_header(&self, panel: PanelId, is_focused: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let header = theme.pane_header;
        let scale = self.state.settings.ui_scale;
        let drag_handle = div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(HEADER_GAP_PX * scale))
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
            .children(self.tree_pane(panel).is_none().then(|| self.status_dot(panel)))
            .child(self.header_title_element(panel, is_focused));
        let is_tree = self.tree_pane(panel).is_some();
        let button_opacity = if is_focused || is_tree { 1.0 } else { UNFOCUSED_BUTTON_OPACITY };
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(theme.layout.pane_header_height_px))
            .pl(px(header.padding_left_px))
            .pr(px(header.padding_right_px))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(HEADER_GAP_PX * scale))
            .font_family(SharedString::from(theme.ui_font.family.clone()))
            .text_size(px(theme.ui_font.pane_title_px))
            .child(drag_handle)
            .when(self.is_pane_in_copy_mode(panel), |header| header.child(self.copy_mode_pill()))
            .when(self.is_pane_maximized(panel), |header| header.child(self.maximized_pill()))
            .when(!is_tree, |header| header.child(self.more_button(panel, cx).opacity(button_opacity)))
            .child(self.close_button(panel, cx).opacity(button_opacity))
    }

    fn is_pane_maximized(&self, panel: PanelId) -> bool {
        self.model.tabs().active().is_some_and(|tab| tab.is_maximized() && tab.focused() == panel)
    }

    fn copy_mode_pill(&self) -> impl IntoElement {
        let theme = &self.state.theme;
        let paint = PillPaint { fill: theme.accent.selection, text: theme.accent.primary, weight: FontWeight::SEMIBOLD };
        self.header_pill(COPY_MODE_LABEL, paint)
    }

    fn maximized_pill(&self) -> impl IntoElement {
        let theme = &self.state.theme;
        let paint = PillPaint { fill: theme.chrome.control_fill, text: theme.text.muted, weight: FontWeight::NORMAL };
        self.header_pill(MAXIMIZED_LABEL, paint)
    }

    fn header_pill(&self, label: &'static str, paint: PillPaint) -> impl IntoElement {
        let scale = self.state.settings.ui_scale;
        div()
            .flex_none()
            .px(px(PILL_PADDING_X_PX * scale))
            .py(px(PILL_PADDING_Y_PX * scale))
            .rounded(px(PILL_RADIUS_PX * scale))
            .bg(paint.fill)
            .text_color(paint.text)
            .text_size(px(self.state.theme.ui_font.pill_px))
            .line_height(px(PILL_LINE_HEIGHT_PX * scale))
            .font_weight(paint.weight)
            .child(label)
    }

    fn status_dot(&self, panel: PanelId) -> impl IntoElement {
        let theme = &self.state.theme;
        let color = if self.documents.get(&panel).is_some_and(DocumentPane::is_dirty) {
            theme.accent.primary
        } else {
            self.pane_run_state(panel).color(theme)
        };
        div().flex_none().size(px(theme.pane_header.status_dot_px)).rounded_full().bg(color)
    }

    fn header_title_element(&self, panel: PanelId, is_focused: bool) -> Stateful<gpui::Div> {
        match self.tree_pane(panel) {
            Some(tree) => self.tree_title_element(panel, tree.branch().map(str::to_string)),
            None => self.pane_title_element(panel, is_focused),
        }
    }

    fn tree_title_element(&self, panel: PanelId, branch: Option<String>) -> Stateful<gpui::Div> {
        let theme = &self.state.theme;
        let workspace = self.model.workspace_name(self.model.active_id()).unwrap_or_default().to_string();
        let title = div().flex_none().text_color(theme.text.default).font_weight(FontWeight::SEMIBOLD).child(workspace);
        self.title_row(panel).child(title).children(branch.map(|branch| self.header_subtitle(branch)))
    }

    fn header_subtitle(&self, text: String) -> gpui::Div {
        let theme = &self.state.theme;
        div().min_w(px(0.0)).truncate().text_size(px(theme.ui_font.hint_px)).text_color(theme.text.subtle).child(text)
    }

    fn title_row(&self, panel: PanelId) -> Stateful<gpui::Div> {
        let scale = self.state.settings.ui_scale;
        div()
            .id(SharedString::from(format!("pane-title-{panel}")))
            .flex_shrink()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_baseline()
            .gap(px(HEADER_GAP_PX * scale))
            .overflow_hidden()
            .whitespace_nowrap()
    }

    fn pane_title_element(&self, panel: PanelId, is_focused: bool) -> Stateful<gpui::Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let id = SharedString::from(format!("pane-title-{panel}"));
        if let Some(field) = self.pane_field.as_ref().filter(|field| field.panel == panel) {
            return div()
                .id(id)
                .flex_none()
                .w(px(RENAME_WIDTH_PX * scale))
                .h(px(RENAME_HEIGHT_PX * scale))
                .px(px(RENAME_PADDING_X_PX * scale))
                .flex()
                .items_center()
                .overflow_hidden()
                .rounded(px(RENAME_RADIUS_PX * scale))
                .border_1()
                .border_color(theme.border.focus)
                .bg(theme.chrome.control_fill)
                .child(field_text(&field.input, &self.pane_display_title(panel), theme));
        }
        let title_color = if is_focused { theme.text.default } else { theme.text.muted };
        let cwd = self.pane_cwd_label(panel).map(|cwd| self.header_subtitle(cwd));
        self.title_row(panel)
            .tooltip(self.tooltip_builder(self.pane_title_tooltip(panel).into()))
            .child(
                div()
                    .flex_none()
                    .text_color(title_color)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.pane_display_title(panel)),
            )
            .children(cwd)
    }

    pub(super) fn header_button(&self, id: String, icon: Svg) -> Stateful<gpui::Div> {
        let header = self.state.theme.pane_header;
        div()
            .id(SharedString::from(id))
            .relative()
            .flex_none()
            .size(px(header.icon_button_px))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(header.icon_button_radius_px))
            .cursor_pointer()
            .child(icon)
    }

    pub(super) fn tooltip_builder(&self, text: SharedString) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
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

    fn more_button(&self, panel: PanelId, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let theme = &self.state.theme;
        let icon = Icon::More.sized(theme.pane_header.icon_px, theme.text.subtle);
        self.header_button(format!("pane-more-{panel}"), icon).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, _event, window, cx| {
                cx.stop_propagation();
                shell.toggle_pane_menu(panel, window, cx);
            }),
        )
    }

    fn close_button(&self, panel: PanelId, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let theme = &self.state.theme;
        let icon = Icon::Close.sized(theme.pane_header.close_icon_px, theme.text.subtle);
        self.header_button(format!("pane-close-{panel}"), icon).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, _event, window, cx| {
                cx.stop_propagation();
                shell.focus_pane(panel);
                window.focus(&shell.focus_handle);
                shell.run_command(CommandId::ClosePane, window, cx);
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
