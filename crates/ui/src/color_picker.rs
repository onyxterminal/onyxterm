#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::theme::custom::ColorField;
use crate::theme::hsv::{Hsv, Rgb, FULL_TURN_DEGREES};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PickerDrag {
    Spectrum,
    Hue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColorPicker {
    field: ColorField,
    original: Rgb,
    persisted: Rgb,
    color: Hsv,
    drag: Option<PickerDrag>,
}

impl ColorPicker {
    pub fn open(field: ColorField, saved: &str) -> Self {
        let original = Rgb::from_hex(saved).unwrap_or_default();
        Self { field, original, persisted: original, color: Hsv::from_rgb(original), drag: None }
    }

    pub fn field(&self) -> ColorField {
        self.field
    }

    pub fn color(&self) -> Hsv {
        self.color
    }

    pub fn hex(&self) -> String {
        self.color.to_rgb().to_hex()
    }

    pub fn original(&self) -> Rgb {
        self.original
    }

    pub fn drag(&self) -> Option<PickerDrag> {
        self.drag
    }

    pub fn begin_drag(&mut self, drag: PickerDrag) {
        self.drag = Some(drag);
    }

    pub fn end_drag(&mut self) {
        self.drag = None;
    }

    pub fn drag_to(&mut self, horizontal: f32, vertical: f32) {
        let (horizontal, vertical) = (horizontal.clamp(0.0, 1.0), vertical.clamp(0.0, 1.0));
        let Hsv { hue, saturation, value } = self.color;
        self.color = match self.drag {
            Some(PickerDrag::Spectrum) => Hsv::new(hue, horizontal, 1.0 - vertical),
            Some(PickerDrag::Hue) => Hsv::new(horizontal * FULL_TURN_DEGREES, saturation, value),
            None => return,
        };
    }

    pub fn pick(&mut self, rgb: Rgb) {
        let picked = Hsv::from_rgb(rgb);
        if !picked.is_achromatic() {
            self.color = picked;
            return;
        }
        let saturation = if picked.value == 0.0 { self.color.saturation } else { picked.saturation };
        self.color = Hsv::new(self.color.hue, saturation, picked.value);
    }

    pub fn revert(&mut self) {
        self.pick(self.original);
    }

    pub fn unsaved_hex(&self) -> Option<String> {
        let current = self.color.to_rgb();
        (current != self.persisted).then(|| current.to_hex())
    }

    pub fn mark_persisted(&mut self) {
        self.persisted = self.color.to_rgb();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picker() -> ColorPicker {
        ColorPicker::open(ColorField::Accent, "#3366CC")
    }

    #[test]
    fn opening_starts_clean_on_the_saved_color() {
        let picker = picker();
        assert_eq!(picker.hex(), "#3366CC");
        assert_eq!(picker.unsaved_hex(), None);
        assert_eq!(picker.drag(), None);
    }

    #[test]
    fn dragging_previews_without_persisting_until_release() {
        let mut picker = picker();
        picker.begin_drag(PickerDrag::Spectrum);
        picker.drag_to(1.0, 0.0);
        picker.drag_to(0.5, 0.5);
        assert_eq!(picker.drag(), Some(PickerDrag::Spectrum));
        assert_eq!(picker.unsaved_hex().as_deref(), Some(picker.hex().as_str()));
        picker.end_drag();
        picker.mark_persisted();
        assert_eq!(picker.unsaved_hex(), None);
    }

    #[test]
    fn spectrum_drag_clamps_outside_the_square() {
        let mut picker = picker();
        picker.begin_drag(PickerDrag::Spectrum);
        picker.drag_to(-3.0, 4.0);
        assert_eq!(picker.hex(), "#000000");
        picker.drag_to(9.0, -9.0);
        assert_eq!((picker.color().saturation, picker.color().value), (1.0, 1.0));
    }

    #[test]
    fn hue_drag_keeps_saturation_and_value() {
        let mut picker = picker();
        let before = picker.color();
        picker.begin_drag(PickerDrag::Hue);
        picker.drag_to(0.0, 0.7);
        assert_eq!(picker.color(), Hsv::new(0.0, before.saturation, before.value));
        picker.drag_to(1.0, 0.0);
        assert_eq!(picker.color().hue, FULL_TURN_DEGREES);
    }

    #[test]
    fn moving_without_a_drag_changes_nothing() {
        let mut picker = picker();
        picker.drag_to(0.0, 1.0);
        assert_eq!(picker.hex(), "#3366CC");
    }

    #[test]
    fn revert_restores_the_opening_color_and_reports_it_unsaved() {
        let mut picker = picker();
        picker.pick(Rgb::from_hex("#FF0000").unwrap());
        picker.mark_persisted();
        picker.revert();
        assert_eq!(picker.hex(), "#3366CC");
        assert_eq!(picker.unsaved_hex().as_deref(), Some("#3366CC"));
    }

    #[test]
    fn picking_grey_keeps_the_hue_so_the_slider_does_not_jump() {
        let mut picker = picker();
        let hue = picker.color().hue;
        picker.pick(Rgb::from_hex("#777777").unwrap());
        assert_eq!(picker.color().hue, hue);
        assert_eq!(picker.hex(), "#777777");
        picker.pick(Rgb::from_hex("#000000").unwrap());
        assert_eq!(picker.color().saturation, 0.0);
        assert_eq!(picker.color().hue, hue);
    }
}
