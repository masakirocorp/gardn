use super::{state, view_state::GithubHost, App, ClientViewState, Mode};
use crate::events::AppEvent;
impl App {
    pub(super) fn handle_github_mouse_for_view(
        &mut self,
        view: &mut ClientViewState,
        mouse: crossterm::event::MouseEvent,
    ) -> bool {
        let inside = view
            .github_content_rect(&self.state)
            .contains((mouse.column, mouse.row).into());
        if !inside {
            if let Some(screen) = view.github.as_mut() {
                screen.handle_mouse(mouse);
            }
        }
        if matches!(view.mode, Mode::Terminal | Mode::Github)
            && view.popup_pane.is_none()
            && view.context_menu.is_none()
            && view.github.is_some()
            && inside
        {
            if matches!(mouse.kind, crossterm::event::MouseEventKind::Down(_)) {
                if let Some(host) = view.github_host.as_ref() {
                    if let Some((ws_idx, tab_idx)) = self
                        .state
                        .workspaces
                        .iter()
                        .enumerate()
                        .find_map(|(ws_idx, workspace)| {
                            (workspace.id == host.key.workspace_id).then(|| {
                                workspace
                                    .tabs
                                    .iter()
                                    .position(|tab| tab.number() == host.key.tab_number)
                                    .map(|tab_idx| (ws_idx, tab_idx))
                            })?
                        })
                    {
                        view.focus_tab_in_workspace(&self.state, ws_idx, tab_idx);
                        view.mode = Mode::Github;
                    }
                }
            }
            let effects = view
                .github
                .as_mut()
                .map(|screen| screen.handle_mouse(mouse))
                .unwrap_or_default();
            self.apply_github_effects(view, effects);
            return true;
        }
        if let Some(screen) = view.github.as_mut() {
            screen.clear_hover();
        }
        false
    }

    pub(crate) fn open_default_github(&mut self, workspace: Option<usize>) {
        self.with_default_github_view(|app, view| {
            view.active_workspace = workspace.or(app.state.active);
            app.open_github_for_view(view);
        });
    }

    pub(crate) fn release_github_for_view(&mut self, view: &mut ClientViewState) {
        if let Some(mut screen) = view.github.take() {
            for id in screen
                .pending_requests()
                .into_iter()
                .chain(screen.cancel_requests())
            {
                self.github_runtime.cancel(id);
            }
        }
        view.github_host.take();
        if view.mode == Mode::Github {
            view.return_to_active_workspace_mode();
        }
        view.reconcile(&self.state);
    }

    pub(crate) fn close_github_for_view(&mut self, view: &mut ClientViewState) {
        let host = view.github_host.clone();
        self.release_github_for_view(view);
        let Some(host) = host else {
            return;
        };
        let Some(ws_idx) = self
            .state
            .workspaces
            .iter()
            .position(|workspace| workspace.id == host.key.workspace_id)
        else {
            return;
        };
        let Some(tab_idx) = self.state.workspaces[ws_idx]
            .tabs
            .iter()
            .position(|tab| tab.number() == host.key.tab_number && tab.is_github())
        else {
            return;
        };
        if !self.state.close_workspace_tab(ws_idx, tab_idx) {
            return;
        }
        if let Some(source_focus) = host.source_focus {
            if let Some(ws_idx) = self
                .state
                .workspaces
                .iter()
                .position(|workspace| workspace.id == source_focus.workspace_id)
            {
                if let Some(tab_idx) =
                    self.state.workspaces[ws_idx].find_tab_index_for_pane(source_focus.pane_id)
                {
                    view.focus_pane_in_workspace(
                        &self.state,
                        ws_idx,
                        tab_idx,
                        source_focus.pane_id,
                    );
                }
            }
        }
    }

    pub(crate) fn open_github_for_view(&mut self, view: &mut ClientViewState) {
        let Some(ws_idx) = view.active_workspace else {
            return;
        };
        let workspace_id = self.state.workspaces[ws_idx].id.clone();
        let source_focus = view.current_pane_focus_target(&self.state).or_else(|| {
            view.github_host
                .as_ref()
                .filter(|host| host.key.workspace_id == workspace_id)
                .and_then(|host| host.source_focus.clone())
        });
        let scope = match self
            .state
            .resolved_github_scope(&self.terminal_runtimes, ws_idx)
        {
            Ok(scope) => scope,
            Err(error) => {
                self.state.toast = Some(state::ToastNotification {
                    kind: state::ToastKind::NeedsAttention,
                    title: "Cannot open GitHub".into(),
                    context: error,
                    position: None,
                    target: None,
                });
                return;
            }
        };
        let scope_settings = {
            let workspace = &self.state.workspaces[ws_idx];
            (
                workspace.github_scope.clone(),
                self.state
                    .groups
                    .iter()
                    .find(|group| group.id == workspace.group_id)
                    .and_then(|group| group.github_organization.clone()),
            )
        };
        self.release_github_for_view(view);
        let tab_idx = self.state.workspaces[ws_idx].ensure_github_tab();
        let tab_number = self.state.workspaces[ws_idx].tabs[tab_idx].number();
        let key = crate::app::view_state::ClientTabViewKey::new(&workspace_id, tab_number);
        view.github_host = Some(GithubHost {
            key,
            source_focus,
            scope_settings,
        });
        view.github = Some(crate::github::screen::GithubScreen::new(scope));
        view.focus_tab_in_workspace(&self.state, ws_idx, tab_idx);
        view.mode = Mode::Github;
        self.pump_github_for_view(view);
    }

    pub(crate) fn poll_github(&mut self) -> bool {
        self.github_runtime.tick()
    }
    pub(crate) fn github_has_pending(&self) -> bool {
        self.github_runtime.has_pending()
    }

    pub(crate) fn pump_github_for_view(&mut self, view: &mut ClientViewState) -> bool {
        let focused_github = view.active_workspace.and_then(|ws_idx| {
            let tab_idx = view.active_tab_index_for_workspace(&self.state, ws_idx)?;
            let workspace = self.state.workspaces.get(ws_idx)?;
            let tab = workspace.tabs.get(tab_idx)?;
            tab.is_github().then(|| {
                (
                    ws_idx,
                    tab_idx,
                    crate::app::view_state::ClientTabViewKey::new(&workspace.id, tab.number()),
                )
            })
        });
        let hosted_tab_exists = view.github_host.as_ref().is_some_and(|host| {
            self.state.workspaces.iter().any(|workspace| {
                workspace.id == host.key.workspace_id
                    && workspace
                        .tabs
                        .iter()
                        .any(|tab| tab.number() == host.key.tab_number && tab.is_github())
            })
        });
        let focused_host_differs = match (&view.github_host, &focused_github) {
            (Some(host), Some((_, _, key))) => host.key != *key,
            (Some(_), None) => !hosted_tab_exists,
            _ => false,
        };
        if focused_host_differs {
            self.release_github_for_view(view);
        }
        let mut changed = focused_host_differs;
        if view.github_host.is_none() {
            let Some((ws_idx, tab_idx, key)) = focused_github else {
                return changed;
            };
            let scope = match self
                .state
                .resolved_github_scope(&self.terminal_runtimes, ws_idx)
            {
                Ok(scope) => scope,
                Err(_) => return changed,
            };
            let scope_settings = {
                let workspace = &self.state.workspaces[ws_idx];
                (
                    workspace.github_scope.clone(),
                    self.state
                        .groups
                        .iter()
                        .find(|group| group.id == workspace.group_id)
                        .and_then(|group| group.github_organization.clone()),
                )
            };
            view.github_host = Some(GithubHost {
                key,
                source_focus: None,
                scope_settings,
            });
            view.github = Some(crate::github::screen::GithubScreen::new(scope));
            let previous_mode = view.mode;
            view.focus_tab_in_workspace(&self.state, ws_idx, tab_idx);
            if matches!(previous_mode, Mode::Terminal | Mode::Github) {
                view.mode = Mode::Github;
            }
            changed = true;
        }
        let Some(host_snapshot) = view.github_host.clone() else {
            return changed;
        };
        let Some(ws_idx) = self
            .state
            .workspaces
            .iter()
            .position(|workspace| workspace.id == host_snapshot.key.workspace_id)
        else {
            self.release_github_for_view(view);
            return true;
        };
        let workspace = &self.state.workspaces[ws_idx];
        let organization = self
            .state
            .groups
            .iter()
            .find(|group| group.id == workspace.group_id)
            .and_then(|group| group.github_organization.as_ref());
        if host_snapshot.scope_settings.0 != workspace.github_scope
            || host_snapshot.scope_settings.1.as_ref() != organization
        {
            self.release_github_for_view(view);
            return true;
        }
        let Some(tab_idx) = workspace
            .tabs
            .iter()
            .position(|tab| tab.number() == host_snapshot.key.tab_number && tab.is_github())
        else {
            self.release_github_for_view(view);
            return true;
        };
        if view.github.is_none() {
            let scope = match self
                .state
                .resolved_github_scope(&self.terminal_runtimes, ws_idx)
            {
                Ok(scope) => scope,
                Err(_) => return changed,
            };
            view.github = Some(crate::github::screen::GithubScreen::new(scope));
            changed = true;
        }
        if view.active_workspace == Some(ws_idx)
            && view.active_tab_index_for_workspace(&self.state, ws_idx) == Some(tab_idx)
            && matches!(view.mode, Mode::Terminal | Mode::Github)
        {
            view.mode = Mode::Github;
        }
        let Some(screen) = view.github.as_mut() else {
            return changed;
        };
        for id in screen.cancel_requests() {
            self.github_runtime.cancel(id);
        }
        for id in screen.pending_requests() {
            if let Some(result) = self.github_runtime.take(id) {
                screen.apply(id, result);
                changed = true;
            }
        }
        let force_refresh = screen.take_force_refresh();
        for request in screen.drain_requests() {
            let id = if force_refresh {
                self.github_runtime
                    .submit_refresh(screen.scope.clone(), request)
            } else {
                self.github_runtime.submit(screen.scope.clone(), request)
            };
            screen.track_request(id);
            changed = true;
        }
        changed
    }

    pub(super) fn apply_github_effects(
        &mut self,
        view: &mut ClientViewState,
        effects: Vec<crate::github::screen::GithubEffect>,
    ) {
        for effect in effects {
            match effect {
                crate::github::screen::GithubEffect::Close => self.close_github_for_view(view),
                crate::github::screen::GithubEffect::OpenPalette => {
                    view.command_palette.query.clear();
                    view.command_palette.list.select(0);
                    view.command_palette.scroll = 0;
                    view.mode = Mode::CommandPalette;
                }
                crate::github::screen::GithubEffect::OpenUrl(url) => {
                    if let Err(error) = self.event_tx.try_send(AppEvent::ClientOpenUrl {
                        view_id: view.id(),
                        url,
                    }) {
                        tracing::warn!(%error, "failed to queue GitHub browser action");
                    }
                }
                crate::github::screen::GithubEffect::Copy(text) => {
                    if let Err(error) = self.event_tx.try_send(AppEvent::ClientClipboardWrite {
                        view_id: view.id(),
                        content: text.into_bytes(),
                    }) {
                        tracing::warn!(%error, "failed to queue GitHub clipboard action");
                    }
                }
                crate::github::screen::GithubEffect::OpenEditor => {
                    if let Some(ws_idx) = view.github_host.as_ref().and_then(|host| {
                        self.state
                            .workspaces
                            .iter()
                            .position(|workspace| workspace.id == host.key.workspace_id)
                    }) {
                        self.release_github_for_view(view);
                        view.active_workspace = Some(ws_idx);
                        self.execute_client_view_command_palette_action(
                            view,
                            crate::app::command_palette::CommandPaletteAction::OpenEditor,
                        );
                    }
                }
            }
        }
        self.pump_github_for_view(view);
    }

    pub(super) fn with_default_github_view<R>(
        &mut self,
        action: impl FnOnce(&mut Self, &mut ClientViewState) -> R,
    ) -> R {
        let replacement = ClientViewState::from_default_client_state(&self.state);
        let mut view = std::mem::replace(&mut self.default_client_view, replacement);
        let previous_host_key = view.github_host.as_ref().map(|host| host.key.clone());
        let had_screen = view.github.is_some();
        view.active_tabs = self
            .state
            .workspaces
            .iter()
            .map(|workspace| (workspace.id.clone(), workspace.active_tab))
            .collect();
        view.focused_panes = self
            .state
            .workspaces
            .iter()
            .flat_map(|workspace| {
                workspace.terminal_tabs().map(|(_, tab)| {
                    (
                        super::view_state::ClientTabViewKey::new(&workspace.id, tab.number),
                        tab.layout.focused(),
                    )
                })
            })
            .collect();
        view.tab_canvas_view = None;
        view.active_workspace = self.state.active;
        view.mode = self.state.mode;
        view.computed = self.state.view.clone();
        view.command_palette = self.state.command_palette.clone();
        view.sync_github_mode(&self.state);
        view.compute_github(&self.state);
        let result = action(self, &mut view);
        if let Some(key) = view.current_tab_key(&self.state) {
            if let Some(ws_idx) = self
                .state
                .workspaces
                .iter()
                .position(|workspace| workspace.id == key.workspace_id)
            {
                if let Some(tab_idx) = self.state.workspaces[ws_idx]
                    .tabs
                    .iter()
                    .position(|tab| tab.number() == key.tab_number)
                {
                    if self.state.active != Some(ws_idx) {
                        self.state.switch_workspace(ws_idx);
                    }
                    if self.state.workspaces[ws_idx].active_tab != tab_idx {
                        self.state.switch_tab(tab_idx);
                    }
                }
            }
        }
        let restored_source = previous_host_key.is_some() && view.github_host.is_none();
        let opened_screen = view.github.is_some()
            && (!had_screen
                || previous_host_key != view.github_host.as_ref().map(|host| host.key.clone()));
        if opened_screen || restored_source {
            if let Some(target) = view.current_pane_focus_target(&self.state) {
                if let Some(ws_idx) = self
                    .state
                    .workspaces
                    .iter()
                    .position(|workspace| workspace.id == target.workspace_id)
                {
                    if let Some(tab_idx) =
                        workspace_tab_for_pane(&self.state.workspaces[ws_idx], target.pane_id)
                    {
                        self.state.switch_workspace(ws_idx);
                        self.state.switch_tab(tab_idx);
                        self.state.focus_pane(target.pane_id);
                    }
                }
            }
        }
        self.state.mode = view.mode;
        self.state.command_palette = view.command_palette.clone();
        self.default_client_view = view;
        result
    }
}

fn workspace_tab_for_pane(
    workspace: &crate::workspace::Workspace,
    pane_id: crate::layout::PaneId,
) -> Option<usize> {
    workspace.find_tab_index_for_pane(pane_id)
}
