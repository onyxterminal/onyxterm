use std::path::PathBuf;
use std::time::{Duration, Instant};

use domain::TerminalSessionId;
use terminal_core::event::{
    CellColor, CellPixels, ColorPalette, CursorPosition, GridPoint, GridSize, InputModes, KittyKeyboard,
    MouseModes, SelectionKind, SelectionSpan, TerminalEvent,
};
use terminal_core::{BlockStatus, Rgb, ShellPhase, Side, ViMotion};
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
fn snapshot_reports_italic_dim_and_strikeout_attributes() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 3), 100);
    terminal.feed(b"\x1b[3mI\x1b[0m\x1b[2mD\x1b[0m\x1b[9mS\x1b[0mP");
    let cells = terminal.snapshot_grid().cells;
    assert!(cells[0].is_italic() && !cells[0].is_dim());
    assert!(cells[1].is_dim() && !cells[1].is_strikeout());
    assert!(cells[2].is_strikeout() && !cells[2].is_italic());
    assert!(!cells[3].is_italic() && !cells[3].is_dim() && !cells[3].is_strikeout());
}

#[test]
fn shell_marks_tag_the_typed_command_and_record_its_outcome() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(20, 4), 100);
    let mut events = terminal.subscribe();
    terminal.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07false\r\n\x1b]133;C;cmdline=false\x07oops\r\n\x1b]133;D;1\x07");
    terminal.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07");
    let cells = terminal.snapshot_grid().cells;
    let block = cells[2].block.expect("typed command is tagged");
    assert!(cells[..2].iter().all(|cell| cell.block.is_none()));
    assert!(cells[2..7].iter().all(|cell| cell.block == Some(block)));
    assert!(cells[20..40].iter().all(|cell| cell.block.is_none()));
    let finished = terminal.with_command_blocks(|blocks| blocks.block(block).cloned()).expect("block recorded");
    assert_eq!(finished.command, "false");
    let BlockStatus::Finished(outcome) = finished.status else {
        panic!("block should be finished");
    };
    assert_eq!(outcome.exit_code, Some(1));
    assert!(matches!(terminal.shell_phase(), ShellPhase::Editing(next) if next != block));
    assert!(matches!(events.try_recv(), Ok(TerminalEvent::CommandStarted(id)) if id == block));
    assert!(matches!(events.try_recv(), Ok(TerminalEvent::CommandFinished(done)) if done.id == block));
}

#[test]
fn autosuggestion_completes_typed_input_from_pane_history() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(40, 6), 100);
    let prompt = b"\x1b]133;A\x07$ \x1b]133;B\x07";
    terminal.feed(prompt);
    terminal.feed(b"cargo test --workspace\r\n\x1b]133;C;cmdline=cargo test --workspace\x07\x1b]133;D;0\x07");
    terminal.feed(prompt);
    assert_eq!(terminal.autosuggestion(), None);
    terminal.feed(b"cargo t");
    assert_eq!(terminal.autosuggestion().as_deref(), Some("est --workspace"));
    terminal.feed(b"x");
    assert_eq!(terminal.autosuggestion(), None);
}

#[test]
fn autosuggestion_needs_the_cursor_at_the_end_of_the_input() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(40, 6), 100);
    let prompt = b"\x1b]133;A\x07$ \x1b]133;B\x07";
    terminal.feed(prompt);
    terminal.feed(b"git status\r\n\x1b]133;C;cmdline=git status\x07\x1b]133;D;0\x07");
    terminal.feed(prompt);
    terminal.feed(b"git s\x08\x08");
    assert_eq!(terminal.autosuggestion(), None);
    terminal.feed(b"\x1b[C\x1b[C");
    assert_eq!(terminal.autosuggestion().as_deref(), Some("tatus"));
}

#[test]
fn input_modes_track_bracketed_paste_and_app_cursor() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 3), 100);
    assert_eq!(terminal.input_modes(), InputModes { alternate_scroll: true, ..InputModes::default() });
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
    assert!(modes.mouse.is_reporting());
}

#[test]
fn input_modes_track_mouse_protocol_focus_and_alternate_scroll() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 3), 100);
    terminal.feed(b"\x1b[?1003h\x1b[?1006h\x1b[?1004h\x1b[?1007l");
    let modes = terminal.input_modes();
    assert_eq!(modes.mouse, MouseModes { report_motion: true, sgr: true, ..MouseModes::default() });
    assert!(modes.focus_reporting);
    assert!(!modes.alternate_scroll);
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

#[test]
fn find_walks_scrollback_matches_top_down_ignoring_case() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(20, 3), 100);
    terminal.feed(b"Error one\r\nok\r\nerror two\r\nfine\r\nERROR three\r\n");
    let found = terminal.find_matches("error", 100);
    let lines: Vec<i32> = found.iter().map(|hit| hit.start.line).collect();
    assert_eq!(found.len(), 3);
    assert!(lines.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!((found[0].start.col, found[0].end.col), (0, 4));
    assert!(terminal.find_matches("err.r", 100).is_empty());
    assert_eq!(terminal.find_matches("error", 2).len(), 2);
}

#[test]
fn revealing_a_scrollback_match_scrolls_it_into_view() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(20, 3), 100);
    terminal.feed(b"needle\r\n1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");
    let hit = terminal.find_matches("needle", 10)[0];
    assert!(hit.start.line < 0);
    terminal.reveal_line(hit.start.line);
    let row = hit.start.line + terminal.display_offset() as i32;
    assert!((0..3).contains(&row));
}

fn gray(level: u8) -> Rgb {
    Rgb { r: level, g: level, b: level }
}

#[test]
fn cursor_position_and_device_attribute_queries_queue_replies() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 3), 100);
    terminal.feed(b"ab\x1b[6n\x1b[c");
    assert_eq!(terminal.take_replies(), b"\x1b[1;3R\x1b[?6c".to_vec());
    assert!(terminal.take_replies().is_empty());
}

#[test]
fn color_queries_answer_from_the_palette_and_osc_overrides() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 3), 100);
    terminal.set_palette(ColorPalette {
        foreground: gray(0xee),
        background: gray(0x11),
        cursor: gray(0x80),
        ansi: [gray(1); 16],
    });
    terminal.feed(b"\x1b]11;?\x07\x1b]4;196;?\x07\x1b]10;#123456\x07\x1b]10;?\x07");
    let replies = String::from_utf8(terminal.take_replies()).expect("utf8");
    assert_eq!(
        replies,
        "\x1b]11;rgb:1111/1111/1111\x07\x1b]4;196;rgb:ffff/0000/0000\x07\x1b]10;rgb:1212/3434/5656\x07"
    );
}

#[test]
fn text_area_size_query_reports_pixels() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 3), 100);
    terminal.set_cell_pixels(CellPixels { width: 8, height: 16 });
    terminal.feed(b"\x1b[14t");
    assert_eq!(terminal.take_replies(), b"\x1b[4;48;80t".to_vec());
}

#[test]
fn osc52_copy_surfaces_a_clipboard_event_and_paste_queries_stay_unanswered() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 3), 100);
    let mut events = terminal.subscribe();
    terminal.feed(b"\x1b]52;c;aGVsbG8=\x07\x1b]52;c;?\x07");
    assert!(matches!(events.try_recv(), Ok(TerminalEvent::ClipboardStore(text)) if text == "hello"));
    assert!(terminal.take_replies().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_writes_query_replies_back_to_the_pty() {
    let service = InMemoryTerminalService::new(Box::new(NativePty::new()));
    let script = "stty raw -echo; printf '\\033[6n'; reply=$(dd bs=1 count=6 2>/dev/null); \
                  printf 'reply:%s:end' \"$(printf %s \"$reply\" | tr '\\033' E)\"; sleep 1";
    let command = ShellCommand {
        program: PathBuf::from(SHELL),
        args: vec!["-c".to_string(), script.to_string()],
        cwd: cwd(),
    };
    let id = service.spawn_command(command, GridSize::new(20, 5)).expect("spawn");
    let mut rx = service.events(id).expect("events receiver");
    let mut output = String::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !output.contains(":end") {
        if let Ok(Ok(TerminalEvent::Output(out))) = timeout(Duration::from_millis(500), rx.recv()).await {
            output.push_str(&String::from_utf8_lossy(&out.bytes));
        }
    }
    assert!(output.contains("reply:E[1;1R:end"), "unexpected output: {output:?}");
}

#[test]
fn kitty_keyboard_flags_follow_push_query_and_pop() {
    let terminal = Terminal::new(TerminalSessionId::new(), GridSize::new(10, 3), 100);
    terminal.feed(b"\x1b[>9u\x1b[?u");
    let expected = KittyKeyboard { disambiguate: true, report_all_keys: true, ..KittyKeyboard::default() };
    assert_eq!(terminal.input_modes().kitty, expected);
    assert_eq!(terminal.take_replies(), b"\x1b[?9u".to_vec());
    terminal.feed(b"\x1b[<u");
    assert_eq!(terminal.input_modes().kitty, KittyKeyboard::default());
}
