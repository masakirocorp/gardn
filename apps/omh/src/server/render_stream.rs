//! Virtual rendering helpers for headless client frame streaming.

use ratatui::backend::{Backend, ClearType, TestBackend, WindowSize};
use ratatui::layout::{Position, Rect, Size};

use crate::app::state::AppState;
use crate::app::view_state::{
    apply_terminal_offsets_to_runtimes, capture_terminal_offsets_from_runtimes, ClientViewState,
};
use crate::app::Mode;
use crate::protocol::render_ansi::{BlitEncoder, EncodedBlit};
use crate::protocol::{CursorState, FrameData, RenderEncoding, ServerMessage, TerminalFrame};
use crate::terminal::TerminalRuntimeRegistry;

type RenderedKittyImages = Vec<((u16, u16), String, String)>;

/// Per-client render baseline for the negotiated render encoding.
pub(crate) enum ClientRenderState {
    /// Semantic clients compare full frame data and skip identical frames.
    Semantic { last_frame: Option<FrameData> },
    /// Terminal-ANSI clients keep a terminal diff encoder and sequence number.
    TerminalAnsi { blit_encoder: BlitEncoder, seq: u64 },
}

impl ClientRenderState {
    pub(crate) fn new(render_encoding: RenderEncoding) -> Self {
        match render_encoding {
            RenderEncoding::SemanticFrame => Self::Semantic { last_frame: None },
            RenderEncoding::TerminalAnsi => Self::TerminalAnsi {
                blit_encoder: BlitEncoder::new(),
                seq: 0,
            },
        }
    }

    pub(crate) fn reset_baseline(&mut self) {
        match self {
            Self::Semantic { last_frame } => *last_frame = None,
            Self::TerminalAnsi { blit_encoder, .. } => *blit_encoder = BlitEncoder::new(),
        }
    }

    pub(crate) fn reset_semantic_input_baseline(&mut self) {
        if let Self::Semantic { last_frame } = self {
            *last_frame = None;
        }
    }

    pub(crate) fn prepare_frame(&mut self, frame: FrameData) -> Option<PreparedRender> {
        match self {
            Self::Semantic { last_frame } => {
                if last_frame.as_ref() == Some(&frame) {
                    return None;
                }
                Some(PreparedRender::Semantic {
                    message: ServerMessage::Frame(frame),
                })
            }
            Self::TerminalAnsi { blit_encoder, seq } => {
                if blit_encoder.is_current(&frame) {
                    return None;
                }
                let mut encoded = blit_encoder.encode(&frame, false);
                insert_graphics_before_sync_end(&mut encoded.bytes, &frame.graphics);
                Some(PreparedRender::TerminalAnsi {
                    message: ServerMessage::Terminal(TerminalFrame {
                        seq: *seq + 1,
                        width: frame.width,
                        height: frame.height,
                        full: encoded.full,
                        bytes: encoded.bytes.clone(),
                    }),
                    frame,
                    encoded: Some(encoded),
                })
            }
        }
    }

    pub(crate) fn commit_sent_frame(&mut self, prepared: PreparedRender) {
        match (self, prepared) {
            (
                Self::Semantic { last_frame },
                PreparedRender::Semantic {
                    message: ServerMessage::Frame(frame),
                },
            ) => *last_frame = Some(frame),
            (
                Self::TerminalAnsi { blit_encoder, seq },
                PreparedRender::TerminalAnsi {
                    frame,
                    encoded: Some(encoded),
                    ..
                },
            ) => {
                blit_encoder.commit(frame, encoded);
                *seq += 1;
            }
            _ => {}
        }
    }

    #[cfg(test)]
    pub(crate) fn terminal_seq(&self) -> Option<u64> {
        match self {
            Self::Semantic { .. } => None,
            Self::TerminalAnsi { seq, .. } => Some(*seq),
        }
    }
}

const SYNC_OUTPUT_END: &[u8] = b"\x1b[?2026l";

fn insert_graphics_before_sync_end(encoded: &mut Vec<u8>, graphics: &[u8]) {
    if graphics.is_empty() {
        return;
    }

    if let Some(sync_end) = rfind_subslice(encoded, SYNC_OUTPUT_END) {
        encoded.splice(sync_end..sync_end, graphics.iter().copied());
    } else {
        encoded.extend_from_slice(graphics);
    }
}

fn rfind_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }

    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

/// A prepared client render message plus any baseline state needed after send.
pub(crate) enum PreparedRender {
    Semantic {
        message: ServerMessage,
    },
    TerminalAnsi {
        message: ServerMessage,
        frame: FrameData,
        encoded: Option<EncodedBlit>,
    },
}

impl PreparedRender {
    pub(crate) fn message(&self) -> &ServerMessage {
        match self {
            Self::Semantic { message } | Self::TerminalAnsi { message, .. } => message,
        }
    }

    pub(crate) fn into_frame(self) -> Option<FrameData> {
        match self {
            Self::Semantic {
                message: ServerMessage::Frame(frame),
            } => Some(frame),
            Self::TerminalAnsi { frame, .. } => Some(frame),
            _ => None,
        }
    }
}

struct CursorTrackingBackend {
    inner: TestBackend,
    rendered_cursor: Option<Position>,
}

impl CursorTrackingBackend {
    fn new(width: u16, height: u16) -> Self {
        Self {
            inner: TestBackend::new(width, height),
            rendered_cursor: None,
        }
    }

    fn buffer(&self) -> &ratatui::buffer::Buffer {
        self.inner.buffer()
    }

    fn rendered_cursor(&self) -> Option<CursorState> {
        self.rendered_cursor.map(|pos| CursorState {
            x: pos.x,
            y: pos.y,
            visible: true,
            shape: 0,
        })
    }
}

impl Backend for CursorTrackingBackend {
    type Error = std::convert::Infallible;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
    {
        self.inner.draw(content)
    }

    fn append_lines(&mut self, n: u16) -> Result<(), Self::Error> {
        self.inner.append_lines(n)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()?;
        self.rendered_cursor = None;
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        let position = position.into();
        self.inner.set_cursor_position(position)?;
        self.rendered_cursor = Some(position);
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush()
    }
}

/// Renders the AppState to an in-memory ratatui Buffer.
///
/// This produces the same output as the monolithic binary's terminal draw,
/// but writes to a `Buffer` instead of stdout. Cursor visibility is captured
/// from explicit frame cursor intent rather than incidental backend state.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn render_virtual(
    app_state: &mut AppState,
    area: Rect,
    resize_panes: bool,
) -> (ratatui::buffer::Buffer, Option<CursorState>) {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    render_virtual_with_runtime_registry(
        app_state,
        &terminal_runtimes,
        area,
        resize_panes,
        crate::kitty_graphics::HostCellSize::default(),
    )
}

pub(crate) fn render_virtual_with_runtime_registry(
    app_state: &mut AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
    resize_panes: bool,
    cell_size: crate::kitty_graphics::HostCellSize,
) -> (ratatui::buffer::Buffer, Option<CursorState>) {
    if resize_panes {
        crate::ui::compute_view_with_cell_size(app_state, terminal_runtimes, area, cell_size);
    } else {
        crate::ui::compute_view_without_resizing_panes(app_state, terminal_runtimes, area);
    }

    let backend = CursorTrackingBackend::new(area.width, area.height);
    let mut terminal = ratatui::Terminal::new(backend).expect("TestBackend::new should never fail");

    terminal
        .draw(|frame| {
            crate::ui::render_with_runtime_registry(app_state, terminal_runtimes, frame);
        })
        .expect("render to TestBackend should never fail");

    let buffer = terminal.backend().buffer().clone();
    let cursor = focused_terminal_cursor(app_state, terminal_runtimes)
        .or_else(|| terminal.backend().rendered_cursor());

    (buffer, cursor)
}
fn capture_terminal_offsets(
    app_state: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> std::collections::HashMap<crate::terminal::TerminalId, usize> {
    let mut offsets = std::collections::HashMap::new();
    for workspace in &app_state.workspaces {
        for tab in &workspace.tabs {
            for pane in tab.panes.values() {
                let Some(terminal_id) = pane.terminal_id() else {
                    continue;
                };
                let Some(metrics) = terminal_runtimes
                    .get(terminal_id)
                    .and_then(|runtime| runtime.scroll_metrics())
                else {
                    continue;
                };
                offsets.insert(terminal_id.clone(), metrics.offset_from_bottom);
            }
        }
    }
    offsets
}

fn live_terminal_ids(app_state: &AppState) -> Vec<crate::terminal::TerminalId> {
    let mut ids = Vec::new();
    for workspace in &app_state.workspaces {
        for tab in &workspace.tabs {
            for pane in tab.panes.values() {
                if let Some(terminal_id) = pane.terminal_id() {
                    ids.push(terminal_id.clone());
                }
            }
        }
    }
    ids
}

fn restore_terminal_offsets(
    terminal_runtimes: &TerminalRuntimeRegistry,
    offsets: &std::collections::HashMap<crate::terminal::TerminalId, usize>,
) {
    for (terminal_id, offset) in offsets {
        if let Some(runtime) = terminal_runtimes.get(terminal_id) {
            runtime.set_scroll_offset_from_bottom(*offset);
        }
    }
}

pub(crate) fn render_virtual_for_client_view(
    app_state: &mut AppState,
    client_view: &mut ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
    resize_panes: bool,
    cell_size: crate::kitty_graphics::HostCellSize,
) -> (
    ratatui::buffer::Buffer,
    Option<CursorState>,
    RenderedKittyImages,
) {
    let live_terminal_ids = live_terminal_ids(app_state);
    let shared_offsets = capture_terminal_offsets(app_state, terminal_runtimes);

    client_view.reconcile(app_state);
    apply_terminal_offsets_to_runtimes(&live_terminal_ids, terminal_runtimes, client_view);

    if resize_panes {
        crate::ui::compute_view_for_client_with_cell_size(
            app_state,
            client_view,
            terminal_runtimes,
            area,
            cell_size,
        );
    } else {
        crate::ui::compute_view_for_client_without_resizing_panes(
            app_state,
            client_view,
            terminal_runtimes,
            area,
        );
    }

    let backend = CursorTrackingBackend::new(area.width, area.height);
    let mut terminal = ratatui::Terminal::new(backend).expect("TestBackend::new should never fail");

    terminal
        .draw(|frame| {
            crate::ui::render_with_runtime_registry_for_view(
                app_state,
                client_view,
                terminal_runtimes,
                frame,
            );
        })
        .expect("render to TestBackend should never fail");

    let buffer = terminal.backend().buffer().clone();
    let cursor = if client_view.can_mutate_tab() {
        focused_terminal_cursor_for_view(app_state, client_view, terminal_runtimes)
            .or_else(|| terminal.backend().rendered_cursor())
    } else {
        None
    };

    let hyperlinks = visible_hyperlinks_for_view(app_state, client_view, terminal_runtimes);
    capture_terminal_offsets_from_runtimes(&live_terminal_ids, terminal_runtimes, client_view);
    restore_terminal_offsets(terminal_runtimes, &shared_offsets);
    (buffer, cursor, hyperlinks)
}

/// Renders one server-owned terminal directly for `terminal attach` clients.
pub(crate) fn render_terminal_virtual(
    runtime: &crate::terminal::TerminalRuntime,
    area: Rect,
) -> (ratatui::buffer::Buffer, Option<CursorState>) {
    let backend = CursorTrackingBackend::new(area.width, area.height);
    let mut terminal = ratatui::Terminal::new(backend).expect("TestBackend::new should never fail");

    terminal
        .draw(|frame| {
            runtime.render_with_theme_background(frame, area, true, None);
        })
        .expect("render to TestBackend should never fail");

    let buffer = terminal.backend().buffer().clone();
    let cursor = runtime
        .cursor_state(area, true)
        .map(|cursor| CursorState {
            x: cursor.x,
            y: cursor.y,
            visible: cursor.visible && !crate::ui::pane_is_scrolled_back(runtime),
            shape: cursor.shape,
        })
        .or_else(|| terminal.backend().rendered_cursor());

    (buffer, cursor)
}

pub(crate) fn visible_hyperlinks_for_view(
    app_state: &AppState,
    client_view: &ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Vec<((u16, u16), String, String)> {
    let Some(ws_idx) = client_view.active_workspace else {
        return Vec::new();
    };
    let Some(workspace) = app_state.workspaces.get(ws_idx) else {
        return Vec::new();
    };
    let Some(tab_idx) = client_view.active_tab_index_for_workspace(app_state, ws_idx) else {
        return Vec::new();
    };
    let Some(tab) = workspace.tabs.get(tab_idx) else {
        return Vec::new();
    };

    let mut links = Vec::new();
    for info in &client_view.computed.pane_infos {
        let Some(runtime) = tab
            .terminal_id(info.id)
            .and_then(|terminal_id| terminal_runtimes.get(terminal_id))
        else {
            continue;
        };
        for link in runtime.visible_hyperlinks(info.inner_rect) {
            if let Some(link) = project_hyperlink_for_view(client_view, link) {
                links.push(link);
            }
        }
    }
    links
}

fn project_hyperlink_for_view(
    client_view: &ClientViewState,
    link: ((u16, u16), String, String),
) -> Option<((u16, u16), String, String)> {
    let ((x, y), label, uri) = link;
    let position = client_view
        .tab_canvas_view
        .map(|canvas_view| canvas_view.canvas_to_screen(x, y))
        .unwrap_or(Some((x, y)))?;
    Some((position, label, uri))
}

fn project_cursor_for_view(
    client_view: &ClientViewState,
    mut cursor: CursorState,
) -> Option<CursorState> {
    let Some(canvas_view) = client_view.tab_canvas_view else {
        return Some(cursor);
    };
    let (x, y) = canvas_view.canvas_to_screen(cursor.x, cursor.y)?;
    cursor.x = x;
    cursor.y = y;
    Some(cursor)
}

fn focused_terminal_cursor_for_view(
    app_state: &AppState,
    client_view: &ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Option<CursorState> {
    if client_view.mode != Mode::Terminal {
        return None;
    }
    // Watchers never see a terminal cursor: their keystrokes go nowhere, so
    // the honest caret is a hidden one. This gates the CJK IME reveal path
    // below as well.
    if client_view.tab_control.is_watching() {
        return None;
    }

    let ws_idx = client_view.active_workspace?;
    let info = client_view
        .computed
        .pane_infos
        .iter()
        .find(|info| info.is_focused)?;
    let workspace = app_state.workspaces.get(ws_idx)?;
    let terminal_id = workspace.terminal_id(info.id)?;
    let rt = terminal_runtimes.get(terminal_id)?;
    let scrolled_back = crate::ui::pane_is_scrolled_back(rt);

    let reveal = app_state.reveal_hidden_cursor_for_cjk_ime
        && (!app_state.cjk_ime_agent_filter_configured || {
            let detected = app_state
                .terminals
                .get(terminal_id)
                .and_then(|t| t.detected_agent);
            detected.is_some_and(|agent| app_state.cjk_ime_agents.contains(&agent))
        });

    let cursor = if let Some(cursor) = rt.cursor_state(info.inner_rect, true) {
        let visible = if reveal {
            !scrolled_back
        } else {
            cursor.visible && !scrolled_back
        };
        Some(CursorState {
            x: cursor.x,
            y: cursor.y,
            visible,
            shape: if reveal && visible {
                app_state.cjk_ime_cursor_shape
            } else {
                cursor.shape
            },
        })
    } else if reveal && !scrolled_back {
        Some(CursorState {
            x: info.inner_rect.x,
            y: info.inner_rect.y,
            visible: true,
            shape: app_state.cjk_ime_cursor_shape,
        })
    } else {
        None
    };
    cursor.and_then(|cursor| project_cursor_for_view(client_view, cursor))
}

fn focused_terminal_cursor(
    app_state: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Option<CursorState> {
    if app_state.mode != Mode::Terminal {
        return None;
    }

    let ws_idx = app_state.active?;
    let info = app_state
        .view
        .pane_infos
        .iter()
        .find(|info| info.is_focused)?;
    let rt = app_state.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)?;
    let scrolled_back = crate::ui::pane_is_scrolled_back(rt);

    // Determine whether the IME-anchor reveal applies to this focused pane.
    // The master switch must be on, and either no agent filter is configured
    // (apply to any pane) or the focused pane's detected agent matches the
    // allow-list. A configured list with no valid entries reveals nothing.
    let reveal = app_state.reveal_hidden_cursor_for_cjk_ime
        && (!app_state.cjk_ime_agent_filter_configured || {
            let detected = app_state
                .workspaces
                .get(ws_idx)
                .and_then(|ws| ws.terminal_id(info.id))
                .and_then(|tid| app_state.terminals.get(tid))
                .and_then(|t| t.detected_agent);
            detected.is_some_and(|agent| app_state.cjk_ime_agents.contains(&agent))
        });

    if let Some(cursor) = rt.cursor_state(info.inner_rect, true) {
        // When the reveal applies, expose the cursor anchor regardless of the
        // pane's `?25l` request so macOS IMEs keep tracking the candidate
        // window when TUIs paint their own cursor. Scrollback suppression
        // still applies.
        let visible = if reveal {
            !scrolled_back
        } else {
            cursor.visible && !scrolled_back
        };
        Some(CursorState {
            x: cursor.x,
            y: cursor.y,
            visible,
            shape: if reveal && visible {
                app_state.cjk_ime_cursor_shape
            } else {
                cursor.shape
            },
        })
    } else if reveal && !scrolled_back {
        // cursor_state() returned None — the viewport has no cursor position
        // (can happen with complex TUIs). Fall back to the pane's top-left so
        // the outer terminal still exposes a cursor anchor for IME tracking.
        Some(CursorState {
            x: info.inner_rect.x,
            y: info.inner_rect.y,
            visible: true,
            shape: app_state.cjk_ime_cursor_shape,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ClientTabControl, ClientViewState};
    use crate::workspace::Workspace;

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        let mut text = String::new();
        for y in buffer.area.y..buffer.area.y + buffer.area.height {
            for x in buffer.area.x..buffer.area.x + buffer.area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }
    #[test]
    fn client_view_rendering_keeps_shared_view_state_isolated() {
        let mut state = AppState::test_new();
        state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        state.active = Some(0);
        state.selected = 0;
        crate::ui::compute_view(&mut state, Rect::new(0, 0, 100, 30));
        let shared_before = ClientViewState::from_default_client_state(&state);

        let mut first_client = ClientViewState::from_default_client_state(&state);
        let mut second_client = ClientViewState::from_default_client_state(&state);
        second_client.active_workspace = Some(1);
        second_client.selected_workspace = 1;

        let terminal_runtimes = TerminalRuntimeRegistry::new();
        render_virtual_for_client_view(
            &mut state,
            &mut first_client,
            &terminal_runtimes,
            Rect::new(0, 0, 120, 40),
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        render_virtual_for_client_view(
            &mut state,
            &mut second_client,
            &terminal_runtimes,
            Rect::new(0, 0, 80, 24),
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );

        let shared_after = ClientViewState::from_default_client_state(&state);
        assert_eq!(
            shared_after.active_workspace,
            shared_before.active_workspace
        );
        assert_eq!(
            shared_after.selected_workspace,
            shared_before.selected_workspace
        );
        assert_eq!(
            shared_after.computed.terminal_area,
            shared_before.computed.terminal_area
        );
        assert_eq!(first_client.active_workspace, Some(0));
        assert_eq!(second_client.active_workspace, Some(1));
        assert_ne!(
            first_client.computed.terminal_area,
            second_client.computed.terminal_area
        );
    }

    #[test]
    fn client_view_rendering_uses_client_workspace_tab_and_sidebar_state() {
        let mut state = AppState::test_new();
        let mut shared_workspace = Workspace::test_new("leftspace");
        let shared_tab = shared_workspace.test_add_tab(Some("sharedtab"));
        shared_workspace.switch_tab(shared_tab);
        let mut client_workspace = Workspace::test_new("rightspace");
        client_workspace.test_add_tab(Some("clienttab"));
        let client_workspace_id = client_workspace.id.clone();

        state.workspaces = vec![shared_workspace, client_workspace];
        state.active = Some(0);
        state.selected = 0;
        state.mode = crate::app::Mode::Terminal;
        state.sidebar_collapsed = false;

        let mut client = ClientViewState::from_default_client_state(&state);
        client.active_workspace = Some(1);
        client.selected_workspace = 1;
        client.sidebar_collapsed = true;
        client.active_tabs.insert(client_workspace_id, 1);

        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let (buffer, _, _) = render_virtual_for_client_view(
            &mut state,
            &mut client,
            &terminal_runtimes,
            Rect::new(0, 0, 120, 30),
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let text = buffer_text(&buffer);

        assert!(
            text.contains("clienttab"),
            "client render should show the invoking client's active tab:\n{text}"
        );
        assert!(
            !text.contains("sharedtab"),
            "client render must not use the shared active workspace tab bar:\n{text}"
        );
        assert!(
            !text.contains("leftspace"),
            "client render must respect the invoking client's collapsed sidebar:\n{text}"
        );
        assert_eq!(state.active, Some(0));
        assert!(!state.sidebar_collapsed);
    }

    #[test]
    fn client_rename_modal_uses_the_invoking_clients_text_input() {
        let mut state = AppState::test_new();
        state.workspaces = vec![
            Workspace::test_new("rename-first-space"),
            Workspace::test_new("rename-second-space"),
        ];
        state.active = Some(0);
        state.selected = 0;
        state.mode = crate::app::Mode::RenameWorkspace;
        state.name_input = "shared-rename-default".to_string();

        let mut first_client = ClientViewState::from_default_client_state(&state);
        first_client.name_input = "first-client-rename".to_string();
        let mut second_client = ClientViewState::from_default_client_state(&state);
        second_client.active_workspace = Some(1);
        second_client.selected_workspace = 1;
        second_client.name_input = "second-client-rename".to_string();

        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let (first_buffer, _, _) = render_virtual_for_client_view(
            &mut state,
            &mut first_client,
            &terminal_runtimes,
            Rect::new(0, 0, 120, 40),
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let (second_buffer, _, _) = render_virtual_for_client_view(
            &mut state,
            &mut second_client,
            &terminal_runtimes,
            Rect::new(0, 0, 120, 40),
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let first_text = buffer_text(&first_buffer);
        let second_text = buffer_text(&second_buffer);

        assert!(
            first_text.contains("first-client-rename"),
            "first client should see its rename text input:\n{first_text}"
        );
        assert!(
            !first_text.contains("shared-rename-default")
                && !first_text.contains("second-client-rename"),
            "first client must not see another client's or the shared rename text input:\n{first_text}"
        );
        assert!(
            second_text.contains("second-client-rename"),
            "second client should see its rename text input:\n{second_text}"
        );
        assert!(
            !second_text.contains("shared-rename-default")
                && !second_text.contains("first-client-rename"),
            "second client must not see another client's or the shared rename text input:\n{second_text}"
        );
    }

    #[test]
    fn client_navigator_uses_the_invoking_clients_search_query() {
        let mut state = AppState::test_new();
        state.workspaces = vec![
            Workspace::test_new("navigator-first-space"),
            Workspace::test_new("navigator-second-space"),
        ];
        state.active = Some(0);
        state.selected = 0;
        state.mode = crate::app::Mode::Navigator;
        state.navigator.query = "shared-navigator-default".to_string();

        let mut first_client = ClientViewState::from_default_client_state(&state);
        first_client.navigator.query = "first-client-navigator".to_string();
        let mut second_client = ClientViewState::from_default_client_state(&state);
        second_client.active_workspace = Some(1);
        second_client.selected_workspace = 1;
        second_client.navigator.query = "second-client-navigator".to_string();

        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let (first_buffer, _, _) = render_virtual_for_client_view(
            &mut state,
            &mut first_client,
            &terminal_runtimes,
            Rect::new(0, 0, 120, 40),
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let (second_buffer, _, _) = render_virtual_for_client_view(
            &mut state,
            &mut second_client,
            &terminal_runtimes,
            Rect::new(0, 0, 120, 40),
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let first_text = buffer_text(&first_buffer);
        let second_text = buffer_text(&second_buffer);

        assert!(
            first_text.contains("first-client-navigator"),
            "first client should see its navigator search query:\n{first_text}"
        );
        assert!(
            !first_text.contains("shared-navigator-default")
                && !first_text.contains("second-client-navigator"),
            "first client must not see another client's or the shared navigator query:\n{first_text}"
        );
        assert!(
            second_text.contains("second-client-navigator"),
            "second client should see its navigator search query:\n{second_text}"
        );
        assert!(
            !second_text.contains("shared-navigator-default")
                && !second_text.contains("first-client-navigator"),
            "second client must not see another client's or the shared navigator query:\n{second_text}"
        );
    }

    #[tokio::test]
    async fn eng57_client_render_draws_copy_mode_cursor() {
        let mut state = AppState::test_new();
        let workspace = Workspace::test_new("copy-render");
        let pane_id = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane_id).cloned().unwrap();
        state.workspaces = vec![workspace];
        state.ensure_test_terminals();
        state.active = Some(0);
        state.selected = 0;
        state.mode = crate::app::Mode::Terminal;

        let mut client = ClientViewState::from_default_client_state(&state);
        client.mode = crate::app::Mode::Copy;
        client.copy_mode = Some(crate::app::state::CopyModeState::new(pane_id, 1, 2, None));
        let mut terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal_runtimes.insert(
            terminal_id,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(
                20,
                5,
                b"alpha\r\nbeta\r\ngamma\r\n",
            ),
        );

        let (buffer, _, _) = render_virtual_for_client_view(
            &mut state,
            &mut client,
            &terminal_runtimes,
            Rect::new(0, 0, 100, 20),
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );

        let pane = client
            .computed
            .pane_infos
            .iter()
            .find(|info| info.id == pane_id)
            .expect("copy-mode pane rendered");
        let (cursor_x, cursor_y) = client
            .tab_canvas_view
            .and_then(|view| view.canvas_to_screen(pane.inner_rect.x + 2, pane.inner_rect.y + 1))
            .expect("copy cursor should project into the client viewport");
        let cell = &buffer[(cursor_x, cursor_y)];
        assert_eq!(
            cell.style().bg,
            Some(state.active_workspace_accent_color()),
            "client copy mode should draw the visible cursor with the active workspace accent"
        );
        assert!(
            cell.style()
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD),
            "client copy-mode cursor should be visibly emphasized"
        );
    }

    fn watcher_cursor_fixture() -> (AppState, ClientViewState, TerminalRuntimeRegistry) {
        let mut state = AppState::test_new();
        let workspace = Workspace::test_new("watch-cursor");
        let pane_id = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane_id).cloned().unwrap();
        state.workspaces = vec![workspace];
        state.ensure_test_terminals();
        state.active = Some(0);
        state.selected = 0;
        state.mode = crate::app::Mode::Terminal;
        let mut terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal_runtimes.insert(
            terminal_id,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(
                20,
                5,
                b"alpha\r\nbeta\r\n\x1b[?25h",
            ),
        );
        let mut client = ClientViewState::from_default_client_state(&state);
        crate::ui::compute_view_for_client_without_resizing_panes(
            &state,
            &mut client,
            &terminal_runtimes,
            Rect::new(0, 0, 100, 20),
        );
        (state, client, terminal_runtimes)
    }
    #[tokio::test]
    async fn render_virtual_for_client_view_hides_watcher_terminal_cursor_but_keeps_controller_cursor(
    ) {
        let (mut state, mut client, terminal_runtimes) = watcher_cursor_fixture();
        let area = Rect::new(0, 0, 100, 20);

        let (_, controller_cursor, _) = render_virtual_for_client_view(
            &mut state,
            &mut client,
            &terminal_runtimes,
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        assert!(
            controller_cursor.is_some(),
            "full controller render should preserve the visible backend cursor"
        );

        client.set_tab_control(ClientTabControl::WatchingControlled { epoch: 1 });
        let (_, watcher_cursor, _) = render_virtual_for_client_view(
            &mut state,
            &mut client,
            &terminal_runtimes,
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        assert_eq!(
            watcher_cursor, None,
            "full watcher render must not leak a backend-rendered terminal cursor"
        );
    }

    #[test]
    fn hyperlink_projection_maps_canvas_coordinates_with_nonzero_origin() {
        let state = AppState::test_new();
        let mut client = ClientViewState::from_default_client_state(&state);
        client.tab_canvas_view = Some(crate::app::view_state::TabCanvasViewport::new(
            Size::new(120, 50),
            Rect::new(10, 5, 20, 10),
            crate::app::view_state::CanvasOrigin { col: 30, row: 12 },
        ));

        assert_eq!(
            project_hyperlink_for_view(
                &client,
                (
                    (32, 14),
                    "label".to_string(),
                    "https://example.test".to_string()
                ),
            ),
            Some((
                (12, 7),
                "label".to_string(),
                "https://example.test".to_string()
            ))
        );
        assert_eq!(
            project_hyperlink_for_view(
                &client,
                (
                    (29, 14),
                    "outside".to_string(),
                    "https://example.test".to_string()
                ),
            ),
            None,
            "links outside the canonical canvas must not enter the frame"
        );
    }

    #[test]
    fn hyperlink_projection_preserves_controller_origin_zero_and_rejects_padding() {
        let state = AppState::test_new();
        let mut client = ClientViewState::from_default_client_state(&state);
        client.tab_canvas_view = None;
        assert_eq!(
            project_hyperlink_for_view(
                &client,
                (
                    (3, 4),
                    "label".to_string(),
                    "https://example.test".to_string()
                ),
            ),
            Some((
                (3, 4),
                "label".to_string(),
                "https://example.test".to_string()
            ))
        );

        let viewport = crate::app::view_state::TabCanvasViewport::new(
            Size::new(12, 8),
            Rect::new(10, 5, 20, 10),
            crate::app::view_state::CanvasOrigin { col: 0, row: 0 },
        );
        assert_eq!(viewport.screen_to_canvas(22, 5), None);
        assert_eq!(viewport.screen_to_canvas(10, 13), None);
    }

    #[tokio::test]
    async fn watching_clients_have_no_terminal_cursor() {
        let (state, mut client, terminal_runtimes) = watcher_cursor_fixture();

        // The controller keeps the focused terminal cursor.
        assert!(
            focused_terminal_cursor_for_view(&state, &client, &terminal_runtimes).is_some(),
            "controller should keep the terminal cursor"
        );

        client.set_tab_control(ClientTabControl::WatchingControlled { epoch: 1 });
        assert_eq!(
            focused_terminal_cursor_for_view(&state, &client, &terminal_runtimes),
            None,
            "watching an occupied tab hides the cursor"
        );

        client.set_tab_control(ClientTabControl::WatchingFree { epoch: 1 });
        assert_eq!(
            focused_terminal_cursor_for_view(&state, &client, &terminal_runtimes),
            None,
            "watching a free tab hides the cursor"
        );
    }

    #[tokio::test]
    async fn cjk_ime_reveal_does_not_leak_cursor_to_watchers() {
        let (mut state, mut client, terminal_runtimes) = watcher_cursor_fixture();
        state.reveal_hidden_cursor_for_cjk_ime = true;

        // The reveal still applies to the controller.
        assert!(
            focused_terminal_cursor_for_view(&state, &client, &terminal_runtimes).is_some(),
            "controller keeps the revealed cursor"
        );

        client.set_tab_control(ClientTabControl::WatchingControlled { epoch: 1 });
        assert_eq!(
            focused_terminal_cursor_for_view(&state, &client, &terminal_runtimes),
            None,
            "reveal must not leak a cursor to watchers"
        );

        client.set_tab_control(ClientTabControl::WatchingFree { epoch: 1 });
        assert_eq!(
            focused_terminal_cursor_for_view(&state, &client, &terminal_runtimes),
            None,
            "reveal must not leak a cursor to watchers"
        );
    }

    #[test]
    fn watcher_frame_shows_control_chip_and_controller_frame_does_not() {
        let mut state = AppState::test_new();
        state.context_bar_visibility = crate::config::ContextBarVisibilityConfig::Always;
        let mut workspace = Workspace::test_new("ignored");
        workspace.custom_name = Some("website".into());
        workspace.tabs[0].custom_name = Some("release".into());
        state.workspaces = vec![workspace];
        state.active = Some(0);
        state.selected = 0;
        state.mode = crate::app::Mode::Terminal;

        let mut watcher = ClientViewState::from_default_client_state(&state);
        watcher.set_tab_control(ClientTabControl::WatchingControlled { epoch: 4 });
        let mut controller = ClientViewState::from_default_client_state(&state);

        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let (watcher_buffer, _, _) = render_virtual_for_client_view(
            &mut state,
            &mut watcher,
            &terminal_runtimes,
            Rect::new(0, 0, 100, 20),
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let (controller_buffer, _, _) = render_virtual_for_client_view(
            &mut state,
            &mut controller,
            &terminal_runtimes,
            Rect::new(0, 0, 100, 20),
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let watcher_text = buffer_text(&watcher_buffer);
        let controller_text = buffer_text(&controller_buffer);

        assert!(watcher_text.contains("WATCHING"), "{watcher_text}");
        assert!(
            watcher_text.contains("another client controls"),
            "{watcher_text}"
        );
        assert!(
            !controller_text.contains("WATCHING") && !controller_text.contains("FREE"),
            "controller frame must not carry watcher chrome:\n{controller_text}"
        );
        assert!(
            !controller_text.contains("another client controls"),
            "controller frame must not carry watcher copy:\n{controller_text}"
        );
    }
}
