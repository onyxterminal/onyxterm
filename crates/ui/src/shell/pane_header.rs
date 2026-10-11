use super::{short_cwd, Shell};
use crate::command::CommandId;
use domain::{LayoutNode, PanelId};
use gpui::{div, prelude::*, px, AnyElement, Context, Hsla, MouseButton, SharedString, Window};

const ICON_WIDTH_PX: f32 = 12.0;
const ICON_HEIGHT_PX: f32 = 10.0;
const RESTORE_ICON_INSET_PX: f32 = 2.0;
const CLOSE_GLYPH: &str = "\u{00d7}";

impl Shell {
    pub(super) fn render_pane_header(&self, panel: PanelId, is_focused: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let title = self.panes.get(&panel).map(|pane| pane.title.clone()).filter(|title| !title.is_empty());
        let title = title.unwrap_or_else(|| workspace_service::DEFAULT_PANEL_TITLE.to_string());
        let cwd = short_cwd(&self.panel_cwd(panel), self.model.home());
        let title_color = if is_focused { theme.text_default() } else { theme.text.muted };
        let is_maximized = self.model.tabs().active().is_some_and(|tab| tab.is_maximized());
        let actions = [
            HeaderAction { command: CommandId::SplitRight, tooltip: "Split Right" },
            HeaderAction { command: CommandId::SplitDown, tooltip: "Split Down" },
            HeaderAction {
                command: CommandId::MaximizePane,
                tooltip: if is_maximized { "Restore Pane" } else { "Maximize Pane" },
            },
            HeaderAction { command: CommandId::CloseActive, tooltip: "Close Pane" },
        ];
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
            .child(div().flex_shrink().min_w(px(0.0)).truncate().text_color(title_color).child(title))
            .child(div().flex_1().min_w(px(0.0)).truncate().text_color(theme.text.muted).child(cwd))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .children(actions.into_iter().map(|action| self.header_button(panel, action, is_maximized, cx))),
            )
    }

    fn header_button(
        &self,
        panel: PanelId,
        action: HeaderAction,
        is_maximized: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.state.theme;
        let hover_bg = theme.surface.subtle;
        let command = action.command;
        let tooltip = SharedString::from(action.tooltip);
        let colors = (theme.canvas(), theme.border_default(), theme.text_default());
        div()
            .id(SharedString::from(format!("{}-{panel}", action.tooltip)))
            .p_1()
            .flex()
            .items_center()
            .rounded(px(theme.layout.border_radius_px))
            .cursor_pointer()
            .text_color(theme.text.muted)
            .hover(move |style| style.bg(hover_bg))
            .tooltip(move |_window, cx| cx.new(|_| Tooltip { text: tooltip.clone(), colors }).into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event, window, cx| {
                    cx.stop_propagation();
                    shell.focus_pane(panel);
                    window.focus(&shell.focus_handle);
                    shell.run_command(command, window, cx);
                }),
            )
            .child(header_icon(command, is_maximized, theme.text.muted))
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

struct HeaderAction {
    command: CommandId,
    tooltip: &'static str,
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

fn header_icon(command: CommandId, is_maximized: bool, color: Hsla) -> AnyElement {
    let frame = || div().w(px(ICON_WIDTH_PX)).h(px(ICON_HEIGHT_PX)).border_1().border_color(color).rounded_sm();
    match command {
        CommandId::SplitRight => frame().flex().flex_row().child(div().w_1_2().h_full().border_r_1().border_color(color)),
        CommandId::SplitDown => frame().flex().flex_col().child(div().h_1_2().w_full().border_b_1().border_color(color)),
        CommandId::MaximizePane if is_maximized => frame().p(px(RESTORE_ICON_INSET_PX)).child(
            div().size_full().border_1().border_color(color),
        ),
        CommandId::MaximizePane => frame(),
        _ => div().text_color(color).child(CLOSE_GLYPH),
    }
    .into_any_element()
}
