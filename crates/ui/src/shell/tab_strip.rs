use super::pane_drag::bounds_probe;
use super::{edit_field, Shell, ENTER_KEY, ESCAPE_KEY};
use crate::workspace_menu::CARET;
use gpui::{div, img, prelude::*, px, Context, Image, ImageFormat, Keystroke, MouseButton, SharedString, Window};
use std::sync::{Arc, LazyLock};

const LOGO_SIZE_PX: f32 = 18.0;
const DOUBLE_CLICK: usize = 2;
const LOGO_PNG: &[u8] = include_bytes!("../../../../assets/logo.png");

static LOGO: LazyLock<Arc<Image>> =
    LazyLock::new(|| Arc::new(Image::from_bytes(ImageFormat::Png, LOGO_PNG.to_vec())));

impl Shell {
    pub(super) fn render_tab_strip(&self, is_fullscreen: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let active = self.model.tabs().active_index();
        let labels: Vec<(String, bool)> = self
            .model
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let title = self.pane_display_title(tab.focused());
                let label = match self.tab_field.as_ref().filter(|field| field.index == index) {
                    Some(field) => format!("{}{CARET}", field.text),
                    None => format!("{}  {}", index + 1, tab.name().unwrap_or(&title)),
                };
                (label, index == active)
            })
            .collect();
        let layout = theme.layout;
        let drop_border = theme.accent.primary;
        let leading_inset = match is_fullscreen {
            true => layout.window_margin_px,
            false => layout.traffic_lights_width_px,
        };
        div()
            .h(px(layout.tab_strip_height_px))
            .flex_none()
            .pl(px(leading_inset))
            .pr(px(layout.window_margin_px))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(layout.island_gap_px))
            .on_mouse_down(MouseButton::Left, |event, window, _cx| {
                if event.click_count == DOUBLE_CLICK {
                    window.titlebar_double_click();
                }
            })
            .child(self.render_logo_button(cx))
            .children(labels.into_iter().enumerate().map(|(index, (label, is_active))| {
                let (bg, fg, border) = if is_active {
                    (theme.canvas(), theme.text_default(), theme.border_default())
                } else {
                    (theme.surface.subtle, theme.text.muted, theme.surface.subtle)
                };
                let hover_bg = theme.border.muted;
                let is_editing = self.tab_field.as_ref().is_some_and(|field| field.index == index);
                let border = if is_editing { theme.border_focus() } else { border };
                div()
                    .id(("tab", index))
                    .px_3()
                    .py_1()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .rounded(px(layout.border_radius_px))
                    .bg(bg)
                    .text_color(fg)
                    .border_1()
                    .border_color(border)
                    .when(!is_active, |el| el.hover(move |style| style.bg(hover_bg)))
                    .when(self.is_tab_drop_target(index), |el| el.border_color(drop_border))
                    .relative()
                    .child(bounds_probe(&self.tab_bounds, index))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |shell, event: &gpui::MouseDownEvent, window, cx| {
                            shell.click_tab(index, event.click_count, window, cx)
                        }),
                    )
                    .child(label)
            }))
    }

    fn click_tab(&mut self, index: usize, click_count: usize, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.tab_field.as_ref().is_some_and(|field| field.index == index) {
            return;
        }
        self.commit_tab_field();
        self.activate_tab(index, window, cx);
        if click_count == DOUBLE_CLICK {
            self.start_rename_tab(index);
        }
    }

    pub(super) fn start_rename_tab(&mut self, index: usize) {
        let Some(tab) = self.model.tabs().iter().nth(index) else {
            return;
        };
        let title = self.panes.get(&tab.focused()).map(|pane| pane.title.as_str()).unwrap_or_default();
        let text = tab.name().unwrap_or(title).to_string().into();
        self.tab_field = Some(TabField { index, text });
    }

    pub(super) fn handle_tab_field_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        match keystroke.key.as_str() {
            ESCAPE_KEY => self.tab_field = None,
            ENTER_KEY => self.commit_tab_field(),
            _ => {
                if let Some(field) = self.tab_field.as_mut() {
                    edit_field(&mut field.text, keystroke);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn commit_tab_field(&mut self) {
        let Some(field) = self.tab_field.take() else {
            return;
        };
        let Some(tab) = self.model.tabs_mut().get_mut(field.index) else {
            return;
        };
        tab.rename(Some(field.text.to_string()));
        self.persist();
    }

    pub(super) fn commit_tab_field_on_click(&mut self, cx: &mut Context<Self>) {
        if self.tab_field.is_some() || self.pane_field.is_some() {
            self.commit_tab_field();
            self.commit_pane_field();
            cx.notify();
        }
    }

    fn activate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.model.tabs_mut().activate(index);
        self.persist();
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn render_logo_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let hover_bg = theme.border.muted;
        div()
            .id("workspace-menu-button")
            .p_1()
            .flex()
            .items_center()
            .cursor_pointer()
            .rounded(px(theme.layout.border_radius_px))
            .hover(move |style| style.bg(hover_bg))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _event, _window, cx| {
                    cx.stop_propagation();
                    shell.toggle_workspace_menu(cx);
                }),
            )
            .child(img(LOGO.clone()).size(px(LOGO_SIZE_PX)))
    }
}

pub(super) struct TabField {
    pub(super) index: usize,
    pub(super) text: SharedString,
}
