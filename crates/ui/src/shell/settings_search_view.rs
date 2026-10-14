use super::settings_panes::ROW_INSET_PX;
use super::settings_view::{SettingAction, CAPTION_PX, TITLE_PX};
use super::Shell;
use crate::settings::SettingsPane;
use crate::settings_search::{search_settings, SearchResult, SearchTarget, SettingsRow};
use crate::text_field::TextField;
use gpui::{div, prelude::*, px, Context, Div, FontWeight, MouseButton};

const RESULT_ROW_MIN_HEIGHT_PX: f32 = 44.0;
const RESULT_RADIUS_PX: f32 = 8.0;
const CHEVRON: &str = "\u{203A}";
const NO_RESULTS: &str = "Nothing matches. Try \u{201C}font\u{201D}, \u{201C}split\u{201D}, or \u{201C}theme\u{201D}.";

impl Shell {
    pub(super) fn clear_settings_search(&mut self) {
        self.state.settings_query.clear();
        self.state.settings_result_selection = 0;
    }

    pub(super) fn move_search_selection(&mut self, step: isize) {
        let count = self.settings_results().len();
        if count == 0 {
            return;
        }
        let current = self.state.settings_result_selection.min(count - 1) as isize;
        self.state.settings_result_selection = (current + step).rem_euclid(count as isize) as usize;
    }

    pub(super) fn open_selected_search_result(&mut self, cx: &mut Context<Self>) {
        let selected = self.settings_results().get(self.state.settings_result_selection).map(|result| result.target);
        if let Some(target) = selected {
            self.open_search_result(target, cx);
        }
    }

    fn open_search_result(&mut self, target: SearchTarget, cx: &mut Context<Self>) {
        self.clear_settings_search();
        self.reset_settings_interaction();
        match target {
            SearchTarget::Setting(SettingsRow::CustomizeColors) => {
                self.state.settings_pane = SettingsPane::Appearance;
                self.change_setting(SettingAction::Customize, cx);
            }
            SearchTarget::Setting(row) => {
                self.state.settings_pane = row.pane();
                self.state.settings_highlight = Some(row);
            }
            SearchTarget::Shortcut(command) => {
                self.state.settings_pane = SettingsPane::Keyboard;
                let title = self.state.registry.get(command).map_or("", |found| found.title);
                self.state.shortcut_filter = TextField::new(title);
            }
        }
        cx.notify();
    }

    fn settings_results(&self) -> Vec<SearchResult> {
        search_settings(self.state.settings_query.text(), &self.state.registry)
    }

    pub(super) fn render_settings_content(&self, cx: &mut Context<Self>) -> Div {
        if self.state.settings_query.is_empty() {
            return self.render_settings_pane(cx);
        }
        self.render_search_results(cx)
    }

    fn render_search_results(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let results = self.settings_results();
        if results.is_empty() {
            return div().flex().items_center().justify_center().text_color(theme.text.muted).child(NO_RESULTS);
        }
        let query = self.state.settings_query.text().trim();
        div().flex().flex_col().gap(px(10.0 * scale))
            .child(div().text_size(px(TITLE_PX * scale)).font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text.default).child(format!("Results for \u{201C}{query}\u{201D}")))
            .child(self.settings_group(results.into_iter().enumerate()
                .map(|(index, result)| self.search_result_row(index, result, cx))))
    }

    fn search_result_row(&self, index: usize, result: SearchResult, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let is_selected = index == self.state.settings_result_selection;
        let hover_fill = theme.chrome.control_fill;
        let combo = match result.target {
            SearchTarget::Shortcut(command) => self.state.keymap.combo_for(command),
            SearchTarget::Setting(_) => None,
        };
        let target = result.target;
        div().child(div().id(("settings-result", index))
            .min_h(px(RESULT_ROW_MIN_HEIGHT_PX * scale)).px(px(ROW_INSET_PX * scale)).py(px(6.0 * scale))
            .flex().items_center().justify_between().gap(px(12.0 * scale))
            .rounded(px(RESULT_RADIUS_PX * scale)).cursor_pointer()
            .when(is_selected, |row| row.bg(hover_fill))
            .hover(move |style| style.bg(hover_fill))
            .child(div().flex_1().min_w(px(0.0))
                .child(div().text_size(px(TITLE_PX * scale)).text_color(theme.text.default).child(result.label))
                .child(div().mt(px(2.0 * scale)).text_size(px(CAPTION_PX * scale)).text_color(theme.text.muted)
                    .child(result.caption)))
            .child(div().flex_none().flex().items_center().gap(px(10.0 * scale))
                .children(combo.map(|combo| self.keycaps(combo)))
                .child(div().text_color(theme.text.muted).child(CHEVRON)))
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation(); shell.open_search_result(target, cx);
            })))
    }
}
