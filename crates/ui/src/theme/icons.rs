use super::Appearance;
use gpui::{img, prelude::*, px, svg, AssetSource, Hsla, Img, SharedString, Svg};
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
    Logo,
    LogoPrompt,
    Plus,
    More,
    CloseBold,
    MagnifierBold,
    ChevronUp,
    CloseFind,
    File,
}

impl Icon {
    pub const ALL: [Self; 18] = [
        Self::Gear, Self::Close, Self::Chevron, Self::Magnifier, Self::Trash,
        Self::Appearance, Self::Terminal, Self::Editor, Self::Keyboard,
        Self::Logo, Self::LogoPrompt, Self::Plus, Self::More, Self::CloseBold, Self::MagnifierBold,
        Self::ChevronUp, Self::CloseFind, Self::File,
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
            Icon::Logo => "icons/ui/logo.svg",
            Icon::LogoPrompt => "icons/ui/logo-prompt.svg",
            Icon::Plus => "icons/ui/plus.svg",
            Icon::More => "icons/ui/more.svg",
            Icon::CloseBold => "icons/ui/close-bold.svg",
            Icon::MagnifierBold => "icons/ui/magnifier-bold.svg",
            Icon::ChevronUp => "icons/ui/chevron-up.svg",
            Icon::CloseFind => "icons/ui/close-find.svg",
            Icon::File => "icons/ui/file.svg",
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
            Icon::Logo => include_bytes!("../../../../assets/icons/ui/logo.svg"),
            Icon::LogoPrompt => include_bytes!("../../../../assets/icons/ui/logo-prompt.svg"),
            Icon::Plus => include_bytes!("../../../../assets/icons/ui/plus.svg"),
            Icon::More => include_bytes!("../../../../assets/icons/ui/more.svg"),
            Icon::CloseBold => include_bytes!("../../../../assets/icons/ui/close-bold.svg"),
            Icon::MagnifierBold => include_bytes!("../../../../assets/icons/ui/magnifier-bold.svg"),
            Icon::ChevronUp => include_bytes!("../../../../assets/icons/ui/chevron-up.svg"),
            Icon::CloseFind => include_bytes!("../../../../assets/icons/ui/close-find.svg"),
            Icon::File => include_bytes!("../../../../assets/icons/ui/file.svg"),
        }
    }

    pub fn sized(self, size_px: f32, color: Hsla) -> Svg {
        svg().path(self.asset_path()).size(px(size_px)).flex_none().text_color(color)
    }
}

const LOGO_LIGHT: &str = "logos/logo-light-mode.svg";
const LOGO_DARK: &str = "logos/logo-dark-mode.svg";

fn logo_asset_path(appearance: Appearance) -> &'static str {
    match appearance {
        Appearance::Light => LOGO_LIGHT,
        Appearance::Dark => LOGO_DARK,
    }
}

pub fn logo_mark(size_px: f32, appearance: Appearance) -> Img {
    img(logo_asset_path(appearance)).flex_none().size(px(size_px))
}

pub struct IconAssets;

impl AssetSource for IconAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        match path {
            LOGO_LIGHT => return Ok(Some(Cow::Borrowed(include_bytes!("../../../../logos/logo-light-mode.svg")))),
            LOGO_DARK => return Ok(Some(Cow::Borrowed(include_bytes!("../../../../logos/logo-dark-mode.svg")))),
            _ => {},
        }
        let icon = Icon::ALL.into_iter().find(|icon| icon.asset_path() == path);
        Ok(icon.map(|icon| Cow::Borrowed(icon.svg_bytes())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let paths = Icon::ALL.into_iter().map(Icon::asset_path)
            .chain([LOGO_LIGHT, LOGO_DARK]).filter(|asset| asset.starts_with(path));
        Ok(paths.map(SharedString::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_logos_use_the_supplied_artwork_for_each_appearance() {
        for (appearance, expected) in [
            (Appearance::Light, include_bytes!("../../../../logos/logo-light-mode.svg").as_slice()),
            (Appearance::Dark, include_bytes!("../../../../logos/logo-dark-mode.svg").as_slice()),
        ] {
            let bytes = IconAssets.load(logo_asset_path(appearance)).unwrap().expect("logo is bundled");
            assert_eq!(bytes.as_ref(), expected);
        }
        assert_eq!(IconAssets.list("logos/").unwrap().len(), 2);
    }

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
