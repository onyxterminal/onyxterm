#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::app::AppState;
use crate::command::{open_palette, CommandId};
use crate::keymap::Combo;
use crate::palette as palette_render;
use gpui::{
    div, prelude::*, px, IntoElement, KeyDownEvent, ParentElement, Render, Styled, Window,
    WindowOptions,
};

pub struct Shell {
    state: AppState,
}

impl Shell {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    fn resolve_combo(&self, event: &KeyDownEvent) -> Option<CommandId> {
        let combo = Combo::new(event.keystroke.modifiers, event.keystroke.key.as_str());
        self.state.keymap.resolve(&combo)
    }

    fn dispatch(&mut self, id: CommandId) {
        crate::command::CommandRegistry::dispatch(&mut self.state, id);
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let tab_strip_height = self.state.theme.layout.tab_strip_height_px;
        let rail_width = self.state.theme.layout.workspace_rail_width_px;
        let theme = &self.state.theme;
        let canvas_bg = theme.canvas();
        let border = theme.border_default();
        let text_default = theme.text_default();
        let accent = theme.accent_color();
        let muted = theme.text.muted;
        let rail_bg = theme.surface.subtle;
        let font_size = theme.font.size_px;

        let palette_overlay = palette_render::build_overlay(&self.state, theme);
        let workspaces = self.state.workspaces.clone();
        let palette_visible = self.state.palette_visible;

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(canvas_bg)
            .border_1()
            .border_color(border)
            .text_color(text_default)
            .on_key_down(cx.listener(|view, event, _window, cx| {
                if let Some(id) = view.resolve_combo(event) {
                    view.dispatch(id);
                    cx.notify();
                }
            }))
            .child(
                div()
                    .h(px(tab_strip_height))
                    .bg(rail_bg)
                    .border_b_1()
                    .border_color(border)
                    .flex()
                    .items_center()
                    .px_3()
                    .child("Onyxterm 0.1.0"),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .size_full()
                    .child(
                        div()
                            .w(px(rail_width))
                            .bg(rail_bg)
                            .border_r_1()
                            .border_color(border)
                            .flex_col()
                            .py_2()
                            .px_2()
                            .child(div().text_color(muted).py_1().child("Workspaces"))
                            .children(workspaces.iter().map(|ws| {
                                div().py_1().text_color(muted).child(ws.name.clone())
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .size_full()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(accent)
                                    .text_size(px(font_size * 3.0))
                                    .child("Onyxterm"),
                            )
                            .child(
                                div()
                                    .text_color(muted)
                                    .text_size(px(font_size))
                                    .child("Press \u{2318}K to open the command palette."),
                            ),
                    ),
            )
            .when(palette_visible, |el| el.child(palette_overlay))
    }
}

pub fn open_shell_window(
    options: WindowOptions,
    state: AppState,
    cx: &mut gpui::App,
) -> gpui::Result<gpui::WindowHandle<Shell>> {
    let themed_options = WindowOptions {
        window_background: state.theme.window_background(),
        ..options
    };
    cx.open_window(themed_options, |_window, cx| {
        cx.new(|_cx| Shell::new(state))
    })
}

pub fn palette_visible(state: &AppState) -> bool {
    state.palette_visible
}

pub fn open_or_toggle_palette(state: &mut AppState) {
    if !state.palette_visible {
        open_palette(state);
    } else {
        state.palette_visible = false;
    }
}
