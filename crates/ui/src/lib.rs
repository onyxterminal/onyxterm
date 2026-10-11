#![deny(warnings)]
#![forbid(unsafe_code)]

pub mod app;
pub mod color_picker;
pub mod command;
pub mod confirm;
pub mod copy_mode;
pub mod document;
pub mod editor_keys;
pub mod editor_view;
pub mod input;
pub mod island_look;
pub mod keymap;
pub mod menus;
pub mod motion;
pub mod palette;
pub mod pane_drop;
pub mod pane_layout;
pub mod pane_menu;
pub mod pane_motion;
pub mod pane_title;
pub mod shell;
pub mod settings;
pub mod tabs;
pub mod terminal_view;
pub mod text_field;
pub mod theme;
pub mod workspace_menu;
pub mod workspaces;

#[cfg(test)]
mod pane_motion_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod workspace_tests;

use gpui::{Bounds, WindowOptions};
use std::borrow::Cow;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UiError {
    #[error("theme load failed: {0}")]
    Theme(#[from] theme::ThemeError),
    #[error("window open failed: {0}")]
    Window(String),
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub use shell::Startup;

pub fn run_app(startup: Startup) -> Result<(), UiError> {
    let app = gpui::Application::new();
    app.run(|cx| {
        if let Err(err) = cx.text_system().add_fonts(vec![
            Cow::Borrowed(include_bytes!("../../../assets/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf")),
            Cow::Borrowed(include_bytes!("../../../assets/fonts/jetbrains-mono/JetBrainsMono-Bold.ttf")),
            Cow::Borrowed(include_bytes!("../../../assets/fonts/jetbrains-mono/JetBrainsMono-Italic.ttf")),
            Cow::Borrowed(include_bytes!("../../../assets/fonts/jetbrains-mono/JetBrainsMono-BoldItalic.ttf")),
        ]) {
            tracing::error!(?err, "failed to load bundled JetBrains Mono");
        }
        menus::install_menus(cx);
        let options = WindowOptions {
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Onyxterm".into()),
                ..Default::default()
            }),
            window_bounds: Some(gpui::WindowBounds::Windowed(Bounds::centered(
                None,
                gpui::Size {
                    width: gpui::px(1280.0),
                    height: gpui::px(800.0),
                },
                cx,
            ))),
            focus: true,
            show: true,
            ..Default::default()
        };
        if let Err(err) = shell::open_shell_window(options, startup, cx) {
            tracing::error!(?err, "failed to open main window");
            cx.quit();
        }
        cx.on_window_closed(|cx| cx.quit()).detach();
    });
    Ok(())
}
