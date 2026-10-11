use super::{Shell, BACKSPACE_KEY, DOWN_KEY, ENTER_KEY, ESCAPE_KEY, UP_KEY};
use crate::settings::{filtered_fonts, DIM_UNFOCUSED_KEY, FONT_KEY, FONT_SIZE_KEY, THEME_KEY, UI_SCALE_KEY};
use crate::terminal_view::TerminalStyle;
use gpui::{div, prelude::*, px, Context, Div, Keystroke, MouseButton, SharedString, Stateful};

const FONT_ROWS: usize = 9;

#[derive(Clone, Copy)]
enum SettingAction { Theme, FontPicker, FontSmaller, FontLarger, UiSmaller, UiLarger, DimPanes, Close }

impl Shell {
    fn change_setting(&mut self, action: SettingAction, cx: &mut Context<Self>) {
        self.state.settings_error = None;
        if matches!(action, SettingAction::FontPicker) {
            self.state.font_picker_visible = true;
            self.state.font_picker_query.clear();
            let fonts = self.font_picker_matches(cx);
            self.state.font_picker_selection = fonts.iter()
                .position(|name| name == &self.state.settings.font_family).unwrap_or(0);
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
            SettingAction::Theme => {
                settings.theme = settings.theme.next();
                (THEME_KEY, settings.theme.key().to_string())
            }
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
            SettingAction::FontPicker | SettingAction::Close => unreachable!(),
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
        let query = self.state.font_picker_query.trim().to_lowercase();
        let mut fonts = filtered_fonts(&cx.text_system().all_font_names(), &query);
        if "system monospace".contains(&query) {
            fonts.insert(0, "ui-monospace".into());
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

    pub(super) fn handle_font_picker_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) {
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
                    let step = if key.key == UP_KEY { count - 1 } else { 1 };
                    self.state.font_picker_selection = (self.state.font_picker_selection + step) % count;
                }
            }
            BACKSPACE_KEY => {
                self.state.font_picker_query.pop();
                self.state.font_picker_selection = 0;
            }
            _ if !key.modifiers.control && !key.modifiers.platform => {
                if let Some(text) = key.key_char.as_deref() {
                    self.state.font_picker_query.push_str(text);
                    self.state.font_picker_selection = 0;
                }
            }
            _ => {}
        }
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

    pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let card = if self.state.font_picker_visible { self.render_font_picker(cx) } else { self.render_settings_card(cx) };
        div().absolute().inset_0().flex().items_center().justify_center()
            .bg(theme.chrome.backdrop).occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                shell.state.settings_visible = false;
                shell.state.font_picker_visible = false;
                cx.notify();
            }))
            .child(card.w(px(520.0 * scale)).p(px(22.0 * scale))
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
            .child(row("Theme", settings.theme.label().into(), vec![self.settings_button("Change", SettingAction::Theme, cx)]))
            .child(row("Terminal font", if settings.font_family == "ui-monospace" { "System monospace".into() }
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
        let range = crate::palette::visible_rows(fonts.len(), self.state.font_picker_selection, FONT_ROWS);
        div().child(div().flex().items_center().justify_between().mb(px(14.0 * scale))
                .child(div().text_size(px(theme.ui_font.palette_input_px)).child("Choose terminal font"))
                .child(div().id("font-back").cursor_pointer().child("Back")
                    .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                        cx.stop_propagation(); shell.state.font_picker_visible = false; cx.notify();
                    }))))
            .child(div().h(px(38.0 * scale)).px(px(10.0 * scale)).flex().items_center()
                .rounded(px(6.0 * scale)).bg(theme.surface.sunken)
                .text_color(theme.text.default)
                .child(if self.state.font_picker_query.is_empty() {
                    "Search installed fonts…".into()
                } else { format!("{}│", self.state.font_picker_query) }))
            .child(div().mt(px(10.0 * scale)).flex().flex_col()
                .children(fonts[range.clone()].iter().enumerate().map(|(offset, name)| {
                    let selected = range.start + offset == self.state.font_picker_selection;
                    let family = name.clone();
                    div().id(("font-option", range.start + offset))
                        .h(px(34.0 * scale)).px(px(10.0 * scale)).flex().items_center()
                        .rounded(px(5.0 * scale)).cursor_pointer()
                        .bg(if selected { theme.accent.primary } else { theme.surface.raised })
                        .text_color(if selected { theme.text.inverse } else { theme.text.default })
                        .child(if name == "ui-monospace" { "System monospace".into() } else { name.clone() })
                        .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                            cx.stop_propagation(); shell.select_font(family.clone(), cx);
                        }))
                })))
            .when(fonts.is_empty(), |el| el.child(div().py(px(16.0 * scale))
                .text_color(theme.text.muted).child("No matching fonts")))
            .child(div().pt(px(12.0 * scale)).text_color(theme.text.subtle)
                .child(format!("{} matching fonts  ·  Type to search, ↑↓ to move, Enter to select", fonts.len())))
    }
}
