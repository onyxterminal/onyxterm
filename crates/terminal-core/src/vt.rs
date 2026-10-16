use crate::event::{
    Cell, CellColor, CellPixels, ColorPalette, CursorPosition, GridPoint, GridSize, GridSnapshot, InputModes,
    KittyKeyboard, MouseModes, PartialRow, SelectionKind, SelectionSpan, TerminalError, TerminalEvent, TerminalOutput,
    DEFAULT_CURSOR_STYLE,
};
use crate::command_blocks::{BlockChange, BlockId, CommandBlocks, ShellPhase};
use crate::pty::{ForegroundJob, PtyHandle};
use crate::search::TextMatch;
use crate::shell_marks::{MarkScanner, ShellMark};
use alacritty_terminal::event::{Event as AlacEvent, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Grid, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::term::cell::{Cell as AlacCell, Flags};
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{ClipboardType, Config as TermConfig, Term, TermMode};
use alacritty_terminal::vi_mode::ViMotion;
use alacritty_terminal::vte::ansi::{Color, Handler, Hyperlink, NamedColor, Processor, Rgb};
use domain::TerminalSessionId;
use parking_lot::Mutex;
use std::io::Read;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

pub const DEFAULT_SCROLLBACK: usize = 10_000;
pub const UNKNOWN_EXIT_CODE: i32 = -1;

const EVENTS_CAPACITY: usize = 1024;
const BLOCK_LINK_SCHEME: &str = "onyx-block:";
const WIDE_SPACERS: Flags = Flags::WIDE_CHAR_SPACER.union(Flags::LEADING_WIDE_CHAR_SPACER);

pub struct Terminal {
    inner: Arc<TerminalInner>,
}

pub struct TerminalHandle {
    pub(crate) terminal: Terminal,
    pub(crate) pty: PtyHandle,
}

impl Clone for TerminalHandle {
    fn clone(&self) -> Self {
        Self {
            terminal: self.terminal.clone(),
            pty: self.pty.clone(),
        }
    }
}

struct TerminalInner {
    id: TerminalSessionId,
    state: Mutex<TermState>,
    events: broadcast::Sender<TerminalEvent>,
    decoder: Mutex<Decoder>,
    requests: Arc<Mutex<Vec<TermRequest>>>,
}

type ColorFormatter = Arc<dyn Fn(Rgb) -> String + Sync + Send>;
type SizeFormatter = Arc<dyn Fn(WindowSize) -> String + Sync + Send>;

enum TermRequest {
    Write(String),
    Color(usize, ColorFormatter),
    TextAreaSize(SizeFormatter),
}

#[derive(Default)]
struct Decoder {
    processor: Processor,
    scanner: MarkScanner,
}

struct TermState {
    term: Term<TermListener>,
    grid_size: GridSize,
    blocks: CommandBlocks,
    palette: Option<ColorPalette>,
    cell_pixels: CellPixels,
    scroll_fraction: f32,
}

impl TermState {
    fn scroll(&mut self, scroll: Scroll) {
        self.scroll_fraction = 0.0;
        self.term.scroll_display(scroll);
    }

    fn scroll_smoothly(&mut self, lines: f32) {
        let offset = self.term.grid().display_offset();
        let history = self.term.grid().history_size();
        let position = (offset as f32 + self.scroll_fraction + lines).clamp(0.0, history as f32);
        let target = position.floor();
        self.term.scroll_display(Scroll::Delta(target as i32 - offset as i32));
        self.scroll_fraction = position - target;
    }

    fn partial_row(&self) -> Option<PartialRow> {
        let grid = self.term.grid();
        let line = Line(-(grid.display_offset() as i32) - 1);
        if self.scroll_fraction <= 0.0 || line < grid.topmost_line() {
            return None;
        }
        Some(PartialRow { cells: row_cells(grid, line).collect(), visible_fraction: self.scroll_fraction })
    }

    fn apply_mark(&mut self, mark: ShellMark) -> Option<BlockChange> {
        let change = self.blocks.apply(mark, Instant::now());
        let link = match change {
            Some(BlockChange::InputStarted(id)) => Some(block_link(id)),
            _ => None,
        };
        self.term.set_hyperlink(link);
        change
    }

    fn answer(&self, request: TermRequest) -> Option<String> {
        match request {
            TermRequest::Write(text) => Some(text),
            TermRequest::Color(index, format) => self.color_at(index).map(|color| format(color)),
            TermRequest::TextAreaSize(format) => Some(format(self.window_size())),
        }
    }

    fn color_at(&self, index: usize) -> Option<Rgb> {
        self.term.colors()[index].or_else(|| self.palette?.color_at(index))
    }

    fn window_size(&self) -> WindowSize {
        WindowSize {
            num_lines: self.grid_size.rows,
            num_cols: self.grid_size.cols,
            cell_width: self.cell_pixels.width,
            cell_height: self.cell_pixels.height,
        }
    }
}

impl TerminalInner {
    fn process(&self, bytes: &[u8]) -> Vec<BlockChange> {
        let mut decoder = self.decoder.lock();
        let marks = decoder.scanner.scan(bytes);
        let mut state = self.state.lock();
        let mut changes = Vec::new();
        let mut start = 0;
        for scanned in marks {
            decoder.processor.advance(&mut state.term, &bytes[start..scanned.end]);
            start = scanned.end;
            changes.extend(state.apply_mark(scanned.mark));
        }
        decoder.processor.advance(&mut state.term, &bytes[start..]);
        changes
    }

    fn announce(&self, changes: Vec<BlockChange>) {
        for change in changes {
            let event = match change {
                BlockChange::InputStarted(_) => continue,
                BlockChange::CommandStarted(id) => TerminalEvent::CommandStarted(id),
                BlockChange::CommandFinished(block) => TerminalEvent::CommandFinished(block),
            };
            let _ = self.events.send(event);
        }
    }

    fn sync_deadline(&self) -> Option<Instant> {
        self.decoder.lock().processor.sync_timeout().sync_timeout()
    }

    fn flush_sync(&self) {
        let mut decoder = self.decoder.lock();
        let mut state = self.state.lock();
        decoder.processor.stop_sync(&mut state.term);
    }

    fn take_replies(&self) -> Vec<u8> {
        let state = self.state.lock();
        let requests = std::mem::take(&mut *self.requests.lock());
        requests.into_iter().filter_map(|request| state.answer(request)).flat_map(String::into_bytes).collect()
    }
}

fn block_link(id: BlockId) -> Hyperlink {
    Hyperlink { id: None, uri: format!("{BLOCK_LINK_SCHEME}{}", id.0) }
}

fn linked_block(cell: &AlacCell) -> Option<BlockId> {
    cell.extra.as_ref()?;
    let link = cell.hyperlink()?;
    link.uri().strip_prefix(BLOCK_LINK_SCHEME)?.parse().ok().map(BlockId)
}

struct TermListener {
    events: broadcast::Sender<TerminalEvent>,
    requests: Arc<Mutex<Vec<TermRequest>>>,
}

impl EventListener for TermListener {
    fn send_event(&self, event: AlacEvent) {
        match event {
            AlacEvent::Title(text) => {
                let _ = self.events.send(TerminalEvent::TitleChanged(text));
            }
            AlacEvent::Exit => {
                let _ = self.events.send(TerminalEvent::Exited { code: UNKNOWN_EXIT_CODE });
            }
            AlacEvent::ClipboardStore(ClipboardType::Clipboard, text) => {
                let _ = self.events.send(TerminalEvent::ClipboardStore(text));
            }
            AlacEvent::PtyWrite(text) => self.requests.lock().push(TermRequest::Write(text)),
            AlacEvent::ColorRequest(index, format) => self.requests.lock().push(TermRequest::Color(index, format)),
            AlacEvent::TextAreaSizeRequest(format) => self.requests.lock().push(TermRequest::TextAreaSize(format)),
            _ => {}
        }
    }
}

impl Clone for Terminal {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl Terminal {
    pub fn id(&self) -> TerminalSessionId {
        self.inner.id
    }

    pub fn new(id: TerminalSessionId, size: GridSize, scrollback: usize) -> Self {
        let (tx, _rx) = broadcast::channel(EVENTS_CAPACITY);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let listener = TermListener { events: tx.clone(), requests: requests.clone() };
        let cfg = TermConfig {
            scrolling_history: scrollback.max(1),
            kitty_keyboard: true,
            default_cursor_style: DEFAULT_CURSOR_STYLE,
            ..TermConfig::default()
        };
        let dim = TermSize::new(size.cols as usize, size.rows as usize);
        let term = Term::new(cfg, &dim, listener);
        Self {
            inner: Arc::new(TerminalInner {
                id,
                state: Mutex::new(TermState {
                    term,
                    grid_size: size,
                    blocks: CommandBlocks::default(),
                    palette: None,
                    cell_pixels: CellPixels::default(),
                    scroll_fraction: 0.0,
                }),
                events: tx,
                decoder: Mutex::new(Decoder::default()),
                requests,
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TerminalEvent> {
        self.inner.events.subscribe()
    }

    pub fn feed(&self, bytes: &[u8]) {
        let changes = self.inner.process(bytes);
        self.inner.announce(changes);
    }

    pub fn take_replies(&self) -> Vec<u8> {
        self.inner.take_replies()
    }

    pub fn set_palette(&self, palette: ColorPalette) {
        self.inner.state.lock().palette = Some(palette);
    }

    pub fn set_cell_pixels(&self, cell_pixels: CellPixels) {
        self.inner.state.lock().cell_pixels = cell_pixels;
    }

    pub fn shell_phase(&self) -> ShellPhase {
        self.inner.state.lock().blocks.phase()
    }

    pub fn with_command_blocks<R>(&self, read: impl FnOnce(&CommandBlocks) -> R) -> R {
        read(&self.inner.state.lock().blocks)
    }

    pub fn autosuggestion(&self) -> Option<String> {
        let state = self.inner.state.lock();
        let ShellPhase::Editing(block) = state.blocks.phase() else {
            return None;
        };
        let input = typed_input(&state.term, block)?;
        let suggestion = state.blocks.suggestion(&input)?;
        Some(suggestion[input.len()..].to_string())
    }

    pub fn emit_output(&self, bytes: Vec<u8>) {
        let _ = self.inner.events.send(TerminalEvent::Output(TerminalOutput { bytes }));
    }

    pub fn grid_size(&self) -> GridSize {
        self.inner.state.lock().grid_size
    }

    pub fn snapshot_grid(&self) -> GridSnapshot {
        let state = self.inner.state.lock();
        let grid: &Grid<AlacCell> = state.term.grid();
        let screen_lines = grid.screen_lines();
        let rows = screen_lines as u16;
        let cols = grid.columns() as u16;
        let offset = grid.display_offset() as i32;
        let mut cells = Vec::with_capacity((rows as usize) * (cols as usize));
        for line_idx in 0..screen_lines {
            cells.extend(row_cells(grid, Line(line_idx as i32 - offset)));
        }
        let cursor = visible_cursor(&state.term);
        let cursor_style = state.term.cursor_style();
        let selection = visible_selection(&state.term);
        let is_vi_mode = state.term.mode().contains(TermMode::VI);
        let partial_row = state.partial_row();
        GridSnapshot { rows, cols, cells, cursor, cursor_style, selection, is_vi_mode, partial_row }
    }

    pub fn input_modes(&self) -> InputModes {
        let state = self.inner.state.lock();
        let mode = state.term.mode();
        InputModes {
            app_cursor: mode.contains(TermMode::APP_CURSOR),
            bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
            alt_screen: mode.contains(TermMode::ALT_SCREEN),
            alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
            focus_reporting: mode.contains(TermMode::FOCUS_IN_OUT),
            mouse: mouse_modes(mode),
            kitty: kitty_keyboard(mode),
        }
    }

    pub fn display_offset(&self) -> usize {
        self.inner.state.lock().term.grid().display_offset()
    }

    pub fn scroll_lines(&self, delta: i32) {
        self.inner.state.lock().scroll(Scroll::Delta(delta));
    }

    pub fn scroll_smoothly(&self, lines: f32) {
        self.inner.state.lock().scroll_smoothly(lines);
    }

    pub fn scroll_to_bottom(&self) {
        self.inner.state.lock().scroll(Scroll::Bottom);
    }

    pub fn start_selection(&self, kind: SelectionKind, point: GridPoint, side: Side) {
        let mut state = self.inner.state.lock();
        let location = viewport_to_grid(&state.term, point);
        state.term.selection = Some(Selection::new(selection_type(kind), location, side));
    }

    pub fn update_selection(&self, point: GridPoint, side: Side) {
        let mut state = self.inner.state.lock();
        let location = viewport_to_grid(&state.term, point);
        if let Some(selection) = state.term.selection.as_mut() {
            selection.update(location, side);
        }
    }

    pub fn has_selection(&self) -> bool {
        self.inner.state.lock().term.selection.is_some()
    }

    pub fn select_all(&self) {
        let mut state = self.inner.state.lock();
        let grid = state.term.grid();
        let start = Point::new(grid.topmost_line(), Column(0));
        let end = Point::new(grid.bottommost_line(), grid.last_column());
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Right);
        state.term.selection = Some(selection);
    }

    pub fn clear_selection(&self) {
        self.inner.state.lock().term.selection = None;
    }

    pub fn selection_text(&self) -> Option<String> {
        let state = self.inner.state.lock();
        state.term.selection_to_string().filter(|text| !text.is_empty())
    }

    pub fn is_vi_mode(&self) -> bool {
        self.inner.state.lock().term.mode().contains(TermMode::VI)
    }

    pub fn enter_vi_mode(&self) {
        let mut state = self.inner.state.lock();
        if !state.term.mode().contains(TermMode::VI) {
            state.term.toggle_vi_mode();
        }
    }

    pub fn exit_vi_mode(&self) {
        let mut state = self.inner.state.lock();
        if state.term.mode().contains(TermMode::VI) {
            state.term.toggle_vi_mode();
        }
        state.term.selection = None;
        state.scroll(Scroll::Bottom);
    }

    pub fn vi_motion(&self, motion: ViMotion) {
        self.inner.state.lock().term.vi_motion(motion);
    }

    pub fn vi_scroll_lines(&self, lines: i32) {
        let mut state = self.inner.state.lock();
        let target = state.term.vi_mode_cursor.scroll(&state.term, lines);
        state.scroll(Scroll::Delta(lines));
        state.term.vi_goto_point(target.point);
    }

    pub fn vi_goto_top(&self) {
        let mut state = self.inner.state.lock();
        let line = state.term.topmost_line();
        vi_goto_line(&mut state.term, line);
    }

    pub fn vi_goto_bottom(&self) {
        let mut state = self.inner.state.lock();
        let line = state.term.bottommost_line();
        vi_goto_line(&mut state.term, line);
    }

    pub fn toggle_vi_selection(&self, kind: SelectionKind) {
        let mut state = self.inner.state.lock();
        let ty = selection_type(kind);
        if state.term.selection.as_ref().is_some_and(|selection| selection.ty == ty) {
            state.term.selection = None;
            return;
        }
        let mut selection = Selection::new(ty, state.term.vi_mode_cursor.point, Side::Left);
        selection.include_all();
        state.term.selection = Some(selection);
    }

    pub fn scrollback_len(&self) -> usize {
        let state = self.inner.state.lock();
        let total = state.term.grid().total_lines();
        let visible = state.term.grid().screen_lines();
        total.saturating_sub(visible)
    }

    pub fn find_matches(&self, query: &str, limit: usize) -> Vec<TextMatch> {
        crate::search::find_in(&self.inner.state.lock().term, query, limit)
    }

    pub fn reveal_line(&self, line: i32) {
        let mut state = self.inner.state.lock();
        let grid = state.term.grid();
        let current = grid.display_offset();
        let target = crate::search::offset_revealing(line, current, grid.screen_lines(), grid.history_size());
        state.scroll(Scroll::Delta(target as i32 - current as i32));
    }

    pub fn set_bracketed_paste(&self, on: bool) {
        let bytes: &[u8] = if on { b"\x1b[?2004h" } else { b"\x1b[?2004l" };
        self.feed(bytes);
    }

    pub fn resize(&self, new_size: GridSize) -> Result<(), TerminalError> {
        let mut state = self.inner.state.lock();
        if new_size == state.grid_size {
            return Ok(());
        }
        let dim = TermSize::new(new_size.cols as usize, new_size.rows as usize);
        state.term.resize(dim);
        state.grid_size = new_size;
        let _ = self.inner.events.send(TerminalEvent::Resized {
            cols: new_size.cols,
            rows: new_size.rows,
        });
        Ok(())
    }
}

impl TerminalHandle {
    pub fn attach(terminal: Terminal, pty: PtyHandle) -> Result<Self, TerminalError> {
        let reader = pty.take_reader().ok_or(TerminalError::Terminated)?;
        let inner = terminal.inner.clone();
        let exit_source = pty.clone();
        std::thread::Builder::new()
            .name(format!("term-event-{}", terminal.id()))
            .spawn(move || run_event_loop(inner, reader, exit_source))
            .map_err(|e| TerminalError::PtyRead(format!("thread spawn failed: {e}")))?;
        Ok(Self { terminal, pty })
    }

    pub fn id(&self) -> TerminalSessionId {
        self.terminal.id()
    }

    pub fn write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        self.pty.write(bytes)
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        self.pty.resize(rows, cols)?;
        self.terminal.resize(GridSize { rows, cols })?;
        Ok(())
    }

    pub fn kill(&self) -> Result<(), TerminalError> {
        self.pty.kill()
    }

    pub fn grid_size(&self) -> GridSize {
        self.terminal.grid_size()
    }

    pub fn snapshot_grid(&self) -> GridSnapshot {
        self.terminal.snapshot_grid()
    }

    pub fn scrollback_len(&self) -> usize {
        self.terminal.scrollback_len()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TerminalEvent> {
        self.terminal.subscribe()
    }

    pub fn input_modes(&self) -> InputModes {
        self.terminal.input_modes()
    }

    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    pub fn current_dir(&self) -> Option<std::path::PathBuf> {
        self.pty.current_dir()
    }

    pub fn has_foreground_job(&self) -> bool {
        self.pty.has_foreground_job()
    }

    pub fn foreground_process_group(&self) -> Option<i32> {
        self.pty.foreground_process_group()
    }

    pub fn foreground_job(&self) -> Option<ForegroundJob> {
        self.pty.foreground_job()
    }
}

fn run_event_loop(inner: Arc<TerminalInner>, mut reader: Box<dyn Read + Send>, pty: PtyHandle) {
    let mut buf = [0u8; 8192];
    let mut announced_deadline = None;
    let (deadlines, pending_deadlines) = mpsc::channel();
    let (flusher, flusher_pty) = (inner.clone(), pty.clone());
    let spawned = std::thread::Builder::new()
        .name(format!("term-sync-{}", inner.id))
        .spawn(move || run_sync_flusher(flusher, pending_deadlines, flusher_pty));
    if let Err(err) = spawned {
        tracing::warn!(?err, "sync flusher spawn failed");
    }
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let changes = inner.process(&buf[..n]);
                write_replies(&inner, &pty);
                let _ = inner.events.send(TerminalEvent::Output(TerminalOutput {
                    bytes: buf[..n].to_vec(),
                }));
                inner.announce(changes);
                let deadline = inner.sync_deadline();
                if let Some(pending) = deadline.filter(|_| deadline != announced_deadline) {
                    let _ = deadlines.send(pending);
                }
                announced_deadline = deadline;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    let code = pty.wait_exit_code().unwrap_or(UNKNOWN_EXIT_CODE);
    let _ = inner.events.send(TerminalEvent::Exited { code });
}

fn run_sync_flusher(inner: Arc<TerminalInner>, deadlines: mpsc::Receiver<Instant>, pty: PtyHandle) {
    while let Ok(deadline) = deadlines.recv() {
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        let is_expired = inner.sync_deadline().is_some_and(|pending| pending <= Instant::now());
        if !is_expired {
            continue;
        }
        inner.flush_sync();
        write_replies(&inner, &pty);
        let _ = inner.events.send(TerminalEvent::Output(TerminalOutput::empty()));
    }
}

fn mouse_modes(mode: &TermMode) -> MouseModes {
    MouseModes {
        report_click: mode.contains(TermMode::MOUSE_REPORT_CLICK),
        report_drag: mode.contains(TermMode::MOUSE_DRAG),
        report_motion: mode.contains(TermMode::MOUSE_MOTION),
        sgr: mode.contains(TermMode::SGR_MOUSE),
        utf8: mode.contains(TermMode::UTF8_MOUSE),
    }
}

fn kitty_keyboard(mode: &TermMode) -> KittyKeyboard {
    KittyKeyboard {
        disambiguate: mode.contains(TermMode::DISAMBIGUATE_ESC_CODES),
        report_event_types: mode.contains(TermMode::REPORT_EVENT_TYPES),
        report_alternate_keys: mode.contains(TermMode::REPORT_ALTERNATE_KEYS),
        report_all_keys: mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC),
        report_associated_text: mode.contains(TermMode::REPORT_ASSOCIATED_TEXT),
    }
}

fn write_replies(inner: &TerminalInner, pty: &PtyHandle) {
    let replies = inner.take_replies();
    if replies.is_empty() {
        return;
    }
    if let Err(err) = pty.write(&replies) {
        tracing::warn!(?err, "pty reply write failed");
    }
}

fn selection_type(kind: SelectionKind) -> SelectionType {
    match kind {
        SelectionKind::Simple => SelectionType::Simple,
        SelectionKind::Word => SelectionType::Semantic,
        SelectionKind::Line => SelectionType::Lines,
    }
}

fn vi_goto_line(term: &mut Term<TermListener>, line: Line) {
    term.vi_goto_point(Point::new(line, Column(0)));
    term.vi_motion(ViMotion::FirstOccupied);
}

fn viewport_to_grid(term: &Term<TermListener>, point: GridPoint) -> Point {
    let offset = term.grid().display_offset() as i32;
    Point::new(Line(i32::from(point.row) - offset), Column(usize::from(point.col)))
}

fn visible_selection(term: &Term<TermListener>) -> Option<SelectionSpan> {
    let range: SelectionRange = term.selection.as_ref()?.to_range(term)?;
    let offset = term.grid().display_offset() as i32;
    let rows = term.grid().screen_lines() as i32;
    let last_col = term.grid().columns().saturating_sub(1) as u16;
    let start_row = range.start.line.0 + offset;
    let end_row = range.end.line.0 + offset;
    if end_row < 0 || start_row >= rows {
        return None;
    }
    let start = if start_row < 0 {
        GridPoint { row: 0, col: 0 }
    } else {
        GridPoint { row: start_row as u16, col: range.start.column.0 as u16 }
    };
    let end = if end_row >= rows {
        GridPoint { row: (rows - 1) as u16, col: last_col }
    } else {
        GridPoint { row: end_row as u16, col: range.end.column.0 as u16 }
    };
    Some(SelectionSpan { start, end })
}

fn typed_input(term: &Term<TermListener>, block: BlockId) -> Option<String> {
    let grid = term.grid();
    let is_editable = !term.mode().contains(TermMode::VI) && grid.display_offset() == 0;
    if !is_editable {
        return None;
    }
    let cursor = grid.cursor.point;
    let mut input = String::new();
    for line in 0..=cursor.line.0 {
        let row = &grid[Line(line)];
        for column in 0..grid.columns() {
            let cell = &row[Column(column)];
            let is_typed = linked_block(cell) == Some(block) && !cell.flags.intersects(WIDE_SPACERS);
            let is_before_cursor = line < cursor.line.0 || column < cursor.column.0;
            match (is_typed, is_before_cursor) {
                (true, true) => input.push(cell.c),
                (true, false) if cell.c != ' ' => return None,
                _ => {}
            }
        }
    }
    Some(input)
}

fn visible_cursor(term: &Term<TermListener>) -> Option<CursorPosition> {
    if term.mode().contains(TermMode::VI) {
        return visible_vi_cursor(term);
    }
    let grid = term.grid();
    if !term.mode().contains(TermMode::SHOW_CURSOR) || grid.display_offset() != 0 {
        return None;
    }
    let point = grid.cursor.point;
    Some(CursorPosition { row: point.line.0 as u16, col: point.column.0 as u16 })
}

fn visible_vi_cursor(term: &Term<TermListener>) -> Option<CursorPosition> {
    let point = term.vi_mode_cursor.point;
    let row = point.line.0 + term.grid().display_offset() as i32;
    if row < 0 || row >= term.grid().screen_lines() as i32 {
        return None;
    }
    Some(CursorPosition { row: row as u16, col: point.column.0 as u16 })
}

fn row_cells(grid: &Grid<AlacCell>, line: Line) -> impl Iterator<Item = Cell> + '_ {
    let row = &grid[line];
    (0..grid.columns()).map(move |column| alac_cell_to_cell(&row[Column(column)]))
}

fn alac_cell_to_cell(c: &AlacCell) -> Cell {
    Cell {
        ch: c.c,
        fg: cell_color(&c.fg),
        bg: cell_color(&c.bg),
        flags: c.flags.bits(),
        block: linked_block(c),
    }
}

fn cell_color(color: &Color) -> CellColor {
    match color {
        Color::Spec(rgb) => CellColor::Rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(index) => CellColor::Indexed(*index),
        Color::Named(named) => named_cell_color(*named),
    }
}

fn named_cell_color(named: NamedColor) -> CellColor {
    let index = named as usize;
    let dim_black = NamedColor::DimBlack as usize;
    let dim_white = NamedColor::DimWhite as usize;
    if index <= NamedColor::BrightWhite as usize {
        return CellColor::Indexed(index as u8);
    }
    if (dim_black..=dim_white).contains(&index) {
        return CellColor::Indexed((index - dim_black) as u8);
    }
    CellColor::Default
}
