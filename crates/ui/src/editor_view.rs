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
pub const SHAPE_MARGIN_COLUMNS: usize = 64;
pub const WHOLE_LINE_MAX_CHARS: usize = 4096;

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
        let column = offset - laid.start_char;
        if !laid.is_shaped_at(column) {
            return None;
        }
        let x = self.text_origin.x + laid.x_at_char(column);
        Some(Bounds::new(point(x, self.row_y(laid)), size(width, self.line_height)))
    }
}

#[derive(Debug, Clone)]
pub struct LaidLine {
    pub line: usize,
    pub start_char: usize,
    pub len_chars: usize,
    pub window: Range<usize>,
    pub window_x: Pixels,
    pub cell_width: Pixels,
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
        if char < self.window.start {
            return self.window_x - self.cell_width * (self.window.start - char) as f32;
        }
        if char > self.window.end {
            return self.window_x + self.shaped.width + self.cell_width * (char - self.window.end) as f32;
        }
        self.window_x + self.shaped.x_for_index(self.display.byte_at_char(char - self.window.start))
    }

    pub fn char_at_x(&self, x: Pixels) -> usize {
        let byte = self.shaped.closest_index_for_x(x - self.window_x);
        self.window.start + self.display.char_at_byte(byte)
    }

    pub fn width(&self) -> Pixels {
        self.x_at_char(self.len_chars)
    }

    pub fn is_shaped_at(&self, char: usize) -> bool {
        self.window.contains(&char) || char == self.window.end
    }

    pub fn is_newline_shaped(&self) -> bool {
        self.window.end == self.len_chars
    }

    pub fn shaped_span(&self, start: usize, end: usize) -> Option<Range<usize>> {
        let start = start.max(self.window.start);
        let end = end.min(self.window.end);
        (start <= end).then_some(start..end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleColumns {
    pub first: usize,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineWindow {
    pub chars: Range<usize>,
    pub start_display_column: usize,
    pub display: DisplayLine,
}

struct DisplayLineBuilder {
    text: String,
    byte_of_char: Vec<usize>,
    start_column: usize,
    column: usize,
}

impl DisplayLineBuilder {
    fn starting_at(column: usize) -> Self {
        Self { text: String::new(), byte_of_char: Vec::new(), start_column: column, column }
    }

    fn push(&mut self, ch: char) {
        self.byte_of_char.push(self.text.len());
        let next = next_display_column(self.column, ch);
        if ch == '\t' {
            self.text.extend(std::iter::repeat_n(' ', next - self.column));
        } else {
            self.text.push(ch);
        }
        self.column = next;
    }

    fn finish(mut self) -> DisplayLine {
        self.byte_of_char.push(self.text.len());
        DisplayLine { text: self.text, byte_of_char: self.byte_of_char }
    }
}

fn next_display_column(column: usize, ch: char) -> usize {
    if ch == '\t' {
        return column + TAB_WIDTH - column % TAB_WIDTH;
    }
    column + 1
}

#[derive(Default)]
struct ColumnCursor {
    char: usize,
    column: usize,
}

impl ColumnCursor {
    fn skipped_char_count(&self, chunk: &str, line_len: usize, columns_before_window: usize) -> Option<usize> {
        if chunk.as_bytes().contains(&b'\t') {
            return None;
        }
        let count = chunk.chars().count();
        let is_within_line = self.char + count <= line_len;
        (is_within_line && self.column + count <= columns_before_window).then_some(count)
    }
}

fn shaped_columns(line_len: usize, visible: VisibleColumns) -> Range<usize> {
    if line_len <= WHOLE_LINE_MAX_CHARS {
        return 0..usize::MAX;
    }
    let start = visible.first.saturating_sub(SHAPE_MARGIN_COLUMNS);
    start..visible.first + visible.count + SHAPE_MARGIN_COLUMNS
}

pub fn line_window<'a>(chunks: impl Iterator<Item = &'a str>, line_len: usize, visible: VisibleColumns) -> LineWindow {
    let columns = shaped_columns(line_len, visible);
    let mut cursor = ColumnCursor::default();
    let mut window: Option<(usize, DisplayLineBuilder)> = None;
    'chunks: for chunk in chunks {
        if window.is_none() {
            if let Some(count) = cursor.skipped_char_count(chunk, line_len, columns.start) {
                cursor.char += count;
                cursor.column += count;
                continue;
            }
        }
        for ch in chunk.chars() {
            if ch == '\n' || cursor.char == line_len || cursor.column >= columns.end {
                break 'chunks;
            }
            let next = next_display_column(cursor.column, ch);
            if window.is_none() && next > columns.start {
                window = Some((cursor.char, DisplayLineBuilder::starting_at(cursor.column)));
            }
            if let Some((_, builder)) = window.as_mut() {
                builder.push(ch);
            }
            cursor.char += 1;
            cursor.column = next;
        }
    }
    let (start_char, builder) = window.unwrap_or_else(|| (cursor.char, DisplayLineBuilder::starting_at(cursor.column)));
    LineWindow {
        chars: start_char..cursor.char,
        start_display_column: builder.start_column,
        display: builder.finish(),
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
    let mut builder = DisplayLineBuilder::starting_at(0);
    line.chars().take_while(|ch| *ch != '\n').for_each(|ch| builder.push(ch));
    builder.finish()
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
    let char = laid.char_at_x(position.x - layout.text_origin.x);
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
    let text_width = f32::from(text_rect.size.width);
    let shaper = LineShaper { rope: &frame.rope, style, cell_width: cell.width, window };
    let lay = |scroll_left: f32| {
        let visible = visible_columns(scroll_left, text_width, f32::from(cell.width));
        range.clone().map(|line| shaper.lay(line, visible)).collect::<Vec<_>>()
    };
    let mut lines = lay(view_ref.scroll_left);
    let widest = lines.iter().map(|laid| f32::from(laid.width())).fold(0.0, f32::max);
    let max_left = max_scroll_left(widest, f32::from(cell.width), text_width);
    let clamped_left = view_ref.scroll_left.clamp(0.0, max_left);
    if clamped_left != view_ref.scroll_left {
        view_ref.scroll_left = clamped_left;
        lines = lay(clamped_left);
    }
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

pub fn visible_columns(scroll_left: f32, text_width: f32, cell_width: f32) -> VisibleColumns {
    if cell_width <= 0.0 {
        return VisibleColumns { first: 0, count: 0 };
    }
    let first = (scroll_left / cell_width).floor().max(0.0) as usize;
    let count = (text_width / cell_width).ceil().max(0.0) as usize + 1;
    VisibleColumns { first, count }
}

struct LineShaper<'a> {
    rope: &'a Rope,
    style: &'a EditorStyle,
    cell_width: Pixels,
    window: &'a Window,
}

impl LineShaper<'_> {
    fn lay(&self, line: usize, visible: VisibleColumns) -> LaidLine {
        let len_chars = line_len(self.rope, line);
        let shaped_window = line_window(self.rope.line(line).chunks(), len_chars, visible);
        let run = TextRun {
            len: shaped_window.display.text.len(),
            font: self.style.font(),
            color: self.style.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let text: SharedString = shaped_window.display.text.clone().into();
        let shaped = self.window.text_system().shape_line(text, self.style.font_size, &[run], None);
        LaidLine {
            line,
            start_char: self.rope.line_to_char(line),
            len_chars,
            window: shaped_window.chars,
            window_x: self.cell_width * shaped_window.start_display_column as f32,
            cell_width: self.cell_width,
            shaped,
            display: shaped_window.display,
        }
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
        let column = offset - laid.start_char;
        if !laid.is_shaped_at(column) {
            return;
        }
        let x = laid.x_at_char(column);
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
            if !empty_selection_on_line(selection, laid) {
                continue;
            }
            let bounds = Bounds::new(point(self.text_rect.origin.x, y), size(self.text_rect.size.width, self.line_height));
            window.paint_quad(fill(bounds, self.tokens.current_line));
            break;
        }
    }

    fn paint_selections(&self, laid: &LaidLine, y: Pixels, window: &mut Window) {
        for selection in self.frame.selections.as_slice() {
            if selection.is_empty() {
                continue;
            }
            let Some((start_col, end_col, covers_newline)) =
                selection_span_on_line(selection.range(), laid.start_char, laid.len_chars)
            else {
                continue;
            };
            let Some(span) = laid.shaped_span(start_col, end_col) else {
                continue;
            };
            let is_newline_painted = covers_newline && laid.is_newline_shaped();
            let x_start = laid.x_at_char(span.start);
            let newline_width = if is_newline_painted { self.cell.width * NEWLINE_EXTRA_CELLS } else { px(0.0) };
            let x_end = laid.x_at_char(span.end) + newline_width;
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
            if head < laid.start_char || !laid.is_shaped_at(head - laid.start_char) {
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

fn empty_selection_on_line(selection: &Selection, laid: &LaidLine) -> bool {
    if !selection.is_empty() {
        return false;
    }
    let line_end = laid.start_char + laid.len_chars;
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

    fn long_line(prefix: &str) -> String {
        let mut text = String::from(prefix);
        text.extend(std::iter::repeat_n('x', WHOLE_LINE_MAX_CHARS * 2));
        text
    }

    fn window_of(text: &str, first: usize, count: usize) -> LineWindow {
        let len = text.trim_end_matches('\n').chars().count();
        line_window(std::iter::once(text), len, VisibleColumns { first, count })
    }

    #[test]
    fn short_line_windows_the_whole_line() {
        let window = window_of("\tab\n", 500, 80);
        assert_eq!(window.chars, 0..3);
        assert_eq!(window.start_display_column, 0);
        assert_eq!(window.display, display_line("\tab"));
    }

    #[test]
    fn long_line_without_tabs_windows_the_visible_columns_plus_margin() {
        let text = long_line("");
        let window = window_of(&text, 1000, 80);
        let start = 1000 - SHAPE_MARGIN_COLUMNS;
        let end = 1000 + 80 + SHAPE_MARGIN_COLUMNS;
        assert_eq!(window.chars, start..end);
        assert_eq!(window.start_display_column, start);
        assert_eq!(window.display.text.len(), end - start);
    }

    #[test]
    fn long_line_window_near_the_start_clamps_to_column_zero() {
        let text = long_line("");
        let window = window_of(&text, 10, 80);
        assert_eq!(window.chars, 0..10 + 80 + SHAPE_MARGIN_COLUMNS);
        assert_eq!(window.start_display_column, 0);
    }

    #[test]
    fn tabs_before_the_window_shift_its_display_column() {
        let text = long_line("\t\tab\t");
        let window = window_of(&text, 1000, 80);
        let start_column = 1000 - SHAPE_MARGIN_COLUMNS;
        let leading_columns = 12;
        let leading_chars = 5;
        assert_eq!(window.start_display_column, start_column);
        assert_eq!(window.chars.start, start_column - leading_columns + leading_chars);
    }

    #[test]
    fn window_starting_inside_a_tab_begins_at_that_tab() {
        let mut text = "x".repeat(SHAPE_MARGIN_COLUMNS + 1);
        text.push('\t');
        let text = long_line(&text);
        let window = window_of(&text, 2 * SHAPE_MARGIN_COLUMNS + 2, 10);
        assert_eq!(window.chars.start, SHAPE_MARGIN_COLUMNS + 1);
        assert_eq!(window.start_display_column, SHAPE_MARGIN_COLUMNS + 1);
        assert!(window.display.text.starts_with("   x"));
    }

    #[test]
    fn window_at_the_end_of_a_long_line_clamps_to_the_line() {
        let text = format!("{}\n", long_line(""));
        let len = WHOLE_LINE_MAX_CHARS * 2;
        let window = window_of(&text, len - 10, 80);
        assert_eq!(window.chars, len - 10 - SHAPE_MARGIN_COLUMNS..len);
        assert!(!window.display.text.contains('\n'));
    }

    #[test]
    fn window_past_the_end_of_a_long_line_is_empty_at_the_line_end() {
        let text = long_line("");
        let len = WHOLE_LINE_MAX_CHARS * 2;
        let window = window_of(&text, len * 2, 80);
        assert_eq!(window.chars, len..len);
        assert_eq!(window.start_display_column, len);
        assert!(window.display.text.is_empty());
    }

    #[test]
    fn window_spans_chunk_boundaries() {
        let text = long_line("\t");
        let (head, tail) = text.split_at(WHOLE_LINE_MAX_CHARS);
        let len = text.chars().count();
        let whole = window_of(&text, 3000, 80);
        let split = line_window([head, tail].into_iter(), len, VisibleColumns { first: 3000, count: 80 });
        assert_eq!(split, whole);
    }

    #[test]
    #[ignore]
    fn windowing_the_end_of_a_ten_megabyte_line_is_fast() {
        const TEN_MEGABYTES: usize = 10 * 1024 * 1024;
        const BUDGET: std::time::Duration = std::time::Duration::from_millis(2);
        const RUNS: usize = 9;
        let mut text = String::from("\t");
        text.extend(std::iter::repeat_n('x', TEN_MEGABYTES));
        let rope = Rope::from_str(&text);
        let len = line_len(&rope, 0);
        let visible = VisibleColumns { first: len, count: 200 };
        let mut timings: Vec<_> = (0..RUNS)
            .map(|_| {
                let started = std::time::Instant::now();
                let window = line_window(rope.line(0).chunks(), len, visible);
                let elapsed = started.elapsed();
                assert_eq!(window.chars.end, len);
                elapsed
            })
            .collect();
        timings.sort();
        let median = timings[RUNS / 2];
        let (fastest, slowest) = (timings[0], timings[RUNS - 1]);
        println!("windowed a {TEN_MEGABYTES}-byte line: median {median:?}, min {fastest:?}, max {slowest:?}");
        assert!(median < BUDGET, "median took {median:?}");
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
