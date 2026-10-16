#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::cell_paint::{merged_spans, CellGlyph, GlyphCache, GlyphKey};
use crate::file_links::FileLink;
use crate::input::{paste_bytes, wheel_bytes};
use crate::mouse_report::{mouse_report_bytes, wheel_report_bytes, MouseAction, MouseReport, ReportedButton};
use crate::terminal_annotations::{Annotations, RowLabel, RowLink};
use crate::pixel_grid::{
    align_glyphs_to_cells, baseline_offset, glyph_aligned_origin, snap_cell_length, snap_point, snap_to_device,
};
use crate::theme::{FindTokens, InputTokens, Theme};
use gpui::{
    canvas, fill, font, point, px, size, App, Bounds, Canvas, ClipboardItem, ContentMask, CursorStyle, DispatchPhase,
    Font, FontStyle, FontWeight, Hitbox, HitboxBehavior, Hsla, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollWheelEvent, ShapedLine, SharedString, Size, StrikethroughStyle, Styled, TextRun,
    TouchPhase, UnderlineStyle, Window,
};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terminal_core::{
    Cell, CellColor, CellPixels, ColorPalette, CursorPosition, CursorShape, GridPoint, GridSize, GridSnapshot, Rgb,
    SelectionKind, Side, TerminalHandle, TextMatch, DEFAULT_CURSOR_STYLE,
};

const FALLBACK_CELL_WIDTH_RATIO: f32 = 0.6;
const WORD_CLICK_COUNT: usize = 2;
const LINE_CLICK_COUNT: usize = 3;
const MIN_COLS: u16 = 2;
const MIN_ROWS: u16 = 1;
const ANSI_COLOR_COUNT: u8 = 16;
const REFERENCE_GLYPH: char = 'm';
const UNDERLINE_DESCENT_RATIO: f32 = 0.618;
const STRIKE_RISE_RATIO: f32 = 0.27;
const DIM_TEXT_OPACITY: f32 = 0.66;
const DECORATION_THICKNESS_PX: f32 = 1.0;
const CARET_WIDTH_PX: f32 = 1.0;
const CARET_UNDERLINE_PX: f32 = 2.0;
const BLINK_HALF_PERIOD_MS: u128 = 530;
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
const HIT_RADIUS_PX: f32 = 2.0;
const WHEEL_LINES_PER_NOTCH: f32 = 3.0;

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
    let color = terminal_core::xterm_indexed_rgb(index);
    pack_rgb(color.r, color.g, color.b)
}

fn rgb_of(color: Hsla) -> Rgb {
    let rgba = color.to_rgb();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb { r: channel(rgba.r), g: channel(rgba.g), b: channel(rgba.b) }
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

pub fn scaled_wheel_delta(delta: Pixels, is_precise: bool, multiplier: f32) -> f32 {
    let notch_lines = if is_precise { 1.0 } else { WHEEL_LINES_PER_NOTCH };
    f32::from(delta) * notch_lines * multiplier
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
    reported_button: Option<ReportedButton>,
    reported_cell: Option<GridPoint>,
    is_blink_frame_scheduled: bool,
}

pub type LinkOpener = Rc<dyn Fn(&FileLink, &mut Window, &mut App)>;

#[derive(Clone)]
pub struct TerminalPane {
    pub handle: TerminalHandle,
    pub pointer: Rc<RefCell<PointerState>>,
    pub hits: TerminalHits,
    opener: LinkOpener,
}

impl TerminalPane {
    pub fn new(handle: TerminalHandle, opener: LinkOpener) -> Self {
        Self { handle, pointer: Rc::default(), hits: TerminalHits::default(), opener }
    }

    pub fn has_hovered_link(&self) -> bool {
        self.pointer.borrow().hovered_link.is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TerminalHits {
    pub matches: Arc<Vec<TextMatch>>,
    pub current: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowHit {
    pub row: usize,
    pub columns: Range<usize>,
    pub is_current: bool,
}

pub fn visible_hits(hits: &TerminalHits, display_offset: usize, grid: GridSize) -> Vec<RowHit> {
    let top = -(display_offset as i32);
    let bottom = top + i32::from(grid.rows) - 1;
    let first = hits.matches.partition_point(|hit| hit.end.line < top);
    hits.matches[first..]
        .iter()
        .enumerate()
        .take_while(|(_, hit)| hit.start.line <= bottom)
        .flat_map(|(offset, hit)| {
            let is_current = hits.current == Some(first + offset);
            (hit.start.line.max(top)..=hit.end.line.min(bottom)).filter_map(move |line| {
                let columns = hit.columns_on(line, grid.cols)?;
                let row = (line - top) as usize;
                Some(RowHit { row, columns: usize::from(columns.start)..usize::from(columns.end), is_current })
            })
        })
        .collect()
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
    pub find: FindTokens,
    pub insets: ContentInsets,
    pub input: InputTokens,
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
            find: theme.find,
            insets: ContentInsets::from_theme(theme),
            input: theme.input,
            cell_size_cache: Rc::new(RefCell::new(None)),
            align_cache: Rc::default(),
            glyph_cache: Rc::default(),
        }
    }

    pub fn color_palette(&self) -> ColorPalette {
        ColorPalette {
            foreground: rgb_of(self.colors.foreground),
            background: rgb_of(self.colors.background),
            cursor: rgb_of(self.caret),
            ansi: self.colors.ansi.map(rgb_of),
        }
    }

    pub fn cell_pixels(&self, window: &Window) -> CellPixels {
        let cell = self.cell_size(window);
        let device = |length: Pixels| (f32::from(length) * window.scale_factor()).round() as u16;
        CellPixels { width: device(cell.width), height: device(cell.height) }
    }

    fn font(&self) -> Font {
        font(self.font_family.clone())
    }
}

pub struct Frame {
    snapshot: GridSnapshot,
    cell: Size<Pixels>,
    annotations: Annotations,
    hits: Vec<RowHit>,
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
    let prepare_hits = pane.hits.clone();
    let is_focused = focus.is_some();
    canvas(
        move |bounds, window, _cx| {
            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            let mut frame = prepare_frame(&prepare_handle, &prepare_style, is_focused, window);
            let grid = GridSize::new(frame.snapshot.cols, frame.snapshot.rows);
            frame.hits = visible_hits(&prepare_hits, prepare_handle.terminal().display_offset(), grid);
            (frame, hitbox)
        },
        move |bounds, (frame, hitbox), window, cx| {
            let bounds = device_aligned_content(bounds, style.insets, window);
            let grid_bounds = scrolled_bounds(bounds, &frame, window);
            let is_link_modifier_held = window.modifiers().platform;
            let hovered_link = pane.pointer.borrow().hovered_link.clone().filter(|_| is_link_modifier_held);
            if hovered_link.is_some() {
                window.set_cursor_style(CursorStyle::PointingHand, &hitbox);
            }
            let caret_shape = caret_shape(&frame.snapshot, is_focused);
            let painter = FramePainter { frame: &frame, bounds: grid_bounds, style: &style, hovered_link, caret_shape };
            painter.paint_copy_line(window);
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                painter.paint_partial_row(window, cx);
                painter.paint(window, cx);
                if let Some(text) = focus.as_ref().and_then(|focus| focus.marked_text.as_deref()) {
                    painter.paint_marked_text(text, window, cx);
                }
            });
            if let Some(focus) = focus {
                (focus.register_input)(bounds, window, cx);
            }
            if is_focused && frame.snapshot.cursor.is_some() && frame.snapshot.cursor_style.blinking {
                schedule_blink_frame(&pane, window, cx);
            }
            let grid = GridSize::new(frame.snapshot.cols, frame.snapshot.rows);
            let links = Rc::new(frame.annotations.links);
            let grid_origin = grid_bounds.origin;
            let area = PaneArea { bounds, grid_origin, cell: frame.cell, grid, hitbox, links, input: style.input };
            register_mouse_handlers(pane, area, window);
        },
    )
    .size_full()
}

pub fn caret_shape(snapshot: &GridSnapshot, is_focused: bool) -> Option<CursorShape> {
    let style = snapshot.cursor_style;
    if !is_focused {
        return (style != DEFAULT_CURSOR_STYLE).then_some(CursorShape::HollowBlock);
    }
    if style.blinking && !is_blink_phase_on() {
        return None;
    }
    Some(style.shape)
}

fn blink_clock_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_millis())
}

fn is_blink_phase_on() -> bool {
    (blink_clock_ms() / BLINK_HALF_PERIOD_MS) % 2 == 0
}

fn until_next_blink() -> Duration {
    let remaining = BLINK_HALF_PERIOD_MS - blink_clock_ms() % BLINK_HALF_PERIOD_MS;
    Duration::from_millis(remaining as u64)
}

fn schedule_blink_frame(pane: &TerminalPane, window: &Window, cx: &App) {
    if std::mem::replace(&mut pane.pointer.borrow_mut().is_blink_frame_scheduled, true) {
        return;
    }
    let pointer = pane.pointer.clone();
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(until_next_blink()).await;
            pointer.borrow_mut().is_blink_frame_scheduled = false;
            if let Err(err) = cx.update(|window, _cx| window.refresh()) {
                tracing::debug!(?err, "cursor blink refresh skipped");
            }
        })
        .detach();
}

fn scrolled_bounds(content: Bounds<Pixels>, frame: &Frame, window: &Window) -> Bounds<Pixels> {
    let fraction = frame.snapshot.partial_row.as_ref().map_or(0.0, |row| row.visible_fraction);
    let shift = snap_to_device(frame.cell.height * fraction, window.scale_factor());
    Bounds::new(content.origin + point(px(0.0), shift), content.size)
}

pub fn terminal_preview(handle: TerminalHandle, style: TerminalStyle) -> Canvas<Frame> {
    let prepare_style = style.clone();
    canvas(
        move |_bounds, window, _cx| prepare_frame(&handle, &prepare_style, false, window),
        move |bounds, frame, window, cx| {
            let bounds = device_aligned_content(bounds, style.insets, window);
            let caret_shape = caret_shape(&frame.snapshot, false);
            let painter = FramePainter { frame: &frame, bounds, style: &style, hovered_link: None, caret_shape };
            window.with_content_mask(Some(ContentMask { bounds }), |window| painter.paint(window, cx));
        },
    )
    .size_full()
}

#[derive(Clone)]
struct PaneArea {
    bounds: Bounds<Pixels>,
    grid_origin: Point<Pixels>,
    cell: Size<Pixels>,
    grid: GridSize,
    hitbox: Hitbox,
    links: Rc<Vec<RowLink>>,
    input: InputTokens,
}

impl PaneArea {
    fn point_at(&self, position: Point<Pixels>) -> (GridPoint, Side) {
        grid_point_at(position, self.grid_origin, self.cell, self.grid)
    }

    fn is_hovered(&self, position: Point<Pixels>, window: &Window) -> bool {
        self.bounds.contains(&position) && self.hitbox.is_hovered(window)
    }

    fn link_at(&self, position: Point<Pixels>, window: &Window) -> Option<RowLink> {
        if !self.is_hovered(position, window) {
            return None;
        }
        let (point, _) = self.point_at(position);
        let (row, col) = (usize::from(point.row), usize::from(point.col));
        self.links.iter().find(|found| found.row == row && found.link.columns.contains(&col)).cloned()
    }
}

fn register_mouse_handlers(pane: TerminalPane, area: PaneArea, window: &mut Window) {
    let (down, down_area) = (pane.clone(), area.clone());
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase == DispatchPhase::Bubble {
            handle_mouse_down(&down, &down_area, event, window, cx);
        }
    });
    let (moved, moved_area) = (pane.clone(), area.clone());
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _cx| {
        if phase == DispatchPhase::Bubble {
            handle_mouse_move(&moved, &moved_area, event, window);
        }
    });
    let (up, up_area) = (pane.clone(), area.clone());
    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
        if phase == DispatchPhase::Bubble {
            handle_mouse_up(&up, &up_area, event, window, cx);
        }
    });
    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, _cx| {
        let is_over_pane = area.bounds.contains(&event.position) && area.hitbox.should_handle_scroll(window);
        if phase != DispatchPhase::Bubble || !is_over_pane {
            return;
        }
        scroll_pane(&pane, &area, event);
        window.refresh();
    });
}

fn handle_mouse_down(pane: &TerminalPane, area: &PaneArea, event: &MouseDownEvent, window: &mut Window, cx: &App) {
    if !area.is_hovered(event.position, window) {
        return;
    }
    let modified_link = area.link_at(event.position, window).filter(|_| is_link_click(event));
    if let Some(link) = modified_link {
        pane.pointer.borrow_mut().pressed_link = Some(link);
        return;
    }
    if is_reporting_mouse(pane, event.modifiers) {
        report_press(pane, area, event);
        window.refresh();
        return;
    }
    if event.button == MouseButton::Middle {
        paste_clipboard(&pane.handle, cx);
        return;
    }
    if event.button != MouseButton::Left {
        return;
    }
    let (point, side) = area.point_at(event.position);
    let terminal = pane.handle.terminal();
    if event.modifiers.shift && terminal.has_selection() {
        terminal.update_selection(point, side);
    } else {
        terminal.start_selection(selection_kind_for(event.click_count), point, side);
    }
    pane.pointer.borrow_mut().is_selecting = true;
    window.refresh();
}

fn handle_mouse_move(pane: &TerminalPane, area: &PaneArea, event: &MouseMoveEvent, window: &mut Window) {
    track_hovered_link(pane, area.link_at(event.position, window), window);
    let (reported_button, is_selecting) = {
        let pointer = pane.pointer.borrow();
        (pointer.reported_button, pointer.is_selecting)
    };
    if let Some(button) = reported_button {
        report_motion(pane, area, event, button);
        return;
    }
    if is_selecting && event.pressed_button == Some(MouseButton::Left) {
        let (point, side) = area.point_at(event.position);
        pane.handle.terminal().update_selection(point, side);
        window.refresh();
        return;
    }
    let is_hovering = event.pressed_button.is_none() && area.is_hovered(event.position, window);
    if is_hovering && is_reporting_mouse(pane, event.modifiers) {
        report_motion(pane, area, event, ReportedButton::None);
    }
}

fn handle_mouse_up(pane: &TerminalPane, area: &PaneArea, event: &MouseUpEvent, window: &mut Window, cx: &mut App) {
    if let Some(button) = take_reported_button(pane, event.button) {
        let (point, _) = area.point_at(event.position);
        send_report(pane, MouseReport { button, action: MouseAction::Release, point, modifiers: event.modifiers });
        return;
    }
    if event.button != MouseButton::Left {
        return;
    }
    let (pressed, was_selecting) = {
        let mut pointer = pane.pointer.borrow_mut();
        let was_selecting = std::mem::take(&mut pointer.is_selecting);
        (pointer.pressed_link.take(), was_selecting)
    };
    let released = area.link_at(event.position, window);
    if let Some(link) = pressed.filter(|pressed| Some(pressed) == released.as_ref()) {
        pane.handle.terminal().clear_selection();
        (pane.opener)(&link.link, window, cx);
        return;
    }
    if was_selecting && area.input.copy_on_select {
        copy_selection(pane, cx);
    }
}

pub fn paste_clipboard(handle: &TerminalHandle, cx: &App) {
    let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
        return;
    };
    handle.terminal().scroll_to_bottom();
    if let Err(err) = handle.write(&paste_bytes(&text, handle.input_modes())) {
        tracing::warn!(?err, "pty paste write failed");
    }
}

fn copy_selection(pane: &TerminalPane, cx: &mut App) {
    if let Some(text) = pane.handle.terminal().selection_text() {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

fn is_link_click(event: &MouseDownEvent) -> bool {
    event.modifiers.platform && event.button == MouseButton::Left
}

fn is_reporting_mouse(pane: &TerminalPane, modifiers: Modifiers) -> bool {
    !modifiers.shift && pane.handle.input_modes().mouse.is_reporting()
}

fn reported_button_for(button: MouseButton) -> Option<ReportedButton> {
    match button {
        MouseButton::Left => Some(ReportedButton::Left),
        MouseButton::Middle => Some(ReportedButton::Middle),
        MouseButton::Right => Some(ReportedButton::Right),
        MouseButton::Navigate(_) => None,
    }
}

fn report_press(pane: &TerminalPane, area: &PaneArea, event: &MouseDownEvent) {
    let Some(button) = reported_button_for(event.button) else {
        return;
    };
    let (point, _) = area.point_at(event.position);
    {
        let mut pointer = pane.pointer.borrow_mut();
        pointer.reported_button = Some(button);
        pointer.reported_cell = Some(point);
    }
    pane.handle.terminal().clear_selection();
    send_report(pane, MouseReport { button, action: MouseAction::Press, point, modifiers: event.modifiers });
}

fn report_motion(pane: &TerminalPane, area: &PaneArea, event: &MouseMoveEvent, button: ReportedButton) {
    let (point, _) = area.point_at(event.position);
    {
        let mut pointer = pane.pointer.borrow_mut();
        if pointer.reported_cell == Some(point) {
            return;
        }
        pointer.reported_cell = Some(point);
    }
    send_report(pane, MouseReport { button, action: MouseAction::Motion, point, modifiers: event.modifiers });
}

fn take_reported_button(pane: &TerminalPane, released: MouseButton) -> Option<ReportedButton> {
    let mut pointer = pane.pointer.borrow_mut();
    let button = pointer.reported_button.filter(|&button| Some(button) == reported_button_for(released))?;
    pointer.reported_button = None;
    Some(button)
}

fn send_report(pane: &TerminalPane, report: MouseReport) {
    let Some(bytes) = mouse_report_bytes(report, pane.handle.input_modes()) else {
        return;
    };
    if let Err(err) = pane.handle.write(&bytes) {
        tracing::warn!(?err, "pty mouse report write failed");
    }
}

fn track_hovered_link(pane: &TerminalPane, hovered: Option<RowLink>, window: &mut Window) {
    let mut pointer = pane.pointer.borrow_mut();
    if pointer.hovered_link != hovered {
        pointer.hovered_link = hovered;
        window.refresh();
    }
}

fn scroll_pane(pane: &TerminalPane, area: &PaneArea, event: &ScrollWheelEvent) {
    if matches!(event.touch_phase, TouchPhase::Started) {
        pane.pointer.borrow_mut().scroll_remainder = 0.0;
    }
    let line_height = f32::from(area.cell.height);
    let raw_delta = vertical_delta(event, area.cell.height);
    let delta = scaled_wheel_delta(raw_delta, event.delta.precise(), area.input.scroll_multiplier);
    let modes = pane.handle.input_modes();
    let is_app_scroll = modes.alt_screen || modes.mouse.is_reporting();
    if event.modifiers.shift || !is_app_scroll {
        pane.handle.terminal().scroll_smoothly(delta / line_height);
        return;
    }
    let lines = take_whole_lines(pane, delta, line_height);
    if lines == 0 {
        return;
    }
    let bytes = if modes.mouse.is_reporting() {
        wheel_report_bytes(lines, area.point_at(event.position).0, event.modifiers, modes)
    } else if modes.alternate_scroll {
        wheel_bytes(lines, modes)
    } else {
        return;
    };
    if let Err(err) = pane.handle.write(&bytes) {
        tracing::warn!(?err, "pty wheel write failed");
    }
}

fn vertical_delta(event: &ScrollWheelEvent, line_height: Pixels) -> Pixels {
    let delta = event.delta.pixel_delta(line_height);
    if event.modifiers.shift && delta.y == px(0.0) {
        return delta.x;
    }
    delta.y
}

fn take_whole_lines(pane: &TerminalPane, delta: f32, line_height: f32) -> i32 {
    let mut pointer = pane.pointer.borrow_mut();
    let (lines, remainder) = split_scroll(pointer.scroll_remainder + delta, line_height);
    pointer.scroll_remainder = remainder;
    lines
}

fn prepare_frame(handle: &TerminalHandle, style: &TerminalStyle, is_focused: bool, window: &Window) -> Frame {
    let snapshot = handle.snapshot_grid();
    let annotations = Annotations::read(&snapshot, handle.terminal(), is_focused);
    Frame { snapshot, cell: style.cell_size(window), annotations, hits: Vec::new() }
}

struct FramePainter<'a> {
    frame: &'a Frame,
    bounds: Bounds<Pixels>,
    style: &'a TerminalStyle,
    hovered_link: Option<RowLink>,
    caret_shape: Option<CursorShape>,
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
    inked: Option<Range<usize>>,
}

impl PaintRow<'_> {
    fn is_link(&self, col: usize) -> bool {
        self.links.iter().any(|columns| columns.contains(&col))
    }

    fn is_inked(&self, col: usize) -> bool {
        self.inked.as_ref().is_some_and(|columns| columns.contains(&col))
    }
}

impl FramePainter<'_> {
    fn paint(&self, window: &mut Window, cx: &mut App) {
        for (row_index, row) in self.rows() {
            self.paint_backgrounds(row_index, row, window);
        }
        self.paint_selection(window);
        let metrics = self.text_metrics(window);
        self.paint_hits(metrics, window);
        self.paint_caret(metrics, window);
        window.paint_layer(self.grid_bounds(), |window| {
            for (index, cells) in self.rows() {
                let baseline_origin = self.baseline_origin(index, metrics.baseline, window);
                let links = self.frame.annotations.link_columns_on(index).collect();
                let inked = self.inked_columns(index);
                let row = PaintRow { index, cells, baseline_origin, links, inked };
                self.paint_underlines(&row, metrics.underline_drop, window);
                self.paint_text(&row, window, cx);
                self.paint_strikethroughs(&row, metrics.strike_rise, window);
            }
            self.paint_hovered_link(metrics, window);
            self.paint_ghost(metrics, window, cx);
            self.paint_block_labels(metrics, window, cx);
        });
    }

    fn paint_partial_row(&self, window: &mut Window, cx: &mut App) {
        let Some(partial) = self.frame.snapshot.partial_row.as_ref() else {
            return;
        };
        let cell = self.frame.cell;
        let empty = GridSnapshot::empty(GridSize::new(self.frame.snapshot.cols, 0));
        let snapshot = GridSnapshot { rows: 1, cells: partial.cells.clone(), ..empty };
        let frame = Frame { snapshot, cell, annotations: Annotations::default(), hits: Vec::new() };
        let origin = self.bounds.origin - point(px(0.0), cell.height);
        let bounds = Bounds::new(origin, size(self.bounds.size.width, cell.height));
        let painter = FramePainter { frame: &frame, bounds, style: self.style, hovered_link: None, caret_shape: None };
        painter.paint(window, cx);
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
        let (Some(shape), Some(cursor)) = (self.caret_shape, self.frame.snapshot.cursor) else {
            return;
        };
        let row = usize::from(cursor.row);
        let cell = self.cell_bounds(row, usize::from(cursor.col), 1);
        let caret = self.style.caret;
        match shape {
            CursorShape::Beam => self.paint_beam(cell, row, metrics, window),
            CursorShape::Block => window.paint_quad(fill(cell, caret)),
            CursorShape::Underline => {
                let thickness = px(CARET_UNDERLINE_PX);
                let origin = point(cell.origin.x, cell.bottom() - thickness);
                window.paint_quad(fill(Bounds::new(origin, size(cell.size.width, thickness)), caret));
            }
            CursorShape::HollowBlock => {
                let border = px(CARET_WIDTH_PX);
                let transparent = gpui::transparent_black();
                window.paint_quad(gpui::quad(cell, px(0.0), transparent, border, caret, gpui::BorderStyle::Solid));
            }
            CursorShape::Hidden => {}
        }
    }

    fn paint_beam(&self, cell: Bounds<Pixels>, row: usize, metrics: TextMetrics, window: &mut Window) {
        let row_origin = self.baseline_origin(row, metrics.baseline, window);
        let top = row_origin.y - metrics.ascent;
        let origin = point(snap_to_device(cell.origin.x, window.scale_factor()), top);
        let bounds = Bounds::new(origin, size(px(CARET_WIDTH_PX), metrics.ascent + metrics.descent));
        window.paint_quad(fill(bounds, self.style.caret));
    }

    fn is_block_caret_at(&self, row: usize, col: usize) -> bool {
        let cursor = self.frame.snapshot.cursor;
        let is_here = cursor.is_some_and(|cursor| usize::from(cursor.row) == row && usize::from(cursor.col) == col);
        is_here && self.caret_shape == Some(CursorShape::Block)
    }

    fn paint_hits(&self, metrics: TextMetrics, window: &mut Window) {
        let text_height = metrics.ascent + metrics.descent;
        let inset = (self.frame.cell.height - text_height) / 2.0;
        for hit in &self.frame.hits {
            let cells = self.cell_bounds(hit.row, hit.columns.start, hit.columns.len());
            let bounds = Bounds::new(cells.origin + point(px(0.0), inset), size(cells.size.width, text_height));
            let color = if hit.is_current { self.style.find.hit_current } else { self.style.find.hit };
            window.paint_quad(fill(bounds, color).corner_radii(px(HIT_RADIUS_PX)));
        }
    }

    fn inked_columns(&self, row: usize) -> Option<Range<usize>> {
        let hit = self.frame.hits.iter().find(|hit| hit.is_current && hit.row == row)?;
        Some(hit.columns.clone())
    }

    fn cell_color(&self, row: &PaintRow, col: usize, cell: &Cell) -> Hsla {
        if self.is_block_caret_at(row.index, col) {
            return self.style.colors.background;
        }
        if row.is_inked(col) {
            return self.style.find.hit_current_ink;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use terminal_core::{CursorStyle, MatchPoint};

    fn hit(line: i32, start: u16, end: u16) -> TextMatch {
        TextMatch { start: MatchPoint { line, col: start }, end: MatchPoint { line, col: end } }
    }

    #[test]
    fn visible_hits_map_scrollback_lines_to_viewport_rows() {
        let matches = vec![hit(-30, 0, 3), hit(-12, 2, 5), hit(-10, 0, 1), hit(1, 4, 4)];
        let hits = TerminalHits { matches: Arc::new(matches), current: Some(2) };
        let rows = visible_hits(&hits, 12, GridSize::new(20, 5));
        assert_eq!(rows, vec![
            RowHit { row: 0, columns: 2..6, is_current: false },
            RowHit { row: 2, columns: 0..2, is_current: true },
        ]);
    }

    #[test]
    fn a_match_wrapping_past_the_row_end_paints_on_both_rows() {
        let wrapped = TextMatch { start: MatchPoint { line: 0, col: 18 }, end: MatchPoint { line: 1, col: 1 } };
        let hits = TerminalHits { matches: Arc::new(vec![wrapped]), current: None };
        let rows = visible_hits(&hits, 0, GridSize::new(20, 5));
        let columns: Vec<(usize, Range<usize>)> = rows.into_iter().map(|hit| (hit.row, hit.columns)).collect();
        assert_eq!(columns, vec![(0, 18..20), (1, 0..2)]);
    }

    #[test]
    fn caret_shape_follows_the_app_style_and_hollows_when_unfocused() {
        let mut snapshot = GridSnapshot::empty(GridSize::new(4, 2));
        assert_eq!(caret_shape(&snapshot, true), Some(CursorShape::Beam));
        assert_eq!(caret_shape(&snapshot, false), None);
        snapshot.cursor_style = CursorStyle { shape: CursorShape::Underline, blinking: false };
        assert_eq!(caret_shape(&snapshot, true), Some(CursorShape::Underline));
        assert_eq!(caret_shape(&snapshot, false), Some(CursorShape::HollowBlock));
    }
}
