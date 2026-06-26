//! Pure state mutations on AppState.
//! These don't need channels, async, or PTY runtime.

use tracing::{info, warn};

use crate::detect::{Agent, AgentState};
use crate::events::AppEvent;
use crate::layout::{find_in_direction, NavDirection, PaneId};
use crate::selection::Selection;
use crate::terminal::{EffectiveStateChange, TerminalStateMutation};
#[cfg(test)]
use crate::workspace::GitWorkSummary;
use crate::workspace::WorkspaceGitStatus;
use unicode_width::UnicodeWidthChar;

use super::state::{
    AgentNotificationDelivery, AppState, Group, Mode, NavigatorRow, NavigatorStateFilter,
    NavigatorTarget, Palette, PaneFocusTarget, PendingAgentNotification, ToastKind,
    ToastNotification, ToastTarget, ViewLayout,
};

fn hunk_diff_project_command(
    root: std::path::PathBuf,
    palette: &crate::app::state::Palette,
    appearance: crate::terminal_theme::ThemeAppearance,
    terminal_theme: crate::terminal_theme::TerminalTheme,
    passthrough_terminal: bool,
) -> crate::commands::ProjectCommand {
    let repo_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(|name| format!("diff · {name}"))
        .unwrap_or_else(|| "diff".to_string());
    crate::commands::ProjectCommand {
        id: format!("builtin:git-diff:{}", root.display()),
        root,
        source: crate::commands::CommandSource::BuiltIn,
        name: repo_name,
        command: crate::hunk_theme::command(
            palette,
            appearance,
            terminal_theme,
            passthrough_terminal,
        ),
        confidence: crate::commands::CommandConfidence::Explicit,
    }
}

fn observed_git_repos_from_cwd(cwd: &std::path::Path) -> Vec<std::path::PathBuf> {
    if let Some(root) = crate::workspace::git_repo_root(cwd) {
        return vec![root];
    }

    let Ok(entries) = std::fs::read_dir(cwd) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            file_type.is_dir().then(|| entry.path())
        })
        .filter_map(|path| crate::workspace::git_repo_root(&path))
        .collect()
}

fn hunk_uses_terminal_color_passthrough(theme_name: &str) -> bool {
    matches!(theme_name, "system" | "terminal")
}

fn is_background_completion_transition(prev_state: AgentState, new_state: AgentState) -> bool {
    matches!(new_state, AgentState::Idle)
        && matches!(prev_state, AgentState::Working | AgentState::Blocked)
}

pub fn active_tab_suppresses_notifications(
    is_active_tab: bool,
    outer_terminal_focus: Option<bool>,
) -> bool {
    is_active_tab && outer_terminal_focus != Some(false)
}

pub fn notification_sound_for_state_change(
    suppress_active_tab_notifications: bool,
    prev_state: AgentState,
    new_state: AgentState,
) -> Option<crate::sound::Sound> {
    if new_state == prev_state {
        return None;
    }

    match new_state {
        AgentState::Blocked => Some(crate::sound::Sound::Request),
        AgentState::Idle
            if is_background_completion_transition(prev_state, new_state)
                && !suppress_active_tab_notifications =>
        {
            Some(crate::sound::Sound::Done)
        }
        _ => None,
    }
}

pub fn notification_toast_for_state_change(
    suppress_active_tab_notifications: bool,
    prev_state: AgentState,
    new_state: AgentState,
) -> Option<ToastKind> {
    if suppress_active_tab_notifications || new_state == prev_state {
        return None;
    }

    match new_state {
        AgentState::Blocked => Some(ToastKind::NeedsAttention),
        AgentState::Idle if is_background_completion_transition(prev_state, new_state) => {
            Some(ToastKind::Finished)
        }
        _ => None,
    }
}

fn toast_agent_label(agent_label: &str) -> &str {
    agent_label
}

fn toast_event_text(kind: ToastKind) -> &'static str {
    match kind {
        ToastKind::NeedsAttention => "needs attention",
        ToastKind::Finished => "finished",
        ToastKind::UpdateInstalled => "updated",
    }
}

fn sound_for_toast_kind(
    kind: ToastKind,
    suppress_active_tab_notifications: bool,
) -> Option<crate::sound::Sound> {
    match kind {
        ToastKind::NeedsAttention => Some(crate::sound::Sound::Request),
        ToastKind::Finished if !suppress_active_tab_notifications => {
            Some(crate::sound::Sound::Done)
        }
        ToastKind::Finished | ToastKind::UpdateInstalled => None,
    }
}

pub fn notification_context(
    ws: &crate::workspace::Workspace,
    workspace_label: &str,
    ws_idx: usize,
    pane_id: PaneId,
) -> String {
    let mut context = format!("{} · {}", workspace_label, ws_idx + 1);
    if ws.tabs.len() > 1 {
        if let Some(tab_idx) = ws.find_tab_index_for_pane(pane_id) {
            let tab = &ws.tabs[tab_idx];
            context.push_str(&format!(" · {}", tab.display_name()));
        }
    }
    context
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneStateUpdate {
    pub pane_id: PaneId,
    pub ws_idx: usize,
    pub previous_agent_label: Option<String>,
    pub previous_known_agent: Option<Agent>,
    pub previous_state: AgentState,
    pub previous_presentation: crate::terminal::EffectivePresentation,
    pub agent_label: Option<String>,
    pub known_agent: Option<Agent>,
    pub state: AgentState,
    pub presentation: crate::terminal::EffectivePresentation,
}

// ---------------------------------------------------------------------------
// Navigator operations
// ---------------------------------------------------------------------------

impl AppState {
    pub(crate) fn current_pane_focus_target(&self) -> Option<PaneFocusTarget> {
        let ws_idx = self.active?;
        let ws = self.workspaces.get(ws_idx)?;
        let pane_id = ws.focused_pane_id()?;
        Some(PaneFocusTarget {
            workspace_id: ws.id.clone(),
            pane_id,
        })
    }

    fn pane_focus_target_indices(&self, target: &PaneFocusTarget) -> Option<(usize, usize)> {
        let ws_idx = self
            .workspaces
            .iter()
            .position(|ws| ws.id == target.workspace_id)?;
        let tab_idx = self.workspaces[ws_idx].find_tab_index_for_pane(target.pane_id)?;
        Some((ws_idx, tab_idx))
    }

    pub(crate) fn record_pane_focus_change(
        &mut self,
        previous: Option<PaneFocusTarget>,
        ws_idx: usize,
        pane_id: PaneId,
    ) {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return;
        };
        let target = PaneFocusTarget {
            workspace_id: ws.id.clone(),
            pane_id,
        };
        if previous.as_ref() != Some(&target) {
            self.previous_pane_focus = previous;
        }
    }

    fn record_pane_focus_after_navigation(&mut self, previous: Option<PaneFocusTarget>) {
        let current = self.current_pane_focus_target();
        if previous != current {
            self.previous_pane_focus = previous;
        }
    }

    pub(crate) fn focus_pane_in_workspace(&mut self, ws_idx: usize, pane_id: PaneId) -> bool {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return false;
        };
        let Some(tab_idx) = ws.find_tab_index_for_pane(pane_id) else {
            return false;
        };
        let previous = self.current_pane_focus_target();
        let target = PaneFocusTarget {
            workspace_id: ws.id.clone(),
            pane_id,
        };
        if previous.as_ref() == Some(&target) {
            return false;
        }

        self.switch_workspace_tab(ws_idx, tab_idx);
        if let Some(tab) = self
            .workspaces
            .get_mut(ws_idx)
            .and_then(|ws| ws.tabs.get_mut(tab_idx))
        {
            tab.layout.focus_pane(pane_id);
            self.previous_pane_focus = previous;
            self.mark_session_dirty();
            return true;
        }
        false
    }

    pub(crate) fn open_navigator(&mut self) {
        self.navigator.query.clear();
        self.navigator.search_focused = false;
        self.navigator.state_filter = None;
        self.navigator.scroll = 0;
        self.navigator.expanded_workspaces.clear();

        for ws in &self.workspaces {
            self.navigator.expanded_workspaces.insert(ws.id.clone());
        }

        self.mode = Mode::Navigator;
        self.navigator.selected = self.current_navigator_row_index().unwrap_or(0);
        self.ensure_navigator_selection_visible();
    }
    #[cfg(test)]
    pub(crate) fn open_navigator_from(
        &mut self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) {
        self.navigator.query.clear();
        self.navigator.search_focused = false;
        self.navigator.state_filter = None;
        self.navigator.scroll = 0;
        self.navigator.expanded_workspaces.clear();

        for ws in &self.workspaces {
            self.navigator.expanded_workspaces.insert(ws.id.clone());
        }

        self.mode = Mode::Navigator;
        self.navigator.selected = self
            .current_navigator_row_index_from(terminal_runtimes)
            .unwrap_or(0);
        self.ensure_navigator_selection_visible();
    }

    pub(crate) fn navigator_rows(&self) -> Vec<NavigatorRow> {
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        self.navigator_rows_from(&terminal_runtimes)
    }

    pub(crate) fn navigator_rows_from(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> Vec<NavigatorRow> {
        let query = self.navigator.query.trim().to_lowercase();
        let query_kind = navigator_query_kind(&query, self.navigator.state_filter);
        let mut rows = Vec::new();
        for (ws_idx, ws) in self.workspaces.iter().enumerate() {
            let workspace_label = ws.display_name_from(&self.terminals, terminal_runtimes);
            let activity = workspace_activity_summary(ws, &self.terminals);
            let workspace_search_text = format!("{workspace_label} {activity}").to_lowercase();
            let workspace_matches = match query_kind {
                NavigatorQueryKind::Empty => true,
                NavigatorQueryKind::State(filter) => {
                    let (state, seen) = ws.aggregate_state(&self.terminals);
                    navigator_state_filter_matches(filter, state, seen)
                }
                NavigatorQueryKind::Text => navigator_matches(&query, &workspace_search_text),
            };

            let child_rows = self.navigator_child_rows(ws_idx, query_kind, &query);
            if !workspace_matches && child_rows.is_empty() {
                continue;
            }

            let expanded = !matches!(query_kind, NavigatorQueryKind::Empty)
                || self.navigator.expanded_workspaces.contains(&ws.id);
            let (state, seen) = ws.aggregate_state(&self.terminals);
            let pane_count = ws.tabs.iter().map(|tab| tab.panes.len()).sum::<usize>();
            rows.push(NavigatorRow {
                target: NavigatorTarget::Workspace { ws_idx },
                depth: 0,
                label: format!("{workspace_label} ({pane_count})"),
                meta: activity,
                status: state,
                seen,
                is_current: self.active == Some(ws_idx),
                is_workspace: true,
                is_tab: false,
                expanded,
                search_text: workspace_search_text,
            });
            if expanded {
                rows.extend(child_rows);
            }
        }
        rows
    }

    fn navigator_child_rows(
        &self,
        ws_idx: usize,
        query_kind: NavigatorQueryKind,
        query: &str,
    ) -> Vec<NavigatorRow> {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return Vec::new();
        };
        let multi_tab = ws.tabs.len() > 1;
        let mut rows = Vec::new();
        for tab_idx in 0..ws.tabs.len() {
            let tab_row = multi_tab.then(|| self.navigator_tab_row(ws_idx, tab_idx));
            let tab_matches = tab_row.as_ref().is_some_and(|row| match query_kind {
                NavigatorQueryKind::Empty => true,
                NavigatorQueryKind::State(filter) => {
                    navigator_state_filter_matches(filter, row.status, row.seen)
                }
                NavigatorQueryKind::Text => navigator_matches(query, &row.search_text),
            });
            let pane_rows = self.navigator_pane_rows_for_tab(ws_idx, tab_idx, multi_tab);
            let filtered_panes = match query_kind {
                NavigatorQueryKind::Empty => pane_rows,
                NavigatorQueryKind::State(filter) => pane_rows
                    .into_iter()
                    .filter(|row| navigator_state_filter_matches(filter, row.status, row.seen))
                    .collect::<Vec<_>>(),
                NavigatorQueryKind::Text if tab_matches => pane_rows,
                NavigatorQueryKind::Text => pane_rows
                    .into_iter()
                    .filter(|row| navigator_matches(query, &row.search_text))
                    .collect::<Vec<_>>(),
            };

            if let Some(tab_row) = tab_row {
                if tab_matches || !filtered_panes.is_empty() {
                    rows.push(tab_row);
                }
            }
            rows.extend(filtered_panes);
        }
        rows
    }

    fn navigator_tab_row(&self, ws_idx: usize, tab_idx: usize) -> NavigatorRow {
        let ws = &self.workspaces[ws_idx];
        let tab = &ws.tabs[tab_idx];
        let label = tab.display_name();
        let (status, seen) = tab_aggregate_state(tab, &self.terminals);
        let activity = tab_activity_summary(tab, &self.terminals);
        let pane_count = tab.panes.len();
        let meta = if activity.is_empty() {
            format!("{pane_count} panes")
        } else {
            format!("{pane_count} panes · {activity}")
        };
        let search_text = format!("{label} {meta}").to_lowercase();
        NavigatorRow {
            target: NavigatorTarget::Tab { ws_idx, tab_idx },
            depth: 1,
            label,
            meta,
            status,
            seen,
            is_current: false,
            is_workspace: false,
            is_tab: true,
            expanded: true,
            search_text,
        }
    }

    fn navigator_pane_rows_for_tab(
        &self,
        ws_idx: usize,
        tab_idx: usize,
        multi_tab: bool,
    ) -> Vec<NavigatorRow> {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return Vec::new();
        };
        let Some(tab) = ws.tabs.get(tab_idx) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for pane_id in tab.layout.pane_ids() {
            let Some(pane) = tab.panes.get(&pane_id) else {
                continue;
            };
            let terminal = self.terminals.get(&pane.attached_terminal_id);
            let pane_number = ws.public_pane_number(pane_id).unwrap_or(0);
            let label = terminal
                .and_then(|terminal| terminal.effective_title())
                .or_else(|| {
                    terminal
                        .and_then(|terminal| terminal.manual_label.as_deref().map(str::to_string))
                })
                .or_else(|| {
                    terminal.and_then(|terminal| terminal.agent_name.as_deref().map(str::to_string))
                })
                .or_else(|| {
                    terminal
                        .and_then(|terminal| terminal.effective_agent_label().map(str::to_string))
                })
                .or_else(|| {
                    launch_label(terminal.and_then(|terminal| terminal.launch_argv.as_ref()))
                })
                .unwrap_or_else(|| format!("pane {pane_number}"));
            let display_agent = terminal.and_then(|terminal| terminal.effective_display_agent());
            let agent_label = display_agent.as_deref().or_else(|| {
                terminal
                    .and_then(|terminal| terminal.agent_name.as_deref())
                    .or_else(|| terminal.and_then(|terminal| terminal.effective_agent_label()))
            });
            let custom_status = terminal.and_then(|terminal| terminal.effective_custom_status());
            let state = terminal
                .map(|terminal| terminal.state)
                .unwrap_or(AgentState::Unknown);
            let status_label = terminal
                .map(|terminal| terminal.effective_presentation().state_labels)
                .and_then(|labels| labels.get(state_label_text(state, pane.seen)).cloned());
            let status = custom_status
                .or(status_label)
                .or_else(|| agent_label.map(|_| state_label_text(state, pane.seen).to_string()));
            let meta = match (agent_label, status.as_deref()) {
                (Some(agent_label), Some(status)) => format!("{agent_label} · {status}"),
                (Some(agent_label), None) => agent_label.to_string(),
                (None, _) => "shell".to_string(),
            };
            let is_current = self.is_active_pane(ws_idx, tab_idx, pane_id);
            let search_text = format!("{label} {meta}").to_lowercase();
            rows.push(NavigatorRow {
                target: NavigatorTarget::Pane {
                    ws_idx,
                    tab_idx,
                    pane_id,
                },
                depth: if multi_tab { 2 } else { 1 },
                label,
                meta,
                status: state,
                seen: pane.seen,
                is_current,
                is_workspace: false,
                is_tab: false,
                expanded: false,
                search_text,
            });
        }
        rows
    }

    fn current_navigator_row_index(&self) -> Option<usize> {
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        self.current_navigator_row_index_from(&terminal_runtimes)
    }

    fn current_navigator_row_index_from(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> Option<usize> {
        let rows = self.navigator_rows_from(terminal_runtimes);
        rows.iter()
            .position(|row| matches!(row.target, NavigatorTarget::Pane { .. }) && row.is_current)
            .or_else(|| rows.iter().position(|row| row.is_current))
    }

    pub(crate) fn ensure_navigator_selection_visible(&mut self) {
        let body = self.navigator_body_rect();
        let viewport = body.height as usize;
        if viewport == 0 {
            self.navigator.scroll = 0;
            return;
        }
        let max_scroll = self.navigator_max_scroll(viewport);
        if self.navigator.selected < self.navigator.scroll {
            self.navigator.scroll = self.navigator.selected;
        } else if self.navigator.selected >= self.navigator.scroll.saturating_add(viewport) {
            self.navigator.scroll = self
                .navigator
                .selected
                .saturating_add(1)
                .saturating_sub(viewport);
        }
        self.navigator.scroll = self.navigator.scroll.min(max_scroll);
    }

    pub(crate) fn navigator_max_scroll(&self, viewport: usize) -> usize {
        if viewport == 0 {
            return 0;
        }
        self.navigator_rows().len().saturating_sub(viewport)
    }

    pub(crate) fn move_navigator_selection(&mut self, delta: isize) {
        let count = self.navigator_rows().len();
        if count == 0 {
            self.navigator.selected = 0;
            self.navigator.scroll = 0;
            return;
        }
        let current = self.navigator.selected.min(count - 1) as isize;
        self.navigator.selected = (current + delta).clamp(0, count as isize - 1) as usize;
        self.ensure_navigator_selection_visible();
    }

    pub(crate) fn clamp_navigator_selection(&mut self) {
        let count = self.navigator_rows().len();
        self.navigator.selected = self.navigator.selected.min(count.saturating_sub(1));
        self.ensure_navigator_selection_visible();
    }

    pub(crate) fn toggle_selected_navigator_workspace(&mut self) {
        let Some(row) = self.navigator_rows().get(self.navigator.selected).cloned() else {
            return;
        };
        let NavigatorTarget::Workspace { ws_idx } = row.target else {
            return;
        };
        let Some(workspace_id) = self.workspaces.get(ws_idx).map(|ws| ws.id.clone()) else {
            return;
        };
        if self.navigator.expanded_workspaces.contains(&workspace_id) {
            self.navigator.expanded_workspaces.remove(&workspace_id);
        } else {
            self.navigator.expanded_workspaces.insert(workspace_id);
        }
        self.clamp_navigator_selection();
    }

    pub(crate) fn accept_navigator_selection(&mut self) -> bool {
        let Some(row) = self.navigator_rows().get(self.navigator.selected).cloned() else {
            return false;
        };
        self.focus_navigator_target(row.target)
    }

    pub(crate) fn focus_navigator_target(&mut self, target: NavigatorTarget) -> bool {
        match target {
            NavigatorTarget::Workspace { ws_idx } => {
                if ws_idx >= self.workspaces.len() {
                    return false;
                }
                self.switch_workspace(ws_idx);
                self.mode = Mode::Terminal;
                true
            }
            NavigatorTarget::Tab { ws_idx, tab_idx } => {
                if ws_idx >= self.workspaces.len() {
                    return false;
                }
                let tab_exists = self
                    .workspaces
                    .get(ws_idx)
                    .is_some_and(|ws| tab_idx < ws.tabs.len());
                if !tab_exists {
                    return false;
                }
                self.switch_workspace_tab(ws_idx, tab_idx);
                self.mode = Mode::Terminal;
                true
            }
            NavigatorTarget::Pane {
                ws_idx,
                tab_idx,
                pane_id,
            } => {
                if ws_idx >= self.workspaces.len() {
                    return false;
                }
                if self
                    .workspaces
                    .get(ws_idx)
                    .and_then(|ws| ws.tabs.get(tab_idx))
                    .is_some_and(|tab| tab.panes.contains_key(&pane_id))
                {
                    self.focus_pane_in_workspace(ws_idx, pane_id);
                    self.mode = Mode::Terminal;
                    return true;
                }
                false
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavigatorQueryKind {
    Empty,
    Text,
    State(NavigatorStateFilter),
}

fn navigator_query_kind(
    query: &str,
    state_filter: Option<NavigatorStateFilter>,
) -> NavigatorQueryKind {
    if let Some(filter) = state_filter {
        return NavigatorQueryKind::State(filter);
    }
    if query.is_empty() {
        NavigatorQueryKind::Empty
    } else {
        NavigatorQueryKind::Text
    }
}

fn navigator_state_filter_matches(
    filter: NavigatorStateFilter,
    state: AgentState,
    seen: bool,
) -> bool {
    match filter {
        NavigatorStateFilter::Blocked => state == AgentState::Blocked,
        NavigatorStateFilter::Working => state == AgentState::Working,
        NavigatorStateFilter::Idle => state == AgentState::Idle && seen,
        NavigatorStateFilter::Done => state == AgentState::Idle && !seen,
    }
}

fn text_matches_query(query: &str, text: &str) -> bool {
    let haystack = text.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|needle| haystack.contains(needle))
}

fn navigator_matches(query: &str, text: &str) -> bool {
    text_matches_query(query, text)
}

fn launch_label(argv: Option<&Vec<String>>) -> Option<String> {
    let argv = argv?;
    let command = argv.first()?;
    std::path::Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .or_else(|| Some(command.clone()))
}

fn unix_secs_for_activity_instant(activity_at: std::time::Instant) -> u64 {
    let now_instant = std::time::Instant::now();
    let now_system = std::time::SystemTime::now();
    let activity_system = if activity_at <= now_instant {
        now_system
            .checked_sub(now_instant.duration_since(activity_at))
            .unwrap_or(now_system)
    } else {
        now_system
            .checked_add(activity_at.duration_since(now_instant))
            .unwrap_or(now_system)
    };

    activity_system
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn state_label_text(state: AgentState, seen: bool) -> &'static str {
    match (state, seen) {
        (AgentState::Blocked, _) => "blocked",
        (AgentState::Working, _) => "working",
        (AgentState::Idle, false) => "done",
        (AgentState::Idle, true) => "idle",
        (AgentState::Unknown, _) => "unknown",
    }
}

fn tab_aggregate_state(
    tab: &crate::workspace::Tab,
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
) -> (AgentState, bool) {
    let mut aggregate = AgentState::Unknown;
    let mut seen = true;
    for pane in tab.panes.values() {
        let Some(terminal) = terminals.get(&pane.attached_terminal_id) else {
            continue;
        };
        if state_priority(terminal.state, pane.seen) > state_priority(aggregate, seen) {
            aggregate = terminal.state;
            seen = pane.seen;
        }
    }
    (aggregate, seen)
}

fn state_priority(state: AgentState, seen: bool) -> u8 {
    match (state, seen) {
        (AgentState::Blocked, _) => 5,
        (AgentState::Working, _) => 4,
        (AgentState::Idle, false) => 3,
        (AgentState::Idle, true) => 2,
        (AgentState::Unknown, _) => 1,
    }
}

fn tab_activity_summary(
    tab: &crate::workspace::Tab,
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
) -> String {
    activity_summary_for_panes(tab.panes.values(), terminals)
}

fn workspace_activity_summary(
    ws: &crate::workspace::Workspace,
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
) -> String {
    activity_summary_for_panes(ws.tabs.iter().flat_map(|tab| tab.panes.values()), terminals)
}

fn activity_summary_for_panes<'a>(
    panes: impl Iterator<Item = &'a crate::pane::PaneState>,
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
) -> String {
    let mut blocked = 0usize;
    let mut working = 0usize;
    let mut done = 0usize;
    for pane in panes {
        let Some(terminal) = terminals.get(&pane.attached_terminal_id) else {
            continue;
        };
        match (terminal.state, pane.seen) {
            (AgentState::Blocked, _) => blocked += 1,
            (AgentState::Working, _) => working += 1,
            (AgentState::Idle, false) => done += 1,
            _ => {}
        }
    }

    let mut parts = Vec::new();
    if blocked > 0 {
        parts.push(format!("{blocked} blocked"));
    }
    if working > 0 {
        parts.push(format!("{working} working"));
    }
    if done > 0 {
        parts.push(format!("{done} done"));
    }
    parts.join(" · ")
}

// ---------------------------------------------------------------------------
// Workspace operations
// ---------------------------------------------------------------------------

impl AppState {
    fn command_target_for_root(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        root: &std::path::Path,
    ) -> Option<(usize, usize, PaneId)> {
        for (ws_idx, workspace) in self.workspaces.iter().enumerate() {
            for (tab_idx, tab) in workspace.tabs.iter().enumerate() {
                for pane_id in tab.layout.pane_ids() {
                    let Some(cwd) = tab.cwd_for_pane(pane_id, &self.terminals, terminal_runtimes)
                    else {
                        continue;
                    };
                    if crate::commands::project_root_from_cwd(&cwd) == root {
                        return Some((ws_idx, tab_idx, pane_id));
                    }
                }
            }
        }
        None
    }

    fn command_terminal_target(
        &self,
        terminal_id: &crate::terminal::TerminalId,
    ) -> Option<(usize, usize, PaneId)> {
        for (ws_idx, workspace) in self.workspaces.iter().enumerate() {
            for (tab_idx, tab) in workspace.tabs.iter().enumerate() {
                for (pane_id, pane) in &tab.panes {
                    if &pane.attached_terminal_id == terminal_id {
                        return Some((ws_idx, tab_idx, *pane_id));
                    }
                }
            }
        }
        None
    }

    pub(crate) fn focus_command_run(&mut self, command_id: &str) -> bool {
        let Some(terminal_id) = self
            .command_runs
            .get(command_id)
            .map(|run| run.terminal_id.clone())
        else {
            return false;
        };
        let Some((ws_idx, tab_idx, pane_id)) = self.command_terminal_target(&terminal_id) else {
            if let Some(run) = self.command_runs.get_mut(command_id) {
                run.status = crate::commands::CommandRunStatus::Unknown;
            }
            return false;
        };

        self.switch_workspace(ws_idx);
        self.switch_tab(tab_idx);
        self.focus_pane(pane_id);
        self.mode = Mode::Terminal;
        true
    }

    pub(crate) fn run_project_command(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        command_id: &str,
    ) -> Result<(), String> {
        let command = self
            .command_catalog
            .iter()
            .find(|command| command.id == command_id)
            .cloned()
            .ok_or_else(|| format!("command {command_id} not found"))?;

        let (ws_idx, _, _) = self
            .command_target_for_root(terminal_runtimes, &command.root)
            .ok_or_else(|| format!("no pane for {}", command.root.display()))?;

        self.run_project_command_entry(terminal_runtimes, command, ws_idx)
    }

    pub(crate) fn open_git_diff_panel(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
    ) -> Result<(), String> {
        let fallback_ws_idx = if matches!(self.mode, Mode::Navigate) {
            Some(self.selected)
        } else {
            self.active
        };
        let ws_idx = self
            .requested_git_diff_workspace
            .take()
            .or(fallback_ws_idx)
            .ok_or_else(|| "no git repo for current space".to_string())?;
        self.open_git_diff_panel_for_workspace(terminal_runtimes, ws_idx)
    }

    fn open_git_diff_panel_for_workspace(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        ws_idx: usize,
    ) -> Result<(), String> {
        let roots = self.observed_git_repos_for_workspace(terminal_runtimes, ws_idx);
        let root = match roots.as_slice() {
            [] => return Err("no git repo for current space".to_string()),
            [root] => root.clone(),
            _ => {
                self.git_repo_picker.ws_idx = ws_idx;
                self.git_repo_picker.roots = roots;
                self.git_repo_picker.selected = 0;
                self.git_repo_picker.scroll = 0;
                self.mode = Mode::GitRepoPicker;
                return Ok(());
            }
        };
        self.open_native_git_diff_tab(root, ws_idx)
    }

    fn open_native_git_diff_tab(
        &mut self,
        root: std::path::PathBuf,
        ws_idx: usize,
    ) -> Result<(), String> {
        let session = crate::native_diff::load_native_diff_session(&root).map_err(|err| err.0)?;
        let workspace = self
            .workspaces
            .get_mut(ws_idx)
            .ok_or_else(|| "diff workspace disappeared".to_string())?;
        let tab_idx = workspace.create_native_diff_tab(session)?;
        if let Some(tab) = workspace.tabs.get_mut(tab_idx) {
            let name = root
                .file_name()
                .and_then(|name| name.to_str())
                .map_or_else(|| "diff".to_string(), |name| format!("diff {name}"));
            tab.set_custom_name(name);
            if let Some(diff) = tab
                .panes
                .values_mut()
                .find_map(|pane| pane.native_diff_mut())
            {
                diff.wrap_lines = self.native_diff_wrap_lines;
            }
        }
        self.switch_workspace(ws_idx);
        self.switch_tab(tab_idx);
        self.mode = Mode::Terminal;
        self.mark_session_dirty();
        Ok(())
    }

    fn hunk_diff_theme_for_workspace(
        &self,
        ws_idx: usize,
    ) -> (
        crate::app::state::Palette,
        crate::terminal_theme::ThemeAppearance,
        bool,
    ) {
        let mut palette = self.global_palette.clone();
        if let Some(accent) = self
            .workspaces
            .get(ws_idx)
            .and_then(|workspace| self.group_index_for_id(&workspace.group_id))
            .and_then(|group_idx| self.groups.get(group_idx))
            .and_then(|group| group.accent)
        {
            palette.accent = Palette::terminal_accent_color(self.host_terminal_theme, accent);
        }

        let appearance = self.theme_appearance_for_mode(self.global_theme_mode);
        (
            palette,
            appearance,
            hunk_uses_terminal_color_passthrough(&self.global_theme_name),
        )
    }

    fn run_project_command_entry(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        command: crate::commands::ProjectCommand,
        ws_idx: usize,
    ) -> Result<(), String> {
        let command_id = command.id.clone();
        if let Some(run) = self.command_runs.get(&command_id).cloned() {
            if run.status == crate::commands::CommandRunStatus::Running
                && self.focus_command_run(&command.id)
            {
                return Ok(());
            }
            if let Some((ws_idx, tab_idx, pane_id)) = self.command_terminal_target(&run.terminal_id)
            {
                self.restart_command_in_tab(
                    terminal_runtimes,
                    &command,
                    &run.terminal_id,
                    ws_idx,
                    tab_idx,
                    pane_id,
                )?;
                return Ok(());
            }
            if let Some(run) = self.command_runs.get_mut(&command_id) {
                run.status = crate::commands::CommandRunStatus::Unknown;
            }
        }

        self.open_command_tab(terminal_runtimes, command, ws_idx)
    }

    pub(crate) fn observed_git_repos_for_workspace(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        ws_idx: usize,
    ) -> Vec<std::path::PathBuf> {
        let mut roots = self
            .workspaces
            .get(ws_idx)
            .into_iter()
            .flat_map(|workspace| {
                workspace.git_status_cwds_from(&self.terminals, terminal_runtimes)
            })
            .flat_map(|cwd| observed_git_repos_from_cwd(&cwd))
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();
        roots
    }

    pub(crate) fn open_selected_git_diff_panel(
        &mut self,
        _terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
    ) -> Result<(), String> {
        let Some(root) = self
            .git_repo_picker
            .roots
            .get(self.git_repo_picker.selected)
            .cloned()
        else {
            return Err("no git repo selected".to_string());
        };
        let ws_idx = self.git_repo_picker.ws_idx;
        self.open_native_git_diff_tab(root, ws_idx)
    }

    #[cfg(test)]
    fn git_diff_target_for_workspace(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        ws_idx: usize,
    ) -> Option<std::path::PathBuf> {
        self.observed_git_repos_for_workspace(terminal_runtimes, ws_idx)
            .into_iter()
            .next()
    }

    #[cfg(test)]
    fn git_diff_target(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> Option<(std::path::PathBuf, usize)> {
        let ws_idx = if matches!(self.mode, Mode::Navigate) {
            self.selected
        } else {
            self.active?
        };
        self.git_diff_target_for_workspace(terminal_runtimes, ws_idx)
            .map(|root| (root, ws_idx))
    }

    fn open_command_tab(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        command: crate::commands::ProjectCommand,
        ws_idx: usize,
    ) -> Result<(), String> {
        let (rows, cols) = self.estimate_pane_size();
        let workspace = self
            .workspaces
            .get_mut(ws_idx)
            .ok_or_else(|| "command workspace disappeared".to_string())?;
        let (tab_idx, terminal, runtime) = workspace
            .create_command_tab(
                rows.max(4),
                cols.max(10),
                command.root.clone(),
                &command.command,
                &[],
                self.pane_scrollback_limit_bytes,
                self.host_terminal_theme,
            )
            .map_err(|err| err.to_string())?;
        if let Some(tab) = workspace.tabs.get_mut(tab_idx) {
            tab.set_custom_name(command.name.clone());
        }
        let terminal_id = terminal.id.clone();
        terminal_runtimes.insert(terminal.id.clone(), runtime);
        self.terminals.insert(terminal.id.clone(), terminal);

        self.command_runs.insert(
            command.id.clone(),
            crate::commands::CommandRun {
                command_id: command.id,
                terminal_id,
                status: crate::commands::CommandRunStatus::Running,
            },
        );

        self.switch_workspace(ws_idx);
        self.switch_tab(tab_idx);
        self.mode = Mode::Terminal;
        self.mark_session_dirty();
        Ok(())
    }

    fn restart_command_in_tab(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        command: &crate::commands::ProjectCommand,
        terminal_id: &crate::terminal::TerminalId,
        ws_idx: usize,
        tab_idx: usize,
        pane_id: PaneId,
    ) -> Result<(), String> {
        if let Some(runtime) = terminal_runtimes.remove(terminal_id) {
            runtime.shutdown();
        }

        let (rows, cols) = self.estimate_pane_size();
        let (events, render_notify, render_dirty) = {
            let tab = self
                .workspaces
                .get(ws_idx)
                .and_then(|workspace| workspace.tabs.get(tab_idx))
                .ok_or_else(|| "command tab disappeared".to_string())?;
            (
                tab.events.clone(),
                tab.render_notify.clone(),
                tab.render_dirty.clone(),
            )
        };
        let launch_env = self
            .workspaces
            .get(ws_idx)
            .and_then(|workspace| {
                let tab_number = workspace.public_tab_number(tab_idx)?;
                let pane_number = workspace.public_pane_number(pane_id)?;
                Some(
                    crate::pane::PaneLaunchEnv::from_extra(Vec::new()).with_identity(
                        workspace.id.clone(),
                        crate::workspace::public_tab_id_for_number(&workspace.id, tab_number),
                        crate::workspace::public_pane_id_for_number(&workspace.id, pane_number),
                    ),
                )
            })
            .ok_or_else(|| "command pane identity disappeared".to_string())?;
        let runtime = crate::terminal::TerminalRuntime::spawn_shell_command(
            pane_id,
            rows.max(4),
            cols.max(10),
            command.root.clone(),
            &command.command,
            &launch_env,
            self.pane_scrollback_limit_bytes,
            self.host_terminal_theme,
            events,
            render_notify,
            render_dirty,
        )
        .map_err(|err| err.to_string())?;
        terminal_runtimes.insert(terminal_id.clone(), runtime);
        if let Some(terminal) = self.terminals.get_mut(terminal_id) {
            terminal.cwd = command.root.clone();
        }
        if let Some(run) = self.command_runs.get_mut(&command.id) {
            run.status = crate::commands::CommandRunStatus::Running;
        }

        self.switch_workspace(ws_idx);
        self.switch_tab(tab_idx);
        self.focus_pane(pane_id);
        self.mode = Mode::Terminal;
        self.mark_session_dirty();
        Ok(())
    }

    pub fn stop_project_command(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        command_id: &str,
    ) -> bool {
        let Some(run) = self.command_runs.get_mut(command_id) else {
            return false;
        };
        if let Some(runtime) = terminal_runtimes.remove(&run.terminal_id) {
            runtime.shutdown();
            run.status = crate::commands::CommandRunStatus::Stopped;
        } else {
            run.status = crate::commands::CommandRunStatus::Unknown;
        }
        true
    }

    pub(crate) fn refresh_command_run_statuses(
        &mut self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> bool {
        let mut changed = false;
        for run in self.command_runs.values_mut() {
            if run.status != crate::commands::CommandRunStatus::Running {
                continue;
            }
            let alive = terminal_runtimes
                .get(&run.terminal_id)
                .map(|runtime| runtime.child_pid())
                .is_some_and(|pid| pid != 0 && crate::platform::process_exists(pid));
            if !alive {
                run.status = crate::commands::CommandRunStatus::Stopped;
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn command_scope_workspace_indices(&self) -> Vec<usize> {
        let idx = if matches!(self.mode, Mode::Navigate) {
            Some(self.selected)
        } else {
            self.active
        };
        idx.filter(|idx| self.workspaces.get(*idx).is_some())
            .into_iter()
            .collect()
    }

    pub(crate) fn refresh_command_catalog(
        &mut self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> bool {
        let mut roots = self
            .command_scope_workspace_indices()
            .into_iter()
            .filter_map(|ws_idx| self.workspaces.get(ws_idx))
            .flat_map(|ws| {
                ws.tabs.iter().flat_map(|tab| {
                    tab.layout.pane_ids().into_iter().filter_map(|pane_id| {
                        tab.cwd_for_pane(pane_id, &self.terminals, terminal_runtimes)
                    })
                })
            })
            .map(|cwd| crate::commands::project_root_from_cwd(&cwd))
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();

        let mut catalog = roots
            .into_iter()
            .flat_map(|root| {
                let mut commands = crate::commands::discover_project_commands(&root);
                if let Some(git_root) = crate::workspace::git_repo_root(&root) {
                    let (palette, appearance, passthrough_terminal) =
                        self.hunk_diff_theme_for_workspace(self.active.unwrap_or(self.selected));
                    commands.push(hunk_diff_project_command(
                        git_root,
                        &palette,
                        appearance,
                        self.host_terminal_theme,
                        passthrough_terminal,
                    ));
                }
                commands
            })
            .collect::<Vec<_>>();
        catalog.sort_by_key(|command| {
            (
                command.root.clone(),
                command.confidence,
                command.source,
                command.name.clone(),
            )
        });

        let changed = self.command_catalog != catalog;
        self.command_catalog = catalog;
        changed
    }

    fn group_index_for_id(&self, group_id: &str) -> Option<usize> {
        self.groups.iter().position(|group| group.id == group_id)
    }

    fn switch_to_group_index(&mut self, group_idx: usize) {
        if group_idx >= self.groups.len() {
            return;
        }

        self.active_group = group_idx;
        self.group_filter_enabled = true;
        self.apply_effective_theme();
        self.select_first_visible_workspace();
        self.mark_session_dirty();
    }

    pub fn apply_effective_theme(&mut self) {
        self.palette = self.global_palette.clone();
        self.theme_name = self.global_theme_name.clone();
    }
    pub fn preview_theme_with_mode(
        &mut self,
        theme_name: &str,
        mode: crate::config::ThemeMode,
    ) -> bool {
        let Some(palette) = self.palette_for_theme_mode(theme_name, mode) else {
            return false;
        };
        self.palette = palette;
        self.theme_name = theme_name.to_string();
        true
    }

    pub fn preview_theme_with_mode_and_terminal_accent(
        &mut self,
        theme_name: &str,
        mode: crate::config::ThemeMode,
        terminal_accent: crate::config::TerminalAccent,
    ) -> bool {
        let Some(palette) = self.palette_for_theme_mode_with_terminal_accents(
            theme_name,
            mode,
            terminal_accent,
            terminal_accent,
        ) else {
            return false;
        };
        self.palette = palette;
        self.theme_name = theme_name.to_string();
        true
    }

    pub fn set_group_accent(
        &mut self,
        group_idx: usize,
        accent: Option<crate::config::TerminalAccent>,
    ) -> bool {
        let Some(group) = self.groups.get_mut(group_idx) else {
            return false;
        };
        group.accent = accent;
        self.mark_session_dirty();
        self.apply_effective_theme();
        true
    }

    pub fn show_all_groups(&mut self) {
        self.group_filter_enabled = false;
        self.workspace_scroll = 0;
        self.agent_panel_scroll = 0;
        if self.active.is_none() {
            self.active = self.first_visible_workspace();
            self.selected = self.active.unwrap_or(0);
        }
        self.apply_effective_theme();
        self.mark_session_dirty();
        self.ensure_workspace_visible(self.selected);
    }

    pub fn toggle_group_filter(&mut self) {
        if self.group_filter_enabled {
            self.show_all_groups();
        } else {
            self.group_filter_enabled = true;
            self.select_first_visible_workspace();
            self.mark_session_dirty();
        }
    }

    fn select_first_visible_workspace(&mut self) {
        self.workspace_scroll = 0;
        self.agent_panel_scroll = 0;
        self.active = self.first_visible_workspace();
        self.selected = self.active.unwrap_or(0);
        self.tab_scroll_follow_active = true;
        if self.active.is_none() {
            self.tab_scroll = 0;
            if self.mode == Mode::Terminal {
                self.mode = Mode::Navigate;
            }
        }
        self.refresh_tab_bar_view();
    }

    pub fn switch_group(&mut self, group_idx: usize) {
        self.switch_to_group_index(group_idx);
    }

    pub fn previous_group(&mut self) {
        if self.groups.is_empty() {
            return;
        }
        let prev = if self.active_group == 0 {
            self.groups.len() - 1
        } else {
            self.active_group - 1
        };
        self.switch_group(prev);
    }

    pub fn next_group(&mut self) {
        if self.groups.is_empty() {
            return;
        }
        self.switch_group((self.active_group + 1) % self.groups.len());
    }

    pub fn create_group(&mut self, name: String) -> usize {
        self.create_group_with_icon(name, super::state::DEFAULT_GROUP_ICON.to_string())
    }

    pub fn create_group_with_icon(&mut self, name: String, icon: String) -> usize {
        self.groups.push(Group {
            id: super::state::generate_group_id(),
            name,
            icon: super::state::normalize_group_icon(&icon),
            accent: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        self.mark_session_dirty();
        self.groups.len() - 1
    }

    pub fn rename_group(&mut self, group_idx: usize, name: String) -> bool {
        let Some(group) = self.groups.get_mut(group_idx) else {
            return false;
        };
        group.name = name;
        self.mark_session_dirty();
        true
    }

    pub fn rename_workspace(&mut self, ws_idx: usize, name: String) -> bool {
        let Some(workspace) = self.workspaces.get_mut(ws_idx) else {
            return false;
        };
        workspace.set_custom_name(name);
        self.mark_session_dirty();
        true
    }

    pub fn set_workspace_default_cwd(&mut self, ws_idx: usize, cwd: std::path::PathBuf) -> bool {
        let Some(workspace) = self.workspaces.get_mut(ws_idx) else {
            return false;
        };
        if workspace.record_default_cwd(cwd) {
            self.mark_session_dirty();
            return true;
        }
        false
    }

    pub fn set_group_icon(&mut self, group_idx: usize, icon: String) -> bool {
        let Some(group) = self.groups.get_mut(group_idx) else {
            return false;
        };
        group.icon = super::state::normalize_group_icon(&icon);
        self.mark_session_dirty();
        true
    }

    pub fn delete_group(&mut self, group_idx: usize) -> Result<(), &'static str> {
        if self.groups.len() <= 1 {
            return Err("cannot delete the last group");
        }
        let Some(group) = self.groups.get(group_idx) else {
            return Err("group not found");
        };
        let deleted_group_id = group.id.clone();
        let active_id = self.active.map(|idx| self.workspaces[idx].id.clone());
        let selected_id = self.workspaces.get(self.selected).map(|ws| ws.id.clone());

        let deleting_active = self.active_group == group_idx;
        let terminal_ids = self
            .workspaces
            .iter()
            .filter(|workspace| workspace.group_id == deleted_group_id)
            .flat_map(|workspace| {
                workspace
                    .tabs
                    .iter()
                    .flat_map(|tab| tab.panes.values())
                    .map(|pane| pane.attached_terminal_id.clone())
            })
            .collect::<Vec<_>>();
        for workspace in self
            .workspaces
            .iter()
            .filter(|workspace| workspace.group_id == deleted_group_id)
        {
            crate::logging::workspace_closed(&workspace.id);
        }
        self.workspaces
            .retain(|workspace| workspace.group_id != deleted_group_id);
        self.remove_unattached_terminal_ids(terminal_ids);
        self.groups.remove(group_idx);
        if deleting_active {
            self.active_group = self.active_group.min(self.groups.len().saturating_sub(1));
        } else if self.active_group > group_idx {
            self.active_group = self.active_group.saturating_sub(1);
        }
        self.apply_effective_theme();

        self.active = active_id.and_then(|id| self.workspaces.iter().position(|ws| ws.id == id));
        self.selected = selected_id
            .and_then(|id| self.workspaces.iter().position(|ws| ws.id == id))
            .or(self.active)
            .or_else(|| self.first_visible_workspace())
            .unwrap_or(0);
        if self.active.is_none() {
            self.active = self.first_visible_workspace();
        }
        if self.active.is_none() && self.mode == Mode::Terminal {
            self.mode = Mode::Navigate;
        }
        self.workspace_scroll = 0;
        self.agent_panel_scroll = 0;
        self.tab_scroll_follow_active = true;
        self.refresh_tab_bar_view();
        self.mark_session_dirty();
        Ok(())
    }

    pub(crate) fn toggle_group_agent_profile_favorite(
        &mut self,
        group_idx: usize,
        profile_id: &str,
    ) {
        let Some(group) = self.groups.get_mut(group_idx) else {
            return;
        };
        if let Some(pos) = group
            .favorite_agent_profile_ids
            .iter()
            .position(|id| id == profile_id)
        {
            group.favorite_agent_profile_ids.remove(pos);
            if group.default_agent_profile_id.as_deref() == Some(profile_id) {
                group.default_agent_profile_id = None;
            }
        } else if self.agent_profiles.get(profile_id).is_some() {
            group
                .favorite_agent_profile_ids
                .push(profile_id.to_string());
        }
        self.mark_session_dirty();
    }

    pub(crate) fn toggle_group_default_agent_profile(
        &mut self,
        group_idx: usize,
        profile_id: &str,
    ) {
        if self.agent_profiles.get(profile_id).is_none() {
            return;
        }
        let Some(group) = self.groups.get_mut(group_idx) else {
            return;
        };
        if group.default_agent_profile_id.as_deref() == Some(profile_id) {
            group.default_agent_profile_id = None;
        } else {
            if !group
                .favorite_agent_profile_ids
                .iter()
                .any(|id| id == profile_id)
            {
                group
                    .favorite_agent_profile_ids
                    .push(profile_id.to_string());
            }
            group.default_agent_profile_id = Some(profile_id.to_string());
        }
        self.mark_session_dirty();
    }

    pub fn move_group(&mut self, source_idx: usize, insert_idx: usize) {
        if source_idx >= self.groups.len() || insert_idx > self.groups.len() {
            return;
        }

        let active_group_id = self
            .groups
            .get(self.active_group)
            .map(|group| group.id.clone());

        let group = self.groups.remove(source_idx);
        let target_idx = if source_idx < insert_idx {
            insert_idx.saturating_sub(1)
        } else {
            insert_idx
        }
        .min(self.groups.len());
        self.groups.insert(target_idx, group);

        self.active_group = active_group_id
            .and_then(|id| self.groups.iter().position(|group| group.id == id))
            .unwrap_or(0);
        self.apply_effective_theme();
        self.mark_session_dirty();
    }

    pub fn move_workspace_to_group(&mut self, ws_idx: usize, group_idx: usize) -> bool {
        let was_active = self.active == Some(ws_idx);
        let Some(group_id) = self.groups.get(group_idx).map(|group| group.id.clone()) else {
            return false;
        };
        let Some(workspace) = self.workspaces.get_mut(ws_idx) else {
            return false;
        };
        workspace.group_id = group_id;
        self.mark_session_dirty();
        if was_active && !self.workspace_in_active_group(ws_idx) {
            self.select_first_visible_workspace();
        }

        true
    }

    pub(crate) fn next_agent_metadata_expiry(&self) -> Option<std::time::Instant> {
        self.terminals
            .values()
            .filter_map(|terminal| terminal.next_agent_metadata_expiry())
            .min()
    }

    pub(crate) fn expire_agent_metadata_at(
        &mut self,
        scheduled_deadline: std::time::Instant,
        now: std::time::Instant,
    ) -> Vec<PaneStateUpdate> {
        let pane_terminals: Vec<_> = self
            .workspaces
            .iter()
            .enumerate()
            .flat_map(|(ws_idx, ws)| {
                ws.tabs.iter().flat_map(move |tab| {
                    tab.layout
                        .pane_ids()
                        .into_iter()
                        .filter_map(move |pane_id| {
                            ws.pane_state(pane_id)
                                .map(|pane| (ws_idx, pane_id, pane.attached_terminal_id.clone()))
                        })
                })
            })
            .collect();
        pane_terminals
            .into_iter()
            .filter_map(|(ws_idx, pane_id, terminal_id)| {
                let mutation = self
                    .terminals
                    .get_mut(&terminal_id)?
                    .expire_agent_metadata_at(scheduled_deadline, now)?;
                let change = mutation.effective_state_change?;
                let seq = self.next_agent_activity_seq();
                let unix_secs = unix_secs_for_activity_instant(now);
                if let Some(terminal) = self.terminals.get_mut(&terminal_id) {
                    terminal.mark_meaningful_agent_activity(seq, unix_secs);
                }

                let update = PaneStateUpdate {
                    pane_id,
                    ws_idx,
                    previous_agent_label: change.previous_agent_label.clone(),
                    previous_known_agent: change.previous_known_agent,
                    previous_state: change.previous_state,
                    previous_presentation: change.previous_presentation.clone(),
                    agent_label: change.agent_label.clone(),
                    known_agent: change.known_agent,
                    state: change.state,
                    presentation: change.presentation.clone(),
                };
                self.apply_pane_state_change(ws_idx, pane_id, &change);
                Some(update)
            })
            .collect()
    }
    pub(crate) fn pane_is_in_active_tab(&self, ws_idx: usize, pane_id: PaneId) -> bool {
        let Some(active_ws_idx) = self.active else {
            return false;
        };
        if active_ws_idx != ws_idx {
            return false;
        }
        self.workspaces[ws_idx]
            .find_tab_index_for_pane(pane_id)
            .is_some_and(|tab_idx| tab_idx == self.workspaces[ws_idx].active_tab)
    }

    pub fn switch_workspace(&mut self, idx: usize) {
        if idx < self.workspaces.len() {
            let previous_focus = self.current_pane_focus_target();
            let group_id = self.workspaces[idx].group_id.clone();
            if let Some(group_idx) = self.group_index_for_id(&group_id) {
                self.active_group = group_idx;
            }
            self.selection = None;
            self.selection_autoscroll = None;
            self.active = Some(idx);
            self.selected = idx;

            let workspace_id = self.workspaces[idx].id.clone();
            crate::logging::workspace_focused(&workspace_id);
            self.mark_session_dirty();
            if matches!(
                self.agent_panel_scope,
                crate::app::state::AgentPanelScope::CurrentWorkspace
            ) {
                self.agent_panel_scroll = 0;
            }
            self.ensure_workspace_visible(idx);
            if let Some(ws) = self.workspaces.get_mut(idx) {
                let active_tab = ws.active_tab;
                ws.switch_tab(active_tab);
                let tab_id = format!("{}:{}", workspace_id, active_tab + 1);
                crate::logging::tab_focused(&workspace_id, &tab_id);
            }
            self.tab_scroll_follow_active = true;
            self.refresh_tab_bar_view();
            self.record_pane_focus_after_navigation(previous_focus);
        }
    }

    pub(crate) fn switch_workspace_tab(&mut self, ws_idx: usize, tab_idx: usize) -> bool {
        if ws_idx >= self.workspaces.len() {
            return false;
        }
        if self
            .workspaces
            .get(ws_idx)
            .is_none_or(|ws| tab_idx >= ws.tabs.len())
        {
            return false;
        }

        let previous_focus = self.current_pane_focus_target();
        let workspace_changed = self.active != Some(ws_idx);
        let group_id = self.workspaces[ws_idx].group_id.clone();
        if let Some(group_idx) = self.group_index_for_id(&group_id) {
            self.active_group = group_idx;
        }
        self.selection = None;
        self.selection_autoscroll = None;
        self.active = Some(ws_idx);
        self.selected = ws_idx;
        let workspace_id = self.workspaces[ws_idx].id.clone();
        if workspace_changed {
            crate::logging::workspace_focused(&workspace_id);
        }
        self.mark_session_dirty();
        if workspace_changed
            && matches!(
                self.agent_panel_scope,
                crate::app::state::AgentPanelScope::CurrentWorkspace
            )
        {
            self.agent_panel_scroll = 0;
        }
        self.ensure_workspace_visible(ws_idx);
        if let Some(ws) = self.workspaces.get_mut(ws_idx) {
            ws.switch_tab(tab_idx);
            let tab_id = format!("{}:{}", workspace_id, tab_idx + 1);
            crate::logging::tab_focused(&workspace_id, &tab_id);
        }
        self.tab_scroll_follow_active = true;
        self.refresh_tab_bar_view();
        self.record_pane_focus_after_navigation(previous_focus);
        true
    }

    pub(crate) fn ensure_workspace_visible(&mut self, idx: usize) {
        if idx >= self.workspaces.len() {
            return;
        }

        if self.view.layout == ViewLayout::Mobile && self.mode == Mode::Navigate {
            self.ensure_mobile_workspace_visible(idx);
            return;
        }

        if self.sidebar_collapsed {
            return;
        }

        let Some(target_pos) = crate::ui::workspace_list_position_for_workspace(self, idx) else {
            return;
        };

        let workspace_area = if self.view.right_sidebar_rect != ratatui::layout::Rect::default() {
            crate::ui::left_sidebar_workspace_rect(self.view.sidebar_rect)
        } else {
            self.view.sidebar_rect
        };
        let mut cards = if self.view.right_sidebar_rect != ratatui::layout::Rect::default() {
            crate::ui::compute_workspace_card_areas_in_list(self, workspace_area)
        } else {
            crate::ui::compute_workspace_card_areas(self, workspace_area)
        };
        if cards.is_empty() {
            self.workspace_scroll = target_pos;
            return;
        }

        let first_pos = cards
            .first()
            .and_then(|card| crate::ui::workspace_list_position_for_workspace(self, card.ws_idx))
            .unwrap_or(0);
        if target_pos < first_pos {
            self.workspace_scroll = target_pos;
            return;
        }

        while cards
            .last()
            .and_then(|card| crate::ui::workspace_list_position_for_workspace(self, card.ws_idx))
            .unwrap_or(target_pos)
            < target_pos
        {
            let previous_scroll = self.workspace_scroll;
            self.workspace_scroll = self.workspace_scroll.saturating_add(1);
            if self.workspace_scroll == previous_scroll {
                break;
            }
            cards = if self.view.right_sidebar_rect != ratatui::layout::Rect::default() {
                crate::ui::compute_workspace_card_areas_in_list(self, workspace_area)
            } else {
                crate::ui::compute_workspace_card_areas(self, workspace_area)
            };
            if cards.is_empty() {
                break;
            }
        }
    }

    fn ensure_mobile_workspace_visible(&mut self, idx: usize) {
        let viewport = crate::ui::mobile_switcher_areas(self).viewport;
        if viewport.height == 0 {
            return;
        }

        let visible = self.visible_workspace_indices();
        let Some(visible_idx) = visible.iter().position(|ws_idx| *ws_idx == idx) else {
            return;
        };
        let row_range = crate::ui::mobile_switcher_workspace_doc_range(visible_idx);
        let visible_start = self.mobile_switcher_scroll;
        let visible_end = visible_start.saturating_add(viewport.height as usize);
        if row_range.start < visible_start {
            self.mobile_switcher_scroll = row_range.start;
        } else if row_range.end > visible_end {
            self.mobile_switcher_scroll = row_range.end.saturating_sub(viewport.height as usize);
        }
        self.mobile_switcher_scroll = self
            .mobile_switcher_scroll
            .min(crate::ui::mobile_switcher_max_scroll(self));
    }

    pub fn switch_tab(&mut self, idx: usize) {
        if let Some(ws_idx) = self.active {
            let previous_focus = self.current_pane_focus_target();
            self.selection = None;
            self.selection_autoscroll = None;
            let Some(ws) = self.workspaces.get_mut(ws_idx) else {
                return;
            };
            ws.switch_tab(idx);
            let workspace_id = ws.id.clone();
            let tab_id = format!("{}:{}", workspace_id, idx + 1);
            crate::logging::tab_focused(&workspace_id, &tab_id);
            self.mark_session_dirty();
            self.tab_scroll_follow_active = true;
            self.refresh_tab_bar_view();
            self.record_pane_focus_after_navigation(previous_focus);
        }
    }

    pub(crate) fn mark_active_tab_seen(&mut self) -> bool {
        let Some(ws_idx) = self.active else {
            return false;
        };
        let Some(tab) = self
            .workspaces
            .get_mut(ws_idx)
            .and_then(crate::workspace::Workspace::active_tab_mut)
        else {
            return false;
        };

        let mut changed = false;
        for pane in tab.panes.values_mut() {
            if !pane.seen {
                pane.seen = true;
                changed = true;
            }
        }
        changed
    }

    pub fn next_workspace(&mut self) {
        let visible = self.visible_workspace_indices();
        if !visible.is_empty() {
            let current = self.active.unwrap_or(self.selected);
            let current_pos = visible.iter().position(|idx| *idx == current).unwrap_or(0);
            let next = visible[(current_pos + 1) % visible.len()];
            self.switch_workspace(next);
        }
    }

    pub fn previous_workspace(&mut self) {
        let visible = self.visible_workspace_indices();
        if !visible.is_empty() {
            let current = self.active.unwrap_or(self.selected);
            let current_pos = visible.iter().position(|idx| *idx == current).unwrap_or(0);
            let prev = if current_pos == 0 {
                visible[visible.len() - 1]
            } else {
                visible[current_pos - 1]
            };
            self.switch_workspace(prev);
        }
    }

    pub fn move_workspace(&mut self, source_idx: usize, insert_idx: usize) {
        if source_idx >= self.workspaces.len() || insert_idx > self.workspaces.len() {
            return;
        }

        self.mark_session_dirty();

        let active_id = self.active.map(|idx| self.workspaces[idx].id.clone());
        let selected_id = self
            .workspaces
            .get(self.selected)
            .map(|workspace| workspace.id.clone());

        let workspace = self.workspaces.remove(source_idx);
        let target_idx = if source_idx < insert_idx {
            insert_idx.saturating_sub(1)
        } else {
            insert_idx
        }
        .min(self.workspaces.len());
        self.workspaces.insert(target_idx, workspace);

        self.active = active_id.and_then(|id| self.workspaces.iter().position(|ws| ws.id == id));
        self.selected = selected_id
            .and_then(|id| self.workspaces.iter().position(|ws| ws.id == id))
            .unwrap_or(0);
        self.ensure_workspace_visible(self.selected);
    }

    pub fn scroll_tabs_left(&mut self) {
        self.tab_scroll_follow_active = false;
        self.tab_scroll = self.tab_scroll.saturating_sub(1);
        self.refresh_tab_bar_view();
    }

    pub fn scroll_tabs_right(&mut self) {
        self.tab_scroll_follow_active = false;
        self.tab_scroll = self.tab_scroll.saturating_add(1);
        self.refresh_tab_bar_view();
    }

    pub fn move_tab(&mut self, source_idx: usize, insert_idx: usize) {
        if let Some(ws) = self.active.and_then(|i| self.workspaces.get_mut(i)) {
            if ws.move_tab(source_idx, insert_idx) {
                self.mark_session_dirty();
                self.tab_scroll_follow_active = true;
                self.refresh_tab_bar_view();
            }
        }
    }

    pub fn next_tab(&mut self) {
        if let Some(ws) = self.active.and_then(|i| self.workspaces.get(i)) {
            if !ws.tabs.is_empty() {
                let next = (ws.active_tab + 1) % ws.tabs.len();
                self.switch_tab(next);
            }
        }
    }

    pub fn previous_tab(&mut self) {
        if let Some(ws) = self.active.and_then(|i| self.workspaces.get(i)) {
            if !ws.tabs.is_empty() {
                let prev = if ws.active_tab == 0 {
                    ws.tabs.len() - 1
                } else {
                    ws.active_tab - 1
                };
                self.switch_tab(prev);
            }
        }
    }

    pub fn next_agent(&mut self) {
        self.cycle_agent_entry(true);
    }

    pub fn previous_agent(&mut self) {
        self.cycle_agent_entry(false);
    }

    pub fn focus_agent_entry(&mut self, idx: usize) -> bool {
        let entries = crate::ui::agent_panel_entries(self);
        let Some(target) = entries.get(idx) else {
            return false;
        };
        let ws_idx = target.ws_idx;
        let pane_id = target.pane_id;

        if self.active == Some(ws_idx) && self.workspaces[ws_idx].focused_pane_id() == Some(pane_id)
        {
            self.ensure_agent_panel_entry_visible(idx);
            return true;
        }

        if self.focus_pane_in_workspace(ws_idx, pane_id) {
            self.ensure_agent_panel_entry_visible(idx);
            return true;
        }
        false
    }

    fn cycle_agent_entry(&mut self, forward: bool) {
        let entries = crate::ui::agent_panel_entries(self);
        if entries.is_empty() {
            return;
        }

        let focused = self
            .active
            .and_then(|idx| self.workspaces.get(idx))
            .and_then(crate::workspace::Workspace::focused_pane_id);
        let current_idx =
            focused.and_then(|pane_id| entries.iter().position(|entry| entry.pane_id == pane_id));
        let target_idx = match (current_idx, forward) {
            (Some(idx), true) => (idx + 1) % entries.len(),
            (Some(0), false) => entries.len() - 1,
            (Some(idx), false) => idx - 1,
            (None, true) => 0,
            (None, false) => entries.len() - 1,
        };

        self.focus_agent_entry(target_idx);
    }

    fn ensure_agent_panel_entry_visible(&mut self, idx: usize) {
        if self.sidebar_collapsed {
            return;
        }

        let (detail_area, leading_separator) =
            if self.view.right_sidebar_rect != ratatui::layout::Rect::default() {
                if self.right_sidebar_collapsed {
                    return;
                }
                (
                    crate::ui::right_sidebar_content_rect(self.view.right_sidebar_rect),
                    false,
                )
            } else {
                let (_, detail_area) = crate::ui::expanded_sidebar_sections(
                    self.view.sidebar_rect,
                    self.sidebar_section_split,
                );
                (detail_area, true)
            };
        let metrics = crate::ui::agent_panel_scroll_metrics(self, detail_area, leading_separator);
        let visible = metrics.viewport_rows;
        if visible == 0 {
            return;
        }

        if idx < self.agent_panel_scroll {
            self.agent_panel_scroll = idx;
        } else if idx >= self.agent_panel_scroll.saturating_add(visible) {
            self.agent_panel_scroll = idx.saturating_add(1).saturating_sub(visible);
        }

        let max_scroll =
            crate::ui::agent_panel_scroll_metrics(self, detail_area, leading_separator)
                .max_offset_from_bottom;
        self.agent_panel_scroll = self.agent_panel_scroll.min(max_scroll);
    }

    pub(crate) fn terminal_ids_for_workspace(
        &self,
        ws_idx: usize,
    ) -> Vec<crate::terminal::TerminalId> {
        self.workspaces
            .get(ws_idx)
            .into_iter()
            .flat_map(|ws| &ws.tabs)
            .flat_map(|tab| tab.panes.values())
            .map(|pane| pane.attached_terminal_id.clone())
            .collect()
    }

    pub(crate) fn pane_ids_for_workspace(&self, ws_idx: usize) -> Vec<PaneId> {
        self.workspaces
            .get(ws_idx)
            .into_iter()
            .flat_map(|ws| &ws.tabs)
            .flat_map(|tab| tab.layout.pane_ids())
            .collect()
    }

    pub(crate) fn terminal_ids_for_tab(
        &self,
        ws_idx: usize,
        tab_idx: usize,
    ) -> Vec<crate::terminal::TerminalId> {
        self.workspaces
            .get(ws_idx)
            .and_then(|ws| ws.tabs.get(tab_idx))
            .into_iter()
            .flat_map(|tab| tab.panes.values())
            .map(|pane| pane.attached_terminal_id.clone())
            .collect()
    }

    pub(crate) fn pane_ids_for_tab(&self, ws_idx: usize, tab_idx: usize) -> Vec<PaneId> {
        self.workspaces
            .get(ws_idx)
            .and_then(|ws| ws.tabs.get(tab_idx))
            .map(|tab| tab.layout.pane_ids())
            .unwrap_or_default()
    }

    pub(crate) fn terminal_id_for_pane(
        &self,
        ws_idx: usize,
        pane_id: PaneId,
    ) -> Option<crate::terminal::TerminalId> {
        self.workspaces
            .get(ws_idx)?
            .pane_state(pane_id)
            .map(|pane| pane.attached_terminal_id.clone())
    }

    pub(crate) fn remove_unattached_terminal_ids(
        &mut self,
        terminal_ids: impl IntoIterator<Item = crate::terminal::TerminalId>,
    ) {
        for terminal_id in terminal_ids {
            let still_attached = self.workspaces.iter().any(|ws| {
                ws.tabs.iter().any(|tab| {
                    tab.panes
                        .values()
                        .any(|pane| pane.attached_terminal_id == terminal_id)
                })
            });
            if !still_attached
                && self.terminals.remove(&terminal_id).is_some()
                && !self.terminal_runtime_shutdowns.contains(&terminal_id)
            {
                self.terminal_runtime_shutdowns.push(terminal_id);
            }
        }
    }

    pub(crate) fn terminal_has_command_run(
        &self,
        terminal_id: &crate::terminal::TerminalId,
    ) -> bool {
        self.command_runs
            .values()
            .any(|run| &run.terminal_id == terminal_id)
    }

    fn empty_workspace(&mut self, ws_idx: usize) {
        let Some(ws) = self.workspaces.get_mut(ws_idx) else {
            return;
        };
        while !ws.tabs.is_empty() {
            ws.close_tab_allow_empty(0);
        }
        self.active = Some(ws_idx);
        self.selected = ws_idx;
        self.workspace_scroll = self.workspace_scroll.min(ws_idx);
        self.tab_scroll = 0;
        self.tab_scroll_follow_active = true;
        self.refresh_tab_bar_view();
    }

    pub(crate) fn return_to_active_workspace_mode(&mut self) {
        self.mode = if self.active.is_some() {
            Mode::Terminal
        } else {
            Mode::Navigate
        };
    }

    pub(crate) fn close_selected_workspace_from_ui(&mut self) {
        self.close_selected_workspace();
        self.return_to_active_workspace_mode();
    }

    pub(crate) fn remove_selected_workspace(&mut self) {
        self.close_selected_workspace();
    }

    pub(crate) fn remove_plugin_pane_records(
        &mut self,
        pane_ids: impl IntoIterator<Item = PaneId>,
    ) {
        for pane_id in pane_ids {
            self.plugin_panes.remove(&pane_id);
        }
    }

    pub fn close_selected_workspace(&mut self) {
        if self.workspaces.is_empty() {
            return;
        }
        self.selection = None;
        self.selection_autoscroll = None;
        self.mark_session_dirty();
        let close_indices = self
            .workspaces
            .get(self.selected)
            .and_then(|ws| ws.worktree_space())
            .filter(|space| !space.is_linked_worktree)
            .map(|space| {
                self.workspaces
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, ws)| {
                        ws.worktree_space()
                            .is_some_and(|member| member.key == space.key)
                            .then_some(idx)
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|indices| indices.len() >= 2)
            .unwrap_or_else(|| vec![self.selected]);

        let mut terminal_ids = Vec::new();
        let mut pane_ids = Vec::new();
        for idx in &close_indices {
            terminal_ids.extend(self.terminal_ids_for_workspace(*idx));
            pane_ids.extend(self.pane_ids_for_workspace(*idx));
            if let Some(workspace_id) = self.workspaces.get(*idx).map(|ws| ws.id.clone()) {
                crate::logging::workspace_closed(&workspace_id);
            }
        }
        self.remove_plugin_pane_records(pane_ids);
        for idx in close_indices.iter().rev() {
            self.workspaces.remove(*idx);
        }
        self.remove_unattached_terminal_ids(terminal_ids);
        if self.workspaces.is_empty() {
            self.active = None;
            self.selected = 0;
            self.workspace_scroll = 0;
            self.tab_scroll = 0;
            self.tab_scroll_follow_active = true;
        } else {
            if self.selected >= self.workspaces.len() {
                self.selected = self.workspaces.len() - 1;
            }
            self.active = Some(self.selected);
            self.workspace_scroll = self
                .workspace_scroll
                .min(self.workspaces.len().saturating_sub(1));
            self.ensure_workspace_visible(self.selected);
            self.tab_scroll_follow_active = true;
            self.refresh_tab_bar_view();
        }
    }

    fn refresh_tab_bar_view(&mut self) {
        let area = self.view.tab_bar_rect;
        let Some(ws) = self.active.and_then(|idx| self.workspaces.get(idx)) else {
            self.tab_scroll = 0;
            self.view.tab_hit_areas.clear();
            self.view.tab_close_hit_areas.clear();
            self.view.tab_scroll_left_hit_area = ratatui::layout::Rect::default();
            self.view.tab_scroll_right_hit_area = ratatui::layout::Rect::default();
            self.view.new_tab_hit_area = ratatui::layout::Rect::default();
            return;
        };

        let layout = crate::ui::compute_tab_bar_view(
            ws,
            area,
            self.tab_scroll,
            self.tab_scroll_follow_active,
            self.mouse_capture,
            self.hovered_tab,
        );
        self.tab_scroll = layout.scroll;
        self.view.tab_hit_areas = layout.tab_hit_areas;
        self.view.tab_close_hit_areas = layout.tab_close_hit_areas;
        self.view.tab_scroll_left_hit_area = layout.scroll_left_hit_area;
        self.view.tab_scroll_right_hit_area = layout.scroll_right_hit_area;
        self.view.new_tab_hit_area = layout.new_tab_hit_area;
    }
}

// ---------------------------------------------------------------------------
// Pane operations
// ---------------------------------------------------------------------------

impl AppState {
    pub fn navigate_pane(&mut self, direction: NavDirection) {
        let Some(ws_idx) = self.active else {
            return;
        };
        let Some(tab) = self.workspaces.get(ws_idx).and_then(|ws| ws.active_tab()) else {
            return;
        };
        let panes = if tab.zoomed {
            tab.layout.panes(self.view.terminal_area)
        } else {
            self.view.pane_infos.clone()
        };

        if let Some(focused) = panes.iter().find(|p| p.is_focused) {
            if let Some(target) = find_in_direction(focused, direction, &panes) {
                self.focus_pane_in_workspace(ws_idx, target);
            }
        }
    }

    pub fn resize_pane(&mut self, direction: NavDirection) {
        if let Some(first) = self.view.pane_infos.first() {
            let area = self
                .view
                .pane_infos
                .iter()
                .fold(first.rect, |acc, p| acc.union(p.rect));
            if let Some(tab) = self
                .active
                .and_then(|i| self.workspaces.get_mut(i))
                .and_then(|ws| ws.active_tab_mut())
            {
                tab.layout.resize_focused(direction, 0.05, area);
                self.mark_session_dirty();
            }
        }
    }

    pub fn cycle_pane(&mut self, reverse: bool) {
        let Some(ws_idx) = self.active else {
            return;
        };
        let Some(tab) = self.workspaces.get(ws_idx).and_then(|ws| ws.active_tab()) else {
            return;
        };
        let ids = tab.layout.pane_ids();
        if let Some(pos) = ids.iter().position(|id| *id == tab.layout.focused()) {
            let target = if reverse {
                ids[(pos + ids.len() - 1) % ids.len()]
            } else {
                ids[(pos + 1) % ids.len()]
            };
            self.focus_pane_in_workspace(ws_idx, target);
        }
    }

    pub fn last_pane(&mut self) {
        let Some(target) = self.previous_pane_focus.clone() else {
            return;
        };
        let Some((ws_idx, tab_idx)) = self.pane_focus_target_indices(&target) else {
            self.previous_pane_focus = None;
            return;
        };
        let current = self.current_pane_focus_target();
        if current.as_ref() == Some(&target) {
            self.previous_pane_focus = None;
            return;
        }

        self.switch_workspace_tab(ws_idx, tab_idx);
        if let Some(tab) = self
            .workspaces
            .get_mut(ws_idx)
            .and_then(|ws| ws.tabs.get_mut(tab_idx))
        {
            tab.layout.focus_pane(target.pane_id);
            self.previous_pane_focus = current;
            self.mark_session_dirty();
        }
    }

    pub fn toggle_zoom(&mut self) {
        if let Some(tab) = self
            .active
            .and_then(|i| self.workspaces.get_mut(i))
            .and_then(|ws| ws.active_tab_mut())
        {
            if tab.layout.pane_count() > 1 {
                tab.zoomed = !tab.zoomed;
                self.mark_session_dirty();
            }
        }
    }

    pub(crate) fn workspace_close_would_close_worktree_group(&self, ws_idx: usize) -> bool {
        self.workspaces
            .get(ws_idx)
            .and_then(|ws| ws.worktree_space())
            .filter(|space| !space.is_linked_worktree)
            .is_some_and(|space| {
                self.workspaces
                    .iter()
                    .filter(|ws| {
                        ws.worktree_space()
                            .is_some_and(|member| member.key == space.key)
                    })
                    .count()
                    >= 2
            })
    }

    pub(crate) fn confirm_implicit_worktree_group_close(&mut self, ws_idx: usize) -> bool {
        if self.confirm_close && self.workspace_close_would_close_worktree_group(ws_idx) {
            self.selected = ws_idx;
            self.mode = Mode::ConfirmClose;
            true
        } else {
            false
        }
    }

    fn close_focused_pane_would_close_workspace(&self, ws_idx: usize) -> bool {
        self.workspaces.get(ws_idx).is_some_and(|ws| {
            let pane_count = ws
                .active_tab()
                .map(|tab| tab.layout.pane_count())
                .unwrap_or(0);
            pane_count <= 1 && ws.tabs.len() <= 1
        })
    }

    pub(crate) fn close_pane_would_close_workspace(&self, ws_idx: usize, pane_id: PaneId) -> bool {
        self.workspaces.get(ws_idx).is_some_and(|ws| {
            ws.find_tab_index_for_pane(pane_id).is_some_and(|tab_idx| {
                ws.tabs[tab_idx].layout.pane_count() <= 1 && ws.tabs.len() <= 1
            })
        })
    }

    fn record_focused_workspace_default_cwd(&mut self, ws_idx: usize) {
        let cwd = self.workspaces.get(ws_idx).and_then(|ws| {
            let tab = ws.active_tab()?;
            let terminal_id = tab.terminal_id(tab.layout.focused())?;
            self.terminals
                .get(terminal_id)
                .map(|terminal| terminal.cwd.clone())
        });
        if let Some(cwd) = cwd {
            if let Some(ws) = self.workspaces.get_mut(ws_idx) {
                ws.record_default_cwd(cwd);
            }
        }
    }

    /// Close the focused pane. Returns true when the close was deferred to confirmation.
    pub fn close_pane(&mut self) -> bool {
        let active = self.active;
        if active.is_some_and(|ws_idx| {
            self.close_focused_pane_would_close_workspace(ws_idx)
                && self.workspace_close_would_close_worktree_group(ws_idx)
        }) {
            if let Some(ws_idx) = active {
                if self.confirm_implicit_worktree_group_close(ws_idx) {
                    return true;
                }
            }
        }

        self.selection = None;
        self.selection_autoscroll = None;
        self.mark_session_dirty();
        if let Some(ws_idx) = active {
            self.record_focused_workspace_default_cwd(ws_idx);
        }
        let terminal_ids = active
            .and_then(|i| {
                self.workspaces
                    .get(i)
                    .and_then(|ws| ws.focused_pane_id().map(|pane_id| (i, pane_id)))
            })
            .and_then(|(i, pane_id)| self.terminal_id_for_pane(i, pane_id))
            .into_iter()
            .collect::<Vec<_>>();
        let pane_ids = active
            .and_then(|i| self.workspaces.get(i).and_then(|ws| ws.focused_pane_id()))
            .into_iter()
            .collect::<Vec<_>>();
        let should_close_workspace = active
            .and_then(|i| self.workspaces.get_mut(i))
            .is_some_and(|ws| ws.close_focused());
        self.remove_plugin_pane_records(pane_ids);
        if should_close_workspace {
            if let Some(active) = active {
                self.selected = active;
            }
            self.close_selected_workspace();
        } else {
            self.remove_unattached_terminal_ids(terminal_ids);
        }
        false
    }

    /// Close the active tab while preserving the workspace, even when it was the last tab.
    pub fn close_tab(&mut self) -> bool {
        let Some(ws_idx) = self.active else {
            return false;
        };
        if self
            .workspaces
            .get(ws_idx)
            .is_none_or(|ws| ws.tabs.is_empty())
        {
            return false;
        }
        self.close_tab_at(self.workspaces[ws_idx].active_tab)
    }

    /// Close a tab by index while preserving the workspace, even when it was the last tab.
    pub fn close_tab_at(&mut self, tab_idx: usize) -> bool {
        let Some(ws_idx) = self.active else {
            return false;
        };
        if self
            .workspaces
            .get(ws_idx)
            .is_none_or(|ws| ws.tabs.get(tab_idx).is_none())
        {
            return false;
        }
        self.selection = None;
        self.selection_autoscroll = None;
        let terminal_ids = self.terminal_ids_for_tab(ws_idx, tab_idx);
        let pane_ids = self.pane_ids_for_tab(ws_idx, tab_idx);
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return false;
        };
        let workspace_id = ws.id.clone();
        let closing_tab_id = ws
            .public_tab_number(tab_idx)
            .map(|number| crate::workspace::public_tab_id_for_number(&workspace_id, number))
            .unwrap_or_else(|| format!("{}:{}", workspace_id, tab_idx + 1));
        self.record_focused_workspace_default_cwd(ws_idx);
        let Some(ws) = self.workspaces.get_mut(ws_idx) else {
            return false;
        };
        if !ws.close_tab_allow_empty(tab_idx) {
            return false;
        }
        self.remove_plugin_pane_records(pane_ids);
        self.remove_unattached_terminal_ids(terminal_ids);
        crate::logging::tab_closed(&workspace_id, &closing_tab_id);
        self.mark_session_dirty();
        self.hovered_tab = None;
        self.tab_scroll_follow_active = true;
        self.refresh_tab_bar_view();
        false
    }
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

impl AppState {
    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.selection_autoscroll = None;
    }

    pub(crate) fn stop_selection_autoscroll_state(&mut self) {
        self.selection_autoscroll = None;
    }

    pub(crate) fn copy_word_at_pane_cell(
        &mut self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        pane_id: crate::layout::PaneId,
        viewport_row: u16,
        col: u16,
    ) -> bool {
        // Resolve the active pane cell the double-click landed on.
        let Some(ws_idx) = self
            .active
            .filter(|idx| self.workspaces.get(*idx).is_some())
        else {
            return false;
        };

        let Some(info) = self.pane_info_by_id(pane_id) else {
            return false;
        };
        if viewport_row >= info.inner_rect.height || col >= info.inner_rect.width {
            return false;
        }

        // Leave mouse input to terminal apps that requested it.
        let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, pane_id)
        else {
            return false;
        };
        if rt
            .input_state()
            .is_some_and(crate::pane::InputState::mouse_reporting_enabled)
        {
            return false;
        }

        // Read the visible row and identify the clicked token bounds.
        let metrics = self.pane_scroll_metrics(terminal_runtimes, pane_id);
        let row_selection = Selection::range(
            pane_id,
            viewport_row,
            0,
            info.inner_rect.width.saturating_sub(1),
            metrics,
        );
        let Some(row_text) = rt.extract_selection(&row_selection) else {
            return false;
        };
        let Some((start_col, end_col)) = word_bounds_at_column(&row_text, col) else {
            return false;
        };

        // Copy the token and keep its selection visible as short-lived feedback.
        let mut selection = Selection::range(pane_id, viewport_row, start_col, end_col, metrics);
        if !selection.finish() {
            return false;
        }

        let Some(text) = rt
            .extract_selection(&selection)
            .filter(|text| !text.is_empty())
        else {
            self.clear_selection();
            return false;
        };
        self.request_clipboard_write = Some(text.into_bytes());
        self.selection = Some(selection);
        self.selection_autoscroll = None;
        info!("copied double-clicked token to clipboard");
        true
    }

    pub(crate) fn url_at_pane_cell(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        pane_id: crate::layout::PaneId,
        viewport_row: u16,
        col: u16,
    ) -> Option<String> {
        let ws_idx = self
            .active
            .filter(|idx| self.workspaces.get(*idx).is_some())?;
        let info = self.pane_info_by_id(pane_id)?;
        if viewport_row >= info.inner_rect.height || col >= info.inner_rect.width {
            return None;
        }

        let rt = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, pane_id)?;
        let screen_col = info.inner_rect.x.saturating_add(col);
        let screen_row = info.inner_rect.y.saturating_add(viewport_row);
        if let Some((_, _, uri)) = rt
            .visible_hyperlinks(info.inner_rect)
            .into_iter()
            .find(|((x, y), _, _)| *x == screen_col && *y == screen_row)
        {
            return safe_web_url(&uri).map(str::to_owned);
        }

        let metrics = self.pane_scroll_metrics(terminal_runtimes, pane_id);
        let row_selection = Selection::range(
            pane_id,
            viewport_row,
            0,
            info.inner_rect.width.saturating_sub(1),
            metrics,
        );
        let row_text = rt.extract_selection(&row_selection)?;
        url_at_column(&row_text, col).map(str::to_owned)
    }

    pub fn copy_selection(&mut self, terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry) {
        let mut sel = match self.selection.take() {
            Some(sel) => sel,
            None => return,
        };
        if !sel.finish() {
            return;
        }

        let ws_idx = match self.active {
            Some(ws_idx) if self.workspaces.get(ws_idx).is_some() => ws_idx,
            _ => return,
        };

        let text = self
            .runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, sel.pane_id)
            .and_then(|rt| rt.extract_selection(&sel));
        if let Some(text) = text {
            if !text.is_empty() {
                self.request_clipboard_write = Some(text.into_bytes());
                info!("copied selection to clipboard");
            }
        }

        self.clear_selection();
    }
}

pub(crate) fn safe_web_url(url: &str) -> Option<&str> {
    (url.starts_with("http://") || url.starts_with("https://")).then_some(url)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TextCell {
    ch: char,
    start_col: u16,
    end_col: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellSpan {
    start: usize,
    end: usize,
}

impl CellSpan {
    fn contains(self, idx: usize) -> bool {
        idx >= self.start && idx <= self.end
    }

    fn columns(self, cells: &[TextCell]) -> (u16, u16) {
        (cells[self.start].start_col, cells[self.end].end_col)
    }
}

/// Finds the terminal display-column bounds for the token under a double-click.
///
/// The algorithm first maps text to terminal cells so wide characters and
/// zero-width marks use display columns, then prefers structured spans that
/// users expect to copy whole (URLs and quoted paths), and finally falls back
/// to a separator-delimited token.
fn word_bounds_at_column(row: &str, col: u16) -> Option<(u16, u16)> {
    // Map the row into display cells before doing any word-boundary work.
    let cells = text_cells(row);
    let clicked_idx = cell_index_at_column(&cells, col)?;

    // Prefer spans that can legally include punctuation or spaces.
    let span = url_span_at_column(&cells, clicked_idx)
        .or_else(|| quoted_path_span_at_column(&cells, clicked_idx))
        .or_else(|| token_span_at_column(&cells, clicked_idx))?;

    // Convert the internal cell span back to inclusive terminal columns.
    Some(span.columns(&cells))
}

pub(crate) fn url_at_column(row: &str, col: u16) -> Option<&str> {
    let cells = text_cells(row);
    let clicked_idx = cell_index_at_column(&cells, col)?;
    let span = url_span_at_column(&cells, clicked_idx)?;
    let start_byte = byte_index_for_cell(row, span.start);
    let end_byte = byte_index_after_cell(row, span.end);
    safe_web_url(row.get(start_byte..end_byte)?)
}

fn token_span_at_column(cells: &[TextCell], clicked_idx: usize) -> Option<CellSpan> {
    if is_word_separator(cells[clicked_idx].ch) {
        return None;
    }

    let mut start = clicked_idx;
    while start > 0 && !is_word_separator(cells[start - 1].ch) {
        start -= 1;
    }

    let mut end = clicked_idx;
    while end + 1 < cells.len() && !is_word_separator(cells[end + 1].ch) {
        end += 1;
    }

    trim_token_edges(cells, CellSpan { start, end }).filter(|span| span.contains(clicked_idx))
}

fn text_cells(row: &str) -> Vec<TextCell> {
    let mut next_col = 0u16;
    row.chars()
        .map(|ch| {
            let width = UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
            let start_col = if width == 0 {
                next_col.saturating_sub(1)
            } else {
                next_col
            };
            if width > 0 {
                next_col = next_col.saturating_add(width);
            }
            TextCell {
                ch,
                start_col,
                end_col: next_col.saturating_sub(1),
            }
        })
        .collect()
}

fn cell_index_at_column(cells: &[TextCell], col: u16) -> Option<usize> {
    cells
        .iter()
        .position(|cell| cell.start_col <= col && col <= cell.end_col)
}

fn byte_index_for_cell(row: &str, cell_idx: usize) -> usize {
    row.char_indices()
        .nth(cell_idx)
        .map(|(idx, _)| idx)
        .unwrap_or(row.len())
}

fn byte_index_after_cell(row: &str, cell_idx: usize) -> usize {
    row.char_indices()
        .nth(cell_idx.saturating_add(1))
        .map(|(idx, _)| idx)
        .unwrap_or(row.len())
}

fn url_span_at_column(cells: &[TextCell], clicked_idx: usize) -> Option<CellSpan> {
    let mut start = 0;
    while start < cells.len() {
        if starts_with_chars(&cells[start..], "http://")
            || starts_with_chars(&cells[start..], "https://")
        {
            let mut end = start;
            while end + 1 < cells.len() && !cells[end + 1].ch.is_whitespace() {
                end += 1;
            }
            if clicked_idx >= start && clicked_idx <= end {
                let span = trim_url_edges(cells, CellSpan { start, end })?;
                return span.contains(clicked_idx).then_some(span);
            }
            start = end + 1;
        } else {
            start += 1;
        }
    }
    None
}

fn trim_url_edges(cells: &[TextCell], span: CellSpan) -> Option<CellSpan> {
    let start = span.start;
    let mut end = span.end;
    while start <= end && should_trim_trailing_url_cell(cells, start, end) {
        if end == 0 {
            return None;
        }
        end -= 1;
    }
    (start <= end).then_some(CellSpan { start, end })
}

fn should_trim_trailing_url_cell(cells: &[TextCell], start: usize, end: usize) -> bool {
    match cells[end].ch {
        '"' | '\'' | '`' | '.' | ',' | ';' | ':' | '!' | '?' => true,
        ')' => !trailing_url_closer_is_balanced(cells, start, end, '(', ')'),
        ']' => !trailing_url_closer_is_balanced(cells, start, end, '[', ']'),
        '}' => !trailing_url_closer_is_balanced(cells, start, end, '{', '}'),
        _ => false,
    }
}

fn trailing_url_closer_is_balanced(
    cells: &[TextCell],
    start: usize,
    end: usize,
    open: char,
    close: char,
) -> bool {
    let mut balance = 0i32;
    for cell in &cells[start..end] {
        if cell.ch == open {
            balance += 1;
        } else if cell.ch == close {
            balance -= 1;
        }
    }
    balance > 0
}

fn quoted_path_span_at_column(cells: &[TextCell], clicked_idx: usize) -> Option<CellSpan> {
    let clicked = cells.get(clicked_idx)?.ch;
    if clicked == '"' || clicked == '\'' || clicked == '`' {
        return None;
    }

    for quote in ['"', '\'', '`'] {
        let mut start = None;
        for (idx, cell) in cells.iter().copied().enumerate() {
            let ch = cell.ch;
            if ch != quote || is_escaped(cells, idx) {
                continue;
            }
            if let Some(open) = start {
                if clicked_idx > open
                    && clicked_idx < idx
                    && cells[open + 1..idx].iter().any(|cell| cell.ch == '/')
                {
                    return Some(CellSpan {
                        start: open + 1,
                        end: idx - 1,
                    });
                }
                start = None;
            } else {
                start = Some(idx);
            }
        }
    }
    None
}

fn is_escaped(cells: &[TextCell], idx: usize) -> bool {
    let mut slashes = 0;
    let mut cursor = idx;
    while cursor > 0 && cells[cursor - 1].ch == '\\' {
        slashes += 1;
        cursor -= 1;
    }
    slashes % 2 == 1
}

fn starts_with_chars(cells: &[TextCell], prefix: &str) -> bool {
    prefix
        .chars()
        .enumerate()
        .all(|(idx, expected)| cells.get(idx).is_some_and(|cell| cell.ch == expected))
}

fn is_word_separator(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '|' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '!'
        )
}

fn trim_token_edges(cells: &[TextCell], span: CellSpan) -> Option<CellSpan> {
    let mut start = span.start;
    let mut end = span.end;
    while start <= end && is_leading_token_wrapper(cells[start].ch) {
        start += 1;
    }
    if start < end && cells[end].ch == '$' && is_trailing_token_wrapper(cells[end - 1].ch) {
        end -= 1;
    }
    while start <= end && is_trailing_token_wrapper(cells[end].ch) {
        if end == 0 {
            return None;
        }
        end -= 1;
    }
    (start <= end).then_some(CellSpan { start, end })
}

fn is_leading_token_wrapper(ch: char) -> bool {
    matches!(ch, '(' | '[' | '{' | '<' | '"' | '\'' | '`')
}

fn is_trailing_token_wrapper(ch: char) -> bool {
    matches!(
        ch,
        ')' | ']' | '}' | '>' | '"' | '\'' | '`' | '.' | ',' | ';' | ':' | '!' | '?'
    )
}

// ---------------------------------------------------------------------------
// Event handling
// ---------------------------------------------------------------------------

impl AppState {
    pub fn apply_workspace_git_statuses(
        &mut self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        results: Vec<WorkspaceGitStatus>,
    ) -> bool {
        let mut changed = false;
        for result in results {
            let Some(ws_idx) = self
                .workspaces
                .iter()
                .position(|ws| ws.id == result.workspace_id)
            else {
                continue;
            };

            if self.workspaces[ws_idx]
                .resolved_identity_cwd_from(&self.terminals, terminal_runtimes)
                .as_ref()
                != Some(&result.resolved_identity_cwd)
            {
                continue;
            }
            if self.workspaces[ws_idx].git_status_cwds_from(&self.terminals, terminal_runtimes)
                != result.cwd_fingerprint
            {
                continue;
            }

            let ws = &mut self.workspaces[ws_idx];
            if ws.cached_git_branch != result.branch {
                ws.cached_git_branch = result.branch;
                changed = true;
            }
            if ws.cached_git_ahead_behind != result.ahead_behind {
                ws.cached_git_ahead_behind = result.ahead_behind;
                changed = true;
            }
            if ws.cached_git_work_summary != result.work_summary {
                ws.cached_git_work_summary = result.work_summary;
                changed = true;
            }
            if ws.cached_git_space != result.space {
                ws.cached_git_space = result.space;
                changed = true;
            }
        }
        changed
    }

    pub fn handle_app_event(&mut self, event: AppEvent) -> Vec<PaneStateUpdate> {
        self.handle_app_event_with_terminal_runtimes(None, event)
    }

    pub fn handle_app_event_with_terminal_runtimes(
        &mut self,
        terminal_runtimes: Option<&crate::terminal::TerminalRuntimeRegistry>,
        event: AppEvent,
    ) -> Vec<PaneStateUpdate> {
        match event {
            AppEvent::PaneDied {
                pane_id,
                child_pid,
                exit_success,
            } => {
                self.handle_pane_died(terminal_runtimes, pane_id, child_pid, exit_success);
                Vec::new()
            }
            AppEvent::UpdateReady {
                version,
                install_command,
            } => {
                self.update_available = Some(version.clone());
                self.update_install_command = install_command.clone();
                self.latest_release_notes_available = true;
                self.update_dismissed = true;
                if matches!(
                    self.toast_config.delivery,
                    crate::config::ToastDelivery::Hako
                ) {
                    self.toast = Some(ToastNotification {
                        kind: ToastKind::UpdateInstalled,
                        title: format!("v{version} available"),
                        context: format!("detach, then run `{install_command}`"),
                        position: None,
                        target: None,
                    });
                }
                Vec::new()
            }
            AppEvent::AgentDetectionManifestsUpdated { updated, status } => {
                self.agent_manifest_update_status = status;
                self.refresh_agent_manifest_summaries();
                if !updated.is_empty()
                    && matches!(
                        self.toast_config.delivery,
                        crate::config::ToastDelivery::Hako
                    )
                {
                    let agent_list = updated
                        .iter()
                        .map(|item| {
                            format!(
                                "{} {}",
                                crate::detect::agent_label(item.agent),
                                item.version
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.toast = Some(ToastNotification {
                        kind: ToastKind::UpdateInstalled,
                        title: "Agent detection rules updated".to_string(),
                        context: agent_list,
                        position: None,
                        target: None,
                    });
                }
                Vec::new()
            }
            AppEvent::StateChanged {
                pane_id,
                agent,
                state,
                visible_blocker,
                visible_idle,
                visible_working,
                process_exited,
                observed_at,
            } => self
                .update_terminal_state_at(pane_id, observed_at, |terminal| {
                    Some(terminal.set_detected_state_with_screen_signals_at(
                        agent,
                        state,
                        visible_blocker,
                        visible_idle,
                        visible_working,
                        process_exited,
                        observed_at,
                    ))
                })
                .into_iter()
                .collect(),
            AppEvent::HookStateReported {
                pane_id,
                source,
                agent_label,
                state,
                message,
                custom_status,
                seq,
                session_ref,
                launch_env,
            } => self
                .update_terminal_state(pane_id, |terminal| {
                    let mut mutation = terminal.set_hook_authority_with_session_ref(
                        source,
                        agent_label,
                        state,
                        message,
                        custom_status,
                        session_ref,
                        seq,
                    )?;
                    if terminal.launch_env != launch_env {
                        terminal.launch_env = launch_env;
                        mutation.session_ref_changed = true;
                    }
                    Some(mutation)
                })
                .into_iter()
                .collect(),
            AppEvent::HookSessionReported {
                pane_id,
                source,
                agent_label,
                seq,
                session_ref,
                launch_env,
            } => self
                .update_terminal_state(pane_id, |terminal| {
                    let mut mutation =
                        terminal.set_agent_session_ref(source, agent_label, session_ref, seq)?;
                    if terminal.launch_env != launch_env {
                        terminal.launch_env = launch_env;
                        mutation.session_ref_changed = true;
                    }
                    Some(mutation)
                })
                .into_iter()
                .collect(),
            AppEvent::HookMetadataReported {
                pane_id,
                source,
                agent_label,
                applies_to_source,
                title,
                display_agent,
                custom_status,
                state_labels,
                clear_title,
                clear_display_agent,
                clear_custom_status,
                clear_state_labels,
                seq,
                ttl,
            } => self
                .update_terminal_state(pane_id, |terminal| {
                    terminal.set_agent_metadata(crate::terminal::AgentMetadataReport {
                        source,
                        agent_label,
                        applies_to_source,
                        title,
                        display_agent,
                        custom_status,
                        state_labels,
                        clear_title,
                        clear_display_agent,
                        clear_custom_status,
                        clear_state_labels,
                        ttl,
                        seq,
                    })
                })
                .into_iter()
                .collect(),
            AppEvent::HookAuthorityCleared {
                pane_id,
                source,
                seq,
            } => self
                .update_terminal_state(pane_id, |terminal| {
                    terminal.clear_hook_authority_with_mutation(source.as_deref(), seq)
                })
                .into_iter()
                .collect(),
            AppEvent::HookAgentReleased {
                pane_id,
                source,
                agent_label,
                session_ref,
                seq,
                ..
            } => self
                .update_terminal_state(pane_id, |terminal| {
                    terminal.release_agent_with_mutation(&source, &agent_label, session_ref, seq)
                })
                .into_iter()
                .collect(),
            // Intercepted in App::handle_internal_event before reaching this
            // dispatch; never touches AppState.
            AppEvent::ClipboardWrite { .. } => Vec::new(),
            AppEvent::GitStatusRefreshed {
                results,
                cache_updates,
                repo_summaries,
            } => {
                let _ = results;
                let _ = cache_updates;
                let _ = repo_summaries;
                Vec::new()
            }
            AppEvent::PluginCommandFinished { .. } => Vec::new(),
        }
    }

    fn update_terminal_state<F>(&mut self, pane_id: PaneId, update: F) -> Option<PaneStateUpdate>
    where
        F: FnOnce(&mut crate::terminal::TerminalState) -> Option<TerminalStateMutation>,
    {
        self.update_terminal_state_at(pane_id, std::time::Instant::now(), update)
    }

    fn update_terminal_state_at<F>(
        &mut self,
        pane_id: PaneId,
        activity_at: std::time::Instant,
        update: F,
    ) -> Option<PaneStateUpdate>
    where
        F: FnOnce(&mut crate::terminal::TerminalState) -> Option<TerminalStateMutation>,
    {
        let ws_idx = self
            .workspaces
            .iter()
            .position(|ws| ws.pane_state(pane_id).is_some())?;
        let terminal_id = self.workspaces[ws_idx]
            .pane_state(pane_id)?
            .attached_terminal_id
            .clone();
        let mutation = {
            let terminal = self.terminals.get_mut(&terminal_id)?;
            update(terminal)?
        };
        if mutation.session_ref_changed {
            self.mark_session_dirty();
        }
        if mutation.effective_state_change.is_some() {
            let seq = self.next_agent_activity_seq();
            let unix_secs = unix_secs_for_activity_instant(activity_at);
            if let Some(terminal) = self.terminals.get_mut(&terminal_id) {
                terminal.mark_meaningful_agent_activity(seq, unix_secs);
            }
        }
        let change = mutation.effective_state_change?;
        let update = PaneStateUpdate {
            pane_id,
            ws_idx,
            previous_agent_label: change.previous_agent_label.clone(),
            previous_known_agent: change.previous_known_agent,
            previous_state: change.previous_state,
            previous_presentation: change.previous_presentation.clone(),
            agent_label: change.agent_label.clone(),
            known_agent: change.known_agent,
            state: change.state,
            presentation: change.presentation.clone(),
        };
        self.apply_pane_state_change(ws_idx, pane_id, &change);
        Some(update)
    }

    pub(crate) fn publish_pane_process_exit_if_agent(
        &mut self,
        pane_id: PaneId,
    ) -> Option<PaneStateUpdate> {
        let observed_at = std::time::Instant::now();
        self.update_terminal_state(pane_id, |terminal| {
            let agent = terminal.effective_known_agent().or(terminal.detected_agent);
            if agent.is_none() && !terminal.full_lifecycle_hook_authority_active() {
                return None;
            }
            Some(terminal.set_detected_state_with_screen_signals_at(
                agent,
                AgentState::Idle,
                false,
                true,
                false,
                true,
                observed_at,
            ))
        })
    }
    fn apply_pane_state_change(
        &mut self,
        ws_idx: usize,
        pane_id: PaneId,
        change: &EffectiveStateChange,
    ) {
        let is_active_tab = self.pane_is_in_active_tab(ws_idx, pane_id);
        let suppress_active_tab_notifications =
            active_tab_suppresses_notifications(is_active_tab, self.outer_terminal_focus);
        let Some(pane) = self.workspaces[ws_idx]
            .tabs
            .iter_mut()
            .find_map(|tab| tab.panes.get_mut(&pane_id))
        else {
            return;
        };

        if change.state != AgentState::Idle {
            pane.seen = true;
        } else if is_background_completion_transition(change.previous_state, change.state) {
            pane.seen = suppress_active_tab_notifications;
        }

        if let Some(delivery) = self.record_or_deliver_agent_notification(ws_idx, pane_id, change) {
            self.apply_agent_notification_delivery(&delivery);
        }
    }

    fn record_or_deliver_agent_notification(
        &mut self,
        ws_idx: usize,
        pane_id: PaneId,
        change: &EffectiveStateChange,
    ) -> Option<AgentNotificationDelivery> {
        self.pending_agent_notifications.remove(&pane_id);

        let is_active_tab = self.pane_is_in_active_tab(ws_idx, pane_id);
        let suppress_active_tab_notifications =
            active_tab_suppresses_notifications(is_active_tab, self.outer_terminal_focus);

        let client_notification_kind = notification_toast_for_state_change(
            suppress_active_tab_notifications,
            change.previous_state,
            change.state,
        );
        let sound = notification_sound_for_state_change(
            suppress_active_tab_notifications,
            change.previous_state,
            change.state,
        );
        if client_notification_kind.is_none() && sound.is_none() {
            return None;
        }

        let agent_label = change.agent_label.clone()?;
        let kind = client_notification_kind.unwrap_or(match sound {
            Some(crate::sound::Sound::Request) => ToastKind::NeedsAttention,
            Some(crate::sound::Sound::Done) | None => ToastKind::Finished,
        });
        let workspace_id = self.workspaces[ws_idx].id.clone();

        if self.toast_config.delay_seconds == 0 {
            return self.agent_notification_delivery(
                ws_idx,
                pane_id,
                workspace_id,
                agent_label,
                change.known_agent,
                kind,
                change.state,
            );
        }

        self.pending_agent_notifications.insert(
            pane_id,
            PendingAgentNotification {
                pane_id,
                workspace_id,
                agent_label,
                known_agent: change.known_agent,
                kind,
                state: change.state,
                deadline: {
                    let now = std::time::Instant::now();
                    let delay_seconds = self
                        .toast_config
                        .delay_seconds
                        .min(crate::config::MAX_TOAST_DELAY_SECONDS);
                    now.checked_add(std::time::Duration::from_secs(delay_seconds))
                        .unwrap_or(now)
                },
            },
        );
        None
    }

    fn agent_notification_delivery(
        &self,
        ws_idx: usize,
        pane_id: PaneId,
        workspace_id: String,
        agent_label: String,
        known_agent: Option<Agent>,
        kind: ToastKind,
        expected_state: AgentState,
    ) -> Option<AgentNotificationDelivery> {
        let terminal_state = self
            .workspaces
            .get(ws_idx)?
            .pane_state(pane_id)
            .and_then(|pane| self.terminals.get(&pane.attached_terminal_id))?;
        if terminal_state.state != expected_state {
            return None;
        }
        if terminal_state.effective_agent_label() != Some(agent_label.as_str()) {
            return None;
        }

        let is_active_tab = self.pane_is_in_active_tab(ws_idx, pane_id);
        let suppress_active_tab_notifications =
            active_tab_suppresses_notifications(is_active_tab, self.outer_terminal_focus);
        let sound = sound_for_toast_kind(kind, suppress_active_tab_notifications)
            .filter(|_| self.sound.allows(known_agent));
        let build_toast = || {
            let workspace_label = self.workspaces[ws_idx].display_name();
            let context =
                notification_context(&self.workspaces[ws_idx], &workspace_label, ws_idx, pane_id);
            ToastNotification {
                kind,
                title: format!(
                    "{} {}",
                    toast_agent_label(&agent_label),
                    toast_event_text(kind)
                ),
                context,
                position: None,
                target: Some(ToastTarget {
                    workspace_id: workspace_id.clone(),
                    pane_id,
                }),
            }
        };
        let toast = (!is_active_tab).then(build_toast);
        let client_notification = (!suppress_active_tab_notifications).then(build_toast);

        if toast.is_none() && client_notification.is_none() && sound.is_none() {
            return None;
        }

        Some(AgentNotificationDelivery {
            pane_id,
            workspace_id,
            agent_label,
            known_agent,
            kind,
            toast,
            client_notification,
            sound,
        })
    }

    fn apply_agent_notification_delivery(&mut self, delivery: &AgentNotificationDelivery) {
        if self.local_sound_playback {
            if let Some(sound) = delivery.sound {
                crate::sound::play(sound, &self.sound);
            }
        }

        if matches!(
            self.toast_config.delivery,
            crate::config::ToastDelivery::Hako
        ) {
            if let Some(toast) = delivery.toast.clone() {
                self.toast = Some(toast);
            }
        }
    }

    pub fn next_pending_agent_notification_deadline(&self) -> Option<std::time::Instant> {
        self.pending_agent_notifications
            .values()
            .map(|pending| pending.deadline)
            .min()
    }

    pub fn drain_due_agent_notifications(
        &mut self,
        now: std::time::Instant,
    ) -> Vec<AgentNotificationDelivery> {
        let due_panes: Vec<PaneId> = self
            .pending_agent_notifications
            .iter()
            .filter_map(|(&pane_id, pending)| (pending.deadline <= now).then_some(pane_id))
            .collect();
        let mut deliveries = Vec::new();

        for pane_id in due_panes {
            let Some(pending) = self.pending_agent_notifications.remove(&pane_id) else {
                continue;
            };
            let Some(ws_idx) = self
                .workspaces
                .iter()
                .position(|ws| ws.id == pending.workspace_id)
            else {
                continue;
            };
            let Some(delivery) = self.agent_notification_delivery(
                ws_idx,
                pending.pane_id,
                pending.workspace_id,
                pending.agent_label,
                pending.known_agent,
                pending.kind,
                pending.state,
            ) else {
                continue;
            };
            self.apply_agent_notification_delivery(&delivery);
            deliveries.push(delivery);
        }

        deliveries
    }

    fn handle_pane_died(
        &mut self,
        terminal_runtimes: Option<&crate::terminal::TerminalRuntimeRegistry>,
        pane_id: PaneId,
        child_pid: u32,
        exit_success: bool,
    ) {
        self.pending_agent_notifications.remove(&pane_id);
        self.plugin_panes.remove(&pane_id);
        let ws_idx = self
            .workspaces
            .iter()
            .position(|ws| ws.find_tab_index_for_pane(pane_id).is_some());

        let Some(ws_idx) = ws_idx else {
            warn!(pane = pane_id.raw(), "PaneDied for unknown pane");
            return;
        };

        if self
            .selection
            .as_ref()
            .is_some_and(|s| s.pane_id == pane_id)
        {
            self.selection = None;
            self.selection_autoscroll = None;
        }

        let pane_terminal_id = self.terminal_id_for_pane(ws_idx, pane_id);
        if let Some(terminal_id) = pane_terminal_id.as_ref() {
            if self.handle_command_pane_died(
                terminal_runtimes,
                terminal_id,
                child_pid,
                exit_success,
            ) {
                return;
            }
        }

        let workspace_terminal_ids = self.terminal_ids_for_workspace(ws_idx);
        self.pane_id_aliases.retain(|_, alias| *alias != pane_id);
        let should_close_workspace = {
            let ws = &mut self.workspaces[ws_idx];
            ws.remove_pane(pane_id)
        };
        self.mark_session_dirty();

        if should_close_workspace {
            if self.workspaces.len() == 1 {
                self.empty_workspace(ws_idx);
                self.remove_unattached_terminal_ids(workspace_terminal_ids);
            } else {
                self.workspaces.remove(ws_idx);
                self.remove_unattached_terminal_ids(workspace_terminal_ids);
                if let Some(active) = self.active {
                    if active >= self.workspaces.len() {
                        self.active = Some(self.workspaces.len() - 1);
                    }
                }
                if self.selected >= self.workspaces.len() {
                    self.selected = self.workspaces.len() - 1;
                }
            }
        } else {
            self.remove_unattached_terminal_ids(pane_terminal_id);
        }
    }

    fn handle_command_pane_died(
        &mut self,
        terminal_runtimes: Option<&crate::terminal::TerminalRuntimeRegistry>,
        terminal_id: &crate::terminal::TerminalId,
        child_pid: u32,
        exit_success: bool,
    ) -> bool {
        let Some(command_id) = self.command_runs.iter().find_map(|(command_id, run)| {
            (&run.terminal_id == terminal_id).then(|| command_id.clone())
        }) else {
            return false;
        };

        if terminal_runtimes
            .and_then(|runtimes| runtimes.get(terminal_id))
            .is_some_and(|runtime| runtime.child_pid() != child_pid)
        {
            return true;
        }

        if !self.terminal_runtime_shutdowns.contains(terminal_id) {
            self.terminal_runtime_shutdowns.push(terminal_id.clone());
            if let Some(run) = self.command_runs.get_mut(&command_id) {
                run.status = if exit_success {
                    crate::commands::CommandRunStatus::Stopped
                } else {
                    crate::commands::CommandRunStatus::Failed
                };
            }
            self.mark_session_dirty();
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Palette;
    use crate::config::ThemeMode;
    use crate::detect::{Agent, AgentState};
    use crate::terminal_theme::{DefaultColorKind, RgbColor, TerminalTheme};
    use crate::workspace::Workspace;
    use ratatui::layout::Direction;

    fn app_with_workspaces(names: &[&str]) -> AppState {
        let mut state = AppState::test_new();
        for name in names {
            let ws = Workspace::test_new(name);
            state.workspaces.push(ws);
        }
        state.ensure_test_terminals();
        if !state.workspaces.is_empty() {
            state.active = Some(0);
            state.mode = Mode::Terminal;
        }
        state
    }

    #[test]
    fn hunk_terminal_passthrough_covers_terminal_color_sources() {
        assert!(hunk_uses_terminal_color_passthrough("system"));
        assert!(hunk_uses_terminal_color_passthrough("terminal"));
        assert!(!hunk_uses_terminal_color_passthrough("tokyo-night"));
    }

    #[test]
    fn hunk_theme_uses_target_workspace_group_accent() {
        let mut state = app_with_workspaces(&["main", "docs"]);
        let docs_group = state.create_group("Docs".to_string());
        state.move_workspace_to_group(1, docs_group);
        state.set_group_accent(docs_group, Some(crate::config::TerminalAccent::Magenta));

        let (palette, appearance, passthrough_terminal) = state.hunk_diff_theme_for_workspace(1);

        assert_eq!(
            appearance,
            state.theme_appearance_for_mode(state.global_theme_mode)
        );
        assert_eq!(
            passthrough_terminal,
            hunk_uses_terminal_color_passthrough(&state.global_theme_name)
        );
        assert_eq!(palette.panel_bg, state.global_palette.panel_bg);
        assert_eq!(palette.accent, ratatui::style::Color::Magenta);
    }

    fn temp_project(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "hako-app-commands-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn temp_git_repo(name: &str) -> std::path::PathBuf {
        let root = temp_project(name);
        let status = std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(&root)
            .status()
            .unwrap();
        assert!(status.success());
        root
    }

    fn mark_parent_worktree(state: &mut AppState, ws_idx: usize) {
        state.workspaces[ws_idx].worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "hako".into(),
            repo_root: "/repo/hako".into(),
            checkout_path: "/repo/hako".into(),
            is_linked_worktree: false,
        });
    }

    fn mark_linked_worktree(state: &mut AppState, ws_idx: usize) {
        state.workspaces[ws_idx].worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "hako".into(),
            repo_root: "/repo/hako".into(),
            checkout_path: "/repo/hako-linked".into(),
            is_linked_worktree: true,
        });
    }

    #[test]
    fn notification_context_formats_resolved_workspace_label() {
        let state = app_with_workspaces(&["stale"]);
        let root = state.workspaces[0].tabs[0].root_pane;

        assert_eq!(
            notification_context(&state.workspaces[0], "__hako_projects__", 0, root),
            "__hako_projects__ · 1"
        );
    }

    fn selected_word(row: &str, col: u16) -> Option<String> {
        let (start, end) = word_bounds_at_column(row, col)?;
        Some(text_in_cell_range(row, start, end))
    }

    fn selected_url<'a>(row: &'a str, click: &str) -> Option<&'a str> {
        url_at_column(row, col_of(row, click))
    }

    fn text_in_cell_range(row: &str, start_col: u16, end_col: u16) -> String {
        text_cells(row)
            .into_iter()
            .filter(|cell| cell.start_col >= start_col && cell.end_col <= end_col)
            .map(|cell| cell.ch)
            .collect()
    }

    fn col_of(row: &str, needle: &str) -> u16 {
        let byte_idx = row
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} not found in {row:?}"));
        let prefix = &row[..byte_idx];
        prefix
            .chars()
            .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0) as u16)
            .sum()
    }

    fn assert_selects(row: &str, click: &str, expected: &str) {
        assert_eq!(
            selected_word(row, col_of(row, click)).as_deref(),
            Some(expected),
            "row={row:?}, click={click:?}"
        );
    }

    fn assert_selects_nothing(row: &str, click: &str) {
        assert_eq!(
            selected_word(row, col_of(row, click)),
            None,
            "row={row:?}, click={click:?}"
        );
    }

    #[test]
    fn double_click_word_bounds_cover_terminal_text() {
        let cases = [
            (
                "see https://example.com/a-b_c?q=x@y.",
                "example.com",
                "https://example.com/a-b_c?q=x@y",
            ),
            (
                "open \"https://example.com/a,b;c?q=x\";",
                "example.com",
                "https://example.com/a,b;c?q=x",
            ),
            (
                "see https://en.wikipedia.org/wiki/Foo_(bar_(baz)),",
                "wikipedia",
                "https://en.wikipedia.org/wiki/Foo_(bar_(baz))",
            ),
            (
                "see https://example.com/a(b[c{d}e]f),",
                "example.com",
                "https://example.com/a(b[c{d}e]f)",
            ),
            (
                "see (https://example.com/a(b(c)d)))",
                "example.com",
                "https://example.com/a(b(c)d)",
            ),
            (
                "open /tmp/foo-bar/baz_qux/",
                "foo-bar",
                "/tmp/foo-bar/baz_qux/",
            ),
            (
                "open ./src/app/actions.rs:795",
                "actions",
                "./src/app/actions.rs:795",
            ),
            (
                "open ../hako-worktrees/issue-1",
                "hako",
                "../hako-worktrees/issue-1",
            ),
            (
                "edit src/app/actions.rs,then",
                "actions",
                "src/app/actions.rs",
            ),
            (
                "cat \"/tmp/build output/log.txt\"",
                "output",
                "/tmp/build output/log.txt",
            ),
            (
                "cat '/Users/me/Library/Application Support/app/config.json'",
                "Support",
                "/Users/me/Library/Application Support/app/config.json",
            ),
            ("echo 你好-world done", "好", "你好-world"),
            ("先跑 cargo test", "cargo", "cargo"),
            (
                "export PATH=$HOME/.cargo/bin:$PATH",
                "$HOME",
                "PATH=$HOME/.cargo/bin:$PATH",
            ),
            (
                "git checkout feature/foo-bar_baz",
                "foo",
                "feature/foo-bar_baz",
            ),
            ("refs #123 and @owner/name", "#123", "#123"),
            ("refs #123 and @owner/name", "owner", "@owner/name"),
            ("cargo test --package=hako", "--package", "--package=hako"),
            (
                "cargo test app::actions::tests",
                "app::",
                "app::actions::tests",
            ),
            (
                "image ghcr.io/org/app:latest",
                "ghcr",
                "ghcr.io/org/app:latest",
            ),
            ("ERROR [worker-1] request_id=abc-123", "worker", "worker-1"),
            (
                "tmux|newhoo|fixhoo|newmoo|notification|window_bell|hako",
                "newhoo",
                "newhoo",
            ),
            (
                "render_status_line(app, area)",
                "render",
                "render_status_line",
            ),
            ("render_status_line(app, area)", "app", "app"),
            ("render_status_line(app, area)", "area", "area"),
            ("if !enabled {", "enabled", "enabled"),
            ("println!(\"hi\")", "println", "println"),
            ("( master)$", "master", "master"),
            ("regex foo$", "foo", "foo$"),
        ];

        for (row, click, expected) in cases {
            assert_selects(row, click, expected);
        }

        let row = "echo 你好-world done";
        assert_eq!(
            selected_word(row, col_of(row, "好") + 1).as_deref(),
            Some("你好-world")
        );
    }

    #[test]
    fn double_click_word_bounds_ignore_delimiters() {
        for (row, click) in [
            (
                "tmux|newhoo|fixhoo|newmoo|notification|window_bell|hako",
                "|",
            ),
            ("alpha,beta;gamma", ","),
            ("alpha,beta;gamma", ";"),
            ("render_status_line(app, area)", "("),
            ("render_status_line(app, area)", ")"),
            ("if !enabled {", "!"),
            ("if !enabled {", "{"),
            ("(done).", "("),
            ("(done).", "."),
        ] {
            assert_selects_nothing(row, click);
        }
    }

    #[test]
    fn url_at_column_returns_safe_visible_url_only() {
        assert_eq!(
            selected_url("see https://example.com/a(b)c.", "example"),
            Some("https://example.com/a(b)c")
        );
        assert_eq!(
            selected_url("[docs](https://example.com/docs),", "example"),
            Some("https://example.com/docs")
        );
        assert_eq!(
            selected_url("[docs](https://example.com/docs)", "docs"),
            None
        );
        assert_eq!(selected_url("open file:///tmp/report", "file"), None);
    }

    #[test]
    fn navigator_rows_show_tab_nodes_only_for_multi_tab_workspaces() {
        let mut state = app_with_workspaces(&["single", "multi"]);
        state.workspaces[1].test_add_tab(Some("tests"));
        state.ensure_test_terminals();

        state.open_navigator();
        let rows = state.navigator_rows();

        assert!(!rows
            .iter()
            .any(|row| matches!(row.target, NavigatorTarget::Tab { ws_idx: 0, .. })));
        assert!(rows.iter().any(|row| matches!(
            row.target,
            NavigatorTarget::Tab {
                ws_idx: 1,
                tab_idx: 0
            }
        )));
        assert!(rows.iter().any(|row| matches!(
            row.target,
            NavigatorTarget::Tab {
                ws_idx: 1,
                tab_idx: 1
            }
        )));
    }

    #[tokio::test]
    async fn navigator_rows_match_live_root_runtime_cwd_workspace_label() {
        let unique = format!(
            "hako-navigator-runtime-cwd-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let stale_cwd = root.join("issue-264-nix-support");
        let live_cwd = root.join("hako");
        std::fs::create_dir_all(stale_cwd.join(".git")).unwrap();
        std::fs::create_dir_all(live_cwd.join(".git")).unwrap();

        let mut state = AppState::test_new();
        let mut workspace = Workspace::test_new("stale-name");
        workspace.custom_name = None;
        workspace.identity_cwd = stale_cwd.clone();
        let pane = workspace.tabs[0].root_pane;
        state.workspaces = vec![workspace];
        state.ensure_test_terminals();
        let terminal_id = state.workspaces[0].terminal_id(pane).cloned().unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = stale_cwd;

        let (events, _) = tokio::sync::mpsc::channel(4);
        let runtime = crate::terminal::TerminalRuntime::spawn(
            pane,
            24,
            80,
            live_cwd.clone(),
            0,
            crate::terminal_theme::TerminalTheme::default(),
            crate::pane::PaneShellConfig::new("/bin/sh", crate::config::ShellModeConfig::NonLogin),
            &crate::pane::PaneLaunchEnv::default(),
            events,
            std::sync::Arc::new(tokio::sync::Notify::new()),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while runtime.cwd() != Some(live_cwd.clone()) && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let mut runtime_registry = crate::terminal::TerminalRuntimeRegistry::new();
        runtime_registry.insert(terminal_id, runtime);
        state.open_navigator_from(&runtime_registry);
        state.navigator.query = "hako".into();
        let rows = state.navigator_rows_from(&runtime_registry);

        for (_, runtime) in runtime_registry.drain() {
            runtime.shutdown();
        }
        let _ = std::fs::remove_dir_all(root);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "hako (1)");
    }

    #[test]
    fn navigator_rows_include_shell_and_agent_panes() {
        let mut state = app_with_workspaces(&["one"]);
        let shell = state.workspaces[0].tabs[0].root_pane;
        let agent = state.workspaces[0].test_split(Direction::Horizontal);
        state.ensure_test_terminals();

        let terminal_id = state.workspaces[0].terminal_id(agent).cloned().unwrap();
        state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_detected_state(Some(Agent::Claude), AgentState::Working);

        state.open_navigator();
        let rows = state.navigator_rows();

        assert!(rows.iter().any(|row| matches!(
            row.target,
            NavigatorTarget::Pane { pane_id, .. } if pane_id == shell
        )));
        assert!(rows.iter().any(|row| matches!(
            row.target,
            NavigatorTarget::Pane { pane_id, .. } if pane_id == agent
        ) && row.meta.contains("claude")));
    }

    #[test]
    fn accepting_navigator_pane_switches_workspace_tab_and_focus() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let target = state.workspaces[1].tabs[0].root_pane;
        state.open_navigator();
        state.navigator.selected = state
            .navigator_rows()
            .iter()
            .position(|row| {
                matches!(
                    row.target,
                    NavigatorTarget::Pane { pane_id, .. } if pane_id == target
                )
            })
            .unwrap();

        assert!(state.accept_navigator_selection());

        assert_eq!(state.active, Some(1));
        assert_eq!(state.workspaces[1].focused_pane_id(), Some(target));
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn navigator_search_filters_panes_but_keeps_workspace_context() {
        let mut state = app_with_workspaces(&["one"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.workspaces[0].terminal_id(root).cloned().unwrap();
        state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_manual_label("weekly review".into());

        state.open_navigator();
        state.navigator.query = "weekly".into();
        let rows = state.navigator_rows();

        assert!(rows.iter().any(|row| row.is_workspace));
        assert!(rows
            .iter()
            .any(|row| !row.is_workspace && row.label.contains("weekly")));
    }

    #[test]
    fn navigator_state_filter_matches_agent_state_not_label_text() {
        let mut state = app_with_workspaces(&["one"]);
        let shell = state.workspaces[0].tabs[0].root_pane;
        let working = state.workspaces[0].test_split(Direction::Horizontal);
        state.ensure_test_terminals();

        let shell_terminal_id = state.workspaces[0].terminal_id(shell).cloned().unwrap();
        state
            .terminals
            .get_mut(&shell_terminal_id)
            .unwrap()
            .set_manual_label("working notes".into());
        let working_terminal_id = state.workspaces[0].terminal_id(working).cloned().unwrap();
        state
            .terminals
            .get_mut(&working_terminal_id)
            .unwrap()
            .set_detected_state(Some(Agent::Codex), AgentState::Working);

        state.open_navigator();
        state.navigator.state_filter = Some(NavigatorStateFilter::Working);
        let rows = state.navigator_rows();

        assert!(rows.iter().any(|row| matches!(
            row.target,
            NavigatorTarget::Pane { pane_id, .. } if pane_id == working
        )));
        assert!(!rows.iter().any(|row| matches!(
            row.target,
            NavigatorTarget::Pane { pane_id, .. } if pane_id == shell
        )));
    }
    #[test]
    fn git_diff_with_multiple_observed_repos_opens_picker() {
        let first = temp_git_repo("diff-first-observed");
        let second = temp_git_repo("diff-second-observed");
        let mut state = app_with_workspaces(&["multi"]);
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = first.clone();
        let tab_idx = state.workspaces[0].test_add_tab(Some("api"));
        let pane_id = state.workspaces[0].tabs[tab_idx].root_pane;
        let terminal = state.workspaces[0].terminal_id(pane_id).cloned().unwrap();
        state.ensure_test_terminals();
        state.terminals.get_mut(&terminal).unwrap().cwd = second.clone();

        state
            .open_git_diff_panel_for_workspace(&mut terminal_runtimes, 0)
            .expect("multi-repo diff should open picker");

        assert_eq!(state.mode, Mode::GitRepoPicker);
        assert_eq!(state.git_repo_picker.roots, vec![first, second]);
    }

    #[test]
    fn git_diff_observes_direct_child_repos_from_non_git_workspace_cwd() {
        let parent = temp_project("diff-fake-monorepo");
        let first = parent.join("api");
        let second = parent.join("web");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        for root in [&first, &second] {
            let status = std::process::Command::new("git")
                .arg("init")
                .arg("-q")
                .arg(root)
                .status()
                .unwrap();
            assert!(status.success());
        }
        let mut state = app_with_workspaces(&["multi"]);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = parent;

        assert_eq!(
            state.observed_git_repos_for_workspace(&terminal_runtimes, 0),
            vec![first, second]
        );
    }
    #[test]
    fn git_diff_opens_native_tab_named_after_repo_root() {
        let root = temp_git_repo("diff-native-tab");
        std::fs::write(root.join("changed.txt"), "changed\n").unwrap();
        let mut state = app_with_workspaces(&["web"]);
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = root.clone();

        state
            .open_git_diff_panel_for_workspace(&mut terminal_runtimes, 0)
            .expect("single repo diff should open native tab");

        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.workspaces[0].tabs.len(), 2);
        assert_eq!(
            state.workspaces[0].active_tab().unwrap().display_name(),
            format!("diff {}", root.file_name().unwrap().to_string_lossy())
        );
        let pane_id = state.workspaces[0].active_tab().unwrap().root_pane;
        assert!(state.workspaces[0]
            .active_tab()
            .unwrap()
            .panes
            .get(&pane_id)
            .unwrap()
            .native_diff()
            .is_some());
    }

    #[test]
    fn git_diff_opens_native_tab_after_last_terminal_tab_closed() {
        let root = temp_git_repo("diff-empty-workspace");
        std::fs::write(root.join("changed.txt"), "changed\n").unwrap();
        let mut state = app_with_workspaces(&["web"]);
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        state.workspaces[0].identity_cwd = std::path::PathBuf::from("/stale/identity");
        state.workspaces[0].default_cwd = root.clone();
        assert!(state.workspaces[0].close_tab_allow_empty(0));

        state
            .open_git_diff_panel_for_workspace(&mut terminal_runtimes, 0)
            .expect("empty workspace should still open native diff");

        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.workspaces[0].tabs.len(), 1);
        assert_eq!(
            state.workspaces[0].active_tab().unwrap().display_name(),
            format!("diff {}", root.file_name().unwrap().to_string_lossy())
        );
        let pane_id = state.workspaces[0].active_tab().unwrap().root_pane;
        assert!(state.workspaces[0]
            .active_tab()
            .unwrap()
            .panes
            .get(&pane_id)
            .unwrap()
            .native_diff()
            .is_some());
    }

    #[test]
    fn git_diff_target_can_use_non_focused_workspace_repo_cwd() {
        let root = temp_git_repo("diff-extra-root");
        let mut state = app_with_workspaces(&["web"]);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = root.clone();

        assert_eq!(
            state.git_diff_target_for_workspace(&terminal_runtimes, 0),
            Some(root)
        );
    }

    #[test]
    fn git_diff_target_uses_focused_pane_git_root() {
        let root = temp_git_repo("diff-root");
        let nested = root.join("apps/web");
        std::fs::create_dir_all(&nested).unwrap();
        let mut state = app_with_workspaces(&["web"]);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = nested;

        assert_eq!(state.git_diff_target(&terminal_runtimes), Some((root, 0)));
    }

    #[test]
    fn git_diff_target_uses_selected_space_in_navigate_mode() {
        let first = temp_git_repo("diff-first");
        let second = temp_git_repo("diff-second");
        let mut state = app_with_workspaces(&["first", "second"]);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        state.mode = Mode::Navigate;
        state.selected = 1;
        let first_pane = state.workspaces[0].tabs[0].root_pane;
        let second_pane = state.workspaces[1].tabs[0].root_pane;
        let first_terminal_id = state.terminal_id_for_pane(0, first_pane).unwrap();
        let second_terminal_id = state.terminal_id_for_pane(1, second_pane).unwrap();
        state.terminals.get_mut(&first_terminal_id).unwrap().cwd = first;
        state.terminals.get_mut(&second_terminal_id).unwrap().cwd = second.clone();

        assert_eq!(state.git_diff_target(&terminal_runtimes), Some((second, 1)));
    }

    #[test]
    fn command_catalog_refresh_uses_pane_cwd_project_roots_in_scope() {
        let project = temp_project("scope");
        std::fs::write(
            project.join("package.json"),
            r#"{"scripts":{"dev":"vite"}}"#,
        )
        .unwrap();
        let nested = project.join("apps/web");
        std::fs::create_dir_all(&nested).unwrap();
        let mut state = app_with_workspaces(&["web"]);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = nested;

        assert!(state.refresh_command_catalog(&terminal_runtimes));

        assert_eq!(state.command_catalog.len(), 1);
        assert_eq!(state.command_catalog[0].name, "dev");
        assert_eq!(state.command_catalog[0].root, project);
    }

    #[test]
    fn command_catalog_refresh_uses_current_workspace_when_activity_scope_is_all() {
        let current = temp_project("current-scope");
        let other = temp_project("other-scope");
        std::fs::write(
            current.join("package.json"),
            r#"{"scripts":{"dev":"vite"}}"#,
        )
        .unwrap();
        std::fs::write(
            other.join("package.json"),
            r#"{"scripts":{"build":"vite build"}}"#,
        )
        .unwrap();
        let mut state = app_with_workspaces(&["current", "other"]);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        state.agent_panel_scope = crate::app::state::AgentPanelScope::AllWorkspaces;
        let current_pane = state.workspaces[0].tabs[0].root_pane;
        let other_pane = state.workspaces[1].tabs[0].root_pane;
        let current_terminal_id = state.terminal_id_for_pane(0, current_pane).unwrap();
        let other_terminal_id = state.terminal_id_for_pane(1, other_pane).unwrap();
        state.terminals.get_mut(&current_terminal_id).unwrap().cwd = current.clone();
        state.terminals.get_mut(&other_terminal_id).unwrap().cwd = other;

        assert!(state.refresh_command_catalog(&terminal_runtimes));

        assert_eq!(state.command_catalog.len(), 1);
        assert_eq!(state.command_catalog[0].name, "dev");
        assert_eq!(state.command_catalog[0].root, current);
    }

    fn project_command(
        root: std::path::PathBuf,
        name: &str,
        command: &str,
    ) -> crate::commands::ProjectCommand {
        crate::commands::ProjectCommand {
            id: format!("{}:package.json:{name}", root.display()),
            root,
            source: crate::commands::CommandSource::PackageJson,
            name: name.to_string(),
            command: command.to_string(),
            confidence: crate::commands::CommandConfidence::Explicit,
        }
    }

    async fn wait_for_runtime_pid(
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        terminal_id: &crate::terminal::TerminalId,
    ) {
        for _ in 0..50 {
            if terminal_runtimes
                .get(terminal_id)
                .is_some_and(|runtime| runtime.child_pid() != 0)
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    fn drain_runtime_shutdowns(
        state: &mut AppState,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
    ) {
        for terminal_id in state.terminal_runtime_shutdowns.drain(..) {
            if let Some(runtime) = terminal_runtimes.remove(&terminal_id) {
                runtime.shutdown();
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn project_command_runs_in_managed_pane_and_can_stop() {
        let project = temp_project("run");
        std::fs::write(
            project.join("package.json"),
            r#"{"scripts":{"dev":"sleep 30"}}"#,
        )
        .unwrap();
        let mut state = app_with_workspaces(&["web"]);
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let root_terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&root_terminal_id).unwrap().cwd = project.clone();
        let command = project_command(project, "dev", "sleep 30");
        let command_id = command.id.clone();
        state.command_catalog = vec![command];

        state
            .run_project_command(&mut terminal_runtimes, &command_id)
            .unwrap();

        let run = state.command_runs.get(&command_id).unwrap();
        let command_terminal_id = run.terminal_id.clone();
        assert_eq!(run.status, crate::commands::CommandRunStatus::Running);
        assert!(terminal_runtimes.contains_key(&command_terminal_id));
        assert_eq!(state.workspaces[0].tabs.len(), 2);
        assert_eq!(state.workspaces[0].active_tab, 1);
        assert_eq!(state.workspaces[0].tabs[1].display_name(), "dev");
        wait_for_runtime_pid(&terminal_runtimes, &command_terminal_id).await;
        assert_ne!(
            terminal_runtimes
                .get(&command_terminal_id)
                .map(|runtime| runtime.child_pid()),
            Some(0)
        );
        assert!(state.focus_command_run(&command_id));

        assert!(state.stop_project_command(&mut terminal_runtimes, &command_id));

        let run = state.command_runs.get(&command_id).unwrap();
        assert_eq!(run.status, crate::commands::CommandRunStatus::Stopped);
        assert!(!terminal_runtimes.contains_key(&command_terminal_id));

        state
            .run_project_command(&mut terminal_runtimes, &command_id)
            .unwrap();

        let run = state.command_runs.get(&command_id).unwrap();
        assert_eq!(run.status, crate::commands::CommandRunStatus::Running);
        assert_eq!(&run.terminal_id, &command_terminal_id);
        assert_eq!(state.workspaces[0].tabs.len(), 2);
        assert_eq!(state.workspaces[0].active_tab, 1);
        assert!(terminal_runtimes.contains_key(&command_terminal_id));
        wait_for_runtime_pid(&terminal_runtimes, &command_terminal_id).await;
        assert!(state.stop_project_command(&mut terminal_runtimes, &command_id));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn command_pane_exit_retains_tab_and_records_failure() {
        let project = temp_project("exit-failure");
        let mut state = app_with_workspaces(&["web"]);
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let root_terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&root_terminal_id).unwrap().cwd = project.clone();
        let command = project_command(project, "check", "false");
        let command_id = command.id.clone();
        state.command_catalog = vec![command];

        state
            .run_project_command(&mut terminal_runtimes, &command_id)
            .unwrap();

        let terminal_id = state
            .command_runs
            .get(&command_id)
            .unwrap()
            .terminal_id
            .clone();
        wait_for_runtime_pid(&terminal_runtimes, &terminal_id).await;
        let (_, _, pane_id) = state.command_terminal_target(&terminal_id).unwrap();
        let child_pid = terminal_runtimes.get(&terminal_id).unwrap().child_pid();

        state.handle_pane_died(Some(&terminal_runtimes), pane_id, child_pid, false);
        drain_runtime_shutdowns(&mut state, &mut terminal_runtimes);

        assert_eq!(state.workspaces[0].tabs.len(), 2);
        assert!(state.terminals.contains_key(&terminal_id));
        assert!(!terminal_runtimes.contains_key(&terminal_id));
        assert_eq!(
            state.command_runs.get(&command_id).unwrap().status,
            crate::commands::CommandRunStatus::Failed
        );
        assert!(state.command_terminal_target(&terminal_id).is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stale_command_pane_exit_does_not_stop_restarted_command() {
        let project = temp_project("stale-exit");
        let mut state = app_with_workspaces(&["web"]);
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let root_terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&root_terminal_id).unwrap().cwd = project.clone();
        let command = project_command(project, "dev", "sleep 30");
        let command_id = command.id.clone();
        state.command_catalog = vec![command];

        state
            .run_project_command(&mut terminal_runtimes, &command_id)
            .unwrap();
        let terminal_id = state
            .command_runs
            .get(&command_id)
            .unwrap()
            .terminal_id
            .clone();
        wait_for_runtime_pid(&terminal_runtimes, &terminal_id).await;
        assert!(state.stop_project_command(&mut terminal_runtimes, &command_id));

        state
            .run_project_command(&mut terminal_runtimes, &command_id)
            .unwrap();
        wait_for_runtime_pid(&terminal_runtimes, &terminal_id).await;
        let (_, _, pane_id) = state.command_terminal_target(&terminal_id).unwrap();
        let current_pid = terminal_runtimes.get(&terminal_id).unwrap().child_pid();

        state.handle_pane_died(
            Some(&terminal_runtimes),
            pane_id,
            current_pid.saturating_add(1),
            false,
        );

        assert_eq!(
            state.command_runs.get(&command_id).unwrap().status,
            crate::commands::CommandRunStatus::Running
        );
        assert!(terminal_runtimes.contains_key(&terminal_id));
        assert!(state.command_terminal_target(&terminal_id).is_some());
        assert!(state.stop_project_command(&mut terminal_runtimes, &command_id));
    }

    #[test]
    fn command_run_refresh_stops_missing_owned_runtime() {
        let mut state = app_with_workspaces(&["web"]);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let command_id = "missing-runtime".to_string();
        state.command_runs.insert(
            command_id.clone(),
            crate::commands::CommandRun {
                command_id: command_id.clone(),
                terminal_id: crate::terminal::TerminalId::alloc(),
                status: crate::commands::CommandRunStatus::Running,
            },
        );

        assert!(state.refresh_command_run_statuses(&terminal_runtimes));

        assert_eq!(
            state.command_runs.get(&command_id).unwrap().status,
            crate::commands::CommandRunStatus::Stopped
        );
    }

    #[test]
    fn visible_workspace_indices_only_include_active_group() {
        let mut state = app_with_workspaces(&["one", "two", "three"]);
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(1, side_group);

        assert_eq!(state.visible_workspace_indices(), vec![0, 2]);

        state.switch_group(side_group);

        assert_eq!(state.visible_workspace_indices(), vec![1]);
        assert_eq!(state.active, Some(1));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn switching_group_applies_group_accent_override() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(1, side_group);
        state.set_group_accent(side_group, Some(crate::config::TerminalAccent::Red));
        state.switch_group(0);

        assert_eq!(state.theme_name, state.global_theme_name);
        assert_eq!(state.palette.accent, state.global_palette.accent);

        state.switch_group(side_group);

        assert_eq!(state.theme_name, state.global_theme_name);
        assert_eq!(state.palette.accent, state.global_palette.accent);
        assert_eq!(
            state.active_workspace_accent_color(),
            ratatui::style::Color::LightRed
        );
        assert_eq!(state.palette.panel_bg, state.global_palette.panel_bg);
    }

    #[test]
    fn all_spaces_theme_accent_follows_active_workspace_group() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(1, side_group);
        state.set_group_accent(0, Some(crate::config::TerminalAccent::Blue));
        state.set_group_accent(side_group, Some(crate::config::TerminalAccent::Red));
        state.group_filter_enabled = false;
        state.active_group = 0;
        state.active = Some(1);

        state.apply_effective_theme();

        assert_eq!(state.palette.accent, state.global_palette.accent);
        assert_eq!(
            state.active_workspace_accent_color(),
            ratatui::style::Color::LightRed
        );

        state.switch_workspace(0);

        assert_eq!(state.palette.accent, state.global_palette.accent);
        assert_eq!(
            state.active_workspace_accent_color(),
            state.group_accent_color(0)
        );
    }

    #[test]
    fn system_theme_mode_uses_terminal_background() {
        let mut state = app_with_workspaces(&["one"]);
        state.global_light_theme_name = "gruvbox-light".to_string();
        state.global_dark_theme_name = "gruvbox".to_string();
        state.global_theme_mode = ThemeMode::System;
        state.host_terminal_theme = TerminalTheme::default().with_color(
            DefaultColorKind::Background,
            RgbColor {
                r: 245,
                g: 245,
                b: 245,
            },
        );

        state.refresh_global_palette();
        state.apply_effective_theme();

        assert_eq!(state.theme_name, "gruvbox-light");
        assert_eq!(state.palette.panel_bg, Palette::gruvbox_light().panel_bg);
    }

    #[test]
    fn clearing_group_accent_follows_global_accent() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(1, side_group);
        state.set_group_accent(side_group, Some(crate::config::TerminalAccent::Cyan));
        state.switch_group(side_group);

        state.global_palette = Palette::dracula();
        state.global_theme_name = "dracula".to_string();
        state.set_group_accent(side_group, None);

        assert_eq!(state.theme_name, "dracula");
        assert_eq!(state.palette.accent, Palette::dracula().accent);
    }

    #[test]
    fn workspace_navigation_stays_inside_active_group() {
        let mut state = app_with_workspaces(&["one", "two", "three"]);
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(1, side_group);
        state.switch_workspace(0);

        state.next_workspace();
        assert_eq!(state.active, Some(2));

        state.next_workspace();
        assert_eq!(state.active, Some(0));

        state.previous_workspace();
        assert_eq!(state.active, Some(2));
    }

    #[test]
    fn apply_workspace_git_statuses_updates_matching_workspace() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let first_id = state.workspaces[0].id.clone();
        let first_cwd = state.workspaces[0].resolved_identity_cwd().unwrap();
        let first_cwd_fingerprint = state.workspaces[0].git_status_cwds();
        let second_id = state.workspaces[1].id.clone();

        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let changed = state.apply_workspace_git_statuses(
            &terminal_runtimes,
            vec![WorkspaceGitStatus {
                workspace_id: first_id,
                resolved_identity_cwd: first_cwd,
                cwd_fingerprint: first_cwd_fingerprint,
                branch: Some("main".into()),
                ahead_behind: Some((2, 1)),
                work_summary: Some(GitWorkSummary {
                    repo_count: 1,
                    modified: 2,
                    ..GitWorkSummary::default()
                }),
                space: None,
            }],
        );

        assert!(changed);
        assert_eq!(state.workspaces[0].branch().as_deref(), Some("main"));
        assert_eq!(state.workspaces[0].git_ahead_behind(), Some((2, 1)));
        assert_eq!(state.workspaces[0].git_work_summary_label(), "~2");
        assert_eq!(state.workspaces[1].id, second_id);
        assert_eq!(state.workspaces[1].git_ahead_behind(), None);
    }

    #[test]
    fn apply_workspace_git_statuses_ignores_stale_cwd() {
        let mut state = app_with_workspaces(&["one"]);
        let workspace_id = state.workspaces[0].id.clone();
        state.workspaces[0].cached_git_branch = Some("old".into());
        state.workspaces[0].cached_git_ahead_behind = Some((1, 0));

        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let changed = state.apply_workspace_git_statuses(
            &terminal_runtimes,
            vec![WorkspaceGitStatus {
                workspace_id,
                resolved_identity_cwd: std::path::PathBuf::from("/definitely/not/current"),
                cwd_fingerprint: state.workspaces[0].git_status_cwds(),
                branch: Some("main".into()),
                ahead_behind: Some((0, 1)),
                work_summary: Some(GitWorkSummary {
                    repo_count: 1,
                    added: 1,
                    ..GitWorkSummary::default()
                }),
                space: None,
            }],
        );

        assert!(!changed);
        assert_eq!(state.workspaces[0].branch().as_deref(), Some("old"));
        assert_eq!(state.workspaces[0].git_ahead_behind(), Some((1, 0)));
    }

    #[test]
    fn apply_workspace_git_statuses_clears_missing_git_status() {
        let mut state = app_with_workspaces(&["one"]);
        let workspace_id = state.workspaces[0].id.clone();
        let cwd = state.workspaces[0].resolved_identity_cwd().unwrap();
        let cwd_fingerprint = state.workspaces[0].git_status_cwds();
        state.workspaces[0].cached_git_branch = Some("main".into());
        state.workspaces[0].cached_git_ahead_behind = Some((1, 2));
        state.workspaces[0].cached_git_work_summary = Some(GitWorkSummary {
            repo_count: 1,
            modified: 1,
            ..GitWorkSummary::default()
        });

        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let changed = state.apply_workspace_git_statuses(
            &terminal_runtimes,
            vec![WorkspaceGitStatus {
                workspace_id,
                resolved_identity_cwd: cwd,
                cwd_fingerprint,
                branch: None,
                ahead_behind: None,
                work_summary: None,
                space: None,
            }],
        );

        assert!(changed);
        assert_eq!(state.workspaces[0].branch(), None);
        assert_eq!(state.workspaces[0].git_ahead_behind(), None);
        assert_eq!(state.workspaces[0].git_work_summary_label(), "");
    }

    #[test]
    fn update_ready_sets_explicit_upgrade_toast() {
        let mut state = AppState::test_new();
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;

        let updates = state.handle_app_event(crate::events::AppEvent::UpdateReady {
            version: "0.5.0".into(),
            install_command: "hako update".into(),
        });

        assert!(updates.is_empty());
        assert_eq!(state.update_available.as_deref(), Some("0.5.0"));
        assert!(state.latest_release_notes_available);
        let toast = state.toast.as_ref().expect("update toast");
        assert_eq!(toast.title, "v0.5.0 available");
        assert_eq!(toast.context, "detach, then run `hako update`");
    }

    fn mark_agent(state: &mut AppState, ws_idx: usize, tab_idx: usize, pane_id: PaneId) {
        state.ensure_test_terminals();
        let terminal_id = state.workspaces[ws_idx].tabs[tab_idx]
            .panes
            .get(&pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        if let Some(terminal) = state.terminals.get_mut(&terminal_id) {
            terminal.set_detected_state(Some(Agent::Pi), AgentState::Idle);
        }
    }

    #[test]
    fn next_agent_cycles_agent_panel_entries_in_all_scope() {
        let mut first = Workspace::test_new("one");
        let first_root = first.tabs[0].root_pane;
        let first_second = first.test_split(Direction::Horizontal);
        first.tabs[0].layout.focus_pane(first_root);
        let second = Workspace::test_new("two");
        let second_root = second.tabs[0].root_pane;

        let mut state = AppState::test_new();
        state.workspaces = vec![first, second];
        state.ensure_test_terminals();
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Terminal;
        state.agent_panel_scope = crate::app::state::AgentPanelScope::AllWorkspaces;
        mark_agent(&mut state, 0, 0, first_root);
        mark_agent(&mut state, 0, 0, first_second);
        mark_agent(&mut state, 1, 0, second_root);

        state.next_agent();
        assert_eq!(state.active, Some(0));
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(first_second));

        state.next_agent();
        assert_eq!(state.active, Some(1));
        assert_eq!(state.workspaces[1].focused_pane_id(), Some(second_root));

        state.previous_agent();
        assert_eq!(state.active, Some(0));
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(first_second));
    }

    #[test]
    fn focus_agent_entry_uses_agent_panel_order() {
        let mut first = Workspace::test_new("one");
        let first_root = first.tabs[0].root_pane;
        let first_second = first.test_split(Direction::Horizontal);
        first.tabs[0].layout.focus_pane(first_root);
        let second = Workspace::test_new("two");
        let second_root = second.tabs[0].root_pane;

        let mut state = AppState::test_new();
        state.workspaces = vec![first, second];
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Terminal;
        state.agent_panel_scope = crate::app::state::AgentPanelScope::AllWorkspaces;
        mark_agent(&mut state, 0, 0, first_root);
        mark_agent(&mut state, 0, 0, first_second);
        mark_agent(&mut state, 1, 0, second_root);

        assert!(state.focus_agent_entry(2));

        assert_eq!(state.active, Some(1));
        assert_eq!(state.workspaces[1].focused_pane_id(), Some(second_root));
    }

    #[test]
    fn next_agent_cycles_only_current_scope_entries() {
        let mut first = Workspace::test_new("one");
        let first_root = first.tabs[0].root_pane;
        let first_second = first.test_split(Direction::Horizontal);
        first.tabs[0].layout.focus_pane(first_second);
        let second = Workspace::test_new("two");
        let second_root = second.tabs[0].root_pane;

        let mut state = AppState::test_new();
        state.workspaces = vec![first, second];
        state.ensure_test_terminals();
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Terminal;
        state.agent_panel_scope = crate::app::state::AgentPanelScope::CurrentWorkspace;
        mark_agent(&mut state, 0, 0, first_root);
        mark_agent(&mut state, 0, 0, first_second);
        mark_agent(&mut state, 1, 0, second_root);

        state.next_agent();

        assert_eq!(state.active, Some(0));
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(first_root));
    }

    #[test]
    fn previous_agent_keeps_wrapped_target_visible_in_agent_panel() {
        let mut workspace = Workspace::test_new("one");
        let root = workspace.tabs[0].root_pane;
        for idx in 1..8 {
            workspace.test_add_tab(Some(&format!("tab-{idx}")));
        }

        let mut state = AppState::test_new();
        state.workspaces = vec![workspace];
        state.ensure_test_terminals();
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Terminal;
        state.agent_panel_scope = crate::app::state::AgentPanelScope::CurrentWorkspace;
        for tab_idx in 0..state.workspaces[0].tabs.len() {
            let pane_id = state.workspaces[0].tabs[tab_idx].root_pane;
            mark_agent(&mut state, 0, tab_idx, pane_id);
        }
        state.workspaces[0].tabs[0].layout.focus_pane(root);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 80, 14));

        state.previous_agent();

        let last_idx = state.workspaces[0].tabs.len() - 1;
        assert_eq!(state.workspaces[0].active_tab, last_idx);
        assert!(state.agent_panel_scroll > 0);
    }

    #[test]
    fn switch_workspace_updates_active_and_selected() {
        let mut state = app_with_workspaces(&["a", "b", "c"]);
        state.switch_workspace(2);
        assert_eq!(state.active, Some(2));
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn switch_workspace_keeps_selected_visible_in_scrolled_sidebar() {
        let mut state = app_with_workspaces(&["a", "b", "c", "d", "e", "f", "g", "h"]);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 80, 14));

        state.switch_workspace(7);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 80, 14));

        assert!(state
            .view
            .workspace_card_areas
            .iter()
            .any(|card| card.ws_idx == 7));
    }

    #[test]
    fn switching_workspace_keeps_all_mode_group_headers_visible() {
        let mut state = app_with_workspaces(&["charliezugasti", "hako", "hako 2"]);
        let group_two = state.create_group("group 2".to_string());
        state.move_workspace_to_group(1, group_two);
        state.move_workspace_to_group(2, group_two);
        state.group_filter_enabled = false;
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 140, 80));

        state.switch_workspace(2);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 140, 80));

        assert!(state
            .view
            .workspace_group_header_areas
            .iter()
            .any(|header| header.group_idx == 0));
        assert!(state
            .view
            .workspace_group_header_areas
            .iter()
            .any(|header| header.group_idx == group_two));

        state.switch_workspace(0);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 140, 20));

        assert!(state
            .view
            .workspace_group_header_areas
            .iter()
            .any(|header| header.group_idx == 0));
        assert!(state
            .view
            .workspace_group_header_areas
            .iter()
            .any(|header| header.group_idx == group_two));
    }

    #[test]
    fn switch_workspace_marks_panes_seen() {
        let mut state = app_with_workspaces(&["a", "b"]);
        // Mark a pane in workspace 1 as unseen
        let id = *state.workspaces[1].panes.keys().next().unwrap();
        state.workspaces[1].panes.get_mut(&id).unwrap().seen = false;

        state.switch_workspace(1);
        assert!(state.workspaces[1].panes.get(&id).unwrap().seen);
    }

    #[test]
    fn switch_workspace_out_of_bounds_is_noop() {
        let mut state = app_with_workspaces(&["a"]);
        state.switch_workspace(5);
        assert_eq!(state.active, Some(0));
    }
    #[test]
    fn move_group_reorders_without_changing_active_group() {
        let mut state = app_with_workspaces(&["a", "b"]);
        let work_group = state.create_group("work".to_string());
        state.create_group("ops".to_string());
        state.active_group = work_group;
        state.set_group_accent(work_group, Some(crate::config::TerminalAccent::Red));
        let active_group_id = state.groups[work_group].id.clone();
        let active_accent = state.active_workspace_accent_color();

        state.move_group(work_group, state.groups.len());

        let names: Vec<_> = state
            .groups
            .iter()
            .map(|group| group.name.as_str())
            .collect();
        assert_eq!(names, vec!["group 1", "ops", "work"]);
        assert_eq!(state.groups[state.active_group].id, active_group_id);
        assert_eq!(state.active_workspace_accent_color(), active_accent);
    }

    #[test]
    fn move_workspace_reorders_without_changing_logical_selection() {
        let mut state = app_with_workspaces(&["a", "b", "c"]);
        let active_id = state.workspaces[1].id.clone();
        let selected_id = state.workspaces[2].id.clone();
        state.active = Some(1);
        state.selected = 2;

        state.move_workspace(1, 0);

        let names: Vec<_> = state
            .workspaces
            .iter()
            .map(|ws| ws.display_name())
            .collect();
        assert_eq!(names, vec!["b", "a", "c"]);
        assert_eq!(state.active, Some(0));
        assert_eq!(state.selected, 2);
        assert_eq!(state.workspaces[state.active.unwrap()].id, active_id);
        assert_eq!(state.workspaces[state.selected].id, selected_id);
    }

    #[test]
    fn move_workspace_accepts_insert_at_end() {
        let mut state = app_with_workspaces(&["a", "b", "c"]);

        state.move_workspace(0, state.workspaces.len());

        let names: Vec<_> = state
            .workspaces
            .iter()
            .map(|ws| ws.display_name())
            .collect();
        assert_eq!(names, vec!["b", "c", "a"]);
    }

    #[test]
    fn close_workspace_adjusts_indices() {
        let mut state = app_with_workspaces(&["a", "b", "c"]);
        state.selected = 1;
        state.active = Some(1);

        state.close_selected_workspace();

        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(state.selected, 1);
        assert_eq!(state.active, Some(1));
        assert_eq!(state.workspaces[1].custom_name.as_deref(), Some("c"));
    }

    #[test]
    fn close_last_workspace_deletes_space_and_leaves_empty_group() {
        let mut state = app_with_workspaces(&["only"]);
        let terminal_id = state
            .terminal_id_for_pane(0, state.workspaces[0].tabs[0].root_pane)
            .unwrap();
        state.selected = 0;
        state.close_selected_workspace();

        assert!(state.workspaces.is_empty());
        assert_eq!(state.active, None);
        assert_eq!(state.selected, 0);
        assert!(!state.terminals.contains_key(&terminal_id));
    }

    #[test]
    fn close_workspace_at_end_adjusts_selected() {
        let mut state = app_with_workspaces(&["a", "b"]);
        state.selected = 1;
        state.active = Some(1);

        state.close_selected_workspace();

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.selected, 0);
        assert_eq!(state.active, Some(0));
    }

    #[test]
    fn closing_last_tab_keeps_active_workspace_empty_even_when_confirm_close_is_enabled() {
        let mut state = app_with_workspaces(&["a", "b"]);
        let active_terminal_id = state
            .terminal_id_for_pane(1, state.workspaces[1].tabs[0].root_pane)
            .unwrap();
        state.active = Some(1);
        state.selected = 0;
        state.confirm_close = true;

        state.close_tab();

        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.selected, 0);
        assert_eq!(state.active, Some(1));
        assert_eq!(state.workspaces.len(), 2);
        assert!(state.workspaces[1].tabs.is_empty());
        assert!(!state.terminals.contains_key(&active_terminal_id));
    }

    #[test]
    fn closing_last_tab_without_confirmation_keeps_active_workspace_empty() {
        let mut state = app_with_workspaces(&["a", "b"]);
        state.active = Some(1);
        state.selected = 0;
        state.confirm_close = false;

        state.close_tab();

        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(state.workspaces[0].display_name(), "a");
        assert_eq!(state.workspaces[1].display_name(), "b");
        assert!(state.workspaces[1].tabs.is_empty());
    }

    #[test]
    fn pane_died_last_pane_removes_workspace() {
        let mut state = app_with_workspaces(&["a", "b"]);
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();

        state.handle_pane_died(None, pane_id, 0, true);

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].custom_name.as_deref(), Some("b"));
    }

    #[test]
    fn pane_died_last_workspace_keeps_empty_space_active() {
        let mut state = app_with_workspaces(&["only"]);
        state.mode = Mode::Terminal;
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();

        state.handle_pane_died(None, pane_id, 0, true);

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.active, Some(0));
        assert_eq!(state.mode, Mode::Terminal);
        assert!(state.workspaces[0].tabs.is_empty());
    }

    #[test]
    fn pane_died_multi_pane_keeps_workspace() {
        let mut state = app_with_workspaces(&["test"]);
        let second_id = state.workspaces[0].test_split(Direction::Horizontal);

        state.handle_pane_died(None, second_id, 0, true);

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].panes.len(), 1);
    }

    #[test]
    fn pane_died_unknown_pane_is_noop() {
        let mut state = app_with_workspaces(&["test"]);
        let fake_id = PaneId::from_raw(9999);

        state.handle_pane_died(None, fake_id, 0, true);

        assert_eq!(state.workspaces.len(), 1);
    }

    #[test]
    fn pane_died_unrelated_pane_preserves_selection() {
        // Two workspaces; user is selecting text in workspace 0.
        // A pane in workspace 1 dies — selection must be preserved.
        let mut state = app_with_workspaces(&["active", "bg"]);
        let active_pane = *state.workspaces[0].panes.keys().next().unwrap();
        let bg_pane = *state.workspaces[1].panes.keys().next().unwrap();

        state.selection = Some(crate::selection::Selection::anchor(active_pane, 0, 0, None));
        state.selection_autoscroll = Some(crate::app::state::SelectionAutoscroll {
            direction: crate::app::state::SelectionAutoscrollDirection::Down,
            last_mouse_screen_col: 0,
            last_mouse_screen_row: 23,
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
        });

        state.handle_pane_died(None, bg_pane, 0, true);

        assert!(state.selection.is_some());
        assert!(state.selection_autoscroll.is_some());
    }

    #[test]
    fn pane_died_same_pane_clears_selection() {
        let mut state = app_with_workspaces(&["test"]);
        let first_id = state.workspaces[0].tabs[0].root_pane;
        let second_id = state.workspaces[0].test_split(Direction::Horizontal);

        state.selection = Some(crate::selection::Selection::anchor(second_id, 0, 0, None));
        state.selection_autoscroll = Some(crate::app::state::SelectionAutoscroll {
            direction: crate::app::state::SelectionAutoscrollDirection::Down,
            last_mouse_screen_col: 0,
            last_mouse_screen_row: 23,
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
        });

        state.handle_pane_died(None, second_id, 0, true);

        // first_id still alive, workspace stays, but selection was on the dying pane
        assert!(state.selection.is_none());
        assert!(state.selection_autoscroll.is_none());
        assert_eq!(state.workspaces[0].panes.len(), 1);
        assert_eq!(state.workspaces[0].panes.keys().next().unwrap(), &first_id);
    }

    #[test]
    fn state_changed_updates_pane() {
        let mut state = app_with_workspaces(&["test"]);
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        let terminal_id = state.workspaces[0]
            .panes
            .get(&pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        let terminal = state.terminals.get(&terminal_id).unwrap();
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(terminal.detected_agent, Some(Agent::Pi));
    }

    #[test]
    fn state_changed_idle_in_background_marks_unseen() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();

        // First set it to Working
        let bg_terminal_id = state.workspaces[1]
            .panes
            .get(&bg_pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        state.terminals.get_mut(&bg_terminal_id).unwrap().state = AgentState::Working;

        // Now transition to Idle while in background
        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        let pane = state.workspaces[1].panes.get(&bg_pane_id).unwrap();
        assert!(!pane.seen);
    }

    #[test]
    fn active_tab_completion_marks_pane_seen() {
        let mut state = app_with_workspaces(&["active"]);
        state.active = Some(0);
        state.outer_terminal_focus = Some(true);
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();
        let terminal_id = state.workspaces[0]
            .panes
            .get(&pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        state.terminals.get_mut(&terminal_id).unwrap().state = AgentState::Working;
        state.workspaces[0].panes.get_mut(&pane_id).unwrap().seen = false;

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        let terminal = state.terminals.get(&terminal_id).unwrap();
        assert_eq!(terminal.state, AgentState::Idle);
        let pane = state.workspaces[0].panes.get(&pane_id).unwrap();
        assert!(pane.seen);
    }

    #[test]
    fn initial_idle_in_background_stays_seen() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        let pane = state.workspaces[1].panes.get(&bg_pane_id).unwrap();
        assert!(pane.seen);
    }

    #[test]
    fn waiting_sound_plays_even_in_active_workspace() {
        assert_eq!(
            notification_sound_for_state_change(true, AgentState::Working, AgentState::Blocked),
            Some(crate::sound::Sound::Request)
        );
    }

    #[test]
    fn done_sound_only_plays_in_background() {
        assert_eq!(
            notification_sound_for_state_change(false, AgentState::Working, AgentState::Idle),
            Some(crate::sound::Sound::Done)
        );
        assert_eq!(
            notification_sound_for_state_change(true, AgentState::Working, AgentState::Idle),
            None
        );
        assert_eq!(
            notification_sound_for_state_change(false, AgentState::Unknown, AgentState::Idle),
            None
        );
    }

    #[test]
    fn background_waiting_sets_attention_toast() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Blocked,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "pi needs attention");
        assert_eq!(toast.context, "background · 2");
    }

    #[test]
    fn delayed_background_toast_waits_and_revalidates_state() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        state.toast_config.delay_seconds = 1;
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Blocked,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        assert!(state.toast.is_none());
        let deadline = state
            .next_pending_agent_notification_deadline()
            .expect("pending delayed notification");
        let deliveries = state.drain_due_agent_notifications(deadline);

        assert_eq!(deliveries.len(), 1);
        let toast = state.toast.as_ref().expect("delayed toast");
        assert_eq!(toast.title, "pi needs attention");
        assert_eq!(toast.context, "background · 2");
        assert!(state.pending_agent_notifications.is_empty());
    }

    #[test]
    fn hook_reported_unknown_agent_sets_toast_title_from_label() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::HookStateReported {
            pane_id: bg_pane_id,
            source: "custom:hermes".into(),
            agent_label: "hermes".into(),
            state: AgentState::Blocked,
            message: None,
            custom_status: None,
            seq: None,
            session_ref: None,
            launch_env: Vec::new(),
        });

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "hermes needs attention");
        assert_eq!(toast.context, "background · 2");
    }

    #[test]
    fn visible_blocker_overrides_hook_working_and_notifies() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();
        let bg_terminal_id = state.workspaces[1]
            .panes
            .get(&bg_pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        state.handle_app_event(AppEvent::HookStateReported {
            pane_id: bg_pane_id,
            source: "hako:codex".into(),
            agent_label: "codex".into(),
            state: AgentState::Working,
            message: None,
            custom_status: None,
            seq: Some(1),
            session_ref: None,
            launch_env: Vec::new(),
        });
        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Blocked,
            visible_blocker: true,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        let terminal = state.terminals.get(&bg_terminal_id).unwrap();
        assert_eq!(terminal.state, AgentState::Blocked);
        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "codex needs attention");
    }

    #[test]
    fn visible_idle_waits_before_overriding_claude_hook_working() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();
        let bg_terminal_id = state.workspaces[1]
            .panes
            .get(&bg_pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Claude),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        state.handle_app_event(AppEvent::HookStateReported {
            pane_id: bg_pane_id,
            source: "hako:claude".into(),
            agent_label: "claude".into(),
            state: AgentState::Working,
            message: None,
            custom_status: None,
            seq: Some(1),
            session_ref: None,
            launch_env: Vec::new(),
        });
        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Claude),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: true,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        let terminal = state.terminals.get(&bg_terminal_id).unwrap();
        assert_eq!(terminal.state, AgentState::Working);
        assert!(state.toast.is_none());
    }

    #[test]
    fn hidden_session_ref_only_update_marks_session_dirty_without_visible_update() {
        let mut state = app_with_workspaces(&["active"]);
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();

        let first_updates = state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: true,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        assert_eq!(first_updates.len(), 1);
        state.session_dirty = false;

        let second_updates = state.handle_app_event(AppEvent::HookSessionReported {
            pane_id,
            source: "hako:pi".into(),
            agent_label: "pi".into(),
            seq: Some(21),
            session_ref: crate::agent_resume::AgentSessionRef::path("/tmp/two.jsonl"),
            launch_env: vec![("PI_CONFIG_DIR".into(), ".pi-profile".into())],
        });

        assert!(second_updates.is_empty());
        assert!(state.session_dirty);
        let terminal = state
            .terminals
            .get(&state.workspaces[0].terminal_id(pane_id).cloned().unwrap())
            .unwrap();
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(
            terminal.launch_env,
            vec![("PI_CONFIG_DIR".into(), ".pi-profile".into())]
        );

        state.session_dirty = false;
        let third_updates = state.handle_app_event(AppEvent::HookSessionReported {
            pane_id,
            source: "hako:pi".into(),
            agent_label: "pi".into(),
            seq: Some(22),
            session_ref: crate::agent_resume::AgentSessionRef::path("/tmp/three.jsonl"),
            launch_env: Vec::new(),
        });

        assert!(third_updates.is_empty());
        assert!(state.session_dirty);
        let terminal = state
            .terminals
            .get(&state.workspaces[0].terminal_id(pane_id).cloned().unwrap())
            .unwrap();
        assert!(terminal.launch_env.is_empty());
    }

    #[test]
    fn background_idle_sets_finished_toast() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();
        let bg_terminal_id = state.workspaces[1]
            .panes
            .get(&bg_pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        state.terminals.get_mut(&bg_terminal_id).unwrap().state = AgentState::Working;

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Droid),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::Finished);
        assert_eq!(toast.title, "droid finished");
        assert_eq!(toast.context, "background · 2");
        let target = toast.target.as_ref().expect("toast target");
        assert_eq!(&target.workspace_id, &state.workspaces[1].id);
        assert_eq!(target.pane_id, bg_pane_id);
    }

    #[test]
    fn background_toast_includes_tab_name_when_workspace_has_multiple_tabs() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        state.workspaces[1].tabs[0].set_custom_name("main".into());
        let second_tab = state.workspaces[1].test_add_tab(Some("logs"));
        state.ensure_test_terminals();
        let bg_pane_id = state.workspaces[1].tabs[second_tab].root_pane;

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Blocked,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "pi needs attention");
        assert_eq!(toast.context, "background · 2 · logs");
    }

    #[test]
    fn background_tab_in_active_workspace_still_sets_toast() {
        let mut state = app_with_workspaces(&["active"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        state.workspaces[0].tabs[0].set_custom_name("main".into());
        let second_tab = state.workspaces[0].test_add_tab(Some("logs"));
        state.ensure_test_terminals();
        let bg_pane_id = state.workspaces[0].tabs[second_tab].root_pane;

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Blocked,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "pi needs attention");
        assert_eq!(toast.context, "active · 1 · logs");
    }

    #[test]
    fn active_workspace_active_tab_does_not_set_toast() {
        let mut state = app_with_workspaces(&["active"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Blocked,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        assert!(state.toast.is_none());
    }

    #[test]
    fn active_workspace_active_tab_keeps_hako_toast_suppressed_when_outer_terminal_is_unfocused() {
        let mut state = app_with_workspaces(&["active"]);
        state.active = Some(0);
        state.outer_terminal_focus = Some(false);
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Blocked,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        assert!(state.toast.is_none());
    }

    #[test]
    fn active_tab_suppression_preserves_unknown_focus_behavior() {
        assert!(active_tab_suppresses_notifications(true, None));
        assert!(active_tab_suppresses_notifications(true, Some(true)));
        assert!(!active_tab_suppresses_notifications(true, Some(false)));
        assert!(!active_tab_suppresses_notifications(false, None));
    }

    #[test]
    fn update_ready_sets_manual_update_toast() {
        let mut state = AppState::test_new();
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;

        let updates = state.handle_app_event(AppEvent::UpdateReady {
            version: "0.5.0".into(),
            install_command: "hako update".into(),
        });

        assert!(updates.is_empty());
        assert_eq!(state.update_available.as_deref(), Some("0.5.0"));
        assert!(state.latest_release_notes_available);
        assert!(state.update_dismissed);
        let toast = state.toast.as_ref().expect("update toast");
        assert_eq!(toast.kind, ToastKind::UpdateInstalled);
        assert_eq!(toast.title, "v0.5.0 available");
        assert_eq!(toast.context, "detach, then run `hako update`");
    }

    #[test]
    fn update_ready_uses_event_install_command_in_toast() {
        let mut state = AppState::test_new();
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;

        state.handle_app_event(AppEvent::UpdateReady {
            version: "0.5.0".into(),
            install_command: "brew update && brew upgrade hako".into(),
        });

        assert_eq!(
            state.update_install_command,
            "brew update && brew upgrade hako"
        );
        let toast = state.toast.as_ref().expect("update toast");
        assert_eq!(
            toast.context,
            "detach, then run `brew update && brew upgrade hako`"
        );
    }

    #[test]
    fn agent_detection_manifest_update_event_updates_status_and_toast() {
        let mut state = AppState::test_new();
        state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        let status = crate::detect::manifest_update::ManifestUpdateStatus {
            last_result: Some("checked".to_string()),
            ..Default::default()
        };

        let updates = state.handle_app_event(AppEvent::AgentDetectionManifestsUpdated {
            updated: vec![crate::detect::manifest_update::ManifestUpdateCommit {
                agent: Agent::Codex,
                version: crate::detect::manifest_update::ManifestVersion::parse("2026.06.10.1")
                    .unwrap(),
            }],
            status,
        });

        assert!(updates.is_empty());
        assert_eq!(
            state.agent_manifest_update_status.last_result.as_deref(),
            Some("checked")
        );
        let toast = state.toast.as_ref().expect("manifest update toast");
        assert_eq!(toast.kind, ToastKind::UpdateInstalled);
        assert_eq!(toast.title, "Agent detection rules updated");
        assert_eq!(toast.context, "codex 2026.06.10.1");
    }

    #[test]
    fn toggle_zoom_works() {
        let mut state = app_with_workspaces(&["test"]);
        state.workspaces[0].test_split(Direction::Horizontal);

        assert!(!state.workspaces[0].zoomed);
        state.toggle_zoom();
        assert!(state.workspaces[0].zoomed);
        state.toggle_zoom();
        assert!(!state.workspaces[0].zoomed);
    }

    #[test]
    fn toggle_zoom_single_pane_noop() {
        let mut state = app_with_workspaces(&["test"]);
        state.toggle_zoom();
        assert!(!state.workspaces[0].zoomed);
    }

    #[test]
    fn navigate_pane_changes_focus_while_zoomed() {
        let mut state = app_with_workspaces(&["test"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        let right = state.workspaces[0].test_split(Direction::Horizontal);
        state.workspaces[0].layout.focus_pane(root);
        state.workspaces[0].zoomed = true;
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 100, 20));

        assert_eq!(state.view.pane_infos.len(), 1);
        assert_eq!(state.view.pane_infos[0].id, root);

        state.navigate_pane(NavDirection::Right);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 100, 20));

        assert!(state.workspaces[0].zoomed);
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(right));
        assert_eq!(state.view.pane_infos.len(), 1);
        assert_eq!(state.view.pane_infos[0].id, right);
        assert!(state.view.pane_infos[0].inner_rect.x > state.view.pane_infos[0].rect.x);
    }

    #[test]
    fn close_pane_removes_from_workspace() {
        let mut state = app_with_workspaces(&["test"]);
        let closed = state.workspaces[0].test_split(Direction::Horizontal);
        assert_eq!(state.workspaces[0].panes.len(), 2);
        state.plugin_panes.insert(
            closed,
            crate::app::state::PluginPaneRecord {
                plugin_id: "example.pane".into(),
                entrypoint: "board".into(),
            },
        );

        state.close_pane();
        assert_eq!(state.workspaces[0].panes.len(), 1);
        assert!(!state.plugin_panes.contains_key(&closed));
    }

    #[test]
    fn close_pane_removes_unattached_terminal_state() {
        let mut state = app_with_workspaces(&["test"]);
        let pane_id = state.workspaces[0].test_split(Direction::Horizontal);
        state.ensure_test_terminals();
        let terminal_id = state.terminal_id_for_pane(0, pane_id).unwrap();

        state.close_pane();

        assert!(!state.terminals.contains_key(&terminal_id));
    }

    #[test]
    fn close_tab_removes_unattached_terminal_states() {
        let mut state = app_with_workspaces(&["test"]);
        let tab_idx = state.workspaces[0].test_add_tab(Some("logs"));
        state.ensure_test_terminals();
        state.workspaces[0].switch_tab(tab_idx);
        let pane_id = state.workspaces[0].tabs[tab_idx].root_pane;
        let terminal_id = state.terminal_id_for_pane(0, pane_id).unwrap();
        state.session_dirty = false;
        state.plugin_panes.insert(
            pane_id,
            crate::app::state::PluginPaneRecord {
                plugin_id: "example.pane".into(),
                entrypoint: "board".into(),
            },
        );

        state.close_tab();

        assert!(!state.terminals.contains_key(&terminal_id));
        assert!(!state.plugin_panes.contains_key(&pane_id));
        assert!(state.session_dirty);
    }

    #[test]
    fn close_tab_records_workspace_default_cwd_before_removing_last_tab() {
        let root = temp_project("close-tab-default-cwd");
        let mut state = app_with_workspaces(&["test"]);
        let pane_id = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.terminal_id_for_pane(0, pane_id).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = root.clone();
        state.workspaces[0].default_cwd = std::path::PathBuf::from("/stale/default");

        state.close_tab();

        assert!(state.workspaces[0].tabs.is_empty());
        assert_eq!(state.workspaces[0].default_cwd, root);
    }

    #[test]
    fn close_workspace_removes_unattached_terminal_states() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let pane_id = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.terminal_id_for_pane(0, pane_id).unwrap();
        state.plugin_panes.insert(
            pane_id,
            crate::app::state::PluginPaneRecord {
                plugin_id: "example.pane".into(),
                entrypoint: "board".into(),
            },
        );

        state.close_selected_workspace();

        assert!(!state.terminals.contains_key(&terminal_id));
        assert!(!state.plugin_panes.contains_key(&pane_id));
    }

    #[test]
    fn delete_group_removes_unattached_terminal_states() {
        let mut state = app_with_workspaces(&["keep", "drop"]);
        let group_idx = state.create_group("work".into());
        state.move_workspace_to_group(1, group_idx);
        let dropped_terminal_id = state
            .terminal_id_for_pane(1, state.workspaces[1].tabs[0].root_pane)
            .unwrap();
        let kept_terminal_id = state
            .terminal_id_for_pane(0, state.workspaces[0].tabs[0].root_pane)
            .unwrap();

        state.delete_group(group_idx).unwrap();

        assert!(state.terminals.contains_key(&kept_terminal_id));
        assert!(!state.terminals.contains_key(&dropped_terminal_id));
    }

    #[test]
    fn delete_active_group_reapplies_surviving_group_accent() {
        let mut state = app_with_workspaces(&["keep", "drop"]);
        let group_idx = state.create_group("work".into());
        state.move_workspace_to_group(1, group_idx);
        state.active_group = 0;
        state.set_group_accent(0, Some(crate::config::TerminalAccent::Blue));
        let kept_accent = state.active_workspace_accent_color();
        state.active_group = group_idx;
        state.set_group_accent(group_idx, Some(crate::config::TerminalAccent::Red));
        assert_ne!(state.active_workspace_accent_color(), kept_accent);

        state.delete_group(group_idx).unwrap();

        assert_eq!(state.active_group, 0);
        assert_eq!(state.active_workspace_accent_color(), kept_accent);
    }

    #[test]
    fn close_tab_last_tab_empties_active_workspace_not_selected_workspace() {
        let mut state = app_with_workspaces(&["selected", "active"]);
        let active_terminal_id = state
            .terminal_id_for_pane(1, state.workspaces[1].tabs[0].root_pane)
            .unwrap();
        state.active = Some(1);
        state.selected = 0;
        state.confirm_close = false;

        state.close_tab();

        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(state.workspaces[0].display_name(), "selected");
        assert_eq!(state.workspaces[1].display_name(), "active");
        assert!(state.workspaces[1].tabs.is_empty());
        assert!(!state.terminals.contains_key(&active_terminal_id));
    }

    #[test]
    fn close_pane_last_pane_closes_active_workspace_not_selected_workspace() {
        let mut state = app_with_workspaces(&["selected", "active"]);
        let active_terminal_id = state
            .terminal_id_for_pane(1, state.workspaces[1].tabs[0].root_pane)
            .unwrap();
        state.active = Some(1);
        state.selected = 0;
        state.confirm_close = false;

        state.close_pane();

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].display_name(), "selected");
        assert!(!state.terminals.contains_key(&active_terminal_id));
    }

    #[test]
    fn close_pane_last_pane_in_parent_worktree_group_prompts() {
        let mut state = app_with_workspaces(&["parent", "child"]);
        mark_parent_worktree(&mut state, 0);
        mark_linked_worktree(&mut state, 1);
        state.active = Some(0);
        state.selected = 1;

        let deferred = state.close_pane();

        assert!(deferred);
        assert_eq!(state.mode, Mode::ConfirmClose);
        assert_eq!(state.selected, 0);
        assert_eq!(state.workspaces.len(), 2);
    }

    #[test]
    fn close_tab_in_linked_worktree_empties_workspace_only() {
        let mut state = app_with_workspaces(&["selected", "active"]);
        mark_linked_worktree(&mut state, 1);
        state.active = Some(1);
        state.selected = 0;
        state.confirm_close = false;

        state.close_tab();

        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(state.workspaces[0].display_name(), "selected");
        assert_eq!(state.workspaces[1].display_name(), "active");
        assert!(state.workspaces[1].tabs.is_empty());
    }

    #[test]
    fn close_tab_last_tab_in_parent_worktree_group_empties_workspace_without_prompt() {
        let mut state = app_with_workspaces(&["parent", "child"]);
        mark_parent_worktree(&mut state, 0);
        mark_linked_worktree(&mut state, 1);
        state.active = Some(0);
        state.selected = 1;

        let deferred = state.close_tab();

        assert!(!deferred);
        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.selected, 1);
        assert_eq!(state.workspaces.len(), 2);
        assert!(state.workspaces[0].tabs.is_empty());
    }

    #[test]
    fn close_pane_last_pane_in_linked_worktree_closes_workspace_only() {
        let mut state = app_with_workspaces(&["selected", "active"]);
        mark_linked_worktree(&mut state, 1);
        state.active = Some(1);
        state.selected = 0;

        state.close_pane();

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].display_name(), "selected");
    }

    #[test]
    fn close_pane_last_pane_in_parent_worktree_group_closes_when_confirmation_disabled() {
        let mut state = app_with_workspaces(&["parent", "child", "notes"]);
        mark_parent_worktree(&mut state, 0);
        mark_linked_worktree(&mut state, 1);
        state.confirm_close = false;
        state.active = Some(0);
        state.selected = 0;

        let deferred = state.close_pane();

        assert!(!deferred);
        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].display_name(), "notes");
    }
}
