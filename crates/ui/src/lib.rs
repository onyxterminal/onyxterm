#![deny(warnings)]
#![forbid(unsafe_code)]

pub mod app;
pub mod cell_paint;
pub mod color_picker;
pub mod command;
pub mod confirm;
pub mod copy_mode;
pub mod document;
pub mod editor_chrome;
pub mod editor_keys;
pub mod editor_pane;
pub mod editor_view;
pub mod file_links;
pub mod file_tree;
pub mod file_tree_view;
pub mod git_status;
pub mod input;
pub mod island_look;
pub mod keymap;
pub mod menus;
pub mod motion;
pub mod open_target;
pub mod palette;
pub mod pane_drop;
pub mod pane_layout;
pub mod pane_menu;
pub mod pane_motion;
pub mod pane_title;
pub mod pixel_grid;
pub mod shell;
pub mod slider;
pub mod settings;
pub mod settings_search;
pub mod tabs;
pub mod terminal_annotations;
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
    let app = gpui::Application::new().with_assets(theme::icons::IconAssets);
    app.run(|cx| {
        cx.activate(true);
        let bundled_fonts = observability::timed("bundled fonts", || cx.text_system().add_fonts(vec![
            Cow::Borrowed(include_bytes!("../../../assets/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf")),
            Cow::Borrowed(include_bytes!("../../../assets/fonts/jetbrains-mono/JetBrainsMono-Bold.ttf")),
            Cow::Borrowed(include_bytes!("../../../assets/fonts/jetbrains-mono/JetBrainsMono-Italic.ttf")),
            Cow::Borrowed(include_bytes!("../../../assets/fonts/jetbrains-mono/JetBrainsMono-BoldItalic.ttf")),
        ]));
        if let Err(err) = bundled_fonts {
            tracing::error!(?err, "failed to load bundled JetBrains Mono");
        }
        let options = WindowOptions {
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Onyx".into()),
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
        if let Err(err) = observability::timed("window open", || shell::open_shell_window(options, startup, cx)) {
            tracing::error!(?err, "failed to open main window");
            cx.quit();
        }
        cx.on_window_closed(|cx| cx.quit()).detach();
    });
    Ok(())
}
