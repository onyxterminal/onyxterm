#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::cell_paint::{ligature_spans, merged_spans, CellGlyph, GlyphCache, GlyphKey};
use crate::input::wheel_bytes;
use crate::pixel_grid::{align_glyphs_to_cells, baseline_offset, glyph_aligned_origin, snap_cell_length, snap_point};
use crate::theme::Theme;
use gpui::{
    canvas, fill, font, outline, point, px, size, App, BorderStyle, Bounds, Canvas, ContentMask, DispatchPhase, Font,
    FontWeight, Hitbox, HitboxBehavior, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollWheelEvent, ShapedLine, SharedString, Size, Styled, TextRun, UnderlineStyle, Window,
};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use terminal_core::{Cell, CellColor, CursorPosition, GridPoint, GridSize, GridSnapshot, SelectionKind, Side, TerminalHandle};

const FALLBACK_CELL_WIDTH_RATIO: f32 = 0.6;
const SELECTION_ALPHA: f32 = 0.25;
const WORD_CLICK_COUNT: usize = 2;
const LINE_CLICK_COUNT: usize = 3;
const MIN_COLS: u16 = 2;
const MIN_ROWS: u16 = 1;
const ANSI_COLOR_COUNT: u8 = 16;
const CUBE_START: u8 = 16;
const GRAYSCALE_START: u8 = 232;
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
const REFERENCE_GLYPH: char = 'm';
const UNDERLINE_DESCENT_RATIO: f32 = 0.618;

type CellSizeCache = Rc<RefCell<Option<(f32, Size<Pixels>)>>>;

#[derive(Debug, Clone, Copy)]
pub struct TerminalColors {
    pub foreground: Hsla,
    pub background: Hsla,
    pub ansi: [Hsla; 16],
}

impl TerminalColors {
    pub fn from_theme(theme: &Theme) -> Self {
        let a = &theme.ansi;
        Self {
            foreground: theme.text.default,
            background: theme.surface.canvas,
            ansi: [
                a.black, a.red, a.green, a.yellow, a.blue, a.magenta, a.cyan, a.white,
                a.bright_black, a.bright_red, a.bright_green, a.bright_yellow, a.bright_blue,
                a.bright_magenta, a.bright_cyan, a.bright_white,
            ],
        }
    }

    pub fn foreground_of(&self, cell: &Cell) -> Hsla {
        if cell.is_inverse() {
            return self.resolve(cell.bg).unwrap_or(self.background);
        }
        self.resolve(cell.fg).unwrap_or(self.foreground)
    }

    pub fn background_of(&self, cell: &Cell) -> Option<Hsla> {
        if cell.is_inverse() {
            return Some(self.resolve(cell.fg).unwrap_or(self.foreground));
        }
        self.resolve(cell.bg)
    }

    fn resolve(&self, color: CellColor) -> Option<Hsla> {
        match color {
            CellColor::Default => None,
            CellColor::Indexed(index) if index < ANSI_COLOR_COUNT => Some(self.ansi[index as usize]),
            CellColor::Indexed(index) => Some(gpui::rgb(xterm_indexed_rgb(index)).into()),
            CellColor::Rgb(r, g, b) => Some(gpui::rgb(pack_rgb(r, g, b)).into()),
        }
    }
}

pub fn xterm_indexed_rgb(index: u8) -> u32 {
    if index >= GRAYSCALE_START {
        let level = 8 + (index - GRAYSCALE_START) * 10;
        return pack_rgb(level, level, level);
    }
    let cube = index.saturating_sub(CUBE_START);
    let level = |n: u8| CUBE_LEVELS[(n % 6) as usize];
    pack_rgb(level(cube / 36), level(cube / 6), level(cube))
}

fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

pub fn grid_size_for(area: Size<Pixels>, cell: Size<Pixels>) -> GridSize {
    let cols = (area.width / cell.width).floor().max(0.0) as u16;
    let rows = (area.height / cell.height).floor().max(0.0) as u16;
    GridSize::new(cols.max(MIN_COLS), rows.max(MIN_ROWS))
}

pub fn island_grid(island: Size<Pixels>, style: &TerminalStyle, cell: Size<Pixels>) -> GridSize {
    grid_size_for(content_bounds(Bounds::new(Point::default(), island), style.insets).size, cell)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContentInsets {
    pub x: Pixels,
    pub top: Pixels,
    pub bottom: Pixels,
}

impl ContentInsets {
    pub fn from_theme(theme: &Theme) -> Self {
        let island = &theme.island;
        Self {
            x: px(island.pane_padding_x_px),
            top: px(theme.layout.pane_header_height_px + island.pane_padding_top_px),
            bottom: px(island.pane_padding_bottom_px),
        }
    }
}

pub fn content_bounds(bounds: Bounds<Pixels>, insets: ContentInsets) -> Bounds<Pixels> {
    let width = (bounds.size.width - insets.x * 2.0).max(px(0.0));
    let height = (bounds.size.height - insets.top - insets.bottom).max(px(0.0));
    Bounds::new(bounds.origin + point(insets.x, insets.top), size(width, height))
}

fn device_aligned_content(bounds: Bounds<Pixels>, insets: ContentInsets, window: &Window) -> Bounds<Pixels> {
    let content = content_bounds(bounds, insets);
    Bounds::new(snap_point(content.origin, window.scale_factor()), content.size)
}

pub fn grid_point_at(position: Point<Pixels>, origin: Point<Pixels>, cell: Size<Pixels>, grid: GridSize) -> (GridPoint, Side) {
    let local = position - origin;
    let col_position = (local.x / cell.width).max(0.0);
    let row = (local.y / cell.height).floor().clamp(0.0, f32::from(grid.rows.saturating_sub(1))) as u16;
    let col = col_position.floor().min(f32::from(grid.cols.saturating_sub(1))) as u16;
    let side = if col_position.fract() < 0.5 { Side::Left } else { Side::Right };
    (GridPoint { row, col }, side)
}

pub fn split_scroll(accumulated: f32, line_height: f32) -> (i32, f32) {
    let lines = (accumulated / line_height).trunc();
    (lines as i32, accumulated - lines * line_height)
}

pub fn selection_kind_for(click_count: usize) -> SelectionKind {
    match click_count {
        WORD_CLICK_COUNT => SelectionKind::Word,
        count if count >= LINE_CLICK_COUNT => SelectionKind::Line,
        _ => SelectionKind::Simple,
    }
}

#[derive(Debug, Default)]
pub struct PointerState {
    scroll_remainder: f32,
    is_selecting: bool,
}

#[derive(Clone)]
pub struct TerminalPane {
    pub handle: TerminalHandle,
    pub pointer: Rc<RefCell<PointerState>>,
}

impl TerminalPane {
    pub fn new(handle: TerminalHandle) -> Self {
        Self { handle, pointer: Rc::default() }
    }
}

#[derive(Debug, Clone)]
pub struct TerminalStyle {
    pub colors: TerminalColors,
    pub font_family: SharedString,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub selection: Hsla,
    pub insets: ContentInsets,
    cell_size_cache: CellSizeCache,
    align_cache: crate::pixel_grid::AlignCache,
    glyph_cache: GlyphCache,
}

impl TerminalStyle {
    pub fn cell_size(&self, window: &Window) -> Size<Pixels> {
        let scale = window.scale_factor();
        if let Some((cached_scale, cached_size)) = *self.cell_size_cache.borrow() {
            if cached_scale == scale {
                return cached_size;
            }
        }
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&self.font());
        let width = text_system
            .advance(font_id, self.font_size, 'm')
            .map(|advance| advance.width)
            .unwrap_or(self.font_size * FALLBACK_CELL_WIDTH_RATIO);
        let resolved = size(snap_cell_length(width, scale), snap_cell_length(self.line_height, scale));
        *self.cell_size_cache.borrow_mut() = Some((scale, resolved));
        resolved
    }

    pub fn from_theme(theme: &Theme, font_family: String) -> Self {
        Self {
            colors: TerminalColors::from_theme(theme),
            font_family: font_family.into(),
            font_size: px(theme.font.size_px),
            line_height: px((theme.font.size_px * theme.font.line_height).round()),
            selection: theme.accent.primary.opacity(SELECTION_ALPHA),
            insets: ContentInsets::from_theme(theme),
            cell_size_cache: Rc::new(RefCell::new(None)),
            align_cache: Rc::default(),
            glyph_cache: Rc::default(),
        }
    }

    fn font(&self) -> Font {
        font(self.font_family.clone())
    }
}

pub struct Frame {
    snapshot: GridSnapshot,
    cell: Size<Pixels>,
}

pub type InputRegistration = Box<dyn FnOnce(Bounds<Pixels>, &mut Window, &mut App)>;

pub struct PaneFocus {
    pub marked_text: Option<String>,
    pub register_input: InputRegistration,
}

pub fn cursor_bounds(content: Bounds<Pixels>, cell: Size<Pixels>, cursor: CursorPosition) -> Bounds<Pixels> {
    let origin = content.origin + point(cell.width * f32::from(cursor.col), cell.height * f32::from(cursor.row));
    Bounds::new(origin, cell)
}

pub fn terminal_canvas(
    pane: TerminalPane,
    style: TerminalStyle,
    focus: Option<PaneFocus>,
) -> Canvas<(Frame, Hitbox)> {
    let prepare_style = style.clone();
    let prepare_handle = pane.handle.clone();
    canvas(
        move |bounds, window, _cx| {
            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            (prepare_frame(&prepare_handle, &prepare_style, window), hitbox)
        },
        move |bounds, (frame, hitbox), window, cx| {
            let bounds = device_aligned_content(bounds, style.insets, window);
            let is_focused = focus.is_some();
            let painter = FramePainter { frame: &frame, origin: bounds.origin, style: &style, is_focused };
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                painter.paint(window, cx);
                if let Some(text) = focus.as_ref().and_then(|focus| focus.marked_text.as_deref()) {
                    painter.paint_marked_text(text, window, cx);
                }
            });
            if let Some(focus) = focus {
                (focus.register_input)(bounds, window, cx);
            }
            let grid = GridSize::new(frame.snapshot.cols, frame.snapshot.rows);
            register_mouse_handlers(pane, PaneArea { bounds, cell: frame.cell, grid, hitbox }, window);
        },
    )
    .size_full()
}

pub fn terminal_preview(handle: TerminalHandle, style: TerminalStyle) -> Canvas<Frame> {
    let prepare_style = style.clone();
    canvas(
        move |_bounds, window, _cx| prepare_frame(&handle, &prepare_style, window),
        move |bounds, frame, window, cx| {
            let bounds = device_aligned_content(bounds, style.insets, window);
            let painter = FramePainter { frame: &frame, origin: bounds.origin, style: &style, is_focused: false };
            window.with_content_mask(Some(ContentMask { bounds }), |window| painter.paint(window, cx));
        },
    )
    .size_full()
}

#[derive(Clone)]
struct PaneArea {
    bounds: Bounds<Pixels>,
    cell: Size<Pixels>,
    grid: GridSize,
    hitbox: Hitbox,
}

impl PaneArea {
    fn point_at(&self, position: Point<Pixels>) -> (GridPoint, Side) {
        grid_point_at(position, self.bounds.origin, self.cell, self.grid)
    }
}

fn register_mouse_handlers(pane: TerminalPane, area: PaneArea, window: &mut Window) {
    let down = pane.clone();
    let down_area = area.clone();
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _cx| {
        let is_inside = down_area.bounds.contains(&event.position) && down_area.hitbox.is_hovered(window);
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left || !is_inside {
            return;
        }
        let (point, side) = down_area.point_at(event.position);
        down.handle.terminal().start_selection(selection_kind_for(event.click_count), point, side);
        down.pointer.borrow_mut().is_selecting = true;
        window.refresh();
    });
    let drag = pane.clone();
    let drag_area = area.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _cx| {
        let is_dragging = event.pressed_button == Some(MouseButton::Left) && drag.pointer.borrow().is_selecting;
        if phase != DispatchPhase::Bubble || !is_dragging {
            return;
        }
        let (point, side) = drag_area.point_at(event.position);
        drag.handle.terminal().update_selection(point, side);
        window.refresh();
    });
    let up = pane.clone();
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, _cx| {
        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
            up.pointer.borrow_mut().is_selecting = false;
        }
    });
    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, _cx| {
        let is_over_pane = area.bounds.contains(&event.position) && area.hitbox.should_handle_scroll(window);
        if phase != DispatchPhase::Bubble || !is_over_pane {
            return;
        }
        let delta = f32::from(event.delta.pixel_delta(area.cell.height).y);
        scroll_pane(&pane, delta, f32::from(area.cell.height));
        window.refresh();
    });
}

fn scroll_pane(pane: &TerminalPane, delta: f32, line_height: f32) {
    let mut pointer = pane.pointer.borrow_mut();
    let (lines, remainder) = split_scroll(pointer.scroll_remainder + delta, line_height);
    pointer.scroll_remainder = remainder;
    if lines == 0 {
        return;
    }
    let modes = pane.handle.input_modes();
    if !(modes.alt_screen || modes.mouse_reporting) {
        pane.handle.terminal().scroll_lines(lines);
        return;
    }
    if let Err(err) = pane.handle.write(&wheel_bytes(lines, modes)) {
        tracing::warn!(?err, "pty wheel write failed");
    }
}

fn prepare_frame(handle: &TerminalHandle, style: &TerminalStyle, window: &Window) -> Frame {
    Frame { snapshot: handle.snapshot_grid(), cell: style.cell_size(window) }
}

struct FramePainter<'a> {
    frame: &'a Frame,
    origin: Point<Pixels>,
    style: &'a TerminalStyle,
    is_focused: bool,
}

#[derive(Clone, Copy)]
struct TextMetrics {
    baseline: Pixels,
    underline_drop: Pixels,
}

struct PaintRow<'a> {
    index: usize,
    cells: &'a [Cell],
    baseline_origin: Point<Pixels>,
}

impl FramePainter<'_> {
    fn paint(&self, window: &mut Window, cx: &mut App) {
        for (row_index, row) in self.rows() {
            self.paint_backgrounds(row_index, row, window);
        }
        self.paint_selection(window);
        self.paint_cursor(window);
        let metrics = self.text_metrics(window);
        window.paint_layer(self.grid_bounds(), |window| {
            for (index, cells) in self.rows() {
                let baseline_origin = self.baseline_origin(index, metrics.baseline, window);
                let row = PaintRow { index, cells, baseline_origin };
                self.paint_underlines(&row, metrics.underline_drop, window);
                self.paint_text(&row, window, cx);
            }
        });
    }

    fn grid_bounds(&self) -> Bounds<Pixels> {
        let cell = self.frame.cell;
        let snapshot = &self.frame.snapshot;
        Bounds::new(self.origin, size(cell.width * f32::from(snapshot.cols), cell.height * f32::from(snapshot.rows)))
    }

    fn paint_marked_text(&self, text: &str, window: &mut Window, cx: &mut App) {
        let Some(cursor) = self.frame.snapshot.cursor else {
            return;
        };
        let width_in_cells = text.chars().count().max(1);
        let bounds = self.cell_bounds(usize::from(cursor.row), usize::from(cursor.col), width_in_cells);
        window.paint_quad(fill(bounds, self.style.colors.background));
        let underline = UnderlineStyle { thickness: px(1.0), color: None, wavy: false };
        let run = TextRun {
            len: text.len(),
            font: self.style.font(),
            color: self.style.colors.foreground,
            background_color: None,
            underline: Some(underline),
            strikethrough: None,
        };
        let line = window.text_system().shape_line(text.to_string().into(), self.style.font_size, &[run], None);
        let origin = self.glyph_origin(&line, bounds.origin, window);
        if let Err(err) = line.paint(origin, self.frame.cell.height, window, cx) {
            tracing::warn!(?err, "marked text paint failed");
        }
    }

    fn rows(&self) -> impl Iterator<Item = (usize, &[Cell])> {
        let cols = usize::from(self.frame.snapshot.cols.max(1));
        self.frame.snapshot.cells.chunks(cols).enumerate()
    }

    fn cell_bounds(&self, row: usize, col: usize, width_in_cells: usize) -> Bounds<Pixels> {
        let cell = self.frame.cell;
        let origin = self.origin + point(cell.width * col as f32, cell.height * row as f32);
        Bounds::new(origin, size(cell.width * width_in_cells as f32, cell.height))
    }

    fn paint_backgrounds(&self, row_index: usize, row: &[Cell], window: &mut Window) {
        let colors = self.style.colors;
        for (span, color) in merged_spans(row.iter().map(|cell| colors.background_of(cell))) {
            window.paint_quad(fill(self.cell_bounds(row_index, span.start, span.len()), color));
        }
    }

    fn paint_selection(&self, window: &mut Window) {
        let Some(span) = self.frame.snapshot.selection else {
            return;
        };
        let cols = self.frame.snapshot.cols;
        for row in span.start.row..=span.end.row {
            if let Some(range) = span.columns_on(row, cols) {
                let width = usize::from(range.end - range.start);
                let bounds = self.cell_bounds(usize::from(row), usize::from(range.start), width);
                window.paint_quad(fill(bounds, self.style.selection));
            }
        }
    }

    fn paint_cursor(&self, window: &mut Window) {
        let Some(cursor) = self.frame.snapshot.cursor else {
            return;
        };
        let bounds = self.cell_bounds(usize::from(cursor.row), usize::from(cursor.col), 1);
        let color = self.style.colors.foreground;
        if self.is_focused {
            window.paint_quad(fill(bounds, color));
            return;
        }
        window.paint_quad(outline(bounds, color, BorderStyle::Solid));
    }

    fn text_color(&self, row_index: usize, col: usize, cell: &Cell) -> Hsla {
        let cursor = self.frame.snapshot.cursor.filter(|_| self.is_focused);
        let is_cursor_cell = cursor.is_some_and(|c| usize::from(c.row) == row_index && usize::from(c.col) == col);
        if is_cursor_cell {
            return self.style.colors.background;
        }
        self.style.colors.foreground_of(cell)
    }

    fn text_metrics(&self, window: &Window) -> TextMetrics {
        let reference = REFERENCE_GLYPH.to_string();
        let run = self.text_run(FontWeight::NORMAL, self.style.colors.foreground, reference.len());
        let line = window.text_system().shape_line(reference.into(), self.style.font_size, &[run], None);
        TextMetrics {
            baseline: baseline_offset(&line, self.frame.cell.height),
            underline_drop: line.descent * UNDERLINE_DESCENT_RATIO,
        }
    }

    fn baseline_origin(&self, row_index: usize, baseline: Pixels, window: &Window) -> Point<Pixels> {
        let row_origin = self.origin + point(px(0.0), self.frame.cell.height * row_index as f32);
        glyph_aligned_origin(row_origin, baseline, window.scale_factor()) + point(px(0.0), baseline)
    }

    fn paint_underlines(&self, row: &PaintRow, underline_drop: Pixels, window: &mut Window) {
        let colors = row.cells.iter().enumerate().map(|(col, cell)| {
            cell.is_underline().then(|| self.text_color(row.index, col, cell))
        });
        let cell_width = self.frame.cell.width;
        for (span, color) in merged_spans(colors) {
            let origin = row.baseline_origin + point(cell_width * span.start as f32, underline_drop);
            let style = UnderlineStyle { thickness: px(1.0), color: Some(color), wavy: false };
            window.paint_underline(origin, cell_width * span.len() as f32, &style);
        }
    }

    fn paint_text(&self, row: &PaintRow, window: &mut Window, cx: &mut App) {
        if row.cells.iter().all(|cell| cell.ch <= ' ') {
            return;
        }
        let ligatures = ligature_spans(row.cells.iter().map(|cell| cell.ch));
        for span in &ligatures {
            self.paint_shaped(row, span.clone(), window, cx);
        }
        for (col, cell) in row.cells.iter().enumerate() {
            let is_shaped = ligatures.iter().any(|span| span.contains(&col));
            if cell.ch > ' ' && !is_shaped {
                self.paint_cell(row, col, window, cx);
            }
        }
    }

    fn paint_cell(&self, row: &PaintRow, col: usize, window: &mut Window, cx: &mut App) {
        let cell = &row.cells[col];
        let key = GlyphKey { ch: cell.ch, is_bold: cell.is_bold() };
        let glyph = self.style.glyph_cache.borrow_mut().get_or_resolve(key, || self.resolve_glyph(key, window));
        let Some(glyph) = glyph else {
            self.paint_shaped(row, col..col + 1, window, cx);
            return;
        };
        let origin = row.baseline_origin + point(self.frame.cell.width * col as f32, px(0.0));
        let font_size = self.style.font_size;
        let painted = if glyph.is_emoji {
            window.paint_emoji(origin, glyph.font_id, glyph.glyph_id, font_size)
        } else {
            let color = self.text_color(row.index, col, cell);
            window.paint_glyph(origin, glyph.font_id, glyph.glyph_id, font_size, color)
        };
        if let Err(err) = painted {
            tracing::warn!(?err, "terminal glyph paint failed");
        }
    }

    fn resolve_glyph(&self, key: GlyphKey, window: &Window) -> Option<CellGlyph> {
        let run = self.text_run(font_weight(key.is_bold), self.style.colors.foreground, key.ch.len_utf8());
        let line = window.text_system().shape_line(key.ch.to_string().into(), self.style.font_size, &[run], None);
        CellGlyph::sole_glyph_of(&line)
    }

    fn paint_shaped(&self, row: &PaintRow, span: Range<usize>, window: &mut Window, cx: &mut App) {
        let cell = self.frame.cell;
        let (text, runs) = self.text_runs(row, span.clone());
        let mut line = window.text_system().shape_line(text.into(), self.style.font_size, &runs, None);
        align_glyphs_to_cells(&mut line, cell.width, &self.style.align_cache);
        let span_origin = self.cell_bounds(row.index, span.start, span.len()).origin;
        let origin = self.glyph_origin(&line, span_origin, window);
        if let Err(err) = line.paint(origin, cell.height, window, cx) {
            tracing::warn!(?err, "terminal span paint failed");
        }
    }

    fn glyph_origin(&self, line: &ShapedLine, origin: Point<Pixels>, window: &Window) -> Point<Pixels> {
        glyph_aligned_origin(origin, baseline_offset(line, self.frame.cell.height), window.scale_factor())
    }

    fn text_runs(&self, row: &PaintRow, span: Range<usize>) -> (String, Vec<TextRun>) {
        let mut text = String::with_capacity(span.len());
        let mut runs: Vec<TextRun> = Vec::new();
        for col in span {
            let cell = &row.cells[col];
            let ch = if cell.ch < ' ' { ' ' } else { cell.ch };
            let run = self.text_run(font_weight(cell.is_bold()), self.text_color(row.index, col, cell), ch.len_utf8());
            text.push(ch);
            match runs.last_mut() {
                Some(last) if same_style(last, &run) => last.len += run.len,
                _ => runs.push(run),
            }
        }
        (text, runs)
    }

    fn text_run(&self, weight: FontWeight, color: Hsla, len: usize) -> TextRun {
        TextRun {
            len,
            font: Font { weight, ..self.style.font() },
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }
}

fn font_weight(is_bold: bool) -> FontWeight {
    if is_bold {
        return FontWeight::BOLD;
    }
    FontWeight::NORMAL
}

fn same_style(a: &TextRun, b: &TextRun) -> bool {
    a.color == b.color && a.font.weight == b.font.weight
}
