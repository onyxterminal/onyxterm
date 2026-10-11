use super::Shell;
use crate::confirm::ConfirmCopy;
use crate::workspace_menu::{workspace_menu, WorkspaceMenuActions, WorkspaceMenuItem};
use gpui::{div, prelude::*, px, Context, Div, Hsla, MouseButton, SharedString, Window};
use std::rc::Rc;

const CLOSE_PROMPT_WIDTH_PX: f32 = 360.0;
const BACKDROP_ALPHA: f32 = 0.35;

impl Shell {
    pub(super) fn render_workspace_menu(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let active = self.model.active_id();
        let items = self
            .model
            .entries()
            .iter()
            .map(|entry| WorkspaceMenuItem {
                id: entry.workspace.id.to_string().into(),
                name: entry.workspace.name.clone().into(),
                is_active: entry.workspace.id == active,
                is_editable: !entry.is_scratch,
            })
            .collect();
        let actions = WorkspaceMenuActions {
            select: self.menu_handler(cx, |shell, key, cx| shell.select_workspace_by_key(key, cx)),
            rename: self.menu_handler(cx, |shell, key, cx| shell.start_rename_workspace(key, cx)),
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
        div()
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| shell.close_workspace_menu(cx)))
            .child(
                div()
                    .absolute()
                    .top(px(theme.layout.tab_strip_height_px))
                    .left(px(theme.layout.gap_px))
                    .occlude()
                    .child(workspace_menu(items, self.menu_field.clone(), theme, actions)),
            )
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

    pub(super) fn render_notice(&self, notice: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        div()
            .flex_none()
            .px_3()
            .py_1()
            .flex()
            .flex_row()
            .justify_between()
            .gap_3()
            .mx(px(theme.layout.window_margin_px))
            .mb(px(theme.layout.island_gap_px))
            .rounded(px(theme.layout.island_radius_px))
            .bg(theme.canvas())
            .border_1()
            .border_color(theme.border_default())
            .text_color(theme.text.muted)
            .child(notice.to_string())
            .child(
                div()
                    .id("notice-dismiss")
                    .cursor_pointer()
                    .text_color(theme.accent.primary)
                    .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| shell.dismiss_notice(cx)))
                    .child("Dismiss"),
            )
    }

    pub(super) fn render_confirmation(&self, copy: ConfirmCopy, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let backdrop = Hsla { a: BACKDROP_ALPHA, ..gpui::black() };
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(backdrop)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(CLOSE_PROMPT_WIDTH_PX))
                    .p_4()
                    .bg(theme.surface.raised)
                    .border_1()
                    .border_color(theme.border_default())
                    .rounded(px(theme.layout.island_radius_px))
                    .shadow_md()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(copy.title)
                    .child(div().text_color(theme.text.muted).child(copy.body))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap_2()
                            .child(
                                self.prompt_button("close-cancel", "Cancel", (theme.surface.subtle, theme.text_default()))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|shell, _event, _window, cx| shell.cancel_confirmation(cx)),
                                    ),
                            )
                            .child(
                                self.prompt_button(
                                    "close-confirm",
                                    copy.action,
                                    (theme.status.failed, theme.text.inverse),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|shell, _event, _window, cx| shell.accept_confirmation(cx)),
                                ),
                            ),
                    ),
            )
    }

    fn prompt_button(&self, id: &'static str, label: &'static str, (bg, fg): (Hsla, Hsla)) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .px_3()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .bg(bg)
            .text_color(fg)
            .border_1()
            .border_color(self.state.theme.border_default())
            .child(label)
    }
}
