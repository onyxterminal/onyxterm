use gpui::{prelude::*, px, svg, AssetSource, Hsla, SharedString, Svg};
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Gear,
    Close,
    Chevron,
    Magnifier,
    Trash,
    Appearance,
    Terminal,
    Editor,
    Keyboard,
}

impl Icon {
    pub const ALL: [Self; 9] = [
        Self::Gear, Self::Close, Self::Chevron, Self::Magnifier, Self::Trash,
        Self::Appearance, Self::Terminal, Self::Editor, Self::Keyboard,
    ];

    fn asset_path(self) -> &'static str {
        match self {
            Icon::Gear => "icons/ui/gear.svg",
            Icon::Close => "icons/ui/close.svg",
            Icon::Chevron => "icons/ui/chevron.svg",
            Icon::Magnifier => "icons/ui/magnifier.svg",
            Icon::Trash => "icons/ui/trash.svg",
            Icon::Appearance => "icons/ui/appearance.svg",
            Icon::Terminal => "icons/ui/terminal.svg",
            Icon::Editor => "icons/ui/editor.svg",
            Icon::Keyboard => "icons/ui/keyboard.svg",
        }
    }

    fn svg_bytes(self) -> &'static [u8] {
        match self {
            Icon::Gear => include_bytes!("../../../../assets/icons/ui/gear.svg"),
            Icon::Close => include_bytes!("../../../../assets/icons/ui/close.svg"),
            Icon::Chevron => include_bytes!("../../../../assets/icons/ui/chevron.svg"),
            Icon::Magnifier => include_bytes!("../../../../assets/icons/ui/magnifier.svg"),
            Icon::Trash => include_bytes!("../../../../assets/icons/ui/trash.svg"),
            Icon::Appearance => include_bytes!("../../../../assets/icons/ui/appearance.svg"),
            Icon::Terminal => include_bytes!("../../../../assets/icons/ui/terminal.svg"),
            Icon::Editor => include_bytes!("../../../../assets/icons/ui/editor.svg"),
            Icon::Keyboard => include_bytes!("../../../../assets/icons/ui/keyboard.svg"),
        }
    }

    pub fn sized(self, size_px: f32, color: Hsla) -> Svg {
        svg().path(self.asset_path()).size(px(size_px)).flex_none().text_color(color)
    }
}

pub struct IconAssets;

impl AssetSource for IconAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let icon = Icon::ALL.into_iter().find(|icon| icon.asset_path() == path);
        Ok(icon.map(|icon| Cow::Borrowed(icon.svg_bytes())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let paths = Icon::ALL.into_iter().map(Icon::asset_path).filter(|asset| asset.starts_with(path));
        Ok(paths.map(SharedString::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_loads_as_a_tintable_svg() {
        for icon in Icon::ALL {
            let bytes = IconAssets.load(icon.asset_path()).unwrap().expect("icon is bundled");
            let source = std::str::from_utf8(&bytes).unwrap();
            assert!(source.contains("currentColor"), "{icon:?} must tint from text color");
        }
        assert!(IconAssets.load("icons/ui/missing.svg").unwrap().is_none());
        assert_eq!(IconAssets.list("icons/ui/").unwrap().len(), Icon::ALL.len());
    }
}
