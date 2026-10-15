use super::Shell;
use crate::theme::{box_shadows, Theme};
use gpui::{div, prelude::*, px, Context, Div, Hsla, SharedString, Task};
use std::time::Duration;

const TOAST_DURATION: Duration = Duration::from_millis(2200);
const TOAST_BOTTOM_PX: f32 = 22.0;
const TOAST_PADDING_X_PX: f32 = 14.0;
const TOAST_PADDING_Y_PX: f32 = 8.0;
const TOAST_RADIUS_PX: f32 = 9.0;
const TOAST_GAP_PX: f32 = 8.0;
const TOAST_DOT_PX: f32 = 7.0;
const TOAST_TEXT_PX: f32 = 12.0;
const TOAST_LINE_HEIGHT_PX: f32 = 15.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ToastTone {
    Info,
    Success,
    Failure,
}

impl ToastTone {
    fn dot_color(self, theme: &Theme) -> Hsla {
        match self {
            ToastTone::Info => theme.accent.primary,
            ToastTone::Success => theme.status.running,
            ToastTone::Failure => theme.status.failed,
        }
    }
}

pub(super) struct Toast {
    text: SharedString,
    tone: ToastTone,
    _dismissal: Task<()>,
}

impl Shell {
    pub(super) fn show_toast(&mut self, text: impl Into<SharedString>, tone: ToastTone, cx: &mut Context<Self>) {
        let dismissal = cx.spawn(async move |shell, cx| {
            cx.background_executor().timer(TOAST_DURATION).await;
            let _ = shell.update(cx, |shell, cx| {
                shell.toast = None;
                cx.notify();
            });
        });
        self.toast = Some(Toast { text: text.into(), tone, _dismissal: dismissal });
        cx.notify();
    }

    pub(super) fn render_toast(&self) -> Option<Div> {
        let toast = self.toast.as_ref()?;
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let dot = div().flex_none().size(px(TOAST_DOT_PX * scale)).rounded_full().bg(toast.tone.dot_color(theme));
        let card = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(TOAST_GAP_PX * scale))
            .px(px(TOAST_PADDING_X_PX * scale))
            .py(px(TOAST_PADDING_Y_PX * scale))
            .rounded(px(TOAST_RADIUS_PX * scale))
            .bg(theme.chrome.menu_fill)
            .border(px(theme.menu.border_px))
            .border_color(theme.border.default)
            .shadow(box_shadows(&theme.shadow.toast))
            .font_family(SharedString::from(theme.ui_font.family.clone()))
            .text_size(px(TOAST_TEXT_PX * scale))
            .line_height(px(TOAST_LINE_HEIGHT_PX * scale))
            .text_color(theme.text.default)
            .whitespace_nowrap()
            .child(dot)
            .child(toast.text.clone());
        let anchor = div().absolute().left_0().right_0().bottom(px(TOAST_BOTTOM_PX * scale)).flex().justify_center();
        Some(anchor.child(card))
    }
}
