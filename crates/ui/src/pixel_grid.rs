use gpui::{point, px, size, Bounds, LineLayout, Pixels, Point, ShapedLine};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::sync::Arc;

const GLYPH_PHASE_BIAS_DEVICE_PX: f32 = 1.0 / 64.0;
const MIN_CELL_DEVICE_PX: f32 = 1.0;
const ALIGN_CACHE_LIMIT: usize = 1024;

pub type AlignCache = RefCell<HashMap<(u64, Pixels), Arc<LineLayout>>>;

pub fn snap_to_device(value: Pixels, scale: f32) -> Pixels {
    px((f32::from(value) * scale).round() / scale)
}

pub fn snap_cell_length(length: Pixels, scale: f32) -> Pixels {
    let device = (f32::from(length) * scale).round().max(MIN_CELL_DEVICE_PX);
    px(device / scale)
}

pub fn snap_point(position: Point<Pixels>, scale: f32) -> Point<Pixels> {
    point(snap_to_device(position.x, scale), snap_to_device(position.y, scale))
}

pub fn snap_bounds(bounds: Bounds<Pixels>, scale: f32) -> Bounds<Pixels> {
    let origin = snap_point(bounds.origin, scale);
    let corner = snap_point(bounds.bottom_right(), scale);
    Bounds::new(origin, size(corner.x - origin.x, corner.y - origin.y))
}

pub fn baseline_offset(layout: &LineLayout, line_height: Pixels) -> Pixels {
    (line_height - layout.ascent - layout.descent) / 2.0 + layout.ascent
}

pub fn glyph_aligned_origin(origin: Point<Pixels>, baseline: Pixels, scale: f32) -> Point<Pixels> {
    let bias = px(GLYPH_PHASE_BIAS_DEVICE_PX / scale);
    let snapped_baseline = snap_to_device(origin.y + baseline, scale) + bias;
    point(snap_to_device(origin.x, scale) + bias, snapped_baseline - baseline)
}

pub fn align_glyphs_to_cells(line: &mut ShapedLine, cell_width: Pixels, cache: &AlignCache) {
    let mut hasher = DefaultHasher::new();
    hasher.write(line.text.as_bytes());
    let key = (hasher.finish(), cell_width);
    if let Some(aligned) = cache.borrow().get(&key).cloned() {
        let layout: &mut Arc<LineLayout> = line;
        *layout = aligned;
        return;
    }
    let char_starts: Vec<usize> = line.text.char_indices().map(|(byte, _)| byte).collect();
    let column_of = |byte: usize| char_starts.partition_point(|start| *start < byte);
    let mut runs = line.runs.clone();
    for glyph in runs.iter_mut().flat_map(|run| run.glyphs.iter_mut()) {
        glyph.position.x = cell_width * column_of(glyph.index) as f32;
    }
    let aligned = LineLayout {
        font_size: line.font_size,
        width: cell_width * char_starts.len() as f32,
        ascent: line.ascent,
        descent: line.descent,
        runs,
        len: line.len,
    };
    let layout_arc = Arc::new(aligned);
    {
        let layout: &mut Arc<LineLayout> = line;
        *layout = layout_arc.clone();
    }
    let mut cache_mut = cache.borrow_mut();
    if cache_mut.len() >= ALIGN_CACHE_LIMIT {
        cache_mut.clear();
    }
    cache_mut.insert(key, layout_arc);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCALES: [f32; 3] = [1.0, 1.5, 2.0];
    const FONT_SIZES: [f32; 3] = [12.0, 13.0, 14.0];
    const LINE_HEIGHTS: [f32; 2] = [1.2, 1.4];
    const MONO_ADVANCE_RATIO: f32 = 0.6;
    const ASCENT_RATIO: f32 = 1.02;
    const DESCENT_RATIO: f32 = 0.3;
    const EPSILON: f32 = 1e-3;

    fn device(value: Pixels, scale: f32) -> f32 {
        f32::from(value) * scale
    }

    fn is_whole(value: f32) -> bool {
        (value - value.round()).abs() < EPSILON
    }

    fn glyph_phase(value: f32) -> u8 {
        (value.fract() * 4.0).floor() as u8
    }

    fn layout_for(font_size: f32) -> LineLayout {
        LineLayout {
            ascent: px(font_size * ASCENT_RATIO),
            descent: px(font_size * DESCENT_RATIO),
            ..Default::default()
        }
    }

    #[test]
    fn cell_lengths_land_on_whole_device_pixels() {
        for scale in SCALES {
            for font_size in FONT_SIZES {
                let width = snap_cell_length(px(font_size * MONO_ADVANCE_RATIO), scale);
                assert!(is_whole(device(width, scale)), "width {width:?} at {scale}x");
                for ratio in LINE_HEIGHTS {
                    let height = snap_cell_length(px(font_size * ratio), scale);
                    assert!(is_whole(device(height, scale)), "height {height:?} at {scale}x");
                }
            }
        }
    }

    #[test]
    fn cell_length_rounds_to_nearest_device_pixel() {
        assert_eq!(snap_cell_length(px(7.8), 1.0), px(8.0));
        assert_eq!(snap_cell_length(px(7.8), 2.0), px(8.0));
        assert_eq!(snap_cell_length(px(8.4), 1.0), px(8.0));
        assert_eq!(snap_cell_length(px(8.4), 2.0), px(8.5));
        assert_eq!(snap_cell_length(px(18.2), 1.0), px(18.0));
        assert_eq!(snap_cell_length(px(0.1), 1.0), px(1.0));
    }

    #[test]
    fn bounds_snap_both_edges_to_device_grid() {
        let bounds = Bounds::new(point(px(10.3), px(4.6)), size(px(7.8), px(18.2)));
        let snapped = snap_bounds(bounds, 1.0);
        assert_eq!(snapped.origin, point(px(10.0), px(5.0)));
        assert_eq!(snapped.bottom_right(), point(px(18.0), px(23.0)));
    }

    #[test]
    fn glyph_origin_puts_baseline_and_x_on_phase_zero() {
        for scale in SCALES {
            for font_size in FONT_SIZES {
                for ratio in LINE_HEIGHTS {
                    let layout = layout_for(font_size);
                    let line_height = snap_cell_length(px(font_size * ratio), scale);
                    let baseline = baseline_offset(&layout, line_height);
                    for row in 0..40 {
                        let row_top = px(37.3) + line_height * row as f32;
                        let origin = glyph_aligned_origin(point(px(101.7), row_top), baseline, scale);
                        let baseline_device = device(origin.y + baseline, scale);
                        let x_device = device(origin.x, scale);
                        assert_eq!(glyph_phase(baseline_device), 0, "baseline {baseline_device} at {scale}x");
                        assert_eq!(glyph_phase(x_device), 0, "x {x_device} at {scale}x");
                        assert!((origin.y - row_top).abs() < px(1.0));
                    }
                }
            }
        }
    }

    #[test]
    fn unaligned_baseline_would_land_on_a_fractional_phase() {
        let layout = layout_for(13.0);
        let baseline = baseline_offset(&layout, px(18.0));
        assert_ne!(glyph_phase(f32::from(baseline)), 0);
    }

    #[test]
    fn cell_columns_keep_glyphs_on_phase_zero_at_every_scale() {
        for scale in SCALES {
            for font_size in FONT_SIZES {
                let cell = snap_cell_length(px(font_size * MONO_ADVANCE_RATIO), scale);
                let origin = glyph_aligned_origin(point(px(12.3), px(0.0)), px(0.0), scale);
                for column in 0..300 {
                    let x_device = device(origin.x + cell * column as f32, scale);
                    assert_eq!(glyph_phase(x_device), 0, "column {column} at {scale}x, cell {cell:?}");
                }
            }
        }
    }
}
