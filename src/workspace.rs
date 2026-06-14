use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::layout::Direction;
use tokio::sync::{mpsc, Notify};

use crate::events::AppEvent;
use crate::layout::PaneId;
#[cfg(test)]
use crate::layout::TileLayout;
use crate::pane::PaneState;
use crate::terminal::{TerminalId, TerminalRuntime, TerminalRuntimeRegistry, TerminalState};

mod aggregate;
mod git;
mod tab;

#[cfg(test)]
use self::git::git_ahead_behind;
pub(crate) use self::git::git_repo_root;
use self::git::{git_work_summary, git_work_summary_for_root as load_git_work_summary_for_root};
pub use self::{
    git::{
        derive_label_from_cwd, git_branch, git_space_metadata, git_status_cache_key,
        GitSpaceMetadata, GitStatusCacheEntry,
    },
    tab::Tab,
};

pub const DEFAULT_GROUP_ID: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorktreeSpaceMembership {
    pub key: String,
    pub label: String,
    pub repo_root: PathBuf,
    pub checkout_path: PathBuf,
    pub is_linked_worktree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceGitStatus {
    pub workspace_id: String,
    pub resolved_identity_cwd: PathBuf,
    pub cwd_fingerprint: Vec<PathBuf>,
    pub branch: Option<String>,
    pub ahead_behind: Option<(usize, usize)>,
    pub work_summary: Option<GitWorkSummary>,
    pub space: Option<GitSpaceMetadata>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GitWorkSummary {
    pub repo_count: usize,
    pub conflicted: usize,
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceGitStatusSnapshot {
    pub branch: Option<String>,
    pub ahead_behind: Option<(usize, usize)>,
    pub space: Option<GitSpaceMetadata>,
}

impl WorkspaceGitStatusSnapshot {
    pub fn into_workspace_status(
        self,
        workspace_id: String,
        resolved_identity_cwd: PathBuf,
        cwd_fingerprint: Vec<PathBuf>,
    ) -> WorkspaceGitStatus {
        let work_summary = git_work_summary(&cwd_fingerprint);
        WorkspaceGitStatus {
            workspace_id,
            resolved_identity_cwd,
            cwd_fingerprint,
            branch: self.branch,
            ahead_behind: self.ahead_behind,
            work_summary,
            space: self.space,
        }
    }
}

static NEXT_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn generate_workspace_id() -> String {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or(0);
    let counter = NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed);
    format!("w{micros:x}{counter:x}")
}

/// A named workspace containing tabs.
pub struct Workspace {
    /// Stable public workspace identity, independent of display order.
    pub id: String,
    /// User-provided override. If set, auto-derived identity stops updating.
    pub custom_name: Option<String>,
    /// Sidebar group this workspace belongs to.
    pub group_id: String,
    /// Fallback workspace identity source for tests, old snapshots, or missing runtimes.
    pub identity_cwd: PathBuf,
    /// Cached current git branch for the workspace repo.
    pub(crate) cached_git_branch: Option<String>,
    /// Cached ahead/behind counts for the workspace repo's current branch upstream.
    pub(crate) cached_git_ahead_behind: Option<(usize, usize)>,
    /// Cached aggregate git working-tree state across this space's pane cwd set.
    pub(crate) cached_git_work_summary: Option<GitWorkSummary>,
    /// Cached derived Git repo metadata for worktree actions and status display.
    pub(crate) cached_git_space: Option<GitSpaceMetadata>,
    /// Explicit Hako-managed worktree grouping provenance.
    pub worktree_space: Option<WorktreeSpaceMembership>,
    /// Stable-ish public pane numbers within this workspace.
    /// New panes append at the end; closing a pane compacts higher numbers down.
    pub public_pane_numbers: HashMap<PaneId, usize>,
    pub(crate) next_public_pane_number: usize,
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    #[cfg(test)]
    pub(crate) test_runtimes: HashMap<PaneId, TerminalRuntime>,
}

type ArgvLaunch<'a> = (&'a [String], &'a [(String, String)]);

enum NewWorkspaceTabCommand<'a> {
    Shell {
        command: &'a str,
        extra_env: &'a [(String, String)],
    },
    Profile {
        command: &'a str,
        extra_env: &'a [(String, String)],
        shell_config: crate::pane::PaneShellConfig<'a>,
    },
}

impl Deref for Workspace {
    type Target = Tab;

    fn deref(&self) -> &Self::Target {
        self.active_tab()
            .expect("workspace must always have at least one active tab")
    }
}

impl DerefMut for Workspace {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.active_tab_mut()
            .expect("workspace must always have at least one active tab")
    }
}

impl Workspace {
    pub fn new(
        initial_cwd: PathBuf,
        rows: u16,
        cols: u16,
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        shell_config: crate::pane::PaneShellConfig<'_>,
        events: mpsc::Sender<AppEvent>,
        render_notify: Arc<Notify>,
        render_dirty: Arc<AtomicBool>,
    ) -> std::io::Result<(Self, TerminalState, TerminalRuntime)> {
        Self::new_with_tab(
            initial_cwd,
            rows,
            cols,
            scrollback_limit_bytes,
            host_terminal_theme,
            shell_config,
            events,
            render_notify,
            render_dirty,
            None,
        )
    }

    pub fn new_argv_command(
        initial_cwd: PathBuf,
        rows: u16,
        cols: u16,
        argv: &[String],
        extra_env: &[(String, String)],
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        events: mpsc::Sender<AppEvent>,
        render_notify: Arc<Notify>,
        render_dirty: Arc<AtomicBool>,
    ) -> std::io::Result<(Self, TerminalState, TerminalRuntime)> {
        Self::new_with_tab(
            initial_cwd,
            rows,
            cols,
            scrollback_limit_bytes,
            host_terminal_theme,
            crate::pane::PaneShellConfig::new("", crate::config::ShellModeConfig::NonLogin),
            events,
            render_notify,
            render_dirty,
            Some((argv, extra_env)),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_with_tab(
        initial_cwd: PathBuf,
        rows: u16,
        cols: u16,
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        shell_config: crate::pane::PaneShellConfig<'_>,
        events: mpsc::Sender<AppEvent>,
        render_notify: Arc<Notify>,
        render_dirty: Arc<AtomicBool>,
        argv: Option<ArgvLaunch<'_>>,
    ) -> std::io::Result<(Self, TerminalState, TerminalRuntime)> {
        let (tab, terminal, runtime) = if let Some((argv, extra_env)) = argv {
            Tab::new_argv_command(
                1,
                initial_cwd.clone(),
                rows,
                cols,
                argv,
                extra_env,
                scrollback_limit_bytes,
                host_terminal_theme,
                events,
                render_notify,
                render_dirty,
            )?
        } else {
            Tab::new(
                1,
                initial_cwd.clone(),
                rows,
                cols,
                scrollback_limit_bytes,
                host_terminal_theme,
                shell_config,
                events,
                render_notify,
                render_dirty,
            )?
        };
        let mut public_pane_numbers = HashMap::new();
        public_pane_numbers.insert(tab.root_pane, 1);
        Ok((
            Self {
                id: generate_workspace_id(),
                custom_name: None,
                group_id: DEFAULT_GROUP_ID.to_string(),
                identity_cwd: initial_cwd.clone(),
                cached_git_branch: git_branch(&initial_cwd),
                cached_git_ahead_behind: None,
                cached_git_work_summary: None,
                cached_git_space: None,
                worktree_space: None,
                public_pane_numbers,
                next_public_pane_number: 2,
                tabs: vec![tab],
                active_tab: 0,
                #[cfg(test)]
                test_runtimes: HashMap::new(),
            },
            terminal,
            runtime,
        ))
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    pub fn active_tab_index(&self) -> usize {
        self.active_tab
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    pub fn active_tab_display_name(&self) -> Option<String> {
        self.active_tab().map(Tab::display_name)
    }

    pub fn switch_tab(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active_tab = idx;
            if let Some(tab) = self.tabs.get_mut(idx) {
                for pane in tab.panes.values_mut() {
                    pane.seen = true;
                }
            }
        }
    }

    pub fn create_native_diff_tab(
        &mut self,
        session: crate::native_diff::NativeDiffSession,
    ) -> Result<usize, String> {
        let number = self.tabs.len() + 1;
        let Some(source_tab) = self.tabs.get(self.active_tab) else {
            return Err("workspace has no tab to inherit render handles".to_string());
        };
        let tab = Tab::new_native_diff(
            number,
            session,
            source_tab.events.clone(),
            source_tab.render_notify.clone(),
            source_tab.render_dirty.clone(),
        );
        let tab_idx = self.tabs.len();
        self.public_pane_numbers
            .insert(tab.root_pane, self.next_public_pane_number);
        self.next_public_pane_number += 1;
        self.tabs.push(tab);
        self.active_tab = tab_idx;
        Ok(tab_idx)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_tab_with_handles(
        &mut self,
        rows: u16,
        cols: u16,
        cwd: PathBuf,
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        shell_config: crate::pane::PaneShellConfig<'_>,
        events: mpsc::Sender<AppEvent>,
        render_notify: Arc<Notify>,
        render_dirty: Arc<AtomicBool>,
    ) -> std::io::Result<(usize, TerminalState, TerminalRuntime)> {
        self.create_tab_with_runtime(
            rows,
            cols,
            cwd,
            scrollback_limit_bytes,
            host_terminal_theme,
            shell_config,
            None,
            None,
            Some(events),
            Some(render_notify),
            Some(render_dirty),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_command_tab(
        &mut self,
        rows: u16,
        cols: u16,
        cwd: PathBuf,
        command: &str,
        extra_env: &[(String, String)],
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
    ) -> std::io::Result<(usize, TerminalState, TerminalRuntime)> {
        self.create_tab_with_runtime(
            rows,
            cols,
            cwd,
            scrollback_limit_bytes,
            host_terminal_theme,
            crate::pane::PaneShellConfig::new("", crate::config::ShellModeConfig::NonLogin),
            Some(NewWorkspaceTabCommand::Shell { command, extra_env }),
            None,
            None,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_profile_command_tab(
        &mut self,
        rows: u16,
        cols: u16,
        cwd: PathBuf,
        shell_config: crate::pane::PaneShellConfig<'_>,
        command: &str,
        extra_env: &[(String, String)],
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
    ) -> std::io::Result<(usize, TerminalState, TerminalRuntime)> {
        self.create_tab_with_runtime(
            rows,
            cols,
            cwd,
            scrollback_limit_bytes,
            host_terminal_theme,
            crate::pane::PaneShellConfig::new("", crate::config::ShellModeConfig::NonLogin),
            Some(NewWorkspaceTabCommand::Profile {
                command,
                extra_env,
                shell_config,
            }),
            None,
            None,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create_tab_with_runtime(
        &mut self,
        rows: u16,
        cols: u16,
        cwd: PathBuf,
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        shell_config: crate::pane::PaneShellConfig<'_>,
        command: Option<NewWorkspaceTabCommand<'_>>,
        argv: Option<ArgvLaunch<'_>>,
        fallback_events: Option<mpsc::Sender<AppEvent>>,
        fallback_render_notify: Option<Arc<Notify>>,
        fallback_render_dirty: Option<Arc<AtomicBool>>,
    ) -> std::io::Result<(usize, TerminalState, TerminalRuntime)> {
        let number = self.tabs.len() + 1;
        let Some((events, render_notify, render_dirty)) = self
            .active_tab()
            .map(|tab| {
                (
                    tab.events.clone(),
                    tab.render_notify.clone(),
                    tab.render_dirty.clone(),
                )
            })
            .or_else(|| {
                Some((
                    fallback_events?,
                    fallback_render_notify?,
                    fallback_render_dirty?,
                ))
            })
        else {
            return Err(std::io::Error::other(
                "cannot create tab in empty workspace without runtime handles",
            ));
        };

        let (tab, terminal, runtime) = if let Some(command) = command {
            match command {
                NewWorkspaceTabCommand::Shell { command, extra_env } => Tab::new_shell_command(
                    number,
                    cwd,
                    rows,
                    cols,
                    command,
                    extra_env,
                    scrollback_limit_bytes,
                    host_terminal_theme,
                    events,
                    render_notify,
                    render_dirty,
                )?,
                NewWorkspaceTabCommand::Profile {
                    command,
                    extra_env,
                    shell_config,
                } => Tab::new_profile_command(
                    number,
                    cwd,
                    rows,
                    cols,
                    shell_config,
                    command,
                    extra_env,
                    scrollback_limit_bytes,
                    host_terminal_theme,
                    events,
                    render_notify,
                    render_dirty,
                )?,
            }
        } else if let Some((argv, extra_env)) = argv {
            Tab::new_argv_command(
                number,
                cwd,
                rows,
                cols,
                argv,
                extra_env,
                scrollback_limit_bytes,
                host_terminal_theme,
                events,
                render_notify,
                render_dirty,
            )?
        } else {
            Tab::new(
                number,
                cwd,
                rows,
                cols,
                scrollback_limit_bytes,
                host_terminal_theme,
                shell_config,
                events,
                render_notify,
                render_dirty,
            )?
        };
        self.register_new_pane(tab.root_pane);
        self.tabs.push(tab);
        Ok((self.tabs.len() - 1, terminal, runtime))
    }

    pub fn close_tab(&mut self, idx: usize) -> bool {
        if self.tabs.len() <= 1 || idx >= self.tabs.len() {
            return false;
        }
        self.close_tab_allow_empty(idx)
    }

    pub fn close_tab_allow_empty(&mut self, idx: usize) -> bool {
        if idx >= self.tabs.len() {
            return false;
        }
        let tab = self.tabs.remove(idx);
        for pane_id in tab.panes.keys() {
            self.unregister_pane(*pane_id);
        }
        self.renumber_tabs();
        if self.tabs.is_empty() {
            self.active_tab = 0;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        } else if idx <= self.active_tab && self.active_tab > 0 {
            self.active_tab -= 1;
        }
        true
    }

    pub fn move_tab(&mut self, source_idx: usize, insert_idx: usize) -> bool {
        if source_idx >= self.tabs.len() || insert_idx > self.tabs.len() {
            return false;
        }

        let target_idx = if source_idx < insert_idx {
            insert_idx.saturating_sub(1)
        } else {
            insert_idx
        }
        .min(self.tabs.len().saturating_sub(1));

        if source_idx == target_idx {
            return false;
        }

        let active_root_pane = self.tabs.get(self.active_tab).map(|tab| tab.root_pane);
        let tab = self.tabs.remove(source_idx);
        self.tabs.insert(target_idx, tab);
        self.renumber_tabs();
        self.active_tab = active_root_pane
            .and_then(|root_pane| self.tabs.iter().position(|tab| tab.root_pane == root_pane))
            .unwrap_or(target_idx);
        true
    }

    pub fn close_active_tab(&mut self) -> bool {
        self.close_tab(self.active_tab)
    }

    pub fn split_focused(
        &mut self,
        direction: Direction,
        rows: u16,
        cols: u16,
        cwd: Option<PathBuf>,
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        shell_config: crate::pane::PaneShellConfig<'_>,
    ) -> std::io::Result<crate::workspace::tab::NewPane> {
        let new_pane = self
            .active_tab_mut()
            .expect("workspace must always have at least one tab")
            .split_focused(
                direction,
                rows,
                cols,
                cwd,
                scrollback_limit_bytes,
                host_terminal_theme,
                shell_config,
            )?;
        self.register_new_pane(new_pane.pane_id);
        Ok(new_pane)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn split_focused_command(
        &mut self,
        direction: Direction,
        rows: u16,
        cols: u16,
        cwd: Option<PathBuf>,
        command: &str,
        extra_env: &[(String, String)],
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
    ) -> std::io::Result<crate::workspace::tab::NewPane> {
        let new_pane = self
            .active_tab_mut()
            .expect("workspace must always have at least one tab")
            .split_focused_command(
                direction,
                rows,
                cols,
                cwd,
                command,
                extra_env,
                scrollback_limit_bytes,
                host_terminal_theme,
            )?;
        self.register_new_pane(new_pane.pane_id);
        Ok(new_pane)
    }

    pub fn split_pane(
        &mut self,
        pane_id: PaneId,
        direction: Direction,
        rows: u16,
        cols: u16,
        cwd: Option<PathBuf>,
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        shell_config: crate::pane::PaneShellConfig<'_>,
        focus_new_pane: bool,
    ) -> Option<std::io::Result<(usize, crate::workspace::tab::NewPane)>> {
        self.split_pane_with_runtime(
            pane_id,
            direction,
            rows,
            cols,
            cwd,
            scrollback_limit_bytes,
            host_terminal_theme,
            shell_config,
            focus_new_pane,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn split_pane_argv_command(
        &mut self,
        pane_id: PaneId,
        direction: Direction,
        rows: u16,
        cols: u16,
        cwd: Option<PathBuf>,
        argv: &[String],
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        focus_new_pane: bool,
    ) -> Option<std::io::Result<(usize, crate::workspace::tab::NewPane)>> {
        self.split_pane_with_runtime(
            pane_id,
            direction,
            rows,
            cols,
            cwd,
            scrollback_limit_bytes,
            host_terminal_theme,
            crate::pane::PaneShellConfig::new("", crate::config::ShellModeConfig::NonLogin),
            focus_new_pane,
            Some(argv),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn split_pane_with_runtime(
        &mut self,
        pane_id: PaneId,
        direction: Direction,
        rows: u16,
        cols: u16,
        cwd: Option<PathBuf>,
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        shell_config: crate::pane::PaneShellConfig<'_>,
        focus_new_pane: bool,
        argv: Option<&[String]>,
    ) -> Option<std::io::Result<(usize, crate::workspace::tab::NewPane)>> {
        let tab_idx = self.find_tab_index_for_pane(pane_id)?;
        let tab = &mut self.tabs[tab_idx];
        let previous_focus = tab.layout.focused();
        tab.layout.focus_pane(pane_id);
        let new_pane = match if let Some(argv) = argv {
            tab.split_focused_argv_command(
                direction,
                rows,
                cols,
                cwd,
                argv,
                &[],
                scrollback_limit_bytes,
                host_terminal_theme,
            )
        } else {
            tab.split_focused(
                direction,
                rows,
                cols,
                cwd,
                scrollback_limit_bytes,
                host_terminal_theme,
                shell_config,
            )
        } {
            Ok(new_pane) => new_pane,
            Err(err) => {
                tab.layout.focus_pane(previous_focus);
                return Some(Err(err));
            }
        };
        if !focus_new_pane {
            tab.layout.focus_pane(previous_focus);
        }
        self.register_new_pane(new_pane.pane_id);
        Some(Ok((tab_idx, new_pane)))
    }

    /// Close the focused pane. Returns true if the workspace should close.
    pub fn close_focused(&mut self) -> bool {
        let pane_count = self
            .active_tab()
            .map(|tab| tab.layout.pane_count())
            .unwrap_or(0);
        let tab_count = self.tabs.len();
        if pane_count <= 1 {
            return tab_count <= 1 || self.close_active_tab_and_report();
        }

        if let Some((removed, _terminal_id)) = self.active_tab_mut().and_then(Tab::close_focused) {
            self.unregister_pane(removed);
        }
        false
    }

    /// Remove a specific pane from this workspace without terminating its runtime.
    /// Returns true if the workspace should close.
    pub fn remove_pane(&mut self, pane_id: PaneId) -> bool {
        let Some(tab_idx) = self.find_tab_index_for_pane(pane_id) else {
            return false;
        };
        let pane_count = self.tabs[tab_idx].layout.pane_count();
        let tab_count = self.tabs.len();
        if pane_count <= 1 {
            if tab_count <= 1 {
                return true;
            }
            self.tabs.remove(tab_idx);
            self.unregister_pane(pane_id);
            self.renumber_tabs();
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            } else if tab_idx <= self.active_tab && self.active_tab > 0 {
                self.active_tab -= 1;
            }
            return false;
        }

        if let Some((removed, _terminal_id)) = self.tabs[tab_idx].remove_pane(pane_id) {
            self.unregister_pane(removed);
        }
        false
    }

    pub fn public_pane_number(&self, pane_id: PaneId) -> Option<usize> {
        self.public_pane_numbers.get(&pane_id).copied()
    }

    pub fn set_custom_name(&mut self, name: String) {
        self.custom_name = Some(name);
    }

    pub fn resolved_identity_cwd(&self) -> Option<PathBuf> {
        Some(self.identity_cwd.clone())
    }

    pub fn resolved_identity_cwd_from(
        &self,
        terminals: &HashMap<TerminalId, TerminalState>,
        terminal_runtimes: &TerminalRuntimeRegistry,
    ) -> Option<PathBuf> {
        self.active_tab()
            .and_then(|tab| tab.cwd_for_pane(tab.layout.focused(), terminals, terminal_runtimes))
            .or_else(|| Some(self.identity_cwd.clone()))
    }

    pub fn display_name(&self) -> String {
        if let Some(name) = &self.custom_name {
            return name.clone();
        }

        self.resolved_identity_cwd()
            .map(|cwd| derive_label_from_cwd(&cwd))
            .unwrap_or_else(|| "workspace".into())
    }

    pub fn display_name_from(
        &self,
        terminals: &HashMap<TerminalId, TerminalState>,
        terminal_runtimes: &TerminalRuntimeRegistry,
    ) -> String {
        if let Some(name) = &self.custom_name {
            return name.clone();
        }

        self.resolved_identity_cwd_from(terminals, terminal_runtimes)
            .map(|cwd| derive_label_from_cwd(&cwd))
            .unwrap_or_else(|| "workspace".into())
    }

    #[cfg(test)]
    pub fn branch(&self) -> Option<String> {
        self.cached_git_branch.clone()
    }

    #[cfg(test)]
    pub fn git_ahead_behind(&self) -> Option<(usize, usize)> {
        self.cached_git_ahead_behind
    }

    pub fn git_space(&self) -> Option<&GitSpaceMetadata> {
        self.cached_git_space.as_ref()
    }

    pub fn worktree_space(&self) -> Option<&WorktreeSpaceMembership> {
        self.worktree_space.as_ref()
    }

    pub fn git_work_summary_label(&self) -> String {
        let Some(summary) = self.cached_git_work_summary else {
            return String::new();
        };

        let mut parts = Vec::new();
        if summary.conflicted > 0 {
            parts.push(format!("!{}", summary.conflicted));
        }
        if summary.added > 0 {
            parts.push(format!("+{}", summary.added));
        }
        if summary.modified > 0 {
            parts.push(format!("~{}", summary.modified));
        }
        if summary.deleted > 0 {
            parts.push(format!("-{}", summary.deleted));
        }

        let state = if parts.is_empty() {
            String::new()
        } else {
            parts.join(" ")
        };

        if summary.repo_count > 1 {
            if state.is_empty() {
                format!("{} repos", summary.repo_count)
            } else {
                format!("{} repos · {state}", summary.repo_count)
            }
        } else {
            state
        }
    }

    pub fn git_work_summary_for_root(root: &std::path::Path) -> Option<GitWorkSummary> {
        load_git_work_summary_for_root(root)
    }

    #[cfg(test)]
    pub fn refresh_git_ahead_behind(&mut self) {
        let cwd = self.resolved_identity_cwd();
        self.cached_git_branch = cwd.as_deref().and_then(git_branch);
        self.cached_git_ahead_behind = cwd.as_deref().and_then(git_ahead_behind);
        self.cached_git_work_summary = git_work_summary(&self.git_status_cwds());
        self.cached_git_space = cwd.as_deref().and_then(git_space_metadata);
    }

    #[cfg(test)]
    pub fn git_status_cwds(&self) -> Vec<PathBuf> {
        let mut cwds = self
            .tabs
            .iter()
            .flat_map(|tab| {
                tab.layout.pane_ids().into_iter().filter_map(|id| {
                    self.test_runtimes
                        .get(&id)
                        .and_then(TerminalRuntime::cwd)
                        .or_else(|| tab.runtimes.get(&id).and_then(TerminalRuntime::cwd))
                })
            })
            .collect::<Vec<_>>();
        cwds.sort();
        cwds.dedup();
        if cwds.is_empty() {
            cwds.push(self.identity_cwd.clone());
        }
        cwds
    }

    pub fn git_status_cwds_from(
        &self,
        terminals: &HashMap<TerminalId, TerminalState>,
        terminal_runtimes: &TerminalRuntimeRegistry,
    ) -> Vec<PathBuf> {
        let mut cwds = self
            .tabs
            .iter()
            .flat_map(|tab| {
                tab.layout
                    .pane_ids()
                    .into_iter()
                    .filter_map(|id| tab.cwd_for_pane(id, terminals, terminal_runtimes))
            })
            .collect::<Vec<_>>();
        cwds.sort();
        cwds.dedup();
        if cwds.is_empty() {
            cwds.push(self.identity_cwd.clone());
        }
        cwds
    }

    pub fn git_status_snapshot_for_cwd_with_cache(
        resolved_identity_cwd: &std::path::Path,
        cached: Option<&GitStatusCacheEntry>,
    ) -> (WorkspaceGitStatusSnapshot, Option<GitStatusCacheEntry>) {
        self::git::git_status_snapshot_for_cwd(resolved_identity_cwd, cached)
    }

    pub fn find_tab_index_for_pane(&self, pane_id: PaneId) -> Option<usize> {
        self.tabs
            .iter()
            .position(|tab| tab.panes.contains_key(&pane_id))
    }

    pub fn pane_state(&self, pane_id: PaneId) -> Option<&PaneState> {
        self.tabs.iter().find_map(|tab| tab.panes.get(&pane_id))
    }
    pub fn pane_state_mut(&mut self, pane_id: PaneId) -> Option<&mut PaneState> {
        self.tabs
            .iter_mut()
            .find_map(|tab| tab.panes.get_mut(&pane_id))
    }

    pub fn terminal_id(&self, pane_id: PaneId) -> Option<&TerminalId> {
        self.tabs.iter().find_map(|tab| tab.terminal_id(pane_id))
    }

    pub fn focused_pane_id(&self) -> Option<PaneId> {
        self.active_tab().map(|tab| tab.layout.focused())
    }

    pub fn close_pane(&mut self, pane_id: PaneId) -> bool {
        let tab_idx = match self.find_tab_index_for_pane(pane_id) {
            Some(idx) => idx,
            None => return false,
        };
        let pane_count = self.tabs[tab_idx].layout.pane_count();
        let tab_count = self.tabs.len();
        if pane_count <= 1 {
            if tab_count <= 1 {
                return true;
            }
            self.tabs.remove(tab_idx);
            self.unregister_pane(pane_id);
            self.renumber_tabs();
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            } else if tab_idx <= self.active_tab && self.active_tab > 0 {
                self.active_tab -= 1;
            }
            return false;
        }

        if let Some((removed, _terminal_id)) = self.tabs[tab_idx].close_pane(pane_id) {
            self.unregister_pane(removed);
        }
        false
    }

    fn register_new_pane(&mut self, pane_id: PaneId) {
        self.public_pane_numbers
            .insert(pane_id, self.next_public_pane_number);
        self.next_public_pane_number += 1;
    }

    fn unregister_pane(&mut self, pane_id: PaneId) {
        if let Some(removed_number) = self.public_pane_numbers.remove(&pane_id) {
            for number in self.public_pane_numbers.values_mut() {
                if *number > removed_number {
                    *number -= 1;
                }
            }
            self.next_public_pane_number = self.public_pane_numbers.len() + 1;
        }
    }

    fn renumber_tabs(&mut self) {
        for (idx, tab) in self.tabs.iter_mut().enumerate() {
            tab.number = idx + 1;
        }
    }

    fn close_active_tab_and_report(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return true;
        }
        self.close_active_tab();
        false
    }
}

#[cfg(test)]
impl Workspace {
    pub(crate) fn test_new(name: &str) -> Self {
        let (events, _) = mpsc::channel(64);
        let render_notify = Arc::new(Notify::new());
        let render_dirty = Arc::new(AtomicBool::new(false));
        let identity_cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());
        let (layout, root_id) = TileLayout::new();
        let terminal_id = TerminalId::alloc();
        let mut panes = HashMap::new();
        panes.insert(
            root_id,
            PaneState::new_with_env_pane_id(terminal_id, root_id),
        );
        let tab = Tab {
            custom_name: None,
            number: 1,
            root_pane: root_id,
            layout,
            panes,
            runtimes: HashMap::new(),
            zoomed: false,
            events,
            render_notify,
            render_dirty,
        };
        let mut public_pane_numbers = HashMap::new();
        public_pane_numbers.insert(tab.root_pane, 1);
        Self {
            id: generate_workspace_id(),
            custom_name: Some(name.to_string()),
            group_id: DEFAULT_GROUP_ID.to_string(),
            identity_cwd: identity_cwd.clone(),
            cached_git_branch: git_branch(&identity_cwd),
            cached_git_ahead_behind: None,
            cached_git_work_summary: None,
            cached_git_space: None,
            worktree_space: None,
            public_pane_numbers,
            next_public_pane_number: 2,
            tabs: vec![tab],
            active_tab: 0,
            test_runtimes: HashMap::new(),
        }
    }

    pub(crate) fn insert_test_runtime(&mut self, pane_id: PaneId, runtime: TerminalRuntime) {
        self.test_runtimes.insert(pane_id, runtime);
    }

    pub(crate) fn test_split(&mut self, direction: Direction) -> PaneId {
        let tab = self.active_tab_mut().expect("workspace must have tab");
        let new_id = tab.layout.split_focused(direction);
        tab.panes.insert(
            new_id,
            PaneState::new_with_env_pane_id(TerminalId::alloc(), new_id),
        );
        self.register_new_pane(new_id);
        new_id
    }

    pub(crate) fn test_add_tab(&mut self, name: Option<&str>) -> usize {
        let (events, _) = mpsc::channel(64);
        let render_notify = Arc::new(Notify::new());
        let render_dirty = Arc::new(AtomicBool::new(false));
        let (layout, root_id) = TileLayout::new();
        let mut panes = HashMap::new();
        panes.insert(
            root_id,
            PaneState::new_with_env_pane_id(TerminalId::alloc(), root_id),
        );
        let tab = Tab {
            custom_name: name.map(str::to_string),
            number: self.tabs.len() + 1,
            root_pane: root_id,
            layout,
            panes,
            runtimes: HashMap::new(),
            zoomed: false,
            events,
            render_notify,
            render_dirty,
        };
        self.register_new_pane(root_id);
        self.tabs.push(tab);
        self.tabs.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_display_name_from_uses_live_runtime_cwd() {
        let mut ws = Workspace::test_new("ignored");
        ws.custom_name = None;
        ws.identity_cwd = PathBuf::from("/hako-test/original");
        let root_pane = ws.tabs[0].root_pane;
        let terminal_id = ws.tabs[0].terminal_id(root_pane).unwrap().clone();
        let mut terminals = HashMap::new();
        terminals.insert(
            terminal_id.clone(),
            TerminalState::new(terminal_id, PathBuf::from("/hako-test/pion")),
        );
        let terminal_runtimes = TerminalRuntimeRegistry::new();

        assert_eq!(ws.display_name(), "original");

        assert_eq!(ws.display_name_from(&terminals, &terminal_runtimes), "pion");
        assert_eq!(
            ws.resolved_identity_cwd_from(&terminals, &terminal_runtimes),
            Some(PathBuf::from("/hako-test/pion"))
        );
    }

    #[test]
    fn workspace_manual_name_overrides_live_runtime_cwd() {
        let mut ws = Workspace::test_new("manual");
        ws.identity_cwd = PathBuf::from("/hako-test/original");
        let root_pane = ws.tabs[0].root_pane;
        let terminal_id = ws.tabs[0].terminal_id(root_pane).unwrap().clone();
        let mut terminals = HashMap::new();
        terminals.insert(
            terminal_id.clone(),
            TerminalState::new(terminal_id, PathBuf::from("/hako-test/live")),
        );
        let terminal_runtimes = TerminalRuntimeRegistry::new();

        assert_eq!(
            ws.display_name_from(&terminals, &terminal_runtimes),
            "manual"
        );
        assert_eq!(
            ws.resolved_identity_cwd_from(&terminals, &terminal_runtimes),
            Some(PathBuf::from("/hako-test/live"))
        );
    }

    #[test]
    fn git_work_summary_label_describes_shell_clean_and_dirty_spaces_without_clean_noise() {
        let mut ws = Workspace::test_new("test");
        assert_eq!(ws.git_work_summary_label(), "");

        ws.cached_git_work_summary = Some(GitWorkSummary {
            repo_count: 1,
            ..GitWorkSummary::default()
        });
        assert_eq!(ws.git_work_summary_label(), "");

        ws.cached_git_work_summary = Some(GitWorkSummary {
            repo_count: 2,
            ..GitWorkSummary::default()
        });
        assert_eq!(ws.git_work_summary_label(), "2 repos");

        ws.cached_git_work_summary = Some(GitWorkSummary {
            repo_count: 2,
            added: 2,
            modified: 1,
            deleted: 1,
            ..GitWorkSummary::default()
        });
        assert_eq!(ws.git_work_summary_label(), "2 repos · +2 ~1 -1");
    }

    #[test]
    fn moving_tab_keeps_active_identity_and_renumbers_auto_tabs() {
        let mut ws = Workspace::test_new("test");
        let moved_root = ws.tabs[0].root_pane;
        ws.test_add_tab(Some("foo"));
        let final_auto_idx = ws.test_add_tab(None);
        let active_root = ws.tabs[final_auto_idx].root_pane;
        ws.switch_tab(final_auto_idx);

        assert!(ws.move_tab(0, ws.tabs.len()));

        let labels: Vec<_> = ws.tabs.iter().map(|tab| tab.display_name()).collect();
        assert_eq!(labels, vec!["foo", "2", "3"]);
        assert_eq!(ws.tabs[0].custom_name.as_deref(), Some("foo"));
        assert!(ws.tabs[1].custom_name.is_none());
        assert!(ws.tabs[2].custom_name.is_none());
        assert_eq!(ws.tabs[2].root_pane, moved_root);
        assert_eq!(ws.tabs[ws.active_tab].root_pane, active_root);
    }

    #[tokio::test]
    async fn workspace_can_create_tab_after_all_tabs_are_closed() {
        let mut ws = Workspace::test_new("test");
        assert!(ws.close_tab_allow_empty(0));
        assert!(ws.tabs.is_empty());

        let (events, _) = mpsc::channel(64);
        let render_notify = Arc::new(Notify::new());
        let render_dirty = Arc::new(AtomicBool::new(false));
        let cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());

        let (tab_idx, _terminal, _runtime) = ws
            .create_tab_with_handles(
                24,
                80,
                cwd,
                0,
                crate::terminal_theme::TerminalTheme::default(),
                crate::pane::PaneShellConfig::new("", crate::config::ShellModeConfig::NonLogin),
                events,
                render_notify,
                render_dirty,
            )
            .expect("empty workspace creates new tab");

        assert_eq!(tab_idx, 0);
        assert_eq!(ws.tabs.len(), 1);
        assert_eq!(ws.active_tab, 0);
        assert_eq!(ws.tabs[0].number, 1);
    }
}
