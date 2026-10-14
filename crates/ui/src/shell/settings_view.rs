use super::theme_editor::{ButtonTone, EditorButton};
use super::{FieldTarget, Shell, DOWN_KEY, ENTER_KEY, ESCAPE_KEY, UP_KEY};
use crate::settings::{filtered_fonts, step_bundle_size, BundleSide, SettingsPane, CUSTOM_BASE_KEY, CUSTOM_COLORS_KEY,
    DIM_UNFOCUSED_KEY, EDITOR_BUNDLE_SIDE_KEY, EDITOR_BUNDLE_SIZE_KEY, FONT_KEY, FONT_SIZE_KEY, THEME_KEY,
    UI_SCALE_KEY};
use crate::terminal_view::TerminalStyle;
use crate::theme::icons::Icon;
use crate::theme::menu_chrome::field_text;
use crate::theme::{box_shadows, Theme, ThemeKind};
use gpui::{div, prelude::*, px, uniform_list, Context, CursorStyle, Div, FontWeight, Keystroke, MouseButton,
    ScrollStrategy, SharedString, Stateful, Window};
use std::ops::Range;

const FONT_ROWS: usize = 9;
const FONT_ROW_PX: f32 = 34.0;
const CARD_WINDOW_MARGIN_PX: f32 = 24.0;
const CARD_WIDTH_PX: f32 = 520.0;
const THEME_EDITOR_WIDTH_PX: f32 = 600.0;
pub(super) const SYSTEM_MONOSPACE: &str = "ui-monospace";
pub(super) const SYSTEM_MONOSPACE_LABEL: &str = "System monospace";
const FONT_SEARCH_PLACEHOLDER: &str = "Search installed fonts\u{2026}";
const SETTINGS_WIDTH_PX: f32 = 800.0;
const SETTINGS_HEIGHT_PX: f32 = 640.0;
const CONTENT_PADDING_X_PX: f32 = 28.0;
const CONTENT_PADDING_Y_PX: f32 = 22.0;
const FOOTER_HEIGHT_PX: f32 = 44.0;
const TOOLBAR_BUTTON_WIDTH_PX: f32 = 76.0;
const TOOLBAR_ICON_PX: f32 = 22.0;
const TOOLBAR_LABEL_PX: f32 = 11.0;
const SEARCH_FIELD_WIDTH_PX: f32 = 180.0;
const SEARCH_FIELD_HEIGHT_PX: f32 = 26.0;
const SEARCH_ICON_PX: f32 = 12.0;
pub(super) const TITLE_PX: f32 = 13.0;
pub(super) const CAPTION_PX: f32 = 11.5;

#[derive(Clone, Copy)]
pub(super) enum SettingAction {
    Customize, FontPicker, FontSmaller, FontLarger, UiSmaller, UiLarger, DimPanes, EditorSmaller, EditorLarger,
    EditorSide(BundleSide), Close,
}

impl Shell {
    pub(super) fn change_setting(&mut self, action: SettingAction, cx: &mut Context<Self>) {
        self.state.settings_error = None;
        if matches!(action, SettingAction::Customize) {
            self.start_customize(cx);
            return;
        }
        if matches!(action, SettingAction::FontPicker) {
            self.state.font_picker_visible = true;
            self.state.font_picker_query.clear();
            let fonts = self.font_picker_matches(cx);
            self.state.font_picker_selection = fonts.iter()
                .position(|name| name == &self.state.settings.font_family).unwrap_or(0);
            self.font_list_scroll.scroll_to_item(self.state.font_picker_selection, ScrollStrategy::Center);
            cx.notify();
            return;
        }
        if matches!(action, SettingAction::Close) {
            self.close_settings();
            cx.notify();
            return;
        }
        let settings = &mut self.state.settings;
        let (key, value) = match action {
            SettingAction::FontSmaller => {
                settings.font_size = (settings.font_size - 1.0).max(10.0);
                (FONT_SIZE_KEY, settings.font_size.to_string())
            }
            SettingAction::FontLarger => {
                settings.font_size = (settings.font_size + 1.0).min(28.0);
                (FONT_SIZE_KEY, settings.font_size.to_string())
            }
            SettingAction::UiSmaller => {
                settings.ui_scale = ((settings.ui_scale * 100.0).round() - 10.0).max(75.0) / 100.0;
                (UI_SCALE_KEY, settings.ui_scale.to_string())
            }
            SettingAction::UiLarger => {
                settings.ui_scale = ((settings.ui_scale * 100.0).round() + 10.0).min(150.0) / 100.0;
                (UI_SCALE_KEY, settings.ui_scale.to_string())
            }
            SettingAction::DimPanes => {
                settings.dim_unfocused_panes = !settings.dim_unfocused_panes;
                (DIM_UNFOCUSED_KEY, settings.dim_unfocused_panes.to_string())
            }
            SettingAction::EditorSmaller => {
                settings.editor_bundle_size = step_bundle_size(settings.editor_bundle_size, -1.0);
                (EDITOR_BUNDLE_SIZE_KEY, settings.editor_bundle_size.to_string())
            }
            SettingAction::EditorLarger => {
                settings.editor_bundle_size = step_bundle_size(settings.editor_bundle_size, 1.0);
                (EDITOR_BUNDLE_SIZE_KEY, settings.editor_bundle_size.to_string())
            }
            SettingAction::EditorSide(side) => {
                settings.editor_bundle_side = side;
                (EDITOR_BUNDLE_SIDE_KEY, side.key().to_string())
            }
            SettingAction::Customize | SettingAction::FontPicker | SettingAction::Close => unreachable!(),
        };
        self.apply_settings(cx);
        self.save_setting(key, &value);
        cx.notify();
    }

    fn restore_pane_defaults(&mut self, cx: &mut Context<Self>) {
        self.state.settings_error = None;
        for (key, value) in self.state.settings_pane.default_pairs() {
            self.state.settings.apply_pair(key, &value);
            self.save_setting(key, &value);
        }
        self.state.shortcut_replaced = None;
        self.apply_settings(cx);
        self.apply_shortcuts(cx);
    }

    pub(super) fn show_settings_pane(&mut self, pane: SettingsPane, cx: &mut Context<Self>) {
        self.state.settings_pane = pane;
        self.reset_settings_interaction();
        cx.notify();
    }

    pub(super) fn close_settings(&mut self) {
        self.state.settings_visible = false;
        self.state.shortcut_replaced = None;
        self.state.shortcut_filter.clear();
        self.reset_settings_interaction();
    }

    fn reset_settings_interaction(&mut self) {
        self.state.shortcut_recording = None;
        self.settings_focus = None;
    }

    pub(super) fn focus_settings_field(&mut self, target: FieldTarget, cx: &mut Context<Self>) {
        self.settings_focus = Some(target);
        cx.notify();
    }

    pub(super) fn apply_settings(&mut self, cx: &mut Context<Self>) {
        self.state.theme = self.state.settings.theme_tokens();
        let family = self.state.theme.font.resolve_family(&cx.text_system().all_font_names());
        self.style = TerminalStyle::from_theme(&self.state.theme, family);
    }

    pub(super) fn save_setting(&mut self, key: &str, value: &str) {
        if let Err(err) = self.store.set_setting(key, value) {
            self.state.settings_error = Some(format!("Could not save setting: {err}"));
        }
    }

    fn font_picker_matches(&self, cx: &Context<Self>) -> Vec<String> {
        let query = self.state.font_picker_query.text().trim().to_lowercase();
        let mut fonts = filtered_fonts(&cx.text_system().all_font_names(), &query);
        if SYSTEM_MONOSPACE_LABEL.to_lowercase().contains(&query) {
            fonts.insert(0, SYSTEM_MONOSPACE.into());
        }
        fonts
    }

    fn select_font(&mut self, family: String, cx: &mut Context<Self>) {
        self.state.settings.font_family = family.clone();
        self.state.font_picker_visible = false;
        self.state.settings_error = None;
        self.apply_settings(cx);
        self.save_setting(FONT_KEY, &family);
        cx.notify();
    }

    pub(super) fn select_theme(&mut self, kind: ThemeKind, cx: &mut Context<Self>) {
        self.state.settings.theme = kind;
        self.state.settings_error = None;
        self.apply_settings(cx);
        self.save_setting(THEME_KEY, kind.key());
        cx.notify();
    }

    fn start_customize(&mut self, cx: &mut Context<Self>) {
        let was_custom = self.state.settings.theme == ThemeKind::Custom;
        self.state.settings.customize_current_theme();
        self.state.theme_editor_visible = true;
        self.state.theme_editor_terminal_colors = false;
        self.state.color_picker = None;
        self.state.settings_error = None;
        if !was_custom {
            self.save_setting(CUSTOM_BASE_KEY, self.state.settings.custom_base.key());
            self.save_custom_colors();
            self.save_setting(THEME_KEY, ThemeKind::Custom.key());
            self.apply_settings(cx);
        }
        cx.notify();
    }

    pub(super) fn save_custom_colors(&mut self) {
        match serde_json::to_string(&self.state.settings.custom_colors) {
            Ok(value) => self.save_setting(CUSTOM_COLORS_KEY, &value),
            Err(err) => self.state.settings_error = Some(format!("Could not encode colors: {err}")),
        }
    }

    pub(super) fn settings_field(&self) -> Option<FieldTarget> {
        if self.state.theme_editor_visible {
            return self.state.color_picker.as_ref().map(|_| FieldTarget::HexColor);
        }
        if self.state.font_picker_visible {
            return Some(FieldTarget::FontQuery);
        }
        self.settings_focus
    }

    pub(super) fn handle_settings_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        if self.state.theme_editor_visible {
            return self.handle_theme_editor_key(key, cx);
        }
        if self.state.font_picker_visible {
            return self.handle_font_picker_key(key, cx);
        }
        if let Some(command) = self.state.shortcut_recording {
            self.record_shortcut_key(command, key, cx);
            return true;
        }
        if key.key == ESCAPE_KEY {
            self.close_settings();
            return true;
        }
        match self.settings_focus {
            Some(target) => self.type_into_field(target, key, cx),
            None => true,
        }
    }

    fn handle_font_picker_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        match key.key.as_str() {
            ESCAPE_KEY => self.state.font_picker_visible = false,
            ENTER_KEY => {
                let fonts = self.font_picker_matches(cx);
                if let Some(family) = fonts.get(self.state.font_picker_selection).cloned() {
                    self.select_font(family, cx);
                }
            }
            UP_KEY | DOWN_KEY => {
                let count = self.font_picker_matches(cx).len();
                if count > 0 {
                    let (step, strategy) = if key.key == UP_KEY {
                        (count - 1, ScrollStrategy::Top)
                    } else { (1, ScrollStrategy::Bottom) };
                    self.state.font_picker_selection = (self.state.font_picker_selection + step) % count;
                    self.font_list_scroll.scroll_to_item(self.state.font_picker_selection, strategy);
                }
            }
            _ => return self.type_into_field(FieldTarget::FontQuery, key, cx),
        }
        true
    }

    pub(super) fn settings_button(&self, label: &'static str, action: SettingAction, cx: &mut Context<Self>)
        -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        div().id(label).h(px(30.0 * scale)).min_w(px(35.0 * scale))
            .px(px(9.0 * scale)).flex().items_center().justify_center()
            .rounded(px(6.0 * scale)).bg(theme.chrome.control_fill)
            .cursor_pointer().hover(|style| style.bg(theme.chrome.icon_hover_fill)).child(label)
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation(); shell.change_setting(action, cx);
            }))
    }

    pub(super) fn render_settings(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let viewport = window.viewport_size();
        let card = if self.state.theme_editor_visible {
            self.render_settings_subcard(self.render_theme_editor(cx), "theme-editor-card", THEME_EDITOR_WIDTH_PX)
        } else if self.state.font_picker_visible {
            self.render_settings_subcard(self.render_font_picker(cx), "font-picker-card", CARD_WIDTH_PX)
        } else {
            self.render_settings_window(cx).id("settings-card")
                .w(px(SETTINGS_WIDTH_PX * scale)).h(px(SETTINGS_HEIGHT_PX * scale)).overflow_hidden()
        };
        let picker_drag_listener = self.picker_drag_listener(cx);
        div().absolute().inset_0().flex().items_center().justify_center()
            .bg(theme.chrome.backdrop).occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                shell.finish_color_picker(cx);
                shell.close_settings();
                shell.state.font_picker_visible = false;
                shell.state.theme_editor_visible = false;
                cx.notify();
            }))
            .child(card.max_h(viewport.height - px(CARD_WINDOW_MARGIN_PX * 2.0))
                .max_w(viewport.width - px(CARD_WINDOW_MARGIN_PX * 2.0))
                .rounded(px(16.0 * scale)).bg(theme.surface.raised)
                .shadow(box_shadows(&theme.shadow.palette))
                .border_1().border_color(theme.border.default).occlude()
                .font_family(SharedString::from(theme.ui_font.family.clone()))
                .text_size(px(theme.ui_font.dialog_body_px))
                .capture_any_mouse_down(cx.listener(|shell, _event, _window, _cx| shell.reset_settings_interaction()))
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| cx.stop_propagation()))
            .children(picker_drag_listener)
    }

    fn render_settings_subcard(&self, content: Div, id: &'static str, width_px: f32) -> Stateful<Div> {
        let scale = self.state.settings.ui_scale;
        content.id(id).overflow_y_scroll().w(px(width_px * scale)).p(px(24.0 * scale))
    }

    fn render_settings_window(&self, cx: &mut Context<Self>) -> Div {
        let scale = self.state.settings.ui_scale;
        div().flex().flex_col()
            .child(self.render_settings_toolbar(cx))
            .child(self.render_settings_pane(cx).id("settings-content").flex_1().min_h(px(0.0)).overflow_y_scroll()
                .px(px(CONTENT_PADDING_X_PX * scale)).py(px(CONTENT_PADDING_Y_PX * scale)))
            .child(self.render_settings_footer(cx))
    }

    fn render_settings_toolbar(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        div().flex_none().relative().pt(px(12.0 * scale)).pb(px(8.0 * scale))
            .flex().flex_col().items_center().gap(px(8.0 * scale))
            .border_b_1().border_color(theme.border.muted)
            .child(div().text_size(px(TITLE_PX * scale)).font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text.default).child(self.state.settings_pane.label()))
            .child(div().flex().gap(px(4.0 * scale))
                .children(SettingsPane::ALL.map(|pane| self.settings_toolbar_button(pane, cx))))
            .child(div().absolute().top(px(10.0 * scale)).right(px(12.0 * scale))
                .child(self.settings_button("Done", SettingAction::Close, cx)))
    }

    fn settings_toolbar_button(&self, pane: SettingsPane, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let is_selected = self.state.settings_pane == pane;
        let hover_fill = theme.chrome.control_fill;
        div().id(("settings-pane", pane as usize)).w(px(TOOLBAR_BUTTON_WIDTH_PX * scale)).py(px(5.0 * scale))
            .flex().flex_col().items_center().gap(px(3.0 * scale))
            .rounded(px(7.0 * scale)).cursor_pointer()
            .text_size(px(TOOLBAR_LABEL_PX * scale))
            .text_color(if is_selected { theme.accent.primary } else { theme.text.muted })
            .when(is_selected, |button| button.bg(hover_fill))
            .hover(move |style| style.bg(hover_fill))
            .child(pane_icon(pane).sized(TOOLBAR_ICON_PX * scale))
            .child(pane.label())
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation(); shell.show_settings_pane(pane, cx);
            }))
    }

    fn render_settings_footer(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let has_defaults = !self.state.settings_pane.setting_keys().is_empty();
        let restore = EditorButton {
            id: "restore-defaults", label: "Restore Defaults".into(), tone: ButtonTone::Quiet,
        };
        div().flex_none().h(px(FOOTER_HEIGHT_PX * scale)).px(px(16.0 * scale))
            .flex().items_center().justify_between()
            .border_t_1().border_color(theme.border.muted)
            .child(div().text_size(px(CAPTION_PX * scale))
                .text_color(if self.state.settings_error.is_some() { theme.chrome.destructive }
                    else { theme.text.muted })
                .child(self.state.settings_error.clone().unwrap_or_else(|| "Saved automatically".into())))
            .when(has_defaults, |footer| footer
                .child(self.editor_button(restore, |shell, cx| shell.restore_pane_defaults(cx), cx)))
    }

    pub(super) fn settings_search_field(&self, target: FieldTarget, placeholder: &'static str, cx: &mut Context<Self>)
        -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let is_focused = self.settings_focus == Some(target);
        let text = match self.field(target) {
            Some(field) if is_focused => field_text(field, placeholder, theme),
            Some(field) if !field.is_empty() => {
                div().truncate().text_color(theme.text.default).child(field.text().to_string())
            }
            _ => div().truncate().text_color(theme.text.subtle).child(placeholder),
        };
        div().id(("settings-search-field", target as usize)).flex_none()
            .w(px(SEARCH_FIELD_WIDTH_PX * scale)).h(px(SEARCH_FIELD_HEIGHT_PX * scale)).px(px(8.0 * scale))
            .flex().items_center().gap(px(6.0 * scale))
            .rounded(px(7.0 * scale)).bg(theme.chrome.control_fill).border_1()
            .border_color(if is_focused { theme.accent.primary } else { gpui::transparent_black() })
            .cursor(CursorStyle::IBeam).text_size(px(CAPTION_PX * scale))
            .child(Icon::Magnifier.sized(SEARCH_ICON_PX * scale))
            .child(div().flex_1().min_w(px(0.0)).overflow_hidden().child(text))
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation(); shell.focus_settings_field(target, cx);
            }))
    }

    pub(super) fn setting_stepper(&self, id: &'static str, value: String, smaller: SettingAction, larger: SettingAction, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let button = |suffix: &'static str, glyph: &'static str, action, cx: &mut Context<Self>| {
            div().id(SharedString::from(format!("{id}-{suffix}")))
                .size(px(30.0 * scale)).flex().items_center().justify_center()
                .rounded(px(6.0 * scale)).cursor_pointer()
                .hover(|style| style.bg(theme.chrome.icon_hover_fill))
                .child(glyph)
                .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                    cx.stop_propagation(); shell.change_setting(action, cx);
                }))
        };
        div().flex_none().flex().items_center().rounded(px(8.0 * scale))
            .bg(theme.chrome.control_fill).p(px(2.0 * scale))
            .child(button("less", "−", smaller, cx))
            .child(div().min_w(px(60.0 * scale)).flex().justify_center()
                .text_color(theme.text.default).child(value))
            .child(button("more", "+", larger, cx))
    }

    pub(super) fn setting_switch(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let enabled = self.state.settings.dim_unfocused_panes;
        div().id("dim-panes-switch").flex_none()
            .w(px(36.0 * scale)).h(px(22.0 * scale)).p(px(3.0 * scale))
            .flex().items_center().rounded_full().cursor_pointer()
            .bg(if enabled { theme.accent.primary } else { theme.chrome.icon_hover_fill })
            .when(enabled, |el| el.justify_end())
            .tooltip(self.tooltip_builder(if enabled { "Dim inactive panes: On" } else { "Dim inactive panes: Off" }.into()))
            .child(div().size(px(16.0 * scale)).rounded_full().bg(gpui::white()))
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                cx.stop_propagation(); shell.change_setting(SettingAction::DimPanes, cx);
            }))
    }

    fn render_font_picker(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let fonts = self.font_picker_matches(cx);
        let font_count = fonts.len();
        let list_height = px(FONT_ROW_PX * scale * font_count.min(FONT_ROWS) as f32);
        let font_rows = cx.processor(move |shell, range: Range<usize>, _window, cx| shell.font_rows(&fonts, range, cx));
        div().child(div().flex().items_center().justify_between().mb(px(14.0 * scale))
                .child(div().text_size(px(theme.ui_font.palette_input_px)).child("Choose terminal font"))
                .child(div().id("font-back").cursor_pointer().child("Back")
                    .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                        cx.stop_propagation(); shell.state.font_picker_visible = false; cx.notify();
                    }))))
            .child(div().h(px(38.0 * scale)).px(px(10.0 * scale)).flex().items_center()
                .rounded(px(6.0 * scale)).bg(theme.surface.sunken)
                .child(field_text(&self.state.font_picker_query, FONT_SEARCH_PLACEHOLDER, theme)))
            .child(uniform_list("font-options", font_count, font_rows)
                .track_scroll(self.font_list_scroll.clone())
                .mt(px(10.0 * scale)).h(list_height))
            .when(font_count == 0, |el| el.child(div().py(px(16.0 * scale))
                .text_color(theme.text.muted).child("No matching fonts")))
            .child(div().pt(px(12.0 * scale)).text_color(theme.text.subtle)
                .child(format!("{font_count} matching fonts  ·  Type to search, ↑↓ to move, Enter to select")))
    }

    fn font_rows(&self, fonts: &[String], range: Range<usize>, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        range.filter_map(|index| fonts.get(index).map(|name| (index, name.clone()))).map(|(index, name)| {
            let selected = index == self.state.font_picker_selection;
            let label = if name == SYSTEM_MONOSPACE { SYSTEM_MONOSPACE_LABEL.into() } else { name.clone() };
            div().id(("font-option", index))
                .h(px(FONT_ROW_PX * scale)).px(px(10.0 * scale)).flex().items_center()
                .rounded(px(5.0 * scale)).cursor_pointer()
                .bg(if selected { theme.accent.primary } else { theme.surface.raised })
                .text_color(if selected { theme.text.inverse } else { theme.text.default })
                .when(name != SYSTEM_MONOSPACE, |row| row.font_family(SharedString::from(name.clone())))
                .child(label)
                .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                    cx.stop_propagation(); shell.select_font(name.clone(), cx);
                }))
        }).collect()
    }

    pub(super) fn theme_preview(&self, kind: ThemeKind) -> Theme {
        if kind == ThemeKind::Custom {
            let mut theme = Theme::load(self.state.settings.custom_base).expect("built-in theme");
            self.state.settings.custom_colors.apply(&mut theme);
            theme
        } else {
            Theme::load(kind).expect("built-in theme")
        }
    }
}

fn pane_icon(pane: SettingsPane) -> Icon {
    match pane {
        SettingsPane::Appearance => Icon::Appearance,
        SettingsPane::Terminal => Icon::Terminal,
        SettingsPane::Editor => Icon::Editor,
        SettingsPane::Keyboard => Icon::Keyboard,
    }
}
