//! Pure state mutations on AppState.
//! These don't need channels, async, or PTY runtime.

use tracing::warn;

use crate::detect::{Agent, AgentState};
use crate::events::AppEvent;
use crate::layout::PaneId;
use crate::selection::Selection;
use crate::terminal::{EffectiveStateChange, TerminalStateMutation};
#[cfg(test)]
use crate::workspace::GitWorkSummary;
use crate::workspace::WorkspaceGitStatus;
use unicode_width::UnicodeWidthChar;

use super::state::{
    AgentNotificationDelivery, AppState, Group, Mode, NavigatorRow, NavigatorStateFilter,
    NavigatorTarget, PendingAgentNotification, ProjectCommandKind, ToastKind, ToastNotification,
    ToastTarget,
};
use super::ClientViewState;

#[derive(Clone, Copy)]
struct NavigatorFocus {
    active_workspace: Option<usize>,
    active_group: usize,
    active_tab: Option<usize>,
    focused_pane: Option<PaneId>,
    explicit_hierarchy: bool,
}

fn configured_project_command_at(
    location: crate::execution_host::ResourceLocation,
    kind: ProjectCommandKind,
    command: &str,
) -> crate::commands::ProjectCommand {
    let role = match kind {
        ProjectCommandKind::Browser => "Browser",
        ProjectCommandKind::Review => "Review",
        ProjectCommandKind::Editor => "Editor",
        ProjectCommandKind::Github => "GitHub",
    };
    let name = location
        .path
        .as_path()
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(|name| format!("{role} · {name}"))
        .unwrap_or_else(|| role.to_string());
    crate::commands::ProjectCommand::new(
        location,
        crate::commands::CommandSource::BuiltIn,
        name,
        command,
        crate::commands::CommandConfidence::Explicit,
    )
}

fn canonical_git_root(cwd: &std::path::Path) -> Option<std::path::PathBuf> {
    crate::workspace::git_repo_root(cwd).map(|root| std::fs::canonicalize(&root).unwrap_or(root))
}

pub(crate) fn observed_git_repos_from_cwd(cwd: &std::path::Path) -> Vec<std::path::PathBuf> {
    if let Some(root) = canonical_git_root(cwd) {
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
        .filter_map(|path| canonical_git_root(&path))
        .collect()
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

fn missing_integration_agent_title(agent: Agent) -> String {
    match agent {
        Agent::OhMyPi => "OMP".to_string(),
        Agent::OpenCode => "OpenCode".to_string(),
        Agent::GithubCopilot => "Copilot".to_string(),
        Agent::Qodercli => "Qoder CLI".to_string(),
        Agent::Qwen => "Qwen Code".to_string(),
        Agent::Mastracode => "MastraCode".to_string(),
        Agent::Antigravity => "Antigravity CLI".to_string(),
        _ => {
            let label = crate::detect::agent_label(agent);
            let mut chars = label.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

fn toast_event_text(kind: ToastKind) -> &'static str {
    match kind {
        ToastKind::NeedsAttention => "Needs Attention",
        ToastKind::Finished => "Finished",
        ToastKind::UpdateInstalled => "Updated",
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
            if let Some(label) = ws.tab_display_name(tab_idx) {
                context.push_str(&format!(" · {label}"));
            }
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
    pub suppress_completion: bool,
}

// ---------------------------------------------------------------------------
// Navigator operations
// ---------------------------------------------------------------------------

impl AppState {
    fn pane_is_idle_agent(&self, pane: &crate::pane::PaneState) -> bool {
        self.pane_agent_state(pane) == AgentState::Idle
    }

    pub(crate) fn unseen_idle_hold_for_pane(
        &self,
        ws_idx: usize,
        pane_id: PaneId,
    ) -> Option<(String, PaneId)> {
        let workspace = self.workspaces.get(ws_idx)?;
        let tab_idx = workspace.find_tab_index_for_pane(pane_id)?;
        let pane = workspace.terminal_tab(tab_idx).ok()?.panes.get(&pane_id)?;
        if pane.seen || !self.pane_is_idle_agent(pane) {
            return None;
        }
        Some((workspace.id.clone(), pane_id))
    }

    pub(crate) fn navigator_rows_for_view(
        &self,
        view: &ClientViewState,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> Vec<NavigatorRow> {
        self.navigator_rows_with(
            &view.navigator,
            self.navigator_focus_for_view(view, false),
            terminal_runtimes,
        )
    }

    pub(crate) fn mobile_navigation_rows_for_view(
        &self,
        view: &ClientViewState,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> Vec<NavigatorRow> {
        self.mobile_navigation_rows_with(
            self.navigator_focus_for_view(view, true),
            view.mobile_switcher_level,
            terminal_runtimes,
        )
    }

    fn mobile_navigation_rows_with(
        &self,
        focus: NavigatorFocus,
        level: super::state::MobileSwitcherLevel,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> Vec<NavigatorRow> {
        if let super::state::MobileSwitcherLevel::Panes { ws_idx, tab_idx } = level {
            let multi_tab = self
                .workspaces
                .get(ws_idx)
                .is_some_and(|workspace| workspace.tabs.len() > 1);
            return self.navigator_pane_rows_for_tab(ws_idx, tab_idx, multi_tab, focus);
        }

        let mut navigator = super::state::NavigatorState::default();
        let expanded_group = match level {
            super::state::MobileSwitcherLevel::Workspaces { group_idx } => group_idx,
            super::state::MobileSwitcherLevel::Tabs { ws_idx } => self
                .workspaces
                .get(ws_idx)
                .and_then(|workspace| self.group_index_by_id(&workspace.group_id))
                .unwrap_or(focus.active_group),
            _ => focus.active_group,
        };
        if let Some(group) = self.groups.get(expanded_group) {
            navigator.expanded_groups.insert(group.id.clone());
        }
        let expanded_workspace = match level {
            super::state::MobileSwitcherLevel::Tabs { ws_idx } => Some(ws_idx),
            _ => focus.active_workspace,
        };
        if let Some(workspace) = expanded_workspace.and_then(|ws_idx| self.workspaces.get(ws_idx)) {
            navigator.expanded_workspaces.insert(workspace.id.clone());
        }
        self.navigator_rows_with(&navigator, focus, terminal_runtimes)
    }

    fn navigator_focus_for_view(
        &self,
        view: &ClientViewState,
        explicit_hierarchy: bool,
    ) -> NavigatorFocus {
        let active_tab = view
            .active_workspace
            .and_then(|ws_idx| view.active_tab_index_for_workspace(self, ws_idx));
        let focused_pane = view.active_workspace.and_then(|ws_idx| {
            let workspace = self.workspaces.get(ws_idx)?;
            let tab_idx = active_tab?;
            let tab = workspace.tabs.get(tab_idx)?;
            view.focused_pane_for_tab(&workspace.id, tab.number())
        });
        NavigatorFocus {
            active_workspace: view.active_workspace,
            active_group: view.active_group,
            active_tab,
            focused_pane,
            explicit_hierarchy,
        }
    }

    fn navigator_rows_with(
        &self,
        navigator: &super::state::NavigatorState,
        focus: NavigatorFocus,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> Vec<NavigatorRow> {
        let query = navigator.query.trim().to_lowercase();
        let query_kind = navigator_query_kind(&query, navigator.state_filter);
        let mut rows = Vec::new();

        for (group_idx, group) in self.groups.iter().enumerate() {
            let workspace_indices = self
                .workspaces
                .iter()
                .enumerate()
                .filter_map(|(ws_idx, workspace)| {
                    (workspace.group_id == group.id).then_some(ws_idx)
                })
                .collect::<Vec<_>>();
            let pane_count = workspace_indices
                .iter()
                .filter_map(|&ws_idx| self.workspaces.get(ws_idx))
                .flat_map(|workspace| workspace.terminal_tabs())
                .map(|(_, tab)| tab.panes.len())
                .sum::<usize>();
            let activity = activity_summary_for_panes(
                workspace_indices
                    .iter()
                    .filter_map(|&ws_idx| self.workspaces.get(ws_idx))
                    .flat_map(|workspace| workspace.terminal_tabs())
                    .flat_map(|(_, tab)| tab.panes.values()),
                &self.terminals,
            );
            let topology = format!(
                "{} · {}",
                count_label(workspace_indices.len(), "Space", "Spaces"),
                count_label(pane_count, "Pane", "Panes")
            );
            let meta = if activity.is_empty() {
                topology
            } else {
                format!("{topology} · {activity}")
            };
            let search_text = format!("{} {} {meta}", group.icon, group.name).to_lowercase();
            let (status, seen) = aggregate_state_for_panes(
                workspace_indices
                    .iter()
                    .filter_map(|&ws_idx| self.workspaces.get(ws_idx))
                    .flat_map(|workspace| workspace.terminal_tabs())
                    .flat_map(|(_, tab)| tab.panes.values()),
                &self.terminals,
            );
            let group_matches =
                navigator_row_matches(query_kind, &query, status, seen, &search_text);
            let child_query_kind =
                if group_matches && matches!(query_kind, NavigatorQueryKind::Text) {
                    NavigatorQueryKind::Empty
                } else {
                    query_kind
                };
            let child_query = if matches!(child_query_kind, NavigatorQueryKind::Empty) {
                ""
            } else {
                &query
            };
            let child_rows = self.navigator_workspace_rows(
                navigator,
                focus,
                terminal_runtimes,
                child_query_kind,
                child_query,
                query_kind,
                &query,
                &workspace_indices,
            );
            if !group_matches && child_rows.is_empty() {
                continue;
            }

            let has_children = !child_rows.is_empty();
            let expanded = has_children
                && (!matches!(query_kind, NavigatorQueryKind::Empty)
                    || navigator.expanded_groups.contains(&group.id));
            rows.push(NavigatorRow {
                target: NavigatorTarget::Group { group_idx },
                depth: 0,
                label: format!("{} {}", group.icon, group.name),
                meta,
                status,
                seen,
                is_current: group_idx == focus.active_group,
                is_group: true,
                is_workspace: false,
                is_tab: false,
                has_children,
                expanded,
                search_text,
                matched: group_matches,
            });
            if expanded {
                rows.extend(child_rows);
            }
        }
        rows
    }

    fn navigator_workspace_rows(
        &self,
        navigator: &super::state::NavigatorState,
        focus: NavigatorFocus,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        query_kind: NavigatorQueryKind,
        query: &str,
        direct_query_kind: NavigatorQueryKind,
        direct_query: &str,
        workspace_indices: &[usize],
    ) -> Vec<NavigatorRow> {
        let mut rows = Vec::new();
        for &ws_idx in workspace_indices {
            let Some(ws) = self.workspaces.get(ws_idx) else {
                continue;
            };
            let workspace_label = ws.display_name_from(&self.terminals, terminal_runtimes);
            let activity = workspace_activity_summary(ws, &self.terminals);
            let mut workspace_search_text = format!("{workspace_label} {activity}").to_lowercase();
            if matches!(direct_query_kind, NavigatorQueryKind::Text)
                && ws.tabs.len() == 1
                && ws.terminal_tab(0).is_ok_and(|tab| tab.panes.len() == 1)
            {
                if let Some(pane_row) = self
                    .navigator_pane_rows_for_tab(ws_idx, 0, false, focus)
                    .into_iter()
                    .next()
                {
                    workspace_search_text.push(' ');
                    workspace_search_text.push_str(&pane_row.search_text);
                }
            }
            let (state, seen) = ws.aggregate_state(&self.terminals);
            let workspace_matches =
                navigator_row_matches(query_kind, query, state, seen, &workspace_search_text);
            let workspace_direct_matches = navigator_row_matches(
                direct_query_kind,
                direct_query,
                state,
                seen,
                &workspace_search_text,
            );

            let mut child_rows = self.navigator_child_rows(
                ws_idx,
                query_kind,
                query,
                focus,
                direct_query_kind,
                direct_query,
                workspace_direct_matches,
            );
            if !workspace_matches && child_rows.is_empty() {
                continue;
            }

            let has_children = !child_rows.is_empty();
            let expanded = has_children
                && (!matches!(query_kind, NavigatorQueryKind::Empty)
                    || navigator.expanded_workspaces.contains(&ws.id));
            let pane_count = ws
                .terminal_tabs()
                .map(|(_, tab)| tab.panes.len())
                .sum::<usize>();
            rows.push(NavigatorRow {
                target: NavigatorTarget::Workspace { ws_idx },
                depth: 1,
                label: format!("{workspace_label} ({pane_count})"),
                meta: activity,
                status: state,
                seen,
                is_current: focus.active_workspace == Some(ws_idx),
                is_group: false,
                is_workspace: true,
                is_tab: false,
                has_children,
                expanded,
                search_text: workspace_search_text,
                matched: workspace_direct_matches,
            });
            if expanded {
                for row in &mut child_rows {
                    row.depth = row.depth.saturating_add(1);
                }
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
        focus: NavigatorFocus,
        direct_query_kind: NavigatorQueryKind,
        direct_query: &str,
        workspace_direct_match: bool,
    ) -> Vec<NavigatorRow> {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return Vec::new();
        };
        let explicit = focus.explicit_hierarchy;
        let multi_tab = explicit || ws.tabs.len() > 1;
        let mut rows = Vec::new();
        for tab_idx in 0..ws.tabs.len() {
            let mut pane_rows = self.navigator_pane_rows_for_tab(ws_idx, tab_idx, multi_tab, focus);
            let show_pane_rows = if explicit {
                focus.active_workspace == Some(ws_idx)
                    && focus.active_tab == Some(tab_idx)
                    && !pane_rows.is_empty()
            } else {
                pane_rows.len() > 1
            };
            let show_tab_row = explicit
                || multi_tab
                || ws.tabs[tab_idx].custom_name().is_some()
                || ws.tabs[tab_idx].is_github();
            let mut tab_row = show_tab_row
                .then(|| self.navigator_tab_row(ws_idx, tab_idx, show_pane_rows, focus));
            if !show_pane_rows {
                if let (Some(tab_row), Some(pane_row)) = (tab_row.as_mut(), pane_rows.first()) {
                    tab_row.search_text.push(' ');
                    tab_row.search_text.push_str(&pane_row.search_text);
                }
            }
            let tab_matches = tab_row.as_ref().is_some_and(|row| {
                navigator_row_matches(query_kind, query, row.status, row.seen, &row.search_text)
            });
            let tab_direct_match = tab_row.as_ref().is_some_and(|row| {
                navigator_row_matches(
                    direct_query_kind,
                    direct_query,
                    row.status,
                    row.seen,
                    &row.search_text,
                )
            });
            if let Some(tab_row) = tab_row.as_mut() {
                tab_row.matched = tab_direct_match;
            }
            let filtered_panes = if show_pane_rows {
                match query_kind {
                    NavigatorQueryKind::Empty => {
                        for row in &mut pane_rows {
                            row.matched = navigator_row_matches(
                                direct_query_kind,
                                direct_query,
                                row.status,
                                row.seen,
                                &row.search_text,
                            );
                        }
                        pane_rows
                    }
                    NavigatorQueryKind::State(filter) => pane_rows
                        .into_iter()
                        .filter(|row| navigator_state_filter_matches(filter, row.status, row.seen))
                        .map(|mut row| {
                            row.matched = navigator_row_matches(
                                direct_query_kind,
                                direct_query,
                                row.status,
                                row.seen,
                                &row.search_text,
                            );
                            row
                        })
                        .collect::<Vec<_>>(),
                    NavigatorQueryKind::Text if workspace_direct_match || tab_direct_match => {
                        for row in &mut pane_rows {
                            row.matched = navigator_matches(direct_query, &row.search_text);
                        }
                        pane_rows
                    }
                    NavigatorQueryKind::Text => pane_rows
                        .into_iter()
                        .filter(|row| navigator_matches(query, &row.search_text))
                        .map(|mut row| {
                            row.matched = true;
                            row
                        })
                        .collect::<Vec<_>>(),
                }
            } else {
                Vec::new()
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

    fn navigator_tab_row(
        &self,
        ws_idx: usize,
        tab_idx: usize,
        has_visible_pane_children: bool,
        focus: NavigatorFocus,
    ) -> NavigatorRow {
        let ws = &self.workspaces[ws_idx];
        let tab = &ws.tabs[tab_idx];
        let label = ws
            .tab_display_name(tab_idx)
            .unwrap_or_else(|| (tab_idx + 1).to_string());
        let (status, seen) = tab
            .as_terminal()
            .map(|tab| tab_aggregate_state(tab, &self.terminals))
            .unwrap_or((AgentState::Unknown, true));
        let activity = tab
            .as_terminal()
            .map(|tab| tab_activity_summary(tab, &self.terminals))
            .unwrap_or_default();
        let pane_count = tab.as_terminal().map_or(0, |tab| tab.panes.len());
        let pane_count = count_label(pane_count, "pane", "panes");
        let meta = if activity.is_empty() {
            pane_count
        } else {
            format!("{pane_count} · {activity}")
        };
        let search_text = format!("{label} {meta}").to_lowercase();
        NavigatorRow {
            target: NavigatorTarget::Tab { ws_idx, tab_idx },
            depth: 1,
            label,
            meta,
            status,
            seen,
            is_current: focus.active_workspace == Some(ws_idx) && focus.active_tab == Some(tab_idx),
            is_group: false,
            is_workspace: false,
            is_tab: true,
            has_children: has_visible_pane_children,
            expanded: has_visible_pane_children,
            search_text,
            matched: true,
        }
    }

    fn navigator_pane_rows_for_tab(
        &self,
        ws_idx: usize,
        tab_idx: usize,
        multi_tab: bool,
        focus: NavigatorFocus,
    ) -> Vec<NavigatorRow> {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return Vec::new();
        };
        let Ok(tab) = ws.terminal_tab(tab_idx) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for (pane_idx, pane_id) in tab.layout.pane_ids().into_iter().enumerate() {
            let Some(pane) = tab.panes.get(&pane_id) else {
                continue;
            };
            let terminal = self.terminals.get(&pane.attached_terminal_id);
            let pane_number = pane_idx + 1;
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
                .or_else(|| agent_label.map(|_| state_label_display_text(state, pane.seen).into()));
            let meta = match (agent_label, status.as_deref()) {
                (Some(agent_label), Some(status)) if label.eq_ignore_ascii_case(agent_label) => {
                    status.to_string()
                }
                (Some(agent_label), Some(status)) => format!("{agent_label} · {status}"),
                (Some(agent_label), None) if label.eq_ignore_ascii_case(agent_label) => {
                    String::new()
                }
                (Some(agent_label), None) => agent_label.to_string(),
                (None, _) => "Shell".to_string(),
            };
            let is_current = focus.active_workspace == Some(ws_idx)
                && focus.active_tab == Some(tab_idx)
                && focus.focused_pane == Some(pane_id);
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
                is_group: false,
                is_workspace: false,
                is_tab: false,
                has_children: false,
                expanded: false,
                search_text,
                matched: true,
            });
        }
        rows
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
fn navigator_row_matches(
    query_kind: NavigatorQueryKind,
    query: &str,
    state: AgentState,
    seen: bool,
    search_text: &str,
) -> bool {
    match query_kind {
        NavigatorQueryKind::Empty => true,
        NavigatorQueryKind::State(filter) => navigator_state_filter_matches(filter, state, seen),
        NavigatorQueryKind::Text => navigator_matches(query, search_text),
    }
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
fn count_label(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
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

fn state_label_display_text(state: AgentState, seen: bool) -> &'static str {
    match (state, seen) {
        (AgentState::Blocked, _) => "Blocked",
        (AgentState::Working, _) => "Working",
        (AgentState::Idle, false) => "Done",
        (AgentState::Idle, true) => "Idle",
        (AgentState::Unknown, _) => "Unknown",
    }
}

fn tab_aggregate_state(
    tab: &crate::workspace::Tab,
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
) -> (AgentState, bool) {
    aggregate_state_for_panes(tab.panes.values(), terminals)
}

fn aggregate_state_for_panes<'a>(
    panes: impl Iterator<Item = &'a crate::pane::PaneState>,
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
) -> (AgentState, bool) {
    let mut aggregate = AgentState::Unknown;
    let mut seen = true;
    for pane in panes {
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
    activity_summary_for_panes(
        ws.terminal_tabs().flat_map(|(_, tab)| tab.panes.values()),
        terminals,
    )
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
        parts.push(format!("{blocked} Blocked"));
    }
    if working > 0 {
        parts.push(format!("{working} Working"));
    }
    if done > 0 {
        parts.push(format!("{done} Done"));
    }
    parts.join(" · ")
}

// ---------------------------------------------------------------------------
// Workspace operations
// ---------------------------------------------------------------------------

impl AppState {
    fn command_target_for_location(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        location: &crate::execution_host::ResourceLocation,
    ) -> Option<(usize, usize, PaneId)> {
        for (ws_idx, workspace) in self.workspaces.iter().enumerate() {
            for (tab_idx, tab) in workspace.terminal_tabs() {
                for pane_id in tab.layout.pane_ids() {
                    let Some(terminal_id) = workspace.terminal_id(pane_id) else {
                        continue;
                    };
                    let Some(terminal) = self.terminals.get(terminal_id) else {
                        continue;
                    };
                    if terminal.location.execution_host_id != location.execution_host_id {
                        continue;
                    }
                    let Some(cwd) = tab.cwd_for_pane(pane_id, &self.terminals, terminal_runtimes)
                    else {
                        continue;
                    };
                    let matches_root = if location.is_local() {
                        crate::commands::project_root_from_cwd(&cwd) == location.path.as_path()
                    } else {
                        cwd == location.path.as_path()
                    };
                    if matches_root {
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
            for (tab_idx, tab) in workspace.terminal_tabs() {
                for (pane_id, pane) in &tab.panes {
                    if &pane.attached_terminal_id == terminal_id {
                        return Some((ws_idx, tab_idx, *pane_id));
                    }
                }
            }
        }
        None
    }

    fn run_local_project_command(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        command_id: &str,
    ) -> Result<(), String> {
        let command = self
            .command_catalog
            .iter()
            .find(|command| command.id == command_id)
            .cloned()
            .ok_or_else(|| format!("Command {command_id} not found"))?;

        let (ws_idx, _, _) = self
            .command_target_for_location(terminal_runtimes, &command.location)
            .ok_or_else(|| format!("No pane for {}", command.root().display()))?;

        self.run_project_command_entry(terminal_runtimes, command, ws_idx, None, None)
    }

    pub(crate) fn open_project_command(
        &mut self,
        view: &mut ClientViewState,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        kind: ProjectCommandKind,
    ) -> Result<(), String> {
        let fallback_ws_idx = if matches!(view.mode, Mode::Navigate) {
            Some(view.selected_workspace)
        } else {
            view.active_workspace
        };
        let ws_idx = fallback_ws_idx.ok_or_else(|| "No project for current space".to_string())?;
        self.open_project_command_for_workspace(view, terminal_runtimes, ws_idx, kind)
    }

    pub(crate) fn pending_project_command_tab_number_for_workspace(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        ws_idx: usize,
        kind: ProjectCommandKind,
    ) -> Option<usize> {
        if !self.project_command_configured(kind) {
            return None;
        }
        if kind == ProjectCommandKind::Github {
            return None;
        }
        let root = if matches!(
            kind,
            ProjectCommandKind::Browser | ProjectCommandKind::Editor | ProjectCommandKind::Github
        ) {
            self.workspaces.get(ws_idx)?.effective_default_cwd_from(
                None,
                &self.terminals,
                terminal_runtimes,
            )
        } else {
            let roots = self.observed_git_repos_for_workspace(terminal_runtimes, ws_idx);
            let [root] = roots.as_slice() else {
                return None;
            };
            root.clone()
        };
        let command = self
            .configured_project_command(root, kind, Some(ws_idx))
            .ok()?;
        if let Some(run) = self.command_runs.get(&command.id) {
            if let Some((run_ws_idx, tab_idx, _)) = self.command_terminal_target(&run.terminal_id) {
                return (run_ws_idx == ws_idx)
                    .then(|| self.workspaces[ws_idx].tabs[tab_idx].number());
            }
        }
        self.workspaces
            .get(ws_idx)
            .map(crate::workspace::Workspace::next_remote_tab_number)
    }

    pub(crate) fn open_project_command_for_workspace(
        &mut self,
        view: &mut ClientViewState,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        ws_idx: usize,
        kind: ProjectCommandKind,
    ) -> Result<(), String> {
        if !self.project_command_configured(kind) {
            return Err(format!(
                "Configure the {} command in Settings > Commands",
                self.project_command_role(kind)
            ));
        }
        if kind == ProjectCommandKind::Github {
            return Err("GitHub opens in the native screen".to_string());
        }
        if matches!(
            kind,
            ProjectCommandKind::Browser | ProjectCommandKind::Editor | ProjectCommandKind::Github
        ) {
            let root = self
                .workspaces
                .get(ws_idx)
                .ok_or_else(|| "Workspace not found".to_string())?
                .effective_default_cwd_from(None, &self.terminals, terminal_runtimes);
            return self.open_project_command_tab(terminal_runtimes, root, ws_idx, kind);
        }

        let roots = self.observed_git_repos_for_workspace(terminal_runtimes, ws_idx);
        let root = match roots.as_slice() {
            [] => return Err("No Git Repo for Current Space".to_string()),
            [root] => root.clone(),
            _ => {
                view.git_repo_picker.ws_idx = ws_idx;
                view.git_repo_picker.command_kind = kind;
                view.git_repo_picker.roots = roots;
                view.git_repo_picker.list.select(0);
                view.git_repo_picker.list.hide();
                view.git_repo_picker.scroll = 0;
                view.mode = Mode::GitRepoPicker;
                return Ok(());
            }
        };
        self.open_project_command_tab(terminal_runtimes, root, ws_idx, kind)
    }
    pub(crate) fn resolved_github_scope(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        ws_idx: usize,
    ) -> Result<crate::github::ResolvedGithubScope, String> {
        let workspace = self
            .workspaces
            .get(ws_idx)
            .ok_or_else(|| "Workspace not found".to_string())?;
        let scope = &workspace.github_scope;
        let group_organization = self
            .groups
            .iter()
            .find(|group| group.id == workspace.group_id)
            .and_then(|group| group.github_organization.as_ref());
        let discovery = match scope {
            crate::github::GithubRepositoryScope::Automatic => {
                self.github_discovery_for_workspace(terminal_runtimes, ws_idx)
            }
            crate::github::GithubRepositoryScope::GroupOrganization
            | crate::github::GithubRepositoryScope::Selected(_) => {
                crate::github::GithubDiscoveryOutcome::Empty
            }
        };
        crate::github::resolve_github_scope(scope, &discovery, group_organization)
    }

    fn configured_project_command(
        &self,
        root: std::path::PathBuf,
        kind: ProjectCommandKind,
        ws_idx: Option<usize>,
    ) -> Result<crate::commands::ProjectCommand, String> {
        let location = crate::execution_host::ResourceLocation::local(root)
            .map_err(|error| error.to_string())?;
        Ok(self.configured_project_command_for_location(location, kind, ws_idx))
    }

    fn configured_project_command_for_location(
        &self,
        location: crate::execution_host::ResourceLocation,
        kind: ProjectCommandKind,
        ws_idx: Option<usize>,
    ) -> crate::commands::ProjectCommand {
        let configured = match kind {
            ProjectCommandKind::Browser => &self.browser_command,
            ProjectCommandKind::Review => &self.review_command,
            ProjectCommandKind::Editor => &self.editor_command,
            ProjectCommandKind::Github => "",
        };
        let command = match (kind, configured.trim()) {
            (ProjectCommandKind::Browser, crate::browser_theme::BROWSER_COMMAND) => {
                crate::browser_theme::command()
            }
            (ProjectCommandKind::Review, crate::hunk_theme::DIFF_COMMAND) => {
                crate::hunk_theme::command(
                    &ws_idx
                        .and_then(|idx| {
                            self.workspaces
                                .get(idx)
                                .map(|_| self.palette_for_workspace(idx))
                        })
                        .unwrap_or_else(|| self.palette.clone()),
                    self.effective_theme_appearance,
                    self.host_terminal_theme,
                    crate::external_tool_theme::is_terminal_passthrough(&self.theme_name),
                )
            }
            (ProjectCommandKind::Editor, crate::fresh_theme::IDE_COMMAND) => {
                crate::fresh_theme::command(
                    &ws_idx
                        .and_then(|idx| {
                            self.workspaces
                                .get(idx)
                                .map(|_| self.palette_for_workspace(idx))
                        })
                        .unwrap_or_else(|| self.palette.clone()),
                    self.effective_theme_appearance,
                    self.host_terminal_theme,
                    crate::external_tool_theme::is_terminal_passthrough(&self.theme_name),
                )
            }
            _ => configured.to_owned(),
        };
        configured_project_command_at(location, kind, &command)
    }

    fn curated_project_command_terminal_theme_binding(
        &self,
        kind: ProjectCommandKind,
    ) -> Option<crate::terminal_theme::TerminalThemeBinding> {
        let configured = match kind {
            ProjectCommandKind::Browser => &self.browser_command,
            ProjectCommandKind::Review => &self.review_command,
            ProjectCommandKind::Editor => &self.editor_command,
            ProjectCommandKind::Github => return None,
        };
        match (kind, configured.trim()) {
            (ProjectCommandKind::Browser, crate::browser_theme::BROWSER_COMMAND)
            | (ProjectCommandKind::Review, crate::hunk_theme::DIFF_COMMAND)
            | (ProjectCommandKind::Editor, crate::fresh_theme::IDE_COMMAND) => {
                Some(crate::terminal_theme::TerminalThemeBinding::workspace_palette())
            }
            _ => None,
        }
    }

    fn curated_project_command_terminal_theme(
        &self,
        kind: ProjectCommandKind,
        ws_idx: Option<usize>,
    ) -> Option<crate::terminal_theme::ResolvedTerminalTheme> {
        self.curated_project_command_terminal_theme_binding(kind)?;
        if crate::external_tool_theme::is_terminal_passthrough(&self.theme_name) {
            return None;
        }
        let palette = ws_idx
            .and_then(|idx| {
                self.workspaces
                    .get(idx)
                    .map(|_| self.palette_for_workspace(idx))
            })
            .unwrap_or_else(|| self.palette.clone());
        Some(crate::external_tool_theme::resolved_terminal_theme(
            &palette,
            self.effective_theme_appearance,
            self.host_terminal_theme,
        ))
    }

    fn open_project_command_tab(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        root: std::path::PathBuf,
        ws_idx: usize,
        kind: ProjectCommandKind,
    ) -> Result<(), String> {
        let terminal_theme_binding = self.curated_project_command_terminal_theme_binding(kind);
        let terminal_theme = self.curated_project_command_terminal_theme(kind, Some(ws_idx));
        if kind == ProjectCommandKind::Github {
            return Err("GitHub opens in the native screen".to_string());
        }
        let command = self.configured_project_command(root, kind, Some(ws_idx))?;
        self.run_project_command_entry(
            terminal_runtimes,
            command,
            ws_idx,
            terminal_theme_binding,
            terminal_theme,
        )
    }

    fn run_project_command_entry(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        command: crate::commands::ProjectCommand,
        ws_idx: usize,
        terminal_theme_binding: Option<crate::terminal_theme::TerminalThemeBinding>,
        terminal_theme: Option<crate::terminal_theme::ResolvedTerminalTheme>,
    ) -> Result<(), String> {
        let command_id = command.id.clone();
        if let Some(run) = self.command_runs.get(&command_id).cloned() {
            if run.status == crate::commands::CommandRunStatus::Running
                && self.command_terminal_target(&run.terminal_id).is_some()
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
                    terminal_theme_binding,
                    terminal_theme,
                )?;
                return Ok(());
            }
            if let Some(run) = self.command_runs.get_mut(&command_id) {
                run.status = crate::commands::CommandRunStatus::Unknown;
            }
        }

        self.open_command_tab(
            terminal_runtimes,
            command,
            ws_idx,
            terminal_theme_binding,
            terminal_theme,
        )
    }
    pub(crate) fn github_discovery_for_workspace(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        ws_idx: usize,
    ) -> crate::github::GithubDiscoveryOutcome {
        let Some(workspace) = self.workspaces.get(ws_idx) else {
            return crate::github::GithubDiscoveryOutcome::Failed(
                "Workspace not found.".to_string(),
            );
        };
        if !workspace.default_location.is_local() {
            return crate::github::GithubDiscoveryOutcome::Failed(
                "Automatic GitHub discovery requires a local Space location.".to_string(),
            );
        }

        let mut cwds = vec![workspace.default_location.path.as_path().to_path_buf()];
        for (_, tab) in workspace.terminal_tabs() {
            for pane_id in tab.layout.pane_ids() {
                let Some(terminal_id) = workspace.terminal_id(pane_id) else {
                    continue;
                };
                let Some(terminal) = self.terminals.get(terminal_id) else {
                    continue;
                };
                if !terminal.location.is_local() {
                    return crate::github::GithubDiscoveryOutcome::Failed(
                        "Automatic GitHub discovery cannot inspect a remote pane from a local coordinator."
                            .to_string(),
                    );
                }
                if let Some(cwd) = tab.cwd_for_pane(pane_id, &self.terminals, terminal_runtimes) {
                    cwds.push(cwd);
                }
            }
        }
        cwds.sort();
        cwds.dedup();

        crate::workspace::discover_github_repositories(&cwds)
    }

    pub(crate) fn observed_git_repos_for_workspace(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        ws_idx: usize,
    ) -> Vec<std::path::PathBuf> {
        let mut roots = Vec::new();
        let Some(workspace) = self.workspaces.get(ws_idx) else {
            return roots;
        };
        for (_, tab) in workspace.terminal_tabs() {
            for pane_id in tab.layout.pane_ids() {
                let Some(terminal_id) = workspace.terminal_id(pane_id) else {
                    continue;
                };
                if !self
                    .terminals
                    .get(terminal_id)
                    .is_some_and(|terminal| terminal.location.is_local())
                {
                    continue;
                }
                if let Some(cwd) = tab.cwd_for_pane(pane_id, &self.terminals, terminal_runtimes) {
                    roots.extend(observed_git_repos_from_cwd(&cwd));
                }
            }
        }
        roots.sort();
        roots.dedup();
        roots
    }

    pub(crate) fn open_selected_project_command(
        &mut self,
        view: &ClientViewState,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
    ) -> Result<(), String> {
        let Some(root) = view
            .git_repo_picker
            .roots
            .get(view.git_repo_picker.list.selected)
            .cloned()
        else {
            return Err("No Git repo selected".to_string());
        };
        let ws_idx = view.git_repo_picker.ws_idx;
        let kind = view.git_repo_picker.command_kind;
        self.open_project_command_tab(terminal_runtimes, root, ws_idx, kind)
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
        view: &ClientViewState,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> Option<(std::path::PathBuf, usize)> {
        let ws_idx = if matches!(view.mode, Mode::Navigate) {
            view.selected_workspace
        } else {
            view.active_workspace?
        };
        self.git_diff_target_for_workspace(terminal_runtimes, ws_idx)
            .map(|root| (root, ws_idx))
    }

    fn open_command_tab(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        command: crate::commands::ProjectCommand,
        ws_idx: usize,
        terminal_theme_binding: Option<crate::terminal_theme::TerminalThemeBinding>,
        terminal_theme: Option<crate::terminal_theme::ResolvedTerminalTheme>,
    ) -> Result<(), String> {
        if !command.location.is_local() {
            return Err(format!(
                "Project command {} targets execution host {}; AppState cannot route host runtime work",
                command.id, command.location.execution_host_id
            ));
        }
        let (rows, cols) = self.estimate_pane_size();
        let workspace = self
            .workspaces
            .get_mut(ws_idx)
            .ok_or_else(|| "Command workspace disappeared".to_string())?;
        let (tab_idx, terminal, runtime) = workspace
            .create_command_tab(
                rows.max(4),
                cols.max(10),
                command.root().to_path_buf(),
                &command.command,
                &[],
                self.pane_scrollback_limit_bytes,
                self.host_terminal_theme,
                terminal_theme_binding,
                terminal_theme,
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
                execution_host_id: command.location.execution_host_id,
                terminal_id,
                status: crate::commands::CommandRunStatus::Running,
            },
        );

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
        terminal_theme_binding: Option<crate::terminal_theme::TerminalThemeBinding>,
        terminal_theme: Option<crate::terminal_theme::ResolvedTerminalTheme>,
    ) -> Result<(), String> {
        if !command.location.is_local() {
            return Err(format!(
                "Restarting project commands on execution host {} is unsupported; refusing local fallback",
                command.location.execution_host_id
            ));
        }
        if let Some(runtime) = terminal_runtimes.remove(terminal_id) {
            runtime.shutdown();
        }

        let (rows, cols) = self.estimate_pane_size();
        let (events, render_notify, render_dirty) = {
            let tab = self
                .workspaces
                .get(ws_idx)
                .and_then(|workspace| workspace.terminal_tab(tab_idx).ok())
                .ok_or_else(|| "Command tab disappeared".to_string())?;
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
            .ok_or_else(|| "Command pane identity disappeared".to_string())?;
        let runtime = crate::terminal::TerminalRuntime::spawn_shell_command(
            pane_id,
            rows.max(4),
            cols.max(10),
            command.root().to_path_buf(),
            &command.command,
            &launch_env,
            self.pane_scrollback_limit_bytes,
            crate::terminal_theme::PaneTerminalTheme {
                host: self.host_terminal_theme,
                resolved_override: terminal_theme,
            },
            events,
            render_notify,
            render_dirty,
        )
        .map_err(|err| err.to_string())?;
        terminal_runtimes.insert(terminal_id.clone(), runtime);
        if let Some(terminal) = self.terminals.get_mut(terminal_id) {
            terminal.cwd = command.root().to_path_buf();
            terminal.terminal_theme_binding = terminal_theme_binding;
        }
        if let Some(run) = self.command_runs.get_mut(&command.id) {
            run.status = crate::commands::CommandRunStatus::Running;
        }

        self.mark_session_dirty();
        Ok(())
    }

    #[cfg(test)]
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
            if run.execution_host_id.as_str() != crate::execution_host::LOCAL_EXECUTION_HOST_ID {
                // Remote runtimes have no coordinator-local child pid. Their
                // lifecycle is driven by worker events, never local process probes.
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

    pub(crate) fn command_scope_workspace_indices(&self, view: &ClientViewState) -> Vec<usize> {
        let idx = if matches!(view.mode, Mode::Navigate) {
            Some(view.selected_workspace)
        } else {
            view.active_workspace
        };
        idx.filter(|idx| self.workspaces.get(*idx).is_some())
            .into_iter()
            .collect()
    }

    pub(crate) fn refresh_command_catalog_with_hosts(
        &mut self,
        view: &ClientViewState,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        execution_hosts: Option<&mut crate::execution_host::ExecutionHostManager>,
    ) -> bool {
        let scope_ws_idx = self
            .command_scope_workspace_indices(view)
            .into_iter()
            .next();
        let mut local_roots = Vec::new();
        let mut remote_locations = Vec::new();
        for ws_idx in self.command_scope_workspace_indices(view) {
            let Some(workspace) = self.workspaces.get(ws_idx) else {
                continue;
            };
            for (_, tab) in workspace.terminal_tabs() {
                for pane_id in tab.layout.pane_ids() {
                    let Some(terminal_id) = workspace.terminal_id(pane_id) else {
                        continue;
                    };
                    let Some(terminal) = self.terminals.get(terminal_id) else {
                        continue;
                    };
                    if terminal.location.is_local() {
                        if let Some(cwd) =
                            tab.cwd_for_pane(pane_id, &self.terminals, terminal_runtimes)
                        {
                            local_roots.push(crate::commands::project_root_from_cwd(&cwd));
                        }
                    } else {
                        let mut location = terminal.location.clone();
                        if let Some(cwd) =
                            tab.cwd_for_pane(pane_id, &self.terminals, terminal_runtimes)
                        {
                            if let Ok(path) = crate::execution_host::HostPath::new(cwd) {
                                location.path = path;
                            }
                        }
                        remote_locations.push(location);
                    }
                }
            }
        }
        local_roots.sort();
        local_roots.dedup();
        remote_locations.sort_by(|left, right| {
            (left.execution_host_id.as_str(), left.path.as_path())
                .cmp(&(right.execution_host_id.as_str(), right.path.as_path()))
        });
        remote_locations.dedup();

        let mut catalog = local_roots
            .into_iter()
            .flat_map(|root| {
                let mut commands = crate::commands::discover_project_commands(&root);
                if let Some(git_root) = crate::workspace::git_repo_root(&root) {
                    if let Ok(command) = self.configured_project_command(
                        git_root,
                        crate::app::state::ProjectCommandKind::Review,
                        scope_ws_idx,
                    ) {
                        commands.push(command);
                    }
                }
                commands
            })
            .collect::<Vec<_>>();

        if let Some(hosts) = execution_hosts {
            for location in remote_locations {
                // Merge any successful snapshot first. Requesting refresh turns
                // Fresh into Pending and would hide the value if done first.
                let mut merge_locations = vec![location.clone()];
                if let Some(commands) =
                    hosts
                        .project_commands(&location)
                        .and_then(|observation| match observation {
                            crate::execution_host::HostObservation::Fresh { value, .. }
                            | crate::execution_host::HostObservation::Stale { value, .. } => {
                                Some(value.as_slice())
                            }
                            crate::execution_host::HostObservation::Pending {
                                previous: Some(value),
                                ..
                            } => Some(value.as_slice()),
                            crate::execution_host::HostObservation::Pending {
                                previous: None,
                                ..
                            } => None,
                            crate::execution_host::HostObservation::Failed {
                                error,
                                previous,
                                ..
                            } => {
                                if previous.is_none() {
                                    tracing::warn!(
                                        "project command observation failed for {}: {}",
                                        location.execution_host_id,
                                        error.message
                                    );
                                }
                                previous.as_ref().map(|value| value.as_slice())
                            }
                        })
                {
                    for snapshot in commands {
                        if let Some(command) =
                            crate::commands::project_command_from_snapshot(snapshot.clone())
                        {
                            if !merge_locations
                                .iter()
                                .any(|existing| existing == &command.location)
                            {
                                merge_locations.push(command.location.clone());
                            }
                            catalog.push(command);
                        }
                    }
                }
                // Kick a refresh for the next cycle after merging this one.
                if let Err(error) = hosts.request_project_commands(location.clone()) {
                    tracing::warn!(
                        "project command observation request failed for {}: {error}",
                        location.execution_host_id
                    );
                }
                // Merge configured git-diff at each worker-qualified root so it stays host-routed.
                if self.project_command_configured(ProjectCommandKind::Review) {
                    for merge_location in merge_locations {
                        catalog.push(self.configured_project_command_for_location(
                            merge_location,
                            ProjectCommandKind::Review,
                            scope_ws_idx,
                        ));
                    }
                }
            }
        }

        catalog.sort_by_key(|command| {
            (
                command.location.execution_host_id.as_str().to_string(),
                command.location.path.as_path().to_path_buf(),
                command.confidence,
                command.source,
                command.name.clone(),
            )
        });

        let changed = self.command_catalog != catalog;
        self.command_catalog = catalog;
        changed
    }

    pub fn apply_effective_theme(&mut self) {
        self.palette = self.global_palette.clone();
        self.theme_name = self.global_theme_name.clone();
        self.effective_theme_appearance = self.theme_appearance_for_mode(self.global_theme_mode);
    }
    #[cfg(test)]
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
        self.effective_theme_appearance = self.theme_appearance_for_mode(mode);
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

    #[cfg(test)]
    pub fn create_group(&mut self, name: String) -> usize {
        self.create_group_with_icon_and_default_location(
            name,
            super::state::DEFAULT_GROUP_ICON.to_string(),
            None,
        )
    }

    pub fn create_group_with_icon_and_default_location(
        &mut self,
        name: String,
        icon: String,
        default_location: Option<crate::execution_host::ResourceLocation>,
    ) -> usize {
        self.groups.push(Group {
            id: super::state::generate_group_id(),
            name,
            icon: super::state::normalize_group_icon(&icon),
            accent: None,
            default_location,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
            github_organization: None,
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

    pub fn set_group_github_organization(
        &mut self,
        group_idx: usize,
        organization: Option<super::state::GithubOrganization>,
    ) -> bool {
        let Some(group) = self.groups.get_mut(group_idx) else {
            return false;
        };
        if group.github_organization == organization {
            return false;
        }
        group.github_organization = organization;
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

    pub fn set_workspace_default_location(
        &mut self,
        ws_idx: usize,
        location: crate::execution_host::ResourceLocation,
    ) -> bool {
        let Some(workspace) = self.workspaces.get_mut(ws_idx) else {
            return false;
        };
        if workspace.record_default_location(location) {
            self.mark_session_dirty();
            return true;
        }
        false
    }
    pub fn set_workspace_github_scope(
        &mut self,
        ws_idx: usize,
        scope: crate::github::GithubRepositoryScope,
    ) -> bool {
        let Some(workspace) = self.workspaces.get_mut(ws_idx) else {
            return false;
        };
        if workspace.github_scope == scope {
            return false;
        }
        workspace.github_scope = scope;
        self.mark_session_dirty();
        true
    }

    pub fn set_group_default_location(
        &mut self,
        group_idx: usize,
        default_location: Option<crate::execution_host::ResourceLocation>,
    ) -> bool {
        let Some(group) = self.groups.get_mut(group_idx) else {
            return false;
        };
        if group.default_location == default_location {
            return false;
        }
        group.default_location = default_location;
        self.mark_session_dirty();
        true
    }

    pub fn set_group_icon(&mut self, group_idx: usize, icon: String) -> bool {
        let Some(group) = self.groups.get_mut(group_idx) else {
            return false;
        };
        group.icon = super::state::normalize_group_icon(&icon);
        self.mark_session_dirty();
        true
    }

    pub fn delete_group(
        &mut self,
        view: &mut ClientViewState,
        group_idx: usize,
    ) -> Result<(), &'static str> {
        if self.groups.len() <= 1 {
            return Err("Cannot delete the last group");
        }
        let Some(group) = self.groups.get(group_idx) else {
            return Err("Group not found");
        };
        let deleted_group_id = group.id.clone();
        let terminal_ids = self
            .workspaces
            .iter()
            .filter(|workspace| workspace.group_id == deleted_group_id)
            .flat_map(|workspace| {
                workspace
                    .terminal_tabs()
                    .flat_map(|(_, tab)| tab.panes.values())
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
        self.apply_effective_theme();
        self.mark_session_dirty();

        view.workspace_scroll = 0;
        view.agent_panel_scroll = 0;
        view.tab_scroll_follow_active = true;
        view.reconcile(self);
        view.return_to_active_workspace_mode();
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

    pub fn move_group(&mut self, view: &mut ClientViewState, source_idx: usize, insert_idx: usize) {
        if source_idx >= self.groups.len() || insert_idx > self.groups.len() {
            return;
        }

        let group = self.groups.remove(source_idx);
        let target_idx = if source_idx < insert_idx {
            insert_idx.saturating_sub(1)
        } else {
            insert_idx
        }
        .min(self.groups.len());
        self.groups.insert(target_idx, group);

        self.apply_effective_theme();
        self.mark_session_dirty();
        view.reconcile(self);
    }

    pub fn move_workspace_to_group(
        &mut self,
        view: &mut ClientViewState,
        ws_idx: usize,
        group_idx: usize,
    ) -> bool {
        let Some(group_id) = self.groups.get(group_idx).map(|group| group.id.clone()) else {
            return false;
        };
        let Some(workspace) = self.workspaces.get_mut(ws_idx) else {
            return false;
        };
        workspace.group_id = group_id;
        self.mark_session_dirty();
        view.reconcile(self);
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
                ws.terminal_tabs().flat_map(move |(_, tab)| {
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
                    suppress_completion: false,
                };
                self.apply_pane_state_change(ws_idx, pane_id, &change, false, None);
                Some(update)
            })
            .collect()
    }

    pub(crate) fn mark_active_tab_seen_for_view(&mut self, view: &mut ClientViewState) -> bool {
        let Some(ws_idx) = view.active_workspace else {
            return false;
        };
        let Some(tab_idx) = view.active_tab_index_for_workspace(self, ws_idx) else {
            return false;
        };
        let hold = view
            .focused_pane_for_workspace(self, ws_idx)
            .and_then(|(_, pane_id)| self.unseen_idle_hold_for_pane(ws_idx, pane_id));
        let changed = self
            .workspaces
            .get_mut(ws_idx)
            .is_some_and(|workspace| workspace.mark_tab_seen(tab_idx));
        view.triage_hold = hold;
        changed
    }

    pub fn move_workspace(
        &mut self,
        view: &mut ClientViewState,
        source_idx: usize,
        insert_idx: usize,
    ) {
        if source_idx >= self.workspaces.len() || insert_idx > self.workspaces.len() {
            return;
        }

        self.mark_session_dirty();
        let workspace = self.workspaces.remove(source_idx);
        let target_idx = if source_idx < insert_idx {
            insert_idx.saturating_sub(1)
        } else {
            insert_idx
        }
        .min(self.workspaces.len());
        self.workspaces.insert(target_idx, workspace);
        view.reconcile(self);
        view.workspace_scroll = view.workspace_scroll.min(view.selected_workspace);
    }

    pub(crate) fn terminal_ids_for_workspace(
        &self,
        ws_idx: usize,
    ) -> Vec<crate::terminal::TerminalId> {
        self.workspaces
            .get(ws_idx)
            .into_iter()
            .flat_map(|ws| ws.terminal_tabs())
            .flat_map(|(_, tab)| tab.panes.values())
            .map(|pane| pane.attached_terminal_id.clone())
            .collect()
    }

    pub(crate) fn pane_ids_for_workspace(&self, ws_idx: usize) -> Vec<PaneId> {
        self.workspaces
            .get(ws_idx)
            .into_iter()
            .flat_map(|ws| ws.terminal_tabs())
            .flat_map(|(_, tab)| tab.layout.pane_ids())
            .collect()
    }

    pub(crate) fn terminal_ids_for_tab(
        &self,
        ws_idx: usize,
        tab_idx: usize,
    ) -> Vec<crate::terminal::TerminalId> {
        self.workspaces
            .get(ws_idx)
            .and_then(|ws| ws.terminal_tab(tab_idx).ok())
            .into_iter()
            .flat_map(|tab| tab.panes.values())
            .map(|pane| pane.attached_terminal_id.clone())
            .collect()
    }

    pub(crate) fn pane_ids_for_tab(&self, ws_idx: usize, tab_idx: usize) -> Vec<PaneId> {
        self.workspaces
            .get(ws_idx)
            .and_then(|ws| ws.terminal_tab(tab_idx).ok())
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
                ws.terminal_tabs().any(|(_, tab)| {
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
    }

    pub(crate) fn remove_plugin_pane_records(
        &mut self,
        pane_ids: impl IntoIterator<Item = PaneId>,
    ) {
        for pane_id in pane_ids {
            self.plugin_panes.remove(&pane_id);
        }
    }

    #[cfg(test)]
    pub fn close_selected_workspace(&mut self, view: &mut ClientViewState) {
        if self.workspaces.is_empty() {
            return;
        }
        view.selection = None;
        view.selection_autoscroll = None;
        self.mark_session_dirty();
        let selected = view.selected_workspace.min(self.workspaces.len() - 1);
        let terminal_ids = self.terminal_ids_for_workspace(selected);
        let pane_ids = self.pane_ids_for_workspace(selected);
        if let Some(workspace_id) = self.workspaces.get(selected).map(|ws| ws.id.clone()) {
            crate::logging::workspace_closed(&workspace_id);
        }
        self.remove_plugin_pane_records(pane_ids);
        self.workspaces.remove(selected);
        self.remove_unattached_terminal_ids(terminal_ids);
        view.reconcile(self);
        view.workspace_scroll = view
            .workspace_scroll
            .min(self.workspaces.len().saturating_sub(1));
        view.tab_scroll_follow_active = true;
        view.return_to_active_workspace_mode();
    }
}

impl crate::app::App {
    fn focus_command_terminal(&mut self, terminal_id: &crate::terminal::TerminalId) -> bool {
        let Some((ws_idx, tab_idx, pane_id)) = self.state.command_terminal_target(terminal_id)
        else {
            return false;
        };
        self.default_client_view
            .focus_pane_in_workspace(&self.state, ws_idx, tab_idx, pane_id);
        self.default_client_view.return_to_active_workspace_mode();
        true
    }

    pub(crate) fn run_project_command_on_resolved_host(
        &mut self,
        command_id: &str,
    ) -> Result<(), String> {
        let command = self
            .state
            .command_catalog
            .iter()
            .find(|command| command.id == command_id)
            .cloned()
            .ok_or_else(|| format!("command {command_id} not found"))?;
        if command.location.is_local() {
            if let Some(run) = self.state.command_runs.get(command_id).cloned() {
                if run.status == crate::commands::CommandRunStatus::Running
                    && self.focus_command_terminal(&run.terminal_id)
                {
                    return Ok(());
                }
            }
            let result = self
                .state
                .run_local_project_command(&mut self.terminal_runtimes, command_id);
            if result.is_ok() {
                if let Some(run) = self.state.command_runs.get(command_id).cloned() {
                    self.focus_command_terminal(&run.terminal_id);
                }
            }
            return result;
        }

        if let Some(run) = self.state.command_runs.get(command_id).cloned() {
            match run.status {
                crate::commands::CommandRunStatus::Running => {
                    if self.pending_remote_creations.contains_key(&run.terminal_id) {
                        // In-flight remote create is already the active launch for this command.
                        return Ok(());
                    }
                    if self.focus_command_terminal(&run.terminal_id) {
                        return Ok(());
                    }
                    // Running record without a live target — fall through to a fresh launch.
                }
                crate::commands::CommandRunStatus::Stopped
                | crate::commands::CommandRunStatus::Failed
                | crate::commands::CommandRunStatus::Unknown => {
                    // Completed remote commands keep their prior tab for scrollback, but a
                    // re-run always allocates a fresh remote runtime/tab so stale terminal
                    // ids are never reused.
                }
            }
        }

        self.launch_remote_project_command(command)
    }

    fn launch_remote_project_command(
        &mut self,
        command: crate::commands::ProjectCommand,
    ) -> Result<(), String> {
        let hosts = self
            .execution_hosts
            .as_ref()
            .ok_or_else(|| "Execution host manager is unavailable".to_string())?;
        if let Err(error) = hosts.ensure_host_capability(
            &command.location.execution_host_id,
            crate::execution_host::protocol::WorkerCapability::Command,
        ) {
            return Err(error.to_string());
        }
        if let Err(error) = hosts.ensure_host_capability(
            &command.location.execution_host_id,
            crate::execution_host::protocol::WorkerCapability::Terminal,
        ) {
            return Err(error.to_string());
        }

        let (ws_idx, _, _) = self
            .state
            .command_target_for_location(&self.terminal_runtimes, &command.location)
            .ok_or_else(|| {
                format!(
                    "No pane for project {} on execution host {}",
                    command.location.path, command.location.execution_host_id
                )
            })?;
        let spec = crate::execution_host::protocol::CommandSpec {
            program: "/bin/sh".to_string(),
            args: vec!["-lc".to_string(), command.command.clone()],
            env: Vec::new(),
        };
        let terminal_id = self.begin_remote_tab(
            ws_idx,
            command.location.clone(),
            true,
            Some(spec),
            Vec::new(),
        )?;
        if self
            .configure_pending_remote_agent(&terminal_id, None, Some(command.name.clone()), None)
            .is_none()
        {
            self.complete_remote_creation_failed(
                terminal_id.clone(),
                "Pending project command creation disappeared".to_string(),
            );
            let _ = self.clear_command_runs_for_terminal(&terminal_id);
            return Err("Pending project command creation disappeared".to_string());
        }
        self.state.command_runs.insert(
            command.id.clone(),
            crate::commands::CommandRun {
                command_id: command.id,
                execution_host_id: command.location.execution_host_id,
                terminal_id,
                status: crate::commands::CommandRunStatus::Running,
            },
        );
        Ok(())
    }

    /// Drop command-run records keyed to a terminal that failed before commit so
    /// TUI/API callers can retry without being stuck on a phantom Running entry.
    pub(crate) fn clear_command_runs_for_terminal(
        &mut self,
        terminal_id: &crate::terminal::TerminalId,
    ) -> bool {
        let before = self.state.command_runs.len();
        self.state
            .command_runs
            .retain(|_, run| &run.terminal_id != terminal_id);
        before != self.state.command_runs.len()
    }
}

// ---------------------------------------------------------------------------
// Pane operations
// ---------------------------------------------------------------------------

impl AppState {
    pub(crate) fn close_pane_would_close_workspace(&self, ws_idx: usize, pane_id: PaneId) -> bool {
        self.workspaces.get(ws_idx).is_some_and(|ws| {
            ws.find_tab_index_for_pane(pane_id).is_some_and(|tab_idx| {
                ws.terminal_tab(tab_idx)
                    .is_ok_and(|tab| tab.layout.pane_count() <= 1)
                    && ws.tabs.len() <= 1
            })
        })
    }

    pub(crate) fn close_workspace_tab(
        &mut self,
        view: &mut ClientViewState,
        ws_idx: usize,
        tab_idx: usize,
    ) -> bool {
        if self
            .workspaces
            .get(ws_idx)
            .is_none_or(|ws| ws.tabs.get(tab_idx).is_none())
        {
            return false;
        }
        if view.active_workspace == Some(ws_idx) {
            view.selection = None;
            view.selection_autoscroll = None;
        }
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
        let Some(ws) = self.workspaces.get_mut(ws_idx) else {
            return false;
        };
        if !ws.close_tab_allow_empty(tab_idx) {
            return false;
        }
        if !pane_ids.is_empty() {
            self.remove_plugin_pane_records(pane_ids);
            self.remove_unattached_terminal_ids(terminal_ids);
        }
        crate::logging::tab_closed(&workspace_id, &closing_tab_id);
        self.mark_session_dirty();
        if view.active_workspace == Some(ws_idx) {
            view.hovered_tab = None;
            view.tab_scroll_follow_active = true;
        }
        view.reconcile(self);
        true
    }
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

impl AppState {
    pub(crate) fn url_at_pane_cell_for_view(
        &self,
        view: &crate::app::view_state::ClientViewState,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
        pane_id: crate::layout::PaneId,
        viewport_row: u16,
        col: u16,
    ) -> Option<String> {
        let ws_idx = view
            .active_workspace
            .filter(|idx| self.workspaces.get(*idx).is_some())?;
        let info = view
            .computed
            .pane_infos
            .iter()
            .find(|info| info.id == pane_id)?;
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

        let metrics = self.pane_scroll_metrics_in_workspace(terminal_runtimes, ws_idx, pane_id);
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
pub(super) fn word_bounds_at_column(row: &str, col: u16) -> Option<(u16, u16)> {
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
                .resolved_identity_cwd_from(None, &self.terminals, terminal_runtimes)
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
            if ws.cached_identity_cwd != result.resolved_identity_cwd
                || ws.cached_git_status_key != result.status_cache_key
                || ws.cached_auto_label != result.auto_label
            {
                ws.cached_identity_cwd = result.resolved_identity_cwd;
                ws.cached_git_status_key = result.status_cache_key;
                ws.cached_auto_label = result.auto_label;
                changed = true;
            }
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
        self.handle_app_event_with_notification_context(terminal_runtimes, event, None)
    }

    pub(crate) fn handle_app_event_for_active_tab(
        &mut self,
        event: AppEvent,
        notification_is_active_tab: bool,
    ) -> Vec<PaneStateUpdate> {
        self.handle_app_event_with_notification_context(
            None,
            event,
            Some(notification_is_active_tab),
        )
    }

    fn handle_app_event_with_notification_context(
        &mut self,
        terminal_runtimes: Option<&crate::terminal::TerminalRuntimeRegistry>,
        event: AppEvent,
        notification_is_active_tab: Option<bool>,
    ) -> Vec<PaneStateUpdate> {
        match event {
            AppEvent::PaneDied {
                pane_id,
                child_pid,
                exit_success,
                exit_code: _,
                exit_signal: _,
            } => {
                self.handle_pane_died(terminal_runtimes, pane_id, child_pid, exit_success);
                Vec::new()
            }
            AppEvent::UpdateReady { version, install } => {
                self.update_available = Some(version.clone());
                self.update_install = install;
                self.latest_release_notes_available = true;
                self.update_dismissed = true;
                Vec::new()
            }
            AppEvent::AgentDetectionManifestsUpdated { updated, status } => {
                self.agent_manifest_update_status = status;
                self.refresh_agent_manifest_summaries();
                if !updated.is_empty()
                    && matches!(
                        self.toast_config.delivery,
                        crate::config::ToastDelivery::Gardn
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
                        title: "Agent Detection Rules Updated".to_string(),
                        context: agent_list,
                        position: None,
                        target: None,
                    });
                }
                Vec::new()
            }
            AppEvent::AgentProcessDetected {
                pane_id,
                agent,
                observed_at,
            } => self
                .update_terminal_state_at_with_notification_context(
                    pane_id,
                    observed_at,
                    notification_is_active_tab,
                    |terminal| Some(terminal.set_detected_agent_process_at(agent, observed_at)),
                )
                .into_iter()
                .collect(),
            AppEvent::StateChanged {
                pane_id,
                agent,
                state,
                visible_blocker,
                visible_idle,
                visible_working,
                process_exited,
                observed_at,
            } => {
                let updates: Vec<_> = self
                    .update_terminal_state_at_with_notification_context(
                        pane_id,
                        observed_at,
                        notification_is_active_tab,
                        |terminal| {
                            Some(terminal.set_detected_state_with_screen_signals_at(
                                agent,
                                state,
                                visible_blocker,
                                visible_idle,
                                visible_working,
                                process_exited,
                                observed_at,
                            ))
                        },
                    )
                    .into_iter()
                    .collect();
                if !process_exited
                    && (visible_blocker || visible_idle || visible_working)
                    && self.toast.is_none()
                    && updates.iter().all(|update| !update.suppress_completion)
                {
                    self.show_missing_integration_warning_if_needed(pane_id, observed_at);
                }
                updates
            }
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
            } => {
                let updates: Vec<_> = self
                    .update_terminal_state_with_notification_context(
                        pane_id,
                        notification_is_active_tab,
                        |terminal| {
                            let mut mutation = terminal.set_hook_authority_with_session_ref(
                                source,
                                agent_label,
                                state,
                                message,
                                custom_status,
                                session_ref,
                                seq,
                            )?;
                            let launch_context_may_change =
                                terminal.launch_env.is_empty() || mutation.session_ref_changed;
                            if launch_context_may_change && terminal.launch_env != launch_env {
                                terminal.launch_env = launch_env;
                                mutation.session_ref_changed = true;
                            }
                            Some(mutation)
                        },
                    )
                    .into_iter()
                    .collect();
                self.clear_missing_integration_warning_if_integrated(pane_id);
                updates
            }
            AppEvent::HookSessionReported {
                pane_id,
                source,
                agent_label,
                seq,
                session_start_source,
                session_ref,
                launch_env,
            } => {
                let updates: Vec<_> = self
                    .update_terminal_state(pane_id, |terminal| {
                        let mut mutation = terminal.set_agent_session_ref_for_session_start(
                            source,
                            agent_label,
                            session_ref,
                            seq,
                            session_start_source,
                        )?;
                        let launch_context_may_change =
                            terminal.launch_env.is_empty() || mutation.session_ref_changed;
                        if launch_context_may_change && terminal.launch_env != launch_env {
                            terminal.launch_env = launch_env;
                            mutation.session_ref_changed = true;
                        }
                        Some(mutation)
                    })
                    .into_iter()
                    .collect();
                self.clear_missing_integration_warning_if_integrated(pane_id);
                updates
            }
            AppEvent::HookMetadataReported {
                pane_id,
                source,
                agent_label,
                applies_to_source,
                title,
                display_agent,
                custom_status,
                state_labels,
                tokens,
                clear_title,
                clear_display_agent,
                clear_custom_status,
                clear_state_labels,
                seq,
                ttl,
            } => {
                let updates: Vec<_> = self
                    .update_terminal_state(pane_id, |terminal| {
                        terminal.set_agent_metadata(crate::terminal::AgentMetadataReport {
                            source,
                            agent_label,
                            applies_to_source,
                            title,
                            display_agent,
                            custom_status,
                            state_labels,
                            tokens,
                            clear_title,
                            clear_display_agent,
                            clear_custom_status,
                            clear_state_labels,
                            ttl,
                            seq,
                        })
                    })
                    .into_iter()
                    .collect();
                self.clear_missing_integration_warning_if_integrated(pane_id);
                updates
            }
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
            } => {
                if !crate::agent_resume::is_official_agent_source(&source, &agent_label)
                    || crate::agent_resume::releases_process_owned_agent(&source, &agent_label)
                {
                    self.update_terminal_state(pane_id, |terminal| {
                        terminal.release_agent_with_mutation(
                            &source,
                            &agent_label,
                            session_ref,
                            seq,
                        )
                    })
                    .into_iter()
                    .collect()
                } else {
                    Vec::new()
                }
            }
            // Intercepted in App::handle_internal_event before reaching this
            // dispatch; never touches AppState.
            AppEvent::ClipboardWrite { .. }
            | AppEvent::ClientClipboardWrite { .. }
            | AppEvent::ClientOpenUrl { .. }
            | AppEvent::TerminalClipboardWrite { .. }
            | AppEvent::TerminalBell { .. }
            | AppEvent::ExecutionFileStaged { .. }
            | AppEvent::OpenUrl { .. } => Vec::new(),
            AppEvent::PrefixInputSource { .. } => Vec::new(),
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
            AppEvent::PluginCommandFinished { .. }
            | AppEvent::ConnectionRetirementPreviewed { .. }
            | AppEvent::ConnectionRetirementStarted { .. }
            | AppEvent::ConnectionRetired { .. } => Vec::new(),
        }
    }

    fn update_terminal_state<F>(&mut self, pane_id: PaneId, update: F) -> Option<PaneStateUpdate>
    where
        F: FnOnce(&mut crate::terminal::TerminalState) -> Option<TerminalStateMutation>,
    {
        self.update_terminal_state_with_notification_context(pane_id, None, update)
    }

    fn update_terminal_state_with_notification_context<F>(
        &mut self,
        pane_id: PaneId,
        notification_is_active_tab: Option<bool>,
        update: F,
    ) -> Option<PaneStateUpdate>
    where
        F: FnOnce(&mut crate::terminal::TerminalState) -> Option<TerminalStateMutation>,
    {
        self.update_terminal_state_at_with_notification_context(
            pane_id,
            std::time::Instant::now(),
            notification_is_active_tab,
            update,
        )
    }

    fn update_terminal_state_at_with_notification_context<F>(
        &mut self,
        pane_id: PaneId,
        activity_at: std::time::Instant,
        notification_is_active_tab: Option<bool>,
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
        let suppress_completion = self
            .terminals
            .get_mut(&terminal_id)
            .is_some_and(|terminal| terminal.finish_agent_process_acquisition());
        let observed_process_agent = self
            .terminals
            .get(&terminal_id)
            .and_then(|terminal| terminal.detected_agent);
        let change = mutation.effective_state_change?;
        let update = PaneStateUpdate {
            pane_id,
            ws_idx,
            previous_agent_label: change.previous_agent_label.clone(),
            previous_known_agent: change.previous_known_agent,
            previous_state: change.previous_state,
            previous_presentation: change.previous_presentation.clone(),
            agent_label: change
                .agent_label
                .clone()
                .or_else(|| {
                    mutation
                        .agent_released
                        .then(|| change.previous_agent_label.clone())
                        .flatten()
                })
                .or_else(|| {
                    observed_process_agent
                        .map(crate::detect::agent_label)
                        .map(str::to_string)
                }),
            known_agent: change
                .known_agent
                .or_else(|| {
                    mutation
                        .agent_released
                        .then_some(change.previous_known_agent)
                        .flatten()
                })
                .or(observed_process_agent),
            state: change.state,
            presentation: change.presentation.clone(),
            suppress_completion,
        };
        self.apply_pane_state_change(
            ws_idx,
            pane_id,
            &change,
            suppress_completion,
            notification_is_active_tab,
        );
        Some(update)
    }

    fn clear_missing_integration_warning_if_integrated(&mut self, pane_id: PaneId) {
        let Some(terminal_id) = self
            .workspaces
            .iter()
            .find_map(|ws| ws.terminal_id(pane_id).cloned())
        else {
            return;
        };
        let integrated = self.terminals.get(&terminal_id).is_some_and(|terminal| {
            terminal.has_gardn_integration_evidence_for_detected_agent_at(std::time::Instant::now())
        });
        if integrated && self.toast_is_missing_integration_warning_for_pane(pane_id) {
            self.toast = None;
        }
    }

    fn toast_is_missing_integration_warning_for_pane(&self, pane_id: PaneId) -> bool {
        self.toast.as_ref().is_some_and(|toast| {
            toast
                .target
                .as_ref()
                .is_some_and(|target| target.pane_id == pane_id)
                && toast.context.contains("gardn integration install")
        })
    }

    fn show_missing_integration_warning_if_needed(
        &mut self,
        pane_id: PaneId,
        now: std::time::Instant,
    ) {
        if !matches!(
            self.toast_config.delivery,
            crate::config::ToastDelivery::Gardn
        ) {
            return;
        }

        let Some(ws_idx) = self
            .workspaces
            .iter()
            .position(|ws| ws.pane_state(pane_id).is_some())
        else {
            return;
        };
        let Some(terminal_id) = self.workspaces[ws_idx]
            .pane_state(pane_id)
            .map(|pane| pane.attached_terminal_id.clone())
        else {
            return;
        };
        let Some(agent) = self
            .terminals
            .get_mut(&terminal_id)
            .and_then(|terminal| terminal.take_missing_integration_warning_agent(now))
        else {
            return;
        };

        let agent_label = crate::detect::agent_label(agent);
        let agent_title = missing_integration_agent_title(agent);
        let workspace_id = self.workspaces[ws_idx].id.clone();
        let workspace_label = self.workspaces[ws_idx].display_name();
        let context = format!(
            "{}; run `gardn integration install {agent_label}`, then restart agent",
            notification_context(&self.workspaces[ws_idx], &workspace_label, ws_idx, pane_id)
        );
        self.toast = Some(ToastNotification {
            kind: ToastKind::NeedsAttention,
            title: format!("{agent_title} detected without Gardn integration"),
            context,
            position: None,
            target: Some(ToastTarget {
                workspace_id,
                pane_id,
            }),
        });
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
        suppress_completion: bool,
        notification_is_active_tab: Option<bool>,
    ) {
        let is_active_tab = notification_is_active_tab.unwrap_or(false);
        let suppress_active_tab_notifications =
            active_tab_suppresses_notifications(is_active_tab, self.outer_terminal_focus);
        let entered_blocked =
            change.previous_state != AgentState::Blocked && change.state == AgentState::Blocked;
        if entered_blocked {
            self.advance_blocked_review_generation(pane_id);
        }
        let mut blocked_review_changed = false;
        {
            let Some(pane) = self.workspaces[ws_idx]
                .terminal_tabs_mut()
                .find_map(|(_, tab)| tab.panes.get_mut(&pane_id))
            else {
                return;
            };

            let next_blocked_review = if entered_blocked {
                if pane.blocked_review == crate::pane::BlockedReviewState::None {
                    crate::pane::BlockedReviewState::Pending
                } else {
                    pane.blocked_review
                }
            } else if change.state != AgentState::Blocked {
                crate::pane::BlockedReviewState::None
            } else {
                pane.blocked_review
            };
            if pane.blocked_review != next_blocked_review {
                pane.blocked_review = next_blocked_review;
                blocked_review_changed = true;
            }

            if change.state != AgentState::Idle {
                pane.seen = true;
            } else if !suppress_completion
                && is_background_completion_transition(change.previous_state, change.state)
            {
                pane.seen = suppress_active_tab_notifications;
            }
        }
        if blocked_review_changed {
            self.mark_session_dirty();
        }

        if !suppress_completion {
            if let Some(delivery) = self.record_or_deliver_agent_notification(
                ws_idx,
                pane_id,
                change,
                notification_is_active_tab,
            ) {
                self.apply_agent_notification_delivery(&delivery);
            }
        }
    }

    fn record_or_deliver_agent_notification(
        &mut self,
        ws_idx: usize,
        pane_id: PaneId,
        change: &EffectiveStateChange,
        notification_is_active_tab: Option<bool>,
    ) -> Option<AgentNotificationDelivery> {
        self.pending_agent_notifications.remove(&pane_id);

        let is_active_tab = notification_is_active_tab.unwrap_or(false);
        let suppress_active_tab_notifications =
            active_tab_suppresses_notifications(is_active_tab, self.outer_terminal_focus);
        let policy_suppresses_notifications =
            self.toast_config.delay_seconds == 0 && suppress_active_tab_notifications;

        let client_notification_kind = notification_toast_for_state_change(
            policy_suppresses_notifications,
            change.previous_state,
            change.state,
        );
        let sound = notification_sound_for_state_change(
            policy_suppresses_notifications,
            change.previous_state,
            change.state,
        );
        if client_notification_kind.is_none() && sound.is_none() {
            return None;
        }

        let agent_label = change
            .agent_label
            .clone()
            .or_else(|| change.previous_agent_label.clone())?;
        let known_agent = change.known_agent.or(change.previous_known_agent);
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
                known_agent,
                kind,
                change.state,
                is_active_tab,
            );
        }

        self.pending_agent_notifications.insert(
            pane_id,
            PendingAgentNotification {
                pane_id,
                workspace_id,
                agent_label,
                known_agent,
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
        is_active_tab: bool,
    ) -> Option<AgentNotificationDelivery> {
        let workspace = self.workspaces.get(ws_idx)?;
        let tab_idx = workspace.find_tab_index_for_pane(pane_id)?;
        let tab_number = workspace.public_tab_number(tab_idx)?;
        let tab_id = crate::workspace::public_tab_id_for_number(&workspace_id, tab_number);
        let terminal_id = workspace.pane_state(pane_id)?.attached_terminal_id.clone();
        let terminal_state = self.terminals.get(&terminal_id)?;
        if terminal_state.state != expected_state {
            return None;
        }
        if terminal_state
            .effective_agent_label()
            .is_some_and(|current| current != agent_label)
        {
            return None;
        }

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
            tab_id,
            terminal_id: terminal_id.to_string(),
            agent_label,
            known_agent,
            kind,
            toast,
            client_notification,
            sound,
        })
    }

    fn apply_agent_notification_delivery(&mut self, delivery: &AgentNotificationDelivery) {
        self.agent_notification_outbox.push_back(delivery.clone());

        if matches!(
            self.toast_config.delivery,
            crate::config::ToastDelivery::Gardn
        ) {
            if let Some(toast) = delivery.toast.clone() {
                self.toast = Some(toast);
            }
        }
    }

    pub(crate) fn take_agent_notification_deliveries(
        &mut self,
    ) -> std::collections::VecDeque<AgentNotificationDelivery> {
        std::mem::take(&mut self.agent_notification_outbox)
    }

    pub fn next_pending_agent_notification_deadline(&self) -> Option<std::time::Instant> {
        self.pending_agent_notifications
            .values()
            .map(|pending| pending.deadline)
            .min()
    }

    pub(crate) fn drain_due_agent_notifications_with_context(
        &mut self,
        now: std::time::Instant,
        is_active_tab: impl Fn(&Self, usize, PaneId) -> bool,
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
                is_active_tab(self, ws_idx, pane_id),
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
        state
    }

    fn temp_project(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "gardn-app-commands-{name}-{}-{}",
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
        std::fs::canonicalize(&root).unwrap_or(root)
    }

    #[test]
    fn notification_context_formats_resolved_workspace_label() {
        let state = app_with_workspaces(&["stale"]);
        let root = state.workspaces[0].terminal_tab(0).unwrap().root_pane;

        assert_eq!(
            notification_context(&state.workspaces[0], "__gardn_projects__", 0, root),
            "__gardn_projects__ · 1"
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
                "open ../gardn-checkouts/issue-1",
                "gardn",
                "../gardn-checkouts/issue-1",
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
            ("cargo test --package=gardn", "--package", "--package=gardn"),
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
                "tmux|newhoo|fixhoo|newmoo|notification|window_bell|gardn",
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
                "tmux|newhoo|fixhoo|newmoo|notification|window_bell|gardn",
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
    fn git_diff_with_multiple_observed_repos_opens_picker() {
        let first = temp_git_repo("diff-first-observed");
        let second = temp_git_repo("diff-second-observed");
        let mut state = app_with_workspaces(&["multi"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = first.clone();
        let tab_idx = state.workspaces[0].test_add_tab(Some("api"));
        let pane_id = state.workspaces[0].terminal_tab(tab_idx).unwrap().root_pane;
        let terminal = state.workspaces[0].terminal_id(pane_id).cloned().unwrap();
        state.ensure_test_terminals();
        state.terminals.get_mut(&terminal).unwrap().cwd = second.clone();

        state
            .open_project_command_for_workspace(
                &mut view,
                &mut terminal_runtimes,
                0,
                crate::app::state::ProjectCommandKind::Review,
            )
            .expect("multi-repo diff should open picker");

        assert_eq!(view.mode, Mode::GitRepoPicker);
        assert_eq!(view.git_repo_picker.roots, vec![first, second]);
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
        let root_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = parent;

        let expected = vec![
            std::fs::canonicalize(first).unwrap(),
            std::fs::canonicalize(second).unwrap(),
        ];
        assert_eq!(
            state.observed_git_repos_for_workspace(&terminal_runtimes, 0),
            expected
        );
    }

    #[test]
    fn hunk_diff_command_uses_terminal_detected_theme() {
        let root = temp_git_repo("hunk-terminal-command");
        let mut state = app_with_workspaces(&["web"]);
        state.theme_name = "system".to_string();
        state.global_theme_name = "system".to_string();
        state.palette = Palette::terminal();
        let command = state
            .configured_project_command(root, ProjectCommandKind::Review, Some(0))
            .unwrap();
        assert!(command
            .command
            .contains("exec hunk diff --watch --theme auto"));
        assert!(!command.command.contains("XDG_CONFIG_HOME"));
    }
    #[test]
    fn browser_command_preserves_terminal_theme_passthrough() {
        let root = temp_git_repo("browser-terminal-command");
        let mut state = app_with_workspaces(&["web"]);
        state.theme_name = "system".to_string();
        state.global_theme_name = "system".to_string();
        state.palette = Palette::terminal();
        let command = state
            .configured_project_command(root, ProjectCommandKind::Browser, Some(0))
            .unwrap();

        assert!(command.command.contains("exec terminal-browser"));
        assert!(state
            .curated_project_command_terminal_theme(ProjectCommandKind::Browser, Some(0))
            .is_none());
    }

    #[test]
    fn custom_review_command_launches_unchanged() {
        let root = temp_git_repo("plain-review-command");
        let mut state = app_with_workspaces(&["web"]);
        state.review_command = "git diff --stat".to_string();

        let command = state
            .configured_project_command(root, ProjectCommandKind::Review, None)
            .unwrap();

        assert_eq!(command.command, "git diff --stat");
        assert!(state
            .curated_project_command_terminal_theme(ProjectCommandKind::Review, None)
            .is_none());
    }

    #[test]
    fn fresh_ide_command_uses_terminal_builtin_theme() {
        let root = temp_project("fresh-ide-command");
        let mut state = app_with_workspaces(&["web"]);
        state.theme_name = "system".to_string();
        state.global_theme_name = "system".to_string();
        let command = state
            .configured_project_command(root, ProjectCommandKind::Editor, Some(0))
            .unwrap();

        assert!(command
            .command
            .contains("fresh --config \"$config_dir/config.json\" ."));
        assert!(command.command.contains("theme_ref=\"builtin://terminal\""));
        assert!(command.location.is_local());
    }

    #[test]
    fn named_theme_curated_commands_receive_theme_adapters() {
        let root = temp_git_repo("named-themed-commands");
        let mut state = app_with_workspaces(&["web"]);
        state.theme_name = "dracula".to_string();
        state.global_theme_name = "dracula".to_string();
        state.global_theme_mode = ThemeMode::Dark;
        state.palette = Palette::dracula();
        state.global_palette = state.palette.clone();

        let browser = state
            .configured_project_command(root.clone(), ProjectCommandKind::Browser, Some(0))
            .unwrap();
        let review = state
            .configured_project_command(root.clone(), ProjectCommandKind::Review, Some(0))
            .unwrap();
        let editor = state
            .configured_project_command(root, ProjectCommandKind::Editor, Some(0))
            .unwrap();

        assert!(browser.command.contains("exec terminal-browser"));
        assert!(review.command.contains("[custom_theme.syntax_scopes]"));
        assert!(editor
            .command
            .contains("theme_ref=\"file://$theme_dir/theme.json\""));
        assert!(editor.command.contains("\"cursor\": [189, 147, 249]"));
    }
    #[test]
    fn only_exact_curated_commands_receive_terminal_theme_ownership() {
        let mut state = app_with_workspaces(&["web"]);

        for kind in [
            ProjectCommandKind::Browser,
            ProjectCommandKind::Review,
            ProjectCommandKind::Editor,
        ] {
            let binding = state
                .curated_project_command_terminal_theme_binding(kind)
                .expect("exact built-in command should own its terminal theme");
            assert_eq!(
                binding.source,
                crate::terminal_theme::TerminalThemeSource::WorkspacePalette
            );
        }

        state.browser_command = "terminal-browser --debug".to_string();
        state.review_command = "env HUNK_THEME=dark hunk diff --watch".to_string();
        state.editor_command = "fresh --profile custom .".to_string();

        for kind in [
            ProjectCommandKind::Browser,
            ProjectCommandKind::Review,
            ProjectCommandKind::Editor,
        ] {
            assert_eq!(
                state.curated_project_command_terminal_theme_binding(kind),
                None
            );
        }
    }

    #[tokio::test]
    async fn git_diff_opens_configured_command_tab_named_after_repo_root() {
        let root = temp_git_repo("diff-command-tab");
        let mut state = app_with_workspaces(&["web"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = root.clone();

        state
            .open_project_command_for_workspace(
                &mut view,
                &mut terminal_runtimes,
                0,
                crate::app::state::ProjectCommandKind::Review,
            )
            .expect("single repo diff should open command tab");

        assert_eq!(view.mode, Mode::Terminal);
        assert_eq!(state.workspaces[0].tabs.len(), 2);
        let command_tab_idx = 1;
        let command_tab = state.workspaces[0].terminal_tab(command_tab_idx).unwrap();
        let command_tab_number = command_tab.number;
        let command_pane = command_tab.root_pane;
        let workspace_id = state.workspaces[0].id.clone();
        assert!(view.focus_pane_in_workspace(&state, 0, command_tab_idx, command_pane));
        assert_eq!(
            view.active_tab_for_workspace(&workspace_id),
            Some(command_tab_number)
        );
        assert_eq!(
            view.focused_pane_for_tab(&workspace_id, command_tab_number),
            Some(command_pane)
        );
        let expected_name = format!("Review · {}", root.file_name().unwrap().to_string_lossy());
        assert_eq!(
            state.workspaces[0]
                .tab_display_name(command_tab_idx)
                .as_deref(),
            Some(expected_name.as_str())
        );
    }

    #[test]
    fn git_diff_reports_error_after_last_terminal_tab_closed() {
        let root = temp_git_repo("diff-empty-workspace");
        let mut state = app_with_workspaces(&["web"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        state.workspaces[0].identity_cwd = std::path::PathBuf::from("/stale/identity");
        state.workspaces[0].default_location =
            crate::execution_host::ResourceLocation::local(root.clone()).unwrap();
        assert!(state.workspaces[0].close_tab_allow_empty(0));

        let err = state
            .open_project_command_for_workspace(
                &mut view,
                &mut terminal_runtimes,
                0,
                crate::app::state::ProjectCommandKind::Review,
            )
            .expect_err("empty workspace has no runtime handles for command tab");

        assert_eq!(err, "No Git Repo for Current Space");
        assert!(state.workspaces[0].tabs.is_empty());
    }

    #[test]
    fn git_diff_target_can_use_non_focused_workspace_repo_cwd() {
        let root = temp_git_repo("diff-extra-root");
        let mut state = app_with_workspaces(&["web"]);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
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
        let view = ClientViewState::from_default_client_state(&state);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = nested;

        assert_eq!(
            state.git_diff_target(&view, &terminal_runtimes),
            Some((root, 0))
        );
    }

    #[test]
    fn git_diff_target_uses_selected_space_in_navigate_mode() {
        let first = temp_git_repo("diff-first");
        let second = temp_git_repo("diff-second");
        let mut state = app_with_workspaces(&["first", "second"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        view.mode = Mode::Navigate;
        view.selected_workspace = 1;
        let first_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let second_pane = state.workspaces[1].terminal_tab(0).unwrap().root_pane;
        let first_terminal_id = state.terminal_id_for_pane(0, first_pane).unwrap();
        let second_terminal_id = state.terminal_id_for_pane(1, second_pane).unwrap();
        state.terminals.get_mut(&first_terminal_id).unwrap().cwd = first;
        state.terminals.get_mut(&second_terminal_id).unwrap().cwd = second.clone();

        assert_eq!(
            state.git_diff_target(&view, &terminal_runtimes),
            Some((second, 1))
        );
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
        let view = ClientViewState::from_default_client_state(&state);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = nested;

        assert!(state.refresh_command_catalog_with_hosts(&view, &terminal_runtimes, None));

        assert_eq!(state.command_catalog.len(), 1);
        assert_eq!(state.command_catalog[0].name, "dev");
        assert_eq!(state.command_catalog[0].root(), project.as_path());
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
        let mut view = ClientViewState::from_default_client_state(&state);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        view.agent_panel_scope = crate::app::state::AgentPanelScope::AllWorkspaces;
        let current_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let other_pane = state.workspaces[1].terminal_tab(0).unwrap().root_pane;
        let current_terminal_id = state.terminal_id_for_pane(0, current_pane).unwrap();
        let other_terminal_id = state.terminal_id_for_pane(1, other_pane).unwrap();
        state.terminals.get_mut(&current_terminal_id).unwrap().cwd = current.clone();
        state.terminals.get_mut(&other_terminal_id).unwrap().cwd = other;

        assert!(state.refresh_command_catalog_with_hosts(&view, &terminal_runtimes, None));

        assert_eq!(state.command_catalog.len(), 1);
        assert_eq!(state.command_catalog[0].name, "dev");
        assert_eq!(state.command_catalog[0].root(), current.as_path());
    }

    #[test]
    fn command_catalog_uses_scoped_workspace_theme_for_curated_diff() {
        let root = temp_git_repo("catalog-workspace-theme");
        let mut state = app_with_workspaces(&["themed"]);
        let view = ClientViewState::from_default_client_state(&state);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        state.global_theme_name = "tokyo-night".to_string();
        state.global_theme_mode = ThemeMode::Dark;
        state.global_palette = Palette::tokyo_night();
        state.palette = state.global_palette.clone();
        assert!(state.set_group_accent(0, Some(crate::config::TerminalAccent::Red)));
        let root_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = root.clone();
        let expected = state
            .configured_project_command(root, ProjectCommandKind::Review, Some(0))
            .unwrap();

        assert!(state.refresh_command_catalog_with_hosts(&view, &terminal_runtimes, None));
        let diff = state
            .command_catalog
            .iter()
            .find(|command| command.name.starts_with("Review"))
            .expect("curated review command");

        assert_eq!(diff.command, expected.command);
    }

    fn project_command(
        root: std::path::PathBuf,
        name: &str,
        command: &str,
    ) -> crate::commands::ProjectCommand {
        crate::commands::ProjectCommand {
            id: format!("{}:package.json:{name}", root.display()),
            location: crate::execution_host::ResourceLocation::local(root.clone()).unwrap(),
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
        let root_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let root_terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&root_terminal_id).unwrap().cwd = project.clone();
        let command = project_command(project, "dev", "sleep 30");
        let command_id = command.id.clone();
        state.command_catalog = vec![command];

        state
            .run_local_project_command(&mut terminal_runtimes, &command_id)
            .unwrap();

        let run = state.command_runs.get(&command_id).unwrap();
        let command_terminal_id = run.terminal_id.clone();
        assert_eq!(run.status, crate::commands::CommandRunStatus::Running);
        assert!(terminal_runtimes.contains_key(&command_terminal_id));
        assert_eq!(state.workspaces[0].tabs.len(), 2);
        assert_eq!(
            state.workspaces[0].terminal_tab(1).unwrap().display_name(),
            "dev"
        );
        wait_for_runtime_pid(&terminal_runtimes, &command_terminal_id).await;
        assert_ne!(
            terminal_runtimes
                .get(&command_terminal_id)
                .map(|runtime| runtime.child_pid()),
            Some(0)
        );

        assert!(state.stop_project_command(&mut terminal_runtimes, &command_id));

        let run = state.command_runs.get(&command_id).unwrap();
        assert_eq!(run.status, crate::commands::CommandRunStatus::Stopped);
        assert!(!terminal_runtimes.contains_key(&command_terminal_id));

        state
            .run_local_project_command(&mut terminal_runtimes, &command_id)
            .unwrap();

        let run = state.command_runs.get(&command_id).unwrap();
        assert_eq!(run.status, crate::commands::CommandRunStatus::Running);
        assert_eq!(&run.terminal_id, &command_terminal_id);
        assert_eq!(state.workspaces[0].tabs.len(), 2);
        assert!(terminal_runtimes.contains_key(&command_terminal_id));
        wait_for_runtime_pid(&terminal_runtimes, &command_terminal_id).await;
        assert!(state.stop_project_command(&mut terminal_runtimes, &command_id));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pending_project_command_tab_number_tracks_reused_managed_run() {
        let project = temp_git_repo("pending-managed-run");
        let mut state = app_with_workspaces(&["web"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        state.review_command = "sleep 30".to_string();
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let root_terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&root_terminal_id).unwrap().cwd = project;

        assert_eq!(
            state.pending_project_command_tab_number_for_workspace(
                &terminal_runtimes,
                0,
                ProjectCommandKind::Review,
            ),
            Some(2)
        );
        state
            .open_project_command_for_workspace(
                &mut view,
                &mut terminal_runtimes,
                0,
                ProjectCommandKind::Review,
            )
            .unwrap();
        let command_id = state.command_runs.keys().next().unwrap().clone();
        let command_terminal_id = state.command_runs[&command_id].terminal_id.clone();
        wait_for_runtime_pid(&terminal_runtimes, &command_terminal_id).await;

        assert_eq!(
            state.pending_project_command_tab_number_for_workspace(
                &terminal_runtimes,
                0,
                ProjectCommandKind::Review,
            ),
            Some(2)
        );
        state.workspaces[0].test_add_tab(Some("unrelated"));
        assert!(state.stop_project_command(&mut terminal_runtimes, &command_id));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn command_pane_exit_retains_tab_and_records_failure() {
        let project = temp_project("exit-failure");
        let mut state = app_with_workspaces(&["web"]);
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let root_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let root_terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&root_terminal_id).unwrap().cwd = project.clone();
        let command = project_command(project, "check", "false");
        let command_id = command.id.clone();
        state.command_catalog = vec![command];

        state
            .run_local_project_command(&mut terminal_runtimes, &command_id)
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
        let root_pane = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let root_terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&root_terminal_id).unwrap().cwd = project.clone();
        let command = project_command(project, "dev", "sleep 30");
        let command_id = command.id.clone();
        state.command_catalog = vec![command];

        state
            .run_local_project_command(&mut terminal_runtimes, &command_id)
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
            .run_local_project_command(&mut terminal_runtimes, &command_id)
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
                execution_host_id: crate::execution_host::ExecutionHostId::local(),
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
    fn all_spaces_theme_accent_follows_active_workspace_group() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(&mut view, 1, side_group);
        state.set_group_accent(0, Some(crate::config::TerminalAccent::Blue));
        state.set_group_accent(side_group, Some(crate::config::TerminalAccent::Red));
        view.group_filter_enabled = false;
        view.active_group = 0;
        view.active_workspace = Some(1);

        state.apply_effective_theme();

        assert_eq!(state.palette.accent, state.global_palette.accent);
        assert_eq!(
            view.active_workspace_accent_color(&state),
            state.global_palette.red
        );

        view.active_workspace = Some(0);

        assert_eq!(state.palette.accent, state.global_palette.accent);
        assert_eq!(
            view.active_workspace_accent_color(&state),
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
        let mut view = ClientViewState::from_default_client_state(&state);
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(&mut view, 1, side_group);
        state.set_group_accent(side_group, Some(crate::config::TerminalAccent::Cyan));

        state.global_palette = Palette::dracula();
        state.global_theme_name = "dracula".to_string();
        state.set_group_accent(side_group, None);

        assert_eq!(state.theme_name, "dracula");
        assert_eq!(state.palette.accent, Palette::dracula().accent);
    }

    #[test]
    fn apply_workspace_git_statuses_updates_matching_workspace() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let first_id = state.workspaces[0].id.clone();
        let first_cwd = state.workspaces[0].identity_cwd.clone();
        let first_cwd_fingerprint = state.workspaces[0].git_status_cwds();
        let second_id = state.workspaces[1].id.clone();

        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let changed = state.apply_workspace_git_statuses(
            &terminal_runtimes,
            vec![WorkspaceGitStatus {
                workspace_id: first_id,
                resolved_identity_cwd: first_cwd.clone(),
                cwd_fingerprint: first_cwd_fingerprint,
                status_cache_key: first_cwd,
                auto_label: "one".into(),
                branch: Some("main".into()),
                ahead_behind: Some((2, 1)),
                work_summary: Some(GitWorkSummary {
                    repo_count: 1,
                    modified: 2,
                    ..GitWorkSummary::default()
                }),
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
                status_cache_key: std::path::PathBuf::from("/definitely/not/current"),
                auto_label: "stale".into(),
                branch: Some("main".into()),
                ahead_behind: Some((0, 1)),
                work_summary: Some(GitWorkSummary {
                    repo_count: 1,
                    added: 1,
                    ..GitWorkSummary::default()
                }),
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
        let cwd = state.workspaces[0].identity_cwd.clone();
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
                resolved_identity_cwd: cwd.clone(),
                cwd_fingerprint,
                status_cache_key: cwd,
                auto_label: "one".into(),
                branch: None,
                ahead_behind: None,
                work_summary: None,
            }],
        );

        assert!(changed);
        assert_eq!(state.workspaces[0].branch(), None);
        assert_eq!(state.workspaces[0].git_ahead_behind(), None);
        assert_eq!(state.workspaces[0].git_work_summary_label(), "");
    }

    #[test]
    fn update_ready_marks_available_without_toast() {
        let mut state = AppState::test_new();
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;

        let updates = state.handle_app_event(crate::events::AppEvent::UpdateReady {
            version: "0.5.0".into(),
            install: crate::install::UpdateInstallAction::Direct,
        });

        assert!(updates.is_empty());
        assert_eq!(state.update_available.as_deref(), Some("0.5.0"));
        assert!(state.latest_release_notes_available);
        assert_eq!(state.toast, None);
    }

    #[test]
    fn move_group_reorders_without_changing_active_group() {
        let mut state = app_with_workspaces(&["a", "b"]);
        let work_group = state.create_group("work".to_string());
        state.create_group("ops".to_string());
        let mut view = ClientViewState::from_default_client_state(&state);
        view.active_group = work_group;
        view.reconcile(&state);
        state.set_group_accent(work_group, Some(crate::config::TerminalAccent::Red));
        let active_group_id = state.groups[work_group].id.clone();
        let active_accent = view.active_workspace_accent_color(&state);

        state.move_group(&mut view, work_group, state.groups.len());

        let names: Vec<_> = state
            .groups
            .iter()
            .map(|group| group.name.as_str())
            .collect();
        assert_eq!(names, vec!["Group 1", "ops", "work"]);
        assert_eq!(state.groups[view.active_group].id, active_group_id);
        assert_eq!(view.active_workspace_accent_color(&state), active_accent);
    }

    #[test]
    fn move_workspace_reorders_without_changing_logical_selection() {
        let mut state = app_with_workspaces(&["a", "b", "c"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        let active_id = state.workspaces[1].id.clone();
        let selected_id = state.workspaces[2].id.clone();
        view.active_workspace = Some(1);
        view.selected_workspace = 2;
        view.reconcile(&state);

        state.move_workspace(&mut view, 1, 0);

        let names: Vec<_> = state
            .workspaces
            .iter()
            .map(|ws| ws.display_name())
            .collect();
        assert_eq!(names, vec!["b", "a", "c"]);
        assert_eq!(view.active_workspace, Some(0));
        assert_eq!(view.selected_workspace, 2);
        assert_eq!(
            state.workspaces[view.active_workspace.unwrap()].id,
            active_id
        );
        assert_eq!(state.workspaces[view.selected_workspace].id, selected_id);
    }

    #[test]
    fn move_workspace_accepts_insert_at_end() {
        let mut state = app_with_workspaces(&["a", "b", "c"]);
        let mut view = ClientViewState::from_default_client_state(&state);

        state.move_workspace(&mut view, 0, state.workspaces.len());

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
        let mut view = ClientViewState::from_default_client_state(&state);
        view.selected_workspace = 1;
        view.active_workspace = Some(1);
        view.reconcile(&state);

        state.close_selected_workspace(&mut view);

        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(view.selected_workspace, 1);
        assert_eq!(view.active_workspace, Some(1));
        assert_eq!(state.workspaces[1].custom_name.as_deref(), Some("c"));
    }

    #[test]
    fn close_non_focused_workspace_keeps_focus() {
        let mut state = app_with_workspaces(&["a", "b", "c"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        view.selected_workspace = 1;
        view.active_workspace = Some(0);
        view.reconcile(&state);
        let focused_id = state.workspaces[0].id.clone();

        state.close_selected_workspace(&mut view);

        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(view.active_workspace, Some(0));
        assert_eq!(view.selected_workspace, 0);
        assert_eq!(state.workspaces[view.selected_workspace].id, focused_id);
        assert_eq!(state.workspaces[0].custom_name.as_deref(), Some("a"));
        assert_eq!(state.workspaces[1].custom_name.as_deref(), Some("c"));
    }

    #[test]
    fn close_workspace_reanchors_selection_inside_connection_filter() {
        let remote_location = crate::execution_host::ResourceLocation::new(
            crate::execution_host::ExecutionHostId::new("ssh:workbox:1")
                .expect("valid execution host id"),
            crate::execution_host::HostPath::new("/work").expect("valid host path"),
        );
        let mut state = AppState::test_new();
        let mut selected_remote = Workspace::test_new("selected-remote");
        selected_remote.default_location = remote_location.clone();
        let mut first_remaining_remote = Workspace::test_new("first-remaining-remote");
        first_remaining_remote.default_location = remote_location.clone();
        let mut second_remaining_remote = Workspace::test_new("second-remaining-remote");
        second_remaining_remote.default_location = remote_location;
        state.workspaces = vec![
            Workspace::test_new("active-local"),
            selected_remote,
            Workspace::test_new("hidden-local"),
            first_remaining_remote,
            second_remaining_remote,
        ];
        state.ssh_connection_profiles =
            vec![crate::persist::ssh_profiles::SshConnectionProfile::new(
                "workbox", "Workbox", "workbox", None,
            )
            .expect("valid SSH profile")];
        let mut view = ClientViewState::from_default_client_state(&state);
        view.active_workspace = Some(0);
        view.selected_workspace = 1;
        view.connection_scope = crate::app::connection_scope::ConnectionScope::Only(
            crate::app::connection_scope::ConnectionIdentity::Profile(
                crate::execution_host::SshProfileId::new("workbox").expect("valid profile id"),
            ),
        );
        view.reconcile(&state);

        state.close_selected_workspace(&mut view);

        assert_eq!(view.active_workspace, Some(0));
        assert_eq!(view.selected_workspace, 2);
        assert_eq!(
            state.workspaces[view.selected_workspace].display_name(),
            "first-remaining-remote"
        );
    }

    #[test]
    fn close_last_workspace_deletes_space_and_leaves_empty_group() {
        let mut state = app_with_workspaces(&["only"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        let terminal_id = state
            .terminal_id_for_pane(0, state.workspaces[0].terminal_tab(0).unwrap().root_pane)
            .unwrap();
        view.selected_workspace = 0;
        state.close_selected_workspace(&mut view);

        assert!(state.workspaces.is_empty());
        assert_eq!(view.active_workspace, None);
        assert_eq!(view.selected_workspace, 0);
        assert!(!state.terminals.contains_key(&terminal_id));
    }

    #[test]
    fn close_workspace_at_end_adjusts_selected() {
        let mut state = app_with_workspaces(&["a", "b"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        view.selected_workspace = 1;
        view.active_workspace = Some(1);
        view.reconcile(&state);

        state.close_selected_workspace(&mut view);

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(view.selected_workspace, 0);
        assert_eq!(view.active_workspace, Some(0));
    }

    #[test]
    fn pane_died_last_pane_removes_workspace() {
        let mut state = app_with_workspaces(&["a", "b"]);
        let pane_id = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        state.handle_pane_died(None, pane_id, 0, true);

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].custom_name.as_deref(), Some("b"));
    }

    #[test]
    fn pane_died_self_closing_earlier_workspace_keeps_focus() {
        let names = (0..20).map(|i| format!("ws{i:02}")).collect::<Vec<_>>();
        let name_refs = names.iter().map(String::as_str).collect::<Vec<_>>();
        let mut state = app_with_workspaces(&name_refs);
        let mut view = ClientViewState::from_default_client_state(&state);
        view.selected_workspace = 1;
        view.active_workspace = Some(1);
        view.reconcile(&state);
        let focused_id = state.workspaces[1].id.clone();
        let pane_id = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        state.handle_pane_died(None, pane_id, 0, true);
        view.reconcile(&state);

        assert_eq!(state.workspaces.len(), 19);
        assert_eq!(view.active_workspace, Some(0));
        assert_eq!(view.selected_workspace, 0);
        assert_eq!(
            state.workspaces[view.active_workspace.unwrap()].id,
            focused_id
        );
        assert_eq!(state.workspaces[0].custom_name.as_deref(), Some("ws01"));
    }

    #[test]
    fn pane_died_last_workspace_keeps_empty_space_active() {
        let mut state = app_with_workspaces(&["only"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        view.mode = Mode::Terminal;
        let pane_id = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        state.handle_pane_died(None, pane_id, 0, true);
        view.reconcile(&state);

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(view.active_workspace, Some(0));
        assert_eq!(view.mode, Mode::Terminal);
        assert!(state.workspaces[0].tabs.is_empty());
    }

    #[test]
    fn pane_died_multi_pane_keeps_workspace() {
        let mut state = app_with_workspaces(&["test"]);
        let second_id = state.workspaces[0].test_split(Direction::Horizontal);

        state.handle_pane_died(None, second_id, 0, true);

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(
            state.workspaces[0]
                .terminal_tabs()
                .next()
                .unwrap()
                .1
                .panes
                .len(),
            1
        );
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
        let mut view = ClientViewState::from_default_client_state(&state);
        let active_pane = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();
        let bg_pane = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        view.selection = Some(crate::selection::Selection::anchor(active_pane, 0, 0, None));
        view.selection_autoscroll = Some(crate::app::state::SelectionAutoscroll {
            direction: crate::app::state::SelectionAutoscrollDirection::Down,
            last_mouse_screen_col: 0,
            last_mouse_screen_row: 23,
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
        });

        state.handle_pane_died(None, bg_pane, 0, true);
        view.reconcile(&state);

        assert!(view.selection.is_some());
        assert!(view.selection_autoscroll.is_some());
    }

    #[test]
    fn pane_died_same_pane_clears_selection() {
        let mut state = app_with_workspaces(&["test"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        let first_id = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let second_id = state.workspaces[0].test_split(Direction::Horizontal);

        view.selection = Some(crate::selection::Selection::anchor(second_id, 0, 0, None));
        view.selection_autoscroll = Some(crate::app::state::SelectionAutoscroll {
            direction: crate::app::state::SelectionAutoscrollDirection::Down,
            last_mouse_screen_col: 0,
            last_mouse_screen_row: 23,
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
        });

        state.handle_pane_died(None, second_id, 0, true);
        view.reconcile(&state);

        // first_id still alive, workspace stays, but selection was on the dying pane
        assert!(view.selection.is_none());
        assert!(view.selection_autoscroll.is_none());
        assert_eq!(
            state.workspaces[0]
                .terminal_tabs()
                .next()
                .unwrap()
                .1
                .panes
                .len(),
            1
        );
        assert_eq!(
            state.workspaces[0]
                .terminal_tabs()
                .next()
                .unwrap()
                .1
                .panes
                .keys()
                .next()
                .unwrap(),
            &first_id
        );
    }

    #[test]
    fn state_changed_updates_pane() {
        let mut state = app_with_workspaces(&["test"]);
        let pane_id = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

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
            .terminal_tabs()
            .find_map(|(_, tab)| tab.panes.get(&pane_id))
            .map(|pane| pane.attached_terminal_id.clone())
            .unwrap();
        let terminal = state.terminals.get(&terminal_id).unwrap();
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(terminal.detected_agent, Some(Agent::Pi));
    }

    #[test]
    fn first_idle_after_process_detection_is_not_completion() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        let pane_id = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        state.handle_app_event(AppEvent::AgentProcessDetected {
            pane_id,
            agent: Agent::Pi,
            observed_at: std::time::Instant::now(),
        });
        let direct_idle = state
            .handle_app_event_for_active_tab(
                AppEvent::StateChanged {
                    pane_id,
                    agent: Some(Agent::Pi),
                    state: AgentState::Idle,
                    visible_blocker: false,
                    visible_idle: true,
                    visible_working: false,
                    process_exited: false,
                    observed_at: std::time::Instant::now(),
                },
                false,
            )
            .pop()
            .expect("direct idle state update");
        assert!(direct_idle.suppress_completion);
        assert!(state.toast.is_none());

        state.handle_app_event(AppEvent::AgentProcessDetected {
            pane_id,
            agent: Agent::Pi,
            observed_at: std::time::Instant::now(),
        });
        for agent_state in [AgentState::Working, AgentState::Blocked] {
            state.handle_app_event_for_active_tab(
                AppEvent::StateChanged {
                    pane_id,
                    agent: Some(Agent::Pi),
                    state: agent_state,
                    visible_blocker: agent_state == AgentState::Blocked,
                    visible_idle: false,
                    visible_working: agent_state == AgentState::Working,
                    process_exited: false,
                    observed_at: std::time::Instant::now(),
                },
                false,
            );
        }
        state.toast = None;
        let later_idle = state
            .handle_app_event_for_active_tab(
                AppEvent::StateChanged {
                    pane_id,
                    agent: Some(Agent::Pi),
                    state: AgentState::Idle,
                    visible_blocker: false,
                    visible_idle: true,
                    visible_working: false,
                    process_exited: false,
                    observed_at: std::time::Instant::now(),
                },
                false,
            )
            .pop()
            .expect("later idle after working");
        assert!(!later_idle.suppress_completion);
        assert!(state.toast.is_some());
    }

    #[test]
    fn detected_codex_without_gardn_integration_warns_once() {
        let mut state = app_with_workspaces(&["manual"]);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        let pane_id = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let observed_at = std::time::Instant::now();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: true,
            visible_working: false,
            process_exited: false,
            observed_at,
        });

        let toast = state.toast.as_ref().expect("missing integration toast");
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "Codex detected without Gardn integration");
        assert!(toast.context.contains("manual · 1"));
        assert!(toast.context.contains("gardn integration install codex"));
        assert_eq!(
            toast.target.as_ref().map(|target| target.pane_id),
            Some(pane_id)
        );

        state.toast = None;
        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: true,
            process_exited: false,
            observed_at: observed_at + std::time::Duration::from_millis(1),
        });

        assert!(state.toast.is_none());
    }

    #[test]
    fn detected_installable_agent_families_warn_without_gardn_integration() {
        for (agent, title, install_target) in [
            (Agent::Pi, "Pi", "pi"),
            (Agent::OhMyPi, "OMP", "omp"),
            (Agent::Claude, "Claude", "claude"),
            (Agent::OpenCode, "OpenCode", "opencode"),
        ] {
            let mut state = app_with_workspaces(&["manual"]);
            state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
            let pane_id = state.workspaces[0].terminal_tab(0).unwrap().root_pane;

            state.handle_app_event(AppEvent::StateChanged {
                pane_id,
                agent: Some(agent),
                state: AgentState::Idle,
                visible_blocker: false,
                visible_idle: true,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            });

            let toast = state.toast.as_ref().expect("missing integration toast");
            assert_eq!(
                toast.title,
                format!("{title} detected without Gardn integration")
            );
            assert!(
                toast
                    .context
                    .contains(&format!("gardn integration install {install_target}")),
                "{}",
                toast.context
            );
        }
    }

    #[test]
    fn gardn_session_report_suppresses_missing_integration_warning() {
        let mut state = app_with_workspaces(&["integrated"]);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        let pane_id = state.workspaces[0].terminal_tab(0).unwrap().root_pane;

        state.handle_app_event(AppEvent::HookSessionReported {
            pane_id,
            source: "gardn:codex".into(),
            agent_label: "codex".into(),
            seq: Some(1),
            session_start_source: None,
            session_ref: Some(crate::agent_resume::AgentSessionRef::id("codex-session").unwrap()),
            launch_env: Vec::new(),
        });
        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: true,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });

        assert!(state.toast.is_none());
    }

    #[test]
    fn process_exit_allows_next_manual_codex_session_to_warn() {
        let mut state = app_with_workspaces(&["manual"]);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        let pane_id = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let observed_at = std::time::Instant::now();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: true,
            visible_working: false,
            process_exited: false,
            observed_at,
        });
        assert_eq!(
            state.toast.as_ref().map(|toast| toast.title.as_str()),
            Some("Codex detected without Gardn integration")
        );

        state.toast = None;
        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: true,
            observed_at: observed_at + std::time::Duration::from_millis(1),
        });
        assert!(state.toast.is_none());

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: true,
            visible_working: false,
            process_exited: false,
            observed_at: observed_at + std::time::Duration::from_millis(2),
        });

        assert_eq!(
            state.toast.as_ref().map(|toast| toast.title.as_str()),
            Some("Codex detected without Gardn integration")
        );
    }

    #[test]
    fn state_changed_idle_in_background_marks_unseen() {
        let mut state = app_with_workspaces(&["active", "background"]);
        let bg_pane_id = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        // First set it to Working
        let bg_terminal_id = state.workspaces[1]
            .terminal_tabs()
            .find_map(|(_, tab)| tab.panes.get(&bg_pane_id))
            .map(|pane| pane.attached_terminal_id.clone())
            .unwrap();
        state.terminals.get_mut(&bg_terminal_id).unwrap().state = AgentState::Working;

        // Now transition to Idle while in background
        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id: bg_pane_id,
                agent: Some(Agent::Pi),
                state: AgentState::Idle,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            false,
        );

        let pane = state.workspaces[1]
            .terminal_tab(0)
            .unwrap()
            .panes
            .get(&bg_pane_id)
            .unwrap();
        assert!(!pane.seen);
    }

    #[test]
    fn active_tab_completion_marks_pane_seen() {
        let mut state = app_with_workspaces(&["active"]);
        state.outer_terminal_focus = Some(true);
        let pane_id = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();
        let terminal_id = state.workspaces[0]
            .terminal_tab(0)
            .unwrap()
            .panes
            .get(&pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        state.terminals.get_mut(&terminal_id).unwrap().state = AgentState::Working;
        state.workspaces[0]
            .terminal_tab_mut(0)
            .unwrap()
            .panes
            .get_mut(&pane_id)
            .unwrap()
            .seen = false;

        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id,
                agent: Some(Agent::Pi),
                state: AgentState::Idle,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            true,
        );

        let terminal = state.terminals.get(&terminal_id).unwrap();
        assert_eq!(terminal.state, AgentState::Idle);
        let pane = state.workspaces[0]
            .terminal_tab(0)
            .unwrap()
            .panes
            .get(&pane_id)
            .unwrap();
        assert!(pane.seen);
    }

    #[test]
    fn initial_idle_in_background_stays_seen() {
        let mut state = app_with_workspaces(&["active", "background"]);
        let bg_pane_id = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id: bg_pane_id,
                agent: Some(Agent::Pi),
                state: AgentState::Idle,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            false,
        );

        let pane = state.workspaces[1]
            .terminal_tab(0)
            .unwrap()
            .panes
            .get(&bg_pane_id)
            .unwrap();
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
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        let bg_pane_id = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id: bg_pane_id,
                agent: Some(Agent::Pi),
                state: AgentState::Blocked,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            false,
        );

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "pi Needs Attention");
        assert_eq!(toast.context, "background · 2");
    }

    #[test]
    fn delayed_background_toast_waits_and_revalidates_state() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        state.toast_config.delay_seconds = 1;
        let bg_pane_id = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id: bg_pane_id,
                agent: Some(Agent::Pi),
                state: AgentState::Blocked,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            false,
        );

        assert!(state.toast.is_none());
        let deadline = state
            .next_pending_agent_notification_deadline()
            .expect("pending delayed notification");
        let notification_view = ClientViewState::from_default_client_state(&state);
        let deliveries =
            state.drain_due_agent_notifications_with_context(deadline, |state, ws_idx, pane_id| {
                notification_view.active_workspace == Some(ws_idx)
                    && notification_view.active_tab_index_for_workspace(state, ws_idx)
                        == state.workspaces[ws_idx].find_tab_index_for_pane(pane_id)
            });

        assert_eq!(deliveries.len(), 1);
        let toast = state.toast.as_ref().expect("delayed toast");
        assert_eq!(toast.title, "pi Needs Attention");
        assert_eq!(toast.context, "background · 2");
        assert!(state.pending_agent_notifications.is_empty());
    }

    #[test]
    fn immediate_notification_uses_supplied_foreground_view() {
        let mut state = app_with_workspaces(&["foreground", "background"]);
        let pane_id = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id,
                agent: Some(Agent::Pi),
                state: AgentState::Blocked,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            false,
        );

        let mut deliveries = state.take_agent_notification_deliveries();
        let delivery = deliveries.pop_front().expect("notification delivery");
        assert!(delivery.client_notification.is_some());
    }

    #[test]
    fn hook_reported_unknown_agent_sets_toast_title_from_label() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        let bg_pane_id = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        state.handle_app_event_for_active_tab(
            AppEvent::HookStateReported {
                pane_id: bg_pane_id,
                source: "custom:hermes".into(),
                agent_label: "hermes".into(),
                state: AgentState::Blocked,
                message: None,
                custom_status: None,
                seq: None,
                session_ref: None,
                launch_env: Vec::new(),
            },
            false,
        );

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "hermes Needs Attention");
        assert_eq!(toast.context, "background · 2");
    }
    #[test]
    fn delayed_notification_rechecks_current_active_view() {
        let mut state = app_with_workspaces(&["foreground", "background"]);
        state.toast_config.delay_seconds = 1;
        let mut notification_view = ClientViewState::from_default_client_state(&state);
        let pane_id = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();
        let terminal_id = state.workspaces[1]
            .pane_state(pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        state.terminals.get_mut(&terminal_id).unwrap().state = AgentState::Working;

        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id,
                agent: Some(Agent::Pi),
                state: AgentState::Idle,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            false,
        );
        let deadline = state
            .next_pending_agent_notification_deadline()
            .expect("pending delayed notification");
        assert!(notification_view.focus_pane_in_workspace(&state, 1, 0, pane_id));

        let deliveries =
            state.drain_due_agent_notifications_with_context(deadline, |state, ws_idx, pane_id| {
                notification_view.active_workspace == Some(ws_idx)
                    && notification_view.active_tab_index_for_workspace(state, ws_idx)
                        == state.workspaces[ws_idx].find_tab_index_for_pane(pane_id)
            });

        assert!(deliveries.is_empty());
        assert!(state.take_agent_notification_deliveries().is_empty());
    }

    #[test]
    fn visible_blocker_overrides_hook_working_and_notifies() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        let bg_pane_id = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();
        let bg_terminal_id = state.workspaces[1]
            .terminal_tab(0)
            .unwrap()
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
            source: "gardn:codex".into(),
            agent_label: "codex".into(),
            state: AgentState::Working,
            message: None,
            custom_status: None,
            seq: Some(1),
            session_ref: None,
            launch_env: Vec::new(),
        });
        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id: bg_pane_id,
                agent: Some(Agent::Codex),
                state: AgentState::Blocked,
                visible_blocker: true,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            false,
        );

        let terminal = state.terminals.get(&bg_terminal_id).unwrap();
        assert_eq!(terminal.state, AgentState::Blocked);
        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "codex Needs Attention");
    }

    #[test]
    fn blocked_review_survives_initial_observation_and_resets_on_reentry() {
        let mut state = app_with_workspaces(&["agent"]);
        let pane_id = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        state.workspaces[0]
            .terminal_tab_mut(0)
            .unwrap()
            .panes
            .get_mut(&pane_id)
            .unwrap()
            .blocked_review = crate::pane::BlockedReviewState::Reviewed;

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Blocked,
            visible_blocker: true,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        assert_eq!(
            state.workspaces[0]
                .pane_state(pane_id)
                .unwrap()
                .blocked_review,
            crate::pane::BlockedReviewState::Reviewed
        );

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: true,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Blocked,
            visible_blocker: true,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        assert_eq!(
            state.workspaces[0]
                .pane_state(pane_id)
                .unwrap()
                .blocked_review,
            crate::pane::BlockedReviewState::Pending
        );

        let crate::app::state::ContextMenuKind::Agent {
            review_ref: Some(review_ref),
            ..
        } = state.agent_context_menu_kind(&[], 0, pane_id).unwrap()
        else {
            panic!("pending blocked agent should expose Mark Reviewed");
        };
        assert!(state.mark_blocked_reviewed(review_ref));
        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Blocked,
            visible_blocker: true,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        assert_eq!(
            state.workspaces[0]
                .pane_state(pane_id)
                .unwrap()
                .blocked_review,
            crate::pane::BlockedReviewState::Reviewed
        );

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: true,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Codex),
            state: AgentState::Blocked,
            visible_blocker: true,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        assert_eq!(
            state.workspaces[0]
                .pane_state(pane_id)
                .unwrap()
                .blocked_review,
            crate::pane::BlockedReviewState::Pending
        );
    }

    #[test]
    fn visible_idle_waits_before_overriding_claude_hook_working() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        let bg_pane_id = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();
        let bg_terminal_id = state.workspaces[1]
            .terminal_tab(0)
            .unwrap()
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
            source: "gardn:claude".into(),
            agent_label: "claude".into(),
            state: AgentState::Working,
            message: None,
            custom_status: None,
            seq: Some(1),
            session_ref: None,
            launch_env: Vec::new(),
        });
        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id: bg_pane_id,
                agent: Some(Agent::Claude),
                state: AgentState::Idle,
                visible_blocker: false,
                visible_idle: true,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            false,
        );

        let terminal = state.terminals.get(&bg_terminal_id).unwrap();
        assert_eq!(terminal.state, AgentState::Working);
        assert!(state.toast.is_none());
    }

    #[test]
    fn hidden_session_ref_only_update_marks_session_dirty_without_visible_update() {
        let mut state = app_with_workspaces(&["active"]);
        let pane_id = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

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
            source: "gardn:pi".into(),
            agent_label: "pi".into(),
            seq: Some(21),
            session_start_source: None,
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
        let same_session_updates = state.handle_app_event(AppEvent::HookSessionReported {
            pane_id,
            source: "gardn:pi".into(),
            agent_label: "pi".into(),
            seq: Some(22),
            session_start_source: None,
            session_ref: crate::agent_resume::AgentSessionRef::path("/tmp/two.jsonl"),
            launch_env: vec![("PI_CONFIG_DIR".into(), ".wrong-profile".into())],
        });

        assert!(same_session_updates.is_empty());
        assert!(!state.session_dirty);
        let terminal = state
            .terminals
            .get(&state.workspaces[0].terminal_id(pane_id).cloned().unwrap())
            .unwrap();
        assert_eq!(
            terminal.launch_env,
            vec![("PI_CONFIG_DIR".into(), ".pi-profile".into())]
        );

        state.session_dirty = false;
        let third_updates = state.handle_app_event(AppEvent::HookSessionReported {
            pane_id,
            source: "gardn:pi".into(),
            agent_label: "pi".into(),
            session_start_source: None,
            seq: Some(23),
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
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        let bg_pane_id = *state.workspaces[1]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();
        let bg_terminal_id = state.workspaces[1]
            .terminal_tab(0)
            .unwrap()
            .panes
            .get(&bg_pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        state.terminals.get_mut(&bg_terminal_id).unwrap().state = AgentState::Working;

        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id: bg_pane_id,
                agent: Some(Agent::Droid),
                state: AgentState::Idle,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            false,
        );

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::Finished);
        assert_eq!(toast.title, "droid Finished");
        assert_eq!(toast.context, "background · 2");
        let target = toast.target.as_ref().expect("toast target");
        assert_eq!(&target.workspace_id, &state.workspaces[1].id);
        assert_eq!(target.pane_id, bg_pane_id);
    }

    #[test]
    fn background_toast_includes_tab_name_when_workspace_has_multiple_tabs() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        state.workspaces[1]
            .terminal_tab_mut(0)
            .unwrap()
            .set_custom_name("main".into());
        let second_tab = state.workspaces[1].test_add_tab(Some("logs"));
        state.ensure_test_terminals();
        let bg_pane_id = state.workspaces[1]
            .terminal_tab(second_tab)
            .unwrap()
            .root_pane;

        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id: bg_pane_id,
                agent: Some(Agent::Pi),
                state: AgentState::Blocked,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            false,
        );

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "pi Needs Attention");
        assert_eq!(toast.context, "background · 2 · logs");
    }

    #[test]
    fn background_tab_in_active_workspace_still_sets_toast() {
        let mut state = app_with_workspaces(&["active"]);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        state.workspaces[0]
            .terminal_tab_mut(0)
            .unwrap()
            .set_custom_name("main".into());
        let second_tab = state.workspaces[0].test_add_tab(Some("logs"));
        state.ensure_test_terminals();
        let bg_pane_id = state.workspaces[0]
            .terminal_tab(second_tab)
            .unwrap()
            .root_pane;

        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id: bg_pane_id,
                agent: Some(Agent::Pi),
                state: AgentState::Blocked,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            false,
        );

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "pi Needs Attention");
        assert_eq!(toast.context, "active · 1 · logs");
    }

    #[test]
    fn active_workspace_active_tab_does_not_set_toast() {
        let mut state = app_with_workspaces(&["active"]);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        let pane_id = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id,
                agent: Some(Agent::Pi),
                state: AgentState::Blocked,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            true,
        );

        assert!(state.toast.is_none());
    }

    #[test]
    fn active_workspace_active_tab_keeps_gardn_toast_suppressed_when_outer_terminal_is_unfocused() {
        let mut state = app_with_workspaces(&["active"]);
        state.outer_terminal_focus = Some(false);
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
        let pane_id = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();

        state.handle_app_event_for_active_tab(
            AppEvent::StateChanged {
                pane_id,
                agent: Some(Agent::Pi),
                state: AgentState::Blocked,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            },
            true,
        );

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
    fn update_ready_sets_manual_update_state() {
        let mut state = AppState::test_new();
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;

        let updates = state.handle_app_event(AppEvent::UpdateReady {
            version: "0.5.0".into(),
            install: crate::install::UpdateInstallAction::Direct,
        });

        assert!(updates.is_empty());
        assert_eq!(state.update_available.as_deref(), Some("0.5.0"));
        assert!(state.latest_release_notes_available);
        assert!(state.update_dismissed);
        assert_eq!(state.toast, None);
    }

    #[test]
    fn update_ready_stores_event_install_action() {
        let mut state = AppState::test_new();
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;

        state.handle_app_event(AppEvent::UpdateReady {
            version: "0.5.0".into(),
            install: crate::install::UpdateInstallAction::Mise,
        });

        assert_eq!(
            state.update_install,
            crate::install::UpdateInstallAction::Mise
        );
        assert_eq!(state.toast, None);
    }

    #[test]
    fn agent_detection_manifest_update_event_updates_status_and_toast() {
        let mut state = AppState::test_new();
        state.toast_config.delivery = crate::config::ToastDelivery::Gardn;
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
        assert_eq!(toast.title, "Agent Detection Rules Updated");
        assert_eq!(toast.context, "codex 2026.06.10.1");
    }

    #[test]
    fn close_workspace_tab_removes_native_membership_without_terminal_cleanup() {
        let mut state = app_with_workspaces(&["test"]);
        let terminal_id = state.workspaces[0]
            .terminal_tab(0)
            .unwrap()
            .panes
            .values()
            .next()
            .unwrap()
            .attached_terminal_id
            .clone();
        let native_idx = state.workspaces[0].ensure_github_tab();
        let mut view = ClientViewState::from_default_client_state(&state);
        view.active_workspace = Some(0);
        view.selected_workspace = 0;

        assert!(state.close_workspace_tab(&mut view, 0, native_idx));

        assert!(state.workspaces[0].tabs.iter().all(|tab| !tab.is_github()));
        assert!(state.terminals.contains_key(&terminal_id));
        assert_eq!(view.active_workspace, Some(0));
        assert_eq!(view.selected_workspace, 0);
    }

    #[test]
    fn close_workspace_tab_keeps_global_focus_when_closing_background_native_tab() {
        let mut state = app_with_workspaces(&["active", "background"]);
        let native_idx = state.workspaces[1].ensure_github_tab();
        let mut view = ClientViewState::from_default_client_state(&state);
        view.active_workspace = Some(0);
        view.selected_workspace = 0;

        assert!(state.close_workspace_tab(&mut view, 1, native_idx));

        assert_eq!(view.active_workspace, Some(0));
        assert_eq!(view.selected_workspace, 0);
        assert!(state.workspaces[1].tabs.iter().all(|tab| !tab.is_github()));
    }

    #[test]
    fn close_workspace_removes_unattached_terminal_states() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let pane_id = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let terminal_id = state.terminal_id_for_pane(0, pane_id).unwrap();
        state.plugin_panes.insert(
            pane_id,
            crate::app::state::PluginPaneRecord {
                plugin_id: "example.pane".into(),
                entrypoint: "board".into(),
            },
        );
        let mut view = ClientViewState::from_default_client_state(&state);

        state.close_selected_workspace(&mut view);

        assert!(!state.terminals.contains_key(&terminal_id));
        assert!(!state.plugin_panes.contains_key(&pane_id));
    }

    #[test]
    fn delete_group_removes_unattached_terminal_states() {
        let mut state = app_with_workspaces(&["keep", "drop"]);
        let group_idx = state.create_group("work".into());
        let mut view = ClientViewState::from_default_client_state(&state);
        state.move_workspace_to_group(&mut view, 1, group_idx);
        let dropped_terminal_id = state
            .terminal_id_for_pane(1, state.workspaces[1].terminal_tab(0).unwrap().root_pane)
            .unwrap();
        let kept_terminal_id = state
            .terminal_id_for_pane(0, state.workspaces[0].terminal_tab(0).unwrap().root_pane)
            .unwrap();

        state.delete_group(&mut view, group_idx).unwrap();

        assert!(state.terminals.contains_key(&kept_terminal_id));
        assert!(!state.terminals.contains_key(&dropped_terminal_id));
    }

    #[test]
    fn delete_active_group_reapplies_surviving_group_accent() {
        let mut state = app_with_workspaces(&["keep", "drop"]);
        let group_idx = state.create_group("work".into());
        let mut view = ClientViewState::from_default_client_state(&state);
        state.move_workspace_to_group(&mut view, 1, group_idx);
        view.group_filter_enabled = true;
        view.active_group = 0;
        state.set_group_accent(0, Some(crate::config::TerminalAccent::Blue));
        let kept_accent = view.active_workspace_accent_color(&state);
        view.active_group = group_idx;
        view.reconcile(&state);
        state.set_group_accent(group_idx, Some(crate::config::TerminalAccent::Red));
        assert_ne!(view.active_workspace_accent_color(&state), kept_accent);

        state.delete_group(&mut view, group_idx).unwrap();

        assert_eq!(view.active_group, 0);
        assert_eq!(view.active_workspace_accent_color(&state), kept_accent);
    }

    #[test]
    fn changing_defaults_and_group_membership_does_not_move_existing_terminal() {
        let mut state = app_with_workspaces(&["placed"]);
        let pane_id = state.workspaces[0].terminal_tab(0).unwrap().root_pane;
        let terminal_id = state.terminal_id_for_pane(0, pane_id).unwrap();
        let original = crate::execution_host::ResourceLocation::new(
            crate::execution_host::ExecutionHostId::new("ssh:original").unwrap(),
            crate::execution_host::HostPath::new("/srv/original").unwrap(),
        );
        let terminal = state.terminals.get_mut(&terminal_id).unwrap();
        terminal.location = original.clone();
        terminal.cwd = original.path.as_path().to_path_buf();
        let group_idx = state.create_group_with_icon_and_default_location(
            "remote defaults".into(),
            crate::app::state::DEFAULT_GROUP_ICON.into(),
            Some(crate::execution_host::ResourceLocation::new(
                crate::execution_host::ExecutionHostId::new("ssh:new-default").unwrap(),
                crate::execution_host::HostPath::new("/srv/group").unwrap(),
            )),
        );
        let mut view = ClientViewState::from_default_client_state(&state);

        assert!(state.set_workspace_default_location(
            0,
            crate::execution_host::ResourceLocation::local("/local/workspace").unwrap(),
        ));
        assert!(state.set_group_default_location(
            group_idx,
            Some(crate::execution_host::ResourceLocation::local("/local/group").unwrap()),
        ));
        assert!(state.move_workspace_to_group(&mut view, 0, group_idx));

        let terminal = state.terminals.get(&terminal_id).unwrap();
        assert_eq!(terminal.location, original);
        assert_eq!(terminal.cwd, std::path::PathBuf::from("/srv/original"));
    }

    #[test]
    fn official_release_preserves_process_owned_agent_identity() {
        let mut state = app_with_workspaces(&["active"]);
        let pane_id = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();
        let terminal_id = state.workspaces[0]
            .pane_state(pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Claude),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: true,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        let terminal = state.terminals.get_mut(&terminal_id).unwrap();
        terminal.set_persisted_agent_session(crate::agent_resume::PersistedAgentSession {
            source: "gardn:claude".into(),
            agent: "claude".into(),
            session_ref: crate::agent_resume::AgentSessionRef::path(
                std::env::current_dir()
                    .unwrap()
                    .join("release-session.jsonl")
                    .display()
                    .to_string(),
            )
            .unwrap(),
        });
        terminal.set_hook_authority(
            "gardn:claude".into(),
            "claude".into(),
            AgentState::Working,
            None,
            Some(1),
        );
        terminal.set_agent_name("reviewer".into());
        state.session_dirty = false;

        let updates = state.handle_app_event(AppEvent::HookAgentReleased {
            pane_id,
            source: "gardn:claude".into(),
            agent_label: "claude".into(),
            known_agent: Some(Agent::Claude),
            session_ref: None,
            seq: Some(2),
        });

        assert!(updates.is_empty());
        let terminal = &state.terminals[&terminal_id];
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(terminal.detected_agent, Some(Agent::Claude));
        assert_eq!(terminal.agent_name.as_deref(), Some("reviewer"));
        assert!(terminal.full_lifecycle_hook_authority_active());
        assert!(!state.session_dirty);
    }

    #[test]
    fn custom_release_clears_report_owned_agent() {
        let mut state = app_with_workspaces(&["active"]);
        let pane_id = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();
        let terminal_id = state.workspaces[0]
            .pane_state(pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_hook_authority(
                "custom:agent".into(),
                "custom-agent".into(),
                AgentState::Working,
                None,
                Some(1),
            );

        state.handle_app_event(AppEvent::HookAgentReleased {
            pane_id,
            source: "custom:agent".into(),
            agent_label: "custom-agent".into(),
            known_agent: None,
            session_ref: None,
            seq: Some(2),
        });

        let terminal = &state.terminals[&terminal_id];
        assert!(terminal.hook_authority.is_none());
        assert_eq!(terminal.state, AgentState::Unknown);
    }

    #[test]
    fn process_exit_releases_a_newer_hook_owned_agent() {
        let mut state = app_with_workspaces(&["active"]);
        let pane_id = *state.workspaces[0]
            .terminal_tabs()
            .next()
            .unwrap()
            .1
            .panes
            .keys()
            .next()
            .unwrap();
        let terminal_id = state.workspaces[0]
            .pane_state(pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        let now = std::time::Instant::now();
        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: true,
            process_exited: false,
            observed_at: now,
        });
        state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_hook_authority(
                "gardn:pi".into(),
                "pi".into(),
                AgentState::Working,
                None,
                Some(1),
            );
        state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_agent_name("reviewer".into());

        let updates = state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: true,
            visible_working: false,
            process_exited: true,
            observed_at: now + std::time::Duration::from_millis(1),
        });

        let terminal = &state.terminals[&terminal_id];
        assert_eq!(terminal.state, AgentState::Idle);
        assert!(terminal.agent_name.is_none());
        assert!(terminal.hook_authority.is_none());
        assert!(updates
            .iter()
            .any(|update| update.known_agent == Some(Agent::Pi)));
    }
}
