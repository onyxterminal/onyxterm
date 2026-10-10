#![deny(warnings)]
#![forbid(unsafe_code)]

pub mod app;
pub mod command;
pub mod keymap;
pub mod palette;
pub mod shell;
pub mod theme;

#[cfg(test)]
mod tests;

use crate::app::AppState;
use gpui::{Bounds, WindowOptions};
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

pub fn run_app() -> Result<(), UiError> {
    let app = gpui::Application::new();
    app.run(|cx| {
        let state = AppState::new();
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
        let _ = shell::open_shell_window(options, state, cx);
    });
    Ok(())
}
