#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::cell_paint::{merged_spans, CellGlyph, GlyphCache, GlyphKey};
use crate::file_links::FileLink;
use crate::input::wheel_bytes;
use crate::terminal_annotations::{Annotations, RowLabel, RowLink};
use crate::pixel_grid::{
    align_glyphs_to_cells, baseline_offset, glyph_aligned_origin, snap_cell_length, snap_point, snap_to_device,
};
use crate::theme::Theme;
use gpui::{
    canvas, fill, font, point, px, size, App, Bounds, Canvas, ContentMask, CursorStyle, DispatchPhase, Font,
    FontStyle, FontWeight, Hitbox, HitboxBehavior, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, ScrollWheelEvent, ShapedLine, SharedString, Size, StrikethroughStyle, Styled, TextRun,
    UnderlineStyle, Window,
};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use terminal_core::{Cell, CellColor, CursorPosition, GridPoint, GridSize, GridSnapshot, SelectionKind, Side, TerminalHandle};

const FALLBACK_CELL_WIDTH_RATIO: f32 = 0.6;
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
const STRIKE_RISE_RATIO: f32 = 0.27;
const DIM_TEXT_OPACITY: f32 = 0.66;
const DECORATION_THICKNESS_PX: f32 = 1.0;
const CARET_WIDTH_PX: f32 = 1.0;
const COPY_LINE_BLEED_PX: f32 = 4.0;
const COPY_LINE_RADIUS_PX: f32 = 3.0;
const LABEL_TEXT_PX: f32 = 10.5;
const LABEL_GAP_PX: f32 = 12.0;
const GHOST_OPACITY: f32 = 0.7;
const KEYCAP_TEXT: &str = "→";
const KEYCAP_GAP_PX: f32 = 8.0;
const KEYCAP_PADDING_X_PX: f32 = 4.0;
const KEYCAP_LINE_HEIGHT_PX: f32 = 16.0;
const KEYCAP_BORDER_PX: f32 = 1.0;
const KEYCAP_RADIUS_PX: f32 = 4.0;

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
    hovered_link: Option<RowLink>,
    pressed_link: Option<RowLink>,
}

pub type LinkOpener = Rc<dyn Fn(&FileLink, &mut Window, &mut App)>;

#[derive(Clone)]
pub struct TerminalPane {
    pub handle: TerminalHandle,
    pub pointer: Rc<RefCell<PointerState>>,
    opener: LinkOpener,
}

impl TerminalPane {
    pub fn new(handle: TerminalHandle, opener: LinkOpener) -> Self {
        Self { handle, pointer: Rc::default(), opener }
    }
}

#[derive(Debug, Clone)]
pub struct AnnotationColors {
    pub link: Hsla,
    pub quiet: Hsla,
    pub failure: Hsla,
    pub keycap_border: Hsla,
    pub ui_font_family: SharedString,
}

impl AnnotationColors {
    fn from_theme(theme: &Theme) -> Self {
        Self {
            link: theme.ansi.blue,
            quiet: theme.text.subtle,
            failure: theme.status.failed,
            keycap_border: theme.border.default,
            ui_font_family: theme.ui_font.family.clone().into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TerminalStyle {
    pub colors: TerminalColors,
    pub font_family: SharedString,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub selection: Hsla,
    pub caret: Hsla,
    pub annotations: AnnotationColors,
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
            selection: theme.accent.selection,
            caret: theme.accent.primary,
            annotations: AnnotationColors::from_theme(theme),
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
    annotations: Annotations,
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
    let is_focused = focus.is_some();
    canvas(
        move |bounds, window, _cx| {
            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            (prepare_frame(&prepare_handle, &prepare_style, is_focused, window), hitbox)
        },
        move |bounds, (frame, hitbox), window, cx| {
            let bounds = device_aligned_content(bounds, style.insets, window);
            let hovered_link = pane.pointer.borrow().hovered_link.clone();
            if hovered_link.is_some() {
                window.set_cursor_style(CursorStyle::PointingHand, &hitbox);
            }
            let painter = FramePainter { frame: &frame, bounds, style: &style, is_focused, hovered_link };
            painter.paint_copy_line(window);
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
            let links = Rc::new(frame.annotations.links);
            register_mouse_handlers(pane, PaneArea { bounds, cell: frame.cell, grid, hitbox, links }, window);
        },
    )
    .size_full()
}

pub fn terminal_preview(handle: TerminalHandle, style: TerminalStyle) -> Canvas<Frame> {
    let prepare_style = style.clone();
    canvas(
        move |_bounds, window, _cx| prepare_frame(&handle, &prepare_style, false, window),
        move |bounds, frame, window, cx| {
            let bounds = device_aligned_content(bounds, style.insets, window);
            let painter = FramePainter { frame: &frame, bounds, style: &style, is_focused: false, hovered_link: None };
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
    links: Rc<Vec<RowLink>>,
}

impl PaneArea {
    fn point_at(&self, position: Point<Pixels>) -> (GridPoint, Side) {
        grid_point_at(position, self.bounds.origin, self.cell, self.grid)
    }

    fn link_at(&self, position: Point<Pixels>, window: &Window) -> Option<RowLink> {
        if !self.bounds.contains(&position) || !self.hitbox.is_hovered(window) {
            return None;
        }
        let (point, _) = self.point_at(position);
        let (row, col) = (usize::from(point.row), usize::from(point.col));
        self.links.iter().find(|found| found.row == row && found.link.columns.contains(&col)).cloned()
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
        let mut pointer = down.pointer.borrow_mut();
        pointer.is_selecting = true;
        pointer.pressed_link = down_area.link_at(event.position, window);
        window.refresh();
    });
    let drag = pane.clone();
    let drag_area = area.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _cx| {
        if phase != DispatchPhase::Bubble {
            return;
        }
        track_hovered_link(&drag, drag_area.link_at(event.position, window), window);
        let is_dragging = event.pressed_button == Some(MouseButton::Left) && drag.pointer.borrow().is_selecting;
        if !is_dragging {
            return;
        }
        let (point, side) = drag_area.point_at(event.position);
        drag.handle.terminal().update_selection(point, side);
        window.refresh();
    });
    let up = pane.clone();
    let up_area = area.clone();
    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
            return;
        }
        let pressed = {
            let mut pointer = up.pointer.borrow_mut();
            pointer.is_selecting = false;
            pointer.pressed_link.take()
        };
        let released = up_area.link_at(event.position, window);
        if let Some(link) = pressed.filter(|pressed| Some(pressed) == released.as_ref()) {
            up.handle.terminal().clear_selection();
            (up.opener)(&link.link, window, cx);
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

fn track_hovered_link(pane: &TerminalPane, hovered: Option<RowLink>, window: &mut Window) {
    let mut pointer = pane.pointer.borrow_mut();
    if pointer.hovered_link != hovered {
        pointer.hovered_link = hovered;
        window.refresh();
    }
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

fn prepare_frame(handle: &TerminalHandle, style: &TerminalStyle, is_focused: bool, window: &Window) -> Frame {
    let snapshot = handle.snapshot_grid();
    let annotations = Annotations::read(&snapshot, handle.terminal(), is_focused);
    Frame { snapshot, cell: style.cell_size(window), annotations }
}

struct FramePainter<'a> {
    frame: &'a Frame,
    bounds: Bounds<Pixels>,
    style: &'a TerminalStyle,
    is_focused: bool,
    hovered_link: Option<RowLink>,
}

#[derive(Clone, Copy)]
struct TextMetrics {
    baseline: Pixels,
    ascent: Pixels,
    descent: Pixels,
    underline_drop: Pixels,
    strike_rise: Pixels,
}

struct PaintRow<'a> {
    index: usize,
    cells: &'a [Cell],
    baseline_origin: Point<Pixels>,
    links: Vec<Range<usize>>,
}

impl PaintRow<'_> {
    fn is_link(&self, col: usize) -> bool {
        self.links.iter().any(|columns| columns.contains(&col))
    }
}

impl FramePainter<'_> {
    fn paint(&self, window: &mut Window, cx: &mut App) {
        for (row_index, row) in self.rows() {
            self.paint_backgrounds(row_index, row, window);
        }
        self.paint_selection(window);
        let metrics = self.text_metrics(window);
        self.paint_caret(metrics, window);
        window.paint_layer(self.grid_bounds(), |window| {
            for (index, cells) in self.rows() {
                let baseline_origin = self.baseline_origin(index, metrics.baseline, window);
                let links = self.frame.annotations.link_columns_on(index).collect();
                let row = PaintRow { index, cells, baseline_origin, links };
                self.paint_underlines(&row, metrics.underline_drop, window);
                self.paint_text(&row, window, cx);
                self.paint_strikethroughs(&row, metrics.strike_rise, window);
            }
            self.paint_hovered_link(metrics, window);
            self.paint_ghost(metrics, window, cx);
            self.paint_block_labels(metrics, window, cx);
        });
    }

    fn grid_bounds(&self) -> Bounds<Pixels> {
        let cell = self.frame.cell;
        let snapshot = &self.frame.snapshot;
        Bounds::new(self.bounds.origin, size(cell.width * f32::from(snapshot.cols), cell.height * f32::from(snapshot.rows)))
    }

    fn paint_marked_text(&self, text: &str, window: &mut Window, cx: &mut App) {
        let Some(cursor) = self.frame.snapshot.cursor else {
            return;
        };
        let width_in_cells = text.chars().count().max(1);
        let bounds = self.cell_bounds(usize::from(cursor.row), usize::from(cursor.col), width_in_cells);
        window.paint_quad(fill(bounds, self.style.colors.background));
        let underline = UnderlineStyle { thickness: px(DECORATION_THICKNESS_PX), color: None, wavy: false };
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
        let origin = self.bounds.origin + point(cell.width * col as f32, cell.height * row as f32);
        Bounds::new(origin, size(cell.width * width_in_cells as f32, cell.height))
    }

    fn paint_backgrounds(&self, row_index: usize, row: &[Cell], window: &mut Window) {
        let colors = self.style.colors;
        for (span, color) in merged_spans(row.iter().map(|cell| colors.background_of(cell))) {
            window.paint_quad(fill(self.cell_bounds(row_index, span.start, span.len()), color));
        }
    }

    fn paint_copy_line(&self, window: &mut Window) {
        let snapshot = &self.frame.snapshot;
        let Some(cursor) = snapshot.cursor.filter(|_| snapshot.is_vi_mode) else {
            return;
        };
        let row = self.cell_bounds(usize::from(cursor.row), 0, usize::from(snapshot.cols));
        let bleed = px(COPY_LINE_BLEED_PX);
        let bounds = Bounds::new(row.origin - point(bleed, px(0.0)), row.size + size(bleed * 2.0, px(0.0)));
        window.paint_quad(fill(bounds, self.style.selection).corner_radii(px(COPY_LINE_RADIUS_PX)));
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

    fn paint_caret(&self, metrics: TextMetrics, window: &mut Window) {
        let Some(cursor) = self.frame.snapshot.cursor.filter(|_| self.is_focused) else {
            return;
        };
        let cell = self.cell_bounds(usize::from(cursor.row), usize::from(cursor.col), 1);
        let row_origin = self.baseline_origin(usize::from(cursor.row), metrics.baseline, window);
        let top = row_origin.y - metrics.ascent;
        let origin = point(snap_to_device(cell.origin.x, window.scale_factor()), top);
        let bounds = Bounds::new(origin, size(px(CARET_WIDTH_PX), metrics.ascent + metrics.descent));
        window.paint_quad(fill(bounds, self.style.caret));
    }

    fn cell_color(&self, row: &PaintRow, col: usize, cell: &Cell) -> Hsla {
        if row.is_link(col) {
            return self.style.annotations.link;
        }
        self.text_color(cell)
    }

    fn paint_hovered_link(&self, metrics: TextMetrics, window: &mut Window) {
        let Some(hovered) = self.hovered_link.as_ref() else {
            return;
        };
        let columns = &hovered.link.columns;
        let row_origin = self.baseline_origin(hovered.row, metrics.baseline, window);
        let origin = row_origin + point(self.frame.cell.width * columns.start as f32, metrics.underline_drop);
        let color = Some(self.style.annotations.link);
        let style = UnderlineStyle { thickness: px(DECORATION_THICKNESS_PX), color, wavy: false };
        window.paint_underline(origin, self.frame.cell.width * columns.len() as f32, &style);
    }

    fn paint_ghost(&self, metrics: TextMetrics, window: &mut Window, cx: &mut App) {
        let (Some(ghost), Some(cursor)) = (self.frame.annotations.ghost.as_deref(), self.frame.snapshot.cursor) else {
            return;
        };
        let (row, col) = (usize::from(cursor.row), usize::from(cursor.col));
        let room = usize::from(self.frame.snapshot.cols).saturating_sub(col);
        let shown: String = ghost.chars().take(room).collect();
        if shown.is_empty() {
            return;
        }
        let annotations = &self.style.annotations;
        let color = annotations.quiet.opacity(GHOST_OPACITY);
        let run = self.text_run(GlyphStyle::default(), color, shown.len());
        let width = shown.chars().count();
        let mut line = window.text_system().shape_line(shown.into(), self.style.font_size, &[run], None);
        align_glyphs_to_cells(&mut line, self.frame.cell.width, &self.style.align_cache);
        let origin = self.glyph_origin(&line, self.cell_bounds(row, col, width).origin, window);
        if let Err(err) = line.paint(origin, self.frame.cell.height, window, cx) {
            tracing::warn!(?err, "ghost suggestion paint failed");
        }
        let keycap_left = self.cell_bounds(row, col + width, 0).origin.x + px(KEYCAP_GAP_PX);
        self.paint_keycap(keycap_left, self.baseline_origin(row, metrics.baseline, window).y, window, cx);
    }

    fn paint_keycap(&self, left: Pixels, baseline: Pixels, window: &mut Window, cx: &mut App) {
        let annotations = &self.style.annotations;
        let line = self.shape_ui_text(KEYCAP_TEXT, annotations.quiet, window);
        let padding = px(KEYCAP_PADDING_X_PX);
        let border = px(KEYCAP_BORDER_PX);
        let width = line.width + padding * 2.0 + border * 2.0;
        if left + width > self.bounds.right() {
            return;
        }
        let line_height = px(KEYCAP_LINE_HEIGHT_PX);
        let text_baseline = border + baseline_offset(&line, line_height);
        let top = baseline - text_baseline;
        let bounds = Bounds::new(point(left, top), size(width, line_height + border * 2.0));
        let quad = gpui::quad(bounds, px(KEYCAP_RADIUS_PX), gpui::transparent_black(), border, annotations.keycap_border,
            gpui::BorderStyle::Solid);
        window.paint_quad(quad);
        if let Err(err) = line.paint(point(left + border + padding, top + border), line_height, window, cx) {
            tracing::warn!(?err, "suggestion keycap paint failed");
        }
    }

    fn paint_block_labels(&self, metrics: TextMetrics, window: &mut Window, cx: &mut App) {
        for RowLabel { row, label } in &self.frame.annotations.labels {
            let annotations = &self.style.annotations;
            let color = if label.is_failure { annotations.failure } else { annotations.quiet };
            let line = self.shape_ui_text(&label.text, color, window);
            let left = self.bounds.right() - line.width;
            if self.row_text_end(*row) + px(LABEL_GAP_PX) > left {
                continue;
            }
            let baseline = self.baseline_origin(*row, metrics.baseline, window).y;
            let line_height = self.frame.cell.height;
            let origin = point(left, baseline - baseline_offset(&line, line_height));
            if let Err(err) = line.paint(origin, line_height, window, cx) {
                tracing::warn!(?err, "command block label paint failed");
            }
        }
    }

    fn row_text_end(&self, row: usize) -> Pixels {
        let cols = usize::from(self.frame.snapshot.cols.max(1));
        let cells = &self.frame.snapshot.cells[row * cols..(row + 1) * cols];
        let used = cells.iter().rposition(|cell| cell.ch > ' ').map_or(0, |last| last + 1);
        self.cell_bounds(row, used, 0).origin.x
    }

    fn shape_ui_text(&self, text: &str, color: Hsla, window: &Window) -> ShapedLine {
        let run = TextRun {
            len: text.len(),
            font: font(self.style.annotations.ui_font_family.clone()),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window.text_system().shape_line(text.to_string().into(), px(LABEL_TEXT_PX), &[run], None)
    }

    fn text_color(&self, cell: &Cell) -> Hsla {
        let color = self.style.colors.foreground_of(cell);
        if cell.is_dim() {
            return color.opacity(DIM_TEXT_OPACITY);
        }
        color
    }

    fn text_metrics(&self, window: &Window) -> TextMetrics {
        let reference = REFERENCE_GLYPH.to_string();
        let run = self.text_run(GlyphStyle::default(), self.style.colors.foreground, reference.len());
        let line = window.text_system().shape_line(reference.into(), self.style.font_size, &[run], None);
        TextMetrics {
            baseline: baseline_offset(&line, self.frame.cell.height),
            ascent: line.ascent,
            descent: line.descent,
            underline_drop: line.descent * UNDERLINE_DESCENT_RATIO,
            strike_rise: self.style.font_size * STRIKE_RISE_RATIO,
        }
    }

    fn baseline_origin(&self, row_index: usize, baseline: Pixels, window: &Window) -> Point<Pixels> {
        let row_origin = self.bounds.origin + point(px(0.0), self.frame.cell.height * row_index as f32);
        glyph_aligned_origin(row_origin, baseline, window.scale_factor()) + point(px(0.0), baseline)
    }

    fn paint_underlines(&self, row: &PaintRow, underline_drop: Pixels, window: &mut Window) {
        let colors = row.cells.iter().enumerate().map(|(col, cell)| {
            cell.is_underline().then(|| self.cell_color(row, col, cell))
        });
        let cell_width = self.frame.cell.width;
        for (span, color) in merged_spans(colors) {
            let origin = row.baseline_origin + point(cell_width * span.start as f32, underline_drop);
            let style = UnderlineStyle { thickness: px(DECORATION_THICKNESS_PX), color: Some(color), wavy: false };
            window.paint_underline(origin, cell_width * span.len() as f32, &style);
        }
    }

    fn paint_strikethroughs(&self, row: &PaintRow, strike_rise: Pixels, window: &mut Window) {
        let colors = row.cells.iter().enumerate().map(|(col, cell)| {
            cell.is_strikeout().then(|| self.cell_color(row, col, cell))
        });
        let cell_width = self.frame.cell.width;
        for (span, color) in merged_spans(colors) {
            let origin = row.baseline_origin + point(cell_width * span.start as f32, -strike_rise);
            let style = StrikethroughStyle { thickness: px(DECORATION_THICKNESS_PX), color: Some(color) };
            window.paint_strikethrough(origin, cell_width * span.len() as f32, &style);
        }
    }

    fn paint_text(&self, row: &PaintRow, window: &mut Window, cx: &mut App) {
        if row.cells.iter().all(|cell| cell.ch <= ' ') {
            return;
        }
        for (col, cell) in row.cells.iter().enumerate() {
            if cell.ch > ' ' {
                self.paint_cell(row, col, window, cx);
            }
        }
    }

    fn paint_cell(&self, row: &PaintRow, col: usize, window: &mut Window, cx: &mut App) {
        let cell = &row.cells[col];
        let key = GlyphKey { ch: cell.ch, is_bold: cell.is_bold(), is_italic: cell.is_italic() };
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
            let color = self.cell_color(row, col, cell);
            window.paint_glyph(origin, glyph.font_id, glyph.glyph_id, font_size, color)
        };
        if let Err(err) = painted {
            tracing::warn!(?err, "terminal glyph paint failed");
        }
    }

    fn resolve_glyph(&self, key: GlyphKey, window: &Window) -> Option<CellGlyph> {
        let style = GlyphStyle { is_bold: key.is_bold, is_italic: key.is_italic };
        let run = self.text_run(style, self.style.colors.foreground, key.ch.len_utf8());
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
            let run = self.text_run(GlyphStyle::of(cell), self.cell_color(row, col, cell), ch.len_utf8());
            text.push(ch);
            match runs.last_mut() {
                Some(last) if same_style(last, &run) => last.len += run.len,
                _ => runs.push(run),
            }
        }
        (text, runs)
    }

    fn text_run(&self, style: GlyphStyle, color: Hsla, len: usize) -> TextRun {
        TextRun {
            len,
            font: Font { weight: style.weight(), style: style.font_style(), ..self.style.font() },
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct GlyphStyle {
    is_bold: bool,
    is_italic: bool,
}

impl GlyphStyle {
    fn of(cell: &Cell) -> Self {
        Self { is_bold: cell.is_bold(), is_italic: cell.is_italic() }
    }

    fn weight(self) -> FontWeight {
        if self.is_bold {
            return FontWeight::BOLD;
        }
        FontWeight::NORMAL
    }

    fn font_style(self) -> FontStyle {
        if self.is_italic {
            return FontStyle::Italic;
        }
        FontStyle::Normal
    }
}

fn same_style(a: &TextRun, b: &TextRun) -> bool {
    a.color == b.color && a.font.weight == b.font.weight && a.font.style == b.font.style
}
