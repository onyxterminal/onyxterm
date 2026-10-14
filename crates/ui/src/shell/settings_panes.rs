use super::settings_view::{SettingAction, CAPTION_PX, SYSTEM_MONOSPACE, SYSTEM_MONOSPACE_LABEL, TITLE_PX};
use super::theme_editor::{preview_line, preview_span};
use super::Shell;
use crate::command::CommandId;
use crate::keymap::Combo;
use crate::settings::{BundleSide, SettingsPane};
use crate::theme::{box_shadows, Theme, ThemeKind};
use gpui::{div, prelude::*, px, relative, BoxShadow, Context, Div, FontWeight, Hsla, MouseButton, SharedString,
    Stateful};

const GROUP_RADIUS_PX: f32 = 10.0;
const ROW_MIN_HEIGHT_PX: f32 = 56.0;
pub(super) const ROW_INSET_PX: f32 = 16.0;
pub(super) const SECTION_GAP_PX: f32 = 18.0;
const THEME_COLUMNS: u16 = 4;
const THEME_CARD_HEIGHT_PX: f32 = 92.0;
const THEME_CARD_RADIUS_PX: f32 = 9.0;
const THEME_CARD_LABEL_PX: f32 = 12.5;
const SELECTED_RING_SPREAD_PX: f32 = 1.5;
const TRAFFIC_DOT_PX: f32 = 6.0;
const PREVIEW_BAR_HEIGHT_PX: f32 = 5.0;
const PREVIEW_BAR_WIDTHS: [f32; 3] = [0.45, 0.7, 0.35];
const PREVIEW_TEXT_OPACITY: f32 = 0.55;
const SWATCH_PX: f32 = 8.0;
const DARK_SURFACE_LIGHTNESS: f32 = 0.5;
const FONT_POPUP_MAX_WIDTH_PX: f32 = 180.0;
const LIVE_PREVIEW_MIN_HEIGHT_PX: f32 = 150.0;
const LIVE_PREVIEW_MIN_FONT_PX: f32 = 11.0;
const LIVE_PREVIEW_MAX_FONT_PX: f32 = 20.0;
const CURSOR_WIDTH_EM: f32 = 0.6;
const CURSOR_HEIGHT_EM: f32 = 1.25;
const EDITOR_SIDES: [(BundleSide, &str); 3] =
    [(BundleSide::Left, "Left"), (BundleSide::Right, "Right"), (BundleSide::Bottom, "Bottom")];
const EDITOR_SHORTCUTS: [(CommandId, &str); 3] = [(CommandId::OpenEditorHere, "Edit here"),
    (CommandId::ToggleEditorBundle, "Show or hide editor"), (CommandId::NextDocument, "Next document")];
const DIAGRAM_WIDTH_PX: f32 = 300.0;
const DIAGRAM_HEIGHT_PX: f32 = 180.0;
const DIAGRAM_BLOCK_GAP_PX: f32 = 3.0;
const EDITOR_BLOCK_TINT: f32 = 0.08;
pub(super) const KEYCAP_PX: f32 = 20.0;

fn swatches(theme: &Theme) -> [Hsla; 6] {
    let ansi = theme.ansi;
    [ansi.red, ansi.green, ansi.yellow, ansi.blue, ansi.magenta, ansi.cyan]
}

impl Shell {
    pub(super) fn render_settings_pane(&self, cx: &mut Context<Self>) -> Div {
        match self.state.settings_pane {
            SettingsPane::Appearance => self.render_appearance_pane(cx),
            SettingsPane::Terminal => self.render_terminal_pane(cx),
            SettingsPane::Editor => self.render_editor_pane(cx),
            SettingsPane::Keyboard => self.render_keyboard_pane(cx),
        }
    }

    pub(super) fn settings_group(&self, rows: impl IntoIterator<Item = Div>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let divider = || div().h(px(1.0)).mx(px(ROW_INSET_PX * scale)).bg(theme.border.muted);
        div().flex().flex_col().rounded(px(GROUP_RADIUS_PX * scale))
            .bg(theme.surface.subtle).border_1().border_color(theme.border.muted)
            .children(rows.into_iter().enumerate()
                .map(|(index, row)| div().when(index > 0, |el| el.child(divider())).child(row)))
    }

    fn settings_row(&self, title: &'static str, caption: &'static str, control: impl IntoElement) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        div().min_h(px(ROW_MIN_HEIGHT_PX * scale)).px(px(ROW_INSET_PX * scale)).py(px(10.0 * scale))
            .flex().items_center().justify_between().gap(px(16.0 * scale))
            .child(div().flex_1().min_w(px(0.0))
                .child(div().text_size(px(TITLE_PX * scale)).text_color(theme.text.default).child(title))
                .child(div().mt(px(2.0 * scale)).text_size(px(CAPTION_PX * scale))
                    .text_color(theme.text.muted).child(caption)))
            .child(control)
    }

    fn render_appearance_pane(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        div().flex().flex_col().gap(px(SECTION_GAP_PX * scale))
            .child(div().flex().items_center().justify_between()
                .child(div().text_size(px(TITLE_PX * scale)).font_weight(FontWeight::SEMIBOLD).child("Theme"))
                .child(div().id("customize-colors").cursor_pointer().text_color(theme.accent.primary)
                    .child("Customize Colors\u{2026}")
                    .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                        cx.stop_propagation(); shell.change_setting(SettingAction::Customize, cx);
                    }))))
            .child(div().grid().grid_cols(THEME_COLUMNS).gap(px(14.0 * scale))
                .children(ThemeKind::ALL.map(|kind| self.theme_card(kind, cx))))
            .child(self.settings_group([
                self.settings_row("Interface size", "Tabs, menus, and controls.",
                    self.setting_stepper("ui-size", format!("{}%", (scale * 100.0).round() as u32),
                        SettingAction::UiSmaller, SettingAction::UiLarger, cx)),
                self.settings_row("Dim inactive panes", "Keep your active pane in focus.", self.setting_switch(cx)),
            ]))
    }

    fn theme_card(&self, kind: ThemeKind, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let preview = self.theme_preview(kind);
        let is_selected = kind == self.state.settings.theme;
        let caption: SharedString = match kind {
            ThemeKind::Custom => format!("Based on {}", self.state.settings.custom_base.label()).into(),
            _ if preview.surface.canvas.l < DARK_SURFACE_LIGHTNESS => "Dark".into(),
            _ => "Light".into(),
        };
        let ring = BoxShadow {
            color: theme.accent.primary,
            offset: gpui::point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(SELECTED_RING_SPREAD_PX * scale),
        };
        div().id(("theme-card", kind as usize)).min_w(px(0.0)).cursor_pointer()
            .child(self.theme_card_preview(&preview).border_1()
                .border_color(if is_selected { theme.accent.primary } else { theme.border.muted })
                .when(is_selected, |card| card.shadow(vec![ring])))
            .child(div().mt(px(7.0 * scale)).truncate().text_size(px(THEME_CARD_LABEL_PX * scale))
                .text_color(theme.text.default)
                .when(is_selected, |label| label.font_weight(FontWeight::SEMIBOLD))
                .child(kind.label()))
            .child(div().truncate().text_size(px(CAPTION_PX * scale)).text_color(theme.text.muted).child(caption))
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation(); shell.select_theme(kind, cx);
            }))
    }

    fn theme_card_preview(&self, preview: &Theme) -> Div {
        let scale = self.state.settings.ui_scale;
        let ansi = preview.ansi;
        let dot = |color: Hsla| div().size(px(TRAFFIC_DOT_PX * scale)).rounded_full().bg(color);
        let bar_colors = [preview.accent.primary, preview.text.default.opacity(PREVIEW_TEXT_OPACITY), ansi.green];
        div().h(px(THEME_CARD_HEIGHT_PX * scale)).p(px(10.0 * scale))
            .rounded(px(THEME_CARD_RADIUS_PX * scale)).bg(preview.surface.canvas)
            .flex().flex_col().justify_between()
            .child(div().flex().gap(px(4.0 * scale))
                .child(dot(ansi.red)).child(dot(ansi.yellow)).child(dot(ansi.green)))
            .child(div().flex().flex_col().gap(px(5.0 * scale))
                .children(PREVIEW_BAR_WIDTHS.into_iter().zip(bar_colors).map(|(width, color)| {
                    div().w(relative(width)).h(px(PREVIEW_BAR_HEIGHT_PX * scale)).rounded_full().bg(color)
                })))
            .child(div().flex().gap(px(4.0 * scale))
                .children(swatches(preview).map(|color| div().size(px(SWATCH_PX * scale))
                    .rounded(px(2.0 * scale)).bg(color))))
    }

    fn render_terminal_pane(&self, cx: &mut Context<Self>) -> Div {
        let scale = self.state.settings.ui_scale;
        div().flex().flex_col().gap(px(SECTION_GAP_PX * scale))
            .child(self.terminal_live_preview())
            .child(self.settings_group([
                self.settings_row("Font", "Used in terminals and the editor.", self.font_popup_button(cx)),
                self.settings_row("Text size", "Terminal and editor text.",
                    self.setting_stepper("text-size", format!("{} pt", self.state.settings.font_size as u32),
                        SettingAction::FontSmaller, SettingAction::FontLarger, cx)),
            ]))
    }

    fn terminal_live_preview(&self) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let (text, muted, ansi) = (theme.text.default, theme.text.muted, theme.ansi);
        let font_px = self.state.settings.font_size.clamp(LIVE_PREVIEW_MIN_FONT_PX, LIVE_PREVIEW_MAX_FONT_PX);
        let prompt = || preview_line().items_center()
            .child(preview_span("~/onyxterm", ansi.green)).child(preview_span(" main", ansi.blue))
            .child(preview_span(" \u{276F} ", muted));
        let status = |code: &'static str, color: Hsla, path: &'static str| preview_line()
            .child(preview_span(code, color)).child(preview_span(path, text));
        let cursor = div().w(px(font_px * CURSOR_WIDTH_EM)).h(px(font_px * CURSOR_HEIGHT_EM)).bg(theme.accent.primary);
        div().min_h(px(LIVE_PREVIEW_MIN_HEIGHT_PX * scale)).px(px(18.0 * scale)).py(px(16.0 * scale))
            .rounded(px(GROUP_RADIUS_PX * scale)).bg(theme.surface.canvas)
            .border_1().border_color(theme.border.muted)
            .font_family(self.style.font_family.clone()).text_size(px(font_px)).text_color(text)
            .flex().flex_col().gap(px(3.0 * scale))
            .child(prompt().child(preview_span("git status --short", text)))
            .child(status(" M", ansi.yellow, " crates/ui/src/shell/settings_view.rs"))
            .child(status(" D", ansi.red, " crates/ui/src/legacy_prefs.rs"))
            .child(status("??", ansi.green, " crates/ui/src/editor_pane.rs"))
            .child(prompt().child(cursor))
    }

    fn font_popup_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let family = &self.state.settings.font_family;
        let label = if family == SYSTEM_MONOSPACE { SYSTEM_MONOSPACE_LABEL.to_string() } else { family.clone() };
        div().id("font-popup").flex_none().h(px(28.0 * scale)).pl(px(10.0 * scale)).pr(px(8.0 * scale))
            .flex().items_center().gap(px(8.0 * scale))
            .rounded(px(6.0 * scale)).bg(theme.chrome.control_fill).cursor_pointer()
            .hover(|style| style.bg(theme.chrome.icon_hover_fill))
            .child(div().max_w(px(FONT_POPUP_MAX_WIDTH_PX * scale)).truncate()
                .font_family(self.style.font_family.clone()).text_color(theme.text.default).child(label))
            .child(div().flex().flex_col().text_size(px(8.0 * scale)).line_height(px(7.0 * scale))
                .text_color(theme.text.muted).child("\u{2303}").child("\u{2304}"))
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                cx.stop_propagation(); shell.change_setting(SettingAction::FontPicker, cx);
            }))
    }

    fn render_editor_pane(&self, cx: &mut Context<Self>) -> Div {
        let scale = self.state.settings.ui_scale;
        let size_percent = (self.state.settings.editor_bundle_size * 100.0).round() as u32;
        div().flex().flex_col().gap(px(SECTION_GAP_PX * scale))
            .child(self.settings_group([
                self.settings_row("Editor position", "Where files open beside the active terminal.",
                    self.editor_side_control(cx)),
                self.settings_row("Editor size", "Share of the window the editor takes.",
                    self.setting_stepper("editor-size", format!("{size_percent}%"),
                        SettingAction::EditorSmaller, SettingAction::EditorLarger, cx)),
            ]))
            .child(div().flex().items_start().gap(px(20.0 * scale))
                .child(self.editor_layout_diagram())
                .child(div().flex_1().min_w(px(0.0)).flex().flex_col().gap(px(10.0 * scale))
                    .children(EDITOR_SHORTCUTS.into_iter().filter_map(|(command, label)| {
                        self.state.keymap.combo_for(command).map(|combo| self.shortcut_line(label, combo))
                    }))))
    }

    fn editor_side_control(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let current = self.state.settings.editor_bundle_side;
        div().flex_none().flex().p(px(2.0 * scale)).gap(px(2.0 * scale))
            .rounded(px(7.0 * scale)).bg(theme.chrome.control_fill)
            .children(EDITOR_SIDES.into_iter().enumerate().map(|(index, (side, label))| {
                let is_selected = side == current;
                div().id(("editor-side", index)).h(px(24.0 * scale)).px(px(12.0 * scale))
                    .flex().items_center().rounded(px(5.0 * scale)).cursor_pointer()
                    .text_color(if is_selected { theme.text.default } else { theme.text.muted })
                    .when(is_selected, |segment| segment.bg(theme.surface.raised)
                        .font_weight(FontWeight::SEMIBOLD).shadow(box_shadows(&theme.shadow.tab)))
                    .child(label)
                    .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                        cx.stop_propagation(); shell.change_setting(SettingAction::EditorSide(side), cx);
                    }))
            }))
    }

    fn editor_layout_diagram(&self) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let side = self.state.settings.editor_bundle_side;
        let editor_share = self.state.settings.editor_bundle_size;
        let is_stacked = matches!(side, BundleSide::Top | BundleSide::Bottom);
        let block = |label: &'static str, share: f32, fill: Hsla, outline: Hsla| {
            let slot = div().p(px(DIAGRAM_BLOCK_GAP_PX * scale));
            let slot = if is_stacked { slot.w_full().h(relative(share)) } else { slot.h_full().w(relative(share)) };
            slot.child(div().size_full().rounded(px(6.0 * scale)).bg(fill).border_1().border_color(outline)
                .flex().items_center().justify_center()
                .text_size(px(CAPTION_PX * scale)).text_color(theme.text.muted).child(label))
        };
        let terminal = block("Terminal", 1.0 - editor_share, theme.surface.canvas, theme.border.muted);
        let accent = theme.accent.primary;
        let editor = block("Editor", editor_share, accent.opacity(EDITOR_BLOCK_TINT), accent);
        let (first, second) = match side {
            BundleSide::Left | BundleSide::Top => (editor, terminal),
            BundleSide::Right | BundleSide::Bottom => (terminal, editor),
        };
        div().flex_none().w(px(DIAGRAM_WIDTH_PX * scale)).h(px(DIAGRAM_HEIGHT_PX * scale)).p(px(4.0 * scale))
            .rounded(px(GROUP_RADIUS_PX * scale)).bg(theme.surface.subtle).border_1().border_color(theme.border.muted)
            .flex().when(is_stacked, |diagram| diagram.flex_col())
            .child(first).child(second)
    }

    fn shortcut_line(&self, label: &'static str, combo: &Combo) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        div().flex().items_center().justify_between().gap(px(12.0 * scale))
            .child(div().text_size(px(TITLE_PX * scale)).text_color(theme.text.default).child(label))
            .child(self.keycaps(combo))
    }

    pub(super) fn keycaps(&self, combo: &Combo) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        div().flex_none().flex().gap(px(3.0 * scale))
            .children(combo.keycaps().into_iter().map(|cap| div().min_w(px(KEYCAP_PX * scale)).h(px(KEYCAP_PX * scale))
                .px(px(5.0 * scale)).flex().items_center().justify_center()
                .rounded(px(4.0 * scale)).bg(theme.chrome.control_fill).border_1().border_color(theme.border.muted)
                .text_size(px(theme.ui_font.shortcut_px)).text_color(theme.text.muted).child(cap)))
    }
}
