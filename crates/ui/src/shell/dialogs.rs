use super::Shell;
use crate::command::CommandId;
use crate::confirm::ConfirmCopy;
use crate::pane_title::home_relative;
use crate::theme::box_shadows;
use crate::theme::icons::logo_mark;
use crate::workspace_menu::{workspace_menu, WorkspaceMenuActions, WorkspaceMenuItem, WorkspaceMenuModel};
use gpui::{
    div, prelude::*, px, relative, Context, Div, FontWeight, Hsla, MouseButton, SharedString, Stateful, Window,
};
use std::rc::Rc;
use workspace_service::SshService;

const POPOVER_TOP_OVERLAP_PX: f32 = 2.0;
const POPOVER_LEFT_OVERHANG_PX: f32 = 14.0;
const DIALOG_PADDING_TOP_PX: f32 = 20.0;
const DIALOG_PADDING_X_PX: f32 = 16.0;
const DIALOG_PADDING_BOTTOM_PX: f32 = 16.0;
const DIALOG_GAP_PX: f32 = 8.0;
const DIALOG_LOGO_BOTTOM_PX: f32 = 4.0;
const DIALOG_BODY_LINE_HEIGHT: f32 = 1.45;
const DIALOG_BUTTONS_TOP_PX: f32 = 6.0;
const CANCEL_LABEL: &str = "Cancel";

struct ButtonPaint {
    fill: Hsla,
    text: Hsla,
    border: Hsla,
    weight: FontWeight,
}

impl Shell {
    pub(super) fn render_workspace_menu(&self, is_fullscreen: bool, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let keymap = &self.state.keymap;
        let model = WorkspaceMenuModel {
            items: self.workspace_menu_items(),
            ssh_hosts: self.ssh.list_hosts(),
            field: self.menu_field.clone(),
            search: self.menu_search.clone(),
            new_workspace_shortcut: keymap.combo_for(CommandId::NewWorkspace).map(|combo| combo.glyphs()),
            settings_shortcut: keymap.combo_for(CommandId::OpenSettings).map(|combo| combo.glyphs()),
        };
        let actions = WorkspaceMenuActions {
            select: self.menu_handler(cx, |shell, key, cx| shell.select_workspace_by_key(key, cx)),
            delete: self.menu_handler(cx, |shell, key, cx| shell.request_delete_workspace(key, cx)),
            create: self.menu_click_handler(cx, |shell, _window, cx| shell.start_new_workspace(cx)),
            connect: self.menu_handler(cx, |shell, alias, cx| shell.connect_ssh_host(alias, cx)),
            open_settings: self.menu_click_handler(cx, |shell, window, cx| {
                shell.close_workspace_menu(cx);
                shell.run_command(CommandId::OpenSettings, window, cx);
            }),
        };
        let top = theme.layout.tab_strip_height_px - POPOVER_TOP_OVERLAP_PX;
        let left = (theme.top_bar_leading_inset_px(is_fullscreen) - POPOVER_LEFT_OVERHANG_PX).max(0.0);
        div()
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| shell.close_workspace_menu(cx)))
            .child(
                div()
                    .absolute()
                    .top(px(top))
                    .left(px(left))
                    .occlude()
                    .child(workspace_menu(model, theme, actions)),
            )
    }

    fn workspace_menu_items(&self) -> Vec<WorkspaceMenuItem> {
        let active = self.model.active_id();
        let home = self.model.home();
        self.model
            .entries()
            .iter()
            .enumerate()
            .map(|(index, entry)| WorkspaceMenuItem {
                id: entry.workspace.id.to_string().into(),
                name: entry.workspace.name.clone().into(),
                path: home_relative(entry.workspace.root.as_deref().unwrap_or(home), home).into(),
                tab_count: entry.tabs.len(),
                shortcut: self.state.keymap.combo_for(CommandId::SelectWorkspace(index)).map(|combo| combo.glyphs()),
                is_active: entry.workspace.id == active,
                can_delete: !entry.is_scratch,
            })
            .collect()
    }

    fn menu_click_handler(
        &self,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Shell, &mut Window, &mut Context<Shell>) + 'static,
    ) -> crate::workspace_menu::ClickHandler {
        let shell = cx.entity().downgrade();
        let action = Rc::new(action);
        Rc::new(move |window: &mut Window, cx: &mut gpui::App| {
            let action = action.clone();
            if let Err(err) = shell.update(cx, |shell, cx| action(shell, window, cx)) {
                tracing::debug!(?err, "workspace menu after shell dropped");
            }
        })
    }

    fn menu_handler(
        &self,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Shell, &SharedString, &mut Context<Shell>) + 'static,
    ) -> crate::workspace_menu::SelectHandler {
        let shell = cx.entity().downgrade();
        let action = Rc::new(action);
        Rc::new(move |key: &SharedString, _window: &mut Window, cx: &mut gpui::App| {
            let action = action.clone();
            if let Err(err) = shell.update(cx, |shell, cx| action(shell, key, cx)) {
                tracing::debug!(?err, "workspace menu after shell dropped");
            }
        })
    }

    pub(super) fn render_confirmation(&self, copy: ConfirmCopy, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let dialog = theme.dialog;
        let (confirm_fill, confirm_text) = match copy.is_destructive {
            true => (theme.chrome.destructive, theme.chrome.on_destructive),
            false => (theme.accent.primary, theme.text.on_accent),
        };
        let confirm = ButtonPaint {
            fill: confirm_fill,
            text: confirm_text,
            border: confirm_fill,
            weight: FontWeight::SEMIBOLD,
        };
        let cancel = ButtonPaint {
            fill: theme.chrome.control_fill,
            text: theme.text.default,
            border: theme.border.default,
            weight: FontWeight::NORMAL,
        };
        let buttons = div()
            .w_full()
            .mt(px(DIALOG_BUTTONS_TOP_PX))
            .flex()
            .flex_col()
            .gap(px(DIALOG_GAP_PX))
            .child(self.prompt_button("close-confirm", copy.action, confirm).on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _event, _window, cx| shell.accept_confirmation(cx)),
            ))
            .child(self.prompt_button("close-cancel", CANCEL_LABEL, cancel).on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _event, _window, cx| shell.cancel_confirmation(cx)),
            ));
        let body = (!copy.body.is_empty()).then(|| {
            div()
                .text_size(px(theme.ui_font.dialog_body_px))
                .line_height(relative(DIALOG_BODY_LINE_HEIGHT))
                .text_color(theme.text.muted)
                .child(copy.body)
        });
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(theme.chrome.backdrop)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(dialog.width_px))
                    .pt(px(DIALOG_PADDING_TOP_PX))
                    .px(px(DIALOG_PADDING_X_PX))
                    .pb(px(DIALOG_PADDING_BOTTOM_PX))
                    .font_family(SharedString::from(theme.ui_font.family.clone()))
                    .bg(theme.chrome.menu_fill)
                    .border(px(theme.palette.border_px))
                    .border_color(theme.border.default)
                    .rounded(px(dialog.radius_px))
                    .shadow(box_shadows(&theme.shadow.palette))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(DIALOG_GAP_PX))
                    .text_center()
                    .child(
                        logo_mark(dialog.logo_px, theme.appearance)
                            .mb(px(DIALOG_LOGO_BOTTOM_PX)),
                    )
                    .child(
                        div()
                            .text_size(px(theme.ui_font.dialog_title_px))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text.default)
                            .child(copy.title),
                    )
                    .children(body)
                    .child(buttons),
            )
    }

    fn prompt_button(&self, id: &'static str, label: &'static str, paint: ButtonPaint) -> Stateful<Div> {
        let theme = &self.state.theme;
        div()
            .id(id)
            .w_full()
            .h(px(theme.dialog.button_height_px))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(theme.dialog.button_radius_px))
            .cursor_pointer()
            .text_size(px(theme.ui_font.menu_px))
            .font_weight(paint.weight)
            .bg(paint.fill)
            .text_color(paint.text)
            .border(px(theme.palette.border_px))
            .border_color(paint.border)
            .child(label)
    }
}
