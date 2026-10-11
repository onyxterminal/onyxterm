#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::terminal_view::{content_bounds, ContentInsets, PaneFocus};
use crate::theme::{EditorTokens, Theme};
use editor_core::{line_len, Rope, Selections, Selection, TAB_WIDTH};
use gpui::{
    canvas, fill, font, point, px, size, App, Bounds, Canvas, ContentMask, DispatchPhase, Font, Hitbox, HitboxBehavior,
    Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollWheelEvent, SharedString,
    ShapedLine, Size, Styled, TextRun, UnderlineStyle, Window,
};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

const REVEAL_MARGIN_LINES: f32 = 3.0;
const MIN_GUTTER_COLUMNS: usize = 2;
const FALLBACK_CELL_WIDTH_RATIO: f32 = 0.6;
const NEWLINE_EXTRA_CELLS: f32 = 1.0;

#[derive(Debug, Default)]
pub struct EditorView {
    pub scroll_top: f32,
    pub scroll_left: f32,
    pub layout: Option<FrameLayout>,
    pub is_selecting: bool,
    pub last_click_mode: Option<editor_core::ClickMode>,
}

#[derive(Debug, Clone)]
pub struct FrameLayout {
    pub text_origin: Point<Pixels>,
    pub line_height: Pixels,
    pub first_line: usize,
    pub lines: Vec<LaidLine>,
    pub viewport: Size<Pixels>,
    pub cursor_bounds: Option<Bounds<Pixels>>,
}

impl FrameLayout {
    pub fn visible_line_count(&self) -> usize {
        self.lines.len()
    }
}

#[derive(Debug, Clone)]
pub struct LaidLine {
    pub line: usize,
    pub start_char: usize,
    pub shaped: ShapedLine,
    pub display: DisplayLine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayLine {
    pub text: String,
    pub byte_of_char: Vec<usize>,
}

impl DisplayLine {
    pub fn char_at_byte(&self, byte: usize) -> usize {
        self.byte_of_char.iter().rposition(|start| *start <= byte).unwrap_or(0)
    }
}

pub fn display_line(line: &str) -> DisplayLine {
    let mut text = String::with_capacity(line.len());
    let mut byte_of_char = Vec::with_capacity(line.len() + 1);
    let mut column = 0;
    for ch in line.chars().take_while(|ch| *ch != '\n') {
        byte_of_char.push(text.len());
        if ch == '\t' {
            let width = TAB_WIDTH - column % TAB_WIDTH;
            text.extend(std::iter::repeat_n(' ', width));
            column += width;
        } else {
            text.push(ch);
            column += 1;
        }
    }
    byte_of_char.push(text.len());
    DisplayLine { text, byte_of_char }
}

pub fn visible_range(scroll_top: f32, viewport_height: f32, line_height: f32, line_count: usize) -> Range<usize> {
    let first = (scroll_top / line_height).floor().max(0.0) as usize;
    let last = ((scroll_top + viewport_height) / line_height).floor() as usize + 1;
    first.min(line_count)..last.min(line_count)
}

pub fn max_scroll_top(line_count: usize, line_height: f32, _viewport_height: f32) -> f32 {
    line_count.saturating_sub(1) as f32 * line_height
}

pub fn scroll_to_reveal(scroll_top: f32, line: usize, line_height: f32, viewport_height: f32) -> f32 {
    let margin = REVEAL_MARGIN_LINES * line_height;
    let top = line as f32 * line_height;
    let lowest = top + line_height + margin - viewport_height;
    let highest = top - margin;
    scroll_top.max(lowest).min(highest.max(0.0)).max(0.0)
}

pub fn line_at_y(origin_y: f32, line_height: f32, first_line: usize, line_count: usize, y: f32) -> usize {
    let row = ((y - origin_y) / line_height).floor().max(0.0) as usize;
    (first_line + row).min(line_count.saturating_sub(1))
}

pub fn offset_at(layout: &FrameLayout, rope: &Rope, position: Point<Pixels>) -> usize {
    let line_count = rope.len_lines();
    if line_count == 0 {
        return 0;
    }
    let line_index = line_at_y(
        f32::from(layout.text_origin.y),
        f32::from(layout.line_height),
        layout.first_line,
        line_count,
        f32::from(position.y),
    );
    let line_start = rope.line_to_char(line_index);
    let line_chars = line_len(rope, line_index);
    let line_end = line_start + line_chars;
    let Some(laid) = layout.lines.iter().find(|laid| laid.line == line_index) else {
        return line_end;
    };
    let local_x = f32::from(position.x) - f32::from(layout.text_origin.x);
    let byte = laid.shaped.closest_index_for_x(px(local_x));
    let char = laid.display.char_at_byte(byte);
    (line_start + char).min(line_end)
}

pub fn gutter_columns(line_count: usize) -> usize {
    line_count.max(1).to_string().len().max(MIN_GUTTER_COLUMNS)
}

#[derive(Clone)]
pub struct EditorStyle {
    pub font_family: SharedString,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub text: Hsla,
    pub tokens: EditorTokens,
    pub insets: ContentInsets,
}

impl EditorStyle {
    pub fn from_theme(theme: &Theme, font_family: SharedString) -> Self {
        Self {
            font_family,
            font_size: px(theme.font.size_px),
            line_height: px((theme.font.size_px * theme.font.line_height).round()),
            text: theme.text.default,
            tokens: theme.editor,
            insets: ContentInsets::from_theme(theme),
        }
    }

    fn font(&self) -> Font {
        font(self.font_family.clone())
    }

    pub fn cell_size(&self, window: &Window) -> Size<Pixels> {
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&self.font());
        let width = text_system
            .advance(font_id, self.font_size, 'm')
            .map(|advance| advance.width)
            .unwrap_or(self.font_size * FALLBACK_CELL_WIDTH_RATIO);
        size(width, self.line_height)
    }
}

#[derive(Clone)]
pub struct EditorFrame {
    pub rope: Rope,
    pub selections: Selections,
    pub is_focused: bool,
    pub marked_text: Option<String>,
}

pub enum PointerEvent {
    Down { offset: usize, click_count: usize, shift: bool, alt: bool },
    Drag { offset: usize },
    Up,
    Scroll { dx: f32, dy: f32 },
}

pub type PointerHandler = Rc<dyn Fn(PointerEvent, &mut Window, &mut App)>;

pub struct CanvasState {
    layout: FrameLayout,
    cell: Size<Pixels>,
    text_rect: Bounds<Pixels>,
    gutter_rect: Bounds<Pixels>,
    pub _on_pointer: PointerHandler,
}

pub fn editor_canvas(
    frame: EditorFrame,
    style: EditorStyle,
    view: Rc<RefCell<EditorView>>,
    focus: Option<PaneFocus>,
    on_pointer: PointerHandler,
) -> Canvas<CanvasState> {
    let prepare_style = style.clone();
    let prepare_frame = frame.clone();
    let prepare_view = view.clone();
    let paint_on_pointer = on_pointer.clone();
    let canvas = canvas(
        move |bounds, window, _cx| {
            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            prepare_state(&prepare_frame, &prepare_style, &prepare_view, on_pointer.clone(), bounds, hitbox, window)
        },
        move |bounds, state: CanvasState, window, cx| {
            let text_origin = state.layout.text_origin;
            let text_rect = state.text_rect;
            let cell = state.cell;
            let tokens = style.tokens;
            let line_height = style.line_height;
            let focused = frame.is_focused;
            window.with_content_mask(Some(ContentMask { bounds: text_rect }), |window| {
                let painter = FramePainter {
                    frame: &frame,
                    layout: &state.layout,
                    text_rect,
                    text_origin,
                    cell,
                    tokens,
                    line_height,
                    is_focused: focused,
                };
                painter.paint(window, cx);
                if let Some(text) = frame.marked_text.as_deref() {
                    painter.paint_marked_text(text, &style, window, cx);
                }
            });
            paint_gutter(&state.layout, &state.gutter_rect, &style, tokens, window, cx);
            if focused {
                register_editor_mouse_handlers(&state, &frame, &view, &paint_on_pointer, window);
            }
            if let Some(focus) = focus {
                let rect = content_bounds(bounds, style.insets);
                (focus.register_input)(rect, window, cx);
            }
        },
    )
    .size_full();
    canvas
}

fn register_editor_mouse_handlers(
    state: &CanvasState,
    frame: &EditorFrame,
    view: &Rc<RefCell<EditorView>>,
    on_pointer: &PointerHandler,
    window: &mut Window,
) {
    let bounds = state.text_rect;
    let line_height = state.layout.line_height;
    let down_handler = on_pointer.clone();
    let down_view = view.clone();
    let down_layout = state.layout.clone();
    let down_rope = frame.rope.clone();
    let down_bounds = bounds;
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        let is_inside = down_bounds.contains(&event.position) && event.button == MouseButton::Left;
        if phase != DispatchPhase::Bubble || !is_inside {
            return;
        }
        let offset = offset_at(&down_layout, &down_rope, event.position);
        down_view.borrow_mut().is_selecting = true;
        down_handler(
            PointerEvent::Down {
                offset,
                click_count: event.click_count,
                shift: event.modifiers.shift,
                alt: event.modifiers.alt,
            },
            window,
            cx,
        );
        window.refresh();
    });
    let drag_handler = on_pointer.clone();
    let drag_view = view.clone();
    let drag_layout = state.layout.clone();
    let drag_rope = frame.rope.clone();
    let drag_bounds = bounds;
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        let is_dragging = drag_view.borrow().is_selecting && event.pressed_button == Some(MouseButton::Left);
        if phase != DispatchPhase::Bubble || !is_dragging {
            return;
        }
        if !drag_bounds.contains(&event.position) {
            return;
        }
        let offset = offset_at(&drag_layout, &drag_rope, event.position);
        drag_handler(PointerEvent::Drag { offset }, window, cx);
        window.refresh();
    });
    let up_handler = on_pointer.clone();
    let up_view = view.clone();
    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
            return;
        }
        let mut view_mut = up_view.borrow_mut();
        view_mut.is_selecting = false;
        view_mut.last_click_mode = None;
        drop(view_mut);
        up_handler(PointerEvent::Up, window, cx);
    });
    let scroll_handler = on_pointer.clone();
    let scroll_view = view.clone();
    let scroll_bounds = bounds;
    let scroll_rope = frame.rope.clone();
    let scroll_line_height = line_height;
    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || !scroll_bounds.contains(&event.position) {
            return;
        }
        let delta = event.delta.pixel_delta(scroll_line_height);
        let dx = f32::from(delta.x);
        let dy = f32::from(delta.y);
        let line_count = scroll_rope.len_lines();
        let layout_snapshot = scroll_view.borrow().layout.clone();
        if let Some(layout) = layout_snapshot {
            let max_scroll = max_scroll_top(
                line_count,
                f32::from(scroll_line_height),
                f32::from(layout.viewport.height),
            );
            let mut view_mut = scroll_view.borrow_mut();
            view_mut.scroll_top = (view_mut.scroll_top - dy).clamp(0.0, max_scroll);
            view_mut.scroll_left = (view_mut.scroll_left - dx).max(0.0);
        }
        scroll_handler(PointerEvent::Scroll { dx, dy }, window, cx);
        window.refresh();
    });
}

fn prepare_state(
    frame: &EditorFrame,
    style: &EditorStyle,
    view: &Rc<RefCell<EditorView>>,
    on_pointer: PointerHandler,
    bounds: Bounds<Pixels>,
    _hitbox: Hitbox,
    window: &Window,
) -> CanvasState {
    let content = content_bounds(bounds, style.insets);
    let cell = style.cell_size(window);
    let line_count = frame.rope.len_lines();
    let columns = gutter_columns(line_count);
    let gutter_padding_px = style.tokens.gutter_padding_px;
    let gutter_padding = px(gutter_padding_px);
    let gutter_width = px(columns as f32 * f32::from(cell.width)) + gutter_padding * 2.0;
    let gutter_rect = Bounds::new(content.origin, size(gutter_width, content.size.height));
    let text_rect = Bounds::new(
        point(content.origin.x + gutter_width, content.origin.y),
        size((content.size.width - gutter_width).max(px(0.0)), content.size.height),
    );
    let line_height_f = f32::from(style.line_height);
    let viewport_height = f32::from(text_rect.size.height);
    let max_scroll = max_scroll_top(line_count, line_height_f, viewport_height);
    let mut view_ref = view.borrow_mut();
    view_ref.scroll_top = view_ref.scroll_top.clamp(0.0, max_scroll);
    view_ref.scroll_left = view_ref.scroll_left.max(0.0);
    let range = visible_range(view_ref.scroll_top, viewport_height, line_height_f, line_count);
    let first_line = range.start;
    let text_system = window.text_system();
    let mut lines = Vec::with_capacity(range.len());
    for line_index in range.clone() {
        let line_text = frame.rope.line(line_index).to_string();
        let display = display_line(&line_text);
        let start_char = frame.rope.line_to_char(line_index);
        let run = TextRun {
            len: display.text.len(),
            font: style.font(),
            color: style.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = text_system.shape_line(display.text.clone().into(), style.font_size, &[run], None);
        lines.push(LaidLine { line: line_index, start_char, shaped, display });
    }
    let text_offset_x = -view_ref.scroll_left;
    let text_offset_y = -view_ref.scroll_top % line_height_f;
    let text_origin = point(text_rect.origin.x + px(text_offset_x), text_rect.origin.y + px(text_offset_y));
    let cursor_bounds = cursor_bounds_for(frame, text_origin, f32::from(style.line_height), f32::from(cell.width));
    let layout = FrameLayout {
        text_origin,
        line_height: style.line_height,
        first_line,
        lines,
        viewport: text_rect.size,
        cursor_bounds,
    };
    view_ref.layout = Some(layout.clone());
    CanvasState {
        layout,
        cell,
        text_rect,
        gutter_rect,
        _on_pointer: on_pointer,
    }
}

fn cursor_bounds_for(frame: &EditorFrame, text_origin: Point<Pixels>, line_height: f32, cell_width: f32) -> Option<Bounds<Pixels>> {
    let selection = frame.selections.newest();
    let rope = &frame.rope;
    let total = rope.len_chars();
    let offset = selection.head.min(total);
    let line_index = rope.char_to_line(offset);
    let line_origin = text_origin.y + px(line_index as f32 * line_height);
    let rel = offset - rope.line_to_char(line_index);
    let laid_x = cell_width * rel as f32;
    Some(Bounds::new(point(text_origin.x + px(laid_x), line_origin), size(px(cell_width), px(line_height))))
}

struct FramePainter<'a> {
    frame: &'a EditorFrame,
    layout: &'a FrameLayout,
    text_rect: Bounds<Pixels>,
    text_origin: Point<Pixels>,
    cell: Size<Pixels>,
    tokens: EditorTokens,
    line_height: Pixels,
    is_focused: bool,
}

impl<'a> FramePainter<'a> {
    fn paint(&self, window: &mut Window, cx: &mut App) {
        for laid in &self.layout.lines {
            let y = self.row_y(laid);
            if self.is_focused {
                self.paint_current_line(laid, y, window);
            }
            self.paint_selections(laid, y, window);
            self.paint_text(laid, y, window, cx);
            if self.is_focused {
                self.paint_cursors(laid, y, window);
            }
        }
    }

    fn paint_marked_text(
        &self,
        text: &str,
        style: &EditorStyle,
        window: &mut Window,
        cx: &mut App,
    ) {
        let rope = &self.frame.rope;
        let selection = self.frame.selections.newest();
        let total = rope.len_chars();
        let offset = selection.head.min(total);
        let line_index = rope.char_to_line(offset);
        let Some(laid) = self.layout.lines.iter().find(|laid| laid.line == line_index) else {
            return;
        };
        let rel = offset - laid.start_char;
        let byte_col = laid.display.byte_of_char[rel.min(laid.display.byte_of_char.len().saturating_sub(1))];
        let x = laid.shaped.x_for_index(byte_col);
        let y = self.row_y(laid);
        let cell_width = self.cell.width;
        let width_in_cells = text.chars().count().max(1);
        let bounds = Bounds::new(point(self.text_rect.origin.x + x, y), size(cell_width * width_in_cells as f32, self.line_height));
        if bounds.origin.x < self.text_rect.origin.x
            || bounds.origin.x + bounds.size.width > self.text_rect.origin.x + self.text_rect.size.width
        {
            return;
        }
        window.paint_quad(fill(bounds, self.tokens.selection));
        let underline = UnderlineStyle { thickness: px(1.0), color: None, wavy: false };
        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color: self.tokens.cursor,
            background_color: None,
            underline: Some(underline),
            strikethrough: None,
        };
        let line = window.text_system().shape_line(text.to_string().into(), style.font_size, &[run], None);
        if let Err(err) = line.paint(bounds.origin, self.line_height, window, cx) {
            tracing::warn!(?err, "marked text paint failed");
        }
    }

    fn paint_current_line(&self, laid: &LaidLine, y: Pixels, window: &mut Window) {
        for selection in self.frame.selections.as_slice() {
            if !empty_selection_on_line(selection, laid, &self.frame.rope) {
                continue;
            }
            let bounds = Bounds::new(point(self.text_rect.origin.x, y), size(self.text_rect.size.width, self.line_height));
            window.paint_quad(fill(bounds, self.tokens.current_line));
            break;
        }
    }

    fn paint_selections(&self, laid: &LaidLine, y: Pixels, window: &mut Window) {
        let cell_width = self.cell.width;
        for selection in self.frame.selections.as_slice() {
            if selection.is_empty() {
                continue;
            }
            let (Some(start_col), Some(end_col)) = (
                line_byte_at(laid, &self.frame.rope, selection.range().start),
                line_byte_at(laid, &self.frame.rope, selection.range().end),
            ) else {
                continue;
            };
            let x_start = laid.shaped.x_for_index(start_col);
            let x_end = if selection.range().end > laid.start_char + line_len(&self.frame.rope, laid.line) {
                laid.shaped.width + cell_width * NEWLINE_EXTRA_CELLS
            } else {
                laid.shaped.x_for_index(end_col)
            };
            if x_end <= x_start {
                continue;
            }
            let bounds = Bounds::new(
                point(self.text_rect.origin.x + x_start, y),
                size(x_end - x_start, self.line_height),
            );
            if bounds.origin.x < self.text_rect.origin.x
                || bounds.origin.x + bounds.size.width > self.text_rect.origin.x + self.text_rect.size.width
            {
                continue;
            }
            window.paint_quad(fill(bounds, self.tokens.selection));
        }
    }

    fn paint_text(&self, laid: &LaidLine, y: Pixels, window: &mut Window, cx: &mut App) {
        if laid.display.text.is_empty() {
            return;
        }
        let origin = point(self.text_origin.x, y);
        if let Err(err) = laid.shaped.paint(origin, self.line_height, window, cx) {
            tracing::warn!(?err, "editor line paint failed");
        }
    }

    fn paint_cursors(&self, laid: &LaidLine, y: Pixels, window: &mut Window) {
        for selection in self.frame.selections.as_slice() {
            let head = selection.head;
            let line_chars = line_len(&self.frame.rope, laid.line);
            let line_end = laid.start_char + line_chars;
            if head < laid.start_char || head > line_end {
                continue;
            }
            let rel = head - laid.start_char;
            let col = rel.min(laid.display.byte_of_char.len().saturating_sub(1));
            let byte = laid.display.byte_of_char[col];
            let x = laid.shaped.x_for_index(byte);
            let cursor_width = px(self.tokens.cursor_width_px);
            let bounds = Bounds::new(point(self.text_rect.origin.x + x, y), size(cursor_width, self.line_height));
            if bounds.origin.x < self.text_rect.origin.x
                || bounds.origin.x + bounds.size.width > self.text_rect.origin.x + self.text_rect.size.width
            {
                continue;
            }
            window.paint_quad(fill(bounds, self.tokens.cursor));
        }
    }

    fn row_y(&self, laid: &LaidLine) -> Pixels {
        self.text_origin.y + px((laid.line - self.layout.first_line) as f32 * f32::from(self.line_height))
    }
}

fn line_byte_at(laid: &LaidLine, rope: &Rope, absolute_offset: usize) -> Option<usize> {
    let line_chars = line_len(rope, laid.line);
    let line_end = laid.start_char + line_chars;
    if absolute_offset < laid.start_char || absolute_offset > line_end {
        return None;
    }
    let rel = absolute_offset - laid.start_char;
    let col = rel.min(laid.display.byte_of_char.len().saturating_sub(1));
    Some(laid.display.byte_of_char[col])
}

fn empty_selection_on_line(selection: &Selection, laid: &LaidLine, rope: &Rope) -> bool {
    if !selection.is_empty() {
        return false;
    }
    let line_chars = line_len(rope, laid.line);
    let line_end = laid.start_char + line_chars;
    selection.head >= laid.start_char && selection.head <= line_end
}

fn paint_gutter(
    layout: &FrameLayout,
    gutter_rect: &Bounds<Pixels>,
    style: &EditorStyle,
    tokens: EditorTokens,
    window: &mut Window,
    cx: &mut App,
) {
    let padding = px(tokens.gutter_padding_px);
    let active_line = layout.cursor_bounds.map(|_| {
        let row_y = layout.cursor_bounds.unwrap().origin.y - layout.text_origin.y;
        layout.first_line + (f32::from(row_y) / f32::from(layout.line_height)).round() as usize
    });
    let runs: Vec<TextRun> = layout
        .lines
        .iter()
        .map(|laid| {
            let number = (laid.line + 1).to_string();
            let is_active = active_line == Some(laid.line);
            let color = if is_active { tokens.gutter_text_active } else { tokens.gutter_text };
            TextRun {
                len: number.len(),
                font: style.font(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }
        })
        .collect();
    let texts: Vec<String> = layout.lines.iter().map(|laid| (laid.line + 1).to_string()).collect();
    let shaped: Vec<ShapedLine> = texts
        .iter()
        .zip(runs.iter())
        .map(|(text, run)| window.text_system().shape_line(text.clone().into(), style.font_size, std::slice::from_ref(run), None))
        .collect();
    for (laid, shape) in layout.lines.iter().zip(shaped.iter()) {
        let row = laid.line - layout.first_line;
        let y = layout.text_origin.y + px(row as f32 * f32::from(layout.line_height));
        let x = gutter_rect.origin.x + gutter_rect.size.width - padding - shape.width;
        if let Err(err) = shape.paint(point(x, y), layout.line_height, window, cx) {
            tracing::warn!(?err, "gutter paint failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_line_maps_chars_to_bytes_through_tabs_and_emoji() {
        let line = display_line("\ta\u{1F600}b\n");
        assert_eq!(line.text, "    a\u{1F600}b");
        assert_eq!(line.byte_of_char, vec![0, 4, 5, 9, 10]);
        assert_eq!(line.char_at_byte(9), 3);
        assert_eq!(line.char_at_byte(2), 0);
    }

    #[test]
    fn tab_after_text_expands_to_the_next_stop() {
        assert_eq!(display_line("ab\tc").text, "ab  c");
    }

    #[test]
    fn visible_range_covers_partial_lines_and_clamps() {
        assert_eq!(visible_range(0.0, 100.0, 20.0, 1000), 0..6);
        assert_eq!(visible_range(30.0, 100.0, 20.0, 1000), 1..7);
        assert_eq!(visible_range(0.0, 100.0, 20.0, 3), 0..3);
    }

    #[test]
    fn scrolling_clamps_and_reveals_the_cursor_with_margin() {
        assert_eq!(max_scroll_top(100, 20.0, 200.0), 1980.0);
        assert_eq!(max_scroll_top(1, 20.0, 200.0), 0.0);
        assert_eq!(scroll_to_reveal(0.0, 2, 20.0, 200.0), 0.0);
        assert_eq!(scroll_to_reveal(0.0, 20, 20.0, 200.0), 280.0);
        assert_eq!(scroll_to_reveal(400.0, 19, 20.0, 200.0), 320.0);
    }

    #[test]
    fn gutter_fits_the_widest_line_number() {
        assert_eq!(gutter_columns(9), 2);
        assert_eq!(gutter_columns(12345), 5);
    }

    #[test]
    fn y_positions_map_to_lines_and_clamp_to_the_buffer() {
        assert_eq!(line_at_y(100.0, 20.0, 5, 50, 105.0), 5);
        assert_eq!(line_at_y(100.0, 20.0, 5, 50, 141.0), 7);
        assert_eq!(line_at_y(100.0, 20.0, 5, 50, 10.0), 5);
        assert_eq!(line_at_y(100.0, 20.0, 5, 8, 400.0), 7);
    }
}
