use std::time::{Duration, Instant};

use crossterm::terminal;

use super::{
    background_update_check_enabled, mode_accepts_repeat_key, physical_key_identity, App, Mode,
    ANIMATION_INTERVAL, AUTO_UPDATE_CHECK_INTERVAL, COMMAND_SCAN_INTERVAL,
    GIT_REMOTE_STATUS_REFRESH_INTERVAL, MIN_RENDER_INTERVAL, PORT_SCAN_INTERVAL, PORT_STALE_TTL,
    RESIZE_POLL_INTERVAL, SELECTION_AUTOSCROLL_INTERVAL,
};
use crate::events::AppEvent;
use crate::workspace::{GitStatusCacheEntry, Workspace, WorkspaceGitStatus};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceGitRefreshItem {
    pub(crate) workspace_id: String,
    pub(crate) resolved_identity_cwd: std::path::PathBuf,
    pub(crate) location: crate::execution_host::ResourceLocation,
    pub(crate) cache_key: crate::execution_host::ResourceLocation,
    pub(crate) cwd_fingerprint: Vec<std::path::PathBuf>,
    pub(crate) observed_repo_roots: Vec<std::path::PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceGitRefreshTarget {
    pub(crate) workspace_id: String,
    pub(crate) resolved_identity_cwd: std::path::PathBuf,
    pub(crate) cwd_fingerprint: Vec<std::path::PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceGitRefreshJob {
    pub(crate) cache_key: crate::execution_host::ResourceLocation,
    pub(crate) status_cwd: std::path::PathBuf,
    pub(crate) cached: Option<GitStatusCacheEntry>,
    pub(crate) targets: Vec<WorkspaceGitRefreshTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceGitRefreshOutput {
    pub(crate) results: Vec<WorkspaceGitStatus>,
    pub(crate) cache_updates: Vec<(crate::execution_host::ResourceLocation, GitStatusCacheEntry)>,
    pub(crate) repo_summaries: Vec<(std::path::PathBuf, crate::workspace::GitWorkSummary)>,
}

fn retain_custom_command_after_wait(
    pid: u32,
    result: std::io::Result<Option<std::process::ExitStatus>>,
) -> bool {
    match result {
        Ok(None) => true,
        Ok(Some(_)) => false,
        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => true,
        Err(err) => {
            tracing::warn!(pid, err = %err, "failed to reap detached custom command");
            false
        }
    }
}

impl App {
    pub(crate) fn reap_finished_custom_commands(&mut self) {
        self.detached_custom_command_children
            .retain_mut(|child| retain_custom_command_after_wait(child.id(), child.try_wait()));
    }
    pub(crate) fn shutdown_detached_terminal_runtimes(&mut self) {
        for terminal_id in self.state.terminal_runtime_shutdowns.drain(..) {
            if let Some(runtime) = self.terminal_runtimes.remove(&terminal_id) {
                runtime.shutdown();
            }
        }
    }
    pub(crate) fn drain_api_requests(&mut self) -> bool {
        let mut changed = false;
        while let Ok(msg) = self.api_rx.try_recv() {
            changed |= self.handle_api_request_message(msg);
        }
        changed
    }

    pub(super) fn handle_api_request_message(
        &mut self,
        msg: crate::api::ApiRequestMessage,
    ) -> bool {
        let previous_mode = self.state.mode;
        let crate::api::ApiRequestMessage {
            request,
            respond_to,
            response_written: _,
        } = msg;
        let changed = crate::api::request_changes_ui(&request);
        match self.handle_api_request_disposition(request) {
            crate::api::ApiRequestDisposition::Respond(response) => {
                let _ = respond_to.send(response);
            }
            crate::api::ApiRequestDisposition::Deferred(deferred) => {
                // Insert once with the real responder; no placeholder channel.
                let (terminal_id, pending) =
                    crate::app::PendingRemoteApiResponse::from_deferred(deferred, respond_to);
                self.store_pending_remote_api_response(terminal_id, pending);
            }
        }
        self.sync_prefix_input_source(previous_mode);
        changed
    }

    pub(super) async fn handle_raw_input_batch(
        &mut self,
        first: crate::raw_input::RawInputEvent,
    ) -> bool {
        let mut changed = self.handle_raw_input_event(first).await;

        while let Some(rx) = self.input_rx.as_mut() {
            match rx.try_recv() {
                Ok(event) => changed |= self.handle_raw_input_event(event).await,
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.input_rx = None;
                    break;
                }
            }
        }

        changed
    }

    pub(super) async fn handle_raw_input_event(
        &mut self,
        event: crate::raw_input::RawInputEvent,
    ) -> bool {
        let previous_mode = self.state.mode;
        let changed = match event {
            crate::raw_input::RawInputEvent::Key(key) => {
                let suppress_direct_command_repeat = self.state.mode == Mode::Terminal
                    && crate::app::input::command_for_key(
                        &self.state,
                        key,
                        crate::app::input::BindingDispatch::Direct,
                    )
                    .is_some();
                let physical_id = physical_key_identity(&key);
                match key.kind {
                    crossterm::event::KeyEventKind::Press => {
                        let pressed_mode = self.state.mode;
                        self.forwarded_terminal_keys.remove(&physical_id);
                        self.suppressed_repeat_keys.remove(&physical_id);
                        if let Some(target) = self.handle_key(key).await {
                            self.forwarded_terminal_keys.insert(physical_id, target);
                        }
                        let suppress_repeat = if pressed_mode == Mode::Terminal {
                            self.state.mode != pressed_mode || suppress_direct_command_repeat
                        } else {
                            self.state.mode != pressed_mode
                                || !mode_accepts_repeat_key(pressed_mode, &key)
                        };
                        if suppress_repeat {
                            self.suppressed_repeat_keys.insert(physical_id);
                        }
                        true
                    }
                    crossterm::event::KeyEventKind::Repeat => {
                        if let Some(target) =
                            self.forwarded_terminal_keys.get(&physical_id).cloned()
                        {
                            self.forward_terminal_key_to_target(target, key).await;
                            true
                        } else if self.state.mode == Mode::Terminal
                            && !self.suppressed_repeat_keys.contains(&physical_id)
                        {
                            if let Some(target) = self.handle_key(key).await {
                                self.forwarded_terminal_keys.insert(physical_id, target);
                            }
                            true
                        } else if !self.suppressed_repeat_keys.contains(&physical_id)
                            && mode_accepts_repeat_key(self.state.mode, &key)
                        {
                            self.handle_key(key).await;
                            true
                        } else {
                            false
                        }
                    }
                    crossterm::event::KeyEventKind::Release => {
                        self.suppressed_repeat_keys.remove(&physical_id);
                        if let Some(target) = self.forwarded_terminal_keys.remove(&physical_id) {
                            self.forward_terminal_key_to_target(target, key).await;
                        }
                        false
                    }
                }
            }
            crate::raw_input::RawInputEvent::Paste(text) => {
                self.handle_paste(text).await;
                true
            }
            crate::raw_input::RawInputEvent::Mouse(mouse) => {
                if self.state.mouse_capture {
                    self.handle_mouse(mouse);
                } else {
                    self.state
                        .handle_pane_mouse_only(&self.terminal_runtimes, mouse);
                }
                true
            }
            crate::raw_input::RawInputEvent::OuterFocusGained => {
                self.send_outer_focus_event(crate::ghostty::FocusEvent::Gained);
                if self.state.redraw_on_focus_gained {
                    self.request_full_redraw();
                }
                self.state.outer_terminal_focus = Some(true);
                self.state.mark_active_tab_seen();
                self.query_host_terminal_theme();
                true
            }
            crate::raw_input::RawInputEvent::OuterFocusLost => {
                self.send_outer_focus_event(crate::ghostty::FocusEvent::Lost);
                self.state.outer_terminal_focus = Some(false);
                false
            }
            crate::raw_input::RawInputEvent::HostDefaultColor { kind, color } => {
                self.update_host_terminal_theme(kind, color)
            }
            crate::raw_input::RawInputEvent::HostPaletteColor { index, color } => {
                self.update_host_terminal_palette_color(index, color)
            }
            crate::raw_input::RawInputEvent::HostCursorColor { color } => {
                self.update_host_terminal_cursor_color(color)
            }
            crate::raw_input::RawInputEvent::Unsupported => false,
        };
        self.sync_prefix_input_source(previous_mode);
        changed
    }

    fn handle_resize_poll(&mut self) -> bool {
        let Ok(size) = terminal::size() else {
            return false;
        };
        if self.last_terminal_size != Some(size) {
            self.last_terminal_size = Some(size);
            return true;
        }
        false
    }

    pub(crate) fn refresh_ports(&mut self, now: Instant) -> bool {
        let terminals = &self.state.terminals;
        let terminal_targets = self
            .state
            .workspaces
            .iter()
            .flat_map(|workspace| {
                workspace
                    .tabs
                    .iter()
                    .enumerate()
                    .flat_map(move |(tab_idx, tab)| {
                        tab.layout
                            .pane_ids()
                            .into_iter()
                            .filter_map(move |pane_id| {
                                let terminal_id = tab.terminal_id(pane_id)?.clone();
                                let terminal = terminals.get(&terminal_id)?;
                                Some((
                                    terminal_id,
                                    terminal.location.clone(),
                                    crate::ports::PortOwner {
                                        pid: 0,
                                        command: None,
                                        workspace_id: workspace.id.clone(),
                                        tab_idx,
                                        pane_id,
                                        confidence: crate::ports::PortOwnerConfidence::ProcessTree,
                                    },
                                ))
                            })
                    })
            })
            .collect::<Vec<_>>();
        let mut owners = std::collections::HashMap::new();
        let mut remote_locations = std::collections::HashMap::new();
        for (terminal_id, location, owner) in terminal_targets {
            if location.is_local() {
                let Some(runtime) = self.terminal_runtimes.get(&terminal_id) else {
                    continue;
                };
                let child_pid = runtime.child_pid();
                if child_pid == 0 {
                    continue;
                }
                let mut pids = crate::platform::session_processes(child_pid);
                if pids.is_empty() {
                    pids.push(child_pid);
                }
                for pid in pids {
                    let mut owner = owner.clone();
                    owner.pid = pid;
                    owners.insert((location.execution_host_id.clone(), pid), owner);
                }
                continue;
            }

            remote_locations
                .entry(location.execution_host_id.clone())
                .or_insert_with(|| location.clone());
            let process = self.execution_hosts.as_mut().and_then(|hosts| {
                if let Err(error) = hosts.request_process_observation(&terminal_id) {
                    tracing::warn!("process observation request failed for {terminal_id}: {error}");
                }
                hosts
                    .process_observation(&terminal_id)
                    .map(crate::execution_host::HostObservation::to_status)
                    .and_then(|status| match status {
                        crate::execution_host::ObservationStatus::Ready(value)
                        | crate::execution_host::ObservationStatus::Stale(value) => Some(value),
                        crate::execution_host::ObservationStatus::Failed(error) => {
                            tracing::warn!(
                                "process observation failed for {terminal_id}: {}",
                                error.message
                            );
                            None
                        }
                        crate::execution_host::ObservationStatus::Pending => None,
                    })
            });
            if let Some(process) = process {
                let mut session = process.session_processes;
                if session.is_empty() && process.pid != 0 {
                    session.push(crate::execution_host::protocol::ObservedProcess {
                        pid: process.pid,
                        name: process
                            .command
                            .clone()
                            .unwrap_or_else(|| format!("pid-{}", process.pid)),
                        argv0: None,
                        argv: None,
                        cmdline: process.command.clone(),
                        cwd: process.cwd.clone(),
                    });
                }
                for session_process in session {
                    let mut owner = owner.clone();
                    owner.pid = session_process.pid;
                    owner.command = Some(session_process.name).or(session_process.cmdline);
                    owners.insert(
                        (location.execution_host_id.clone(), session_process.pid),
                        owner,
                    );
                }
            }
        }

        let mut observations = crate::execution_host::ResourceLocation::local("/")
            .ok()
            .and_then(|location| crate::execution_host::local::observe_ports(&location).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(crate::ports::PortObservation::from_worker_snapshot)
            .collect::<Vec<_>>();
        for location in remote_locations.into_values() {
            let remote = self.execution_hosts.as_mut().and_then(|hosts| {
                if let Err(error) = hosts.request_ports(location.clone()) {
                    tracing::warn!(
                        "port observation request failed for {}: {error}",
                        location.execution_host_id
                    );
                }
                hosts
                    .ports(&location)
                    .and_then(|observation| match observation.status() {
                        crate::execution_host::ObservationStatus::Ready(value) => {
                            Some(value.clone())
                        }
                        crate::execution_host::ObservationStatus::Failed(error) => {
                            tracing::warn!(
                                "port observation failed for {}: {}",
                                location.execution_host_id,
                                error.message
                            );
                            None
                        }
                        crate::execution_host::ObservationStatus::Pending
                        | crate::execution_host::ObservationStatus::Stale(_) => None,
                    })
            });
            observations.extend(
                remote
                    .into_iter()
                    .flatten()
                    .filter_map(crate::ports::PortObservation::from_worker_snapshot),
            );
        }

        let before = self.state.port_registry.endpoints();
        self.state
            .port_registry
            .sync_observations(now, observations, |host_id, pid| {
                owners.get(&(host_id.clone(), pid)).cloned()
            });
        self.state.port_registry.prune_stale(now, PORT_STALE_TTL);
        self.state.port_registry.endpoints() != before
    }

    pub(crate) fn handle_scheduled_tasks(&mut self, now: Instant, geometry_dirty: bool) -> bool {
        let mut changed = false;
        let mut resized = false;

        self.sync_animation_timer(now);

        if now >= self.next_resize_poll {
            resized = self.handle_resize_poll();
            changed |= resized;
            self.next_resize_poll = now + RESIZE_POLL_INTERVAL;
        }

        if now >= self.next_port_scan {
            changed |= self.refresh_ports(now);
            self.next_port_scan = now + PORT_SCAN_INTERVAL;
        }

        if now >= self.next_command_scan {
            changed |= self.state.refresh_command_catalog_with_hosts(
                &self.terminal_runtimes,
                self.execution_hosts.as_mut(),
            );
            changed |= self
                .state
                .refresh_command_run_statuses(&self.terminal_runtimes);
            self.next_command_scan = now + COMMAND_SCAN_INTERVAL;
        }

        if self
            .config_diagnostic_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.config_diagnostic_deadline = None;
            self.state.config_diagnostic = None;
            changed = true;
        }

        if self.toast_deadline.is_some_and(|deadline| now >= deadline) {
            self.toast_deadline = None;
            self.state.toast = None;
            changed = true;
        }

        if self
            .copy_feedback_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.copy_feedback_deadline = None;
            self.state.copy_feedback = None;
            changed = true;
        }
        if self
            .state
            .next_pending_agent_notification_deadline()
            .is_some_and(|deadline| now >= deadline)
        {
            let previous_toast = self.state.toast.clone();
            let mut deliveries = self.state.drain_due_agent_notifications(now);
            self.refresh_agent_notification_delivery_contexts(&mut deliveries);
            self.emit_delayed_client_local_agent_notifications(&deliveries);
            if !deliveries.is_empty() {
                self.sync_toast_deadline(previous_toast);
                changed = true;
            }
        }

        if self
            .next_animation_tick
            .is_some_and(|deadline| now >= deadline)
        {
            self.state.spinner_tick = self.state.spinner_tick.wrapping_add(1);
            self.next_animation_tick = Some(now + ANIMATION_INTERVAL);
            changed = true;
        }

        if self
            .selection_autoscroll_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.tick_selection_autoscroll(now);
            changed = true;
        }

        changed |= self.clear_due_selection_highlight(now);

        self.start_git_status_refresh_if_due(now);

        if self
            .next_auto_update_check
            .is_some_and(|deadline| now >= deadline)
        {
            self.run_auto_update_check();
        }

        if self
            .next_agent_manifest_update_check
            .is_some_and(|deadline| now >= deadline)
        {
            self.run_agent_manifest_update_check();
        }

        if self
            .session_save_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.start_background_session_save();
        }

        if let Some(deadline) = self
            .agent_metadata_deadline
            .filter(|deadline| now >= *deadline)
        {
            let previous_toast = self.state.toast.clone();
            for update in self.state.expire_agent_metadata_at(deadline, now) {
                self.refresh_new_omh_toast_context_for_update(&update, &previous_toast);
                self.emit_pane_state_update(&update);
            }
            self.sync_agent_metadata_deadline();
            changed = true;
        }

        if geometry_dirty || resized {
            self.pending_agent_resume_deadline = None;
        } else {
            self.sync_pending_agent_resume_deadline(now);
            changed |= self.start_pending_agent_resumes(self.pending_agent_resume_due(now));
        }

        self.sync_animation_timer(now);
        changed
    }

    /// Clears temporary copied-token highlights, such as after double-click copy.
    pub(crate) fn clear_due_selection_highlight(&mut self, now: Instant) -> bool {
        if self
            .selection_highlight_clear_deadline
            .is_none_or(|deadline| now < deadline)
        {
            return false;
        }

        self.selection_highlight_clear_deadline = None;
        if self
            .state
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_in_progress())
        {
            self.state.clear_selection();
            return true;
        }
        false
    }

    pub(crate) fn sync_agent_metadata_deadline(&mut self) {
        self.agent_metadata_deadline = self.state.next_agent_metadata_expiry();
    }

    pub(crate) fn sync_animation_timer(&mut self, now: Instant) {
        self.sync_animation_timer_with_interval(now, ANIMATION_INTERVAL, false);
    }

    pub(crate) fn sync_headless_animation_timer(
        &mut self,
        now: Instant,
        client_view_has_animation: bool,
    ) {
        self.sync_animation_timer_with_interval(
            now,
            crate::app::HEADLESS_ANIMATION_INTERVAL,
            client_view_has_animation,
        );
    }

    fn sync_animation_timer_with_interval(
        &mut self,
        now: Instant,
        interval: Duration,
        client_view_has_animation: bool,
    ) {
        if client_view_has_animation || self.has_local_animation() {
            self.next_animation_tick.get_or_insert(now + interval);
        } else {
            self.next_animation_tick = None;
        }
    }

    fn has_local_animation(&self) -> bool {
        self.agent_panel_has_animation()
            || self
                .state
                .settings
                .connection_editor
                .as_ref()
                .is_some_and(crate::app::state::ConnectionEditorState::retirement_in_progress)
            || self
                .default_client_view
                .settings
                .connection_editor
                .as_ref()
                .is_some_and(crate::app::state::ConnectionEditorState::retirement_in_progress)
    }

    fn agent_panel_has_animation(&self) -> bool {
        match self.state.agent_panel_scope {
            crate::app::state::AgentPanelScope::CurrentWorkspace => self
                .state
                .active
                .and_then(|idx| self.state.workspaces.get(idx))
                .is_some_and(|ws| ws.has_working_pane(&self.state.terminals)),
            crate::app::state::AgentPanelScope::CurrentGroup => {
                let group_id = self
                    .state
                    .active
                    .and_then(|idx| self.state.workspaces.get(idx))
                    .map(|ws| ws.group_id.as_str())
                    .unwrap_or_else(|| self.state.active_group_id());
                self.state
                    .workspaces
                    .iter()
                    .filter(|ws| ws.group_id == group_id)
                    .any(|ws| ws.has_working_pane(&self.state.terminals))
            }
            crate::app::state::AgentPanelScope::AllWorkspaces => self
                .state
                .workspaces
                .iter()
                .any(|ws| ws.has_working_pane(&self.state.terminals)),
        }
    }

    pub(crate) fn tick_selection_autoscroll(&mut self, now: Instant) {
        let Some(autoscroll) = self.state.selection_autoscroll.clone() else {
            // Self-heal: state cleared but deadline leaked
            self.selection_autoscroll_deadline = None;
            return;
        };

        // Selection must still be in progress for autoscroll to continue
        let Some(pane_id) = self.state.selection.as_ref().map(|s| s.pane_id) else {
            self.stop_selection_autoscroll();
            return;
        };
        if !self
            .state
            .selection
            .as_ref()
            .is_some_and(|s| s.is_dragging())
        {
            self.stop_selection_autoscroll();
            return;
        }

        // Rect-change detection: if inner_rect changed since drag, stop
        let current_rect = self
            .state
            .pane_info_by_id(pane_id)
            .map(|info| info.inner_rect);
        if current_rect != Some(autoscroll.inner_rect) {
            self.stop_selection_autoscroll();
            return;
        }

        // Scrollback boundary detection via ScrollMetrics — fail-closed if unavailable
        let Some(metrics) = self
            .state
            .pane_scroll_metrics(&self.terminal_runtimes, pane_id)
        else {
            self.stop_selection_autoscroll();
            return;
        };
        match autoscroll.direction {
            crate::app::state::SelectionAutoscrollDirection::Up => {
                let at_top = metrics.offset_from_bottom >= metrics.max_offset_from_bottom;
                if at_top {
                    self.stop_selection_autoscroll();
                    return;
                }
                self.state
                    .scroll_pane_up(&self.terminal_runtimes, pane_id, 1);
            }
            crate::app::state::SelectionAutoscrollDirection::Down => {
                let at_bottom = metrics.offset_from_bottom == 0;
                if at_bottom {
                    self.stop_selection_autoscroll();
                    return;
                }
                self.state
                    .scroll_pane_down(&self.terminal_runtimes, pane_id, 1);
            }
        }

        // Extend selection cursor to last known mouse position
        self.state.update_selection_cursor(
            &self.terminal_runtimes,
            pane_id,
            autoscroll.last_mouse_screen_col,
            autoscroll.last_mouse_screen_row,
        );

        // Reschedule
        self.selection_autoscroll_deadline = Some(now + SELECTION_AUTOSCROLL_INTERVAL);
    }

    pub(crate) fn stop_selection_autoscroll(&mut self) {
        self.state.stop_selection_autoscroll_state();
        self.selection_autoscroll_deadline = None;
    }

    pub(crate) fn can_render_now(&self, now: Instant) -> bool {
        match self.last_render_at {
            Some(last_render_at) => now.duration_since(last_render_at) >= MIN_RENDER_INTERVAL,
            None => true,
        }
    }

    pub(crate) fn run_auto_update_check(&mut self) {
        if !background_update_check_enabled(self.no_session, self.update_version_check_enabled) {
            self.next_auto_update_check = None;
            return;
        }

        self.next_auto_update_check = self
            .state
            .update_available
            .is_none()
            .then_some(Instant::now() + AUTO_UPDATE_CHECK_INTERVAL);

        if self.state.update_available.is_some() {
            return;
        }

        let update_tx = self.event_tx.clone();
        std::thread::spawn(move || crate::update::auto_update(update_tx));
    }

    pub(crate) fn run_agent_manifest_update_check(&mut self) {
        if !background_update_check_enabled(self.no_session, self.update_manifest_check_enabled) {
            self.next_agent_manifest_update_check = None;
            return;
        }

        self.next_agent_manifest_update_check = Some(Instant::now() + AUTO_UPDATE_CHECK_INTERVAL);

        let manifest_update_tx = self.event_tx.clone();
        std::thread::spawn(move || crate::detect::manifest_update::auto_update(manifest_update_tx));
    }

    pub(crate) fn start_git_status_refresh_if_due(&mut self, now: Instant) {
        let Some(deadline) = self.git_refresh_deadline() else {
            return;
        };

        if now < deadline {
            return;
        }

        let workspaces = self.workspace_git_refresh_items();

        if workspaces.is_empty() {
            self.last_git_remote_status_refresh = now;
            return;
        }

        let mut local_workspaces = Vec::new();
        let mut remote_results = Vec::new();
        for item in workspaces {
            if item.location.is_local() {
                local_workspaces.push(item);
                continue;
            }
            let snapshot = self.execution_hosts.as_mut().and_then(|hosts| {
                if let Err(error) = hosts.request_worktrees(item.location.clone()) {
                    tracing::warn!(
                        "worktree observation request failed for {}: {error}",
                        item.location.execution_host_id
                    );
                }
                let status_location = hosts
                    .worktrees(&item.location)
                    .and_then(|observation| match observation.status() {
                        crate::execution_host::ObservationStatus::Ready(worktrees) => {
                            Some(worktrees.as_slice())
                        }
                        crate::execution_host::ObservationStatus::Failed(error) => {
                            tracing::warn!(
                                "worktree observation failed for {}: {}",
                                item.location.execution_host_id,
                                error.message
                            );
                            None
                        }
                        crate::execution_host::ObservationStatus::Pending
                        | crate::execution_host::ObservationStatus::Stale(_) => None,
                    })
                    .and_then(|worktrees| {
                        worktrees
                            .iter()
                            .filter(|worktree| {
                                item.location
                                    .path
                                    .as_path()
                                    .starts_with(worktree.location.path.as_path())
                            })
                            .max_by_key(|worktree| {
                                worktree.location.path.as_path().components().count()
                            })
                    })
                    .map(|worktree| worktree.location.clone())
                    .unwrap_or_else(|| item.location.clone());
                if let Err(error) = hosts.request_git_status(status_location.clone()) {
                    tracing::warn!(
                        "git observation request failed for {}: {error}",
                        status_location.execution_host_id
                    );
                }
                hosts.git_status(&status_location).and_then(|observation| {
                    match observation.status() {
                        crate::execution_host::ObservationStatus::Ready(status) => {
                            Some(status.clone())
                        }
                        crate::execution_host::ObservationStatus::Failed(error) => {
                            tracing::warn!(
                                "git observation failed for {}: {}",
                                status_location.execution_host_id,
                                error.message
                            );
                            None
                        }
                        crate::execution_host::ObservationStatus::Pending
                        | crate::execution_host::ObservationStatus::Stale(_) => None,
                    }
                })
            });
            remote_results.push(WorkspaceGitStatus {
                workspace_id: item.workspace_id,
                resolved_identity_cwd: item.resolved_identity_cwd,
                cwd_fingerprint: item.cwd_fingerprint,
                branch: snapshot.as_ref().and_then(|status| status.branch.clone()),
                ahead_behind: snapshot.as_ref().and_then(|status| {
                    status
                        .upstream
                        .as_ref()
                        .map(|_| (status.ahead as usize, status.behind as usize))
                }),
                work_summary: None,
            });
        }

        self.git_refresh_in_flight = true;
        let event_tx = self.event_tx.clone();
        let cache = self.git_status_cache.clone();
        std::thread::spawn(move || {
            let mut output = refresh_workspace_git_statuses_with_cache(local_workspaces, &cache);
            output.results.extend(remote_results);
            let _ = event_tx.blocking_send(AppEvent::GitStatusRefreshed {
                results: output.results,
                cache_updates: output.cache_updates,
                repo_summaries: output.repo_summaries,
            });
        });
    }

    pub(crate) fn mark_git_status_refresh_due(&mut self, now: Instant) {
        if self.git_refresh_in_flight {
            self.git_refresh_due_after_in_flight = true;
            return;
        }
        self.last_git_remote_status_refresh = now
            .checked_sub(GIT_REMOTE_STATUS_REFRESH_INTERVAL)
            .unwrap_or(now);
        self.git_refresh_due_after_in_flight = false;
    }

    pub(crate) fn git_refresh_deadline(&self) -> Option<Instant> {
        (!self.git_refresh_in_flight && !self.state.workspaces.is_empty())
            .then_some(self.last_git_remote_status_refresh + GIT_REMOTE_STATUS_REFRESH_INTERVAL)
    }

    pub(crate) fn next_loop_deadline(&self, now: Instant, needs_render: bool) -> Option<Instant> {
        self.next_loop_deadline_with_resize_poll(now, needs_render, true, true)
    }

    pub(crate) fn next_headless_loop_deadline_with_git_refresh(
        &self,
        now: Instant,
        needs_render: bool,
        include_git_refresh: bool,
    ) -> Option<Instant> {
        self.next_loop_deadline_with_resize_poll(now, needs_render, false, include_git_refresh)
    }

    fn next_loop_deadline_with_resize_poll(
        &self,
        now: Instant,
        needs_render: bool,
        include_resize_poll: bool,
        include_git_refresh: bool,
    ) -> Option<Instant> {
        let render_deadline = if needs_render {
            self.last_render_at
                .map(|last_render_at| last_render_at + MIN_RENDER_INTERVAL)
                .filter(|deadline| *deadline > now)
        } else {
            None
        };

        [
            include_resize_poll.then_some(self.next_resize_poll),
            self.config_diagnostic_deadline,
            self.toast_deadline,
            self.copy_feedback_deadline,
            self.state.next_pending_agent_notification_deadline(),
            self.next_animation_tick,
            (!self.state.workspaces.is_empty()).then_some(self.next_command_scan),
            include_git_refresh
                .then(|| self.git_refresh_deadline())
                .flatten(),
            self.execution_hosts_need_poll()
                .then_some(now + Duration::from_millis(50)),
            self.next_auto_update_check,
            self.next_agent_manifest_update_check,
            self.agent_metadata_deadline,
            self.pending_agent_resume_deadline,
            self.session_save_deadline,
            self.selection_autoscroll_deadline,
            self.selection_highlight_clear_deadline,
            render_deadline,
        ]
        .into_iter()
        .flatten()
        .min()
    }
    fn workspace_git_refresh_items(&self) -> Vec<WorkspaceGitRefreshItem> {
        self.state
            .workspaces
            .iter()
            .enumerate()
            .filter_map(|(ws_idx, ws)| {
                let cwd =
                    ws.resolved_identity_cwd_from(&self.state.terminals, &self.terminal_runtimes)?;
                let cwd_fingerprint =
                    ws.git_status_cwds_from(&self.state.terminals, &self.terminal_runtimes);
                let execution_host_id = ws
                    .active_tab()
                    .and_then(|tab| tab.terminal_id(tab.layout.focused()))
                    .and_then(|terminal_id| self.state.terminals.get(terminal_id))
                    .map(|terminal| terminal.location.execution_host_id.clone())
                    .unwrap_or_else(|| ws.default_location.execution_host_id.clone());
                let location = crate::execution_host::ResourceLocation::new(
                    execution_host_id.clone(),
                    crate::execution_host::HostPath::new(cwd.clone()).ok()?,
                );
                let cache_path = if execution_host_id.is_local() {
                    crate::workspace::git_status_cache_key(&cwd).unwrap_or_else(|| cwd.clone())
                } else {
                    cwd.clone()
                };
                let cache_key = crate::execution_host::ResourceLocation::new(
                    execution_host_id,
                    crate::execution_host::HostPath::new(cache_path).ok()?,
                );
                let observed_repo_roots = if location.is_local() {
                    let roots = self
                        .state
                        .observed_git_repos_for_workspace(&self.terminal_runtimes, ws_idx);
                    if roots.is_empty() {
                        crate::app::actions::observed_git_repos_from_cwd(&cwd)
                    } else {
                        roots
                    }
                } else {
                    Vec::new()
                };
                Some(WorkspaceGitRefreshItem {
                    workspace_id: ws.id.clone(),
                    resolved_identity_cwd: cwd,
                    location,
                    cache_key,
                    cwd_fingerprint,
                    observed_repo_roots,
                })
            })
            .collect()
    }

    pub(crate) fn drain_internal_events(&mut self) -> bool {
        let mut had_event = false;
        while let Ok(ev) = self.event_rx.try_recv() {
            had_event = true;
            self.handle_internal_event_with_prefix_sync(ev);
        }
        had_event
    }
}

pub(crate) fn deduplicate_git_refresh_items(
    items: Vec<WorkspaceGitRefreshItem>,
    cache: &HashMap<crate::execution_host::ResourceLocation, GitStatusCacheEntry>,
) -> Vec<WorkspaceGitRefreshJob> {
    let mut indexes = HashMap::<crate::execution_host::ResourceLocation, usize>::new();
    let mut jobs = Vec::<WorkspaceGitRefreshJob>::new();

    for item in items {
        let target = WorkspaceGitRefreshTarget {
            workspace_id: item.workspace_id,
            resolved_identity_cwd: item.resolved_identity_cwd.clone(),
            cwd_fingerprint: item.cwd_fingerprint,
        };
        if let Some(&index) = indexes.get(&item.cache_key) {
            jobs[index].targets.push(target);
            continue;
        }

        let status_cwd = item.cache_key.path.as_path().to_path_buf();
        let cached = cache.get(&item.cache_key).cloned();
        indexes.insert(item.cache_key.clone(), jobs.len());
        jobs.push(WorkspaceGitRefreshJob {
            cache_key: item.cache_key,
            status_cwd,
            cached,
            targets: vec![target],
        });
    }

    jobs
}

pub(crate) fn refresh_workspace_git_statuses_with_cache(
    items: Vec<WorkspaceGitRefreshItem>,
    cache: &HashMap<crate::execution_host::ResourceLocation, GitStatusCacheEntry>,
) -> WorkspaceGitRefreshOutput {
    let mut results = Vec::new();
    let mut cache_updates = Vec::new();
    let mut repo_roots = items
        .iter()
        .flat_map(|item| item.observed_repo_roots.iter().cloned())
        .collect::<Vec<_>>();
    repo_roots.sort();
    repo_roots.dedup();

    for job in deduplicate_git_refresh_items(items, cache) {
        if !job.cache_key.is_local() {
            continue;
        }
        let (snapshot, cache_entry) =
            Workspace::git_status_snapshot_for_cwd_with_cache(&job.status_cwd, job.cached.as_ref());
        if let Some(cache_entry) = cache_entry {
            cache_updates.push((job.cache_key.clone(), cache_entry));
        }
        results.extend(job.targets.into_iter().map(move |target| {
            snapshot.clone().into_workspace_status(
                target.workspace_id,
                target.resolved_identity_cwd,
                target.cwd_fingerprint,
            )
        }));
    }

    let repo_summaries = repo_roots
        .into_iter()
        .filter_map(|root| {
            Workspace::git_work_summary_for_root(&root).map(|summary| (root, summary))
        })
        .collect();

    WorkspaceGitRefreshOutput {
        results,
        cache_updates,
        repo_summaries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state;
    use crate::workspace::Workspace;
    use std::path::PathBuf;

    fn test_app_with_pane() -> (super::super::App, crate::layout::PaneId) {
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        let ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        app.state.workspaces.push(ws);
        app.state.active = Some(0);
        app.state.view.pane_infos.push(crate::layout::PaneInfo {
            id: pane_id,
            rect: ratatui::layout::Rect::new(0, 0, 80, 24),
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
            scrollbar_rect: None,
            is_focused: true,
        });
        (app, pane_id)
    }

    #[test]
    fn interrupted_custom_command_wait_keeps_child_for_retry() {
        let interrupted = std::io::Error::new(std::io::ErrorKind::Interrupted, "test interrupt");

        assert!(retain_custom_command_after_wait(42, Err(interrupted)));
    }

    #[test]
    fn git_refresh_deduplicates_workspaces_with_same_cache_key() {
        let repo =
            std::env::temp_dir().join(format!("omh-git-refresh-dedupe-{}", std::process::id()));
        let nested = repo.join("nested");
        let other = repo.join("other");
        std::fs::create_dir_all(&nested).expect("create nested dir");
        std::fs::create_dir_all(&other).expect("create other dir");
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .arg("init")
            .output()
            .expect("run git init");

        let output = refresh_workspace_git_statuses_with_cache(
            vec![
                WorkspaceGitRefreshItem {
                    workspace_id: "one".into(),
                    resolved_identity_cwd: nested.clone(),
                    location: crate::execution_host::ResourceLocation::local(nested.clone())
                        .expect("local nested location"),
                    cache_key: crate::execution_host::ResourceLocation::local(repo.clone())
                        .expect("local repo location"),
                    cwd_fingerprint: vec![nested.clone()],
                    observed_repo_roots: vec![repo.clone()],
                },
                WorkspaceGitRefreshItem {
                    workspace_id: "two".into(),
                    resolved_identity_cwd: other.clone(),
                    location: crate::execution_host::ResourceLocation::local(other.clone())
                        .expect("local other location"),
                    cache_key: crate::execution_host::ResourceLocation::local(repo.clone())
                        .expect("local repo location"),
                    cwd_fingerprint: vec![other.clone()],
                    observed_repo_roots: vec![repo.clone()],
                },
            ],
            &HashMap::new(),
        );

        assert_eq!(output.cache_updates.len(), 1);
        assert_eq!(
            output.cache_updates[0].0,
            crate::execution_host::ResourceLocation::local(repo.clone())
                .expect("local repo location")
        );
        assert_eq!(output.results.len(), 2);
        assert_eq!(output.results[0].workspace_id, "one");
        assert_eq!(
            output.results[0].resolved_identity_cwd,
            PathBuf::from(&nested)
        );
        assert_eq!(output.results[1].workspace_id, "two");
        assert_eq!(
            output.results[1].resolved_identity_cwd,
            PathBuf::from(&other)
        );

        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn git_refresh_items_include_direct_child_repos_for_non_git_cwd() {
        let parent = std::env::temp_dir().join(format!(
            "omh-git-refresh-child-repos-{}",
            std::process::id()
        ));
        let child = parent.join("child");
        std::fs::create_dir_all(&child).expect("create child repo dir");
        std::process::Command::new("git")
            .arg("-C")
            .arg(&child)
            .arg("init")
            .output()
            .expect("run git init");
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        let mut ws = Workspace::test_new("test");
        ws.identity_cwd = parent.clone();
        ws.default_location = crate::execution_host::ResourceLocation::local(parent.clone())
            .expect("local parent location");
        ws.tabs.clear();
        app.state.workspaces.push(ws);

        let canonical_child = std::fs::canonicalize(&child).expect("canonicalize child repo");
        let items = app.workspace_git_refresh_items();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].observed_repo_roots, vec![canonical_child.clone()]);
        let output = refresh_workspace_git_statuses_with_cache(items, &HashMap::new());
        assert_eq!(output.repo_summaries.len(), 1);
        assert_eq!(output.repo_summaries[0].0, canonical_child);
        let _ = std::fs::remove_dir_all(parent);
    }
    #[test]
    fn git_refresh_items_use_cwd_cache_key_for_non_git_cwd() {
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        let cwd = std::env::temp_dir().join(format!("omh-non-git-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&cwd).expect("create temp cwd");
        let mut ws = Workspace::test_new("test");
        ws.identity_cwd = cwd.clone();
        ws.default_location = crate::execution_host::ResourceLocation::local(cwd.clone())
            .expect("local cwd location");
        ws.tabs.clear();
        app.state.workspaces.push(ws);

        let items = app.workspace_git_refresh_items();

        assert_eq!(items.len(), 1);
        assert!(items[0].cache_key.is_local());
        assert_eq!(items[0].cache_key.path.as_path(), cwd);
        assert_eq!(items[0].cwd_fingerprint, vec![cwd.clone()]);
        let _ = std::fs::remove_dir_all(&cwd);
    }

    #[test]
    fn git_cache_does_not_merge_same_path_on_different_hosts() {
        let path = PathBuf::from("/srv/same-path");
        let local =
            crate::execution_host::ResourceLocation::local(path.clone()).expect("local location");
        let remote = crate::execution_host::ResourceLocation::new(
            crate::execution_host::ExecutionHostId::new("ssh:workbox").expect("remote host id"),
            crate::execution_host::HostPath::new(path.clone()).expect("remote path"),
        );
        let items = [local, remote]
            .into_iter()
            .enumerate()
            .map(|(index, cache_key)| WorkspaceGitRefreshItem {
                workspace_id: format!("workspace-{index}"),
                resolved_identity_cwd: path.clone(),
                location: cache_key.clone(),
                cache_key,
                cwd_fingerprint: vec![path.clone()],
                observed_repo_roots: Vec::new(),
            })
            .collect();

        let jobs = deduplicate_git_refresh_items(items, &HashMap::new());

        assert_eq!(jobs.len(), 2);
        assert_ne!(jobs[0].cache_key, jobs[1].cache_key);
    }

    #[test]
    fn headless_deadline_can_suppress_git_refresh_timer() {
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        app.state.workspaces.push(Workspace::test_new("test"));
        let now = Instant::now();
        app.last_git_remote_status_refresh = now - super::super::GIT_REMOTE_STATUS_REFRESH_INTERVAL;
        app.next_command_scan = now + Duration::from_secs(30);

        assert_eq!(
            app.next_headless_loop_deadline_with_git_refresh(now, false, false),
            Some(app.next_command_scan)
        );
        assert_eq!(
            app.next_headless_loop_deadline_with_git_refresh(now, false, true),
            Some(now)
        );
    }

    #[test]
    fn git_refresh_due_request_survives_in_flight_refresh() {
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        let now = Instant::now();
        app.git_refresh_in_flight = true;

        app.mark_git_status_refresh_due(now);
        assert!(app.git_refresh_due_after_in_flight);

        app.handle_internal_event(crate::events::AppEvent::GitStatusRefreshed {
            results: Vec::new(),
            cache_updates: Vec::new(),
            repo_summaries: Vec::new(),
        });

        assert!(!app.git_refresh_in_flight);
        assert!(!app.git_refresh_due_after_in_flight);
        assert_eq!(app.git_refresh_deadline(), None);

        app.state.workspaces.push(Workspace::test_new("test"));
        let deadline = app
            .git_refresh_deadline()
            .expect("refresh should be due once a workspace exists");
        assert!(deadline <= Instant::now());
    }

    #[test]
    fn tick_selection_autoscroll_stops_when_metrics_unavailable() {
        // Without a runtime, pane_scroll_metrics returns None.
        // Fail-closed: stop autoscroll instead of rescheduling forever.
        let (mut app, pane_id) = test_app_with_pane();
        let now = Instant::now();
        let mut sel = crate::selection::Selection::anchor(pane_id, 0, 0, None);
        // Drag to a different cell so it becomes Dragging
        sel.drag(5, 5, ratatui::layout::Rect::new(0, 0, 80, 24), None);
        app.state.selection = Some(sel);
        app.state.selection_autoscroll = Some(state::SelectionAutoscroll {
            direction: state::SelectionAutoscrollDirection::Down,
            last_mouse_screen_col: 5,
            last_mouse_screen_row: 23,
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
        });
        app.selection_autoscroll_deadline = Some(now);
        app.tick_selection_autoscroll(now);
        // Should stop because no runtime metrics available
        assert!(app.state.selection_autoscroll.is_none());
        assert!(app.selection_autoscroll_deadline.is_none());
    }

    #[test]
    fn tick_selection_autoscroll_stops_when_selection_done() {
        let (mut app, pane_id) = test_app_with_pane();
        let now = Instant::now();
        // Create a selection that is already finished (not in progress)
        let mut sel = crate::selection::Selection::anchor(pane_id, 0, 0, None);
        // Drag to a different cell so it becomes visible, then finish
        sel.drag(5, 5, ratatui::layout::Rect::new(0, 0, 80, 24), None);
        sel.finish(); // now it's Done, not in progress
        app.state.selection = Some(sel);
        app.state.selection_autoscroll = Some(state::SelectionAutoscroll {
            direction: state::SelectionAutoscrollDirection::Down,
            last_mouse_screen_col: 0,
            last_mouse_screen_row: 23,
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
        });
        app.selection_autoscroll_deadline = Some(now);
        app.tick_selection_autoscroll(now);
        assert!(app.state.selection_autoscroll.is_none());
        assert!(app.selection_autoscroll_deadline.is_none());
    }

    #[test]
    fn tick_selection_autoscroll_stops_when_selection_cleared() {
        let (mut app, _pane_id) = test_app_with_pane();
        let now = Instant::now();
        app.state.selection = None;
        app.state.selection_autoscroll = Some(state::SelectionAutoscroll {
            direction: state::SelectionAutoscrollDirection::Down,
            last_mouse_screen_col: 0,
            last_mouse_screen_row: 23,
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
        });
        app.selection_autoscroll_deadline = Some(now);
        app.tick_selection_autoscroll(now);
        assert!(app.state.selection_autoscroll.is_none());
        assert!(app.selection_autoscroll_deadline.is_none());
    }

    #[test]
    fn tick_selection_autoscroll_stops_when_selection_anchored() {
        // Anchored (click, no drag) should not keep the timer running.
        let (mut app, pane_id) = test_app_with_pane();
        let now = Instant::now();
        app.state.selection = Some(crate::selection::Selection::anchor(pane_id, 0, 0, None));
        app.state.selection_autoscroll = Some(state::SelectionAutoscroll {
            direction: state::SelectionAutoscrollDirection::Down,
            last_mouse_screen_col: 0,
            last_mouse_screen_row: 23,
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
        });
        app.selection_autoscroll_deadline = Some(now);
        app.tick_selection_autoscroll(now);
        assert!(app.state.selection_autoscroll.is_none());
        assert!(app.selection_autoscroll_deadline.is_none());
    }

    /// Creates an app with a real TerminalRuntime (no PTY) so scroll_metrics
    /// returns meaningful data. Uses test_with_scrollback_bytes.
    fn test_app_with_runtime(
        cols: u16,
        rows: u16,
        bytes: &[u8],
    ) -> (super::super::App, crate::layout::PaneId) {
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        let mut ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        let runtime =
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(cols, rows, 0, bytes);
        ws.tabs[0].runtimes.insert(pane_id, runtime);
        app.state.workspaces.push(ws);
        app.state.active = Some(0);
        app.state.view.pane_infos.push(crate::layout::PaneInfo {
            id: pane_id,
            rect: ratatui::layout::Rect::new(0, 0, cols, rows),
            inner_rect: ratatui::layout::Rect::new(0, 0, cols, rows),
            scrollbar_rect: None,
            is_focused: true,
        });
        (app, pane_id)
    }

    #[tokio::test]
    async fn tick_selection_autoscroll_stops_at_scrollback_top() {
        // Create a runtime with no scrollback content — we're already at
        // the top (offset_from_bottom == max_offset_from_bottom).
        let (mut app, pane_id) = test_app_with_runtime(80, 24, &[]);
        let now = Instant::now();
        let mut sel = crate::selection::Selection::anchor(pane_id, 5, 5, None);
        sel.drag(0, 0, ratatui::layout::Rect::new(0, 0, 80, 24), None);
        app.state.selection = Some(sel);
        app.state.selection_autoscroll = Some(state::SelectionAutoscroll {
            direction: state::SelectionAutoscrollDirection::Up,
            last_mouse_screen_col: 0,
            last_mouse_screen_row: 0,
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
        });
        app.selection_autoscroll_deadline = Some(now);
        app.tick_selection_autoscroll(now);
        // At scrollback top, can't scroll further up — should stop
        assert!(app.state.selection_autoscroll.is_none());
        assert!(app.selection_autoscroll_deadline.is_none());
    }

    #[tokio::test]
    async fn tick_selection_autoscroll_stops_at_scrollback_bottom() {
        // Create a runtime with no scrollback content — we're already at
        // the bottom (offset_from_bottom == 0).
        let (mut app, pane_id) = test_app_with_runtime(80, 24, &[]);
        let now = Instant::now();
        let mut sel = crate::selection::Selection::anchor(pane_id, 0, 0, None);
        sel.drag(5, 5, ratatui::layout::Rect::new(0, 0, 80, 24), None);
        app.state.selection = Some(sel);
        app.state.selection_autoscroll = Some(state::SelectionAutoscroll {
            direction: state::SelectionAutoscrollDirection::Down,
            last_mouse_screen_col: 5,
            last_mouse_screen_row: 23,
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
        });
        app.selection_autoscroll_deadline = Some(now);
        app.tick_selection_autoscroll(now);
        // At scrollback bottom, can't scroll further down — should stop
        assert!(app.state.selection_autoscroll.is_none());
        assert!(app.selection_autoscroll_deadline.is_none());
    }

    #[test]
    fn immediate_api_method_responds_before_return() {
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        let (respond_to, response_rx) = std::sync::mpsc::channel();
        let changed = app.handle_api_request_message(crate::api::ApiRequestMessage {
            request: crate::api::schema::Request {
                id: "immediate-1".into(),
                method: crate::api::schema::Method::WorkspaceList(
                    crate::api::schema::EmptyParams::default(),
                ),
            },
            respond_to,
            response_written: None,
        });
        let _ = changed;
        let response = response_rx
            .try_recv()
            .expect("immediate methods must respond before handle returns");
        let body: serde_json::Value = serde_json::from_str(&response).expect("json response");
        assert_eq!(body["id"], "immediate-1");
        assert!(
            body.get("result").is_some(),
            "expected success result: {body}"
        );
        assert!(response_rx.try_recv().is_err(), "must respond exactly once");
    }

    #[tokio::test]
    async fn deferred_disposition_attaches_real_responder_once_through_dispatch() {
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![crate::workspace::Workspace::test_new("dispatch-deferred")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = super::super::Mode::Terminal;
        app.state.ensure_test_terminals();

        let host_id = crate::execution_host::ExecutionHostId::new("ssh:dispatch-deferred").unwrap();
        app.execution_hosts
            .as_mut()
            .unwrap()
            .connect_test_host(host_id.clone());

        let (respond_to, response_rx) = std::sync::mpsc::channel();
        let changed = app.handle_api_request_message(crate::api::ApiRequestMessage {
            request: crate::api::schema::Request {
                id: "dispatch-deferred-1".into(),
                method: crate::api::schema::Method::WorkspaceCreate(
                    crate::api::schema::WorkspaceCreateParams {
                        cwd: None,
                        location: Some(crate::api::schema::ResourceLocationParams {
                            execution_host_id: host_id.as_str().to_string(),
                            path: "/srv/dispatch".into(),
                        }),
                        focus: false,
                        label: None,
                        env: Default::default(),
                    },
                ),
            },
            respond_to,
            response_written: None,
        });
        let _ = changed;

        assert!(
            response_rx.try_recv().is_err(),
            "deferred create must not respond before worker completion"
        );
        assert_eq!(
            app.pending_remote_api_responses.len(),
            1,
            "dispatch must insert exactly one pending responder transaction"
        );
        let terminal_id = app
            .pending_remote_api_responses
            .keys()
            .next()
            .expect("pending terminal")
            .clone();

        app.remote_creation_completions
            .push(crate::app::creation::RemoteCreationCompletion {
                terminal_id: terminal_id.clone(),
                result: Err("dispatch path worker failed".into()),
            });
        assert!(app.finish_remote_api_completions());

        let response = response_rx
            .try_recv()
            .expect("failure must deliver through the original dispatch responder");
        let body: serde_json::Value = serde_json::from_str(&response).expect("json");
        assert_eq!(body["id"], "dispatch-deferred-1");
        assert_eq!(body["error"]["code"], "workspace_create_failed");
        assert!(
            body["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("dispatch path worker failed")),
            "unexpected error body: {body}"
        );
        assert!(response_rx.try_recv().is_err(), "must respond exactly once");
        assert!(
            !app.pending_remote_api_responses.contains_key(&terminal_id),
            "pending responder must clear after failure completion"
        );
    }

    #[test]
    fn deferred_remote_create_responds_once_on_failure_completion() {
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        let terminal_id = crate::terminal::TerminalId::alloc();
        let (respond_to, response_rx) = std::sync::mpsc::channel();
        app.store_pending_remote_api_response(
            terminal_id.clone(),
            crate::app::PendingRemoteApiResponse {
                request_id: "remote-fail".into(),
                kind: crate::app::PendingRemoteApiKind::WorkspaceCreate { label: None },
                respond_to,
                focus: false,
                client_view_id: None,
                pending_focus: None,
            },
        );

        // No completion yet — API client must still be waiting.
        assert!(
            response_rx.try_recv().is_err(),
            "deferred create must not respond before worker ACK/failure"
        );

        app.remote_creation_completions
            .push(crate::app::creation::RemoteCreationCompletion {
                terminal_id: terminal_id.clone(),
                result: Err("worker refused create".into()),
            });
        assert!(app.finish_remote_api_completions());

        let response = response_rx
            .try_recv()
            .expect("failure completion must deliver exactly one API error");
        let body: serde_json::Value = serde_json::from_str(&response).expect("json response");
        assert_eq!(body["id"], "remote-fail");
        assert_eq!(body["error"]["code"], "workspace_create_failed");
        assert!(
            body["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("worker refused create")),
            "unexpected error body: {body}"
        );
        assert!(response_rx.try_recv().is_err(), "must respond exactly once");
        assert!(
            !app.pending_remote_api_responses.contains_key(&terminal_id),
            "pending responder must be cleared after completion"
        );
    }

    #[test]
    fn deferred_remote_create_sender_disconnect_does_not_block_completion_drain() {
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        let terminal_id = crate::terminal::TerminalId::alloc();
        let (respond_to, response_rx) = std::sync::mpsc::channel();
        app.store_pending_remote_api_response(
            terminal_id.clone(),
            crate::app::PendingRemoteApiResponse {
                request_id: "remote-drop".into(),
                kind: crate::app::PendingRemoteApiKind::PaneSplit,
                respond_to,
                focus: false,
                client_view_id: None,
                pending_focus: None,
            },
        );
        drop(response_rx);

        app.remote_creation_completions
            .push(crate::app::creation::RemoteCreationCompletion {
                terminal_id: terminal_id.clone(),
                result: Err("transport lost after disconnect".into()),
            });
        // Disconnected sender must not panic or leave the pending map stuck.
        assert!(app.finish_remote_api_completions());
        assert!(!app.pending_remote_api_responses.contains_key(&terminal_id));
    }

    #[test]
    fn deferred_remote_create_success_responds_once_after_commit() {
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        let ws = crate::workspace::Workspace::test_new("deferred-ok");
        let root_pane = ws.tabs[0].root_pane;
        let attached_terminal_id = ws.tabs[0].panes[&root_pane].attached_terminal_id.clone();
        app.state.terminals.insert(
            attached_terminal_id.clone(),
            crate::terminal::TerminalState::new(
                attached_terminal_id,
                std::env::current_dir().unwrap_or_else(|_| "/".into()),
            ),
        );
        app.state.workspaces.push(ws);
        app.state.active = Some(0);

        let terminal_id = crate::terminal::TerminalId::alloc();
        let (respond_to, response_rx) = std::sync::mpsc::channel();
        app.store_pending_remote_api_response(
            terminal_id.clone(),
            crate::app::PendingRemoteApiResponse {
                request_id: "remote-ok".into(),
                kind: crate::app::PendingRemoteApiKind::WorkspaceCreate {
                    label: Some("labeled".into()),
                },
                respond_to,
                focus: false,
                client_view_id: None,
                pending_focus: None,
            },
        );
        assert!(response_rx.try_recv().is_err());

        app.remote_creation_completions
            .push(crate::app::creation::RemoteCreationCompletion {
                terminal_id: terminal_id.clone(),
                result: Ok(crate::app::creation::CommittedRemoteCreation::Workspace { ws_idx: 0 }),
            });
        assert!(app.finish_remote_api_completions());

        let response = response_rx
            .try_recv()
            .expect("success completion must deliver exactly one API success");
        let body: serde_json::Value = serde_json::from_str(&response).expect("json response");
        assert_eq!(body["id"], "remote-ok");
        assert!(
            body.get("result").is_some() && body.get("error").is_none(),
            "expected success body, got {body}"
        );
        assert!(
            body["result"].get("workspace").is_some(),
            "expected workspace create result, got {body}"
        );
        assert!(response_rx.try_recv().is_err(), "must respond exactly once");
        assert!(!app.pending_remote_api_responses.contains_key(&terminal_id));
    }
}

#[cfg(test)]
mod release_forwarding_tests {
    use super::super::{App, ClientViewState, Mode};
    use crate::{
        input::TerminalKey, raw_input::RawInputEvent, terminal::TerminalRuntime,
        workspace::Workspace,
    };
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers, ModifierKeyCode};

    fn app_with_two_input_channels(
        report_events: bool,
    ) -> (
        App,
        crate::layout::PaneId,
        crate::layout::PaneId,
        tokio::sync::mpsc::Receiver<bytes::Bytes>,
        tokio::sync::mpsc::Receiver<bytes::Bytes>,
    ) {
        let mut app = App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        let mut workspace = Workspace::test_new("test");
        let pane_a = workspace.focused_pane_id().expect("focused pane");
        let pane_b = workspace.tabs[0]
            .layout
            .split_focused(ratatui::layout::Direction::Horizontal);
        let make_runtime = || {
            if report_events {
                TerminalRuntime::test_with_channel_and_scrollback_bytes(80, 24, 0, b"\x1b[>10u", 8)
            } else {
                TerminalRuntime::test_with_channel(80, 24)
            }
        };
        let (runtime_a, rx_a) = make_runtime();
        let (runtime_b, rx_b) = make_runtime();
        workspace.insert_test_runtime(pane_a, runtime_a);
        workspace.insert_test_runtime(pane_b, runtime_b);
        workspace.tabs[0].layout.focus_pane(pane_a);
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        (app, pane_a, pane_b, rx_a, rx_b)
    }

    fn app_with_input_channel(
        report_events: bool,
    ) -> (App, tokio::sync::mpsc::Receiver<bytes::Bytes>) {
        let (app, _, _, rx, _) = app_with_two_input_channels(report_events);
        (app, rx)
    }

    fn key(kind: KeyEventKind) -> TerminalKey {
        TerminalKey::new(KeyCode::Char('a'), KeyModifiers::empty()).with_kind(kind)
    }

    fn assert_kitty_stream_stays_on_first_pane(
        mut rx_a: tokio::sync::mpsc::Receiver<bytes::Bytes>,
        mut rx_b: tokio::sync::mpsc::Receiver<bytes::Bytes>,
    ) {
        assert_eq!(
            rx_a.try_recv().expect("Kitty press"),
            bytes::Bytes::from_static(b"\x1b[97;1:1u")
        );
        assert_eq!(
            rx_a.try_recv().expect("Kitty repeat"),
            bytes::Bytes::from_static(b"\x1b[97;1:2u")
        );
        assert_eq!(
            rx_a.try_recv().expect("Kitty release"),
            bytes::Bytes::from_static(b"\x1b[97;1:3u")
        );
        assert!(rx_a.try_recv().is_err());
        assert!(
            rx_b.try_recv().is_err(),
            "newly focused pane received key bytes"
        );
    }

    #[tokio::test]
    async fn monolithic_dispatch_keeps_key_stream_on_press_pane() {
        let (mut app, pane_a, pane_b, rx_a, rx_b) = app_with_two_input_channels(true);
        app.handle_raw_input_event(RawInputEvent::Key(key(KeyEventKind::Press)))
            .await;
        app.state.workspaces[0].tabs[0].layout.focus_pane(pane_b);
        app.handle_raw_input_event(RawInputEvent::Key(key(KeyEventKind::Repeat)))
            .await;
        app.handle_raw_input_event(RawInputEvent::Key(key(KeyEventKind::Release)))
            .await;
        assert_ne!(pane_a, pane_b);
        assert_kitty_stream_stays_on_first_pane(rx_a, rx_b);
    }

    #[tokio::test]
    async fn monolithic_dispatch_resolves_press_workspace_after_reorder() {
        let (mut app, _, _, mut rx_a, mut rx_other_pane) = app_with_two_input_channels(true);
        app.handle_raw_input_event(RawInputEvent::Key(key(KeyEventKind::Press)))
            .await;

        let mut replacement = Workspace::test_new("replacement");
        let replacement_pane = replacement.focused_pane_id().expect("replacement pane");
        let (replacement_runtime, mut replacement_rx) =
            TerminalRuntime::test_with_channel_and_scrollback_bytes(80, 24, 0, b"\x1b[>10u", 8);
        replacement.insert_test_runtime(replacement_pane, replacement_runtime);
        app.state.workspaces.insert(0, replacement);

        app.handle_raw_input_event(RawInputEvent::Key(key(KeyEventKind::Repeat)))
            .await;
        app.handle_raw_input_event(RawInputEvent::Key(key(KeyEventKind::Release)))
            .await;

        assert_eq!(
            rx_a.try_recv().expect("Kitty press"),
            bytes::Bytes::from_static(b"\x1b[97;1:1u")
        );
        assert_eq!(
            rx_a.try_recv().expect("Kitty repeat after reorder"),
            bytes::Bytes::from_static(b"\x1b[97;1:2u")
        );
        assert_eq!(
            rx_a.try_recv().expect("Kitty release after reorder"),
            bytes::Bytes::from_static(b"\x1b[97;1:3u")
        );
        assert!(rx_a.try_recv().is_err());
        assert!(replacement_rx.try_recv().is_err());
        assert!(rx_other_pane.try_recv().is_err());
    }

    #[tokio::test]
    async fn default_client_dispatch_keeps_key_stream_on_press_pane() {
        let (mut app, pane_a, pane_b, rx_a, rx_b) = app_with_two_input_channels(true);
        app.route_client_events(vec![RawInputEvent::Key(key(KeyEventKind::Press))], false);
        app.state.workspaces[0].tabs[0].layout.focus_pane(pane_b);
        app.route_client_events(
            vec![
                RawInputEvent::Key(key(KeyEventKind::Repeat)),
                RawInputEvent::Key(key(KeyEventKind::Release)),
            ],
            false,
        );
        assert_ne!(pane_a, pane_b);
        assert_kitty_stream_stays_on_first_pane(rx_a, rx_b);
    }

    #[tokio::test]
    async fn explicit_client_dispatch_keeps_key_stream_on_press_pane() {
        let (mut app, pane_a, pane_b, rx_a, rx_b) = app_with_two_input_channels(true);
        let mut client = ClientViewState::from_default_client_state(&app.state);
        app.route_client_events_for_view(
            &mut client,
            vec![RawInputEvent::Key(key(KeyEventKind::Press))],
            false,
        );
        client.focus_pane_in_workspace(&app.state, 0, 0, pane_b);
        app.route_client_events_for_view(
            &mut client,
            vec![
                RawInputEvent::Key(key(KeyEventKind::Repeat)),
                RawInputEvent::Key(key(KeyEventKind::Release)),
            ],
            false,
        );
        assert_ne!(pane_a, pane_b);
        assert_kitty_stream_stays_on_first_pane(rx_a, rx_b);
    }

    #[tokio::test]
    async fn modifier_drift_release_uses_press_target_and_release_modifiers() {
        let (mut app, pane_a, pane_b, mut rx_a, mut rx_b) = app_with_two_input_channels(true);
        app.route_client_events(
            vec![RawInputEvent::Key(
                TerminalKey::new(KeyCode::Char('A'), KeyModifiers::CONTROL)
                    .with_kind(KeyEventKind::Press),
            )],
            false,
        );
        app.state.workspaces[0].tabs[0].layout.focus_pane(pane_b);
        app.route_client_events(
            vec![RawInputEvent::Key(
                TerminalKey::new(
                    KeyCode::Modifier(ModifierKeyCode::LeftControl),
                    KeyModifiers::empty(),
                )
                .with_kind(KeyEventKind::Release),
            )],
            false,
        );
        app.route_client_events(
            vec![RawInputEvent::Key(
                TerminalKey::new(KeyCode::Char('a'), KeyModifiers::empty())
                    .with_kind(KeyEventKind::Release),
            )],
            false,
        );
        assert_ne!(pane_a, pane_b);
        assert_eq!(
            rx_a.try_recv().expect("modified Kitty press"),
            bytes::Bytes::from_static(b"\x1b[65;5:1u")
        );
        assert_eq!(
            rx_a.try_recv().expect("unmodified Kitty release"),
            bytes::Bytes::from_static(b"\x1b[97;1:3u")
        );
        assert!(rx_a.try_recv().is_err());
        assert!(rx_b.try_recv().is_err());
        assert!(app.forwarded_terminal_keys.is_empty());
    }

    #[tokio::test]
    async fn monolithic_legacy_dispatch_forwards_only_press_bytes() {
        let (mut app, mut rx) = app_with_input_channel(false);
        app.handle_raw_input_event(RawInputEvent::Key(key(KeyEventKind::Press)))
            .await;
        app.handle_raw_input_event(RawInputEvent::Key(key(KeyEventKind::Release)))
            .await;
        assert_eq!(
            rx.try_recv().expect("legacy press"),
            bytes::Bytes::from_static(b"a")
        );
        assert!(
            rx.try_recv().is_err(),
            "legacy release must encode no bytes"
        );
        assert!(app.forwarded_terminal_keys.is_empty());
    }

    #[tokio::test]
    async fn default_client_reporting_kitty_dispatch_forwards_press_and_release() {
        let (mut app, mut rx) = app_with_input_channel(true);
        app.route_client_events(
            vec![
                RawInputEvent::Key(key(KeyEventKind::Press)),
                RawInputEvent::Key(key(KeyEventKind::Release)),
            ],
            false,
        );
        assert_eq!(
            rx.try_recv().expect("Kitty press"),
            bytes::Bytes::from_static(b"\x1b[97;1:1u")
        );
        assert_eq!(
            rx.try_recv().expect("Kitty release"),
            bytes::Bytes::from_static(b"\x1b[97;1:3u")
        );
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn explicit_client_does_not_forward_release_for_intercepted_prefix_press() {
        let (mut app, mut rx) = app_with_input_channel(true);
        app.state.prefix_code = KeyCode::Char('b');
        app.state.prefix_mods = KeyModifiers::CONTROL;
        let mut client_view = ClientViewState::from_default_client_state(&app.state);
        let prefix =
            |kind| TerminalKey::new(KeyCode::Char('b'), KeyModifiers::CONTROL).with_kind(kind);
        app.route_client_events_for_view(
            &mut client_view,
            vec![
                RawInputEvent::Key(prefix(KeyEventKind::Press)),
                RawInputEvent::Key(prefix(KeyEventKind::Release)),
            ],
            false,
        );
        assert_eq!(client_view.mode, Mode::Prefix);
        assert!(
            rx.try_recv().is_err(),
            "intercepted press must not acquire a pane release"
        );
    }
}
