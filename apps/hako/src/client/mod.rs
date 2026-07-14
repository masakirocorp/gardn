//! Thin client mode — connects to the server's client socket.
//!
//! The client:
//! - Connects to `hako-client.sock`, sends Hello with terminal size and protocol version
//! - Sets up the real terminal (raw mode, mouse capture, keyboard enhancements)
//! - Receives Frame messages and blits them to the terminal (diff against last frame)
//! - Reads stdin events (keystrokes, mouse, paste) and sends them as ClientMessage::Input
//! - Detects terminal resize and sends ClientMessage::Resize
//! - Restores terminal on exit (normal or error)
//! - Handles ServerShutdown gracefully (clean exit, informative message to stderr)
//! - Handles server unreachable (clear error screen, not blank/hang)
//! - Forwards OSC 52 clipboard writes from server to its own stdout
//! - Displays sound/toast notifications forwarded from server

mod input;

use crate::ipc::LocalStream;
use interprocess::local_socket::traits::Stream as _;
use interprocess::TryClone as _;
use std::collections::HashSet;
use std::io::{self, Write as _};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use tracing::{debug, info, warn};

use crate::protocol::render_ansi;
use crate::protocol::{
    self, ClientKeybindings, ClientLaunchMode, ClientMessage, NotifyKind, RenderEncoding,
    ServerMessage, MAX_CLIPBOARD_IMAGE_PAYLOAD, MAX_FRAME_SIZE, MAX_GRAPHICS_FRAME_SIZE,
    PROTOCOL_VERSION,
};
use crate::server::socket_paths::client_socket_path;

static RECEIVED_KITTY_GRAPHICS_IDS: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();

// ---------------------------------------------------------------------------
// Client state
// ---------------------------------------------------------------------------

/// State tracking for the thin client.
struct ClientState {
    /// Stateful semantic-frame encoder used when the server sends FrameData.
    blit_encoder: render_ansi::BlitEncoder,
    /// Whether host mouse capture is currently active.
    mouse_capture_active: bool,
    /// The terminal size we reported to the server in our last Hello/Resize.
    reported_size: (u16, u16),
    /// Client-local sound playback config, refreshed on server request.
    sound_config: crate::config::SoundConfig,
    /// Whether this client may write Kitty graphics bytes to its host terminal.
    kitty_graphics_enabled: bool,
    /// Direct attach prefix escape state. None for full-app clients.
    attach_escape: Option<AttachEscapeState>,
    /// Whether outer focus gain should force a full host-terminal redraw.
    redraw_on_focus_gained: bool,
    /// Whether this client draws the cursor into frame cells instead of using the host cursor.
    draw_host_cursor: bool,
    /// Local-client shortcut that sends a clipboard image to a remote session.
    remote_image_paste_key: Option<(crossterm::event::KeyCode, crossterm::event::KeyModifiers)>,
}

#[derive(Debug, Default)]
struct AttachEscapeState {
    pending_prefix: bool,
}

#[derive(Debug)]
enum AttachInputAction {
    Forward(Vec<u8>),
    Detach,
    None,
}

impl AttachEscapeState {
    fn filter_input(&mut self, data: Vec<u8>) -> AttachInputAction {
        const PREFIX: u8 = 0x02; // Ctrl+B

        let mut output = Vec::with_capacity(data.len());
        for byte in data {
            if self.pending_prefix {
                self.pending_prefix = false;
                match byte {
                    b'q' => return AttachInputAction::Detach,
                    PREFIX => output.push(PREFIX),
                    other => {
                        output.push(PREFIX);
                        output.push(other);
                    }
                }
                continue;
            }

            if byte == PREFIX {
                self.pending_prefix = true;
            } else {
                output.push(byte);
            }
        }

        if output.is_empty() {
            AttachInputAction::None
        } else {
            AttachInputAction::Forward(output)
        }
    }
}

#[cfg(any(windows, test))]
/// Re-encode semantic console events as the raw bytes expected by the direct
/// attach escape filter. Full-app clients keep semantic `InputEvents`; direct
/// attach must continue to recognize Ctrl+B, q and literal forwarded bytes.
fn semantic_events_to_attach_bytes(events: &[crate::protocol::ClientInputEvent]) -> Vec<u8> {
    events
        .iter()
        .filter_map(semantic_event_to_attach_bytes)
        .flatten()
        .collect()
}

#[cfg(any(windows, test))]
fn semantic_event_to_attach_bytes(event: &crate::protocol::ClientInputEvent) -> Option<Vec<u8>> {
    use crate::protocol::{ClientInputEvent, ClientKeyCode, ClientKeyKind};
    use crossterm::event::KeyModifiers;

    match event {
        ClientInputEvent::Key {
            code,
            modifiers,
            kind,
        } if !matches!(kind, ClientKeyKind::Release) => {
            let mut bytes = Vec::new();
            let modifiers = KeyModifiers::from_bits_truncate(*modifiers);
            if modifiers.contains(KeyModifiers::ALT) {
                bytes.push(0x1b);
            }
            match code {
                ClientKeyCode::Char(ch) => {
                    if modifiers.contains(KeyModifiers::CONTROL) {
                        let control = match ch.to_ascii_lowercase() {
                            'a'..='z' => Some((ch.to_ascii_lowercase() as u8) - b'a' + 1),
                            '[' => Some(0x1b),
                            '\\' => Some(0x1c),
                            ']' => Some(0x1d),
                            '^' => Some(0x1e),
                            '_' => Some(0x1f),
                            ' ' => Some(0),
                            _ => None,
                        };
                        if let Some(control) = control {
                            bytes.push(control);
                            return Some(bytes);
                        }
                    }
                    let mut utf8 = [0; 4];
                    bytes.extend_from_slice(ch.encode_utf8(&mut utf8).as_bytes());
                }
                ClientKeyCode::Backspace => bytes.push(0x7f),
                ClientKeyCode::Enter => bytes.push(b'\r'),
                ClientKeyCode::Left => bytes.extend_from_slice(b"\x1b[D"),
                ClientKeyCode::Right => bytes.extend_from_slice(b"\x1b[C"),
                ClientKeyCode::Up => bytes.extend_from_slice(b"\x1b[A"),
                ClientKeyCode::Down => bytes.extend_from_slice(b"\x1b[B"),
                ClientKeyCode::Home => bytes.extend_from_slice(b"\x1b[H"),
                ClientKeyCode::End => bytes.extend_from_slice(b"\x1b[F"),
                ClientKeyCode::PageUp => bytes.extend_from_slice(b"\x1b[5~"),
                ClientKeyCode::PageDown => bytes.extend_from_slice(b"\x1b[6~"),
                ClientKeyCode::Tab => bytes.push(b'\t'),
                ClientKeyCode::BackTab => bytes.extend_from_slice(b"\x1b[Z"),
                ClientKeyCode::Delete => bytes.extend_from_slice(b"\x1b[3~"),
                ClientKeyCode::Insert => bytes.extend_from_slice(b"\x1b[2~"),
                ClientKeyCode::F(n) => {
                    let sequence = match n {
                        1 => b"\x1bOP".as_slice(),
                        2 => b"\x1bOQ".as_slice(),
                        3 => b"\x1bOR".as_slice(),
                        4 => b"\x1bOS".as_slice(),
                        5 => b"\x1b[15~".as_slice(),
                        6 => b"\x1b[17~".as_slice(),
                        7 => b"\x1b[18~".as_slice(),
                        8 => b"\x1b[19~".as_slice(),
                        9 => b"\x1b[20~".as_slice(),
                        10 => b"\x1b[21~".as_slice(),
                        11 => b"\x1b[23~".as_slice(),
                        12 => b"\x1b[24~".as_slice(),
                        _ => return None,
                    };
                    bytes.extend_from_slice(sequence);
                }
                ClientKeyCode::Null => bytes.push(0),
                ClientKeyCode::Esc => bytes.push(0x1b),
            }
            Some(bytes)
        }
        ClientInputEvent::Paste(text) => Some(text.as_bytes().to_vec()),
        _ => None,
    }
}

impl ClientState {
    fn request_full_redraw(&mut self) {
        self.blit_encoder = render_ansi::BlitEncoder::new();
    }
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during client operation.
#[derive(Debug)]
pub enum ClientError {
    /// Could not connect to the server's client socket.
    ConnectionFailed(io::Error),
    /// Server rejected our handshake.
    HandshakeRejected { version: u32, error: String },
    /// Server shut down.
    ServerShutdown { reason: Option<String> },
    /// Lost connection to the server.
    ConnectionLost(io::Error),
    /// Protocol error (framing, deserialization).
    Protocol(protocol::FramingError),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::ConnectionFailed(err) => {
                write!(f, "failed to connect to server: {err}")?;
                let path = client_socket_path();
                write!(f, "\nIs hako server running? Start it with `hako server`.")?;
                write!(f, "\nSocket path: {}", path.display())
            }
            ClientError::HandshakeRejected { version, error } => {
                write!(f, "server rejected handshake (version {version}): {error}")
            }
            ClientError::ServerShutdown { reason } => {
                match reason.as_deref() {
                    Some("detached") => {
                        if let Ok(reattach_command) =
                            std::env::var(crate::remote::REATTACH_COMMAND_ENV_VAR)
                        {
                            write!(f, "detached from remote server")?;
                            write!(f, "\nRun `{reattach_command}` to reattach")?;
                        } else {
                            write!(f, "detached from server")?;
                            write!(
                                f,
                                "\nRun `{}` to reattach",
                                crate::session::local_attach_command()
                            )?;
                        }
                    }
                    _ => {
                        write!(f, "server shut down")?;
                        if let Some(reason) = reason {
                            write!(f, ": {reason}")?;
                        }
                    }
                }
                Ok(())
            }
            ClientError::ConnectionLost(err) => {
                write!(f, "lost connection to server: {err}")
            }
            ClientError::Protocol(err) => {
                write!(f, "protocol error: {err}")
            }
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ClientError::ConnectionFailed(err) => Some(err),
            ClientError::ConnectionLost(err) => Some(err),
            ClientError::Protocol(err) => Some(err),
            _ => None,
        }
    }
}

impl From<protocol::FramingError> for ClientError {
    fn from(err: protocol::FramingError) -> Self {
        match err {
            protocol::FramingError::UnexpectedEof => ClientError::ConnectionLost(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "server closed connection",
            )),
            protocol::FramingError::Io(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::UnexpectedEof
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::ConnectionAborted
                        | io::ErrorKind::BrokenPipe
                        | io::ErrorKind::NotConnected
                ) =>
            {
                ClientError::ConnectionLost(err)
            }
            err => ClientError::Protocol(err),
        }
    }
}

// ---------------------------------------------------------------------------
// Terminal setup / restore
// ---------------------------------------------------------------------------

/// Sets up the terminal for client mode (raw mode, optional mouse, keyboard enhancements).
///
/// Returns a guard that restores the terminal when dropped.
fn setup_terminal(mouse_capture: bool) -> io::Result<TerminalGuard> {
    setup_terminal_with_capabilities(true, mouse_capture)
}

/// Sets up a direct attach terminal.
///
/// Direct attach forwards stdin to the attached PTY, so it must not enable
/// outer-terminal protocols that generate local responses or mouse reports.
fn setup_direct_attach_terminal() -> io::Result<TerminalGuard> {
    setup_terminal_with_capabilities(false, false)
}

fn setup_terminal_with_capabilities(
    enable_client_protocols: bool,
    mouse_capture: bool,
) -> io::Result<TerminalGuard> {
    ratatui::init();
    crate::terminal_modes::clear_host_mouse_reporting(&mut io::stdout())?;

    #[cfg(windows)]
    let windows_virtual_terminal_input =
        if enable_client_protocols && windows_vti_input_backend_enabled() {
            enable_windows_virtual_terminal_input()
        } else {
            WindowsVirtualTerminalInputSetup::default()
        };

    if enable_client_protocols {
        if mouse_capture {
            execute!(io::stdout(), EnableMouseCapture)?;
        } else {
            execute!(io::stdout(), DisableMouseCapture)?;
        }
        execute!(
            io::stdout(),
            EnableBracketedPaste,
            EnableFocusChange,
            PushKeyboardEnhancementFlags(crate::input::ime_compatible_keyboard_enhancement_flags())
        )?;
    } else {
        execute!(io::stdout(), DisableMouseCapture)?;
    }

    #[cfg(windows)]
    if enable_client_protocols
        && windows_vti_input_backend_enabled()
        && windows_virtual_terminal_input.active
        && windows_win32_input_mode_enabled()
    {
        if let Err(err) = enable_windows_win32_input_mode(&mut io::stdout()) {
            if let Some(mode) = windows_virtual_terminal_input.restore_mode {
                restore_windows_input_mode_value(mode);
            }
            return Err(err);
        }
    }

    let modify_other_keys_mode = enable_client_protocols
        .then(crate::input::host_modify_other_keys_mode)
        .flatten();
    if let Some(mode) = modify_other_keys_mode {
        io::stdout().write_all(mode.set_sequence())?;
        io::stdout().flush()?;
    }

    Ok(TerminalGuard {
        reset_modify_other_keys: modify_other_keys_mode.is_some(),
        #[cfg(windows)]
        restore_windows_input_mode: windows_virtual_terminal_input.restore_mode,
    })
}

/// Guard that restores the terminal when dropped.
struct TerminalGuard {
    reset_modify_other_keys: bool,
    #[cfg(windows)]
    restore_windows_input_mode: Option<u32>,
}

fn write_terminal_restore_postlude(writer: &mut impl io::Write) -> io::Result<()> {
    writer.write_all(b"\x1b[?25h\x1b[0 q")?;
    writer.flush()
}

fn set_mouse_capture(enabled: bool) -> io::Result<()> {
    crate::terminal_modes::clear_host_mouse_reporting(&mut io::stdout())?;
    if enabled {
        execute!(io::stdout(), EnableMouseCapture)
    } else {
        execute!(io::stdout(), DisableMouseCapture)
    }
}

fn restore_terminal_state(
    reset_modify_other_keys: bool,
    #[cfg(windows)] restore_windows_input_mode: Option<u32>,
) {
    let _ = clear_received_kitty_graphics(&mut io::stdout());

    if reset_modify_other_keys {
        let _ = io::stdout().write_all(b"\x1b[>4;0m");
        let _ = io::stdout().flush();
    }

    let _ = execute!(
        io::stdout(),
        PopKeyboardEnhancementFlags,
        DisableFocusChange,
        DisableBracketedPaste,
        DisableMouseCapture
    );
    let _ = crate::terminal_modes::clear_host_mouse_reporting(&mut io::stdout());
    #[cfg(windows)]
    if let Some(mode) = restore_windows_input_mode {
        restore_windows_input_mode_value(mode);
    }
    ratatui::restore();
    let _ = write_terminal_restore_postlude(&mut io::stdout());

    #[cfg(windows)]
    if windows_vti_input_backend_enabled() && windows_win32_input_mode_enabled() {
        let _ = disable_windows_win32_input_mode(&mut io::stdout());
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal_state(
            self.reset_modify_other_keys,
            #[cfg(windows)]
            self.restore_windows_input_mode,
        );
    }
}
#[cfg(windows)]
#[derive(Default)]
struct WindowsVirtualTerminalInputSetup {
    active: bool,
    restore_mode: Option<u32>,
}

#[cfg(windows)]
fn enable_windows_virtual_terminal_input() -> WindowsVirtualTerminalInputSetup {
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_VIRTUAL_TERMINAL_INPUT,
        STD_INPUT_HANDLE,
    };

    let handle: HANDLE = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        tracing::warn!("failed to get Windows console input handle for VT input");
        return WindowsVirtualTerminalInputSetup::default();
    }

    let mut mode = 0;
    if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
        tracing::warn!("failed to read Windows console input mode for VT input");
        return WindowsVirtualTerminalInputSetup::default();
    }

    let desired = mode | ENABLE_VIRTUAL_TERMINAL_INPUT;
    if desired == mode {
        return WindowsVirtualTerminalInputSetup {
            active: true,
            restore_mode: None,
        };
    }
    let desired = windows_virtual_terminal_input_mode(mode);
    if unsafe { SetConsoleMode(handle, desired) } == 0 {
        tracing::warn!("failed to enable Windows virtual terminal input");
        return WindowsVirtualTerminalInputSetup::default();
    }
    let mut applied = 0;
    if unsafe { GetConsoleMode(handle, &mut applied) } == 0
        || applied & ENABLE_VIRTUAL_TERMINAL_INPUT == 0
    {
        tracing::warn!("Windows virtual terminal input bit did not stick");
        let _ = unsafe { SetConsoleMode(handle, mode) };
        return WindowsVirtualTerminalInputSetup::default();
    }

    WindowsVirtualTerminalInputSetup {
        active: true,
        restore_mode: Some(mode),
    }
}

#[cfg(windows)]
fn windows_virtual_terminal_input_mode(mode: u32) -> u32 {
    mode | 0x0200
}

#[cfg(windows)]
fn restore_windows_input_mode_value(mode: u32) {
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Console::{GetStdHandle, SetConsoleMode, STD_INPUT_HANDLE};

    let handle: HANDLE = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return;
    }
    let _ = unsafe { SetConsoleMode(handle, mode) };
}

#[cfg(windows)]
fn windows_win32_input_mode_enabled() -> bool {
    std::env::var("HAKO_WINDOWS_INPUT_PROBE")
        .map(|probe| probe.eq_ignore_ascii_case("win32"))
        .unwrap_or(true)
}

#[cfg(windows)]
fn enable_windows_win32_input_mode(writer: &mut impl io::Write) -> io::Result<()> {
    writer.write_all(b"\x1b[?9001h")?;
    writer.flush()
}

#[cfg(windows)]
fn disable_windows_win32_input_mode(writer: &mut impl io::Write) -> io::Result<()> {
    writer.write_all(b"\x1b[?9001l")?;
    writer.flush()
}

// ---------------------------------------------------------------------------
// Handshake
// ---------------------------------------------------------------------------

fn requested_render_encoding() -> RenderEncoding {
    match std::env::var("HAKO_RENDER_ENCODING").ok().as_deref() {
        Some("terminal-ansi" | "terminal_ansi" | "ansi") => RenderEncoding::TerminalAnsi,
        _ => RenderEncoding::SemanticFrame,
    }
}

const LOCAL_HANDSHAKE_READ_TIMEOUT: Duration = Duration::from_secs(5);
const REMOTE_HANDSHAKE_READ_TIMEOUT: Duration = Duration::from_secs(60);

fn handshake_read_timeout(is_remote: bool) -> Duration {
    if is_remote {
        REMOTE_HANDSHAKE_READ_TIMEOUT
    } else {
        LOCAL_HANDSHAKE_READ_TIMEOUT
    }
}

fn requested_keybindings() -> ClientKeybindings {
    match std::env::var(crate::remote::REMOTE_KEYBINDINGS_ENV_VAR)
        .ok()
        .as_deref()
    {
        Some("local") => crate::config::Config::load()
            .config
            .local_keybindings_profile_toml()
            .map(|keys_toml| ClientKeybindings::Local { keys_toml })
            .unwrap_or(ClientKeybindings::Server),
        _ => ClientKeybindings::Server,
    }
}

fn should_draw_host_cursor(mode: crate::config::HostCursorModeConfig) -> bool {
    match mode {
        crate::config::HostCursorModeConfig::Auto => {
            crate::platform::should_draw_host_cursor_by_default()
        }
        crate::config::HostCursorModeConfig::Native => false,
        crate::config::HostCursorModeConfig::Drawn => true,
    }
}

/// Performs the client→server handshake.
///
/// Sends Hello with the terminal size and protocol version, reads the Welcome
/// response. Returns Ok(()) on success, or an error if the server rejects us.
fn do_handshake(
    stream: &mut LocalStream,
    cols: u16,
    rows: u16,
    cell_width_px: u32,
    cell_height_px: u32,
    requested_encoding: RenderEncoding,
    direct_attach_requested: bool,
) -> Result<RenderEncoding, ClientError> {
    stream
        .set_nonblocking(false)
        .map_err(ClientError::ConnectionFailed)?;

    // Send Hello.
    let hello = ClientMessage::Hello {
        version: PROTOCOL_VERSION,
        cols,
        rows,
        cell_width_px,
        cell_height_px,
        requested_encoding,
        keybindings: requested_keybindings(),
        launch_mode: if direct_attach_requested {
            ClientLaunchMode::TerminalAttach
        } else {
            ClientLaunchMode::App
        },
    };
    protocol::write_message(stream, &hello)
        .map_err(|e| ClientError::ConnectionFailed(io::Error::other(e.to_string())))?;

    // Read Welcome.
    stream
        .set_recv_timeout(Some(handshake_read_timeout(
            std::env::var(crate::remote::REMOTE_KEYBINDINGS_ENV_VAR).is_ok(),
        )))
        .map_err(ClientError::ConnectionFailed)?;
    let welcome: ServerMessage = protocol::read_message(stream, MAX_FRAME_SIZE)?;
    stream
        .set_recv_timeout(None)
        .map_err(ClientError::ConnectionFailed)?;

    match welcome {
        ServerMessage::Welcome {
            version,
            encoding,
            error,
        } => {
            if let Some(error) = error {
                return Err(ClientError::HandshakeRejected { version, error });
            }
            info!(version, ?encoding, "handshake succeeded");
            Ok(encoding)
        }
        _ => Err(ClientError::Protocol(protocol::FramingError::Io(
            io::Error::new(io::ErrorKind::InvalidData, "expected Welcome message"),
        ))),
    }
}

// ---------------------------------------------------------------------------
// Client event loop
// ---------------------------------------------------------------------------

/// Internal events for the client event loop.
enum ClientLoopEvent {
    /// Raw input bytes from stdin.
    StdinInput(Vec<u8>),
    #[cfg(windows)]
    /// Semantic input events from a Windows console reader.
    StdinEvents(Vec<crate::protocol::ClientInputEvent>),
    /// Terminal resize detected.
    Resize(u16, u16, u32, u32),
    /// Server message received.
    ServerMessage(ServerMessage),
    /// Server reader thread exited (connection lost).
    ServerDisconnected,
    /// Timer tick.
    Timer,
}

/// Runs the thin client: connects to the server, performs the handshake,
/// and enters the main event loop.
///
/// This is the entry point called from `main.rs` when running in client mode.
pub fn run_client() -> io::Result<()> {
    run_client_with_mode(
        requested_render_encoding(),
        None,
        None,
        "connecting to server",
    )
}

/// Runs a direct terminal attach client.
pub fn run_terminal_attach(terminal_id: String, takeover: bool) -> io::Result<()> {
    run_client_with_mode(
        RenderEncoding::TerminalAnsi,
        Some((terminal_id, takeover)),
        Some(AttachEscapeState::default()),
        "attaching to terminal",
    )
}

fn run_client_with_mode(
    requested_encoding: RenderEncoding,
    attach_request: Option<(String, bool)>,
    attach_escape: Option<AttachEscapeState>,
    log_message: &'static str,
) -> io::Result<()> {
    init_logging();

    let loaded_config = crate::config::Config::load();
    let host_cursor = loaded_config.config.ui.host_cursor;
    crate::terminal_modes::clear_host_mouse_reporting(&mut io::stdout())?;
    let redraw_on_focus_gained = loaded_config.config.ui.redraw_on_focus_gained;
    let remote_image_paste_key = client_remote_image_paste_key(&loaded_config.config);
    let sound_config = loaded_config.config.ui.sound;
    let direct_attach_requested = attach_request.is_some();
    let kitty_graphics_enabled =
        loaded_config.config.experimental.kitty_graphics && !direct_attach_requested;

    let socket_path = client_socket_path();
    crate::logging::startup("client");
    info!(path = %socket_path.display(), "{log_message}");

    // Try to connect to the server.
    let mut stream = match crate::ipc::connect_local_stream(&socket_path) {
        Ok(s) => s,
        Err(err) => {
            // Server unreachable — show clear error and exit.
            let client_err = ClientError::ConnectionFailed(err);
            eprintln!("hako: {client_err}");
            std::process::exit(1);
        }
    };

    // Get the terminal geometry before handshake (before raw mode).
    let (cols, rows, cell_width_px, cell_height_px) =
        current_terminal_geometry(kitty_graphics_enabled);

    // Perform handshake while the stream is still in blocking mode.
    let negotiated_encoding = match do_handshake(
        &mut stream,
        cols,
        rows,
        cell_width_px,
        cell_height_px,
        requested_encoding,
        direct_attach_requested,
    ) {
        Ok(encoding) => encoding,
        Err(err) => {
            eprintln!("hako: {err}");
            std::process::exit(1);
        }
    };

    if let Some((terminal_id, takeover)) = attach_request {
        let attach = ClientMessage::AttachTerminal {
            terminal_id,
            takeover,
        };
        if let Err(err) = write_to_server(&mut stream, &attach) {
            eprintln!("hako: failed to request terminal attach: {err}");
            std::process::exit(1);
        }
    }

    // Now set up the terminal. This must happen AFTER the handshake succeeds,
    // so we don't leave the terminal in raw mode if the server rejects us.
    let direct_attach = attach_escape.is_some();
    let _guard = if direct_attach {
        setup_direct_attach_terminal()
    } else {
        setup_terminal(false)
    }
    .map_err(|err| {
        eprintln!("hako: failed to set up terminal: {err}");
        err
    })?;

    // Install a panic hook to restore the terminal on panic (same as monolithic).
    let panic_reset_modify_other_keys = _guard.reset_modify_other_keys;
    #[cfg(windows)]
    let panic_restore_windows_input_mode = _guard.restore_windows_input_mode;
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal_state(
            panic_reset_modify_other_keys,
            #[cfg(windows)]
            panic_restore_windows_input_mode,
        );
        original_hook(info);
    }));

    // Create the tokio runtime.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(io::Error::other)?;

    let should_quit = Arc::new(AtomicBool::new(false));

    // Install Ctrl+C handler.
    let quit_flag = should_quit.clone();
    let _ = ctrlc::set_handler(move || {
        quit_flag.store(true, Ordering::Release);
    });

    let result = rt.block_on(async {
        run_client_loop(
            stream,
            (cols, rows),
            should_quit,
            sound_config,
            host_cursor,
            redraw_on_focus_gained,
            kitty_graphics_enabled,
            false,
            remote_image_paste_key,
            negotiated_encoding,
            attach_escape,
        )
        .await
    });

    // Restore the terminal before printing any final status message.
    drop(_guard);

    if let Err(err) = result {
        eprintln!("hako: {err}");
        rt.shutdown_timeout(Duration::from_millis(100));
        crate::logging::shutdown("client");

        if matches!(
            err,
            ClientError::ServerShutdown {
                reason: Some(reason)
            } if reason == "detached"
        ) {
            return Ok(());
        }

        std::process::exit(1);
    }

    rt.shutdown_timeout(Duration::from_millis(100));
    crate::logging::shutdown("client");
    Ok(())
}

/// The main client event loop.
///
/// Uses a threaded architecture:
/// - stdin reader thread → sends raw input bytes to main loop
/// - resize poller thread → sends resize events to main loop
/// - server reader thread → reads ServerMessages and sends to main loop
/// - main loop: coordinates input, output, and server communication
async fn run_client_loop(
    stream: LocalStream,
    size: (u16, u16),
    should_quit: Arc<AtomicBool>,
    sound_config: crate::config::SoundConfig,
    host_cursor: crate::config::HostCursorModeConfig,
    redraw_on_focus_gained: bool,
    kitty_graphics_enabled: bool,
    mouse_capture_active: bool,
    remote_image_paste_key: Option<(crossterm::event::KeyCode, crossterm::event::KeyModifiers)>,
    negotiated_encoding: RenderEncoding,
    attach_escape: Option<AttachEscapeState>,
) -> Result<(), ClientError> {
    let mut state = ClientState {
        blit_encoder: render_ansi::BlitEncoder::new(),
        mouse_capture_active,
        reported_size: size,
        sound_config,
        kitty_graphics_enabled,
        draw_host_cursor: attach_escape.is_none() && should_draw_host_cursor(host_cursor),
        attach_escape,
        redraw_on_focus_gained,
        remote_image_paste_key,
    };
    debug!(?negotiated_encoding, "client render encoding active");

    // Channel for events from the stdin, resize, and server reader threads.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<ClientLoopEvent>(256);
    let host_mouse_capture_active = Arc::new(AtomicBool::new(state.mouse_capture_active));

    // Spawn the stdin reader thread.
    let will_query_host_terminal_theme =
        state.attach_escape.is_none() && should_query_host_terminal_theme();
    let stdin_quit = should_quit.clone();
    let stdin_tx = event_tx.clone();
    let stdin_mouse_capture_active = host_mouse_capture_active.clone();
    std::thread::spawn(move || {
        input::stdin_reader_loop(
            stdin_tx,
            &stdin_quit,
            will_query_host_terminal_theme,
            stdin_mouse_capture_active,
        );
    });

    if will_query_host_terminal_theme {
        query_host_terminal_theme();
    }

    // Spawn the resize poller thread.
    let resize_quit = should_quit.clone();
    let resize_tx = event_tx.clone();
    std::thread::spawn(move || {
        resize_poll_loop(
            resize_tx,
            size.0,
            size.1,
            kitty_graphics_enabled,
            &resize_quit,
        );
    });

    // Spawn the server reader thread (blocking reads from the socket).
    // Clone the stream's file descriptor so we can read from a blocking stream.
    let server_read_quit = should_quit.clone();
    let server_read_tx = event_tx.clone();
    let read_stream = stream.try_clone().map_err(ClientError::ConnectionFailed)?;
    std::thread::spawn(move || {
        let max_frame_size = if kitty_graphics_enabled {
            MAX_GRAPHICS_FRAME_SIZE
        } else {
            MAX_FRAME_SIZE
        };
        server_reader_thread(
            read_stream,
            server_read_tx,
            &server_read_quit,
            max_frame_size,
        );
    });

    // Use the original stream for writing (blocking is fine since we write
    // from the async loop).
    let mut write_stream = stream;
    write_stream
        .set_nonblocking(false)
        .map_err(ClientError::ConnectionFailed)?;

    use crate::platform::PrefixInputSource;
    let mut prefix_input_source = crate::platform::RealPrefixInputSource::default();

    // Main event loop.
    while !should_quit.load(Ordering::Acquire) {
        let event = tokio::select! {
            ev = event_rx.recv() => ev.unwrap_or(ClientLoopEvent::Timer),
            _ = tokio::time::sleep(Duration::from_millis(100)) => ClientLoopEvent::Timer,
        };

        match event {
            ClientLoopEvent::StdinInput(data) => {
                let data = if let Some(attach_escape) = &mut state.attach_escape {
                    match attach_escape.filter_input(data) {
                        AttachInputAction::Forward(data) => data,
                        AttachInputAction::Detach => {
                            let _ = write_to_server(&mut write_stream, &ClientMessage::Detach);
                            return Ok(());
                        }
                        AttachInputAction::None => continue,
                    }
                } else {
                    let events = crate::raw_input::parse_raw_input_bytes_sync(&data);
                    if crate::raw_input::events_require_host_surface_redraw(
                        &events,
                        state.redraw_on_focus_gained,
                    ) {
                        state.request_full_redraw();
                    }
                    data
                };
                if should_bridge_clipboard_image_paste(
                    &data,
                    is_remote_client_process(),
                    state.remote_image_paste_key,
                ) {
                    if let Some(image) = crate::platform::read_clipboard_image() {
                        if image.bytes.len() > MAX_CLIPBOARD_IMAGE_PAYLOAD {
                            warn!(
                                bytes = image.bytes.len(),
                                max = MAX_CLIPBOARD_IMAGE_PAYLOAD,
                                "local clipboard image is too large to bridge"
                            );
                            continue;
                        }
                        info!(
                            bytes = image.bytes.len(),
                            extension = image.extension,
                            "bridging local clipboard image paste to remote server"
                        );
                        let msg = ClientMessage::ClipboardImage {
                            extension: image.extension.to_owned(),
                            data: image.bytes,
                        };
                        if let Err(e) = write_to_server(&mut write_stream, &msg) {
                            return Err(ClientError::ConnectionLost(e));
                        }
                        continue;
                    }
                    info!(
                        "clipboard image paste trigger received, but local clipboard has no image"
                    );
                }
                if let Some(image) =
                    read_image_file_from_terminal_drop(&data, is_remote_client_process())
                {
                    info!(
                        bytes = image.bytes.len(),
                        extension = image.extension,
                        "bridging local image file drop to remote server"
                    );
                    let msg = ClientMessage::ClipboardImage {
                        extension: image.extension.to_owned(),
                        data: image.bytes,
                    };
                    if let Err(e) = write_to_server(&mut write_stream, &msg) {
                        return Err(ClientError::ConnectionLost(e));
                    }
                    continue;
                }
                let msg = ClientMessage::Input { data };
                if let Err(e) = write_to_server(&mut write_stream, &msg) {
                    return Err(ClientError::ConnectionLost(e));
                }
            }
            #[cfg(windows)]
            ClientLoopEvent::StdinEvents(events) => {
                if state.attach_escape.is_some() {
                    let data = semantic_events_to_attach_bytes(&events);
                    let data = match state
                        .attach_escape
                        .as_mut()
                        .expect("attach escape state checked above")
                        .filter_input(data)
                    {
                        AttachInputAction::Forward(data) => data,
                        AttachInputAction::Detach => {
                            let _ = write_to_server(&mut write_stream, &ClientMessage::Detach);
                            return Ok(());
                        }
                        AttachInputAction::None => continue,
                    };
                    let msg = ClientMessage::Input { data };
                    if let Err(e) = write_to_server(&mut write_stream, &msg) {
                        return Err(ClientError::ConnectionLost(e));
                    }
                    continue;
                }
                let raw_events = events
                    .iter()
                    .map(crate::protocol::ClientInputEvent::to_raw_input_event)
                    .collect::<Vec<_>>();
                if crate::raw_input::events_require_host_surface_redraw(
                    &raw_events,
                    state.redraw_on_focus_gained,
                ) {
                    state.request_full_redraw();
                }
                let msg = ClientMessage::InputEvents { events };
                if let Err(e) = write_to_server(&mut write_stream, &msg) {
                    return Err(ClientError::ConnectionLost(e));
                }
            }
            ClientLoopEvent::Resize(new_cols, new_rows, cell_width_px, cell_height_px) => {
                state.reported_size = (new_cols, new_rows);
                let msg = ClientMessage::Resize {
                    cols: new_cols,
                    rows: new_rows,
                    cell_width_px,
                    cell_height_px,
                };
                if let Err(e) = write_to_server(&mut write_stream, &msg) {
                    return Err(ClientError::ConnectionLost(e));
                }
            }
            ClientLoopEvent::ServerMessage(msg) => match msg {
                ServerMessage::Frame(frame_data) => {
                    let frame_data = if state.draw_host_cursor {
                        render_ansi::frame_with_drawn_cursor(frame_data)
                    } else {
                        frame_data
                    };
                    let encoded = if state.draw_host_cursor {
                        state
                            .blit_encoder
                            .encode_with_suppressed_visible_cursor(&frame_data, false)
                    } else {
                        state.blit_encoder.encode(&frame_data, false)
                    };
                    let mut stdout = io::stdout();
                    let graphics = if state.kitty_graphics_enabled {
                        frame_data.graphics.as_slice()
                    } else {
                        &[]
                    };
                    let _ =
                        write_encoded_frame_with_graphics(&mut stdout, &encoded.bytes, graphics);
                    let _ = stdout.flush();
                    state.blit_encoder.commit(frame_data, encoded);
                }
                ServerMessage::Terminal(frame) => {
                    if state.kitty_graphics_enabled && contains_kitty_graphics_bytes(&frame.bytes) {
                        record_received_kitty_graphics(&frame.bytes);
                    }
                    let mut stdout = io::stdout();
                    let _ = stdout.write_all(&frame.bytes);
                    let _ = stdout.flush();
                }
                ServerMessage::Graphics { bytes } => {
                    if state.kitty_graphics_enabled {
                        record_received_kitty_graphics(&bytes);
                        let mut stdout = io::stdout();
                        let _ = stdout.write_all(&bytes);
                        let _ = stdout.flush();
                    }
                }
                ServerMessage::ServerShutdown { reason } => {
                    return Err(ClientError::ServerShutdown { reason });
                }
                ServerMessage::Notify { kind, message } => {
                    handle_notify(kind, &message, &state.sound_config);
                }
                ServerMessage::Clipboard { data } => {
                    forward_clipboard(&data);
                    let _ = io::stdout().flush();
                }
                ServerMessage::WindowTitle { title } => {
                    write_window_title(title.as_deref());
                    let _ = io::stdout().flush();
                }
                ServerMessage::ReloadSoundConfig => {
                    reload_local_client_config(
                        &mut state.sound_config,
                        &mut state.redraw_on_focus_gained,
                        &mut state.draw_host_cursor,
                        &mut state.remote_image_paste_key,
                    );
                }
                ServerMessage::MouseCapture { enabled } => {
                    let desired = enabled;
                    if desired != state.mouse_capture_active {
                        set_mouse_capture(desired).map_err(ClientError::ConnectionFailed)?;
                        state.mouse_capture_active = desired;
                        host_mouse_capture_active.store(desired, Ordering::Release);
                    }
                }
                ServerMessage::PrefixInputSource { active } => {
                    if active {
                        prefix_input_source.switch_to_ascii();
                    } else {
                        prefix_input_source.restore();
                    }
                }
                ServerMessage::Welcome { .. } => {
                    debug!("received unexpected Welcome in main loop");
                }
            },
            ClientLoopEvent::ServerDisconnected => {
                return Err(ClientError::ConnectionLost(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "server closed connection",
                )));
            }
            ClientLoopEvent::Timer => {
                // Check if we should quit.
            }
        }
    }

    // Clean exit (Ctrl+C). Send Detach before closing.
    let detach = ClientMessage::Detach;
    let _ = write_to_server(&mut write_stream, &detach);
    let _ = io::stdout().flush();

    Ok(())
}

// ---------------------------------------------------------------------------
// Server reader thread
// ---------------------------------------------------------------------------

/// Blocking thread that reads ServerMessages from the server and sends them
/// to the main event loop.
fn server_reader_thread(
    mut stream: LocalStream,
    event_tx: tokio::sync::mpsc::Sender<ClientLoopEvent>,
    should_quit: &Arc<AtomicBool>,
    max_frame_size: usize,
) {
    // Ensure the read stream is in blocking mode to avoid WouldBlock errors
    // from read_exact inside read_message. The stream should already be
    // blocking after handshake, but we enforce it here as a safety measure.
    if stream.set_nonblocking(false).is_err() {
        // If we can't set blocking mode, the stream is likely broken.
        let _ = event_tx.blocking_send(ClientLoopEvent::ServerDisconnected);
        return;
    }

    loop {
        if should_quit.load(Ordering::Acquire) {
            break;
        }

        match protocol::read_message(&mut stream, max_frame_size) {
            Ok(msg) => {
                if event_tx
                    .blocking_send(ClientLoopEvent::ServerMessage(msg))
                    .is_err()
                {
                    break; // Main loop gone.
                }
            }
            Err(protocol::FramingError::UnexpectedEof) => {
                // Server closed connection.
                let _ = event_tx.blocking_send(ClientLoopEvent::ServerDisconnected);
                break;
            }
            Err(protocol::FramingError::Io(err)) if err.kind() == io::ErrorKind::WouldBlock => {
                // Should not happen with blocking mode, but handle gracefully
                // in case the stream was set nonblocking by another clone.
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            Err(err) => {
                warn!(err = %err, "server read error");
                let _ = event_tx.blocking_send(ClientLoopEvent::ServerDisconnected);
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Write helper
// ---------------------------------------------------------------------------

/// Writes a message to the server stream (blocking).
fn write_to_server(stream: &mut LocalStream, msg: &ClientMessage) -> io::Result<()> {
    protocol::write_message(stream, msg).map_err(|e| io::Error::other(e.to_string()))
}

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

fn reload_local_client_config(
    sound_config: &mut crate::config::SoundConfig,
    draw_host_cursor: &mut bool,
    redraw_on_focus_gained: &mut bool,
    remote_image_paste_key: &mut Option<(
        crossterm::event::KeyCode,
        crossterm::event::KeyModifiers,
    )>,
) {
    match crate::config::load_live_config() {
        Ok(loaded) => {
            for diagnostic in loaded.config.ui.sound.diagnostics() {
                warn!(diagnostic = %diagnostic, "local sound config diagnostic");
            }
            let loaded_remote_image_paste_key = client_remote_image_paste_key(&loaded.config);
            *sound_config = loaded.config.ui.sound;
            *redraw_on_focus_gained = loaded.config.ui.redraw_on_focus_gained;
            *draw_host_cursor = should_draw_host_cursor(loaded.config.ui.host_cursor);
            *remote_image_paste_key = loaded_remote_image_paste_key;
            debug!("reloaded local client config");
        }
        Err(diagnostics) => {
            warn!(diagnostics = ?diagnostics, "failed to reload local client config; keeping current client config");
        }
    }
}

fn handle_notify(kind: NotifyKind, message: &str, sound_config: &crate::config::SoundConfig) {
    handle_notify_with_notifiers(
        kind,
        message,
        sound_config,
        crate::terminal_notify::show_notification,
        crate::platform::show_desktop_notification,
    );
}

fn handle_notify_with_notifiers(
    kind: NotifyKind,
    message: &str,
    sound_config: &crate::config::SoundConfig,
    mut show_terminal_notification: impl FnMut(&str, Option<&str>) -> io::Result<bool>,
    mut show_system_notification: impl FnMut(&str, Option<&str>) -> io::Result<bool>,
) {
    match kind {
        NotifyKind::Sound => {
            let Some(sound) = sound_from_notify_message(message) else {
                warn!(
                    message = message,
                    "received unknown sound notification from server"
                );
                return;
            };
            if sound_config.enabled {
                crate::sound::play(sound, sound_config);
            }
        }
        NotifyKind::Toast => {
            debug!(
                message = message,
                "received terminal toast notification from server"
            );
            let (title, body) = crate::terminal_notify::split_message(message);
            if let Err(err) = show_terminal_notification(title, body) {
                warn!(err = %err, "failed to emit terminal notification");
            }
        }
        NotifyKind::SystemToast => {
            debug!(
                message = message,
                "received system toast notification from server"
            );
            let (title, body) = crate::terminal_notify::split_message(message);
            if let Err(err) = show_system_notification(title, body) {
                warn!(err = %err, "failed to emit system notification");
            }
        }
    }
}

fn sound_from_notify_message(message: &str) -> Option<crate::sound::Sound> {
    match message {
        "agent done" => Some(crate::sound::Sound::Done),
        "agent attention" => Some(crate::sound::Sound::Request),
        _ => None,
    }
}

fn is_remote_client_process() -> bool {
    cfg!(unix) && std::env::var(crate::remote::REMOTE_KEYBINDINGS_ENV_VAR).is_ok()
}

fn client_remote_image_paste_key(
    config: &crate::config::Config,
) -> Option<(crossterm::event::KeyCode, crossterm::event::KeyModifiers)> {
    if !is_remote_client_process() {
        return None;
    }

    match config.remote_image_paste_key() {
        Ok(key) => key,
        Err(diagnostic) => {
            warn!(diagnostic = %diagnostic, "local remote image paste key config diagnostic");
            None
        }
    }
}

fn should_bridge_clipboard_image_paste(
    data: &[u8],
    is_remote_client: bool,
    remote_image_paste_key: Option<(crossterm::event::KeyCode, crossterm::event::KeyModifiers)>,
) -> bool {
    if data == b"\x1b[200~\x1b[201~" {
        return is_remote_client;
    }

    let Some(remote_image_paste_key) = remote_image_paste_key else {
        return false;
    };
    let events = crate::raw_input::parse_raw_input_bytes_sync(data);
    matches!(
        events.as_slice(),
        [crate::raw_input::RawInputEvent::Key(key)]
            if key.kind == crossterm::event::KeyEventKind::Press
                && crate::config::terminal_key_matches_combo(*key, remote_image_paste_key)
    )
}

#[cfg(unix)]
fn read_image_file_from_terminal_drop(
    data: &[u8],
    is_remote_client: bool,
) -> Option<crate::platform::ClipboardImage> {
    let (path, extension) = image_path_from_terminal_drop(data, is_remote_client)?;
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }

    let file = std::fs::File::open(&path).ok()?;
    let bytes =
        match crate::platform::read_limited_reader(file, MAX_CLIPBOARD_IMAGE_PAYLOAD).ok()? {
            crate::platform::LimitedRead::Complete(bytes) => bytes,
            crate::platform::LimitedRead::Empty => return None,
            crate::platform::LimitedRead::Oversized => {
                warn!(
                    max = MAX_CLIPBOARD_IMAGE_PAYLOAD,
                    "local image file drop is too large to bridge"
                );
                return None;
            }
        };

    Some(crate::platform::ClipboardImage { bytes, extension })
}

#[cfg(not(unix))]
fn read_image_file_from_terminal_drop(
    _data: &[u8],
    _is_remote_client: bool,
) -> Option<crate::platform::ClipboardImage> {
    None
}

#[cfg(unix)]
fn image_path_from_terminal_drop(
    data: &[u8],
    is_remote_client: bool,
) -> Option<(std::path::PathBuf, &'static str)> {
    if !is_remote_client {
        return None;
    }

    let bytes = bracketed_paste_payload(data).unwrap_or(data);
    let text = std::str::from_utf8(bytes).ok()?;
    let text = text.trim_end_matches(['\r', '\n']);
    if text.is_empty() || text.contains(['\r', '\n']) {
        return None;
    }

    let text = unescape_terminal_drop_path(strip_matching_path_quotes(text));
    let path = std::path::PathBuf::from(text);
    if !path.is_absolute() {
        return None;
    }

    let extension = recognized_image_extension(path.extension()?.to_str()?)?;
    Some((path, extension))
}

#[cfg(unix)]
fn bracketed_paste_payload(data: &[u8]) -> Option<&[u8]> {
    const START: &[u8] = b"\x1b[200~";
    const END: &[u8] = b"\x1b[201~";
    data.strip_prefix(START)?.strip_suffix(END)
}

#[cfg(unix)]
fn strip_matching_path_quotes(text: &str) -> &str {
    if text.len() < 2 {
        return text;
    }

    let bytes = text.as_bytes();
    match (bytes.first(), bytes.last()) {
        (Some(b'\''), Some(b'\'')) | (Some(b'"'), Some(b'"')) => &text[1..text.len() - 1],
        _ => text,
    }
}

#[cfg(unix)]
fn unescape_terminal_drop_path(text: &str) -> String {
    let mut unescaped = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(escaped) = chars.next() {
                unescaped.push(escaped);
            } else {
                unescaped.push(ch);
            }
        } else {
            unescaped.push(ch);
        }
    }
    unescaped
}

#[cfg(unix)]
fn recognized_image_extension(extension: &str) -> Option<&'static str> {
    if extension.eq_ignore_ascii_case("png") {
        Some("png")
    } else if extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg") {
        Some("jpg")
    } else if extension.eq_ignore_ascii_case("gif") {
        Some("gif")
    } else if extension.eq_ignore_ascii_case("webp") {
        Some("webp")
    } else if extension.eq_ignore_ascii_case("bmp") {
        Some("bmp")
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Clipboard forwarding
// ---------------------------------------------------------------------------

/// Decode a clipboard payload forwarded by the server.
fn decode_clipboard_payload(data: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(data).ok()
}

/// Forwards a clipboard write from the server to the local client clipboard.
fn forward_clipboard(data: &str) {
    let Some(bytes) = decode_clipboard_payload(data) else {
        warn!("received invalid clipboard payload from server");
        return;
    };

    crate::selection::write_osc52_bytes(&bytes);
}

fn window_title_osc(title: Option<&str>) -> Vec<u8> {
    let title = title.unwrap_or("hako");
    let safe_title = title
        .chars()
        .filter(|ch| !matches!(*ch, '\u{1b}' | '\u{7}' | '\u{9c}'))
        .collect::<String>();
    format!("\x1b]0;{safe_title}\x07").into_bytes()
}

fn write_window_title(title: Option<&str>) {
    let _ = io::stdout().write_all(&window_title_osc(title));
}

// ---------------------------------------------------------------------------
// Frame output
// ---------------------------------------------------------------------------

fn write_encoded_frame_with_graphics(
    mut writer: impl io::Write,
    encoded: &[u8],
    graphics: &[u8],
) -> io::Result<()> {
    writer.write_all(encoded)?;
    if graphics.is_empty() {
        return Ok(());
    }

    record_received_kitty_graphics(graphics);
    writer.write_all(b"\x1b7")?;
    writer.write_all(graphics)?;
    writer.write_all(b"\x1b8")
}

fn contains_kitty_graphics_bytes(bytes: &[u8]) -> bool {
    bytes.windows(3).any(|window| window == b"\x1b_G")
}

fn record_received_kitty_graphics(bytes: &[u8]) {
    let ids = kitty_graphics_image_ids(bytes);
    if ids.is_empty() {
        return;
    }
    let set = RECEIVED_KITTY_GRAPHICS_IDS.get_or_init(|| Mutex::new(HashSet::new()));
    if let Ok(mut set) = set.lock() {
        set.extend(ids);
    }
}

fn clear_received_kitty_graphics(mut writer: impl io::Write) -> io::Result<()> {
    let Some(set) = RECEIVED_KITTY_GRAPHICS_IDS.get() else {
        return Ok(());
    };
    let Ok(mut set) = set.lock() else {
        return Ok(());
    };
    for id in set.drain() {
        write!(writer, "\x1b_Ga=d,d=I,i={id},q=2;\x1b\\")?;
    }
    writer.flush()
}

fn kitty_graphics_image_ids(bytes: &[u8]) -> Vec<u32> {
    let mut ids = Vec::new();
    let mut index = 0usize;
    while let Some(start) = find_subslice(&bytes[index..], b"\x1b_G") {
        let command_start = index + start + 3;
        let Some(end) = find_subslice(&bytes[command_start..], b"\x1b\\") else {
            break;
        };
        let command = &bytes[command_start..command_start + end];
        if let Some(id) = kitty_graphics_command_image_id(command) {
            ids.push(id);
        }
        index = command_start + end + 2;
    }
    ids
}

fn kitty_graphics_command_image_id(command: &[u8]) -> Option<u32> {
    let header_end = command
        .iter()
        .position(|byte| *byte == b';')
        .unwrap_or(command.len());
    for part in command[..header_end].split(|byte| *byte == b',') {
        let Some(value) = part.strip_prefix(b"i=") else {
            continue;
        };
        let text = std::str::from_utf8(value).ok()?;
        if let Ok(id) = text.parse::<u32>() {
            return Some(id);
        }
    }
    None
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ---------------------------------------------------------------------------
// Resize polling
// ---------------------------------------------------------------------------

fn current_terminal_geometry(kitty_graphics_enabled: bool) -> (u16, u16, u32, u32) {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    if !kitty_graphics_enabled {
        return (cols, rows, 0, 0);
    }
    let Ok(size) = crossterm::terminal::window_size() else {
        return (cols, rows, 8, 16);
    };
    if size.columns == 0 || size.rows == 0 || size.width == 0 || size.height == 0 {
        return (cols, rows, 8, 16);
    }
    (
        cols,
        rows,
        (size.width as u32 / size.columns as u32).max(1),
        (size.height as u32 / size.rows as u32).max(1),
    )
}

/// Polls the terminal size and sends resize events when it changes.
fn resize_poll_loop(
    resize_tx: tokio::sync::mpsc::Sender<ClientLoopEvent>,
    initial_cols: u16,
    initial_rows: u16,
    kitty_graphics_enabled: bool,
    should_quit: &Arc<AtomicBool>,
) {
    let (_, _, initial_cell_width, initial_cell_height) =
        current_terminal_geometry(kitty_graphics_enabled);
    let mut last_size = (
        initial_cols,
        initial_rows,
        initial_cell_width,
        initial_cell_height,
    );
    while !should_quit.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(100));
        let new_size = current_terminal_geometry(kitty_graphics_enabled);
        if new_size != last_size {
            last_size = new_size;
            if resize_tx
                .blocking_send(ClientLoopEvent::Resize(
                    new_size.0, new_size.1, new_size.2, new_size.3,
                ))
                .is_err()
            {
                break; // Main loop gone.
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

/// Initialize logging for the client process.
fn query_host_terminal_theme() {
    let _ = write_host_terminal_theme_query(io::stdout());
}

fn should_query_host_terminal_theme() -> bool {
    !cfg!(windows)
}
#[cfg(windows)]
fn windows_vti_input_backend_enabled() -> bool {
    std::env::var("HAKO_WINDOWS_INPUT_BACKEND")
        .map(|backend| !backend.eq_ignore_ascii_case("crossterm"))
        .unwrap_or(true)
}

fn write_host_terminal_theme_query(mut writer: impl io::Write) -> io::Result<()> {
    writer.write_all(crate::terminal_theme::HOST_COLOR_QUERY_SEQUENCE.as_bytes())?;
    writer.flush()
}

fn init_logging() {
    crate::logging::init_file_logging("hako-client.log");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarGuard {
        _env: crate::config::TestEnvVar,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            Self {
                _env: crate::config::TestEnvVar::set(key, value),
            }
        }
    }

    struct EnvVarsRemovedGuard {
        _envs: Vec<crate::config::TestEnvVar>,
    }

    impl EnvVarsRemovedGuard {
        fn new(keys: &[&'static str]) -> Self {
            Self {
                _envs: keys
                    .iter()
                    .map(|&key| crate::config::TestEnvVar::remove(key))
                    .collect(),
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn clipboard_image_paste_bridge_is_remote_scoped_and_configured() {
        let ctrl_v = crate::config::parse_key_combo("ctrl+v").unwrap();
        assert!(should_bridge_clipboard_image_paste(
            &[0x16],
            true,
            Some(ctrl_v)
        ));
        assert!(should_bridge_clipboard_image_paste(
            b"\x1b[118;5u",
            true,
            Some(ctrl_v)
        ));
        assert!(should_bridge_clipboard_image_paste(
            b"\x1b[200~\x1b[201~",
            true,
            None
        ));
        assert!(!should_bridge_clipboard_image_paste(
            b"\x1b[200~\x1b[201~",
            false,
            Some(ctrl_v)
        ));
        assert!(!should_bridge_clipboard_image_paste(
            b"\x1b[200~text\x1b[201~",
            true,
            Some(ctrl_v)
        ));
        assert!(!should_bridge_clipboard_image_paste(&[0x16], true, None));
        assert!(!should_bridge_clipboard_image_paste(
            b"v",
            true,
            Some(ctrl_v)
        ));
    }

    #[cfg(unix)]
    struct TempImageFile {
        path: std::path::PathBuf,
    }

    #[cfg(unix)]
    impl TempImageFile {
        fn new(extension: &str, bytes: &[u8]) -> Self {
            Self::with_name_fragment("test", extension, bytes)
        }

        fn with_name_fragment(name_fragment: &str, extension: &str, bytes: &[u8]) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "hako-client-drop-{name_fragment}-{}-{nanos}.{extension}",
                std::process::id()
            ));
            std::fs::write(&path, bytes).unwrap();
            Self { path }
        }
    }

    #[cfg(unix)]
    impl Drop for TempImageFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[cfg(unix)]
    #[test]
    fn remote_image_file_drop_bridge_reads_bracketed_absolute_image_path() {
        let file = TempImageFile::new("PNG", b"image-bytes");
        let input = format!("\x1b[200~{}\x1b[201~", file.path.display());

        let image = read_image_file_from_terminal_drop(input.as_bytes(), true).unwrap();

        assert_eq!(image.extension, "png");
        assert_eq!(image.bytes, b"image-bytes");
    }

    #[cfg(unix)]
    #[test]
    fn remote_image_file_drop_bridge_reads_plain_quoted_path_with_newline() {
        let file = TempImageFile::new("jpeg", b"jpeg-bytes");
        let input = format!("'{}'\n", file.path.display());

        let image = read_image_file_from_terminal_drop(input.as_bytes(), true).unwrap();

        assert_eq!(image.extension, "jpg");
        assert_eq!(image.bytes, b"jpeg-bytes");
    }

    #[cfg(unix)]
    #[test]
    fn remote_image_file_drop_bridge_unescapes_spaces_in_paths() {
        let file = TempImageFile::with_name_fragment("space test", "png", b"image-bytes");
        let escaped_path = file.path.display().to_string().replace(' ', "\\ ");

        let image = read_image_file_from_terminal_drop(escaped_path.as_bytes(), true).unwrap();

        assert_eq!(image.extension, "png");
        assert_eq!(image.bytes, b"image-bytes");
    }

    #[cfg(unix)]
    #[test]
    fn remote_image_file_drop_bridge_ignores_non_remote_and_non_image_input() {
        let file = TempImageFile::new("png", b"image-bytes");
        let path = file.path.display().to_string();

        assert!(read_image_file_from_terminal_drop(path.as_bytes(), false).is_none());
        assert!(read_image_file_from_terminal_drop(b"relative.png\n", true).is_none());
        assert!(read_image_file_from_terminal_drop(b"/tmp/file.txt\n", true).is_none());
        assert!(read_image_file_from_terminal_drop(
            format!("{}\nextra", file.path.display()).as_bytes(),
            true
        )
        .is_none());
    }

    #[test]
    fn graphics_bytes_are_written_after_blit_with_saved_cursor() {
        let mut output = Vec::new();
        write_encoded_frame_with_graphics(
            &mut output,
            b"\x1b[?2026htext\x1b[?2026lcursor",
            b"graphics",
        )
        .unwrap();

        assert_eq!(
            output,
            b"\x1b[?2026htext\x1b[?2026lcursor\x1b7graphics\x1b8"
        );
    }

    #[test]
    fn empty_graphics_writes_only_blit_frame() {
        let mut output = Vec::new();
        write_encoded_frame_with_graphics(&mut output, b"text", b"").unwrap();

        assert_eq!(output, b"text");
    }

    #[test]
    fn terminal_frame_kitty_detection_matches_apc_prefix() {
        assert!(contains_kitty_graphics_bytes(b"text\x1b_Ga=p;\x1b\\"));
        assert!(!contains_kitty_graphics_bytes(b"text\x1b[?2026h"));
    }

    #[test]
    fn kitty_graphics_image_id_parser_tracks_hako_ids_only() {
        let ids = kitty_graphics_image_ids(
            b"text\x1b_Ga=t,t=d,f=32,s=1,v=1,i=10023,q=2;AAAA\x1b\\\x1b_Ga=p,i=10023,p=7;\x1b\\",
        );
        assert_eq!(ids, vec![10023, 10023]);
    }

    #[test]
    fn kitty_graphics_cleanup_deletes_tracked_images_not_all_images() {
        record_received_kitty_graphics(b"\x1b_Ga=t,i=123,q=2;AAAA\x1b\\");
        let mut output = Vec::new();
        clear_received_kitty_graphics(&mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("a=d,d=I,i=123"));
        assert!(!text.contains("d=A"));
    }

    #[test]
    fn write_host_terminal_theme_query_emits_osc_queries() {
        let mut output = Vec::new();
        write_host_terminal_theme_query(&mut output).unwrap();
        assert_eq!(
            output,
            crate::terminal_theme::HOST_COLOR_QUERY_SEQUENCE.as_bytes()
        );
    }

    #[test]
    fn terminal_restore_postlude_restores_visible_default_cursor() {
        let mut output = Vec::new();
        write_terminal_restore_postlude(&mut output).unwrap();
        assert_eq!(output, b"\x1b[?25h\x1b[0 q");
    }

    #[test]
    fn attach_escape_detaches_on_prefix_q() {
        let mut escape = AttachEscapeState::default();
        assert!(matches!(
            escape.filter_input(vec![0x02]),
            AttachInputAction::None
        ));
        assert!(matches!(
            escape.filter_input(vec![b'q']),
            AttachInputAction::Detach
        ));
    }

    #[test]
    fn attach_escape_sends_literal_prefix_on_double_prefix() {
        let mut escape = AttachEscapeState::default();
        assert!(matches!(
            escape.filter_input(vec![0x02]),
            AttachInputAction::None
        ));
        match escape.filter_input(vec![0x02]) {
            AttachInputAction::Forward(bytes) => assert_eq!(bytes, vec![0x02]),
            other => panic!("expected forwarded prefix, got {other:?}"),
        }
    }

    #[test]
    fn attach_escape_forwards_prefix_before_non_escape_key() {
        let mut escape = AttachEscapeState::default();
        assert!(matches!(
            escape.filter_input(vec![b'a', 0x02]),
            AttachInputAction::Forward(bytes) if bytes == b"a"
        ));
        match escape.filter_input(vec![b'x']) {
            AttachInputAction::Forward(bytes) => assert_eq!(bytes, vec![0x02, b'x']),
            other => panic!("expected forwarded bytes, got {other:?}"),
        }
    }

    #[test]
    fn semantic_attach_events_preserve_detach_prefix() {
        use crate::protocol::{ClientInputEvent, ClientKeyCode, ClientKeyKind};

        let events = vec![
            ClientInputEvent::Key {
                code: ClientKeyCode::Char('b'),
                modifiers: crossterm::event::KeyModifiers::CONTROL.bits(),
                kind: ClientKeyKind::Press,
            },
            ClientInputEvent::Key {
                code: ClientKeyCode::Char('q'),
                modifiers: 0,
                kind: ClientKeyKind::Press,
            },
        ];
        assert_eq!(semantic_events_to_attach_bytes(&events), vec![0x02, b'q']);

        let mut escape = AttachEscapeState::default();
        assert!(matches!(
            escape.filter_input(semantic_events_to_attach_bytes(&events[..1])),
            AttachInputAction::None
        ));
        assert!(matches!(
            escape.filter_input(semantic_events_to_attach_bytes(&events[1..])),
            AttachInputAction::Detach
        ));
    }

    #[test]
    fn semantic_attach_events_forward_paste_and_ignore_releases() {
        use crate::protocol::{ClientInputEvent, ClientKeyCode, ClientKeyKind};

        let events = vec![
            ClientInputEvent::Key {
                code: ClientKeyCode::Char('x'),
                modifiers: 0,
                kind: ClientKeyKind::Release,
            },
            ClientInputEvent::Paste("paste".into()),
        ];
        assert_eq!(semantic_events_to_attach_bytes(&events), b"paste");

        let mut escape = AttachEscapeState::default();
        assert!(matches!(
            escape.filter_input(semantic_events_to_attach_bytes(&events)),
            AttachInputAction::Forward(bytes) if bytes == b"paste"
        ));
    }

    #[test]
    fn client_error_display_connection_failed() {
        let err = ClientError::ConnectionFailed(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "connection refused",
        ));
        let msg = err.to_string();
        assert!(
            msg.contains("failed to connect to server"),
            "should mention connection failure: {msg}"
        );
        assert!(
            msg.contains("hako server"),
            "should suggest starting server: {msg}"
        );
    }

    #[test]
    fn client_error_display_handshake_rejected() {
        let err = ClientError::HandshakeRejected {
            version: 1,
            error: "incompatible".into(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("rejected handshake"),
            "should mention rejection: {msg}"
        );
        assert!(msg.contains("incompatible"), "should include error: {msg}");
    }

    #[test]
    fn client_error_display_server_shutdown() {
        let err = ClientError::ServerShutdown {
            reason: Some("maintenance".into()),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("server shut down"),
            "should mention shutdown: {msg}"
        );
        assert!(msg.contains("maintenance"), "should include reason: {msg}");
    }

    #[test]
    fn client_error_display_server_shutdown_no_reason() {
        let err = ClientError::ServerShutdown { reason: None };
        let msg = err.to_string();
        assert!(
            msg.contains("server shut down"),
            "should mention shutdown: {msg}"
        );
    }

    #[test]
    fn client_error_display_detached_default_session_reattach_hint() {
        let _guard = env_lock().lock().unwrap();
        let _env = EnvVarsRemovedGuard::new(&[
            crate::remote::REATTACH_COMMAND_ENV_VAR,
            crate::session::SESSION_ENV_VAR,
        ]);
        let err = ClientError::ServerShutdown {
            reason: Some("detached".into()),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("Run `hako` to reattach"),
            "should suggest default reattach command: {msg}"
        );
    }

    #[test]
    fn client_error_display_detached_named_session_reattach_hint() {
        let _guard = env_lock().lock().unwrap();
        let _remote_env = EnvVarsRemovedGuard::new(&[crate::remote::REATTACH_COMMAND_ENV_VAR]);
        let _session_env = EnvVarGuard::set(crate::session::SESSION_ENV_VAR, "work");
        let err = ClientError::ServerShutdown {
            reason: Some("detached".into()),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("Run `hako session attach work` to reattach"),
            "should suggest named session reattach command: {msg}"
        );
    }

    #[test]
    fn client_error_display_detached_remote_reattach_hint_takes_precedence() {
        let _guard = env_lock().lock().unwrap();
        let _remote_env = EnvVarGuard::set(
            crate::remote::REATTACH_COMMAND_ENV_VAR,
            "hako --remote host --session work",
        );
        let _session_env = EnvVarGuard::set(crate::session::SESSION_ENV_VAR, "work");
        let err = ClientError::ServerShutdown {
            reason: Some("detached".into()),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("Run `hako --remote host --session work` to reattach"),
            "should prefer remote reattach command: {msg}"
        );
    }

    #[test]
    fn client_error_display_connection_lost() {
        let err = ClientError::from(protocol::FramingError::Io(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "connection reset",
        )));
        let msg = err.to_string();
        assert!(
            msg.contains("lost connection to server"),
            "should mention lost connection: {msg}"
        );
    }

    #[test]
    fn sound_from_notify_message_maps_done() {
        assert_eq!(
            sound_from_notify_message("agent done"),
            Some(crate::sound::Sound::Done)
        );
    }

    #[test]
    fn sound_from_notify_message_maps_attention() {
        assert_eq!(
            sound_from_notify_message("agent attention"),
            Some(crate::sound::Sound::Request)
        );
    }

    #[test]
    fn sound_from_notify_message_rejects_unknown_payloads() {
        assert_eq!(sound_from_notify_message("toast"), None);
    }

    #[test]
    fn reload_local_client_config_refreshes_redraw_on_focus_gained() {
        let _guard = crate::config::test_config_env_lock().lock().unwrap();
        let path = std::env::temp_dir().join(format!(
            "hako-client-config-reload-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, "[ui]\nredraw_on_focus_gained = false\n").unwrap();
        let path_string = path.to_string_lossy().to_string();
        let _env = EnvVarGuard::set(crate::config::CONFIG_PATH_ENV_VAR, &path_string);
        let mut sound_config = crate::config::SoundConfig::default();
        let mut redraw_on_focus_gained = true;
        let mut draw_host_cursor = false;
        let mut remote_image_paste_key = None;

        reload_local_client_config(
            &mut sound_config,
            &mut draw_host_cursor,
            &mut redraw_on_focus_gained,
            &mut remote_image_paste_key,
        );

        assert!(!redraw_on_focus_gained);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn remote_handshake_allows_high_latency_connection_setup() {
        assert_eq!(handshake_read_timeout(false), Duration::from_secs(5));
        assert_eq!(handshake_read_timeout(true), Duration::from_secs(60));
    }

    #[test]
    fn toast_notify_from_server_is_emitted_even_when_attach_config_was_off() {
        let sound_config = crate::config::SoundConfig::default();
        let mut emitted = None;

        handle_notify_with_notifiers(
            NotifyKind::Toast,
            "pi finished: workspace 1",
            &sound_config,
            |title, body| {
                emitted = Some((title.to_string(), body.map(str::to_string)));
                Ok(true)
            },
            |_, _| Ok(false),
        );

        assert_eq!(
            emitted,
            Some(("pi finished".to_string(), Some("workspace 1".to_string())))
        );
    }

    #[test]
    fn system_toast_notify_from_server_uses_system_notifier() {
        let sound_config = crate::config::SoundConfig::default();
        let mut emitted = None;

        handle_notify_with_notifiers(
            NotifyKind::SystemToast,
            "pi finished: workspace 1",
            &sound_config,
            |_, _| Ok(false),
            |title, body| {
                emitted = Some((title.to_string(), body.map(str::to_string)));
                Ok(true)
            },
        );

        assert_eq!(
            emitted,
            Some(("pi finished".to_string(), Some("workspace 1".to_string())))
        );
    }

    #[test]
    fn decode_clipboard_payload_decodes_base64() {
        assert_eq!(decode_clipboard_payload("dGVzdA=="), Some(b"test".to_vec()));
    }

    #[test]
    fn decode_clipboard_payload_rejects_invalid_base64() {
        assert_eq!(decode_clipboard_payload("not-base64!!!"), None);
    }

    #[test]
    fn forward_clipboard_uses_local_clipboard_path() {
        let _guard = env_lock().lock().unwrap();
        let _ssh_connection_env = EnvVarGuard::set("SSH_CONNECTION", "1 2 3 4");
        forward_clipboard("dGVzdA==");
    }

    #[test]
    fn window_title_osc_strips_terminators_and_defaults_to_hako() {
        assert_eq!(
            window_title_osc(Some("hako\x1b api\u{7}\u{9c}")),
            b"\x1b]0;hako api\x07"
        );
        assert_eq!(window_title_osc(None), b"\x1b]0;hako\x07");
    }
}
