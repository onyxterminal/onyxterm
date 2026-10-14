#![deny(warnings)]
#![forbid(unsafe_code)]

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderRange {
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

impl SliderRange {
    pub fn value_at(self, fraction: f32) -> f32 {
        let span = self.max - self.min;
        let steps = (fraction.clamp(0.0, 1.0) * span / self.step).round();
        (self.min + steps * self.step).clamp(self.min, self.max)
    }

    pub fn fraction_of(self, value: f32) -> f32 {
        let span = self.max - self.min;
        if span <= 0.0 {
            return 0.0;
        }
        ((value - self.min) / span).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERCENT: SliderRange = SliderRange { min: 75.0, max: 150.0, step: 5.0 };

    #[test]
    fn positions_snap_to_the_nearest_step_within_bounds() {
        assert_eq!(PERCENT.value_at(0.0), 75.0);
        assert_eq!(PERCENT.value_at(1.0), 150.0);
        assert_eq!(PERCENT.value_at(0.5), 115.0);
        assert_eq!(PERCENT.value_at(0.36), 100.0);
        assert_eq!(PERCENT.value_at(0.37), 105.0);
        assert_eq!(PERCENT.value_at(-0.4), 75.0);
        assert_eq!(PERCENT.value_at(1.7), 150.0);
    }

    #[test]
    fn values_map_to_track_fractions() {
        assert_eq!(PERCENT.fraction_of(75.0), 0.0);
        assert_eq!(PERCENT.fraction_of(150.0), 1.0);
        assert_eq!(PERCENT.fraction_of(112.5), 0.5);
        assert_eq!(PERCENT.fraction_of(10.0), 0.0);
        assert_eq!(PERCENT.fraction_of(400.0), 1.0);
        let editor = SliderRange { min: 10.0, max: 90.0, step: 5.0 };
        assert_eq!(editor.value_at(editor.fraction_of(40.0)), 40.0);
        assert_eq!(SliderRange { min: 1.0, max: 1.0, step: 1.0 }.fraction_of(1.0), 0.0);
    }
}
