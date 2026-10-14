use super::settings_panes::{KEYCAP_PX, ROW_INSET_PX, SECTION_GAP_PX};
use super::settings_view::{CAPTION_PX, TITLE_PX};
use super::{FieldTarget, Shell};
use crate::command::CommandId;
use crate::keymap::{capture_shortcut, shortcut_groups, Capture, Combo, Keymap, Shortcut, ShortcutOverrides};
use crate::menus::install_menus;
use crate::settings::SHORTCUTS_KEY;
use gpui::{div, prelude::*, px, Context, Div, FontWeight, Keystroke, MouseButton, SharedString, Stateful};

const GROUP_TITLE_PX: f32 = 11.0;
const SHORTCUT_ROW_MIN_HEIGHT_PX: f32 = 36.0;
const BINDING_PAD_PX: f32 = 3.0;
const RECORDING_PROMPT: &str = "Type shortcut\u{2026}";
const UNASSIGNED_LABEL: &str = "Unassigned";
const KEYBOARD_HINT: &str = "Click a shortcut, then press the new keys. Esc cancels.";
const FILTER_PLACEHOLDER: &str = "Filter shortcuts";
const NO_MATCHES: &str = "No shortcuts match.";

impl Shell {
    pub(super) fn apply_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.state.keymap = Keymap::with_overrides(&self.state.settings.shortcut_overrides);
        install_menus(&self.state.keymap, cx);
    }

    pub(super) fn record_shortcut_key(&mut self, command: CommandId, key: &Keystroke, cx: &mut Context<Self>) {
        match capture_shortcut(key) {
            Some(Capture::Cancel) => self.state.shortcut_recording = None,
            Some(Capture::Unbind) => self.edit_shortcut(None, cx, |overrides| overrides.set(command, None)),
            Some(Capture::Assign(combo)) => {
                let displaced = self.state.keymap.resolve(&combo).filter(|other| *other != command);
                self.edit_shortcut(displaced.map(|other| (command, other)), cx,
                    |overrides| overrides.assign(command, combo));
            }
            None => {}
        }
    }

    fn reset_shortcut(&mut self, command: CommandId, cx: &mut Context<Self>) {
        self.edit_shortcut(None, cx, |overrides| overrides.reset(command));
    }

    fn edit_shortcut(
        &mut self,
        replaced: Option<(CommandId, CommandId)>,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut ShortcutOverrides),
    ) {
        edit(&mut self.state.settings.shortcut_overrides);
        self.state.shortcut_recording = None;
        self.state.shortcut_replaced = replaced;
        self.state.settings_error = None;
        self.apply_shortcuts(cx);
        let encoded = self.state.settings.shortcut_overrides.to_json();
        self.save_setting(SHORTCUTS_KEY, &encoded);
        cx.notify();
    }

    pub(super) fn render_keyboard_pane(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let defaults = Keymap::default();
        let groups = shortcut_groups(&self.state.registry, &self.state.keymap, self.state.shortcut_filter.text());
        div().flex().flex_col().gap(px(SECTION_GAP_PX * scale))
            .child(div().flex().items_center().justify_between().gap(px(16.0 * scale))
                .child(div().flex_1().min_w(px(0.0)).text_size(px(CAPTION_PX * scale)).text_color(theme.text.muted)
                    .child(KEYBOARD_HINT))
                .child(self.settings_search_field(FieldTarget::ShortcutFilter, FILTER_PLACEHOLDER, cx)))
            .when(groups.is_empty(), |pane| pane.child(div().py(px(16.0 * scale)).flex().justify_center()
                .text_color(theme.text.muted).child(NO_MATCHES)))
            .children(groups.into_iter().map(|group| {
                div().flex().flex_col().gap(px(6.0 * scale))
                    .child(div().px(px(4.0 * scale)).text_size(px(GROUP_TITLE_PX * scale))
                        .font_weight(FontWeight::SEMIBOLD).text_color(theme.text.muted).child(group.category))
                    .child(self.settings_group(group.shortcuts.iter()
                        .map(|shortcut| self.shortcut_row(shortcut, &defaults, cx))))
            }))
    }

    fn shortcut_row(&self, shortcut: &Shortcut, defaults: &Keymap, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let command = shortcut.command;
        let is_customized = self.state.keymap.combos_for(command) != defaults.combos_for(command);
        let replaced = self.state.shortcut_replaced
            .filter(|(edited, _)| *edited == command)
            .and_then(|(_, other)| self.state.registry.get(other))
            .map(|other| format!("Replaced {}", other.title));
        div().min_h(px(SHORTCUT_ROW_MIN_HEIGHT_PX * scale)).px(px(ROW_INSET_PX * scale)).py(px(6.0 * scale))
            .flex().items_center().justify_between().gap(px(12.0 * scale))
            .child(div().flex_1().min_w(px(0.0))
                .child(div().text_size(px(TITLE_PX * scale)).text_color(theme.text.default).child(shortcut.title))
                .children(replaced.map(|caption| div().text_size(px(CAPTION_PX * scale))
                    .text_color(theme.status.stopping).child(caption))))
            .child(div().flex_none().flex().items_center().gap(px(10.0 * scale))
                .when(is_customized, |controls| controls.child(self.reset_shortcut_link(command, cx)))
                .child(self.binding_button(command, shortcut.combo.as_ref(), cx)))
    }

    fn reset_shortcut_link(&self, command: CommandId, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        div().id(element_id("reset-shortcut", command)).cursor_pointer()
            .text_size(px(CAPTION_PX * scale)).text_color(theme.accent.primary).child("Reset")
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation(); shell.reset_shortcut(command, cx);
            }))
    }

    fn binding_button(&self, command: CommandId, combo: Option<&Combo>, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let is_recording = self.state.shortcut_recording == Some(command);
        let button = div().id(element_id("shortcut-binding", command))
            .min_h(px((KEYCAP_PX + BINDING_PAD_PX * 2.0) * scale)).p(px(BINDING_PAD_PX * scale))
            .flex().items_center().rounded(px(6.0 * scale)).border_1().cursor_pointer()
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation(); shell.state.shortcut_recording = Some(command); cx.notify();
            }));
        if is_recording {
            return button.px(px(8.0 * scale)).border_color(theme.accent.primary)
                .text_size(px(CAPTION_PX * scale)).text_color(theme.accent.primary).child(RECORDING_PROMPT);
        }
        let hover_fill = theme.chrome.control_fill;
        let button = button.border_color(gpui::transparent_black()).hover(move |style| style.bg(hover_fill));
        match combo {
            Some(combo) => button.child(self.keycaps(combo)),
            None => button.px(px(6.0 * scale)).text_size(px(CAPTION_PX * scale)).text_color(theme.text.muted)
                .child(UNASSIGNED_LABEL),
        }
    }
}

fn element_id(prefix: &str, command: CommandId) -> SharedString {
    SharedString::from(format!("{prefix}-{}", command.key().unwrap_or_default()))
}
