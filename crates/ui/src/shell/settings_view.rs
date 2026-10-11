use super::{Shell, FONT_KEY, FONT_SIZE_KEY, THEME_KEY, UI_SCALE_KEY};
use crate::terminal_view::TerminalStyle;
use gpui::{div, prelude::*, px, Context, Div, MouseButton, SharedString, Stateful};

#[derive(Clone, Copy)]
enum SettingAction { Theme, FontEdit, FontNext, FontSmaller, FontLarger, UiSmaller, UiLarger, Close }

impl Shell {
    fn change_setting(&mut self, action: SettingAction, cx: &mut Context<Self>) {
        self.state.settings_error = None;
        if !matches!(action, SettingAction::FontEdit) { self.state.settings_font_field = None; }
        let settings = &mut self.state.settings;
        let (key, value) = match action {
            SettingAction::Theme => {
                settings.theme = settings.theme.next();
                (THEME_KEY, settings.theme.key().to_string())
            }
            SettingAction::FontEdit => {
                self.state.settings_font_field = Some(settings.font_family.clone());
                cx.notify();
                return;
            }
            SettingAction::FontNext => {
                settings.cycle_font(&cx.text_system().all_font_names());
                (FONT_KEY, settings.font_family.clone())
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
            SettingAction::Close => { self.state.settings_visible = false; cx.notify(); return; }
        };
        self.state.theme = settings.theme_tokens();
        let family = self.state.theme.font.resolve_family(&cx.text_system().all_font_names());
        self.style = TerminalStyle::from_theme(&self.state.theme, family);
        if let Err(err) = self.store.set_setting(key, &value) {
            self.state.settings_error = Some(format!("Could not save setting: {err}"));
        }
        cx.notify();
    }

    pub(super) fn commit_font_field(&mut self, cx: &mut Context<Self>) {
        let Some(typed) = self.state.settings_font_field.take() else { return; };
        let typed = typed.trim();
        if typed.is_empty() { return; }
        let installed = cx.text_system().all_font_names();
        let Some(family) = installed.iter().find(|name| name.eq_ignore_ascii_case(typed)) else {
            self.state.settings_font_field = Some(typed.into());
            self.state.settings_error = Some(format!("Font not installed: {typed}"));
            return;
        };
        self.state.settings_error = None;
        self.state.settings.font_family = family.clone();
        self.state.theme = self.state.settings.theme_tokens();
        self.style = TerminalStyle::from_theme(&self.state.theme, family.clone());
        if let Err(err) = self.store.set_setting(FONT_KEY, family) {
            self.state.settings_error = Some(format!("Could not save setting: {err}"));
        }
    }

    pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let settings = &self.state.settings;
        let scale = settings.ui_scale;
        let button = |label: &'static str, action: SettingAction| {
            div().id(label).h(px(30.0 * scale)).min_w(px(35.0 * scale))
                .px(px(9.0 * scale)).flex().items_center().justify_center()
                .rounded(px(6.0 * scale)).bg(theme.chrome.control_fill)
                .cursor_pointer().child(label)
                .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                    cx.stop_propagation(); shell.change_setting(action, cx);
                }))
        };
        let row = |title: &'static str, value: String, controls: Vec<Stateful<Div>>| {
            div().h(px(52.0 * scale)).flex().items_center().justify_between()
                .border_b_1().border_color(theme.border.muted)
                .child(div().text_color(theme.text.default).child(title))
                .child(div().flex().items_center().gap(px(8.0 * scale))
                    .child(div().text_color(theme.text.muted).child(value))
                    .children(controls))
        };
        div().absolute().inset_0().flex().items_center().justify_center()
            .bg(theme.chrome.backdrop).occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                shell.state.settings_visible = false; cx.notify();
            }))
            .child(div().w(px(500.0 * scale)).p(px(22.0 * scale))
                .rounded(px(12.0 * scale)).bg(theme.surface.raised)
                .border_1().border_color(theme.border.default).occlude()
                .font_family(SharedString::from(theme.ui_font.family.clone()))
                .text_size(px(theme.ui_font.dialog_body_px))
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| cx.stop_propagation())
                .child(div().flex().items_center().justify_between()
                    .mb(px(14.0 * scale))
                    .child(div().text_size(px(theme.ui_font.palette_input_px)).child("Settings"))
                    .child(button("Done", SettingAction::Close)))
                .child(row("Theme", settings.theme.label().into(), vec![button("Change", SettingAction::Theme)]))
                .child(row("Terminal font", self.state.settings_font_field.as_ref()
                    .map_or_else(|| settings.font_family.clone(), |text| format!("{text}│")),
                    vec![button("Type font", SettingAction::FontEdit), button("Next", SettingAction::FontNext)]))
                .child(row("Terminal text size", format!("{} px", settings.font_size as u32), vec![
                    button("−", SettingAction::FontSmaller), button("+", SettingAction::FontLarger)]))
                .child(row("UI size", format!("{}%", (scale * 100.0).round() as u32), vec![
                    button("− UI", SettingAction::UiSmaller), button("+ UI", SettingAction::UiLarger)]))
                .child(div().pt(px(12.0 * scale)).text_color(theme.text.subtle)
                    .child(self.state.settings_error.clone().unwrap_or_else(|| {
                        if self.state.settings_font_field.is_some() {
                            "Type an installed font name, then press Enter. Escape cancels.".into()
                        } else { "Changes apply immediately and are saved automatically.".into() }
                    }))))
    }
}
