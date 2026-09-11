use bytes::Bytes;
use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::time::Instant;
use tracing::warn;

use crate::{
    app::state::{AppState, Mode, PendingPaneMouseMotion, PendingPaneWheel},
    layout::PaneInfo,
    terminal::TerminalRuntimeRegistry,
};

impl AppState {
    pub(crate) fn handle_pane_mouse_only_for_view(
        &mut self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        client_view: &crate::app::view_state::ClientViewState,
        mouse: MouseEvent,
    ) {
        if client_view.mode != Mode::Terminal {
            return;
        }
        let Some(ws_idx) = client_view.active_workspace else {
            return;
        };
        let Some((column, row)) = client_view
            .tab_canvas_view
            .map_or(Some((mouse.column, mouse.row)), |view| {
                view.screen_to_canvas(mouse.column, mouse.row)
            })
        else {
            return;
        };
        let mouse = MouseEvent {
            column,
            row,
            ..mouse
        };
        let Some(info) = client_view
            .computed
            .pane_infos
            .iter()
            .find(|p| {
                u32::from(column) >= u32::from(p.inner_rect.x)
                    && u32::from(column) < u32::from(p.inner_rect.x) + u32::from(p.inner_rect.width)
                    && u32::from(row) >= u32::from(p.inner_rect.y)
                    && u32::from(row) < u32::from(p.inner_rect.y) + u32::from(p.inner_rect.height)
            })
            .cloned()
        else {
            return;
        };

        match mouse.kind {
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight => {
                self.forward_pane_reported_wheel_in_workspace(
                    terminal_runtimes,
                    ws_idx,
                    &info,
                    mouse,
                );
            }
            MouseEventKind::Down(_) | MouseEventKind::Up(_) => {
                self.forward_pane_mouse_button_in_workspace(
                    terminal_runtimes,
                    ws_idx,
                    &info,
                    mouse,
                );
            }
            MouseEventKind::Drag(_) | MouseEventKind::Moved => {
                self.forward_pane_mouse_motion_in_workspace(
                    terminal_runtimes,
                    ws_idx,
                    &info,
                    mouse,
                );
            }
        }
    }

    pub(crate) fn pane_scroll_metrics_in_workspace(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
    ) -> Option<crate::pane::ScrollMetrics> {
        self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, pane_id)
            .and_then(crate::terminal::TerminalRuntime::scroll_metrics)
    }

    pub(crate) fn forward_pane_mouse_button_in_workspace(
        &mut self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        ws_idx: usize,
        info: &PaneInfo,
        mouse: MouseEvent,
    ) -> bool {
        self.flush_pending_pane_mouse_motion(terminal_runtimes);
        let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)
        else {
            return false;
        };
        let column = mouse.column.saturating_sub(info.inner_rect.x);
        let row = mouse.row.saturating_sub(info.inner_rect.y);
        let Some(bytes) = self.encode_pane_mouse_button(
            rt,
            mouse.kind,
            column,
            row,
            mouse.modifiers,
            info.inner_rect,
            self.pointer_host_pixels,
        ) else {
            return false;
        };
        if !matches!(mouse.kind, MouseEventKind::Moved) {
            rt.scroll_reset();
        }
        if let Err(err) = rt.try_send_bytes(Bytes::from(bytes)) {
            warn!(pane = info.id.raw(), err = %err, kind = ?mouse.kind, "failed to forward mouse button event");
        }
        true
    }

    pub(crate) fn forward_pane_mouse_motion_in_workspace(
        &mut self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        ws_idx: usize,
        info: &PaneInfo,
        mouse: MouseEvent,
    ) -> bool {
        let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)
        else {
            return false;
        };
        let column = mouse.column.saturating_sub(info.inner_rect.x);
        let row = mouse.row.saturating_sub(info.inner_rect.y);
        let can_encode = match mouse.kind {
            MouseEventKind::Moved => self
                .encode_pane_mouse_motion(
                    rt,
                    mouse.kind,
                    column,
                    row,
                    mouse.modifiers,
                    info.inner_rect,
                    self.pointer_host_pixels,
                )
                .is_some(),
            MouseEventKind::Drag(_) => self
                .encode_pane_mouse_button(
                    rt,
                    mouse.kind,
                    column,
                    row,
                    mouse.modifiers,
                    info.inner_rect,
                    self.pointer_host_pixels,
                )
                .is_some(),
            _ => false,
        };
        if !can_encode {
            return false;
        }
        let now = Instant::now();
        let due = self
            .last_pane_mouse_motion_flush
            .is_none_or(|last| now.duration_since(last) >= super::super::MIN_RENDER_INTERVAL);
        if due {
            self.pending_pane_mouse_motion = None;
            self.last_pane_mouse_motion_flush = Some(now);
            self.send_pane_mouse_motion(
                terminal_runtimes,
                ws_idx,
                info.id,
                info.inner_rect,
                mouse,
                self.pointer_host_pixels,
            )
        } else {
            self.pending_pane_mouse_motion = Some(PendingPaneMouseMotion {
                ws_idx,
                pane_id: info.id,
                inner_rect: info.inner_rect,
                mouse,
                host_pixels: self.pointer_host_pixels,
            });
            true
        }
    }

    pub(crate) fn pane_mouse_motion_flush_at(&self) -> Option<Instant> {
        if self.pending_pane_mouse_motion.is_none() && self.pending_pane_wheel.is_none() {
            return None;
        }
        Some(
            self.last_pane_mouse_motion_flush
                .map(|last| last + super::super::MIN_RENDER_INTERVAL)
                .unwrap_or_else(Instant::now),
        )
    }

    pub(crate) fn flush_due_pane_mouse_motion(
        &mut self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        now: Instant,
    ) {
        let Some(deadline) = self.pane_mouse_motion_flush_at() else {
            return;
        };
        if now >= deadline {
            self.flush_pending_pane_mouse_motion(terminal_runtimes);
        }
    }

    pub(crate) fn flush_pending_pane_mouse_motion(
        &mut self,
        terminal_runtimes: &TerminalRuntimeRegistry,
    ) {
        let pending_motion = self.pending_pane_mouse_motion.take();
        let pending_wheel = self.pending_pane_wheel.take();
        if pending_motion.is_none() && pending_wheel.is_none() {
            return;
        }
        self.last_pane_mouse_motion_flush = Some(Instant::now());
        if let Some(pending) = pending_motion {
            let _ = self.send_pane_mouse_motion(
                terminal_runtimes,
                pending.ws_idx,
                pending.pane_id,
                pending.inner_rect,
                pending.mouse,
                pending.host_pixels,
            );
        }
        if let Some(pending) = pending_wheel {
            self.send_pending_pane_wheel(terminal_runtimes, pending);
        }
    }

    fn send_pane_mouse_motion(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
        inner_rect: Rect,
        mouse: MouseEvent,
        host_pixels: Option<(u32, u32)>,
    ) -> bool {
        let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, pane_id)
        else {
            return false;
        };
        let column = mouse.column.saturating_sub(inner_rect.x);
        let row = mouse.row.saturating_sub(inner_rect.y);
        let Some(bytes) = (match mouse.kind {
            MouseEventKind::Drag(_) => self.encode_pane_mouse_button(
                rt,
                mouse.kind,
                column,
                row,
                mouse.modifiers,
                inner_rect,
                host_pixels,
            ),
            _ => self.encode_pane_mouse_motion(
                rt,
                mouse.kind,
                column,
                row,
                mouse.modifiers,
                inner_rect,
                host_pixels,
            ),
        }) else {
            return false;
        };
        if let Err(err) = rt.try_send_bytes(Bytes::from(bytes)) {
            warn!(pane = pane_id.raw(), err = %err, kind = ?mouse.kind, "failed to forward mouse motion event");
        }
        true
    }

    fn forward_pane_reported_wheel_in_workspace(
        &mut self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        ws_idx: usize,
        info: &PaneInfo,
        mouse: MouseEvent,
    ) -> bool {
        let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)
        else {
            return false;
        };
        if !rt
            .input_state()
            .is_some_and(crate::pane::InputState::mouse_reporting_enabled)
        {
            return false;
        }
        self.queue_or_send_pane_wheel(terminal_runtimes, ws_idx, info, mouse)
    }

    pub(crate) fn normalize_host_mouse_event(&mut self, mouse: MouseEvent) -> MouseEvent {
        if !self.host_sgr_pixels {
            self.pointer_host_pixels = None;
            return mouse;
        }
        let width = self.host_cell_size.width_px;
        let height = self.host_cell_size.height_px;
        if width == 0 || height == 0 {
            self.pointer_host_pixels = None;
            return mouse;
        }
        let px = u32::from(mouse.column);
        let py = u32::from(mouse.row);
        self.pointer_host_pixels = Some((px, py));
        MouseEvent {
            column: (px / width) as u16,
            row: (py / height) as u16,
            ..mouse
        }
    }

    pub(crate) fn remap_host_pointer_pixels_to_canvas(
        &mut self,
        view: crate::app::view_state::TabCanvasViewport,
    ) {
        let Some((px, py)) = self.pointer_host_pixels else {
            return;
        };
        let cell = self.host_cell_size;
        if !cell.is_known() {
            return;
        }
        let destination = view.destination_rect();
        let canvas_x = px
            .saturating_sub(u32::from(destination.x).saturating_mul(cell.width_px))
            .saturating_add(u32::from(view.origin.col).saturating_mul(cell.width_px));
        let canvas_y = py
            .saturating_sub(u32::from(destination.y).saturating_mul(cell.height_px))
            .saturating_add(u32::from(view.origin.row).saturating_mul(cell.height_px));
        self.pointer_host_pixels = Some((canvas_x, canvas_y));
    }

    fn pane_pointer_surface(
        &self,
        inner_rect: Rect,
        host_pixels: Option<(u32, u32)>,
    ) -> Option<(f32, f32)> {
        let (px, py) = host_pixels?;
        let width = self.host_cell_size.width_px;
        let height = self.host_cell_size.height_px;
        if width == 0 || height == 0 {
            return None;
        }
        Some((
            px.saturating_sub(u32::from(inner_rect.x).saturating_mul(width)) as f32,
            py.saturating_sub(u32::from(inner_rect.y).saturating_mul(height)) as f32,
        ))
    }

    fn encode_pane_mouse_button(
        &self,
        rt: &crate::terminal::TerminalRuntime,
        kind: MouseEventKind,
        column: u16,
        row: u16,
        modifiers: KeyModifiers,
        inner_rect: Rect,
        host_pixels: Option<(u32, u32)>,
    ) -> Option<Vec<u8>> {
        if let Some((x, y)) = self.pane_pointer_surface(inner_rect, host_pixels) {
            rt.encode_mouse_button_xy(kind, x, y, modifiers)
        } else {
            rt.encode_mouse_button(kind, column, row, modifiers)
        }
    }

    fn encode_pane_mouse_motion(
        &self,
        rt: &crate::terminal::TerminalRuntime,
        kind: MouseEventKind,
        column: u16,
        row: u16,
        modifiers: KeyModifiers,
        inner_rect: Rect,
        host_pixels: Option<(u32, u32)>,
    ) -> Option<Vec<u8>> {
        if let Some((x, y)) = self.pane_pointer_surface(inner_rect, host_pixels) {
            rt.encode_mouse_motion_xy(kind, x, y, modifiers)
        } else {
            rt.encode_mouse_motion(kind, column, row, modifiers)
        }
    }

    fn encode_pane_mouse_wheel(
        &self,
        rt: &crate::terminal::TerminalRuntime,
        kind: MouseEventKind,
        column: u16,
        row: u16,
        modifiers: KeyModifiers,
        inner_rect: Rect,
        host_pixels: Option<(u32, u32)>,
    ) -> Option<Vec<u8>> {
        if let Some((x, y)) = self.pane_pointer_surface(inner_rect, host_pixels) {
            rt.encode_mouse_wheel_xy(kind, x, y, modifiers)
        } else {
            rt.encode_mouse_wheel(kind, column, row, modifiers)
        }
    }

    fn queue_or_send_pane_wheel(
        &mut self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        ws_idx: usize,
        info: &PaneInfo,
        mouse: MouseEvent,
    ) -> bool {
        let can_encode = {
            let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)
            else {
                return false;
            };
            let column = mouse.column.saturating_sub(info.inner_rect.x);
            let row = mouse.row.saturating_sub(info.inner_rect.y);
            self.encode_pane_mouse_wheel(
                rt,
                mouse.kind,
                column,
                row,
                mouse.modifiers,
                info.inner_rect,
                self.pointer_host_pixels,
            )
            .is_some()
        };
        if !can_encode {
            return false;
        }
        let now = Instant::now();
        let due = self
            .last_pane_mouse_motion_flush
            .is_none_or(|last| now.duration_since(last) >= super::super::MIN_RENDER_INTERVAL);
        if due {
            self.pending_pane_wheel = None;
            self.last_pane_mouse_motion_flush = Some(now);
            self.send_pane_wheel_ticks(
                terminal_runtimes,
                ws_idx,
                info.id,
                info.inner_rect,
                mouse,
                self.pointer_host_pixels,
                1,
            )
        } else {
            let mut pending = self.pending_pane_wheel.take().unwrap_or(PendingPaneWheel {
                ws_idx,
                pane_id: info.id,
                inner_rect: info.inner_rect,
                mouse,
                host_pixels: self.pointer_host_pixels,
                up: 0,
                down: 0,
                left: 0,
                right: 0,
            });
            pending.ws_idx = ws_idx;
            pending.pane_id = info.id;
            pending.inner_rect = info.inner_rect;
            pending.mouse = mouse;
            pending.host_pixels = self.pointer_host_pixels;
            match mouse.kind {
                MouseEventKind::ScrollUp => pending.up = pending.up.saturating_add(1),
                MouseEventKind::ScrollDown => pending.down = pending.down.saturating_add(1),
                MouseEventKind::ScrollLeft => pending.left = pending.left.saturating_add(1),
                MouseEventKind::ScrollRight => pending.right = pending.right.saturating_add(1),
                _ => {}
            }
            self.pending_pane_wheel = Some(pending);
            true
        }
    }

    fn send_pending_pane_wheel(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        pending: PendingPaneWheel,
    ) {
        let Some(rt) =
            self.runtime_for_pane_in_workspace(terminal_runtimes, pending.ws_idx, pending.pane_id)
        else {
            return;
        };
        rt.scroll_reset();
        for (kind, count) in [
            (MouseEventKind::ScrollUp, pending.up),
            (MouseEventKind::ScrollDown, pending.down),
            (MouseEventKind::ScrollLeft, pending.left),
            (MouseEventKind::ScrollRight, pending.right),
        ] {
            if count == 0 {
                continue;
            }
            let mouse = MouseEvent {
                kind,
                ..pending.mouse
            };
            let _ = self.send_pane_wheel_ticks(
                terminal_runtimes,
                pending.ws_idx,
                pending.pane_id,
                pending.inner_rect,
                mouse,
                pending.host_pixels,
                count,
            );
        }
    }

    fn send_pane_wheel_ticks(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
        inner_rect: Rect,
        mouse: MouseEvent,
        host_pixels: Option<(u32, u32)>,
        count: u32,
    ) -> bool {
        let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, pane_id)
        else {
            return false;
        };
        rt.scroll_reset();
        let column = mouse.column.saturating_sub(inner_rect.x);
        let row = mouse.row.saturating_sub(inner_rect.y);
        let Some(bytes) = self.encode_pane_mouse_wheel(
            rt,
            mouse.kind,
            column,
            row,
            mouse.modifiers,
            inner_rect,
            host_pixels,
        ) else {
            return false;
        };
        for _ in 0..count {
            if let Err(err) = rt.try_send_bytes(Bytes::from(bytes.clone())) {
                warn!(pane = pane_id.raw(), err = %err, kind = ?mouse.kind, "failed to forward mouse wheel event");
                break;
            }
        }
        true
    }
}
