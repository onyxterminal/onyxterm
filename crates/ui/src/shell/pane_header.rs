use super::{short_cwd, Shell};
use crate::command::CommandId;
use crate::workspace_menu::CARET;
use domain::{LayoutNode, PanelId};
use gpui::{div, prelude::*, px, Context, Hsla, MouseButton, SharedString, Window};

const CLOSE_GLYPH: &str = "\u{00d7}";
const GEAR_GLYPH: &str = "\u{2699}\u{fe0e}";
const GEAR_TOOLTIP: &str = "Pane Options";
const CLOSE_TOOLTIP: &str = "Close Pane";

impl Shell {
    pub(super) fn render_pane_header(&self, panel: PanelId, is_focused: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let title = self.pane_display_title(panel);
        let cwd = short_cwd(&self.panel_cwd(panel), self.model.home());
        let title_color = if is_focused { theme.text_default() } else { theme.text.muted };
        let editing = self.pane_field.as_ref().filter(|field| field.panel == panel);
        let title_element = match editing {
            Some(field) => div()
                .flex_shrink()
                .min_w(px(0.0))
                .px_1()
                .rounded(px(theme.layout.border_radius_px))
                .border_1()
                .border_color(theme.border_focus())
                .child(format!("{}{CARET}", field.text)),
            None => div().flex_shrink().min_w(px(0.0)).truncate().text_color(title_color).child(title),
        };
        let drag_handle = div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
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
            .child(title_element)
            .child(div().flex_1().min_w(px(0.0)).truncate().text_color(theme.text.muted).child(cwd));
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(theme.layout.pane_header_height_px))
            .px_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .bg(theme.canvas())
            .rounded_t(px(theme.layout.island_radius_px))
            .border_b_1()
            .border_color(theme.border.muted)
            .child(drag_handle)
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(self.gear_button(panel, cx))
                    .child(self.close_button(panel, cx)),
            )
    }

    fn header_button(&self, id: String, tooltip: &'static str, glyph: &'static str) -> gpui::Stateful<gpui::Div> {
        let theme = &self.state.theme;
        let hover_bg = theme.surface.subtle;
        let tooltip = SharedString::from(tooltip);
        let colors = (theme.canvas(), theme.border_default(), theme.text_default());
        div()
            .id(SharedString::from(id))
            .relative()
            .px_1()
            .flex()
            .items_center()
            .rounded(px(theme.layout.border_radius_px))
            .cursor_pointer()
            .text_color(theme.text.muted)
            .hover(move |style| style.bg(hover_bg))
            .tooltip(move |_window, cx| cx.new(|_| Tooltip { text: tooltip.clone(), colors }).into())
            .child(glyph)
    }

    fn gear_button(&self, panel: PanelId, cx: &mut Context<Self>) -> impl IntoElement {
        self.header_button(format!("pane-gear-{panel}"), GEAR_TOOLTIP, GEAR_GLYPH)
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
        self.header_button(format!("pane-close-{panel}"), CLOSE_TOOLTIP, CLOSE_GLYPH).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, _event, window, cx| {
                cx.stop_propagation();
                shell.focus_pane(panel);
                window.focus(&shell.focus_handle);
                shell.run_command(CommandId::CloseActive, window, cx);
            }),
        )
    }

    pub(super) fn pane_border(&self, is_focused: bool) -> Hsla {
        let theme = &self.state.theme;
        let is_split = self
            .model
            .tabs()
            .active()
            .is_some_and(|tab| !tab.is_maximized() && !matches!(tab.layout(), LayoutNode::Leaf { .. }));
        match is_split && is_focused {
            true => theme.accent.primary,
            false => theme.border_default(),
        }
    }
}

struct Tooltip {
    text: SharedString,
    colors: (Hsla, Hsla, Hsla),
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (bg, border, fg) = self.colors;
        div().px_2().py_1().rounded_md().shadow_md().bg(bg).border_1().border_color(border).text_color(fg).child(self.text.clone())
    }
}
