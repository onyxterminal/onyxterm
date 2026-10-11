use super::{FieldTarget, Shell, DOWN_KEY, ENTER_KEY, ESCAPE_KEY, UP_KEY};
use crate::settings::{filtered_fonts, CUSTOM_BASE_KEY, CUSTOM_COLORS_KEY, DIM_UNFOCUSED_KEY,
    FONT_KEY, FONT_SIZE_KEY, THEME_KEY, UI_SCALE_KEY};
use crate::terminal_view::TerminalStyle;
use crate::theme::custom::{hex_color_field, normalized_hex, ColorField};
use crate::theme::menu_chrome::field_text;
use crate::theme::{hex_to_hsla, Theme, ThemeKind};
use gpui::{div, prelude::*, px, uniform_list, Context, Div, Keystroke, MouseButton, ScrollStrategy, SharedString,
    Stateful, Window};
use std::ops::Range;

const FONT_ROWS: usize = 9;
const FONT_ROW_PX: f32 = 34.0;
const CARD_WINDOW_MARGIN_PX: f32 = 24.0;
const SYSTEM_MONOSPACE: &str = "ui-monospace";
const SYSTEM_MONOSPACE_LABEL: &str = "System monospace";
const FONT_SEARCH_PLACEHOLDER: &str = "Search installed fonts\u{2026}";
const HEX_PLACEHOLDER: &str = "#RRGGBB";

#[derive(Clone, Copy)]
enum SettingAction { ThemePicker, Customize, FontPicker, FontSmaller, FontLarger, UiSmaller, UiLarger, DimPanes, Close }

impl Shell {
    fn change_setting(&mut self, action: SettingAction, cx: &mut Context<Self>) {
        self.state.settings_error = None;
        if matches!(action, SettingAction::ThemePicker) {
            self.state.theme_picker_visible = true;
            self.state.theme_picker_selection = ThemeKind::ALL.iter()
                .position(|kind| *kind == self.state.settings.theme).unwrap_or(0);
            cx.notify();
            return;
        }
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
            self.state.settings_visible = false;
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
            SettingAction::ThemePicker | SettingAction::Customize | SettingAction::FontPicker | SettingAction::Close => unreachable!(),
        };
        self.apply_settings(cx);
        self.save_setting(key, &value);
        cx.notify();
    }

    fn apply_settings(&mut self, cx: &mut Context<Self>) {
        self.state.theme = self.state.settings.theme_tokens();
        let family = self.state.theme.font.resolve_family(&cx.text_system().all_font_names());
        self.style = TerminalStyle::from_theme(&self.state.theme, family);
    }

    fn save_setting(&mut self, key: &str, value: &str) {
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

    fn select_theme(&mut self, kind: ThemeKind, cx: &mut Context<Self>) {
        self.state.settings.theme = kind;
        self.state.theme_picker_visible = false;
        self.state.settings_error = None;
        self.apply_settings(cx);
        self.save_setting(THEME_KEY, kind.key());
        cx.notify();
    }

    fn start_customize(&mut self, cx: &mut Context<Self>) {
        let was_custom = self.state.settings.theme == ThemeKind::Custom;
        self.state.settings.customize_current_theme();
        self.state.theme_picker_visible = false;
        self.state.theme_editor_visible = true;
        self.state.theme_editor_terminal_colors = false;
        self.state.theme_color_field = None;
        self.state.settings_error = None;
        if !was_custom {
            self.save_setting(CUSTOM_BASE_KEY, self.state.settings.custom_base.key());
            self.save_custom_colors();
            self.save_setting(THEME_KEY, ThemeKind::Custom.key());
            self.apply_settings(cx);
        }
        cx.notify();
    }

    fn save_custom_colors(&mut self) {
        match serde_json::to_string(&self.state.settings.custom_colors) {
            Ok(value) => self.save_setting(CUSTOM_COLORS_KEY, &value),
            Err(err) => self.state.settings_error = Some(format!("Could not encode colors: {err}")),
        }
    }

    fn edit_custom_color(&mut self, field: ColorField, cx: &mut Context<Self>) {
        self.state.theme_color_input = hex_color_field(self.state.settings.custom_colors.get(field));
        self.state.theme_color_field = Some(field);
        self.state.settings_error = None;
        cx.notify();
    }

    fn commit_custom_color(&mut self, cx: &mut Context<Self>) {
        let Some(field) = self.state.theme_color_field else { return; };
        match self.state.settings.set_custom_color(field, &normalized_hex(self.state.theme_color_input.text())) {
            Ok(()) => {
                self.state.theme_color_field = None;
                self.state.settings_error = None;
                self.apply_settings(cx);
                self.save_custom_colors();
            }
            Err(message) => self.state.settings_error = Some(message.into()),
        }
    }

    pub(super) fn settings_field(&self) -> Option<FieldTarget> {
        if self.state.theme_editor_visible {
            return self.state.theme_color_field.map(|_| FieldTarget::HexColor);
        }
        if self.state.theme_picker_visible {
            return None;
        }
        self.state.font_picker_visible.then_some(FieldTarget::FontQuery)
    }

    pub(super) fn handle_settings_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        if self.state.theme_editor_visible {
            return self.handle_theme_editor_key(key, cx);
        }
        if self.state.theme_picker_visible {
            self.handle_theme_picker_key(key, cx);
            return true;
        }
        if self.state.font_picker_visible {
            return self.handle_font_picker_key(key, cx);
        }
        if key.key == ESCAPE_KEY {
            self.state.settings_visible = false;
        }
        true
    }

    fn handle_theme_picker_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) {
        match key.key.as_str() {
            ESCAPE_KEY => self.state.theme_picker_visible = false,
            UP_KEY => self.state.theme_picker_selection =
                (self.state.theme_picker_selection + ThemeKind::ALL.len() - 1) % ThemeKind::ALL.len(),
            DOWN_KEY => self.state.theme_picker_selection =
                (self.state.theme_picker_selection + 1) % ThemeKind::ALL.len(),
            ENTER_KEY => self.select_theme(ThemeKind::ALL[self.state.theme_picker_selection], cx),
            _ => {}
        }
    }

    fn handle_theme_editor_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        if self.state.theme_color_field.is_none() {
            if key.key == ESCAPE_KEY { self.state.theme_editor_visible = false; }
            return true;
        }
        match key.key.as_str() {
            ESCAPE_KEY => {
                self.state.theme_color_field = None;
                self.state.settings_error = None;
            }
            ENTER_KEY => self.commit_custom_color(cx),
            _ => return self.type_into_field(FieldTarget::HexColor, key, cx),
        }
        true
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

    fn settings_button(&self, label: &'static str, action: SettingAction, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        div().id(label).h(px(30.0 * scale)).min_w(px(35.0 * scale))
            .px(px(9.0 * scale)).flex().items_center().justify_center()
            .rounded(px(6.0 * scale)).bg(theme.chrome.control_fill)
            .cursor_pointer().child(label)
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation(); shell.change_setting(action, cx);
            }))
    }

    pub(super) fn render_settings(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let (card_id, card) = if self.state.theme_editor_visible { ("theme-editor-card", self.render_theme_editor(cx)) }
            else if self.state.theme_picker_visible { ("theme-picker-card", self.render_theme_picker(cx)) }
            else if self.state.font_picker_visible { ("font-picker-card", self.render_font_picker(cx)) }
            else { ("settings-card", self.render_settings_card(cx)) };
        let max_height = window.viewport_size().height - px(CARD_WINDOW_MARGIN_PX * 2.0);
        div().absolute().inset_0().flex().items_center().justify_center()
            .bg(theme.chrome.backdrop).occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                shell.state.settings_visible = false;
                shell.state.font_picker_visible = false;
                shell.state.theme_picker_visible = false;
                shell.state.theme_editor_visible = false;
                cx.notify();
            }))
            .child(card.id(card_id).max_h(max_height).overflow_y_scroll()
                .w(px(520.0 * scale)).p(px(22.0 * scale))
                .rounded(px(12.0 * scale)).bg(theme.surface.raised)
                .border_1().border_color(theme.border.default).occlude()
                .font_family(SharedString::from(theme.ui_font.family.clone()))
                .text_size(px(theme.ui_font.dialog_body_px))
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| cx.stop_propagation()))
    }

    fn render_settings_card(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let settings = &self.state.settings;
        let scale = settings.ui_scale;
        let row = |title: &'static str, value: String, controls: Vec<Stateful<Div>>| {
            div().h(px(52.0 * scale)).flex().items_center().justify_between()
                .border_b_1().border_color(theme.border.muted)
                .child(div().text_color(theme.text.default).child(title))
                .child(div().flex().items_center().gap(px(8.0 * scale))
                    .child(div().text_color(theme.text.muted).child(value))
                    .children(controls))
        };
        div().child(div().flex().items_center().justify_between()
                .mb(px(14.0 * scale))
                .child(div().text_size(px(theme.ui_font.palette_input_px)).child("Settings"))
                .child(self.settings_button("Done", SettingAction::Close, cx)))
            .child(row("Theme", settings.theme.label().into(), vec![
                self.settings_button("Choose theme", SettingAction::ThemePicker, cx),
                self.settings_button("Customize", SettingAction::Customize, cx)]))
            .child(row("Terminal font", if settings.font_family == SYSTEM_MONOSPACE { SYSTEM_MONOSPACE_LABEL.into() }
                else { settings.font_family.clone() }, vec![self.settings_button("Choose", SettingAction::FontPicker, cx)]))
            .child(row("Terminal text size", format!("{} px", settings.font_size as u32), vec![
                self.settings_button("−", SettingAction::FontSmaller, cx),
                self.settings_button("+", SettingAction::FontLarger, cx)]))
            .child(row("UI size", format!("{}%", (scale * 100.0).round() as u32), vec![
                self.settings_button("− UI", SettingAction::UiSmaller, cx),
                self.settings_button("+ UI", SettingAction::UiLarger, cx)]))
            .child(row("Dim unfocused panes", if settings.dim_unfocused_panes { "On" } else { "Off" }.into(),
                vec![self.settings_button("Toggle", SettingAction::DimPanes, cx)]))
            .child(div().pt(px(12.0 * scale)).text_color(theme.text.subtle)
                .child(self.state.settings_error.clone().unwrap_or_else(||
                    "Changes apply immediately and are saved automatically.".into())))
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
                .child(label)
                .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                    cx.stop_propagation(); shell.select_font(name.clone(), cx);
                }))
        }).collect()
    }

    fn theme_preview(&self, kind: ThemeKind) -> Theme {
        if kind == ThemeKind::Custom {
            let mut theme = Theme::load(self.state.settings.custom_base).expect("built-in theme");
            self.state.settings.custom_colors.apply(&mut theme);
            theme
        } else {
            Theme::load(kind).expect("built-in theme")
        }
    }

    fn render_theme_picker(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        div().child(div().flex().items_center().justify_between().mb(px(14.0 * scale))
                .child(div().text_size(px(theme.ui_font.palette_input_px)).child("Choose theme"))
                .child(div().id("theme-back").cursor_pointer().child("Back")
                    .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                        cx.stop_propagation(); shell.state.theme_picker_visible = false; cx.notify();
                    }))))
            .children(ThemeKind::ALL.into_iter().enumerate().map(|(index, kind)| {
                let preview = self.theme_preview(kind);
                let is_selected = index == self.state.theme_picker_selection;
                let is_current = kind == self.state.settings.theme;
                div().id(("theme-option", index)).h(px(43.0 * scale))
                    .px(px(10.0 * scale)).flex().items_center().gap(px(11.0 * scale))
                    .rounded(px(6.0 * scale)).cursor_pointer()
                    .bg(if is_selected { theme.chrome.control_fill } else { theme.surface.raised })
                    .child(div().w(px(30.0 * scale)).h(px(22.0 * scale))
                        .rounded(px(5.0 * scale)).bg(preview.surface.canvas)
                        .border_1().border_color(preview.border.default)
                        .child(div().ml(px(17.0 * scale)).mt(px(8.0 * scale))
                            .w(px(8.0 * scale)).h(px(8.0 * scale))
                            .rounded_full().bg(preview.accent.primary)))
                    .child(div().flex_1().child(kind.label()))
                    .when(is_current, |el| el.child(div().text_color(theme.accent.primary).child("Selected")))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                        cx.stop_propagation(); shell.select_theme(kind, cx);
                    }))
            }))
            .child(div().pt(px(12.0 * scale)).text_color(theme.text.subtle)
                .child("Choose a preset, then use Customize to make it your own."))
    }

    fn render_theme_editor(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let fields: &[ColorField] = if self.state.theme_editor_terminal_colors {
            &ColorField::TERMINAL
        } else {
            &ColorField::APPEARANCE
        };
        div().child(div().flex().items_center().justify_between().mb(px(10.0 * scale))
                .child(div().text_size(px(theme.ui_font.palette_input_px)).child("Custom theme"))
                .child(div().id("custom-done").cursor_pointer().child("Done")
                    .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                        cx.stop_propagation();
                        shell.state.theme_color_field = None;
                        shell.state.theme_editor_visible = false;
                        cx.notify();
                    }))))
            .child(div().flex().gap(px(12.0 * scale)).pb(px(10.0 * scale))
                .child(self.theme_editor_tab("Appearance", false, cx))
                .child(self.theme_editor_tab("Terminal colors", true, cx)))
            .children(fields.iter().enumerate().map(|(index, field)| {
                let field = *field;
                let saved = self.state.settings.custom_colors.get(field);
                let color = hex_to_hsla(saved, "custom swatch").expect("validated custom color");
                let editing = self.state.theme_color_field == Some(field);
                let label = if editing { field_text(&self.state.theme_color_input, HEX_PLACEHOLDER, theme) }
                    else { div().child(saved.to_string()) };
                div().id(("custom-color", index)).h(px(42.0 * scale))
                    .flex().items_center().justify_between()
                    .border_b_1().border_color(theme.border.muted).cursor_pointer()
                    .child(div().child(field.label()))
                    .child(div().flex().items_center().gap(px(10.0 * scale))
                        .child(div().text_color(theme.text.muted).child(label))
                        .child(div().w(px(24.0 * scale)).h(px(22.0 * scale))
                            .rounded(px(4.0 * scale)).bg(color)
                            .border_1().border_color(theme.border.default)))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                        cx.stop_propagation(); shell.edit_custom_color(field, cx);
                    }))
            }))
            .child(div().pt(px(12.0 * scale)).text_color(theme.text.subtle)
                .child(self.state.settings_error.clone().unwrap_or_else(||
                    if self.state.theme_color_field.is_some() {
                        "Type a color like #RRGGBB, then press Enter. Escape cancels.".into()
                    } else { "Click a color to edit it. Changes are saved automatically.".into() })))
    }

    fn theme_editor_tab(&self, label: &'static str, terminal: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let active = self.state.theme_editor_terminal_colors == terminal;
        div().id(label).h(px(28.0 * scale)).px(px(10.0 * scale))
            .flex().items_center().rounded(px(5.0 * scale)).cursor_pointer()
            .bg(if active { theme.accent.primary } else { theme.chrome.control_fill })
            .text_color(if active { theme.text.inverse } else { theme.text.default })
            .child(label)
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation();
                shell.state.theme_editor_terminal_colors = terminal;
                shell.state.theme_color_field = None;
                shell.state.settings_error = None;
                cx.notify();
            }))
    }
}
