use gpui::{img, prelude::*, px, Image, ImageFormat, Img};
use std::sync::{Arc, LazyLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Gear,
    Close,
    Chevron,
    Magnifier,
    Pencil,
    Trash,
    Appearance,
    Terminal,
    Editor,
    Keyboard,
}

const GEAR_SVG: &[u8] = include_bytes!("../../../../assets/icons/ui/gear.svg");
const CLOSE_SVG: &[u8] = include_bytes!("../../../../assets/icons/ui/close.svg");
const CHEVRON_SVG: &[u8] = include_bytes!("../../../../assets/icons/ui/chevron.svg");
const MAGNIFIER_SVG: &[u8] = include_bytes!("../../../../assets/icons/ui/magnifier.svg");
const PENCIL_SVG: &[u8] = include_bytes!("../../../../assets/icons/ui/pencil.svg");
const TRASH_SVG: &[u8] = include_bytes!("../../../../assets/icons/ui/trash.svg");
const APPEARANCE_SVG: &[u8] = include_bytes!("../../../../assets/icons/ui/appearance.svg");
const TERMINAL_SVG: &[u8] = include_bytes!("../../../../assets/icons/ui/terminal.svg");
const EDITOR_SVG: &[u8] = include_bytes!("../../../../assets/icons/ui/editor.svg");
const KEYBOARD_SVG: &[u8] = include_bytes!("../../../../assets/icons/ui/keyboard.svg");

fn svg_image(bytes: &[u8]) -> Arc<Image> {
    Arc::new(Image::from_bytes(ImageFormat::Svg, bytes.to_vec()))
}

static GEAR: LazyLock<Arc<Image>> = LazyLock::new(|| svg_image(GEAR_SVG));
static CLOSE: LazyLock<Arc<Image>> = LazyLock::new(|| svg_image(CLOSE_SVG));
static CHEVRON: LazyLock<Arc<Image>> = LazyLock::new(|| svg_image(CHEVRON_SVG));
static MAGNIFIER: LazyLock<Arc<Image>> = LazyLock::new(|| svg_image(MAGNIFIER_SVG));
static PENCIL: LazyLock<Arc<Image>> = LazyLock::new(|| svg_image(PENCIL_SVG));
static TRASH: LazyLock<Arc<Image>> = LazyLock::new(|| svg_image(TRASH_SVG));
static APPEARANCE: LazyLock<Arc<Image>> = LazyLock::new(|| svg_image(APPEARANCE_SVG));
static TERMINAL: LazyLock<Arc<Image>> = LazyLock::new(|| svg_image(TERMINAL_SVG));
static EDITOR: LazyLock<Arc<Image>> = LazyLock::new(|| svg_image(EDITOR_SVG));
static KEYBOARD: LazyLock<Arc<Image>> = LazyLock::new(|| svg_image(KEYBOARD_SVG));

impl Icon {
    fn image(self) -> Arc<Image> {
        let image = match self {
            Icon::Gear => &GEAR,
            Icon::Close => &CLOSE,
            Icon::Chevron => &CHEVRON,
            Icon::Magnifier => &MAGNIFIER,
            Icon::Pencil => &PENCIL,
            Icon::Trash => &TRASH,
            Icon::Appearance => &APPEARANCE,
            Icon::Terminal => &TERMINAL,
            Icon::Editor => &EDITOR,
            Icon::Keyboard => &KEYBOARD,
        };
        Arc::clone(image)
    }

    pub fn sized(self, size_px: f32) -> Img {
        img(self.image()).size(px(size_px)).flex_none()
    }
}
