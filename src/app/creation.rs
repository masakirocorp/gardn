use std::path::PathBuf;

use tracing::error;

use std::collections::HashSet;

use super::{
    api_helpers::{pane_agent_status, tab_attention_priority},
    App, Mode,
};
use crate::{
    config::NewTerminalCwdConfig,
    workspace::{derive_label_from_cwd, Workspace},
};

pub(crate) fn resolve_new_terminal_cwd(
    policy: &NewTerminalCwdConfig,
    follow_cwd: Option<PathBuf>,
) -> PathBuf {
    match policy {
        NewTerminalCwdConfig::Follow => follow_cwd
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/")),
        NewTerminalCwdConfig::Home => std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/")),
        NewTerminalCwdConfig::Current => {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
        }
        NewTerminalCwdConfig::Path(path) => expand_new_terminal_cwd_path(path),
    }
}

fn expand_new_terminal_cwd_path(path: &str) -> PathBuf {
    if path == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

impl App {
    pub(super) fn collision_free_workspace_name(
        &self,
        initial_cwd: &std::path::Path,
        group_id: &str,
    ) -> Option<String> {
        let base = derive_label_from_cwd(initial_cwd);
        let names: HashSet<_> = self
            .state
            .workspaces
            .iter()
            .filter(|ws| ws.group_id == group_id)
            .map(|ws| ws.display_name())
            .collect();

        if !names.contains(&base) {
            return None;
        }

        (2..)
            .map(|suffix| format!("{base} {suffix}"))
            .find(|candidate| !names.contains(candidate))
    }

    pub(super) fn seed_cwd_from_workspace(&self, ws_idx: usize) -> Option<std::path::PathBuf> {
        self.state.workspaces.get(ws_idx).map(|workspace| {
            workspace.effective_default_cwd_from(&self.state.terminals, &self.terminal_runtimes)
        })
    }

    pub(super) fn resolve_new_terminal_cwd(&self, follow_cwd: Option<PathBuf>) -> PathBuf {
        resolve_new_terminal_cwd(&self.state.new_terminal_cwd, follow_cwd)
    }

    pub(super) fn workspace_creation_source(&self) -> Option<usize> {
        if self.state.mode == Mode::Navigate
            && self.state.workspaces.get(self.state.selected).is_some()
            && self.state.workspace_in_active_group(self.state.selected)
        {
            return Some(self.state.selected);
        }

        self.state
            .active
            .filter(|idx| self.state.workspace_in_active_group(*idx))
            .or_else(|| {
                self.state
                    .workspaces
                    .get(self.state.selected)
                    .filter(|_| self.state.workspace_in_active_group(self.state.selected))
                    .map(|_| self.state.selected)
            })
    }

    pub(super) fn workspace_creation_group_id(&self, source: Option<usize>) -> String {
        source
            .and_then(|ws_idx| self.state.workspaces.get(ws_idx))
            .map(|ws| ws.group_id.clone())
            .unwrap_or_else(|| self.state.active_group_id().to_string())
    }

    /// Create a workspace with a real PTY (needs event_tx).
    pub(crate) fn create_workspace(&mut self) {
        let source = self.workspace_creation_source();
        let group_id = self.workspace_creation_group_id(source);
        let follow_cwd = source.and_then(|ws_idx| self.seed_cwd_from_workspace(ws_idx));
        let initial_cwd = self.resolve_new_terminal_cwd(follow_cwd);
        if let Err(e) = self.create_workspace_with_options_in_group(initial_cwd, true, group_id) {
            error!(err = %e, "failed to create workspace");
            self.state.mode = Mode::Navigate;
        }
    }

    pub(crate) fn create_tab(&mut self) {
        let custom_name = self.state.requested_new_tab_name.take();
        let follow_cwd = self
            .state
            .active
            .and_then(|ws_idx| self.seed_cwd_from_workspace(ws_idx));
        let initial_cwd = self.resolve_new_terminal_cwd(follow_cwd);
        match self.create_tab_with_options(initial_cwd, true) {
            Ok(tab_idx) => {
                if let Some(name) = custom_name {
                    if let Some(ws) = self
                        .state
                        .active
                        .and_then(|ws_idx| self.state.workspaces.get_mut(ws_idx))
                    {
                        if let Some(tab) = ws.tabs.get_mut(tab_idx) {
                            tab.set_custom_name(name);
                        }
                        self.schedule_session_save();
                    }
                }
            }
            Err(e) => {
                error!(err = %e, "failed to create tab");
            }
        }
    }

    pub(super) fn create_tab_with_options(
        &mut self,
        initial_cwd: PathBuf,
        focus: bool,
    ) -> std::io::Result<usize> {
        let Some(ws_idx) = self.state.active else {
            return self.create_workspace_with_options(initial_cwd, focus);
        };
        let (rows, cols) = self.state.estimate_pane_size();
        let scrollback_limit_bytes = self.state.pane_scrollback_limit_bytes;
        let host_terminal_theme = self.state.host_terminal_theme;
        let default_shell = self.state.default_shell.clone();
        let shell_mode = self.state.shell_mode;
        let event_tx = self.event_tx.clone();
        let render_notify = self.render_notify.clone();
        let render_dirty = self.render_dirty.clone();
        let (idx, terminal, runtime, root_pane) = {
            let ws = &mut self.state.workspaces[ws_idx];
            let (idx, terminal, runtime) = ws.create_tab_with_handles(
                rows,
                cols,
                initial_cwd,
                scrollback_limit_bytes,
                host_terminal_theme,
                crate::pane::PaneShellConfig::new(&default_shell, shell_mode),
                event_tx,
                render_notify,
                render_dirty,
            )?;
            let root_pane = ws.tabs[idx].root_pane;
            (idx, terminal, runtime, root_pane)
        };
        self.terminal_runtimes.insert(terminal.id.clone(), runtime);
        self.state.terminals.insert(terminal.id.clone(), terminal);
        self.state.remove_alias_shadowed_by_new_pane(root_pane);
        if focus {
            self.state.workspaces[ws_idx].switch_tab(idx);
            self.state.mode = Mode::Terminal;
        }
        let workspace_id = self.state.workspaces[ws_idx].id.clone();
        let tab_id = self
            .public_tab_id(ws_idx, idx)
            .unwrap_or_else(|| format!("{}:{}", workspace_id, idx + 1));
        let root_pane = self.state.workspaces[ws_idx].tabs[idx].root_pane.raw();
        crate::logging::tab_created(&workspace_id, &tab_id, root_pane);
        self.schedule_session_save();
        Ok(idx)
    }

    pub(crate) fn create_agent_profile_tab(
        &mut self,
        ws_idx: usize,
        profile_id: &str,
    ) -> std::io::Result<usize> {
        let Some(profile) = self.state.agent_profiles.get(profile_id).cloned() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "agent profile not found",
            ));
        };
        if !self.state.agent_profile_launchable(&profile) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "agent profile requires an installed integration",
            ));
        }
        let follow_cwd = self.seed_cwd_from_workspace(ws_idx);
        let initial_cwd = self.resolve_new_terminal_cwd(follow_cwd);
        let (rows, cols) = self.state.estimate_pane_size();
        let scrollback_limit_bytes = self.state.pane_scrollback_limit_bytes;
        let host_terminal_theme = self.state.host_terminal_theme;
        let default_shell = self.state.default_shell.clone();
        let shell_mode = self.state.shell_mode;
        let (idx, terminal, runtime, root_pane) = {
            let ws = &mut self.state.workspaces[ws_idx];
            let (idx, mut terminal, runtime) = ws.create_profile_command_tab(
                rows,
                cols,
                initial_cwd,
                crate::pane::PaneShellConfig::new(&default_shell, shell_mode),
                &profile.command,
                &profile.env,
                scrollback_limit_bytes,
                host_terminal_theme,
            )?;
            terminal.launch_argv = Some(profile.argv.clone());
            terminal.launch_env = profile.env.clone();
            if let Some(tab) = ws.tabs.get_mut(idx) {
                tab.set_custom_name(profile.name.clone());
            }
            let root_pane = ws.tabs[idx].root_pane;
            (idx, terminal, runtime, root_pane)
        };
        self.terminal_runtimes.insert(terminal.id.clone(), runtime);
        self.state.terminals.insert(terminal.id.clone(), terminal);
        self.state.remove_alias_shadowed_by_new_pane(root_pane);
        self.state.workspaces[ws_idx].switch_tab(idx);
        self.state.active = Some(ws_idx);
        self.state.mode = Mode::Terminal;
        let workspace_id = self.state.workspaces[ws_idx].id.clone();
        let tab_id = self
            .public_tab_id(ws_idx, idx)
            .unwrap_or_else(|| format!("{}:{}", workspace_id, idx + 1));
        crate::logging::tab_created(&workspace_id, &tab_id, root_pane.raw());
        self.schedule_session_save();
        Ok(idx)
    }

    pub(super) fn create_workspace_with_options(
        &mut self,
        initial_cwd: PathBuf,
        focus: bool,
    ) -> std::io::Result<usize> {
        let group_id = self.state.active_group_id().to_string();
        self.create_workspace_with_launch_env_in_group(initial_cwd, focus, group_id, Vec::new())
    }

    pub(super) fn create_workspace_with_options_in_group(
        &mut self,
        initial_cwd: std::path::PathBuf,
        focus: bool,
        group_id: String,
    ) -> std::io::Result<usize> {
        self.create_workspace_with_launch_env_in_group(initial_cwd, focus, group_id, Vec::new())
    }

    pub(crate) fn create_workspace_with_launch_env(
        &mut self,
        initial_cwd: PathBuf,
        focus: bool,
        extra_env: Vec<(String, String)>,
    ) -> std::io::Result<usize> {
        let group_id = self.state.active_group_id().to_string();
        self.create_workspace_with_launch_env_in_group(initial_cwd, focus, group_id, extra_env)
    }

    pub(crate) fn create_workspace_with_launch_env_in_group(
        &mut self,
        initial_cwd: PathBuf,
        focus: bool,
        group_id: String,
        extra_env: Vec<(String, String)>,
    ) -> std::io::Result<usize> {
        let (rows, cols) = self.state.estimate_pane_size();
        let custom_name = self.collision_free_workspace_name(&initial_cwd, &group_id);
        let (mut ws, terminal, runtime) = Workspace::new_with_extra_env(
            initial_cwd,
            rows,
            cols,
            self.state.pane_scrollback_limit_bytes,
            self.state.host_terminal_theme,
            crate::pane::PaneShellConfig::new(&self.state.default_shell, self.state.shell_mode),
            self.event_tx.clone(),
            self.render_notify.clone(),
            self.render_dirty.clone(),
            extra_env,
        )?;
        ws.group_id = group_id;
        if let Some(name) = custom_name {
            ws.set_custom_name(name);
        }
        self.terminal_runtimes.insert(terminal.id.clone(), runtime);
        self.state.terminals.insert(terminal.id.clone(), terminal);
        self.state.workspaces.push(ws);
        let idx = self.state.workspaces.len() - 1;
        self.state
            .remove_alias_shadowed_by_new_pane(self.state.workspaces[idx].tabs[0].root_pane);
        let workspace_id = self.state.workspaces[idx].id.clone();
        let root_pane = self.state.workspaces[idx].tabs[0].root_pane.raw();
        crate::logging::workspace_created(&workspace_id, root_pane);
        if focus || self.state.active.is_none() {
            self.state.switch_workspace(idx);
            self.state.mode = Mode::Terminal;
        }
        self.schedule_session_save();
        Ok(idx)
    }

    pub(super) fn collect_panes_for_workspace(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<crate::api::schema::PaneInfo>, (String, String)> {
        if let Some(workspace_id) = workspace_id {
            let Some(ws_idx) = self.parse_workspace_id(workspace_id) else {
                return Err((
                    "workspace_not_found".into(),
                    format!("workspace {workspace_id} not found"),
                ));
            };
            let Some(ws) = self.state.workspaces.get(ws_idx) else {
                return Err((
                    "workspace_not_found".into(),
                    format!("workspace {workspace_id} not found"),
                ));
            };
            Ok(ws
                .tabs
                .iter()
                .flat_map(|tab| tab.layout.pane_ids().into_iter())
                .filter_map(|pane_id| self.pane_info(ws_idx, pane_id))
                .collect())
        } else {
            Ok(self
                .state
                .workspaces
                .iter()
                .enumerate()
                .flat_map(|(ws_idx, ws)| {
                    ws.tabs
                        .iter()
                        .flat_map(|tab| tab.layout.pane_ids().into_iter())
                        .filter_map(move |pane_id| self.pane_info(ws_idx, pane_id))
                })
                .collect())
        }
    }

    pub(super) fn tab_info(
        &self,
        ws_idx: usize,
        tab_idx: usize,
    ) -> Option<crate::api::schema::TabInfo> {
        let ws = self.state.workspaces.get(ws_idx)?;
        let tab = ws.tabs.get(tab_idx)?;
        let (agg_state, seen) = tab
            .panes
            .values()
            .filter_map(|pane| {
                self.state
                    .terminals
                    .get(&pane.attached_terminal_id)
                    .map(|terminal| (terminal.state, pane.seen))
            })
            .max_by_key(|(state, seen)| tab_attention_priority(*state, *seen))
            .unwrap_or((crate::detect::AgentState::Unknown, true));
        Some(crate::api::schema::TabInfo {
            tab_id: self.public_tab_id(ws_idx, tab_idx)?,
            workspace_id: self.public_workspace_id(ws_idx),
            number: tab_idx + 1,
            label: tab.display_name(),
            focused: self.state.active == Some(ws_idx) && ws.active_tab == tab_idx,
            pane_count: tab.panes.len(),
            agent_status: pane_agent_status(agg_state, seen),
        })
    }

    pub(super) fn workspace_created_result(
        &self,
        ws_idx: usize,
    ) -> Option<crate::api::schema::ResponseResult> {
        Some(crate::api::schema::ResponseResult::WorkspaceCreated {
            workspace: self.workspace_info(ws_idx),
            tab: self.tab_info(ws_idx, 0)?,
            root_pane: self.root_pane_info(ws_idx, 0)?,
        })
    }

    pub(super) fn tab_created_result(
        &self,
        ws_idx: usize,
        tab_idx: usize,
    ) -> Option<crate::api::schema::ResponseResult> {
        Some(crate::api::schema::ResponseResult::TabCreated {
            tab: self.tab_info(ws_idx, tab_idx)?,
            root_pane: self.root_pane_info(ws_idx, tab_idx)?,
        })
    }

    pub(super) fn root_pane_info(
        &self,
        ws_idx: usize,
        tab_idx: usize,
    ) -> Option<crate::api::schema::PaneInfo> {
        let ws = self.state.workspaces.get(ws_idx)?;
        let tab = ws.tabs.get(tab_idx)?;
        self.pane_info(ws_idx, tab.root_pane)
    }

    pub(super) fn pane_info(
        &self,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
    ) -> Option<crate::api::schema::PaneInfo> {
        let ws = self.state.workspaces.get(ws_idx)?;
        let pane = ws.pane_state(pane_id)?;
        let terminal = self.state.terminals.get(&pane.attached_terminal_id)?;
        let tab_idx = ws.find_tab_index_for_pane(pane_id)?;
        let focused = self.state.active == Some(ws_idx)
            && ws.active_tab == tab_idx
            && ws
                .focused_pane_id()
                .is_some_and(|focused| focused == pane_id);
        let presentation = terminal.effective_presentation();
        Some(crate::api::schema::PaneInfo {
            pane_id: self.public_pane_id(ws_idx, pane_id)?,
            terminal_id: terminal.id.to_string(),
            workspace_id: self.public_workspace_id(ws_idx),
            tab_id: self.public_tab_id(ws_idx, tab_idx)?,
            focused,
            cwd: ws.tabs[tab_idx]
                .cwd_for_pane(pane_id, &self.state.terminals, &self.terminal_runtimes)
                .map(|cwd| cwd.display().to_string()),
            foreground_cwd: ws.tabs[tab_idx]
                .foreground_cwd_for_pane(pane_id, &self.terminal_runtimes)
                .map(|cwd| cwd.display().to_string()),
            label: terminal.manual_label.clone(),
            agent: terminal.effective_agent_label().map(str::to_string),
            title: presentation.title,
            display_agent: presentation.display_agent,
            agent_status: pane_agent_status(terminal.state, pane.seen),
            custom_status: presentation.custom_status,
            state_labels: presentation.state_labels,
            agent_session: terminal_agent_session_info(terminal),
            revision: terminal.revision,
        })
    }

    pub(super) fn lookup_runtime(
        &self,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
    ) -> Option<(&crate::terminal::TerminalRuntime, String)> {
        let runtime =
            self.state
                .runtime_for_pane_in_workspace(&self.terminal_runtimes, ws_idx, pane_id)?;
        Some((runtime, self.public_workspace_id(ws_idx)))
    }

    pub(super) fn lookup_runtime_sender(
        &self,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
    ) -> Option<&crate::terminal::TerminalRuntime> {
        self.state
            .runtime_for_pane_in_workspace(&self.terminal_runtimes, ws_idx, pane_id)
    }

    pub(super) fn workspace_info(&self, index: usize) -> crate::api::schema::WorkspaceInfo {
        let ws = &self.state.workspaces[index];
        let (agg_state, seen) = ws.aggregate_state(&self.state.terminals);
        crate::api::schema::WorkspaceInfo {
            workspace_id: self.public_workspace_id(index),
            group_id: ws.group_id.clone(),
            number: index + 1,
            label: ws.display_name_from(&self.state.terminals, &self.terminal_runtimes),
            focused: self.state.active == Some(index),
            pane_count: ws.public_pane_numbers.len(),
            tab_count: ws.tabs.len(),
            active_tab_id: self
                .public_tab_id(index, ws.active_tab)
                .unwrap_or_else(|| format!("{}:{}", ws.id, ws.active_tab + 1)),
            agent_status: pane_agent_status(agg_state, seen),
            worktree: ws
                .worktree_space()
                .map(|space| crate::api::schema::WorkspaceWorktreeInfo {
                    repo_key: space.key.clone(),
                    repo_name: space.label.clone(),
                    repo_root: space.repo_root.display().to_string(),
                    checkout_path: space.checkout_path.display().to_string(),
                    is_linked_worktree: space.is_linked_worktree,
                }),
        }
    }

    pub(super) fn group_info(&self, index: usize) -> crate::api::schema::GroupInfo {
        let group = &self.state.groups[index];
        let workspace_count = self
            .state
            .workspaces
            .iter()
            .filter(|workspace| workspace.group_id == group.id)
            .count();
        crate::api::schema::GroupInfo {
            group_id: group.id.clone(),
            number: index + 1,
            name: group.name.clone(),
            icon: group.icon.clone(),
            focused: self.state.active_group == index,
            workspace_count,
        }
    }
}

fn terminal_agent_session_info(
    terminal: &crate::terminal::TerminalState,
) -> Option<crate::api::schema::AgentSessionInfo> {
    if let Some(authority) = terminal.hook_authority.as_ref() {
        if let Some(session_ref) = authority.session_ref.as_ref() {
            return Some(crate::api::schema::AgentSessionInfo {
                source: authority.source.clone(),
                agent: authority.agent_label.clone(),
                kind: session_ref.kind,
                value: session_ref.value.clone(),
            });
        }
    }

    terminal
        .persisted_agent_session
        .as_ref()
        .map(|session| crate::api::schema::AgentSessionInfo {
            source: session.source.clone(),
            agent: session.agent.clone(),
            kind: session.session_ref.kind,
            value: session.session_ref.value.clone(),
        })
}
