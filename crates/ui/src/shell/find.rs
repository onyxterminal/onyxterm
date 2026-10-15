use super::{FieldTarget, Shell, ENTER_KEY, ESCAPE_KEY};
use crate::editor_pane::{LineJump, OpenDocument};
use crate::editor_view::FindPaint;
use crate::find_bar::{DocumentVersion, FindBar, FindHits, MATCH_LIMIT};
use crate::terminal_view::TerminalHits;
use crate::theme::icons::Icon;
use crate::theme::menu_chrome::field_text;
use crate::theme::{box_shadows, tabular_ui_font};
use domain::PanelId;
use editor_core::{find_matches, line_len, Rope};
use gpui::{div, prelude::*, px, AnyElement, Context, Div, Keystroke, MouseButton, SharedString, Stateful, Task};
use std::sync::Arc;
use terminal_core::Terminal;

const PLACEHOLDER: &str = "Find";
const BAR_RIGHT_PX: f32 = 10.0;
const BAR_TOP_BELOW_HEADER_PX: f32 = 4.0;
const BAR_TOP_IN_EDITOR_PX: f32 = 62.0;
const BAR_HEIGHT_PX: f32 = 30.0;
const BAR_PADDING_LEFT_PX: f32 = 10.0;
const BAR_PADDING_RIGHT_PX: f32 = 4.0;
const BAR_GAP_PX: f32 = 2.0;
const BAR_RADIUS_PX: f32 = 8.0;
const BAR_BORDER_PX: f32 = 1.0;
const INPUT_WIDTH_PX: f32 = 150.0;
const INPUT_TEXT_PX: f32 = 12.5;
const COUNT_MIN_WIDTH_PX: f32 = 52.0;
const COUNT_PADDING_RIGHT_PX: f32 = 4.0;
const BUTTON_PX: f32 = 22.0;
const BUTTON_RADIUS_PX: f32 = 5.0;
const STEP_ICON_PX: f32 = 11.0;
const CLOSE_ICON_PX: f32 = 9.0;

enum FindSource {
    Editor { rope: Rope, searched: DocumentVersion },
    Terminal(Terminal),
}

impl FindSource {
    fn search(self, query: &str) -> FindHits {
        match self {
            FindSource::Editor { rope, searched } => {
                FindHits::Editor { searched, ranges: Arc::new(find_matches(&rope, query, MATCH_LIMIT)) }
            }
            FindSource::Terminal(terminal) => FindHits::Terminal(Arc::new(terminal.find_matches(query, MATCH_LIMIT))),
        }
    }
}

pub(super) struct FindSearch {
    _task: Task<()>,
    is_stale: bool,
}

impl Shell {
    pub(super) fn open_find(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.model.tabs().active().map(|tab| tab.focused()) else {
            return;
        };
        if self.find_source(panel).is_none() {
            return;
        }
        match self.find.as_mut() {
            Some(find) => find.reopen(panel),
            None => self.find = Some(FindBar::new(panel)),
        }
        self.restart_find_search(cx);
    }

    pub(super) fn close_find(&mut self, cx: &mut Context<Self>) {
        self.find = None;
        self.find_search = None;
        cx.notify();
    }

    pub(super) fn is_find_focused(&self) -> bool {
        self.find.as_ref().is_some_and(|find| find.is_focused)
    }

    pub(super) fn blur_find(&mut self) {
        if let Some(find) = self.find.as_mut() {
            find.is_focused = false;
        }
    }

    pub(super) fn handle_find_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> Option<bool> {
        let modifiers = keystroke.modifiers;
        if modifiers.platform || modifiers.control {
            return None;
        }
        match keystroke.key.as_str() {
            ESCAPE_KEY => self.close_find(cx),
            ENTER_KEY => self.step_find(if modifiers.shift { -1 } else { 1 }, cx),
            _ => return Some(self.type_into_field(FieldTarget::Find, keystroke, cx)),
        }
        Some(true)
    }

    pub(super) fn on_find_query_edited(&mut self, cx: &mut Context<Self>) {
        if let Some(find) = self.find.as_mut() {
            find.current = 0;
        }
        self.restart_find_search(cx);
    }

    pub(super) fn refresh_find_for(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        if self.find.as_ref().is_some_and(|find| find.panel == panel) {
            self.request_find_search(cx);
        }
    }

    pub(super) fn sync_find(&mut self, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_ref() else {
            return;
        };
        let panel = find.panel;
        let Some(source) = self.find_source(panel) else {
            self.find = None;
            self.find_search = None;
            return;
        };
        let searched = find.hits.searched_document();
        let is_outdated = match source {
            FindSource::Editor { searched: current, .. } => searched != Some(current),
            FindSource::Terminal(_) => false,
        };
        if is_outdated && !find.input.is_empty() {
            self.request_find_search(cx);
        }
    }

    fn step_find(&mut self, step: isize, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_mut() else {
            return;
        };
        find.step(step);
        let panel = find.panel;
        let range = find.current_range();
        let terminal_match = find.current_match();
        if let (Some(range), Some(document)) = (range, self.active_document_mut(panel)) {
            reveal_range(document, range.start);
        }
        if let (Some(found), Some(pane)) = (terminal_match, self.panes.get(&panel)) {
            pane.terminal.handle.terminal().reveal_line(found.start.line);
        }
        cx.notify();
    }

    fn active_document_mut(&mut self, panel: PanelId) -> Option<&mut OpenDocument> {
        self.editor_pane_mut(panel).map(|pane| pane.active_mut())
    }

    fn find_source(&self, panel: PanelId) -> Option<FindSource> {
        if let Some(pane) = self.panes.get(&panel) {
            return Some(FindSource::Terminal(pane.terminal.handle.terminal().clone()));
        }
        let document = self.editor_pane(panel)?.active();
        let buffer = document.ready()?.buffer();
        let searched = DocumentVersion { document: document.id, version: buffer.version() };
        Some(FindSource::Editor { rope: buffer.rope().clone(), searched })
    }

    fn request_find_search(&mut self, cx: &mut Context<Self>) {
        match self.find_search.as_mut() {
            Some(search) => search.is_stale = true,
            None => self.start_find_search(cx),
        }
    }

    fn restart_find_search(&mut self, cx: &mut Context<Self>) {
        self.find_search = None;
        self.start_find_search(cx);
    }

    fn start_find_search(&mut self, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let query = find.input.text().to_string();
        let panel = find.panel;
        if query.is_empty() {
            find.accept(FindHits::None);
            return;
        }
        let Some(source) = self.find_source(panel) else {
            return;
        };
        let search = cx.background_executor().spawn(async move { source.search(&query) });
        let task = cx.spawn(async move |shell, cx| {
            let hits = search.await;
            let updated = shell.update(cx, |shell, cx| shell.finish_find_search(panel, hits, cx));
            if let Err(err) = updated {
                tracing::debug!(?err, "shell dropped before find finished");
            }
        });
        self.find_search = Some(FindSearch { _task: task, is_stale: false });
    }

    fn finish_find_search(&mut self, panel: PanelId, hits: FindHits, cx: &mut Context<Self>) {
        let is_stale = self.find_search.take().is_some_and(|search| search.is_stale);
        if let Some(find) = self.find.as_mut().filter(|find| find.panel == panel) {
            find.accept(hits);
        }
        if is_stale {
            self.start_find_search(cx);
        }
        cx.notify();
    }

    pub(super) fn editor_find_paint(&self, panel: PanelId, document: &OpenDocument) -> Option<FindPaint> {
        let find = self.find.as_ref().filter(|find| find.panel == panel)?;
        let FindHits::Editor { searched, ranges } = &find.hits else {
            return None;
        };
        let is_current_document = searched.document == document.id;
        is_current_document.then(|| FindPaint { ranges: ranges.clone(), current: Some(find.current) })
    }

    pub(super) fn terminal_hits(&self, panel: PanelId) -> TerminalHits {
        let Some(find) = self.find.as_ref().filter(|find| find.panel == panel) else {
            return TerminalHits::default();
        };
        match &find.hits {
            FindHits::Terminal(matches) => TerminalHits { matches: matches.clone(), current: Some(find.current) },
            FindHits::None | FindHits::Editor { .. } => TerminalHits::default(),
        }
    }

    pub(super) fn render_find_bar(&self, panel: PanelId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let find = self.find.as_ref().filter(|find| find.panel == panel)?;
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let top = match self.editor_pane(panel) {
            Some(_) => BAR_TOP_IN_EDITOR_PX * scale,
            None => theme.layout.pane_header_height_px + BAR_TOP_BELOW_HEADER_PX * scale,
        };
        let input = match find.is_focused {
            true => field_text(&find.input, PLACEHOLDER, theme),
            false => self.resting_find_text(find),
        };
        let bar = div()
            .id(SharedString::from(format!("find-bar-{panel}")))
            .absolute()
            .top(px(top))
            .right(px(BAR_RIGHT_PX * scale))
            .h(px(BAR_HEIGHT_PX * scale))
            .pl(px(BAR_PADDING_LEFT_PX * scale))
            .pr(px(BAR_PADDING_RIGHT_PX * scale))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(BAR_GAP_PX * scale))
            .rounded(px(BAR_RADIUS_PX * scale))
            .bg(theme.chrome.menu_fill)
            .border(px(BAR_BORDER_PX))
            .border_color(theme.border.default)
            .shadow(box_shadows(&theme.shadow.find))
            .font_family(SharedString::from(theme.ui_font.family.clone()))
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event, window, cx| {
                    cx.stop_propagation();
                    shell.focus_pane(panel);
                    window.focus(&shell.focus_handle);
                    if let Some(find) = shell.find.as_mut() {
                        find.is_focused = true;
                    }
                    cx.notify();
                }),
            )
            .child(
                div()
                    .w(px(INPUT_WIDTH_PX * scale))
                    .flex_none()
                    .overflow_hidden()
                    .text_size(px(INPUT_TEXT_PX * scale))
                    .child(input),
            )
            .child(
                div()
                    .min_w(px(COUNT_MIN_WIDTH_PX * scale))
                    .flex_none()
                    .pr(px(COUNT_PADDING_RIGHT_PX * scale))
                    .flex()
                    .justify_end()
                    .whitespace_nowrap()
                    .font(tabular_ui_font(theme))
                    .text_size(px(theme.ui_font.hint_px))
                    .text_color(theme.text.subtle)
                    .child(find.count_label()),
            )
            .child(self.find_button(FindButton::Previous, panel, cx))
            .child(self.find_button(FindButton::Next, panel, cx))
            .child(self.find_button(FindButton::Close, panel, cx));
        Some(bar.into_any_element())
    }

    fn resting_find_text(&self, find: &FindBar) -> Div {
        let theme = &self.state.theme;
        let (text, color) = match find.input.is_empty() {
            true => (PLACEHOLDER.to_string(), theme.text.subtle),
            false => (find.input.text().to_string(), theme.text.default),
        };
        div().whitespace_nowrap().text_color(color).child(text)
    }

    fn find_button(&self, button: FindButton, panel: PanelId, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let (icon, size) = match button {
            FindButton::Previous => (Icon::ChevronUp, STEP_ICON_PX),
            FindButton::Next => (Icon::Chevron, STEP_ICON_PX),
            FindButton::Close => (Icon::CloseFind, CLOSE_ICON_PX),
        };
        div()
            .id(SharedString::from(format!("find-{button:?}-{panel}")))
            .size(px(BUTTON_PX * scale))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(BUTTON_RADIUS_PX * scale))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event, _window, cx| {
                    cx.stop_propagation();
                    match button {
                        FindButton::Previous => shell.step_find(-1, cx),
                        FindButton::Next => shell.step_find(1, cx),
                        FindButton::Close => shell.close_find(cx),
                    }
                }),
            )
            .child(icon.sized(size * scale, theme.text.muted))
    }
}

#[derive(Debug, Clone, Copy)]
enum FindButton {
    Previous,
    Next,
    Close,
}

fn reveal_range(document: &mut OpenDocument, start: usize) {
    let Some(editor) = document.ready() else {
        return;
    };
    let rope = editor.buffer().rope();
    let line = rope.char_to_line(start.min(rope.len_chars()));
    let column = (start - rope.line_to_char(line)).min(line_len(rope, line));
    document.jump_to(LineJump { line, column });
}
