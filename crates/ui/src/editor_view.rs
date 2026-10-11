#![deny(warnings)]
#![forbid(unsafe_code)]

#[derive(Debug, Default)]
pub struct EditorView {
    pub scroll_top: f32,
    pub scroll_left: f32,
}
