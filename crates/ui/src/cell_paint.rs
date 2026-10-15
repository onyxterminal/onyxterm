use gpui::{FontId, GlyphId, LineLayout};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

const GLYPH_CACHE_LIMIT: usize = 4096;

pub type GlyphCache = Rc<RefCell<GlyphTable>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub ch: char,
    pub is_bold: bool,
    pub is_italic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellGlyph {
    pub font_id: FontId,
    pub glyph_id: GlyphId,
    pub is_emoji: bool,
}

impl CellGlyph {
    pub fn sole_glyph_of(layout: &LineLayout) -> Option<Self> {
        let mut glyphs = layout.runs.iter().flat_map(|run| {
            run.glyphs.iter().map(|glyph| Self { font_id: run.font_id, glyph_id: glyph.id, is_emoji: glyph.is_emoji })
        });
        let first = glyphs.next()?;
        glyphs.next().is_none().then_some(first)
    }
}

#[derive(Debug)]
pub struct GlyphTable<G = CellGlyph> {
    entries: HashMap<GlyphKey, Option<G>>,
}

impl<G> Default for GlyphTable<G> {
    fn default() -> Self {
        Self { entries: HashMap::new() }
    }
}

impl<G: Copy> GlyphTable<G> {
    pub fn get_or_resolve(&mut self, key: GlyphKey, resolve: impl FnOnce() -> Option<G>) -> Option<G> {
        if let Some(glyph) = self.entries.get(&key) {
            return *glyph;
        }
        if self.entries.len() >= GLYPH_CACHE_LIMIT {
            self.entries.clear();
        }
        *self.entries.entry(key).or_insert_with(resolve)
    }
}

pub fn merged_spans<T: PartialEq + Copy>(items: impl Iterator<Item = Option<T>>) -> Vec<(Range<usize>, T)> {
    let mut spans: Vec<(Range<usize>, T)> = Vec::new();
    for (col, item) in items.enumerate() {
        let Some(value) = item else {
            continue;
        };
        match spans.last_mut() {
            Some((span, last)) if span.end == col && *last == value => span.end = col + 1,
            _ => spans.push((col..col + 1, value)),
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(ch: char) -> GlyphKey {
        GlyphKey { ch, is_bold: false, is_italic: false }
    }

    #[test]
    fn glyph_table_resolves_each_key_once() {
        let mut table: GlyphTable<u32> = GlyphTable::default();
        let mut resolved = 0;
        for _ in 0..3 {
            let glyph = table.get_or_resolve(key('a'), || {
                resolved += 1;
                Some(7)
            });
            assert_eq!(glyph, Some(7));
        }
        assert_eq!(resolved, 1);
    }

    #[test]
    fn glyph_table_remembers_fallback_decisions() {
        let mut table: GlyphTable<u32> = GlyphTable::default();
        assert_eq!(table.get_or_resolve(key('x'), || None), None);
        assert_eq!(table.get_or_resolve(key('x'), || Some(1)), None);
    }

    #[test]
    fn glyph_table_separates_bold_from_regular() {
        let mut table: GlyphTable<u32> = GlyphTable::default();
        table.get_or_resolve(key('a'), || Some(1));
        let bold = table.get_or_resolve(GlyphKey { ch: 'a', is_bold: true, is_italic: false }, || Some(2));
        assert_eq!(bold, Some(2));
    }

    #[test]
    fn glyph_table_separates_italic_from_regular() {
        let mut table: GlyphTable<u32> = GlyphTable::default();
        table.get_or_resolve(key('a'), || Some(1));
        let italic = table.get_or_resolve(GlyphKey { ch: 'a', is_bold: false, is_italic: true }, || Some(3));
        assert_eq!(italic, Some(3));
    }

    #[test]
    fn glyph_table_stays_bounded() {
        let mut table: GlyphTable<u32> = GlyphTable::default();
        for code in 0..(GLYPH_CACHE_LIMIT as u32 + 10) {
            let ch = char::from_u32(0x4e00 + code).unwrap();
            table.get_or_resolve(key(ch), || Some(code));
        }
        assert!(table.entries.len() <= GLYPH_CACHE_LIMIT);
    }

    #[test]
    fn empty_layout_has_no_sole_glyph() {
        assert_eq!(CellGlyph::sole_glyph_of(&LineLayout::default()), None);
    }

    #[test]
    fn merged_spans_join_equal_neighbours() {
        let items = [None, Some(1), Some(1), Some(2), None, Some(2)];
        assert_eq!(merged_spans(items.into_iter()), vec![(1..3, 1), (3..4, 2), (5..6, 2)]);
    }

    #[test]
    fn merged_spans_of_nothing_is_empty() {
        assert!(merged_spans([None::<u8>, None].into_iter()).is_empty());
    }
}
