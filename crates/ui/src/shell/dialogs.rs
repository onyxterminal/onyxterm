use super::Shell;
use crate::command::CommandId;
use crate::confirm::ConfirmCopy;
use crate::theme::box_shadows;
use crate::workspace_menu::{workspace_menu, WorkspaceMenuActions, WorkspaceMenuItem, WorkspaceMenuModel};
use gpui::{div, prelude::*, px, Context, Div, FontWeight, Hsla, MouseButton, SharedString, Stateful, Window};
use std::rc::Rc;
use workspace_service::SshService;

const HALF: f32 = 0.5;
const DIALOG_PADDING_PX: f32 = 18.0;
const DIALOG_TEXT_GAP_PX: f32 = 6.0;
const DIALOG_BUTTONS_TOP_PX: f32 = 16.0;
const DIALOG_BUTTON_GAP_PX: f32 = 8.0;
const DIALOG_BUTTON_PADDING_X_PX: f32 = 14.0;
const CANCEL_LABEL: &str = "Cancel";

struct ButtonPaint {
    fill: Hsla,
    text: Hsla,
    border: Hsla,
}

impl Shell {
    pub(super) fn render_workspace_menu(&self, is_fullscreen: bool, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let model = WorkspaceMenuModel {
            items: self.workspace_menu_items(),
            ssh_hosts: self.ssh.list_hosts(),
            field: self.menu_field.clone(),
            search: self.menu_search.clone(),
            new_workspace_shortcut: self.state.keymap.combo_for(CommandId::NewWorkspace).map(|combo| combo.glyphs()),
        };
        let actions = WorkspaceMenuActions {
            select: self.menu_handler(cx, |shell, key, cx| shell.select_workspace_by_key(key, cx)),
            delete: self.menu_handler(cx, |shell, key, cx| shell.request_delete_workspace(key, cx)),
            create: {
                let shell = cx.entity().downgrade();
                Rc::new(move |_window: &mut Window, cx: &mut gpui::App| {
                    if let Err(err) = shell.update(cx, |shell, cx| shell.start_new_workspace(cx)) {
                        tracing::debug!(?err, "workspace menu after shell dropped");
                    }
                })
            },
        };
        let layout = theme.layout;
        let top = (layout.tab_strip_height_px + theme.top_bar.control_height_px) * HALF + theme.menu.offset_px;
        let left = theme.top_bar_leading_inset_px(is_fullscreen);
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
        self.model
            .entries()
            .iter()
            .map(|entry| WorkspaceMenuItem {
                id: entry.workspace.id.to_string().into(),
                name: entry.workspace.name.clone().into(),
                tab_count: entry.tabs.len(),
                is_active: entry.workspace.id == active,
                can_delete: !entry.is_scratch,
            })
            .collect()
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
        let (confirm_fill, confirm_text) = match copy.is_destructive {
            true => (theme.chrome.destructive, theme.chrome.on_destructive),
            false => (theme.accent.primary, theme.text.on_accent),
        };
        let cancel = ButtonPaint { fill: theme.surface.canvas, text: theme.text.default, border: theme.border.default };
        let confirm = ButtonPaint { fill: confirm_fill, text: confirm_text, border: confirm_fill };
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
                    .w(px(theme.dialog.width_px))
                    .p(px(DIALOG_PADDING_PX))
                    .font_family(SharedString::from(theme.ui_font.family.clone()))
                    .bg(theme.chrome.menu_fill)
                    .border(px(theme.palette.border_px))
                    .border_color(theme.border.default)
                    .rounded(px(theme.dialog.radius_px))
                    .shadow(box_shadows(&theme.shadow.palette))
                    .flex()
                    .flex_col()
                    .gap(px(DIALOG_TEXT_GAP_PX))
                    .child(
                        div()
                            .text_size(px(theme.ui_font.dialog_title_px))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text.default)
                            .child(copy.title),
                    )
                    .child(div().text_size(px(theme.ui_font.dialog_body_px)).text_color(theme.text.muted).child(copy.body))
                    .child(
                        div()
                            .pt(px(DIALOG_BUTTONS_TOP_PX))
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap(px(DIALOG_BUTTON_GAP_PX))
                            .child(self.prompt_button("close-cancel", CANCEL_LABEL, cancel).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|shell, _event, _window, cx| shell.cancel_confirmation(cx)),
                            ))
                            .child(self.prompt_button("close-confirm", copy.action, confirm).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|shell, _event, _window, cx| shell.accept_confirmation(cx)),
                            )),
                    ),
            )
    }

    fn prompt_button(&self, id: &'static str, label: &'static str, paint: ButtonPaint) -> Stateful<Div> {
        let theme = &self.state.theme;
        div()
            .id(id)
            .h(px(theme.dialog.button_height_px))
            .px(px(DIALOG_BUTTON_PADDING_X_PX))
            .flex()
            .items_center()
            .rounded(px(theme.dialog.button_radius_px))
            .cursor_pointer()
            .text_size(px(theme.ui_font.button_px))
            .font_weight(FontWeight::MEDIUM)
            .bg(paint.fill)
            .text_color(paint.text)
            .border(px(theme.palette.border_px))
            .border_color(paint.border)
            .child(label)
    }
}
