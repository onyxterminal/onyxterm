use std::path::PathBuf;
use std::time::{Duration, Instant};

use domain::TerminalSessionId;
use terminal_core::event::GridSize;
use terminal_core::pty::{NativePty, Pty, PtyHandle};
use terminal_core::service::{InMemoryTerminalService, TerminalService};
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
