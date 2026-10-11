use super::Shell;
use crate::copy_mode::{copy_mode_action, CopyModeAction};
use domain::PanelId;
use gpui::{ClipboardItem, Context, Keystroke};
use terminal_core::Terminal;

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
