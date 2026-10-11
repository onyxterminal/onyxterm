use std::path::PathBuf;
use std::time::{Duration, Instant};

use domain::TerminalSessionId;
use terminal_core::event::{
    CellColor, CursorPosition, GridPoint, GridSize, InputModes, SelectionKind, SelectionSpan, TerminalEvent,
};
use terminal_core::{Side, ViMotion};
use terminal_core::pty::{NativePty, Pty, PtyHandle};
use terminal_core::service::{InMemoryTerminalService, ShellCommand, TerminalService};
use terminal_core::vt::Terminal;
use tokio::time::timeout;

const SHELL: &str = "/bin/sh";

fn cwd() -> PathBuf {
    PathBuf::from("/tmp")
}

fn collect_until(
    reader: &mut Box<dyn std::io::Read + Send>,
    marker: &[u8],
    deadline: Duration,
) -> Vec<u8> {
    let mut buf = [0u8; 1024];
    let mut collected = Vec::new();
    let start = Instant::now();
    while start.elapsed() < deadline
        && collected.windows(marker.len()).position(|w| w == marker).is_none()
    {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => collected.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    collected
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pty_spawn_echoes_stdout() {
    let pty = NativePty::new();
    let handle: PtyHandle = pty
        .spawn(
            PathBuf::from(SHELL),
            vec!["-c".to_string(), "echo hello-onyxterm-test".to_string()],
            cwd(),
            Default::default(),
            GridSize::new(80, 24),
        )
        .expect("spawn");
    let mut reader = handle.take_reader().expect("reader");
    let collected =
        collect_until(&mut reader, b"hello-onyxterm-test", Duration::from_secs(5));
    assert!(
        collected
            .windows(b"hello-onyxterm-test".len())
            .any(|w| w == b"hello-onyxterm-test"),
        "expected marker in output, got: {:?}",
        String::from_utf8_lossy(&collected)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pty_resize_does_not_kill_child() {
    let pty = NativePty::new();
    let handle = pty
        .spawn(
            PathBuf::from(SHELL),
            vec!["-i".to_string()],
            cwd(),
            Default::default(),
            GridSize::new(80, 24),
        )
        .expect("spawn");
    handle.resize(40, 100).expect("resize 1");
    handle.resize(50, 120).expect("resize 2");
    let mut reader = handle.take_reader().expect("reader");
    handle.write(b"echo resized-marker\n").expect("write");
    let collected = collect_until(&mut reader, b"resized-marker", Duration::from_secs(5));
    assert!(
        collected
            .windows(b"resized-marker".len())
            .any(|w| w == b"resized-marker"),
        "expected resize marker in output, got: {:?}",
        String::from_utf8_lossy(&collected)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pty_terminate_sends_sigterm_then_sigkill() {
    let pty = NativePty::new();
    let handle = pty
        .spawn(
            PathBuf::from(SHELL),
            vec!["-c".to_string(), "trap '' TERM; sleep 60".to_string()],
            cwd(),
            Default::default(),
            GridSize::new(80, 24),
        )
        .expect("spawn");
    let pid = handle.child_pid().expect("pid");
    let start = Instant::now();
    handle.kill().expect("kill");
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(1500),
        "terminate took too long: {elapsed:?}"
    );
    for _ in 0..50 {
        let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if rc != 0 {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("child {pid} still alive after terminate");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_scrollback_caps_lines() {
    let id = TerminalSessionId::new();
    let terminal = Terminal::new(id, GridSize::new(80, 24), 10_000);
    let mut buf = Vec::with_capacity(80 * 15_000);
    for _ in 0..15_000 {
        buf.extend_from_slice(b"x\n");
    }
    terminal.feed(&buf);
    assert_eq!(
        terminal.scrollback_len(),
        10_000,
        "scrollback should be capped at 10000"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn terminal_event_channel_delivers_grid_events() {
    let service = InMemoryTerminalService::new(Box::new(NativePty::new()));
    let id = service
        .spawn(PathBuf::from(SHELL), cwd(), GridSize::new(40, 5))
        .await
        .expect("spawn");
    let mut rx = service.events(id).expect("events receiver");
    service
        .write(id, b"echo marker-onyxterm-2026\n")
        .await
        .expect("write 1");
    service
        .resize(id, GridSize::new(20, 5))
        .await
        .expect("resize trigger");
    let mut saw_marker = false;
    let mut saw_resized = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !(saw_marker && saw_resized) {
        match timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Ok(event)) => match event {
                terminal_core::event::TerminalEvent::Output(out) => {
                    if String::from_utf8_lossy(&out.bytes).contains("marker-onyxterm-2026") {
                        saw_marker = true;
                    }
                }
                terminal_core::event::TerminalEvent::Resized { cols, rows }
                    if cols == 20 && rows == 5 =>
                {
                    saw_resized = true;
                }
                _ => {}
            },
            _ => break,
        }
    }
    assert!(saw_marker, "expected Output event with marker");
    assert!(saw_resized, "expected Resized event for the new dimensions");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn service_terminates_only_target_session() {
    let service = InMemoryTerminalService::new(Box::new(NativePty::new()));
    let keep = service
        .spawn(PathBuf::from(SHELL), cwd(), GridSize::new(80, 24))
        .await
        .expect("spawn keep");
    let drop = service
        .spawn(PathBuf::from(SHELL), cwd(), GridSize::new(80, 24))
        .await
        .expect("spawn drop");
    service.terminate(drop).await.expect("terminate drop");
    service
        .write(keep, b"echo keep-after-drop-marker\n")
        .await
        .expect("write keep");
    let mut rx = service.events(keep).expect("events keep");
    let mut saw = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !saw {
        match timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Ok(terminal_core::event::TerminalEvent::Output(out))) => {
                if String::from_utf8_lossy(&out.bytes).contains("keep-after-drop-marker") {
                    saw = true;
                }
            }
            _ => break,
        }
    }
    assert!(saw, "kept session should still respond");
}

#[test]
fn snapshot_preserves_indexed_and_truecolor_and_cursor() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 3), 100);
    terminal.feed(b"\x1b[31mA\x1b[38;5;200mB\x1b[38;2;1;2;3mC\x1b[0m\x1b[7mD");
    let snapshot = terminal.snapshot_grid();
    assert_eq!(snapshot.cells[0].fg, CellColor::Indexed(1));
    assert_eq!(snapshot.cells[1].fg, CellColor::Indexed(200));
    assert_eq!(snapshot.cells[2].fg, CellColor::Rgb(1, 2, 3));
    assert_eq!(snapshot.cells[3].fg, CellColor::Default);
    assert!(snapshot.cells[3].is_inverse());
    assert_eq!(snapshot.cursor, Some(CursorPosition { row: 0, col: 4 }));
}

#[test]
fn input_modes_track_bracketed_paste_and_app_cursor() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 3), 100);
    assert_eq!(terminal.input_modes(), InputModes::default());
    terminal.feed(b"\x1b[?2004h\x1b[?1h");
    let modes = terminal.input_modes();
    assert!(modes.bracketed_paste);
    assert!(modes.app_cursor);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shell_exit_emits_exited_event() {
    let service = InMemoryTerminalService::new(Box::new(NativePty::new()));
    let command = ShellCommand {
        program: PathBuf::from(SHELL),
        args: vec!["-c".to_string(), "sleep 0.2".to_string()],
        cwd: cwd(),
    };
    let id = service.spawn_command(command, GridSize::new(20, 5)).expect("spawn");
    let mut rx = service.events(id).expect("events receiver");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut exited = false;
    while Instant::now() < deadline && !exited {
        if let Ok(Ok(TerminalEvent::Exited { .. })) =
            timeout(Duration::from_millis(500), rx.recv()).await
        {
            exited = true;
        }
    }
    assert!(exited, "expected Exited event after the shell finishes");
}

fn numbered_lines(count: usize) -> Vec<u8> {
    (0..count).flat_map(|n| format!("{n}\r\n").into_bytes()).collect()
}

fn first_char_of_row(terminal: &Terminal, row: usize) -> char {
    let snapshot = terminal.snapshot_grid();
    snapshot.cells[row * usize::from(snapshot.cols)].ch
}

#[test]
fn scroll_offset_clamps_to_history_and_bottom() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 5), 100);
    terminal.feed(&numbered_lines(20));
    let history = terminal.scrollback_len();
    terminal.scroll_lines(1_000);
    assert_eq!(terminal.display_offset(), history);
    assert_eq!(first_char_of_row(&terminal, 0), '0');
    terminal.scroll_lines(-3);
    assert_eq!(terminal.display_offset(), history - 3);
    terminal.scroll_lines(-1_000);
    assert_eq!(terminal.display_offset(), 0);
    terminal.scroll_lines(2);
    terminal.scroll_to_bottom();
    assert_eq!(terminal.display_offset(), 0);
}

#[test]
fn selection_range_maps_to_text_and_viewport_span() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(20, 3), 100);
    terminal.feed(b"hello world");
    terminal.start_selection(SelectionKind::Simple, GridPoint { row: 0, col: 0 }, Side::Left);
    terminal.update_selection(GridPoint { row: 0, col: 4 }, Side::Right);
    assert_eq!(terminal.selection_text().as_deref(), Some("hello"));
    let span = terminal.snapshot_grid().selection.expect("selection visible");
    assert_eq!(span.start, GridPoint { row: 0, col: 0 });
    assert_eq!(span.end, GridPoint { row: 0, col: 4 });
    terminal.start_selection(SelectionKind::Word, GridPoint { row: 0, col: 8 }, Side::Left);
    assert_eq!(terminal.selection_text().as_deref(), Some("world"));
    terminal.clear_selection();
    assert_eq!(terminal.selection_text(), None);
    assert_eq!(terminal.snapshot_grid().selection, None);
}

#[test]
fn selection_follows_scrolled_viewport() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 5), 100);
    terminal.feed(&numbered_lines(20));
    terminal.scroll_lines(1_000);
    terminal.start_selection(SelectionKind::Line, GridPoint { row: 0, col: 0 }, Side::Left);
    assert_eq!(terminal.selection_text().as_deref(), Some("0\n"));
    terminal.scroll_to_bottom();
    assert_eq!(terminal.snapshot_grid().selection, None);
}

#[test]
fn vi_mode_moves_its_own_cursor_selects_and_restores_on_exit() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 5), 100);
    terminal.feed(&numbered_lines(20));
    terminal.enter_vi_mode();
    assert!(terminal.is_vi_mode());
    assert_eq!(terminal.snapshot_grid().cursor, Some(CursorPosition { row: 4, col: 0 }));
    terminal.vi_goto_top();
    assert_eq!(terminal.display_offset(), terminal.scrollback_len());
    assert_eq!(terminal.snapshot_grid().cursor, Some(CursorPosition { row: 0, col: 0 }));
    terminal.toggle_vi_selection(SelectionKind::Line);
    terminal.vi_motion(ViMotion::Down);
    assert_eq!(terminal.selection_text().as_deref(), Some("0\n1\n"));
    terminal.toggle_vi_selection(SelectionKind::Line);
    assert_eq!(terminal.selection_text(), None);
    terminal.vi_goto_bottom();
    assert_eq!(terminal.display_offset(), 0);
    terminal.vi_scroll_lines(2);
    assert_eq!(terminal.display_offset(), 2);
    terminal.toggle_vi_selection(SelectionKind::Simple);
    terminal.exit_vi_mode();
    assert!(!terminal.is_vi_mode());
    assert_eq!(terminal.display_offset(), 0);
    assert_eq!(terminal.selection_text(), None);
}

#[test]
fn selection_span_reports_columns_per_row() {
    let span = SelectionSpan { start: GridPoint { row: 1, col: 3 }, end: GridPoint { row: 3, col: 1 } };
    assert_eq!(span.columns_on(0, 10), None);
    assert_eq!(span.columns_on(1, 10), Some(3..10));
    assert_eq!(span.columns_on(2, 10), Some(0..10));
    assert_eq!(span.columns_on(3, 10), Some(0..2));
    assert_eq!(span.columns_on(4, 10), None);
}

#[test]
fn input_modes_track_alt_screen_and_mouse_reporting() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 3), 100);
    terminal.feed(b"\x1b[?1049h\x1b[?1000h");
    let modes = terminal.input_modes();
    assert!(modes.alt_screen);
    assert!(modes.mouse_reporting);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shell_exit_reports_real_exit_code() {
    let service = InMemoryTerminalService::new(Box::new(NativePty::new()));
    let command = ShellCommand {
        program: PathBuf::from(SHELL),
        args: vec!["-c".to_string(), "exit 3".to_string()],
        cwd: cwd(),
    };
    let id = service.spawn_command(command, GridSize::new(20, 5)).expect("spawn");
    let mut rx = service.events(id).expect("events receiver");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut code = None;
    while Instant::now() < deadline && code.is_none() {
        if let Ok(Ok(TerminalEvent::Exited { code: exit })) = timeout(Duration::from_millis(500), rx.recv()).await {
            code = Some(exit);
        }
    }
    assert_eq!(code, Some(3));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_job_detected_while_child_runs() {
    let service = InMemoryTerminalService::new(Box::new(NativePty::new()));
    let command = ShellCommand { program: PathBuf::from(SHELL), args: vec!["-i".to_string()], cwd: cwd() };
    let id = service.spawn_command(command, GridSize::new(40, 5)).expect("spawn");
    let handle = service.handle_for(id).expect("handle");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!handle.has_foreground_job(), "idle shell owns the foreground");
    handle.write(b"sleep 3\n").expect("write");
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut detected = false;
    while Instant::now() < deadline && !detected {
        tokio::time::sleep(Duration::from_millis(50)).await;
        detected = handle.has_foreground_job();
    }
    assert!(detected, "sleep should own the foreground process group");
    service.terminate_session(id).expect("terminate");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_job_reports_command_line() {
    let service = InMemoryTerminalService::new(Box::new(NativePty::new()));
    let command = ShellCommand { program: PathBuf::from(SHELL), args: vec!["-i".to_string()], cwd: cwd() };
    let id = service.spawn_command(command, GridSize::new(40, 5)).expect("spawn");
    let handle = service.handle_for(id).expect("handle");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(handle.foreground_job(), None);
    handle.write(b"sleep 3\n").expect("write");
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut argv = None;
    while Instant::now() < deadline && argv.as_deref() != Some(&["sleep".to_string(), "3".to_string()][..]) {
        tokio::time::sleep(Duration::from_millis(50)).await;
        argv = handle.foreground_job().map(|job| job.argv);
    }
    assert_eq!(argv, Some(vec!["sleep".to_string(), "3".to_string()]));
    service.terminate_session(id).expect("terminate");
}

#[test]
fn current_dir_follows_shell_cd() {
    let service = InMemoryTerminalService::new(Box::new(NativePty::new()));
    let target = std::fs::canonicalize(std::env::temp_dir()).expect("temp dir");
    let command = ShellCommand {
        program: PathBuf::from(SHELL),
        args: vec!["-c".to_string(), format!("cd '{}' && sleep 2", target.display())],
        cwd: PathBuf::from("/"),
    };
    let id = service.spawn_command(command, GridSize::new(20, 5)).expect("spawn");
    let handle = service.handle_for(id).expect("handle");
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut seen = None;
    while Instant::now() < deadline && seen.as_ref() != Some(&target) {
        std::thread::sleep(Duration::from_millis(50));
        seen = handle.current_dir();
    }
    assert_eq!(seen, Some(target));
    service.terminate_session(id).expect("terminate");
}

#[test]
fn select_all_covers_scrollback_and_screen() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(20, 2), 100);
    terminal.feed(b"first\r\nsecond\r\nthird");
    terminal.select_all();
    let text = terminal.selection_text().expect("selection");
    assert!(text.contains("first") && text.contains("third"), "got {text:?}");
}
