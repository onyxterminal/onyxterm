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
    pub layout: Option<Rc<FrameLayout>>,
    pub is_selecting: bool,
}

#[derive(Debug)]
pub struct FrameLayout {
    pub text_origin: Point<Pixels>,
    pub line_height: Pixels,
    pub first_line: usize,
    pub lines: Vec<LaidLine>,
    pub viewport: Size<Pixels>,
    pub cursor_bounds: Option<Bounds<Pixels>>,
    pub cursor_line: Option<usize>,
}

impl FrameLayout {
    pub fn visible_line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn laid_line(&self, line: usize) -> Option<&LaidLine> {
        self.lines.iter().find(|laid| laid.line == line)
    }

    fn row_y(&self, laid: &LaidLine) -> Pixels {
        self.text_origin.y + self.line_height * (laid.line - self.first_line) as f32
    }

    fn caret_bounds(&self, rope: &Rope, head: usize, width: Pixels) -> Option<Bounds<Pixels>> {
        let offset = head.min(rope.len_chars());
        let laid = self.laid_line(rope.char_to_line(offset))?;
        let x = self.text_origin.x + laid.x_at_char(offset - laid.start_char);
        Some(Bounds::new(point(x, self.row_y(laid)), size(width, self.line_height)))
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

    pub fn byte_at_char(&self, char: usize) -> usize {
        let last = self.byte_of_char.len().saturating_sub(1);
        self.byte_of_char.get(char.min(last)).copied().unwrap_or(0)
    }
}

impl LaidLine {
    pub fn x_at_char(&self, char: usize) -> Pixels {
        self.shaped.x_for_index(self.display.byte_at_char(char))
    }
}

pub fn selection_span_on_line(sel: Range<usize>, line_start: usize, line_len: usize) -> Option<(usize, usize, bool)> {
    let line_end = line_start + line_len;
    let start = sel.start.max(line_start);
    let end = sel.end.min(line_end);
    let covers_newline = sel.start <= line_end && sel.end > line_end;
    if start > end || (start == end && !covers_newline) {
        return None;
    }
    Some((start - line_start, end - line_start, covers_newline))
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

pub fn max_scroll_top(line_count: usize, line_height: f32) -> f32 {
    line_count.saturating_sub(1) as f32 * line_height
}

pub fn max_scroll_left(widest_line_width: f32, cell_width: f32, text_width: f32) -> f32 {
    (widest_line_width + cell_width - text_width).max(0.0)
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
    let Some(laid) = layout.laid_line(line_index) else {
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
}

pub type PointerHandler = Rc<dyn Fn(PointerEvent, &mut Window, &mut App)>;

pub struct CanvasState {
    layout: Rc<FrameLayout>,
    cell: Size<Pixels>,
    text_rect: Bounds<Pixels>,
    gutter_rect: Bounds<Pixels>,
    hitbox: Hitbox,
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
    let canvas = canvas(
        move |bounds, window, _cx| {
            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            prepare_state(&prepare_frame, &prepare_style, &prepare_view, bounds, hitbox, window)
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
            paint_gutter(&state.layout, state.gutter_rect, &style, window, cx);
            let area = PointerArea {
                layout: state.layout.clone(),
                rope: frame.rope.clone(),
                text_rect,
                hitbox: state.hitbox.clone(),
            };
            register_editor_mouse_handlers(area, &view, &on_pointer, window);
            if let Some(focus) = focus {
                let rect = content_bounds(bounds, style.insets);
                (focus.register_input)(rect, window, cx);
            }
        },
    )
    .size_full();
    canvas
}

#[derive(Clone)]
struct PointerArea {
    layout: Rc<FrameLayout>,
    rope: Rope,
    text_rect: Bounds<Pixels>,
    hitbox: Hitbox,
}

impl PointerArea {
    fn offset_at(&self, position: Point<Pixels>) -> usize {
        let clamped = position.clamp(&self.text_rect.origin, &self.text_rect.bottom_right());
        offset_at(&self.layout, &self.rope, clamped)
    }
}

fn register_editor_mouse_handlers(
    area: PointerArea,
    view: &Rc<RefCell<EditorView>>,
    on_pointer: &PointerHandler,
    window: &mut Window,
) {
    register_press(area.clone(), view.clone(), on_pointer.clone(), window);
    register_drag(area.clone(), view.clone(), on_pointer.clone(), window);
    register_release(view.clone(), window);
    register_scroll(area, view.clone(), window);
}

fn register_press(area: PointerArea, view: Rc<RefCell<EditorView>>, on_pointer: PointerHandler, window: &mut Window) {
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        let is_inside = area.text_rect.contains(&event.position) && area.hitbox.is_hovered(window);
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left || !is_inside {
            return;
        }
        let offset = area.offset_at(event.position);
        view.borrow_mut().is_selecting = true;
        let shift = event.modifiers.shift;
        let alt = event.modifiers.alt;
        on_pointer(PointerEvent::Down { offset, click_count: event.click_count, shift, alt }, window, cx);
        window.refresh();
    });
}

fn register_drag(area: PointerArea, view: Rc<RefCell<EditorView>>, on_pointer: PointerHandler, window: &mut Window) {
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        let is_dragging = view.borrow().is_selecting && event.pressed_button == Some(MouseButton::Left);
        if phase != DispatchPhase::Bubble || !is_dragging {
            return;
        }
        let offset = area.offset_at(event.position);
        on_pointer(PointerEvent::Drag { offset }, window, cx);
        window.refresh();
    });
}

fn register_release(view: Rc<RefCell<EditorView>>, window: &mut Window) {
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, _cx| {
        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
            view.borrow_mut().is_selecting = false;
        }
    });
}

fn register_scroll(area: PointerArea, view: Rc<RefCell<EditorView>>, window: &mut Window) {
    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, _cx| {
        if phase != DispatchPhase::Bubble || !area.hitbox.should_handle_scroll(window) {
            return;
        }
        let line_height = area.layout.line_height;
        let delta = event.delta.pixel_delta(line_height);
        let dx = f32::from(delta.x);
        let dy = f32::from(delta.y);
        let max_scroll = max_scroll_top(area.rope.len_lines(), f32::from(line_height));
        let mut view_mut = view.borrow_mut();
        view_mut.scroll_top = (view_mut.scroll_top - dy).clamp(0.0, max_scroll);
        view_mut.scroll_left = (view_mut.scroll_left - dx).max(0.0);
        window.refresh();
    });
}

fn prepare_state(
    frame: &EditorFrame,
    style: &EditorStyle,
    view: &Rc<RefCell<EditorView>>,
    bounds: Bounds<Pixels>,
    hitbox: Hitbox,
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
    let max_scroll = max_scroll_top(line_count, line_height_f);
    let mut view_ref = view.borrow_mut();
    view_ref.scroll_top = view_ref.scroll_top.clamp(0.0, max_scroll);
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
    let widest = lines.iter().map(|laid| f32::from(laid.shaped.width)).fold(0.0, f32::max);
    let max_left = max_scroll_left(widest, f32::from(cell.width), f32::from(text_rect.size.width));
    view_ref.scroll_left = view_ref.scroll_left.clamp(0.0, max_left);
    let text_offset_x = -view_ref.scroll_left;
    let text_offset_y = -view_ref.scroll_top % line_height_f;
    let text_origin = point(text_rect.origin.x + px(text_offset_x), text_rect.origin.y + px(text_offset_y));
    let head = frame.selections.newest().head;
    let mut layout = FrameLayout {
        text_origin,
        line_height: style.line_height,
        first_line,
        lines,
        viewport: text_rect.size,
        cursor_bounds: None,
        cursor_line: frame.is_focused.then(|| frame.rope.char_to_line(head.min(frame.rope.len_chars()))),
    };
    layout.cursor_bounds = layout.caret_bounds(&frame.rope, head, cell.width);
    let layout = Rc::new(layout);
    view_ref.layout = Some(layout.clone());
    CanvasState {
        layout,
        cell,
        text_rect,
        gutter_rect,
        hitbox,
    }
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
        let Some(laid) = self.layout.laid_line(line_index) else {
            return;
        };
        let x = laid.x_at_char(offset - laid.start_char);
        let y = self.row_y(laid);
        let width_in_cells = text.chars().count().max(1);
        let bounds = Bounds::new(
            point(self.text_origin.x + x, y),
            size(self.cell.width * width_in_cells as f32, self.line_height),
        );
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
        let line_chars = line_len(&self.frame.rope, laid.line);
        for selection in self.frame.selections.as_slice() {
            if selection.is_empty() {
                continue;
            }
            let Some((start_col, end_col, covers_newline)) =
                selection_span_on_line(selection.range(), laid.start_char, line_chars)
            else {
                continue;
            };
            let x_start = laid.x_at_char(start_col);
            let newline_width = if covers_newline { self.cell.width * NEWLINE_EXTRA_CELLS } else { px(0.0) };
            let x_end = laid.x_at_char(end_col) + newline_width;
            let bounds = Bounds::new(point(self.text_origin.x + x_start, y), size(x_end - x_start, self.line_height));
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
            let x = laid.x_at_char(head - laid.start_char);
            let cursor_width = px(self.tokens.cursor_width_px);
            let bounds = Bounds::new(point(self.text_origin.x + x, y), size(cursor_width, self.line_height));
            window.paint_quad(fill(bounds, self.tokens.cursor));
        }
    }

    fn row_y(&self, laid: &LaidLine) -> Pixels {
        self.layout.row_y(laid)
    }
}

fn empty_selection_on_line(selection: &Selection, laid: &LaidLine, rope: &Rope) -> bool {
    if !selection.is_empty() {
        return false;
    }
    let line_chars = line_len(rope, laid.line);
    let line_end = laid.start_char + line_chars;
    selection.head >= laid.start_char && selection.head <= line_end
}

fn paint_gutter(layout: &FrameLayout, gutter_rect: Bounds<Pixels>, style: &EditorStyle, window: &mut Window, cx: &mut App) {
    let tokens = style.tokens;
    let right = gutter_rect.origin.x + gutter_rect.size.width - px(tokens.gutter_padding_px);
    window.with_content_mask(Some(ContentMask { bounds: gutter_rect }), |window| {
        for laid in &layout.lines {
            let number = (laid.line + 1).to_string();
            let is_active = layout.cursor_line == Some(laid.line);
            let color = if is_active { tokens.gutter_text_active } else { tokens.gutter_text };
            let run = TextRun {
                len: number.len(),
                font: style.font(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shape = window.text_system().shape_line(number.into(), style.font_size, &[run], None);
            let origin = point(right - shape.width, layout.row_y(laid));
            if let Err(err) = shape.paint(origin, layout.line_height, window, cx) {
                tracing::warn!(?err, "gutter paint failed");
            }
        }
    });
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
    fn char_columns_map_to_display_bytes_and_clamp_past_the_end() {
        let line = display_line("\tab");
        assert_eq!(line.byte_at_char(0), 0);
        assert_eq!(line.byte_at_char(1), 4);
        assert_eq!(line.byte_at_char(3), 6);
        assert_eq!(line.byte_at_char(9), 6);
    }

    #[test]
    fn two_line_selection_spans_both_rows_and_the_first_newline() {
        assert_eq!(selection_span_on_line(3..12, 0, 9), Some((3, 9, true)));
        assert_eq!(selection_span_on_line(3..12, 10, 5), Some((0, 2, false)));
    }

    #[test]
    fn selection_inside_one_line_spans_only_its_columns() {
        assert_eq!(selection_span_on_line(12..14, 10, 5), Some((2, 4, false)));
        assert_eq!(selection_span_on_line(12..14, 0, 9), None);
    }

    #[test]
    fn line_strictly_inside_a_selection_is_fully_covered() {
        assert_eq!(selection_span_on_line(3..20, 10, 5), Some((0, 5, true)));
        assert_eq!(selection_span_on_line(3..20, 10, 0), Some((0, 0, true)));
    }

    #[test]
    fn selection_ending_at_a_line_start_leaves_that_line_unselected() {
        assert_eq!(selection_span_on_line(3..10, 10, 5), None);
        assert_eq!(selection_span_on_line(3..10, 0, 9), Some((3, 9, true)));
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
        assert_eq!(max_scroll_top(100, 20.0), 1980.0);
        assert_eq!(max_scroll_top(1, 20.0), 0.0);
        assert_eq!(scroll_to_reveal(0.0, 2, 20.0, 200.0), 0.0);
        assert_eq!(scroll_to_reveal(0.0, 20, 20.0, 200.0), 280.0);
        assert_eq!(scroll_to_reveal(400.0, 19, 20.0, 200.0), 320.0);
    }

    #[test]
    fn horizontal_scroll_stops_one_cell_past_the_widest_line() {
        assert_eq!(max_scroll_left(500.0, 10.0, 300.0), 210.0);
        assert_eq!(max_scroll_left(100.0, 10.0, 300.0), 0.0);
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
