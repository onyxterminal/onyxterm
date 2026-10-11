use super::{FieldTarget, Shell, ENTER_KEY, ESCAPE_KEY};
use crate::color_picker::ColorPicker;
use crate::theme::custom::{hex_color_field, normalized_hex, ColorField};
use crate::theme::hsv::Rgb;
use crate::theme::{hex_to_hsla, Theme};
use gpui::{div, prelude::*, px, AnyElement, Context, Div, Hsla, Keystroke, MouseButton, SharedString, Stateful};

const INVALID_HEX_MESSAGE: &str = "Use a color like #3B82F6.";
const IDLE_HINT: &str = "Click a color to fine-tune it. Changes save automatically.";
const PICKING_HINT: &str = "Drag to adjust or type a hex value. Enter keeps it, Esc restores the original.";
const COLOR_ROW_PX: f32 = 46.0;
const ROW_SWATCH_PX: f32 = 28.0;
const PREVIEW_DOT_PX: f32 = 9.0;
const PREVIEW_FONT_PX: f32 = 12.0;
const PREVIEW_BLOCK_WIDTH_PX: f32 = 26.0;
const PREVIEW_BLOCK_HEIGHT_PX: f32 = 10.0;
const PREVIEW_CURSOR_WIDTH_PX: f32 = 7.0;
const PREVIEW_CURSOR_HEIGHT_PX: f32 = 14.0;

#[derive(Clone, Copy)]
pub(super) enum ButtonTone {
    Primary,
    Quiet,
}

pub(super) struct EditorButton {
    pub id: &'static str,
    pub label: SharedString,
    pub tone: ButtonTone,
}

fn ansi_palette(theme: &Theme) -> [Hsla; 8] {
    let ansi = theme.ansi;
    [ansi.black, ansi.red, ansi.green, ansi.yellow, ansi.blue, ansi.magenta, ansi.cyan, ansi.white]
}

fn preview_line() -> Div {
    div().flex().flex_row().whitespace_nowrap()
}

fn preview_span(text: &'static str, color: Hsla) -> Div {
    div().text_color(color).child(text)
}

impl Shell {
    pub(super) fn handle_theme_editor_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        if self.state.color_picker.is_none() {
            if key.key == ESCAPE_KEY {
                self.state.theme_editor_visible = false;
            }
            return true;
        }
        match key.key.as_str() {
            ESCAPE_KEY => self.cancel_color_picker(cx),
            ENTER_KEY => self.commit_typed_hex(cx),
            _ => return self.type_into_field(FieldTarget::HexColor, key, cx),
        }
        true
    }

    fn toggle_color_picker(&mut self, field: ColorField, cx: &mut Context<Self>) {
        let is_open = self.state.color_picker.as_ref().is_some_and(|picker| picker.field() == field);
        self.finish_color_picker(cx);
        if is_open {
            return;
        }
        let saved = self.state.settings.custom_colors.get(field);
        self.state.theme_color_input = hex_color_field(saved);
        self.state.color_picker = Some(ColorPicker::open(field, saved));
    }

    pub(super) fn finish_color_picker(&mut self, cx: &mut Context<Self>) {
        self.persist_picker_color();
        self.state.color_picker = None;
        self.state.settings_error = None;
        cx.notify();
    }

    pub(super) fn cancel_color_picker(&mut self, cx: &mut Context<Self>) {
        if let Some(picker) = self.state.color_picker.as_mut() {
            picker.revert();
        }
        self.preview_picker_color(cx);
        self.finish_color_picker(cx);
    }

    fn typed_rgb(&self) -> Option<Rgb> {
        Rgb::from_hex(&normalized_hex(self.state.theme_color_input.text()))
    }

    pub(super) fn preview_typed_hex(&mut self, cx: &mut Context<Self>) {
        let Some(rgb) = self.typed_rgb() else { return };
        let Some(picker) = self.state.color_picker.as_mut() else { return };
        picker.pick(rgb);
        self.state.settings_error = None;
        self.preview_picker_color(cx);
    }

    fn commit_typed_hex(&mut self, cx: &mut Context<Self>) {
        if self.typed_rgb().is_none() {
            self.state.settings_error = Some(INVALID_HEX_MESSAGE.into());
            return;
        }
        self.preview_typed_hex(cx);
        self.finish_color_picker(cx);
    }

    pub(super) fn choose_picker_color(&mut self, rgb: Rgb, cx: &mut Context<Self>) {
        let Some(picker) = self.state.color_picker.as_mut() else { return };
        picker.pick(rgb);
        self.preview_picker_color(cx);
        self.sync_hex_field();
        self.persist_picker_color();
    }

    pub(super) fn preview_picker_color(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = self.state.color_picker.as_ref() else { return };
        let (field, hex) = (picker.field(), picker.hex());
        if self.state.settings.set_custom_color(field, &hex).is_ok() {
            self.apply_settings(cx);
        }
        cx.notify();
    }

    pub(super) fn sync_hex_field(&mut self) {
        if let Some(picker) = self.state.color_picker.as_ref() {
            self.state.theme_color_input = hex_color_field(&picker.hex());
        }
    }

    pub(super) fn persist_picker_color(&mut self) {
        let Some(picker) = self.state.color_picker.as_mut() else { return };
        if picker.unsaved_hex().is_none() {
            return;
        }
        picker.mark_persisted();
        self.save_custom_colors();
    }

    fn reset_custom_colors(&mut self, cx: &mut Context<Self>) {
        self.state.color_picker = None;
        self.state.settings.reset_custom_colors();
        self.state.settings_error = None;
        self.apply_settings(cx);
        self.save_custom_colors();
        cx.notify();
    }

    fn close_theme_editor(&mut self, cx: &mut Context<Self>) {
        self.finish_color_picker(cx);
        self.state.theme_editor_visible = false;
    }

    fn show_color_group(&mut self, is_terminal: bool, cx: &mut Context<Self>) {
        self.finish_color_picker(cx);
        self.state.theme_editor_terminal_colors = is_terminal;
    }

    pub(super) fn editor_button(
        &self,
        button: EditorButton,
        action: impl Fn(&mut Shell, &mut Context<Shell>) + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let (fill, hover, text) = match button.tone {
            ButtonTone::Primary => (theme.accent.primary, theme.accent.primary_hover, theme.text.inverse),
            ButtonTone::Quiet => (theme.chrome.control_fill, theme.chrome.icon_hover_fill, theme.text.default),
        };
        div().id(button.id).flex_none().h(px(28.0 * scale)).px(px(12.0 * scale))
            .flex().items_center().justify_center().rounded(px(6.0 * scale))
            .bg(fill).text_color(text).cursor_pointer()
            .hover(move |style| style.bg(hover))
            .child(button.label)
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation();
                action(shell, cx);
                cx.notify();
            }))
    }

    pub(super) fn render_theme_editor(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let fields: &[ColorField] = match self.state.theme_editor_terminal_colors {
            true => &ColorField::TERMINAL,
            false => &ColorField::APPEARANCE,
        };
        let hint = match self.state.color_picker {
            Some(_) => PICKING_HINT,
            None => IDLE_HINT,
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, field) in fields.iter().copied().enumerate() {
            rows.push(self.color_row(index, field, cx).into_any_element());
            if let Some(picker) = self.state.color_picker.as_ref().filter(|picker| picker.field() == field) {
                rows.push(self.render_color_picker(picker, cx).into_any_element());
            }
        }
        div().flex().flex_col()
            .child(self.theme_editor_header(cx))
            .child(self.theme_preview_terminal())
            .child(div().flex().pb(px(8.0 * scale)).child(div().flex().p(px(2.0 * scale)).gap(px(2.0 * scale))
                .rounded(px(7.0 * scale)).bg(theme.chrome.control_fill)
                .child(self.theme_editor_tab("Appearance", false, cx))
                .child(self.theme_editor_tab("Terminal colors", true, cx))))
            .children(rows)
            .child(div().pt(px(12.0 * scale)).text_color(theme.text.subtle)
                .child(self.state.settings_error.clone().unwrap_or_else(|| hint.into())))
    }

    fn theme_editor_header(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let base = self.state.settings.custom_base.label();
        let reset = EditorButton { id: "reset-custom", label: format!("Reset to {base}").into(), tone: ButtonTone::Quiet };
        let done = EditorButton { id: "custom-done", label: "Done".into(), tone: ButtonTone::Primary };
        div().flex().items_center().justify_between().mb(px(14.0 * scale))
            .child(div().flex().flex_col().gap(px(2.0 * scale))
                .child(div().text_size(px(theme.ui_font.palette_input_px)).child("Custom theme"))
                .child(div().text_color(theme.text.muted).child(format!("Based on {base}"))))
            .child(div().flex().gap(px(8.0 * scale))
                .child(self.editor_button(reset, |shell, cx| shell.reset_custom_colors(cx), cx))
                .child(self.editor_button(done, |shell, cx| shell.close_theme_editor(cx), cx)))
    }

    fn theme_preview_terminal(&self) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let (text, muted, accent, ansi) = (theme.text.default, theme.text.muted, theme.accent.primary, theme.ansi);
        let prompt = || preview_line()
            .child(preview_span("~/onyxterm", ansi.cyan)).child(preview_span(" on ", muted))
            .child(preview_span("main", ansi.magenta)).child(preview_span(" \u{276F} ", accent));
        let dot = |color: Hsla| div().size(px(PREVIEW_DOT_PX * scale)).rounded_full().bg(color);
        let block = |color: Hsla| div().w(px(PREVIEW_BLOCK_WIDTH_PX * scale)).h(px(PREVIEW_BLOCK_HEIGHT_PX * scale))
            .rounded(px(2.0 * scale)).bg(color);
        let cursor = div().w(px(PREVIEW_CURSOR_WIDTH_PX * scale)).h(px(PREVIEW_CURSOR_HEIGHT_PX * scale)).bg(accent);
        div().mb(px(16.0 * scale)).p(px(6.0 * scale)).rounded(px(10.0 * scale))
            .bg(theme.surface.subtle).border_1().border_color(theme.border.default)
            .child(div().h(px(20.0 * scale)).px(px(6.0 * scale)).flex().items_center().gap(px(6.0 * scale))
                .child(dot(ansi.red)).child(dot(ansi.yellow)).child(dot(ansi.green))
                .child(div().flex_1().flex().justify_center().pr(px(36.0 * scale))
                    .text_size(px(theme.ui_font.hint_px)).text_color(muted).child("zsh")))
            .child(div().mt(px(4.0 * scale)).px(px(14.0 * scale)).py(px(12.0 * scale))
                .rounded(px(7.0 * scale)).bg(theme.surface.canvas)
                .font_family(SharedString::from(theme.font.family.clone()))
                .text_size(px(PREVIEW_FONT_PX * scale)).text_color(text)
                .flex().flex_col().gap(px(3.0 * scale))
                .child(prompt().child(preview_span("cargo test --workspace", text)))
                .child(preview_line().child(preview_span("   Compiling", ansi.green)).child(preview_span(" ui v0.4.0", text)))
                .child(preview_line().child(preview_span("warning", ansi.yellow))
                    .child(preview_span(": unused import ", text)).child(preview_span("std::rc::Rc", ansi.blue)))
                .child(preview_line().child(preview_span("error", ansi.red)).child(preview_span(": test ", text))
                    .child(preview_span("hue_wraps", ansi.white)).child(preview_span(" failed", text)))
                .child(preview_span("41 passed, 1 failed in 0.42s", muted))
                .child(prompt().items_center().child(cursor))
                .child(div().mt(px(6.0 * scale)).flex().gap(px(4.0 * scale))
                    .children(ansi_palette(theme).map(block))))
    }

    fn theme_editor_tab(&self, label: &'static str, is_terminal: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let is_active = self.state.theme_editor_terminal_colors == is_terminal;
        div().id(label).h(px(26.0 * scale)).px(px(12.0 * scale))
            .flex().items_center().rounded(px(5.0 * scale)).cursor_pointer()
            .text_color(if is_active { theme.text.default } else { theme.text.muted })
            .when(is_active, |tab| tab.bg(theme.surface.raised).border_1().border_color(theme.border.muted))
            .child(label)
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation();
                shell.show_color_group(is_terminal, cx);
            }))
    }

    fn color_row(&self, index: usize, field: ColorField, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let saved = self.state.settings.custom_colors.get(field);
        let color = hex_to_hsla(saved, "custom swatch").expect("validated custom color");
        let is_selected = self.state.color_picker.as_ref().is_some_and(|picker| picker.field() == field);
        let highlight = theme.chrome.control_fill;
        div().id(("custom-color", index)).h(px(COLOR_ROW_PX * scale)).px(px(8.0 * scale))
            .flex().items_center().gap(px(12.0 * scale))
            .rounded(px(8.0 * scale)).cursor_pointer()
            .when(is_selected, |row| row.bg(highlight))
            .hover(move |style| style.bg(highlight))
            .child(div().flex_none().size(px(ROW_SWATCH_PX * scale)).rounded(px(7.0 * scale)).bg(color)
                .border_1().border_color(theme.border.default))
            .child(div().flex_1().child(field.label()))
            .child(div().font_family(SharedString::from(theme.font.family.clone()))
                .text_color(theme.text.muted).child(saved.to_string()))
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation();
                shell.toggle_color_picker(field, cx);
            }))
    }
}
