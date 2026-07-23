//! Stdin input reading for the thin client.
//!
//! Reads stdin bytes and forwards framed input to the main event loop.
//! Unlike the monolithic Oh My Herdr, the thin client does NOT parse input into
//! key/mouse/paste events. It keeps enough byte-framing state to avoid splitting
//! terminal control strings, then sends bytes to the server as `ClientMessage::Input`.
//! The server handles semantic parsing.
//!
//! This is simpler and more reliable because:
//! - The server has the same input parsing code
//! - We avoid duplicating parsing logic in the client
//! - Host terminal control replies can be buffered or discarded before they leak

#[cfg(unix)]
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(unix)]
use std::os::fd::AsRawFd;
use tokio::sync::mpsc;

use super::ClientLoopEvent;
#[cfg(any(windows, test))]
mod windows_vti;

// ---------------------------------------------------------------------------
// Stdin reader thread
// ---------------------------------------------------------------------------

/// Reads raw bytes from stdin and sends them to the main event loop.
///
/// This runs on a dedicated thread because stdin reading is blocking.
/// The main loop receives the raw bytes and forwards them as
/// `ClientMessage::Input` to the server.
pub fn stdin_reader_loop(
    event_tx: mpsc::Sender<ClientLoopEvent>,
    should_quit: &Arc<AtomicBool>,
    host_color_query_sent: bool,
    host_mouse_capture_active: Arc<AtomicBool>,
) {
    #[cfg(windows)]
    {
        let _ = (host_color_query_sent, host_mouse_capture_active);
        return windows_stdin_reader_loop(event_tx, should_quit, host_color_query_sent);
    }

    #[cfg(unix)]
    unix_stdin_reader_loop(
        event_tx,
        should_quit,
        host_color_query_sent,
        host_mouse_capture_active,
    );
}
#[cfg(windows)]
fn windows_stdin_reader_loop(
    event_tx: mpsc::Sender<ClientLoopEvent>,
    should_quit: &Arc<AtomicBool>,
    _host_color_query_sent: bool,
) {
    if !super::windows_vti_input_backend_enabled() {
        windows_crossterm_reader_loop(event_tx, should_quit);
        return;
    }

    match windows_vti::console_input_handle() {
        Ok(handle) if windows_vti::virtual_terminal_input_enabled(handle) => {
            windows_vti::raw_console_reader_loop(handle, event_tx, should_quit);
        }
        _ => windows_crossterm_reader_loop(event_tx, should_quit),
    }
}

#[cfg(windows)]
fn windows_crossterm_reader_loop(
    event_tx: mpsc::Sender<ClientLoopEvent>,
    should_quit: &Arc<AtomicBool>,
) {
    use std::time::Duration;

    let mut framer = crate::raw_input::RawInputFramer::for_host_input();

    while !should_quit.load(Ordering::Acquire) {
        match crossterm::event::poll(Duration::from_millis(10)) {
            Ok(true) => {}
            Ok(false) => {
                if framer.has_pending_input() {
                    if !send_windows_raw_events(framer.flush_timeout(), &event_tx) {
                        return;
                    }
                }

                continue;
            }
            Err(_) => break,
        }

        let event = match crossterm::event::read() {
            Ok(event) => event,
            Err(_) => break,
        };

        let raw_sequence_pending = framer.has_pending_input();
        if let Some(bytes) = windows_key_raw_bytes(&event, raw_sequence_pending) {
            if !send_windows_raw_events(framer.push(&bytes), &event_tx) {
                return;
            }
            continue;
        }

        if framer.has_pending_input() {
            if !send_windows_raw_events(framer.flush_timeout(), &event_tx) {
                return;
            }
        }

        let Some(event) = crate::protocol::ClientInputEvent::from_crossterm(event) else {
            continue;
        };
        if event_tx
            .blocking_send(ClientLoopEvent::StdinEvents(vec![event]))
            .is_err()
        {
            return;
        }
    }

    if framer.has_pending_input() {
        let _ = send_windows_raw_events(framer.flush_timeout(), &event_tx);
    }
}

#[cfg(windows)]
fn windows_key_raw_bytes(
    event: &crossterm::event::Event,
    raw_sequence_pending: bool,
) -> Option<Vec<u8>> {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

    let Event::Key(key) = event else {
        return None;
    };
    if key.kind == KeyEventKind::Release {
        return None;
    }

    match key.code {
        KeyCode::Esc if key.modifiers.is_empty() => Some(vec![0x1b]),
        KeyCode::Char('[') if !raw_sequence_pending && key.modifiers == KeyModifiers::CONTROL => {
            Some(vec![0x1b])
        }
        KeyCode::Char(ch)
            if !raw_sequence_pending
                && matches!(ch, 'i' | 'I')
                && key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            let mut buf = [0; 4];
            Some(ch.encode_utf8(&mut buf).as_bytes().to_vec())
        }
        KeyCode::Char(ch) if raw_sequence_pending || ch.is_control() => {
            let mut bytes = Vec::new();
            if key.modifiers.contains(KeyModifiers::ALT) {
                bytes.push(0x1b);
            }
            let mut buf = [0; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            Some(bytes)
        }
        _ => None,
    }
}

#[cfg(windows)]
fn send_windows_raw_events(
    events: Vec<crate::raw_input::RawInputEvent>,
    event_tx: &mpsc::Sender<ClientLoopEvent>,
) -> bool {
    let events = events
        .into_iter()
        .filter_map(windows_client_input_event_from_raw)
        .collect::<Vec<_>>();
    events.is_empty()
        || event_tx
            .blocking_send(ClientLoopEvent::StdinEvents(events))
            .is_ok()
}

#[cfg(any(windows, test))]
fn windows_client_input_event_from_raw(
    event: crate::raw_input::RawInputEvent,
) -> Option<crate::protocol::ClientInputEvent> {
    match event {
        crate::raw_input::RawInputEvent::Key(key) => Some(crate::protocol::ClientInputEvent::Key {
            code: crate::protocol::ClientKeyCode::from_crossterm(key.code)?,
            modifiers: key.modifiers.bits(),
            kind: crate::protocol::ClientKeyKind::from_crossterm(key.kind),
        }),
        crate::raw_input::RawInputEvent::Mouse(mouse) => {
            Some(crate::protocol::ClientInputEvent::Mouse {
                kind: crate::protocol::ClientMouseKind::from_crossterm(mouse.kind)?,
                column: mouse.column,
                row: mouse.row,
                modifiers: mouse.modifiers.bits(),
            })
        }
        crate::raw_input::RawInputEvent::Paste(text) => {
            Some(crate::protocol::ClientInputEvent::Paste(text))
        }
        crate::raw_input::RawInputEvent::OuterFocusGained => {
            Some(crate::protocol::ClientInputEvent::FocusGained)
        }
        crate::raw_input::RawInputEvent::OuterFocusLost => {
            Some(crate::protocol::ClientInputEvent::FocusLost)
        }
        crate::raw_input::RawInputEvent::HostDefaultColor { .. }
        | crate::raw_input::RawInputEvent::HostPaletteColor { .. }
        | crate::raw_input::RawInputEvent::HostCursorColor { .. }
        | crate::raw_input::RawInputEvent::Unsupported => None,
    }
}

#[cfg(unix)]
fn unix_stdin_reader_loop(
    event_tx: mpsc::Sender<ClientLoopEvent>,
    should_quit: &Arc<AtomicBool>,
    host_color_query_sent: bool,
    host_mouse_capture_active: Arc<AtomicBool>,
) {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut scratch = [0u8; 4096];
    let mut framer = crate::raw_input::RawInputByteFramer::for_host_input();
    if host_color_query_sent {
        framer.host_color_query_sent();
    }

    while !should_quit.load(Ordering::Acquire) {
        match reader.read(&mut scratch) {
            Ok(0) => break,
            Ok(n) => {
                for data in framer.push(&scratch[..n]) {
                    if event_tx
                        .blocking_send(ClientLoopEvent::StdinInput(data))
                        .is_err()
                    {
                        return;
                    }
                }

                let timeout_ms = idle_flush_timeout_ms(
                    &framer,
                    host_mouse_capture_active.load(Ordering::Acquire),
                );
                if stdin_read_ready(&reader, timeout_ms) == Some(false) {
                    let had_pending = framer.has_pending_input();
                    let chunks = framer.flush_timeout();
                    let held_escape = had_pending && chunks.is_empty();
                    for data in chunks {
                        if event_tx
                            .blocking_send(ClientLoopEvent::StdinInput(data))
                            .is_err()
                        {
                            return;
                        }
                    }
                    if held_escape
                        && stdin_read_ready(
                            &reader,
                            crate::raw_input::RAW_INPUT_IDLE_FLUSH_TIMEOUT_MS,
                        ) == Some(false)
                    {
                        for data in framer.flush_timeout() {
                            if event_tx
                                .blocking_send(ClientLoopEvent::StdinInput(data))
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                }
            }
            Err(err) => {
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }
        }
    }
}

#[cfg(unix)]
fn idle_flush_timeout_ms(
    framer: &crate::raw_input::RawInputByteFramer,
    host_mouse_capture_active: bool,
) -> i32 {
    if host_mouse_capture_active
        && (framer.has_pending_lone_escape() || framer.has_pending_incomplete_sgr_mouse_sequence())
    {
        crate::raw_input::MOUSE_ACTIVE_ESCAPE_SEQUENCE_FLUSH_TIMEOUT_MS
    } else {
        crate::raw_input::RAW_INPUT_IDLE_FLUSH_TIMEOUT_MS
    }
}

#[cfg(unix)]
fn stdin_read_ready<R: AsRawFd>(reader: &R, timeout_ms: i32) -> Option<bool> {
    poll_read_ready(reader.as_raw_fd(), timeout_ms)
}

#[cfg(not(unix))]
fn stdin_read_ready<R>(_reader: &R, _timeout_ms: i32) -> Option<bool> {
    None
}

#[cfg(unix)]
fn poll_read_ready(fd: i32, timeout_ms: i32) -> Option<bool> {
    #[repr(C)]
    struct PollFd {
        fd: i32,
        events: i16,
        revents: i16,
    }

    unsafe extern "C" {
        fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
    }

    const POLLIN: i16 = 0x0001;

    let mut pfd = PollFd {
        fd,
        events: POLLIN,
        revents: 0,
    };

    let result = unsafe { poll(&mut pfd as *mut PollFd, 1, timeout_ms) };
    if result < 0 {
        None
    } else {
        Some(result > 0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    // The stdin reader thread is hard to unit test since it reads from actual stdin.
    // Integration tests will verify the full client→server input flow.
    // Here we test the event type construction.

    use super::*;

    #[cfg(unix)]
    #[test]
    fn raw_input_idle_flush_timeout_keeps_escape_responsive() {
        let timeout_ms = std::hint::black_box(crate::raw_input::RAW_INPUT_IDLE_FLUSH_TIMEOUT_MS);
        assert!(timeout_ms <= 20);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn coalesced_escape_leaves_next_escape_pending_for_client_flush() {
        let mut framer = crate::raw_input::RawInputByteFramer::for_host_input();

        assert_eq!(framer.push(b"\x1b\x1b"), vec![b"\x1b".to_vec()]);
        assert!(framer.has_pending_input());
        assert_eq!(framer.flush_timeout(), vec![b"\x1b".to_vec()]);
    }

    #[cfg(unix)]
    #[test]
    fn mouse_active_escape_sequences_get_longer_reassembly_window() {
        let mut escape = crate::raw_input::RawInputByteFramer::default();
        assert!(escape.push(b"\x1b").is_empty());
        let mut mouse = crate::raw_input::RawInputByteFramer::default();
        assert!(mouse.push(b"\x1b[<3").is_empty());
        let mut unrelated = crate::raw_input::RawInputByteFramer::default();
        assert!(unrelated.push(b"\x1b[49:33;2:").is_empty());

        for framer in [&escape, &mouse, &unrelated] {
            assert_eq!(
                idle_flush_timeout_ms(framer, false),
                crate::raw_input::RAW_INPUT_IDLE_FLUSH_TIMEOUT_MS
            );
        }
        for framer in [&escape, &mouse] {
            assert_eq!(
                idle_flush_timeout_ms(framer, true),
                crate::raw_input::MOUSE_ACTIVE_ESCAPE_SEQUENCE_FLUSH_TIMEOUT_MS
            );
        }
        assert_eq!(
            idle_flush_timeout_ms(&unrelated, true),
            crate::raw_input::RAW_INPUT_IDLE_FLUSH_TIMEOUT_MS
        );

        let mouse_timeout_ms =
            std::hint::black_box(crate::raw_input::MOUSE_ACTIVE_ESCAPE_SEQUENCE_FLUSH_TIMEOUT_MS);
        assert!(mouse_timeout_ms > 100);
    }

    #[test]
    fn stdin_input_event_carries_raw_bytes() {
        let data = vec![0x1b, b'[', b'A']; // Up arrow escape sequence
        let event = ClientLoopEvent::StdinInput(data.clone());
        match event {
            ClientLoopEvent::StdinInput(d) => assert_eq!(d, data),
            _ => panic!("expected StdinInput event"),
        }
    }
}
