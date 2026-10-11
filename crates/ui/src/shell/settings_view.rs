use super::{Shell, BACKSPACE_KEY, DOWN_KEY, ENTER_KEY, ESCAPE_KEY, UP_KEY};
use crate::settings::{filtered_fonts, CUSTOM_BASE_KEY, CUSTOM_COLORS_KEY, DIM_UNFOCUSED_KEY,
    FONT_KEY, FONT_SIZE_KEY, THEME_KEY, UI_SCALE_KEY};
use crate::terminal_view::TerminalStyle;
use crate::theme::custom::ColorField;
use crate::theme::{hex_to_hsla, Theme, ThemeKind};
use gpui::{div, prelude::*, px, Context, Div, Keystroke, MouseButton, SharedString, Stateful};

const FONT_ROWS: usize = 9;

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
        self.state.theme_color_input = "#".into();
        self.state.theme_color_field = Some(field);
        self.state.settings_error = None;
        cx.notify();
    }

    fn commit_custom_color(&mut self, cx: &mut Context<Self>) {
        let Some(field) = self.state.theme_color_field else { return; };
        match self.state.settings.set_custom_color(field, &self.state.theme_color_input) {
            Ok(()) => {
                self.state.theme_color_field = None;
                self.state.settings_error = None;
                self.apply_settings(cx);
                self.save_custom_colors();
            }
            Err(message) => self.state.settings_error = Some(message.into()),
        }
    }

    pub(super) fn handle_theme_picker_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) {
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

    pub(super) fn handle_theme_editor_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) {
        if self.state.theme_color_field.is_none() {
            if key.key == ESCAPE_KEY { self.state.theme_editor_visible = false; }
            return;
        }
        match key.key.as_str() {
            ESCAPE_KEY => {
                self.state.theme_color_field = None;
                self.state.settings_error = None;
            }
            ENTER_KEY => self.commit_custom_color(cx),
            BACKSPACE_KEY => { self.state.theme_color_input.pop(); }
            _ if !key.modifiers.control && !key.modifiers.platform => {
                if let Some(text) = key.key_char.as_deref() {
                    if self.state.theme_color_input.len() < 7 {
                        self.state.theme_color_input.push_str(text);
                    }
                }
            }
            _ => {}
        }
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
        let card = if self.state.theme_editor_visible { self.render_theme_editor(cx) }
            else if self.state.theme_picker_visible { self.render_theme_picker(cx) }
            else if self.state.font_picker_visible { self.render_font_picker(cx) }
            else { self.render_settings_card(cx) };
        div().absolute().inset_0().flex().items_center().justify_center()
            .bg(theme.chrome.backdrop).occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                shell.state.settings_visible = false;
                shell.state.font_picker_visible = false;
                shell.state.theme_picker_visible = false;
                shell.state.theme_editor_visible = false;
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
            .child(row("Theme", settings.theme.label().into(), vec![
                self.settings_button("Choose theme", SettingAction::ThemePicker, cx),
                self.settings_button("Customize", SettingAction::Customize, cx)]))
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
                let label = if editing { format!("{}│", self.state.theme_color_input) } else { saved.into() };
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
                        "Type six hex digits after #, then press Enter. Escape cancels.".into()
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
