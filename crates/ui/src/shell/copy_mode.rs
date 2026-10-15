use super::Shell;
use crate::copy_mode::{copy_mode_action, CopyModeAction};
use crate::theme::box_shadows;
use domain::PanelId;
use gpui::{div, prelude::*, px, ClipboardItem, Context, Div, FontWeight, Keystroke, SharedString};
use terminal_core::Terminal;

const COPY_HINTS: [(&str, &str); 4] = [("h j k l", "move"), ("v", "select"), ("y", "copy"), ("esc", "exit")];
const HINT_BOTTOM_PX: f32 = 10.0;
const HINT_PADDING_X_PX: f32 = 12.0;
const HINT_PADDING_Y_PX: f32 = 6.0;
const HINT_GAP_PX: f32 = 14.0;
const HINT_KEY_GAP_PX: f32 = 3.0;
const HINT_RADIUS_PX: f32 = 8.0;
const HINT_LINE_HEIGHT_PX: f32 = 13.0;

impl Shell {
    pub(super) fn toggle_copy_mode(&mut self) {
        self.is_copy_top_pending = false;
        let Some(terminal) = self.focused_terminal() else {
            return;
        };
        if terminal.is_vi_mode() {
            terminal.exit_vi_mode();
            return;
        }
        terminal.enter_vi_mode();
    }

    pub(super) fn is_copy_mode_active(&self) -> bool {
        self.focused_terminal().is_some_and(Terminal::is_vi_mode)
    }

    pub(super) fn is_pane_in_copy_mode(&self, panel: PanelId) -> bool {
        self.panes.get(&panel).is_some_and(|pane| pane.terminal.handle.terminal().is_vi_mode())
    }

    pub(super) fn handle_copy_mode_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let action = copy_mode_action(keystroke, self.is_copy_top_pending);
        self.is_copy_top_pending = action == CopyModeAction::AwaitTop;
        if let Some(terminal) = self.focused_terminal() {
            apply_copy_action(terminal, action, cx);
        }
        cx.notify();
        true
    }

    pub(super) fn render_copy_hint(&self, panel: PanelId) -> Option<Div> {
        if !self.is_pane_in_copy_mode(panel) {
            return None;
        }
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let hints = COPY_HINTS.iter().map(|(keys, action)| {
            div()
                .flex()
                .flex_row()
                .gap(px(HINT_KEY_GAP_PX * scale))
                .child(div().font_weight(FontWeight::BOLD).child(*keys))
                .child(*action)
        });
        let bar = div()
            .flex()
            .flex_row()
            .gap(px(HINT_GAP_PX * scale))
            .px(px(HINT_PADDING_X_PX * scale))
            .py(px(HINT_PADDING_Y_PX * scale))
            .rounded(px(HINT_RADIUS_PX * scale))
            .bg(theme.chrome.menu_fill)
            .border(px(theme.menu.border_px))
            .border_color(theme.border.default)
            .shadow(box_shadows(&theme.shadow.hint))
            .font_family(SharedString::from(theme.ui_font.family.clone()))
            .text_size(px(theme.ui_font.hint_px))
            .line_height(px(HINT_LINE_HEIGHT_PX * scale))
            .text_color(theme.text.muted)
            .whitespace_nowrap()
            .children(hints);
        Some(div().absolute().left_0().right_0().bottom(px(HINT_BOTTOM_PX * scale)).flex().justify_center().child(bar))
    }

    fn focused_terminal(&self) -> Option<&Terminal> {
        self.focused_handle().map(|handle| handle.terminal())
    }
}

fn apply_copy_action(terminal: &Terminal, action: CopyModeAction, cx: &mut Context<Shell>) {
    match action {
        CopyModeAction::Motion(motion) => terminal.vi_motion(motion),
        CopyModeAction::HalfPageUp => terminal.vi_scroll_lines(half_page(terminal)),
        CopyModeAction::HalfPageDown => terminal.vi_scroll_lines(-half_page(terminal)),
        CopyModeAction::Top => terminal.vi_goto_top(),
        CopyModeAction::Bottom => terminal.vi_goto_bottom(),
        CopyModeAction::ToggleSelection(kind) => terminal.toggle_vi_selection(kind),
        CopyModeAction::Yank => yank_selection(terminal, cx),
        CopyModeAction::Exit => terminal.exit_vi_mode(),
        CopyModeAction::AwaitTop | CopyModeAction::Ignore => {}
    }
}

fn half_page(terminal: &Terminal) -> i32 {
    i32::from(terminal.grid_size().rows / 2)
}

fn yank_selection(terminal: &Terminal, cx: &mut Context<Shell>) {
    if let Some(text) = terminal.selection_text() {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
    terminal.exit_vi_mode();
}
