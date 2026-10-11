use crate::event::{Cell, GridSize, GridSnapshot, TerminalError, TerminalEvent, TerminalOutput};
use crate::pty::PtyHandle;
use alacritty_terminal::event::{Event as AlacEvent, EventListener};
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Cell as AlacCell;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::vte::ansi::Processor;
use domain::TerminalSessionId;
use parking_lot::Mutex;
use std::io::Read;
use std::sync::Arc;
use tokio::sync::broadcast;

pub const DEFAULT_SCROLLBACK: usize = 10_000;

const EVENTS_CAPACITY: usize = 1024;

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
    parser: Mutex<Processor>,
}

struct TermState {
    term: Term<TermListener>,
    grid_size: GridSize,
}

struct TermListener {
    events: broadcast::Sender<TerminalEvent>,
}

impl EventListener for TermListener {
    fn send_event(&self, event: AlacEvent) {
        match event {
            AlacEvent::Title(text) => {
                let _ = self.events.send(TerminalEvent::TitleChanged(text));
            }
            AlacEvent::Exit => {
                let _ = self.events.send(TerminalEvent::Exited { code: -1 });
            }
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
        let listener = TermListener { events: tx.clone() };
        let cfg = TermConfig {
            scrolling_history: scrollback.max(1),
            ..TermConfig::default()
        };
        let dim = TermSize::new(size.cols as usize, size.rows as usize);
        let term = Term::new(cfg, &dim, listener);
        Self {
            inner: Arc::new(TerminalInner {
                id,
                state: Mutex::new(TermState { term, grid_size: size }),
                events: tx,
                parser: Mutex::new(Processor::new()),
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TerminalEvent> {
        self.inner.events.subscribe()
    }

    pub fn feed(&self, bytes: &[u8]) {
        let mut parser = self.inner.parser.lock();
        let mut state = self.inner.state.lock();
        parser.advance(&mut state.term, bytes);
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
        let columns = grid.columns();
        let rows = screen_lines as u16;
        let cols = columns as u16;
        let mut cells = Vec::with_capacity((rows as usize) * (cols as usize));
        for line_idx in 0..screen_lines {
            let line = &grid[Line(line_idx as i32)];
            for col_idx in 0..columns {
                let cell = &line[Column(col_idx)];
                cells.push(alac_cell_to_cell(cell));
            }
        }
        GridSnapshot { rows, cols, cells }
    }

    pub fn scrollback_len(&self) -> usize {
        let state = self.inner.state.lock();
        let total = state.term.grid().total_lines();
        let visible = state.term.grid().screen_lines();
        total.saturating_sub(visible)
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
        std::thread::Builder::new()
            .name(format!("term-event-{}", terminal.id()))
            .spawn(move || run_event_loop(inner, reader))
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
}

fn run_event_loop(inner: Arc<TerminalInner>, mut reader: Box<dyn Read + Send>) {
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let _ = inner.events.send(TerminalEvent::Output(TerminalOutput {
                    bytes: buf[..n].to_vec(),
                }));
                let mut parser = inner.parser.lock();
                let mut state = inner.state.lock();
                parser.advance(&mut state.term, &buf[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

fn alac_cell_to_cell(c: &AlacCell) -> Cell {
    Cell {
        ch: c.c,
        fg: color_u32(&c.fg),
        bg: color_u32(&c.bg),
        flags: c.flags.bits(),
    }
}

fn color_u32(color: &alacritty_terminal::vte::ansi::Color) -> u32 {
    use alacritty_terminal::vte::ansi::Color;
    match color {
        Color::Spec(rgb) => ((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | (rgb.b as u32),
        Color::Indexed(_) | Color::Named(_) => 0,
    }
}
