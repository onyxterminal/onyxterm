use super::{write_to, Shell};
use crate::command::CommandId;
use crate::input::is_composed_text;
use crate::terminal_view::cursor_bounds;
use crate::text_field::{Clipboard, TextField};
use editor_core::EditorAction;
use gpui::{
    App, Bounds, ClipboardItem, Context, EntityInputHandler, Keystroke, Pixels, Point, ScrollStrategy, UTF16Selection,
    Window,
};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldTarget {
    HexColor,
    FontQuery,
    MenuName,
    MenuSearch,
    PaneName,
    TabName,
    PaletteQuery,
    ShortcutFilter,
    SettingsQuery,
}

struct AppClipboard<'a>(&'a mut App);

impl Clipboard for AppClipboard<'_> {
    fn read_text(&mut self) -> Option<String> {
        self.0.read_from_clipboard().and_then(|item| item.text())
    }

    fn write_text(&mut self, text: String) {
        self.0.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

impl Shell {
    fn marked_len_utf16(&self) -> usize {
        self.marked_text.as_deref().map_or(0, |text| text.encode_utf16().count())
    }

    pub(super) fn active_field(&self) -> Option<FieldTarget> {
        if self.pending.is_some() {
            return None;
        }
        if self.state.settings_visible {
            return self.settings_field();
        }
        if self.is_workspace_menu_open {
            return self
                .menu_field
                .as_ref()
                .map(|_| FieldTarget::MenuName)
                .or(Some(FieldTarget::MenuSearch));
        }
        if self.is_pane_menu_open() {
            return None;
        }
        if self.pane_field.is_some() {
            return Some(FieldTarget::PaneName);
        }
        if self.tab_field.is_some() {
            return Some(FieldTarget::TabName);
        }
        self.state.palette_visible.then_some(FieldTarget::PaletteQuery)
    }

    pub(super) fn field(&self, target: FieldTarget) -> Option<&TextField> {
        match target {
            FieldTarget::HexColor => Some(&self.state.theme_color_input),
            FieldTarget::FontQuery => Some(&self.state.font_picker_query),
            FieldTarget::MenuSearch => Some(&self.menu_search),
            FieldTarget::MenuName => self.menu_field.as_ref().map(|field| &field.input),
            FieldTarget::PaneName => self.pane_field.as_ref().map(|field| &field.input),
            FieldTarget::TabName => self.tab_field.as_ref().map(|field| &field.input),
            FieldTarget::PaletteQuery => Some(&self.state.palette_query),
            FieldTarget::ShortcutFilter => Some(&self.state.shortcut_filter),
            FieldTarget::SettingsQuery => Some(&self.state.settings_query),
        }
    }

    fn field_mut(&mut self, target: FieldTarget) -> Option<&mut TextField> {
        match target {
            FieldTarget::HexColor => Some(&mut self.state.theme_color_input),
            FieldTarget::FontQuery => Some(&mut self.state.font_picker_query),
            FieldTarget::MenuSearch => Some(&mut self.menu_search),
            FieldTarget::MenuName => self.menu_field.as_mut().map(|field| &mut field.input),
            FieldTarget::PaneName => self.pane_field.as_mut().map(|field| &mut field.input),
            FieldTarget::TabName => self.tab_field.as_mut().map(|field| &mut field.input),
            FieldTarget::PaletteQuery => Some(&mut self.state.palette_query),
            FieldTarget::ShortcutFilter => Some(&mut self.state.shortcut_filter),
            FieldTarget::SettingsQuery => Some(&mut self.state.settings_query),
        }
    }

    fn active_text_field(&self) -> Option<&TextField> {
        self.field(self.active_field()?)
    }

    fn with_field<R>(
        &mut self,
        target: FieldTarget,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut TextField, &mut dyn Clipboard) -> R,
    ) -> Option<R> {
        let mut clipboard = AppClipboard(cx);
        let field = self.field_mut(target)?;
        let before = field.text().to_string();
        let result = edit(field, &mut clipboard);
        if field.text() != before {
            self.on_field_edited(target, cx);
        }
        cx.notify();
        Some(result)
    }

    fn with_active_field<R>(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut TextField, &mut dyn Clipboard) -> R,
    ) -> Option<R> {
        let target = self.active_field()?;
        self.with_field(target, cx, edit)
    }

    fn on_field_edited(&mut self, target: FieldTarget, cx: &mut Context<Self>) {
        match target {
            FieldTarget::FontQuery => {
                self.state.font_picker_selection = 0;
                self.font_list_scroll.scroll_to_item(0, ScrollStrategy::Top);
            }
            FieldTarget::PaletteQuery => {
                self.state.palette_selection = 0;
                self.scroll_palette_to_top();
            }
            FieldTarget::HexColor => self.preview_typed_hex(cx),
            FieldTarget::SettingsQuery => self.state.settings_result_selection = 0,
            FieldTarget::MenuName | FieldTarget::MenuSearch | FieldTarget::PaneName | FieldTarget::TabName
            | FieldTarget::ShortcutFilter => {}
        }
    }

    pub(super) fn edit_field(&mut self, target: FieldTarget, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        self.with_field(target, cx, |field, clipboard| field.handle_key(keystroke, clipboard)).unwrap_or(false)
    }

    pub(super) fn type_into_field(
        &mut self,
        target: FieldTarget,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        if is_composed_text(keystroke, false) {
            return false;
        }
        self.edit_field(target, keystroke, cx);
        true
    }

    pub(super) fn run_field_command(&mut self, id: CommandId, cx: &mut Context<Self>) {
        self.with_active_field(cx, |field, clipboard| match id {
            CommandId::Copy => field.copy(clipboard),
            CommandId::Paste => field.paste(clipboard),
            CommandId::SelectAll => field.select_all(),
            _ => {}
        });
    }
}

impl EntityInputHandler for Shell {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let (text, actual) = self.active_text_field()?.text_for_utf16(range);
        *adjusted_range = Some(actual);
        Some(text)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if let Some(field) = self.active_text_field() {
            return Some(field.selected_utf16());
        }
        let end = self.marked_len_utf16();
        Some(UTF16Selection { range: end..end, reversed: false })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        if let Some(field) = self.active_text_field() {
            return field.marked_utf16();
        }
        self.marked_text.as_ref().map(|_| 0..self.marked_len_utf16())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.with_active_field(cx, |field, _clipboard| field.unmark()).is_some() {
            return;
        }
        self.marked_text = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.with_active_field(cx, |field, _clipboard| field.replace_utf16(range, text)).is_some() {
            return;
        }
        if let Some(panel) = self.model.focused_document() {
            self.marked_text = None;
            if !text.is_empty() {
                self.apply_editor_action(panel, EditorAction::InsertText(text.to_string()), cx);
            }
            cx.notify();
            return;
        }
        let is_typing_into_shell = !text.is_empty() && !self.is_copy_mode_active();
        if let Some(handle) = self.focused_handle().filter(|_| is_typing_into_shell) {
            write_to(handle, text.as_bytes());
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mark = |field: &mut TextField, _clipboard: &mut dyn Clipboard| {
            field.mark_utf16(range, new_text, new_selected_range)
        };
        if self.with_active_field(cx, mark).is_some() {
            return;
        }
        self.marked_text = Some(new_text.to_string()).filter(|text| !text.is_empty());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if let Some(field) = self.active_text_field() {
            return field.caret_bounds().get();
        }
        if let Some(panel) = self.model.focused_document() {
            let editor_pane = self.editor_pane(panel)?;
            return editor_pane.active().view.borrow().layout.as_ref().and_then(|layout| layout.cursor_bounds);
        }
        let cursor = self.focused_handle()?.snapshot_grid().cursor?;
        Some(cursor_bounds(element_bounds, self.style.cell_size(window), cursor))
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
