use super::Shell;
use crate::document::{document_title, load_error_text, DocumentPane, EditorPane, EditorState};
use crate::editor_keys::{key_input, should_defer_to_input_handler};
use crate::editor_view::{scroll_to_reveal, PointerEvent, PointerHandler};
use domain::PanelId;
use editor_core::{io, standard_action, Buffer, ClickMode, Editor, EditorAction};
use gpui::{ClipboardItem, Context, KeyDownEvent, Window};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

const DEFAULT_PAGE_LINES: usize = 30;
const SAVE_FAILED_PREFIX: &str = "Couldn't save";

impl Shell {
    pub(super) fn load_editor(&mut self, panel: PanelId, path: PathBuf, cx: &mut Context<Self>) {
        let task = cx.background_executor().spawn(async move { io::load(&path) });
        cx.spawn(async move |shell, cx| {
            let loaded = task.await;
            let updated = shell.update(cx, |shell, cx| {
                shell.finish_load(panel, loaded);
                cx.notify();
            });
            if let Err(err) = updated {
                tracing::debug!(?err, "shell dropped before load finished");
            }
        })
        .detach();
    }

    fn finish_load(&mut self, panel: PanelId, loaded: Result<Buffer, io::LoadError>) {
        let Some(DocumentPane::Editor(pane)) = self.documents.get_mut(&panel) else {
            return;
        };
        pane.state = match loaded {
            Ok(buffer) => EditorState::Ready(Editor::new(buffer)),
            Err(err) => EditorState::Failed(load_error_text(&err)),
        };
    }

    pub(super) fn focused_editor_mut(&mut self) -> Option<(PanelId, &mut EditorPane)> {
        let panel = self.model.focused_document()?;
        let Some(DocumentPane::Editor(pane)) = self.documents.get_mut(&panel) else {
            return None;
        };
        Some((panel, pane))
    }

    pub(super) fn has_focused_editor(&self) -> bool {
        let Some(panel) = self.model.focused_document() else {
            return false;
        };
        let Some(DocumentPane::Editor(pane)) = self.documents.get(&panel) else {
            return false;
        };
        pane.ready().is_some()
    }

    pub(super) fn send_key_to_document(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if should_defer_to_input_handler(&event.keystroke, event.is_held) {
            return false;
        }
        let Some((panel, pane)) = self.focused_editor_mut() else {
            return false;
        };
        let page_lines = pane
            .view
            .borrow()
            .layout
            .as_ref()
            .map_or(DEFAULT_PAGE_LINES, |layout| layout.visible_line_count());
        let Some(action) = standard_action(&key_input(&event.keystroke), page_lines) else {
            return false;
        };
        self.apply_editor_action(panel, action, cx);
        true
    }

    pub(super) fn apply_editor_action(&mut self, panel: PanelId, action: EditorAction, cx: &mut Context<Self>) {
        let Some((focused, pane)) = self.focused_editor_mut() else {
            return;
        };
        if focused != panel {
            return;
        }
        let Some(editor) = pane.ready_mut() else {
            return;
        };
        if let Some(text) = editor.apply(action, Instant::now()) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        self.reveal_cursor(panel);
        cx.notify();
    }

    pub(super) fn reveal_cursor(&mut self, panel: PanelId) {
        let Some((focused, pane)) = self.focused_editor_mut() else {
            return;
        };
        if focused != panel {
            return;
        }
        let Some(editor) = pane.ready() else {
            return;
        };
        let rope = editor.buffer().rope();
        let head = editor.selections().newest().head.min(rope.len_chars());
        let line_index = rope.char_to_line(head);
        let rel = head - rope.line_to_char(line_index);
        let layout_snapshot = {
            let view_ref = pane.view.borrow();
            let Some(layout) = view_ref.layout.clone() else {
                return;
            };
            (view_ref.scroll_top, view_ref.scroll_left, layout)
        };
        let (scroll_top, scroll_left, layout) = layout_snapshot;
        let line_height = f32::from(layout.line_height);
        let viewport_height = f32::from(layout.viewport.height);
        let text_width = f32::from(layout.viewport.width);
        let new_scroll_top = scroll_to_reveal(scroll_top, line_index, line_height, viewport_height);
        let mut new_scroll_left = scroll_left;
        let margin = f32::from(layout.line_height);
        if let Some(laid) = layout.laid_line(line_index) {
            let x = f32::from(laid.x_at_char(rel));
            let left = scroll_left;
            let right = left + text_width;
            if x < left + margin {
                new_scroll_left = (x - margin).max(0.0);
            } else if x > right - margin {
                new_scroll_left = (x - text_width + margin).max(0.0);
            }
        }
        let mut view_mut = pane.view.borrow_mut();
        view_mut.scroll_top = new_scroll_top;
        view_mut.scroll_left = new_scroll_left;
    }

    pub(super) fn copy_from_editor(&mut self, cx: &mut Context<Self>) {
        let Some((_, pane)) = self.focused_editor_mut() else {
            return;
        };
        let Some(text) = pane.ready().and_then(|editor| editor.copy_text()) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        cx.notify();
    }

    pub(super) fn paste_into_editor(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let Some((panel, pane)) = self.focused_editor_mut() else {
            return;
        };
        let Some(editor) = pane.ready_mut() else {
            return;
        };
        editor.paste(&text, Instant::now());
        self.reveal_cursor(panel);
        cx.notify();
    }

    pub(super) fn select_all_in_editor(&mut self, cx: &mut Context<Self>) {
        let Some((panel, _)) = self.focused_editor_mut() else {
            return;
        };
        self.apply_editor_action(panel, EditorAction::SelectAll, cx);
    }

    pub(super) fn save_focused_editor(&mut self, cx: &mut Context<Self>) {
        let Some((panel, pane)) = self.focused_editor_mut() else {
            return;
        };
        let path = pane.path.clone();
        let Some(buffer) = pane.ready().map(|editor| editor.buffer().clone()) else {
            return;
        };
        let version = buffer.version();
        let task = cx.background_executor().spawn(async move { io::save(&path, &buffer) });
        cx.spawn(async move |shell, cx| {
            let result = task.await;
            let updated = shell.update(cx, |shell, cx| shell.finish_save(panel, version, result, cx));
            if let Err(err) = updated {
                tracing::debug!(?err, "shell dropped before save finished");
            }
        })
        .detach();
    }

    fn finish_save(
        &mut self,
        panel: PanelId,
        version: u64,
        result: std::io::Result<()>,
        cx: &mut Context<Self>,
    ) {
        let Some(DocumentPane::Editor(pane)) = self.documents.get_mut(&panel) else {
            return;
        };
        match result {
            Ok(()) => {
                if let Some(editor) = pane.ready_mut() {
                    editor.mark_saved_at(version);
                }
            }
            Err(err) => {
                let name = document_title(&pane.path);
                tracing::warn!(%err, "save failed");
                self.notice = Some(format!("{SAVE_FAILED_PREFIX} {name}: {err}"));
            }
        }
        cx.notify();
    }

    pub(super) fn editor_pointer_handler(&self, panel: PanelId, cx: &gpui::Context<Self>) -> PointerHandler {
        let shell = cx.entity();
        Rc::new(move |event, window, cx| {
            shell.update(cx, |shell, cx| shell.handle_editor_pointer(panel, event, window, cx));
            window.refresh();
        })
    }

    fn handle_editor_pointer(&mut self, panel: PanelId, event: PointerEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            PointerEvent::Down { offset, click_count, shift, alt } => {
                self.press_in_editor(panel, offset, down_click_mode(click_count, shift, alt), window, cx);
            }
            PointerEvent::Drag { offset } => self.click_in_editor(panel, offset, ClickMode::Extend, cx),
            PointerEvent::Up => {}
            PointerEvent::Scroll { dx: _, dy: _ } => {}
        }
    }

    fn press_in_editor(
        &mut self,
        panel: PanelId,
        offset: usize,
        mode: ClickMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_pane(panel);
        window.focus(&self.focus_handle);
        self.click_in_editor(panel, offset, mode, cx);
    }

    fn click_in_editor(&mut self, panel: PanelId, offset: usize, mode: ClickMode, cx: &mut Context<Self>) {
        let Some((focused, pane)) = self.focused_editor_mut() else {
            return;
        };
        if focused != panel {
            return;
        }
        let Some(editor) = pane.ready_mut() else {
            return;
        };
        editor.click(offset, mode);
        cx.notify();
    }
}

fn down_click_mode(click_count: usize, shift: bool, alt: bool) -> ClickMode {
    if click_count == 1 {
        if shift {
            ClickMode::Extend
        } else if alt {
            ClickMode::AddCursor
        } else {
            ClickMode::Place
        }
    } else if click_count == 2 {
        ClickMode::Word
    } else {
        ClickMode::Line
    }
}
