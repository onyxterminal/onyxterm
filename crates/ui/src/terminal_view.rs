#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::theme::Theme;
use gpui::{
    canvas, fill, font, outline, point, px, size, App, BorderStyle, Bounds, Canvas, Font, FontWeight, Hsla,
    Pixels, Point, Size, Styled, TextRun, UnderlineStyle, Window,
};
use terminal_core::{Cell, CellColor, GridSize, GridSnapshot, TerminalHandle};

const FONT_FAMILY: &str = "Menlo";
const FALLBACK_CELL_WIDTH_RATIO: f32 = 0.6;
const MIN_COLS: u16 = 2;
const MIN_ROWS: u16 = 1;
const ANSI_COLOR_COUNT: u8 = 16;
const CUBE_START: u8 = 16;
const GRAYSCALE_START: u8 = 232;
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

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

#[derive(Debug, Clone, Copy)]
pub struct TerminalStyle {
    pub colors: TerminalColors,
    pub font_size: Pixels,
    pub line_height: Pixels,
}

impl TerminalStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            colors: TerminalColors::from_theme(theme),
            font_size: px(theme.font.size_px),
            line_height: px((theme.font.size_px * theme.font.line_height).round()),
        }
    }

    fn cell_size(&self, window: &Window) -> Size<Pixels> {
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font(FONT_FAMILY));
        let width = text_system
            .advance(font_id, self.font_size, 'm')
            .map(|advance| advance.width)
            .unwrap_or(self.font_size * FALLBACK_CELL_WIDTH_RATIO);
        size(width, self.line_height)
    }
}

pub struct Frame {
    snapshot: GridSnapshot,
    cell: Size<Pixels>,
}

pub fn terminal_canvas(handle: TerminalHandle, style: TerminalStyle, is_focused: bool) -> Canvas<Frame> {
    canvas(
        move |bounds, window, _cx| prepare_frame(&handle, bounds, style, window),
        move |bounds, frame, window, cx| {
            let painter = FramePainter { frame: &frame, origin: bounds.origin, style, is_focused };
            painter.paint(window, cx);
        },
    )
    .size_full()
}

fn prepare_frame(handle: &TerminalHandle, bounds: Bounds<Pixels>, style: TerminalStyle, window: &Window) -> Frame {
    let cell = style.cell_size(window);
    let grid = grid_size_for(bounds.size, cell);
    if handle.grid_size() != grid {
        if let Err(err) = handle.resize(grid.rows, grid.cols) {
            tracing::warn!(?err, "terminal resize failed");
        }
    }
    Frame { snapshot: handle.snapshot_grid(), cell }
}

struct FramePainter<'a> {
    frame: &'a Frame,
    origin: Point<Pixels>,
    style: TerminalStyle,
    is_focused: bool,
}

impl FramePainter<'_> {
    fn paint(&self, window: &mut Window, cx: &mut App) {
        for (row_index, row) in self.rows() {
            self.paint_backgrounds(row_index, row, window);
        }
        self.paint_cursor(window);
        for (row_index, row) in self.rows() {
            self.paint_text(row_index, row, window, cx);
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
        let mut col = 0;
        while col < row.len() {
            let color = colors.background_of(&row[col]);
            let span = row[col..].iter().take_while(|c| colors.background_of(c) == color).count();
            if let Some(color) = color {
                window.paint_quad(fill(self.cell_bounds(row_index, col, span), color));
            }
            col += span;
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

    fn cursor_col_on(&self, row_index: usize) -> Option<usize> {
        let cursor = self.frame.snapshot.cursor.filter(|_| self.is_focused)?;
        (usize::from(cursor.row) == row_index).then_some(usize::from(cursor.col))
    }

    fn paint_text(&self, row_index: usize, row: &[Cell], window: &mut Window, cx: &mut App) {
        let (text, runs) = self.text_runs(row_index, row);
        if text.trim_end().is_empty() {
            return;
        }
        let cell = self.frame.cell;
        let line = window.text_system().shape_line(text.into(), self.style.font_size, &runs, Some(cell.width));
        let origin = self.origin + point(px(0.0), cell.height * row_index as f32);
        if let Err(err) = line.paint(origin, cell.height, window, cx) {
            tracing::warn!(?err, "terminal row paint failed");
        }
    }

    fn text_runs(&self, row_index: usize, row: &[Cell]) -> (String, Vec<TextRun>) {
        let cursor_col = self.cursor_col_on(row_index);
        let mut text = String::with_capacity(row.len());
        let mut runs: Vec<TextRun> = Vec::new();
        for (col, cell) in row.iter().enumerate() {
            let ch = if cell.ch < ' ' { ' ' } else { cell.ch };
            let color = if cursor_col == Some(col) {
                self.style.colors.background
            } else {
                self.style.colors.foreground_of(cell)
            };
            let run = cell_run(cell, color, ch.len_utf8());
            text.push(ch);
            match runs.last_mut() {
                Some(last) if same_style(last, &run) => last.len += run.len,
                _ => runs.push(run),
            }
        }
        (text, runs)
    }
}

fn cell_run(cell: &Cell, color: Hsla, len: usize) -> TextRun {
    let weight = if cell.is_bold() { FontWeight::BOLD } else { FontWeight::NORMAL };
    let underline = cell.is_underline().then_some(UnderlineStyle { thickness: px(1.0), color: None, wavy: false });
    TextRun {
        len,
        font: Font { weight, ..font(FONT_FAMILY) },
        color,
        background_color: None,
        underline,
        strikethrough: None,
    }
}

fn same_style(a: &TextRun, b: &TextRun) -> bool {
    a.color == b.color && a.font.weight == b.font.weight && a.underline == b.underline
}
