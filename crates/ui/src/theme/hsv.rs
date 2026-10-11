use gpui::{Hsla, Rgba};

const HEX_PREFIX: char = '#';
const HEX_DIGITS: usize = 6;
const CHANNEL_MAX: f32 = 255.0;
pub const FULL_TURN_DEGREES: f32 = 360.0;
const SECTOR_DEGREES: f32 = 60.0;
const SECTOR_COUNT: f32 = 6.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub fn from_hex(hex: &str) -> Option<Self> {
        let digits = hex.strip_prefix(HEX_PREFIX)?;
        if digits.len() != HEX_DIGITS || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let packed = u32::from_str_radix(digits, 16).ok()?;
        let [_, red, green, blue] = packed.to_be_bytes();
        Some(Self { red, green, blue })
    }

    pub fn to_hex(self) -> String {
        format!("{HEX_PREFIX}{:02X}{:02X}{:02X}", self.red, self.green, self.blue)
    }

    pub fn to_hsla(self) -> Hsla {
        let channel = |value: u8| f32::from(value) / CHANNEL_MAX;
        Rgba { r: channel(self.red), g: channel(self.green), b: channel(self.blue), a: 1.0 }.into()
    }

    fn unit_channels(self) -> [f32; 3] {
        [self.red, self.green, self.blue].map(|value| f32::from(value) / CHANNEL_MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsv {
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
}

impl Hsv {
    pub fn new(hue: f32, saturation: f32, value: f32) -> Self {
        Self {
            hue: hue.clamp(0.0, FULL_TURN_DEGREES),
            saturation: saturation.clamp(0.0, 1.0),
            value: value.clamp(0.0, 1.0),
        }
    }

    pub fn from_rgb(rgb: Rgb) -> Self {
        let [red, green, blue] = rgb.unit_channels();
        let max = red.max(green).max(blue);
        let delta = max - red.min(green).min(blue);
        let saturation = if max == 0.0 { 0.0 } else { delta / max };
        Self::new(hue_of(red, green, blue, max, delta), saturation, max)
    }

    pub fn to_rgb(self) -> Rgb {
        let chroma = self.value * self.saturation;
        let sector = self.hue.rem_euclid(FULL_TURN_DEGREES) / SECTOR_DEGREES;
        let secondary = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
        let (red, green, blue) = match sector.floor() as u8 {
            0 => (chroma, secondary, 0.0),
            1 => (secondary, chroma, 0.0),
            2 => (0.0, chroma, secondary),
            3 => (0.0, secondary, chroma),
            4 => (secondary, 0.0, chroma),
            _ => (chroma, 0.0, secondary),
        };
        let offset = self.value - chroma;
        let channel = |value: f32| ((value + offset) * CHANNEL_MAX).round().clamp(0.0, CHANNEL_MAX) as u8;
        Rgb { red: channel(red), green: channel(green), blue: channel(blue) }
    }

    pub fn to_hsla(self) -> Hsla {
        self.to_rgb().to_hsla()
    }

    pub fn pure_hue(self) -> Hsla {
        Self::new(self.hue, 1.0, 1.0).to_hsla()
    }

    pub fn is_achromatic(self) -> bool {
        self.saturation == 0.0 || self.value == 0.0
    }
}

fn hue_of(red: f32, green: f32, blue: f32, max: f32, delta: f32) -> f32 {
    if delta == 0.0 {
        return 0.0;
    }
    let sector = match max {
        top if top == red => ((green - blue) / delta).rem_euclid(SECTOR_COUNT),
        top if top == green => (blue - red) / delta + 2.0,
        _ => (red - green) / delta + 4.0,
    };
    sector * SECTOR_DEGREES
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(hex: &str) -> Rgb {
        Rgb::from_hex(hex).unwrap()
    }

    #[test]
    fn every_sampled_hex_survives_an_hsv_roundtrip() {
        let steps = (0..=255u8).step_by(15);
        for red in steps.clone() {
            for green in steps.clone() {
                for blue in steps.clone() {
                    let color = Rgb { red, green, blue };
                    assert_eq!(Hsv::from_rgb(color).to_rgb(), color, "{}", color.to_hex());
                }
            }
        }
    }

    #[test]
    fn primaries_land_on_their_hue_angles() {
        assert_eq!(Hsv::from_rgb(rgb("#FF0000")), Hsv::new(0.0, 1.0, 1.0));
        assert_eq!(Hsv::from_rgb(rgb("#00FF00")), Hsv::new(120.0, 1.0, 1.0));
        assert_eq!(Hsv::from_rgb(rgb("#0000FF")), Hsv::new(240.0, 1.0, 1.0));
        assert_eq!(Hsv::from_rgb(rgb("#FF00FF")), Hsv::new(300.0, 1.0, 1.0));
    }

    #[test]
    fn grey_has_no_hue_or_saturation() {
        let grey = Hsv::from_rgb(rgb("#808080"));
        assert_eq!((grey.hue, grey.saturation), (0.0, 0.0));
        assert!(grey.is_achromatic());
        assert_eq!(Hsv::new(217.0, 0.0, 0.5).to_rgb(), rgb("#808080"));
    }

    #[test]
    fn zero_value_is_black_for_any_hue() {
        assert_eq!(Hsv::new(42.0, 0.8, 0.0).to_rgb(), rgb("#000000"));
        assert!(Hsv::from_rgb(rgb("#000000")).is_achromatic());
    }

    #[test]
    fn full_turn_wraps_to_red_and_out_of_range_input_clamps() {
        assert_eq!(Hsv::new(360.0, 1.0, 1.0).to_rgb(), rgb("#FF0000"));
        assert_eq!(Hsv::new(720.0, 2.0, -1.0), Hsv::new(360.0, 1.0, 0.0));
    }

    #[test]
    fn hex_parsing_rejects_malformed_input() {
        assert_eq!(rgb("#1a2B3c").to_hex(), "#1A2B3C");
        for bad in ["1A2B3C", "#1A2B3", "#1A2B3CD", "#+1A2B3", "#GG0000", ""] {
            assert_eq!(Rgb::from_hex(bad), None, "{bad}");
        }
    }
}
