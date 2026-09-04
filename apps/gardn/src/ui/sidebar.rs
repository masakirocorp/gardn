mod tokens;

use self::tokens::{agent_rows, separator, space_rows, ResolvedToken, SpaceTokenContext};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::scrollbar::{render_scrollbar, should_show_scrollbar};
use super::status::{
    agent_section_icon, agent_section_style, state_icon, state_label, state_label_color,
    AgentStatusGroup,
};
use super::text::display_width;
use super::widgets::fill_rect;
use crate::app::state::{AgentPanelScope, CollapsedSidebarHover, Palette};
use crate::app::{AppState, ClientViewState, Mode};
use crate::detect::AgentState;
use crate::terminal::TerminalRuntimeRegistry;

const WORKSPACE_SECTION_HEADER_ROWS: u16 = 2;
const AGENT_PANEL_HEADER_ROWS: u16 = 2;
const COLLAPSED_SECTION_HEADER_ROWS: u16 = 2;
const SIDEBAR_GROUP_CHEVRON_COL: u16 = 0;
const SIDEBAR_GROUP_ICON_COL: u16 = 2;
const SIDEBAR_GROUP_NAME_COL: u16 = 4;
const SIDEBAR_WORKSPACE_STATE_COL: u16 = 2;
const SIDEBAR_WORKSPACE_NAME_COL: u16 = 4;
const SIDEBAR_GROUP_COUNT_RIGHT_PAD: u16 = 1;
const RIGHT_SECTION_COUNT_RIGHT_PAD: u16 = 1;
const RIGHT_ENTRY_PRIMARY_COL: u16 = 4;
const RIGHT_SUBSECTION_MARKER_COL: u16 = 0;
const RIGHT_SUBSECTION_ICON_COL: u16 = 2;
const RIGHT_SUBSECTION_LABEL_COL: u16 = 4;

#[derive(Clone)]
pub(crate) struct AgentPanelEntry {
    pub ws_idx: usize,
    pub tab_idx: usize,
    pub pane_id: crate::layout::PaneId,
    pub group_context_idx: Option<usize>,
    pub primary_label: String,
    pub pane_label: Option<String>,
    pub primary_tab_label: Option<String>,
    pub agent_label: Option<String>,
    pub terminal_title: Option<String>,
    pub terminal_title_stripped: Option<String>,
    pub state: AgentState,
    pub agent: Option<crate::detect::Agent>,
    pub seen: bool,
    pub custom_status: Option<String>,
    pub state_labels: std::collections::HashMap<String, String>,
    pub tokens: std::collections::HashMap<String, String>,
    pub last_meaningful_agent_activity_seq: u64,
    pub last_meaningful_agent_activity_unix_secs: Option<u64>,
    pub follow_up_added_at_unix_secs: Option<u64>,
}

pub(crate) struct AgentPanelSection {
    pub group: AgentStatusGroup,
    pub entries: Vec<AgentPanelEntry>,
}

pub(crate) fn agent_panel_empty_row(section: &AgentPanelSection) -> Option<&'static str> {
    (section.group == AgentStatusGroup::FollowUp && section.entries.is_empty())
        .then_some("Drop an agent here")
}

pub(crate) fn agent_panel_section_item_count(section: &AgentPanelSection) -> usize {
    section.entries.len() + usize::from(agent_panel_empty_row(section).is_some())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentPanelHeaderTarget {
    pub section: String,
}

fn sidebar_section_heights(total_h: u16, split_ratio: f32) -> (u16, u16) {
    if total_h == 0 {
        return (0, 0);
    }

    if total_h < 6 {
        let ws_h = total_h.div_ceil(2);
        return (ws_h, total_h.saturating_sub(ws_h));
    }

    let ratio = split_ratio.clamp(0.1, 0.9);
    let ws_h = ((total_h as f32) * ratio).round() as u16;
    let ws_h = ws_h.clamp(3, total_h.saturating_sub(3));
    let detail_h = total_h.saturating_sub(ws_h);
    (ws_h, detail_h)
}

fn sidebar_content_rect(area: Rect, separator_on_left: bool) -> Rect {
    if area.width <= 1 || area.height == 0 {
        return Rect::default();
    }

    let content_w = area.width.saturating_sub(1);
    let content_x = if separator_on_left {
        area.x.saturating_add(1)
    } else {
        area.x
    };
    Rect::new(content_x, area.y, content_w, area.height)
}

fn expanded_sidebar_sections_with_separator(
    area: Rect,
    split_ratio: f32,
    separator_on_left: bool,
) -> (Rect, Rect) {
    let content = sidebar_content_rect(area, separator_on_left);
    if content == Rect::default() {
        return (Rect::default(), Rect::default());
    }

    let (ws_h, detail_h) = sidebar_section_heights(content.height, split_ratio);
    let ws_area = Rect::new(content.x, content.y, content.width, ws_h);
    let detail_area = Rect::new(content.x, content.y + ws_h, content.width, detail_h);
    (ws_area, detail_area)
}

pub(crate) fn expanded_sidebar_sections(area: Rect, split_ratio: f32) -> (Rect, Rect) {
    expanded_sidebar_sections_with_separator(area, split_ratio, false)
}

pub(crate) fn right_aligned_expanded_sidebar_sections(
    area: Rect,
    split_ratio: f32,
) -> (Rect, Rect) {
    expanded_sidebar_sections_with_separator(area, split_ratio, true)
}

pub(crate) fn sidebar_section_divider_rect(area: Rect, split_ratio: f32) -> Rect {
    sidebar_section_divider_rect_with_separator(area, split_ratio, false)
}

pub(crate) fn right_aligned_sidebar_section_divider_rect(area: Rect, split_ratio: f32) -> Rect {
    sidebar_section_divider_rect_with_separator(area, split_ratio, true)
}

fn sidebar_section_divider_rect_with_separator(
    area: Rect,
    split_ratio: f32,
    separator_on_left: bool,
) -> Rect {
    let content = sidebar_content_rect(area, separator_on_left);
    if content == Rect::default() || content.height < 6 {
        return Rect::default();
    }

    let (ws_h, _) = sidebar_section_heights(content.height, split_ratio);
    Rect::new(content.x, content.y + ws_h + 1, content.width, 1)
}

pub(crate) fn agent_panel_toggle_label(scope: AgentPanelScope) -> &'static str {
    match scope {
        AgentPanelScope::CurrentWorkspace => "Space",
        AgentPanelScope::CurrentGroup => "Group",
        AgentPanelScope::AllWorkspaces => "All",
    }
}

fn agent_panel_group_idx(app: &AppState, ws_idx: usize) -> Option<usize> {
    let ws = app.workspaces.get(ws_idx)?;
    app.group_index_by_id(&ws.group_id)
}

fn agent_panel_has_multiple_groups(app: &AppState) -> bool {
    let Some(first_group_id) = app
        .workspaces
        .first()
        .map(|workspace| workspace.group_id.as_str())
    else {
        return false;
    };
    app.workspaces
        .iter()
        .any(|workspace| workspace.group_id != first_group_id)
}

pub(crate) fn agent_panel_toggle_rect(
    area: Rect,
    scope: AgentPanelScope,
    _leading_separator: bool,
) -> Rect {
    if area.width == 0 || area.height < 2 {
        return Rect::default();
    }

    let label = agent_panel_toggle_label(scope);
    let width = (label.chars().count() as u16 + 2).min(area.width);
    let y_offset = 0;
    Rect::new(
        area.x + area.width.saturating_sub(width),
        area.y + y_offset,
        width,
        1,
    )
}

pub(crate) fn agent_panel_entries(app: &AppState) -> Vec<AgentPanelEntry> {
    agent_panel_entries_with_runtimes(app, None, app.agent_panel_scope)
}

pub(crate) fn agent_panel_entries_from(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Vec<AgentPanelEntry> {
    agent_panel_entries_with_runtimes(app, Some(terminal_runtimes), app.agent_panel_scope)
}
pub(crate) fn agent_panel_entries_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
) -> Vec<AgentPanelEntry> {
    let mut entries = agent_panel_entries_with_context(
        app,
        Some(terminal_runtimes),
        view.agent_panel_scope,
        view.active_workspace,
        view.active_group,
    );
    crate::app::agent_view::apply_agent_view(app, view, &mut entries);
    entries
}

pub(crate) fn agent_panel_sections_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
) -> Vec<AgentPanelSection> {
    agent_panel_sections_from_entries(
        app,
        agent_panel_entries_for_view(app, terminal_runtimes, view),
        view.agent_view_override.is_none(),
    )
}

pub(crate) fn agent_panel_sections_all_workspaces(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Vec<AgentPanelSection> {
    agent_panel_sections_from_entries(
        app,
        agent_panel_entries_with_context(
            app,
            Some(terminal_runtimes),
            AgentPanelScope::AllWorkspaces,
            app.active,
            app.active_group,
        ),
        true,
    )
}

pub(crate) fn agent_panel_sections_all_workspaces_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
) -> Vec<AgentPanelSection> {
    agent_panel_sections_from_entries(
        app,
        agent_panel_entries_with_context(
            app,
            Some(terminal_runtimes),
            AgentPanelScope::AllWorkspaces,
            view.active_workspace,
            view.active_group,
        ),
        true,
    )
}

fn agent_panel_entries_with_runtimes(
    app: &AppState,
    terminal_runtimes: Option<&TerminalRuntimeRegistry>,
    scope: AgentPanelScope,
) -> Vec<AgentPanelEntry> {
    agent_panel_entries_with_context(app, terminal_runtimes, scope, app.active, app.active_group)
}

fn make_agent_panel_entry(
    ws_idx: usize,
    detail: crate::workspace::PaneDetail,
    group_context_idx: Option<usize>,
) -> AgentPanelEntry {
    AgentPanelEntry {
        ws_idx,
        tab_idx: detail.tab_idx,
        pane_id: detail.pane_id,
        group_context_idx,
        primary_label: detail.pane_label.clone(),
        pane_label: Some(detail.pane_label),
        primary_tab_label: Some(detail.tab_label),
        agent_label: Some(detail.agent_label),
        terminal_title: detail.terminal_title,
        terminal_title_stripped: detail.terminal_title_stripped,
        agent: detail.agent,
        state: detail.state,
        seen: detail.seen,
        custom_status: detail.custom_status,
        state_labels: detail.state_labels,
        tokens: detail.tokens,
        last_meaningful_agent_activity_seq: detail.last_meaningful_agent_activity_seq,
        last_meaningful_agent_activity_unix_secs: detail.last_meaningful_agent_activity_unix_secs,
        follow_up_added_at_unix_secs: None,
    }
}

fn agent_panel_pane_disambiguator(app: &AppState, entry: &AgentPanelEntry) -> Option<String> {
    let workspace = app.workspaces.get(entry.ws_idx)?;
    workspace
        .pane_state(entry.pane_id)
        .and_then(|pane| app.terminals.get(&pane.attached_terminal_id))
        .and_then(|terminal| terminal.manual_label.clone())
        .or_else(|| {
            workspace
                .pane_display_number(entry.pane_id)
                .map(|number| format!("Pane {number}"))
        })
}

fn disambiguate_agent_panel_labels(app: &AppState, entries: &mut [AgentPanelEntry]) {
    for entry in entries.iter_mut() {
        let Some(workspace) = app.workspaces.get(entry.ws_idx) else {
            continue;
        };
        let include_tab = workspace.tabs.len() > 1;
        let include_pane = workspace
            .tabs
            .get(entry.tab_idx)
            .is_some_and(|tab| tab.layout.pane_count() > 1);
        if !include_tab && !include_pane {
            continue;
        }

        let mut label = entry.primary_label.clone();
        if include_tab {
            if let Some(tab_label) = entry.primary_tab_label.as_deref() {
                label.push('/');
                label.push_str(tab_label);
            }
        }
        if include_pane {
            if let Some(pane_label) = agent_panel_pane_disambiguator(app, entry) {
                label.push('/');
                label.push_str(&pane_label);
            }
        }
        entry.primary_label = label;
    }
}
fn agent_panel_entries_with_context(
    app: &AppState,
    terminal_runtimes: Option<&TerminalRuntimeRegistry>,
    scope: AgentPanelScope,
    active_workspace: Option<usize>,
    active_group: usize,
) -> Vec<AgentPanelEntry> {
    let empty_runtimes;
    let terminal_runtimes = match terminal_runtimes {
        Some(terminal_runtimes) => terminal_runtimes,
        None => {
            empty_runtimes = TerminalRuntimeRegistry::new();
            &empty_runtimes
        }
    };

    let mut entries: Vec<AgentPanelEntry> = match scope {
        AgentPanelScope::CurrentWorkspace => {
            let Some(ws_idx) = active_workspace else {
                return Vec::new();
            };
            let Some(ws) = app.workspaces.get(ws_idx) else {
                return Vec::new();
            };
            ws.pane_details_from(&app.terminals, terminal_runtimes)
                .into_iter()
                .map(|detail| make_agent_panel_entry(ws_idx, detail, None))
                .collect()
        }
        AgentPanelScope::CurrentGroup => {
            let group_id = active_workspace
                .and_then(|idx| app.workspaces.get(idx))
                .map(|ws| ws.group_id.as_str())
                .or_else(|| app.groups.get(active_group).map(|group| group.id.as_str()))
                .unwrap_or_else(|| app.active_group_id());
            app.workspaces
                .iter()
                .enumerate()
                .filter(|(_, ws)| ws.group_id == group_id)
                .flat_map(|(ws_idx, ws)| {
                    ws.pane_details_from(&app.terminals, terminal_runtimes)
                        .into_iter()
                        .map(move |detail| make_agent_panel_entry(ws_idx, detail, None))
                })
                .collect()
        }
        AgentPanelScope::AllWorkspaces => app
            .workspaces
            .iter()
            .enumerate()
            .flat_map(|(ws_idx, ws)| {
                let group_context_idx = agent_panel_has_multiple_groups(app)
                    .then(|| agent_panel_group_idx(app, ws_idx))
                    .flatten();
                ws.pane_details_from(&app.terminals, terminal_runtimes)
                    .into_iter()
                    .map(move |detail| make_agent_panel_entry(ws_idx, detail, group_context_idx))
            })
            .collect(),
    };
    append_follow_up_fallback_entries(app, scope, active_workspace, active_group, &mut entries);
    disambiguate_agent_panel_labels(app, &mut entries);
    entries
}

fn agent_panel_scope_includes_workspace(
    app: &AppState,
    ws_idx: usize,
    scope: AgentPanelScope,
    active_workspace: Option<usize>,
    active_group: usize,
) -> bool {
    match scope {
        AgentPanelScope::AllWorkspaces => true,
        AgentPanelScope::CurrentWorkspace => active_workspace == Some(ws_idx),
        AgentPanelScope::CurrentGroup => {
            let Some(workspace) = app.workspaces.get(ws_idx) else {
                return false;
            };
            let group_id = active_workspace
                .and_then(|idx| app.workspaces.get(idx))
                .map(|ws| ws.group_id.as_str())
                .or_else(|| app.groups.get(active_group).map(|group| group.id.as_str()))
                .unwrap_or_else(|| app.active_group_id());
            workspace.group_id == group_id
        }
    }
}

fn follow_up_fallback_entry(
    app: &AppState,
    ws_idx: usize,
    tab_idx: usize,
    pane_id: crate::layout::PaneId,
) -> Option<AgentPanelEntry> {
    let workspace = app.workspaces.get(ws_idx)?;
    let pane = workspace.pane_state(pane_id)?;
    let terminal = app.terminals.get(&pane.attached_terminal_id);
    let tab_label = workspace
        .tab_display_name(tab_idx)
        .unwrap_or_else(|| (tab_idx + 1).to_string());
    let pane_label = workspace.display_name();
    let state = terminal
        .map(|terminal| terminal.state)
        .unwrap_or(AgentState::Unknown);
    let group_context_idx = agent_panel_has_multiple_groups(app)
        .then(|| agent_panel_group_idx(app, ws_idx))
        .flatten();
    Some(AgentPanelEntry {
        ws_idx,
        tab_idx,
        pane_id,
        group_context_idx,
        primary_label: pane_label.clone(),
        pane_label: Some(pane_label),
        primary_tab_label: Some(tab_label),
        agent_label: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent: None,
        state,
        seen: pane.seen,
        custom_status: None,
        state_labels: std::collections::HashMap::new(),
        tokens: std::collections::HashMap::new(),
        last_meaningful_agent_activity_seq: terminal
            .map(crate::terminal::TerminalState::last_meaningful_agent_activity_seq)
            .unwrap_or_default(),
        last_meaningful_agent_activity_unix_secs: terminal
            .and_then(crate::terminal::TerminalState::last_meaningful_agent_activity_unix_secs),
        follow_up_added_at_unix_secs: None,
    })
}

fn append_follow_up_fallback_entries(
    app: &AppState,
    scope: AgentPanelScope,
    active_workspace: Option<usize>,
    active_group: usize,
    entries: &mut Vec<AgentPanelEntry>,
) {
    for follow_up in &app.agent_follow_up {
        let Some((ws_idx, tab_idx, pane_id)) =
            app.resolve_live_agent_target(&follow_up.workspace_id, follow_up.pane_number)
        else {
            continue;
        };
        if !agent_panel_scope_includes_workspace(app, ws_idx, scope, active_workspace, active_group)
        {
            continue;
        }
        if entries
            .iter()
            .any(|entry| entry.ws_idx == ws_idx && entry.pane_id == pane_id)
        {
            continue;
        }
        if let Some(entry) = follow_up_fallback_entry(app, ws_idx, tab_idx, pane_id) {
            entries.push(entry);
        }
    }
}

fn agent_panel_entry_needs_triage(app: &AppState, entry: &AgentPanelEntry) -> bool {
    if entry.state == AgentState::Blocked {
        return true;
    }
    if entry.state != AgentState::Idle {
        return false;
    }
    if !entry.seen {
        return true;
    }
    let Some((workspace_id, pane_id)) = app.triage_hold.as_ref() else {
        return false;
    };
    app.workspaces
        .get(entry.ws_idx)
        .is_some_and(|workspace| workspace.id == *workspace_id)
        && entry.pane_id == *pane_id
}

#[cfg(test)]
pub(crate) fn agent_panel_triage_entries(app: &AppState) -> Vec<AgentPanelEntry> {
    let empty_runtimes = TerminalRuntimeRegistry::new();
    let mut entries: Vec<_> = agent_panel_entries_from(app, &empty_runtimes)
        .into_iter()
        .filter(|entry| agent_panel_entry_needs_triage(app, entry))
        .collect();
    sort_agent_panel_entries_by_oldest_activity(&mut entries);
    entries
}

fn agent_panel_entry_identity(entry: &AgentPanelEntry) -> (usize, usize, u32) {
    (entry.ws_idx, entry.tab_idx, entry.pane_id.raw())
}

fn sort_agent_panel_entries_by_recent_activity(entries: &mut [AgentPanelEntry]) {
    entries.sort_by(|left, right| {
        right
            .last_meaningful_agent_activity_unix_secs
            .cmp(&left.last_meaningful_agent_activity_unix_secs)
            .then_with(|| {
                right
                    .last_meaningful_agent_activity_seq
                    .cmp(&left.last_meaningful_agent_activity_seq)
            })
            .then_with(|| agent_panel_entry_identity(left).cmp(&agent_panel_entry_identity(right)))
    });
}

fn sort_agent_panel_entries_by_oldest_activity(entries: &mut [AgentPanelEntry]) {
    entries.sort_by(|left, right| {
        left.last_meaningful_agent_activity_unix_secs
            .unwrap_or(0)
            .cmp(&right.last_meaningful_agent_activity_unix_secs.unwrap_or(0))
            .then_with(|| {
                match (
                    left.last_meaningful_agent_activity_unix_secs,
                    right.last_meaningful_agent_activity_unix_secs,
                ) {
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    _ => std::cmp::Ordering::Equal,
                }
            })
            .then_with(|| {
                left.last_meaningful_agent_activity_seq
                    .cmp(&right.last_meaningful_agent_activity_seq)
            })
            .then_with(|| agent_panel_entry_identity(left).cmp(&agent_panel_entry_identity(right)))
    });
}

fn sort_follow_up_entries(entries: &mut [AgentPanelEntry]) {
    entries.sort_by_key(|entry| entry.follow_up_added_at_unix_secs.unwrap_or(0));
}

pub(crate) fn agent_panel_sections(app: &AppState) -> Vec<AgentPanelSection> {
    let empty_runtimes = TerminalRuntimeRegistry::new();
    agent_panel_sections_from(app, &empty_runtimes)
}

pub(crate) fn agent_panel_sections_from(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Vec<AgentPanelSection> {
    agent_panel_sections_from_entries(app, agent_panel_entries_from(app, terminal_runtimes), true)
}

fn agent_panel_sections_from_entries(
    app: &AppState,
    scoped_entries: Vec<AgentPanelEntry>,
    sort_by_recent_activity: bool,
) -> Vec<AgentPanelSection> {
    let mut sections = Vec::new();
    let mut follow_up = Vec::new();
    let mut rest = Vec::new();
    for mut entry in scoped_entries {
        if app.is_agent_follow_up(entry.ws_idx, entry.pane_id) {
            entry.follow_up_added_at_unix_secs =
                app.follow_up_added_at(entry.ws_idx, entry.pane_id);
            follow_up.push(entry);
        } else {
            rest.push(entry);
        }
    }
    sort_follow_up_entries(&mut follow_up);

    let mut triage: Vec<_> = rest
        .iter()
        .filter(|entry| agent_panel_entry_needs_triage(app, entry))
        .cloned()
        .collect();
    sort_agent_panel_entries_by_oldest_activity(&mut triage);
    if !triage.is_empty() {
        sections.push(AgentPanelSection {
            group: AgentStatusGroup::Triage,
            entries: triage,
        });
    }

    sections.push(AgentPanelSection {
        group: AgentStatusGroup::FollowUp,
        entries: follow_up,
    });

    let mut working: Vec<_> = rest
        .iter()
        .filter(|entry| entry.state == AgentState::Working)
        .cloned()
        .collect();
    if sort_by_recent_activity {
        sort_agent_panel_entries_by_recent_activity(&mut working);
    }
    if !working.is_empty() {
        sections.push(AgentPanelSection {
            group: AgentStatusGroup::Working,
            entries: working,
        });
    }

    let mut idle: Vec<_> = rest
        .into_iter()
        .filter(|entry| {
            entry.state != AgentState::Working && !agent_panel_entry_needs_triage(app, entry)
        })
        .collect();
    if sort_by_recent_activity {
        sort_agent_panel_entries_by_recent_activity(&mut idle);
    }
    if !idle.is_empty() {
        sections.push(AgentPanelSection {
            group: AgentStatusGroup::Idle,
            entries: idle,
        });
    }

    sections
}
pub(super) fn agent_panel_status_key(state: AgentState, seen: bool) -> &'static str {
    match (state, seen) {
        (AgentState::Idle, false) => "Done",
        (AgentState::Idle, true) => "Idle",
        (AgentState::Working, _) => "Working",
        (AgentState::Blocked, _) => "Blocked",
        (AgentState::Unknown, _) => "Unknown",
    }
}

fn truncate_text(text: &str, max_width: usize) -> String {
    let len = text.chars().count();
    if len <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let prefix: String = text.chars().take(max_width.saturating_sub(1)).collect();
    format!("{prefix}…")
}

fn count_suffix(text: &str) -> Option<(&str, &str)> {
    let start = text.rfind(" (")?;
    text.ends_with(')')
        .then_some((&text[..start], &text[start..]))
}

fn centered_count_line(text: &str, width: u16, base: Style, count: Style) -> Line<'static> {
    let width = width as usize;
    let text = truncate_text(text, width);
    let len = text.chars().count();
    let left = width.saturating_sub(len) / 2;
    let right = width.saturating_sub(len).saturating_sub(left);

    let mut spans = vec![Span::styled(" ".repeat(left), base)];
    if let Some((name, suffix)) = count_suffix(&text) {
        spans.push(Span::styled(name.to_string(), base));
        spans.push(Span::styled(suffix.to_string(), count));
    } else {
        spans.push(Span::styled(text, base));
    }
    spans.push(Span::styled(" ".repeat(right), base));
    Line::from(spans)
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn format_agent_activity_age(
    last_activity_unix_secs: Option<u64>,
    now_unix_secs: u64,
) -> Option<String> {
    let last_activity_unix_secs = last_activity_unix_secs?;
    let seconds = now_unix_secs.saturating_sub(last_activity_unix_secs);
    if seconds < 60 {
        return Some("Now".to_string());
    }

    let minutes = seconds / 60;
    if minutes < 60 {
        return Some(format!("{minutes}m"));
    }

    let hours = minutes / 60;
    if hours < 24 {
        return Some(format!("{hours}h"));
    }

    Some(format!("{}d", hours / 24))
}
pub(crate) fn compact_agent_entry_text(entry: &AgentPanelEntry) -> (String, String) {
    let mut metadata = Vec::new();
    if let Some(agent_label) = &entry.agent_label {
        metadata.push(agent_label.clone());
    }
    if let Some(custom_status) = &entry.custom_status {
        metadata.push(custom_status.clone());
    }
    if let Some(age) = format_agent_activity_age(
        entry
            .follow_up_added_at_unix_secs
            .or(entry.last_meaningful_agent_activity_unix_secs),
        current_unix_secs(),
    ) {
        metadata.push(age);
    }
    (entry.primary_label.clone(), metadata.join(" · "))
}

pub(crate) fn agent_panel_title_spans(
    label: &str,
    max_width: Option<usize>,
    leaf_style: Style,
    prefix_style: Style,
) -> Vec<Span<'static>> {
    let Some((prefix, leaf)) = label.rsplit_once('/') else {
        let text = match max_width {
            Some(width) => super::text::truncate_end(label, width),
            None => label.to_string(),
        };
        return vec![Span::styled(text, leaf_style)];
    };
    let prefix = format!("{prefix}/");
    let (prefix, leaf) = match max_width {
        None => (prefix, leaf.to_string()),
        Some(width) => {
            let leaf_width = display_width(leaf);
            if leaf_width >= width {
                (String::new(), super::text::truncate_end(leaf, width))
            } else {
                (
                    super::text::truncate_start(&prefix, width.saturating_sub(leaf_width)),
                    leaf.to_string(),
                )
            }
        }
    };
    let mut spans = Vec::new();
    if !prefix.is_empty() {
        spans.push(Span::styled(prefix, prefix_style));
    }
    if !leaf.is_empty() {
        spans.push(Span::styled(leaf, leaf_style));
    }
    spans
}

fn agent_panel_section_shows_entry_status(group: AgentStatusGroup) -> bool {
    matches!(group, AgentStatusGroup::Triage | AgentStatusGroup::FollowUp)
}

fn agent_panel_section_collapsed(app: &AppState, group: AgentStatusGroup) -> bool {
    app.agent_section_collapsed(group.label())
}

fn agent_panel_section_collapsed_for_view(view: &ClientViewState, group: AgentStatusGroup) -> bool {
    view.collapsed_agent_sections
        .iter()
        .any(|key| key == group.label())
}

fn agent_panel_should_show_agent_labels(sections: &[AgentPanelSection]) -> bool {
    let mut first_label: Option<&str> = None;
    for label in sections
        .iter()
        .flat_map(|section| section.entries.iter())
        .filter_map(|entry| entry.agent_label.as_deref())
    {
        match first_label {
            Some(first) if first != label => return true,
            Some(_) => {}
            None => first_label = Some(label),
        }
    }
    false
}

fn agent_panel_entry_has_secondary_detail(
    app: &AppState,
    show_status: bool,
    show_agent_label: bool,
    detail: &AgentPanelEntry,
) -> bool {
    resolved_agent_rows(app, detail).len() > 1
        || detail
            .group_context_idx
            .and_then(|group_idx| app.groups.get(group_idx))
            .is_some()
        || (show_agent_label && detail.agent_label.is_some())
        || show_status
        || detail.custom_status.is_some()
        || detail.last_meaningful_agent_activity_unix_secs.is_some()
        || detail.follow_up_added_at_unix_secs.is_some()
}

fn agent_panel_entry_row_height(
    app: &AppState,
    show_status: bool,
    show_agent_label: bool,
    detail: &AgentPanelEntry,
) -> u16 {
    let token_rows = resolved_agent_rows(app, detail).len().max(1) as u16;
    let legacy_rows =
        if agent_panel_entry_has_secondary_detail(app, show_status, show_agent_label, detail) {
            2
        } else {
            1
        };
    token_rows.max(legacy_rows)
}

fn agent_panel_entry_status_label(entry: &AgentPanelEntry) -> &'static str {
    state_label(entry.state, entry.seen)
}

fn resolved_agent_rows(app: &AppState, detail: &AgentPanelEntry) -> Vec<Vec<ResolvedToken>> {
    let state_text = detail
        .state_labels
        .get(agent_panel_status_key(detail.state, detail.seen))
        .map(String::as_str)
        .unwrap_or_else(|| agent_panel_entry_status_label(detail));
    agent_rows(&app.sidebar_config.agents, detail, state_text)
}

fn agent_token_line(
    tokens: &[ResolvedToken],
    detail: &AgentPanelEntry,
    app: &AppState,
    name_style: Style,
    agent_style: Style,
) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(
                separator(&tokens[index - 1], token),
                agent_style,
            ));
        }
        if let ResolvedToken::Workspace(value) = token {
            spans.extend(agent_panel_title_spans(
                value,
                None,
                name_style,
                Style::default().fg(app.palette.overlay1),
            ));
            continue;
        }
        let (text, style) = match token {
            ResolvedToken::StateIcon => {
                let (icon, style) = state_icon(
                    detail.state,
                    detail.seen,
                    app.spinner_tick,
                    app.status_indicators,
                    &app.palette,
                );
                (icon.to_string(), style)
            }
            ResolvedToken::StateText(value) => (
                value.clone(),
                Style::default().fg(state_label_color(detail.state, detail.seen, &app.palette)),
            ),
            ResolvedToken::Pane(value) => (value.clone(), name_style),
            ResolvedToken::Tab(value) | ResolvedToken::Agent(value) => (value.clone(), agent_style),
            ResolvedToken::TerminalTitle(value)
            | ResolvedToken::Custom(value)
            | ResolvedToken::Branch(value) => (value.clone(), agent_style),
            ResolvedToken::GitStatus { ahead, behind } => {
                (format!("↑{ahead} ↓{behind}"), agent_style)
            }
            ResolvedToken::Workspace(_) => continue,
        };
        spans.push(Span::styled(text, style));
    }
    Line::from(spans)
}

fn agent_panel_section_header_style(section: &AgentPanelSection, p: &Palette) -> Style {
    agent_section_style(section.group, p)
}

fn agent_panel_section_icon(
    section: &AgentPanelSection,
    tick: u32,
    style: crate::config::StatusIndicatorStyle,
    p: &Palette,
) -> (&'static str, Style) {
    agent_section_icon(section.group, tick, style, p)
}

fn right_entry_detail_prefix(_p: &Palette) -> Vec<Span<'static>> {
    vec![Span::styled(
        " ".repeat(RIGHT_ENTRY_PRIMARY_COL as usize),
        Style::default(),
    )]
}

fn workspace_metadata_tokens(
    app: &AppState,
    ws: &crate::workspace::Workspace,
) -> std::collections::HashMap<String, String> {
    let mut tokens = std::collections::HashMap::new();
    for tab in &ws.tabs {
        for pane in tab.panes.values() {
            if let Some(terminal) = app.terminals.get(&pane.attached_terminal_id) {
                tokens.extend(terminal.effective_presentation().tokens);
            }
        }
    }
    tokens
}

fn resolved_space_rows(
    app: &AppState,
    ws: &crate::workspace::Workspace,
    workspace: &str,
) -> Vec<Vec<ResolvedToken>> {
    let (state, seen) = ws.aggregate_state(&app.terminals);
    space_rows(
        &app.sidebar_config.spaces,
        SpaceTokenContext {
            workspace,
            branch: ws.cached_git_branch.as_deref(),
            state_text: state_label(state, seen),
            ahead_behind: ws.cached_git_ahead_behind,
            tokens: &workspace_metadata_tokens(app, ws),
            suppress_git_details: false,
        },
    )
}
fn workspace_host_badge(
    app: &AppState,
    ws: &crate::workspace::Workspace,
) -> Option<(String, ratatui::style::Color)> {
    let hosts = ws
        .tabs
        .iter()
        .flat_map(|tab| tab.panes.values())
        .filter_map(|pane| app.terminals.get(&pane.attached_terminal_id))
        .map(|terminal| terminal.location.execution_host_id.clone())
        .collect::<std::collections::HashSet<_>>();
    let host_ids = if hosts.is_empty() {
        vec![ws.default_location.execution_host_id.clone()]
    } else {
        hosts.into_iter().collect::<Vec<_>>()
    };
    let mut display_names = std::collections::BTreeSet::new();
    for host_id in &host_ids {
        display_names.insert(
            app.host_label(crate::app::host_label::HostLabelTarget::ExecutionHost(
                host_id,
            ))
            .to_string(),
        );
    }
    let name = display_names.into_iter().collect::<Vec<_>>().join(" · ");
    let remote = (host_ids.len() == 1)
        .then(|| &host_ids[0])
        .filter(|host| !host.is_local());
    let health = remote.and_then(|host| app.host_connection_states.get(host));
    let status = health.and_then(|status| match status {
        crate::execution_host::ConnectionStatus::Disconnected => Some("Offline"),
        crate::execution_host::ConnectionStatus::Reconnecting { .. } => Some("Lost"),
        crate::execution_host::ConnectionStatus::AuthenticationRequired => Some("Unavailable"),
        _ => None,
    });
    let label = match status {
        Some(status) => format!("{name} · {status}"),
        None => name,
    };
    let color = if status.is_some() {
        app.palette.yellow
    } else {
        app.palette.overlay0
    };
    Some((label, color))
}

fn workspace_has_metadata(ws: &crate::workspace::Workspace) -> bool {
    ws.cached_git_work_summary.is_some_and(|summary| {
        summary.conflicted + summary.added + summary.modified + summary.deleted > 0
            || summary.repo_count > 1
    })
}

fn workspace_token_line(
    tokens: &[ResolvedToken],
    state: AgentState,
    seen: bool,
    tick: u32,
    indicator_style: crate::config::StatusIndicatorStyle,
    p: &Palette,
    name_style: Style,
) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(
                separator(&tokens[index - 1], token),
                Style::default().fg(p.overlay0),
            ));
        }
        let (text, style) = match token {
            ResolvedToken::StateIcon => {
                let (icon, style) = state_icon(state, seen, tick, indicator_style, p);
                (icon.to_string(), style)
            }
            ResolvedToken::StateText(value) => (
                value.clone(),
                Style::default().fg(state_label_color(state, seen, p)),
            ),
            ResolvedToken::Workspace(value) => (value.clone(), name_style),
            ResolvedToken::Branch(value) | ResolvedToken::Custom(value) => {
                (value.clone(), Style::default().fg(p.overlay0))
            }
            ResolvedToken::GitStatus { ahead, behind } => (
                format!("↑{ahead} ↓{behind}"),
                Style::default().fg(p.overlay0),
            ),
            ResolvedToken::Tab(value)
            | ResolvedToken::Pane(value)
            | ResolvedToken::Agent(value)
            | ResolvedToken::TerminalTitle(value) => {
                (value.clone(), Style::default().fg(p.overlay0))
            }
        };
        spans.push(Span::styled(text, style));
    }
    Line::from(spans)
}

fn render_workspace_token_rows(
    app: &AppState,
    frame: &mut Frame,
    ws: &crate::workspace::Workspace,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
    row_y: u16,
    row_height: u16,
    name_style: Style,
) {
    let workspace = ws.display_name_from(&app.terminals, terminal_runtimes);
    let (state, seen) = ws.aggregate_state(&app.terminals);
    let rows = resolved_space_rows(app, ws, &workspace);
    for (index, row) in rows.iter().take(row_height as usize).enumerate() {
        let prefix_width = if matches!(row.first(), Some(ResolvedToken::StateIcon)) {
            SIDEBAR_WORKSPACE_STATE_COL
        } else {
            SIDEBAR_WORKSPACE_NAME_COL
        };
        let mut spans = vec![Span::styled(
            " ".repeat(prefix_width as usize),
            Style::default(),
        )];
        spans.extend(
            workspace_token_line(
                row,
                state,
                seen,
                app.spinner_tick,
                app.status_indicators,
                &app.palette,
                name_style,
            )
            .spans,
        );
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, row_y.saturating_add(index as u16), area.width, 1),
        );
    }
    if rows.len() == 1
        && row_height > 1
        && app.sidebar_config.spaces == crate::config::SpacesSidebarConfig::default()
    {
        let max_summary_len =
            (area.width as usize).saturating_sub(SIDEBAR_WORKSPACE_NAME_COL as usize);
        let mut spans = vec![Span::styled(
            " ".repeat(SIDEBAR_WORKSPACE_NAME_COL as usize),
            Style::default(),
        )];
        let host_badge = workspace_host_badge(app, ws);
        if let Some((label, color)) = host_badge.as_ref() {
            spans.push(summary_span(label, *color, max_summary_len));
        }
        let used = host_badge.as_ref().map_or(0, |(label, _)| label.len());
        let summary_width = max_summary_len.saturating_sub(used);
        let summary = workspace_summary_spans(ws, &app.palette, summary_width);
        if !summary.is_empty() && host_badge.is_some() {
            spans.push(Span::styled(
                " · ",
                Style::default().fg(app.palette.overlay0),
            ));
        }
        spans.extend(summary);
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, row_y + 1, area.width, 1),
        );
    }
}

fn workspace_row_height(app: &AppState, ws: &crate::workspace::Workspace) -> u16 {
    let configured = resolved_space_rows(app, ws, &ws.display_name()).len();
    let legacy = if app.sidebar_config.spaces == crate::config::SpacesSidebarConfig::default()
        && (workspace_has_metadata(ws) || workspace_host_badge(app, ws).is_some())
    {
        2
    } else {
        1
    };
    configured.max(legacy) as u16
}

pub(crate) fn workspace_list_rect(area: Rect, split_ratio: f32) -> Rect {
    let (ws_area, _) = expanded_sidebar_sections(area, split_ratio);
    ws_area
}

pub(crate) fn right_aligned_workspace_list_rect(area: Rect, split_ratio: f32) -> Rect {
    let (ws_area, _) = right_aligned_expanded_sidebar_sections(area, split_ratio);
    ws_area
}

pub(crate) fn left_sidebar_workspace_rect(area: Rect) -> Rect {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height == 0 {
        return Rect::default();
    }
    content
}

pub(crate) fn right_sidebar_content_rect(area: Rect) -> Rect {
    if area.width <= 1 || area.height == 0 {
        return Rect::default();
    }
    Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(1),
        area.height,
    )
}

pub(crate) fn workspace_list_body_rect(area: Rect, has_scrollbar: bool) -> Rect {
    if area.width == 0 || area.height <= WORKSPACE_SECTION_HEADER_ROWS {
        return Rect::default();
    }

    let body_y = area.y.saturating_add(WORKSPACE_SECTION_HEADER_ROWS);
    let footer_y = area.y + area.height.saturating_sub(1);
    let body_height = footer_y.saturating_sub(body_y);
    let body_width = area.width.saturating_sub(u16::from(has_scrollbar));
    Rect::new(area.x, body_y, body_width, body_height)
}

fn workspace_list_visible_count(app: &AppState, area: Rect, scroll: usize) -> usize {
    let body = workspace_list_body_rect(area, false);
    if body.width == 0 || body.height == 0 {
        return 0;
    }

    let mut used_rows = 0u16;
    let mut visible = 0usize;
    for entry in workspace_list_entries(app).into_iter().skip(scroll) {
        let needed = entry.row_height(app);
        if used_rows.saturating_add(needed) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(needed);
        visible += 1;
    }
    visible
}
fn workspace_list_visible_count_for_view(
    app: &AppState,
    view: &ClientViewState,
    area: Rect,
    scroll: usize,
) -> usize {
    let body = workspace_list_body_rect(area, false);
    if body.width == 0 || body.height == 0 {
        return 0;
    }

    let mut used_rows = 0u16;
    let mut visible = 0usize;
    for entry in workspace_list_entries_for_view(app, view)
        .into_iter()
        .skip(scroll)
    {
        let needed = entry.row_height_for_workspaces(app);
        if used_rows.saturating_add(needed) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(needed);
        visible += 1;
    }
    visible
}

pub(crate) fn workspace_list_scroll_metrics(
    app: &AppState,
    area: Rect,
) -> crate::pane::ScrollMetrics {
    let viewport_rows = workspace_list_visible_count(app, area, app.workspace_scroll);
    let total_rows = workspace_list_entries(app).len();
    let max_offset_from_bottom = total_rows.saturating_sub(viewport_rows);
    let offset_from_bottom = total_rows
        .saturating_sub(app.workspace_scroll)
        .saturating_sub(viewport_rows);

    crate::pane::ScrollMetrics {
        offset_from_bottom,
        max_offset_from_bottom,
        viewport_rows,
    }
}

pub(crate) fn workspace_list_scroll_metrics_for_view(
    app: &AppState,
    view: &ClientViewState,
    area: Rect,
) -> crate::pane::ScrollMetrics {
    let viewport_rows =
        workspace_list_visible_count_for_view(app, view, area, view.workspace_scroll);
    let total_rows = workspace_list_entries_for_view(app, view).len();
    let max_offset_from_bottom = total_rows.saturating_sub(viewport_rows);
    let offset_from_bottom = total_rows
        .saturating_sub(view.workspace_scroll)
        .saturating_sub(viewport_rows);

    crate::pane::ScrollMetrics {
        offset_from_bottom,
        max_offset_from_bottom,
        viewport_rows,
    }
}

pub(crate) fn workspace_list_scrollbar_rect(app: &AppState, area: Rect) -> Option<Rect> {
    let metrics = workspace_list_scroll_metrics(app, area);
    let body = workspace_list_body_rect(area, true);
    (should_show_scrollbar(metrics) && body.width > 0 && body.height > 0).then_some(Rect::new(
        area.x + area.width.saturating_sub(1),
        body.y,
        1,
        body.height,
    ))
}

pub(crate) fn workspace_list_scrollbar_rect_for_view(
    app: &AppState,
    view: &ClientViewState,
    area: Rect,
) -> Option<Rect> {
    let metrics = workspace_list_scroll_metrics_for_view(app, view, area);
    let body = workspace_list_body_rect(area, true);
    (should_show_scrollbar(metrics) && body.width > 0 && body.height > 0).then_some(Rect::new(
        area.x + area.width.saturating_sub(1),
        body.y,
        1,
        body.height,
    ))
}

pub(crate) fn agent_panel_body_rect(
    area: Rect,
    has_scrollbar: bool,
    _leading_separator: bool,
) -> Rect {
    if area.width == 0 || area.height <= AGENT_PANEL_HEADER_ROWS {
        return Rect::default();
    }

    let body_y = area.y.saturating_add(AGENT_PANEL_HEADER_ROWS);
    let footer_y = area.y + area.height.saturating_sub(1);
    let body_height = footer_y.saturating_sub(body_y);
    let body_width = area.width.saturating_sub(u16::from(has_scrollbar));
    Rect::new(area.x, body_y, body_width, body_height)
}

fn agent_panel_visible_count(app: &AppState, area: Rect, leading_separator: bool) -> usize {
    let body = agent_panel_body_rect(area, false, leading_separator);
    if body.width == 0 || body.height == 0 {
        return 0;
    }

    let sections = agent_panel_sections(app);
    let show_agent_labels = agent_panel_should_show_agent_labels(&sections);
    let mut remaining_rows = body.height;
    let mut visible = 0usize;
    let mut skip = app.agent_panel_scroll;
    for section in sections {
        if agent_panel_section_collapsed(app, section.group) {
            if remaining_rows < 1 {
                break;
            }
            remaining_rows = remaining_rows.saturating_sub(1);
            continue;
        }
        let item_count = agent_panel_section_item_count(&section);
        if item_count > 0 && skip >= item_count {
            skip -= item_count;
            continue;
        }
        if remaining_rows < 1 {
            break;
        }

        remaining_rows = remaining_rows.saturating_sub(1);
        if agent_panel_empty_row(&section).is_some() {
            if remaining_rows < 1 {
                break;
            }
            remaining_rows = remaining_rows.saturating_sub(1);
            visible += 1;
        }
        let show_status = agent_panel_section_shows_entry_status(section.group);
        for detail in section.entries.iter().skip(skip) {
            let row_height =
                agent_panel_entry_row_height(app, show_status, show_agent_labels, detail);
            if remaining_rows < row_height {
                break;
            }
            remaining_rows = remaining_rows.saturating_sub(row_height);
            visible += 1;
        }
        skip = 0;
    }
    visible
}

pub(crate) fn agent_panel_scroll_metrics(
    app: &AppState,
    area: Rect,
    leading_separator: bool,
) -> crate::pane::ScrollMetrics {
    let viewport_rows = agent_panel_visible_count(app, area, leading_separator);
    let total_rows = agent_panel_sections(app)
        .iter()
        .filter(|section| !agent_panel_section_collapsed(app, section.group))
        .map(agent_panel_section_item_count)
        .sum::<usize>();
    let max_offset_from_bottom = total_rows.saturating_sub(viewport_rows);
    let offset_from_bottom = total_rows
        .saturating_sub(app.agent_panel_scroll)
        .saturating_sub(viewport_rows);

    crate::pane::ScrollMetrics {
        offset_from_bottom,
        max_offset_from_bottom,
        viewport_rows,
    }
}

pub(crate) fn agent_panel_scroll_metrics_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    area: Rect,
    leading_separator: bool,
) -> crate::pane::ScrollMetrics {
    let body = agent_panel_body_rect(area, false, leading_separator);
    let sections = agent_panel_sections_for_view(app, terminal_runtimes, view);
    let show_agent_labels = agent_panel_should_show_agent_labels(&sections);
    let mut remaining_rows = body.height;
    let mut viewport_rows = 0usize;
    let mut skip = view.agent_panel_scroll;
    if body.width > 0 && body.height > 0 {
        for section in &sections {
            if agent_panel_section_collapsed_for_view(view, section.group) {
                if remaining_rows < 1 {
                    break;
                }
                remaining_rows = remaining_rows.saturating_sub(1);
                continue;
            }
            let item_count = agent_panel_section_item_count(section);
            if item_count > 0 && skip >= item_count {
                skip -= item_count;
                continue;
            }
            if remaining_rows < 1 {
                break;
            }

            remaining_rows = remaining_rows.saturating_sub(1);
            if agent_panel_empty_row(section).is_some() {
                if remaining_rows < 1 {
                    break;
                }
                remaining_rows = remaining_rows.saturating_sub(1);
                viewport_rows += 1;
            }
            let show_status = agent_panel_section_shows_entry_status(section.group);
            for detail in section.entries.iter().skip(skip) {
                let row_height =
                    agent_panel_entry_row_height(app, show_status, show_agent_labels, detail);
                if remaining_rows < row_height {
                    break;
                }
                remaining_rows = remaining_rows.saturating_sub(row_height);
                viewport_rows += 1;
            }
            skip = 0;
        }
    }

    let total_rows = sections
        .iter()
        .filter(|section| !agent_panel_section_collapsed_for_view(view, section.group))
        .map(agent_panel_section_item_count)
        .sum::<usize>();
    let max_offset_from_bottom = total_rows.saturating_sub(viewport_rows);
    let offset_from_bottom = total_rows
        .saturating_sub(view.agent_panel_scroll)
        .saturating_sub(viewport_rows);

    crate::pane::ScrollMetrics {
        offset_from_bottom,
        max_offset_from_bottom,
        viewport_rows,
    }
}
pub(crate) fn agent_panel_scrollbar_rect(
    app: &AppState,
    area: Rect,
    leading_separator: bool,
) -> Option<Rect> {
    let metrics = agent_panel_scroll_metrics(app, area, leading_separator);
    let body = agent_panel_body_rect(area, true, leading_separator);
    (should_show_scrollbar(metrics) && body.width > 0 && body.height > 0).then_some(Rect::new(
        area.x + area.width.saturating_sub(1),
        body.y,
        1,
        body.height,
    ))
}

pub(crate) fn compute_workspace_card_areas(
    app: &AppState,
    area: Rect,
) -> Vec<crate::app::state::WorkspaceCardArea> {
    let ws_area = workspace_list_rect(area, app.sidebar_section_split);
    compute_workspace_card_areas_in_list(app, ws_area)
}

pub(crate) fn compute_workspace_group_header_areas(
    app: &AppState,
    area: Rect,
) -> Vec<crate::app::state::WorkspaceGroupHeaderArea> {
    let ws_area = workspace_list_rect(area, app.sidebar_section_split);
    compute_workspace_group_header_areas_in_list(app, ws_area)
}

pub(crate) fn compute_workspace_card_areas_in_list(
    app: &AppState,
    ws_area: Rect,
) -> Vec<crate::app::state::WorkspaceCardArea> {
    compute_workspace_list_areas_in_list(app, ws_area).0
}

pub(crate) fn compute_workspace_group_header_areas_in_list(
    app: &AppState,
    ws_area: Rect,
) -> Vec<crate::app::state::WorkspaceGroupHeaderArea> {
    compute_workspace_list_areas_in_list(app, ws_area).1
}

pub(crate) fn compute_workspace_group_empty_areas_in_list(
    app: &AppState,
    ws_area: Rect,
) -> Vec<crate::app::state::WorkspaceGroupEmptyArea> {
    compute_workspace_list_areas_in_list(app, ws_area).2
}

pub(crate) fn compute_workspace_group_drop_areas_in_list(
    app: &AppState,
    ws_area: Rect,
) -> Vec<crate::app::state::WorkspaceGroupDropArea> {
    compute_workspace_list_areas_in_list(app, ws_area).3
}
pub(crate) fn compute_workspace_card_areas_in_list_for_view(
    app: &AppState,
    view: &ClientViewState,
    ws_area: Rect,
) -> Vec<crate::app::state::WorkspaceCardArea> {
    compute_workspace_list_areas_in_list_for_view(app, view, ws_area).0
}

pub(crate) fn compute_workspace_group_header_areas_in_list_for_view(
    app: &AppState,
    view: &ClientViewState,
    ws_area: Rect,
) -> Vec<crate::app::state::WorkspaceGroupHeaderArea> {
    compute_workspace_list_areas_in_list_for_view(app, view, ws_area).1
}

pub(crate) fn compute_workspace_group_empty_areas_in_list_for_view(
    app: &AppState,
    view: &ClientViewState,
    ws_area: Rect,
) -> Vec<crate::app::state::WorkspaceGroupEmptyArea> {
    compute_workspace_list_areas_in_list_for_view(app, view, ws_area).2
}

pub(crate) fn workspace_list_entry_count_for_view(app: &AppState, view: &ClientViewState) -> usize {
    workspace_list_entries_for_view(app, view).len()
}

pub(crate) fn workspace_list_entry_count(app: &AppState) -> usize {
    workspace_list_entries(app).len()
}

pub(crate) fn workspace_list_position_for_workspace(
    app: &AppState,
    ws_idx: usize,
) -> Option<usize> {
    workspace_list_entries(app).iter().position(
        |entry| matches!(entry, WorkspaceListEntry::Workspace { ws_idx: idx, .. } if *idx == ws_idx),
    )
}

fn compute_workspace_list_areas_in_list(
    app: &AppState,
    ws_area: Rect,
) -> (
    Vec<crate::app::state::WorkspaceCardArea>,
    Vec<crate::app::state::WorkspaceGroupHeaderArea>,
    Vec<crate::app::state::WorkspaceGroupEmptyArea>,
    Vec<crate::app::state::WorkspaceGroupDropArea>,
) {
    if ws_area == Rect::default() {
        return (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    }

    let metrics = workspace_list_scroll_metrics(app, ws_area);
    let body = workspace_list_body_rect(ws_area, should_show_scrollbar(metrics));
    if body.width == 0 || body.height == 0 {
        return (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    }

    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut cards = Vec::new();
    let mut headers = Vec::new();
    let mut empties = Vec::new();
    let mut drops = Vec::new();

    let entries = workspace_list_entries(app);
    for entry in entries.iter().copied().skip(app.workspace_scroll) {
        let row_height = entry.row_height(app);
        if row_y.saturating_add(row_height) > body_bottom {
            break;
        }

        match entry {
            WorkspaceListEntry::GroupGap => {}
            WorkspaceListEntry::GroupHeader { group_idx } => {
                headers.push(crate::app::state::WorkspaceGroupHeaderArea {
                    group_idx,
                    rect: Rect::new(body.x, row_y, body.width, 1),
                });
            }
            WorkspaceListEntry::EmptyGroup { group_idx } => {
                empties.push(crate::app::state::WorkspaceGroupEmptyArea {
                    group_idx,
                    rect: Rect::new(body.x, row_y, body.width, 1),
                });
            }
            WorkspaceListEntry::Workspace { ws_idx, group_idx } => {
                let Some(ws) = app.workspaces.get(ws_idx) else {
                    continue;
                };
                let row_height = workspace_row_height(app, ws);
                cards.push(crate::app::state::WorkspaceCardArea {
                    ws_idx,
                    rect: Rect::new(body.x, row_y, body.width, row_height),
                });
                if let Some(group_idx) = group_idx {
                    let group_is_seen =
                        drops
                            .iter()
                            .any(|drop: &crate::app::state::WorkspaceGroupDropArea| {
                                drop.group_idx == group_idx
                            });
                    if !group_is_seen {
                        drops.push(crate::app::state::WorkspaceGroupDropArea {
                            group_idx,
                            insert_idx: ws_idx,
                            rect: Rect::new(body.x, row_y, body.width, 1),
                        });
                    }
                    drops.push(crate::app::state::WorkspaceGroupDropArea {
                        group_idx,
                        insert_idx: ws_idx + 1,
                        rect: Rect::new(
                            body.x,
                            workspace_after_drop_row(row_y, row_height, body_bottom),
                            body.width,
                            1,
                        ),
                    });
                }
            }
        }
        row_y = row_y.saturating_add(row_height);
    }

    (cards, headers, empties, drops)
}

fn compute_workspace_list_areas_in_list_for_view(
    app: &AppState,
    view: &ClientViewState,
    ws_area: Rect,
) -> (
    Vec<crate::app::state::WorkspaceCardArea>,
    Vec<crate::app::state::WorkspaceGroupHeaderArea>,
    Vec<crate::app::state::WorkspaceGroupEmptyArea>,
    Vec<crate::app::state::WorkspaceGroupDropArea>,
) {
    if ws_area == Rect::default() {
        return (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    }

    let metrics = workspace_list_scroll_metrics_for_view(app, view, ws_area);
    let body = workspace_list_body_rect(ws_area, should_show_scrollbar(metrics));
    if body.width == 0 || body.height == 0 {
        return (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    }

    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut cards = Vec::new();
    let mut headers = Vec::new();
    let mut empties = Vec::new();
    let mut drops = Vec::new();

    let entries = workspace_list_entries_for_view(app, view);
    for entry in entries.iter().copied().skip(view.workspace_scroll) {
        let row_height = entry.row_height_for_workspaces(app);
        if row_y.saturating_add(row_height) > body_bottom {
            break;
        }

        match entry {
            WorkspaceListEntry::GroupGap => {}
            WorkspaceListEntry::GroupHeader { group_idx } => {
                headers.push(crate::app::state::WorkspaceGroupHeaderArea {
                    group_idx,
                    rect: Rect::new(body.x, row_y, body.width, 1),
                });
            }
            WorkspaceListEntry::EmptyGroup { group_idx } => {
                empties.push(crate::app::state::WorkspaceGroupEmptyArea {
                    group_idx,
                    rect: Rect::new(body.x, row_y, body.width, 1),
                });
            }
            WorkspaceListEntry::Workspace { ws_idx, group_idx } => {
                let Some(ws) = app.workspaces.get(ws_idx) else {
                    continue;
                };
                let row_height = workspace_row_height(app, ws);
                cards.push(crate::app::state::WorkspaceCardArea {
                    ws_idx,
                    rect: Rect::new(body.x, row_y, body.width, row_height),
                });
                if let Some(group_idx) = group_idx {
                    let group_is_seen =
                        drops
                            .iter()
                            .any(|drop: &crate::app::state::WorkspaceGroupDropArea| {
                                drop.group_idx == group_idx
                            });
                    if !group_is_seen {
                        drops.push(crate::app::state::WorkspaceGroupDropArea {
                            group_idx,
                            insert_idx: ws_idx,
                            rect: Rect::new(body.x, row_y, body.width, 1),
                        });
                    }
                    drops.push(crate::app::state::WorkspaceGroupDropArea {
                        group_idx,
                        insert_idx: ws_idx + 1,
                        rect: Rect::new(
                            body.x,
                            workspace_after_drop_row(row_y, row_height, body_bottom),
                            body.width,
                            1,
                        ),
                    });
                }
            }
        }
        row_y = row_y.saturating_add(row_height);
    }

    (cards, headers, empties, drops)
}

fn workspace_after_drop_row(row_y: u16, row_height: u16, body_bottom: u16) -> u16 {
    row_y
        .saturating_add(row_height)
        .min(body_bottom.saturating_sub(1))
}

#[derive(Clone, Copy)]
enum WorkspaceListEntry {
    GroupHeader {
        group_idx: usize,
    },
    EmptyGroup {
        group_idx: usize,
    },
    GroupGap,
    Workspace {
        ws_idx: usize,
        group_idx: Option<usize>,
    },
}

impl WorkspaceListEntry {
    fn row_height(self, app: &AppState) -> u16 {
        self.row_height_for_workspaces(app)
    }

    fn row_height_for_workspaces(self, app: &AppState) -> u16 {
        match self {
            Self::GroupHeader { .. } | Self::EmptyGroup { .. } | Self::GroupGap => 1,
            Self::Workspace { ws_idx, .. } => app
                .workspaces
                .get(ws_idx)
                .map(|ws| workspace_row_height(app, ws))
                .unwrap_or(0),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CollapsedWorkspaceRowEntry {
    GroupHeader { group_idx: usize },
    Workspace { ws_idx: usize, ordinal: usize },
}

pub(crate) fn collapsed_workspace_row_entries(app: &AppState) -> Vec<CollapsedWorkspaceRowEntry> {
    if app.group_filter_enabled {
        return app
            .visible_workspace_indices()
            .into_iter()
            .enumerate()
            .map(|(idx, ws_idx)| CollapsedWorkspaceRowEntry::Workspace {
                ws_idx,
                ordinal: idx + 1,
            })
            .collect();
    }

    let mut entries = Vec::new();
    for (group_idx, group) in app.groups.iter().enumerate() {
        entries.push(CollapsedWorkspaceRowEntry::GroupHeader { group_idx });
        if app.workspace_group_collapsed(&group.id) {
            continue;
        }

        let mut ordinal = 1;
        for (ws_idx, ws) in app.workspaces.iter().enumerate() {
            if ws.group_id == group.id {
                entries.push(CollapsedWorkspaceRowEntry::Workspace { ws_idx, ordinal });
                ordinal += 1;
            }
        }
    }
    entries
}
fn collapsed_workspace_row_entries_for_view(
    app: &AppState,
    view: &ClientViewState,
) -> Vec<CollapsedWorkspaceRowEntry> {
    if view.group_filter_enabled {
        return visible_workspace_indices_for_view(app, view)
            .into_iter()
            .enumerate()
            .map(|(idx, ws_idx)| CollapsedWorkspaceRowEntry::Workspace {
                ws_idx,
                ordinal: idx + 1,
            })
            .collect();
    }

    let mut entries = Vec::new();
    for (group_idx, group) in app.groups.iter().enumerate() {
        entries.push(CollapsedWorkspaceRowEntry::GroupHeader { group_idx });
        if workspace_group_collapsed_for_view(view, &group.id) {
            continue;
        }

        let mut ordinal = 1;
        for (ws_idx, ws) in app.workspaces.iter().enumerate() {
            if ws.group_id == group.id {
                entries.push(CollapsedWorkspaceRowEntry::Workspace { ws_idx, ordinal });
                ordinal += 1;
            }
        }
    }
    entries
}

pub(crate) fn collapsed_workspace_at_row(app: &AppState, area: Rect, row: u16) -> Option<usize> {
    match collapsed_workspace_row_entry_at(
        app,
        area,
        row,
        app.view.right_sidebar_rect == Rect::default(),
    )? {
        CollapsedWorkspaceRowEntry::Workspace { ws_idx, .. } => Some(ws_idx),
        CollapsedWorkspaceRowEntry::GroupHeader { .. } => None,
    }
}

pub(crate) fn collapsed_workspace_group_header_at_row(
    app: &AppState,
    area: Rect,
    row: u16,
) -> Option<usize> {
    match collapsed_workspace_row_entry_at(
        app,
        area,
        row,
        app.view.right_sidebar_rect == Rect::default(),
    )? {
        CollapsedWorkspaceRowEntry::GroupHeader { group_idx } => Some(group_idx),
        CollapsedWorkspaceRowEntry::Workspace { .. } => None,
    }
}

fn collapsed_workspace_row_entry_at(
    app: &AppState,
    area: Rect,
    row: u16,
    show_agent_detail: bool,
) -> Option<CollapsedWorkspaceRowEntry> {
    let rows =
        collapsed_workspace_rows_rect_for_split(area, show_agent_detail, app.sidebar_section_split);
    if rows == Rect::default() || row < rows.y || row >= rows.y + rows.height {
        return None;
    }
    collapsed_workspace_row_entries(app)
        .get((row - rows.y) as usize)
        .copied()
}

pub(crate) fn collapsed_workspace_row_entry_at_for_view(
    app: &AppState,
    view: &ClientViewState,
    area: Rect,
    row: u16,
) -> Option<CollapsedWorkspaceRowEntry> {
    let show_agent_detail = view.computed.right_sidebar_rect == Rect::default();
    let rows = collapsed_workspace_rows_rect_for_split(
        area,
        show_agent_detail,
        view.sidebar_section_split,
    );
    if rows == Rect::default() || row < rows.y || row >= rows.y + rows.height {
        return None;
    }
    collapsed_workspace_row_entries_for_view(app, view)
        .get((row - rows.y) as usize)
        .copied()
}

fn workspace_list_entries(app: &AppState) -> Vec<WorkspaceListEntry> {
    if app.sidebar_collapsed || app.group_filter_enabled {
        return app
            .visible_workspace_indices()
            .into_iter()
            .map(|ws_idx| WorkspaceListEntry::Workspace {
                ws_idx,
                group_idx: None,
            })
            .collect();
    }

    let mut entries = Vec::new();
    for (group_idx, group) in app.groups.iter().enumerate() {
        if group_idx > 0 {
            entries.push(WorkspaceListEntry::GroupGap);
        }
        entries.push(WorkspaceListEntry::GroupHeader { group_idx });
        let group_workspaces = app
            .workspaces
            .iter()
            .enumerate()
            .filter_map(|(ws_idx, ws)| (ws.group_id == group.id).then_some(ws_idx))
            .collect::<Vec<_>>();
        if !app.workspace_group_collapsed(&group.id) {
            if group_workspaces.is_empty() {
                entries.push(WorkspaceListEntry::EmptyGroup { group_idx });
            } else {
                for ws_idx in group_workspaces {
                    entries.push(WorkspaceListEntry::Workspace {
                        ws_idx,
                        group_idx: Some(group_idx),
                    });
                }
            }
        }
    }
    entries
}

fn visible_workspace_indices_for_view(app: &AppState, view: &ClientViewState) -> Vec<usize> {
    if !view.group_filter_enabled {
        return (0..app.workspaces.len()).collect();
    }

    let Some(group) = app.groups.get(view.active_group) else {
        return Vec::new();
    };

    app.workspaces
        .iter()
        .enumerate()
        .filter_map(|(idx, workspace)| (workspace.group_id == group.id).then_some(idx))
        .collect()
}

fn workspace_group_collapsed_for_view(view: &ClientViewState, group_id: &str) -> bool {
    view.collapsed_workspace_groups
        .iter()
        .any(|id| id == group_id)
}

fn workspace_list_entries_for_view(
    app: &AppState,
    view: &ClientViewState,
) -> Vec<WorkspaceListEntry> {
    if view.sidebar_collapsed || view.group_filter_enabled {
        return visible_workspace_indices_for_view(app, view)
            .into_iter()
            .map(|ws_idx| WorkspaceListEntry::Workspace {
                ws_idx,
                group_idx: None,
            })
            .collect();
    }

    let mut entries = Vec::new();
    for (group_idx, group) in app.groups.iter().enumerate() {
        if group_idx > 0 {
            entries.push(WorkspaceListEntry::GroupGap);
        }
        entries.push(WorkspaceListEntry::GroupHeader { group_idx });
        let group_workspaces = app
            .workspaces
            .iter()
            .enumerate()
            .filter_map(|(ws_idx, ws)| (ws.group_id == group.id).then_some(ws_idx))
            .collect::<Vec<_>>();
        if !workspace_group_collapsed_for_view(view, &group.id) {
            if group_workspaces.is_empty() {
                entries.push(WorkspaceListEntry::EmptyGroup { group_idx });
            } else {
                for ws_idx in group_workspaces {
                    entries.push(WorkspaceListEntry::Workspace {
                        ws_idx,
                        group_idx: Some(group_idx),
                    });
                }
            }
        }
    }
    entries
}

/// Auto-scale sidebar width based on workspace identity + agent summary.
#[cfg(test)]
pub(crate) fn collapsed_sidebar_sections(
    area: Rect,
    show_agent_detail: bool,
) -> (Rect, Option<u16>, Rect) {
    collapsed_sidebar_sections_for_split(area, show_agent_detail, 0.5)
}

pub(crate) fn collapsed_sidebar_sections_for_split(
    area: Rect,
    show_agent_detail: bool,
    split_ratio: f32,
) -> (Rect, Option<u16>, Rect) {
    collapsed_sidebar_sections_with_separator(area, show_agent_detail, false, split_ratio)
}

fn right_aligned_collapsed_sidebar_sections(
    area: Rect,
    show_agent_detail: bool,
    split_ratio: f32,
) -> (Rect, Option<u16>, Rect) {
    collapsed_sidebar_sections_with_separator(area, show_agent_detail, true, split_ratio)
}

fn collapsed_sidebar_sections_with_separator(
    area: Rect,
    show_agent_detail: bool,
    separator_on_left: bool,
    split_ratio: f32,
) -> (Rect, Option<u16>, Rect) {
    let content = sidebar_content_rect(area, separator_on_left);
    if content == Rect::default() {
        return (Rect::default(), None, Rect::default());
    }

    if !show_agent_detail {
        return (content, None, Rect::default());
    }

    if content.height < 6 {
        return (content, None, Rect::default());
    }

    let (ws_h, detail_h) = sidebar_section_heights(content.height, split_ratio);
    if ws_h == 0 || detail_h == 0 {
        return (content, None, Rect::default());
    }

    let ws_area = Rect::new(content.x, content.y, content.width, ws_h);
    let detail_area = Rect::new(content.x, content.y + ws_h, content.width, detail_h);
    (ws_area, None, detail_area)
}

pub(crate) fn collapsed_group_header_rect(area: Rect) -> Rect {
    collapsed_group_header_rect_with_separator(area, false)
}

fn right_aligned_collapsed_group_header_rect(area: Rect) -> Rect {
    collapsed_group_header_rect_with_separator(area, true)
}

fn collapsed_group_header_rect_with_separator(area: Rect, separator_on_left: bool) -> Rect {
    let content = sidebar_content_rect(area, separator_on_left);
    if content == Rect::default() {
        return Rect::default();
    }
    Rect::new(content.x, content.y, content.width, 1)
}

#[cfg(test)]
pub(crate) fn collapsed_workspace_rows_rect(area: Rect, show_agent_detail: bool) -> Rect {
    collapsed_workspace_rows_rect_for_split(area, show_agent_detail, 0.5)
}

fn collapsed_workspace_rows_rect_for_split(
    area: Rect,
    show_agent_detail: bool,
    split_ratio: f32,
) -> Rect {
    let (ws_area, _, _) =
        collapsed_sidebar_sections_for_split(area, show_agent_detail, split_ratio);
    if ws_area == Rect::default() || ws_area.height <= COLLAPSED_SECTION_HEADER_ROWS {
        return Rect::default();
    }
    Rect::new(
        ws_area.x,
        ws_area.y + COLLAPSED_SECTION_HEADER_ROWS,
        ws_area.width,
        ws_area.height.saturating_sub(COLLAPSED_SECTION_HEADER_ROWS),
    )
}

fn collapsed_group_label(app: &AppState) -> String {
    if app.group_filter_enabled {
        app.active_group_icon().to_string()
    } else {
        "All".to_string()
    }
}

fn collapsed_agent_scope_label(app: &AppState) -> String {
    match app.agent_panel_scope {
        AgentPanelScope::AllWorkspaces => "All".to_string(),
        AgentPanelScope::CurrentGroup => "f:g".to_string(),
        AgentPanelScope::CurrentWorkspace => "f:s".to_string(),
    }
}

pub(crate) fn collapsed_agent_panel_toggle_rect(area: Rect) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect::default();
    }
    Rect::new(area.x, area.y, area.width, 1)
}

fn collapsed_agent_panel_body_rect(area: Rect) -> Rect {
    if area.width == 0 || area.height <= COLLAPSED_SECTION_HEADER_ROWS {
        return Rect::default();
    }
    Rect::new(
        area.x,
        area.y + COLLAPSED_SECTION_HEADER_ROWS,
        area.width,
        area.height.saturating_sub(COLLAPSED_SECTION_HEADER_ROWS),
    )
}

fn render_collapsed_agent_section_header(
    frame: &mut Frame,
    section: &AgentPanelSection,
    collapsed: bool,
    rows: Rect,
    row_y: u16,
    tick: u32,
    indicator_style: crate::config::StatusIndicatorStyle,
    p: &Palette,
) {
    let (icon, icon_style) = agent_panel_section_icon(section, tick, indicator_style, p);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                if collapsed { "▸ " } else { "▾ " },
                Style::default().fg(p.overlay0),
            ),
            Span::styled(icon, icon_style),
        ])),
        Rect::new(rows.x, row_y, rows.width, 1),
    );
}

fn agent_entry_is_active_for_view(
    app: &AppState,
    view: &ClientViewState,
    entry: &AgentPanelEntry,
) -> bool {
    view.active_workspace == Some(entry.ws_idx)
        && view.focused_pane_for_workspace(app, entry.ws_idx)
            == Some((entry.tab_idx, entry.pane_id))
}

fn render_collapsed_agent_entry(
    app: &AppState,
    frame: &mut Frame,
    entry: &AgentPanelEntry,
    ordinal: usize,
    scope: AgentPanelScope,
    is_active: bool,
    rows: Rect,
    row_y: u16,
    p: &Palette,
) {
    let num_style = if scope == AgentPanelScope::AllWorkspaces {
        entry
            .group_context_idx
            .map(|group_idx| Style::default().fg(app.group_accent_color(group_idx)))
            .unwrap_or_else(|| Style::default().fg(p.overlay0))
    } else {
        Style::default().fg(p.overlay0)
    };
    let row_style = if is_active {
        Style::default().bg(p.surface_dim)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(format!("  {ordinal}"), num_style)).style(row_style),
        Rect::new(rows.x, row_y, rows.width, 1),
    );
}

fn render_collapsed_agent_panel(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
    p: &Palette,
) {
    if area == Rect::default() {
        return;
    }

    let toggle_rect = collapsed_agent_panel_toggle_rect(area);
    if toggle_rect != Rect::default() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                collapsed_agent_scope_label(app),
                Style::default().fg(p.overlay1).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            toggle_rect,
        );
    }

    if area.height > 1 {
        let buf = frame.buffer_mut();
        let divider_y = area.y + 1;
        for x in area.x..area.x + area.width {
            buf[(x, divider_y)].set_symbol("─");
            buf[(x, divider_y)].set_style(Style::default().fg(p.overlay0));
        }
    }

    let rows = collapsed_agent_panel_body_rect(area);
    if rows == Rect::default() {
        return;
    }

    let mut row_y = rows.y;
    let mut ordinal = 1usize;
    for section in agent_panel_sections_from(app, terminal_runtimes) {
        if row_y >= rows.y + rows.height {
            return;
        }
        let collapsed = agent_panel_section_collapsed(app, section.group);
        render_collapsed_agent_section_header(
            frame,
            &section,
            collapsed,
            rows,
            row_y,
            app.spinner_tick,
            app.status_indicators,
            p,
        );
        row_y = row_y.saturating_add(1);
        for entry in &section.entries {
            if !collapsed {
                if row_y >= rows.y + rows.height {
                    return;
                }
                render_collapsed_agent_entry(
                    app,
                    frame,
                    entry,
                    ordinal,
                    app.agent_panel_scope,
                    app.is_active_pane(entry.ws_idx, entry.tab_idx, entry.pane_id),
                    rows,
                    row_y,
                    p,
                );
                row_y = row_y.saturating_add(1);
            }
            ordinal = ordinal.saturating_add(1);
        }
    }
}
fn render_collapsed_agent_panel_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
    p: &Palette,
) {
    if area == Rect::default() {
        return;
    }

    let toggle_rect = collapsed_agent_panel_toggle_rect(area);
    if toggle_rect != Rect::default() {
        let label = match view.agent_panel_scope {
            AgentPanelScope::AllWorkspaces => "All",
            AgentPanelScope::CurrentGroup => "f:g",
            AgentPanelScope::CurrentWorkspace => "f:s",
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                label,
                Style::default().fg(p.overlay1).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            toggle_rect,
        );
    }

    if area.height > 1 {
        let buf = frame.buffer_mut();
        let divider_y = area.y + 1;
        for x in area.x..area.x + area.width {
            buf[(x, divider_y)].set_symbol("─");
            buf[(x, divider_y)].set_style(Style::default().fg(p.overlay0));
        }
    }

    let rows = collapsed_agent_panel_body_rect(area);
    if rows == Rect::default() {
        return;
    }

    let mut row_y = rows.y;
    let mut ordinal = 1usize;
    for section in agent_panel_sections_for_view(app, terminal_runtimes, view) {
        if row_y >= rows.y + rows.height {
            return;
        }
        let collapsed = agent_panel_section_collapsed_for_view(view, section.group);
        render_collapsed_agent_section_header(
            frame,
            &section,
            collapsed,
            rows,
            row_y,
            app.spinner_tick,
            app.status_indicators,
            p,
        );
        row_y = row_y.saturating_add(1);
        for entry in &section.entries {
            if !collapsed {
                if row_y >= rows.y + rows.height {
                    return;
                }
                render_collapsed_agent_entry(
                    app,
                    frame,
                    entry,
                    ordinal,
                    view.agent_panel_scope,
                    agent_entry_is_active_for_view(app, view, entry),
                    rows,
                    row_y,
                    p,
                );
                row_y = row_y.saturating_add(1);
            }
            ordinal = ordinal.saturating_add(1);
        }
    }
}

pub(crate) fn collapsed_agent_panel_header_target_at_row(
    app: &AppState,
    area: Rect,
    row: u16,
) -> Option<AgentPanelHeaderTarget> {
    let rows = collapsed_agent_panel_body_rect(area);
    if rows == Rect::default() || row < rows.y || row >= rows.y + rows.height {
        return None;
    }

    let mut row_y = rows.y;
    for section in agent_panel_sections(app) {
        if row == row_y {
            return Some(AgentPanelHeaderTarget {
                section: section.group.label().to_string(),
            });
        }
        row_y = row_y.saturating_add(1);
        if !agent_panel_section_collapsed(app, section.group) {
            row_y = row_y.saturating_add(section.entries.len() as u16);
        }
        if row_y >= rows.y + rows.height {
            break;
        }
    }
    None
}

pub(crate) fn collapsed_agent_panel_header_target_at_row_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    area: Rect,
    row: u16,
) -> Option<AgentPanelHeaderTarget> {
    let rows = collapsed_agent_panel_body_rect(area);
    if rows == Rect::default() || row < rows.y || row >= rows.y + rows.height {
        return None;
    }

    let mut row_y = rows.y;
    for section in agent_panel_sections_for_view(app, terminal_runtimes, view) {
        if row == row_y {
            return Some(AgentPanelHeaderTarget {
                section: section.group.label().to_string(),
            });
        }
        row_y = row_y.saturating_add(1);
        if !agent_panel_section_collapsed_for_view(view, section.group) {
            row_y = row_y.saturating_add(section.entries.len() as u16);
        }
        if row_y >= rows.y + rows.height {
            break;
        }
    }
    None
}

pub(crate) fn collapsed_agent_panel_entry_at_row(
    app: &AppState,
    area: Rect,
    row: u16,
) -> Option<AgentPanelEntry> {
    let rows = collapsed_agent_panel_body_rect(area);
    if rows == Rect::default() || row < rows.y || row >= rows.y + rows.height {
        return None;
    }

    let mut row_y = rows.y;
    for section in agent_panel_sections(app) {
        row_y = row_y.saturating_add(1);
        if agent_panel_section_collapsed(app, section.group) {
            continue;
        }
        for entry in section.entries {
            if row_y >= rows.y + rows.height {
                return None;
            }
            if row == row_y {
                return Some(entry);
            }
            row_y = row_y.saturating_add(1);
        }
    }

    None
}

pub(crate) fn collapsed_agent_panel_entry_at_row_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    area: Rect,
    row: u16,
) -> Option<AgentPanelEntry> {
    let rows = collapsed_agent_panel_body_rect(area);
    if rows == Rect::default() || row < rows.y || row >= rows.y + rows.height {
        return None;
    }

    let mut row_y = rows.y;
    for section in agent_panel_sections_for_view(app, terminal_runtimes, view) {
        row_y = row_y.saturating_add(1);
        if agent_panel_section_collapsed_for_view(view, section.group) {
            continue;
        }
        for entry in section.entries {
            if row_y >= rows.y + rows.height {
                return None;
            }
            if row == row_y {
                return Some(entry);
            }
            row_y = row_y.saturating_add(1);
        }
    }
    None
}

pub(crate) fn collapsed_agent_rail_rect(
    sidebar: Rect,
    right_sidebar: Rect,
    sidebar_collapsed: bool,
    right_sidebar_collapsed: bool,
    split_ratio: f32,
) -> Option<Rect> {
    if right_sidebar != Rect::default() && right_sidebar_collapsed {
        let content = right_sidebar_content_rect(right_sidebar);
        return (content != Rect::default()).then_some(content);
    }
    if sidebar_collapsed && right_sidebar == Rect::default() {
        let (_, _, detail_area) = collapsed_sidebar_sections_for_split(sidebar, true, split_ratio);
        return (detail_area != Rect::default()).then_some(detail_area);
    }
    None
}

fn collapsed_agent_hover_row(
    app: &AppState,
    area: Rect,
    ws_idx: usize,
    pane_id: crate::layout::PaneId,
) -> Option<(u16, AgentPanelEntry)> {
    let rows = collapsed_agent_panel_body_rect(area);
    if rows == Rect::default() {
        return None;
    }
    let mut row_y = rows.y;
    for section in agent_panel_sections(app) {
        row_y = row_y.saturating_add(1);
        if agent_panel_section_collapsed(app, section.group) {
            continue;
        }
        for entry in section.entries {
            if row_y >= rows.y + rows.height {
                return None;
            }
            if entry.ws_idx == ws_idx && entry.pane_id == pane_id {
                return Some((row_y, entry));
            }
            row_y = row_y.saturating_add(1);
        }
    }
    None
}

fn collapsed_agent_hover_lines(app: &AppState, entry: &AgentPanelEntry) -> Vec<Line<'static>> {
    let p = &app.palette;
    let status = state_label(entry.state, entry.seen);
    let mut detail = Vec::new();
    if let Some(agent_label) = entry.agent_label.as_deref() {
        detail.push(Span::styled(
            agent_label.to_string(),
            Style::default().fg(p.overlay1),
        ));
        detail.push(Span::styled(" · ", Style::default().fg(p.overlay0)));
    }
    detail.push(Span::styled(
        status,
        Style::default()
            .fg(state_label_color(entry.state, entry.seen, p))
            .add_modifier(Modifier::BOLD),
    ));
    vec![
        Line::from(agent_panel_title_spans(
            &entry.primary_label,
            None,
            Style::default().fg(p.text),
            Style::default().fg(p.overlay1),
        )),
        Line::from(detail),
    ]
}

fn render_collapsed_hover_popup(
    app: &AppState,
    frame: &mut Frame,
    rail: Rect,
    row: u16,
    opens_left: bool,
    lines: Vec<Line<'static>>,
) {
    let content_width = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| display_width(span.content.as_ref()))
                .sum::<usize>()
        })
        .max()
        .unwrap_or_default();
    let popup_height = lines.len() as u16 + 2;
    let screen = frame.area();
    let available = if opens_left {
        rail.x.saturating_sub(screen.x)
    } else {
        screen
            .x
            .saturating_add(screen.width)
            .saturating_sub(rail.x.saturating_add(rail.width))
    };
    let width = (content_width as u16)
        .saturating_add(2)
        .min(40)
        .min(available);
    if width < 4 || screen.height < popup_height {
        return;
    }
    let x = if opens_left {
        rail.x.saturating_sub(width)
    } else {
        rail.x.saturating_add(rail.width)
    };
    let y = row.saturating_sub(1).min(
        screen
            .y
            .saturating_add(screen.height)
            .saturating_sub(popup_height),
    );
    let popup = Rect::new(x, y, width, popup_height);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(app.palette.overlay0))
                .style(
                    Style::default()
                        .bg(app.palette.panel_bg)
                        .fg(app.palette.text),
                ),
        ),
        popup,
    );
}

/// Collapsed sidebar: workspace glance plus compact agent list.
fn sidebar_is_combined_right(app: &AppState) -> bool {
    app.view.right_sidebar_rect == Rect::default()
        && app.sidebar_arrangement == crate::config::SidebarArrangementConfig::CombinedRight
}

fn collapsed_rail_separator_x(area: Rect, combined_right: bool) -> u16 {
    if combined_right {
        area.x
    } else {
        area.x + area.width.saturating_sub(1)
    }
}

fn paint_collapsed_rail_separator(
    frame: &mut Frame,
    area: Rect,
    combined_right: bool,
    style: Style,
) {
    let sep_x = collapsed_rail_separator_x(area, combined_right);
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(sep_x, y)].set_symbol("│");
        buf[(sep_x, y)].set_style(style);
    }
}

fn collapsed_workspace_status_icon(
    state: AgentState,
    seen: bool,
    tick: u32,
    style: crate::config::StatusIndicatorStyle,
    p: &Palette,
) -> (&'static str, Style) {
    let (icon, icon_style) = state_icon(state, seen, tick, style, p);
    // ◉ is optically wider than the rail cell and overlaps the collapsed border.
    if icon == "◉" {
        ("●", icon_style)
    } else {
        (icon, icon_style)
    }
}

fn collapsed_workspace_index_line(
    ordinal: usize,
    icon: &'static str,
    icon_style: Style,
    num_style: Style,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(ordinal.to_string(), num_style),
        Span::styled(icon, icon_style),
    ])
}

pub(super) fn render_sidebar_collapsed(app: &AppState, frame: &mut Frame, area: Rect) {
    let is_navigating = matches!(app.mode, Mode::Navigate);
    let show_agent_detail = app.view.right_sidebar_rect == Rect::default();
    let p = &app.palette;
    fill_rect(frame, area, Style::default().bg(p.panel_bg));
    let sep_style = if is_navigating {
        Style::default()
            .fg(app.active_workspace_accent_color())
            .bg(p.panel_bg)
    } else {
        Style::default().fg(p.overlay0).bg(p.panel_bg)
    };
    let combined_right = sidebar_is_combined_right(app);
    paint_collapsed_rail_separator(frame, area, combined_right, sep_style);

    let (ws_area, divider_y, detail_area) = if combined_right {
        right_aligned_collapsed_sidebar_sections(area, show_agent_detail, app.sidebar_section_split)
    } else {
        collapsed_sidebar_sections_for_split(area, show_agent_detail, app.sidebar_section_split)
    };
    let group_header = if combined_right {
        right_aligned_collapsed_group_header_rect(area)
    } else {
        collapsed_group_header_rect(area)
    };
    if group_header != Rect::default() {
        let label = collapsed_group_label(app);
        let style = if app.group_filter_enabled {
            Style::default()
                .fg(app.group_accent_color(app.active_group))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(app.active_workspace_accent_color())
                .add_modifier(Modifier::BOLD)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(label, style)).alignment(Alignment::Center),
            group_header,
        );
    }
    if ws_area.height > 1 {
        let buf = frame.buffer_mut();
        let divider_y = ws_area.y + 1;
        for x in ws_area.x..ws_area.x + ws_area.width {
            buf[(x, divider_y)].set_symbol("─");
            buf[(x, divider_y)].set_style(Style::default().fg(p.overlay0));
        }
    }

    let workspace_rows =
        if ws_area == Rect::default() || ws_area.height <= COLLAPSED_SECTION_HEADER_ROWS {
            Rect::default()
        } else {
            Rect::new(
                ws_area.x,
                ws_area.y + COLLAPSED_SECTION_HEADER_ROWS,
                ws_area.width,
                ws_area.height.saturating_sub(COLLAPSED_SECTION_HEADER_ROWS),
            )
        };
    if ws_area == Rect::default() || workspace_rows == Rect::default() {
        render_global_launcher(app, frame);
        render_sidebar_toggle(app, frame, area, true, p);
        return;
    }

    for (row_idx, entry) in collapsed_workspace_row_entries(app).into_iter().enumerate() {
        let y = workspace_rows.y + row_idx as u16;
        if y >= workspace_rows.y + workspace_rows.height {
            break;
        }

        match entry {
            CollapsedWorkspaceRowEntry::GroupHeader { group_idx } => {
                let Some(group) = app.groups.get(group_idx) else {
                    continue;
                };
                let chevron = if app.workspace_group_collapsed(&group.id) {
                    "▸"
                } else {
                    "▾"
                };
                let group_style = Style::default()
                    .fg(app.group_accent_color(group_idx))
                    .add_modifier(Modifier::BOLD);
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(chevron, Style::default().fg(p.overlay1)),
                        Span::styled(" ", Style::default()),
                        Span::styled(group.icon.clone(), group_style),
                    ])),
                    Rect::new(workspace_rows.x, y, workspace_rows.width, 1),
                );
            }
            CollapsedWorkspaceRowEntry::Workspace { ws_idx, ordinal } => {
                let Some(ws) = app.workspaces.get(ws_idx) else {
                    continue;
                };
                let (agg_state, agg_seen) = ws.aggregate_state(&app.terminals);
                let (icon, icon_style) = collapsed_workspace_status_icon(
                    agg_state,
                    agg_seen,
                    app.spinner_tick,
                    app.status_indicators,
                    p,
                );

                let is_selected = ws_idx == app.selected && is_navigating;
                let is_active = Some(ws_idx) == app.active;
                let row_style = if is_selected {
                    Style::default().bg(p.surface0)
                } else if is_active {
                    Style::default().bg(p.surface_dim)
                } else {
                    Style::default()
                };
                let num_style = if is_selected {
                    Style::default().fg(p.overlay1).bg(p.surface0)
                } else if is_active {
                    Style::default().fg(p.text).bg(p.surface_dim)
                } else {
                    Style::default().fg(p.overlay0)
                };

                if is_selected || is_active {
                    let buf = frame.buffer_mut();
                    for x in workspace_rows.x..workspace_rows.x + workspace_rows.width {
                        buf[(x, y)].set_style(row_style);
                    }
                }

                frame.render_widget(
                    Paragraph::new(collapsed_workspace_index_line(
                        ordinal, icon, icon_style, num_style,
                    )),
                    Rect::new(workspace_rows.x, y, workspace_rows.width, 1),
                );
            }
        }
    }
    paint_collapsed_rail_separator(frame, area, combined_right, sep_style);

    if let Some(divider_y) = divider_y {
        let buf = frame.buffer_mut();
        for x in ws_area.x..ws_area.x + ws_area.width {
            buf[(x, divider_y)].set_symbol("─");
            buf[(x, divider_y)].set_style(Style::default().fg(p.overlay0));
        }
    }

    if !show_agent_detail {
        render_global_launcher(app, frame);
        render_sidebar_toggle(app, frame, area, true, p);
        paint_collapsed_rail_separator(frame, area, combined_right, sep_style);
        return;
    }

    if show_agent_detail {
        let empty_runtimes = TerminalRuntimeRegistry::new();
        render_collapsed_agent_panel(app, &empty_runtimes, frame, detail_area, p);
    }

    render_global_launcher(app, frame);
    render_sidebar_toggle(app, frame, area, true, p);
    paint_collapsed_rail_separator(frame, area, combined_right, sep_style);
}

pub(super) fn render_sidebar_collapsed_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    let p = &app.palette;
    let navigating = matches!(view.mode, Mode::Navigate);
    let show_agent_detail = view.computed.right_sidebar_rect == Rect::default();
    fill_rect(frame, area, Style::default().bg(p.panel_bg));
    let combined_right = sidebar_is_combined_right_for_view(app, view);
    let separator_style = if navigating {
        Style::default()
            .fg(active_workspace_accent_color_for_view(app, view))
            .bg(p.panel_bg)
    } else {
        Style::default().fg(p.overlay0).bg(p.panel_bg)
    };
    paint_collapsed_rail_separator(frame, area, combined_right, separator_style);

    let (workspace_area, divider_y, agent_area) = if combined_right {
        right_aligned_collapsed_sidebar_sections(
            area,
            show_agent_detail,
            view.sidebar_section_split,
        )
    } else {
        collapsed_sidebar_sections_for_split(area, show_agent_detail, view.sidebar_section_split)
    };
    let group_header = if combined_right {
        right_aligned_collapsed_group_header_rect(area)
    } else {
        collapsed_group_header_rect(area)
    };
    if group_header != Rect::default() {
        let label = if view.group_filter_enabled {
            app.groups
                .get(view.active_group)
                .map(|group| group.icon.clone())
                .unwrap_or_else(|| "·".to_string())
        } else {
            "All".to_string()
        };
        let color = if view.group_filter_enabled {
            app.group_accent_color(view.active_group)
        } else {
            active_workspace_accent_color_for_view(app, view)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                label,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            group_header,
        );
    }
    if workspace_area.height > 1 {
        let y = workspace_area.y + 1;
        for x in workspace_area.x..workspace_area.x + workspace_area.width {
            frame.buffer_mut()[(x, y)]
                .set_symbol("─")
                .set_style(Style::default().fg(p.overlay0));
        }
    }
    let rows = Rect::new(
        workspace_area.x,
        workspace_area.y + COLLAPSED_SECTION_HEADER_ROWS,
        workspace_area.width,
        workspace_area
            .height
            .saturating_sub(COLLAPSED_SECTION_HEADER_ROWS),
    );
    if workspace_area != Rect::default() && rows != Rect::default() {
        for (row_idx, entry) in collapsed_workspace_row_entries_for_view(app, view)
            .into_iter()
            .enumerate()
        {
            let y = rows.y + row_idx as u16;
            if y >= rows.y + rows.height {
                break;
            }
            match entry {
                CollapsedWorkspaceRowEntry::GroupHeader { group_idx } => {
                    let Some(group) = app.groups.get(group_idx) else {
                        continue;
                    };
                    let chevron = if workspace_group_collapsed_for_view(view, &group.id) {
                        "▸"
                    } else {
                        "▾"
                    };
                    let group_style = Style::default()
                        .fg(app.group_accent_color(group_idx))
                        .add_modifier(Modifier::BOLD);
                    frame.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled(chevron, Style::default().fg(p.overlay1)),
                            Span::raw(" "),
                            Span::styled(group.icon.clone(), group_style),
                        ])),
                        Rect::new(rows.x, y, rows.width, 1),
                    );
                }
                CollapsedWorkspaceRowEntry::Workspace { ws_idx, ordinal } => {
                    let Some(workspace) = app.workspaces.get(ws_idx) else {
                        continue;
                    };
                    let (state, seen) = workspace.aggregate_state(&app.terminals);
                    let (icon, icon_style) = collapsed_workspace_status_icon(
                        state,
                        seen,
                        app.spinner_tick,
                        app.status_indicators,
                        p,
                    );

                    let selected = navigating && ws_idx == view.selected_workspace;
                    let active = Some(ws_idx) == view.active_workspace;
                    let bg = if selected { p.surface0 } else { p.surface_dim };
                    let row_style = if selected || active {
                        Style::default().bg(bg)
                    } else {
                        Style::default()
                    };
                    let num_style = if selected {
                        Style::default().fg(p.overlay1).bg(bg)
                    } else if active {
                        Style::default().fg(p.text).bg(bg)
                    } else {
                        Style::default().fg(p.overlay0)
                    };
                    if selected || active {
                        for x in rows.x..rows.x + rows.width {
                            frame.buffer_mut()[(x, y)].set_style(row_style);
                        }
                    }
                    frame.render_widget(
                        Paragraph::new(collapsed_workspace_index_line(
                            ordinal, icon, icon_style, num_style,
                        )),
                        Rect::new(rows.x, y, rows.width, 1),
                    );
                }
            }
        }
    }
    if let Some(y) = divider_y {
        for x in workspace_area.x..workspace_area.x + workspace_area.width {
            frame.buffer_mut()[(x, y)]
                .set_symbol("─")
                .set_style(Style::default().fg(p.overlay0));
        }
    }
    if show_agent_detail {
        render_collapsed_agent_panel_for_view(app, terminal_runtimes, view, frame, agent_area, p);
    }
    render_global_launcher_for_view(app, view, frame);
    render_sidebar_toggle(app, frame, area, true, p);
    paint_collapsed_rail_separator(frame, area, combined_right, separator_style);
}

pub(super) fn render_collapsed_sidebar_hover(app: &AppState, frame: &mut Frame) {
    match &app.collapsed_sidebar_hover {
        Some(CollapsedSidebarHover::Agent { ws_idx, pane_id }) => {
            render_collapsed_agent_hover(
                app,
                frame,
                app.view.sidebar_rect,
                app.view.right_sidebar_rect,
                app.sidebar_collapsed,
                app.right_sidebar_collapsed,
                app.sidebar_section_split,
                *ws_idx,
                *pane_id,
            );
            return;
        }
        Some(CollapsedSidebarHover::AgentStatus { section }) => {
            render_collapsed_agent_status_hover(
                app,
                frame,
                app.view.sidebar_rect,
                app.view.right_sidebar_rect,
                app.sidebar_collapsed,
                app.right_sidebar_collapsed,
                app.sidebar_section_split,
                section,
            );
            return;
        }
        _ => {}
    }
    render_collapsed_sidebar_hover_entry(
        app,
        frame,
        app.view.sidebar_rect,
        app.sidebar_section_split,
        app.view.right_sidebar_rect == Rect::default(),
        matches!(app.mode, Mode::Navigate),
        app.selected,
        app.collapsed_sidebar_hover.clone(),
        collapsed_workspace_row_entries(app),
    );
}

pub(super) fn render_collapsed_sidebar_hover_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
) {
    match &view.collapsed_sidebar_hover {
        Some(CollapsedSidebarHover::Agent { ws_idx, pane_id }) => {
            render_collapsed_agent_hover(
                app,
                frame,
                view.computed.sidebar_rect,
                view.computed.right_sidebar_rect,
                view.sidebar_collapsed,
                view.right_sidebar_collapsed,
                view.sidebar_section_split,
                *ws_idx,
                *pane_id,
            );
            return;
        }
        Some(CollapsedSidebarHover::AgentStatus { section }) => {
            render_collapsed_agent_status_hover(
                app,
                frame,
                view.computed.sidebar_rect,
                view.computed.right_sidebar_rect,
                view.sidebar_collapsed,
                view.right_sidebar_collapsed,
                view.sidebar_section_split,
                section,
            );
            return;
        }
        _ => {}
    }
    render_collapsed_sidebar_hover_entry(
        app,
        frame,
        view.computed.sidebar_rect,
        view.sidebar_section_split,
        view.computed.right_sidebar_rect == Rect::default(),
        matches!(view.mode, Mode::Navigate),
        view.selected_workspace,
        view.collapsed_sidebar_hover.clone(),
        collapsed_workspace_row_entries_for_view(app, view),
    );
}

fn render_collapsed_agent_hover(
    app: &AppState,
    frame: &mut Frame,
    sidebar: Rect,
    right_sidebar: Rect,
    sidebar_collapsed: bool,
    right_sidebar_collapsed: bool,
    split_ratio: f32,
    ws_idx: usize,
    pane_id: crate::layout::PaneId,
) {
    let Some(rail) = collapsed_agent_rail_rect(
        sidebar,
        right_sidebar,
        sidebar_collapsed,
        right_sidebar_collapsed,
        split_ratio,
    ) else {
        return;
    };
    let Some((row, entry)) = collapsed_agent_hover_row(app, rail, ws_idx, pane_id) else {
        return;
    };
    let opens_left = app.sidebar_arrangement
        == crate::config::SidebarArrangementConfig::CombinedRight
        || right_sidebar != Rect::default();
    render_collapsed_hover_popup(
        app,
        frame,
        rail,
        row,
        opens_left,
        collapsed_agent_hover_lines(app, &entry),
    );
}

fn render_collapsed_agent_status_hover(
    app: &AppState,
    frame: &mut Frame,
    sidebar: Rect,
    right_sidebar: Rect,
    sidebar_collapsed: bool,
    right_sidebar_collapsed: bool,
    split_ratio: f32,
    section: &str,
) {
    let Some(rail) = collapsed_agent_rail_rect(
        sidebar,
        right_sidebar,
        sidebar_collapsed,
        right_sidebar_collapsed,
        split_ratio,
    ) else {
        return;
    };
    let Some((row, group)) = collapsed_agent_status_hover_row(app, rail, section) else {
        return;
    };
    let (icon, icon_style) =
        agent_section_icon(group, app.spinner_tick, app.status_indicators, &app.palette);
    let opens_left = app.sidebar_arrangement
        == crate::config::SidebarArrangementConfig::CombinedRight
        || right_sidebar != Rect::default();
    render_collapsed_hover_popup(
        app,
        frame,
        rail,
        row,
        opens_left,
        vec![Line::from(vec![
            Span::styled(icon, icon_style),
            Span::styled(format!(" {section}"), icon_style),
        ])],
    );
}

fn collapsed_agent_status_hover_row(
    app: &AppState,
    area: Rect,
    section: &str,
) -> Option<(u16, AgentStatusGroup)> {
    let rows = collapsed_agent_panel_body_rect(area);
    if rows == Rect::default() {
        return None;
    }
    let mut row_y = rows.y;
    for panel in agent_panel_sections(app) {
        if row_y >= rows.y + rows.height {
            return None;
        }
        if panel.group.label() == section {
            return Some((row_y, panel.group));
        }
        row_y = row_y.saturating_add(1);
        if !agent_panel_section_collapsed(app, panel.group) {
            row_y = row_y.saturating_add(panel.entries.len() as u16);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)] // Keeps the shared and client-local render paths identical.
fn render_collapsed_sidebar_hover_entry(
    app: &AppState,
    frame: &mut Frame,
    sidebar: Rect,
    split_ratio: f32,
    show_agent_detail: bool,
    navigating: bool,
    selected_workspace: usize,
    hover: Option<CollapsedSidebarHover>,
    entries: Vec<CollapsedWorkspaceRowEntry>,
) {
    let target = hover
        .or_else(|| navigating.then_some(CollapsedSidebarHover::Workspace(selected_workspace)));
    let Some(target) = target else {
        return;
    };
    let entry = match target {
        CollapsedSidebarHover::Agent { .. } | CollapsedSidebarHover::AgentStatus { .. } => return,
        CollapsedSidebarHover::Group(group_idx) => {
            CollapsedWorkspaceRowEntry::GroupHeader { group_idx }
        }
        CollapsedSidebarHover::Workspace(ws_idx) => {
            let Some(entry) = entries.iter().copied().find(
                |entry| matches!(entry, CollapsedWorkspaceRowEntry::Workspace { ws_idx: idx, .. } if *idx == ws_idx),
            ) else {
                return;
            };
            entry
        }
    };
    let Some(row_idx) = entries.iter().position(|candidate| *candidate == entry) else {
        return;
    };
    let rows = collapsed_workspace_rows_rect_for_split(sidebar, show_agent_detail, split_ratio);
    let row = rows.y.saturating_add(row_idx as u16);
    if rows == Rect::default() || row >= rows.y.saturating_add(rows.height) {
        return;
    }

    let lines = match entry {
        CollapsedWorkspaceRowEntry::GroupHeader { group_idx } => {
            let Some(group) = app.groups.get(group_idx) else {
                return;
            };
            vec![Line::from(vec![
                Span::styled(
                    group.icon.clone(),
                    Style::default()
                        .fg(app.group_accent_color(group_idx))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", group.name),
                    Style::default()
                        .fg(app.group_accent_color(group_idx))
                        .add_modifier(Modifier::BOLD),
                ),
            ])]
        }
        CollapsedWorkspaceRowEntry::Workspace { ws_idx, .. } => {
            let Some(workspace) = app.workspaces.get(ws_idx) else {
                return;
            };
            let Some(group_idx) = app.group_index_by_id(&workspace.group_id) else {
                return;
            };
            let Some(group) = app.groups.get(group_idx) else {
                return;
            };
            let (state, seen) = workspace.aggregate_state(&app.terminals);
            vec![
                Line::from(Span::styled(
                    group.name.clone(),
                    Style::default().fg(app.group_accent_color(group_idx)),
                )),
                Line::from(vec![
                    Span::styled(
                        workspace.display_name(),
                        Style::default().fg(app.palette.text),
                    ),
                    Span::styled(" · ", Style::default().fg(app.palette.overlay0)),
                    Span::styled(
                        state_label(state, seen),
                        Style::default()
                            .fg(state_label_color(state, seen, &app.palette))
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
            ]
        }
    };
    let content_width = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| display_width(span.content.as_ref()))
                .sum::<usize>()
        })
        .max()
        .unwrap_or_default();
    let popup_height = lines.len() as u16 + 2;

    let screen = frame.area();
    let opens_left =
        app.sidebar_arrangement == crate::config::SidebarArrangementConfig::CombinedRight;
    let available = if opens_left {
        sidebar.x.saturating_sub(screen.x)
    } else {
        screen
            .x
            .saturating_add(screen.width)
            .saturating_sub(sidebar.x.saturating_add(sidebar.width))
    };
    let width = (content_width as u16)
        .saturating_add(2)
        .min(40)
        .min(available);
    if width < 4 || screen.height < popup_height {
        return;
    }
    let x = if opens_left {
        sidebar.x.saturating_sub(width)
    } else {
        sidebar.x.saturating_add(sidebar.width)
    };
    let y = row.saturating_sub(1).min(
        screen
            .y
            .saturating_add(screen.height)
            .saturating_sub(popup_height),
    );
    let popup = Rect::new(x, y, width, popup_height);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(app.palette.overlay0))
                .style(
                    Style::default()
                        .bg(app.palette.panel_bg)
                        .fg(app.palette.text),
                ),
        ),
        popup,
    );
}

pub(crate) fn workspace_drop_indicator_row(
    cards: &[crate::app::state::WorkspaceCardArea],
    area: Rect,
    insert_idx: usize,
) -> Option<u16> {
    let first = cards.first()?;
    if insert_idx == first.ws_idx {
        return Some(first.rect.y);
    }

    if let Some(card) = cards.iter().find(|card| card.ws_idx == insert_idx) {
        return Some(card.rect.y);
    }

    cards
        .last()
        .filter(|card| insert_idx == card.ws_idx.saturating_add(1))
        .map(|card| {
            let body_bottom = area.y.saturating_add(area.height);
            card.rect
                .y
                .saturating_add(card.rect.height)
                .min(body_bottom.saturating_sub(1))
        })
}
fn render_drop_indicator(
    frame: &mut Frame,
    left: u16,
    right: u16,
    row: u16,
    color: ratatui::style::Color,
) {
    let buffer = frame.buffer_mut();
    for x in left..right {
        buffer[(x, row)].set_symbol("─");
        buffer[(x, row)].set_style(Style::default().fg(color));
    }
}

fn global_launcher_style(app: &AppState) -> Style {
    Style::default()
        .fg(if app.config_issue.is_some() {
            app.palette.yellow
        } else if app.global_menu_attention_badge_visible() {
            app.palette.accent
        } else {
            app.palette.overlay0
        })
        .add_modifier(Modifier::BOLD)
}

fn render_global_launcher(app: &AppState, frame: &mut Frame) {
    let area = app.global_launcher_rect();
    if area == Rect::default() {
        return;
    }

    frame.render_widget(
        Paragraph::new(Span::styled(
            app.global_launcher_label(area.width),
            global_launcher_style(app),
        )),
        area,
    );
}

fn render_global_launcher_for_view(
    app: &AppState,
    client_view: &ClientViewState,
    frame: &mut Frame,
) {
    let area = global_launcher_rect_for_view(app, client_view);
    if area == Rect::default() {
        return;
    }

    frame.render_widget(
        Paragraph::new(Span::styled(
            app.global_launcher_label(area.width),
            global_launcher_style(app),
        )),
        area,
    );
}

pub(crate) fn global_launcher_rect_for_view(app: &AppState, client_view: &ClientViewState) -> Rect {
    if client_view.computed.layout == crate::app::state::ViewLayout::Mobile {
        return Rect::default();
    }

    if client_view.sidebar_collapsed {
        return collapsed_sidebar_launcher_rect(client_view.computed.sidebar_rect);
    }

    let footer = client_view_sidebar_footer_rect(client_view);
    if footer == Rect::default() {
        return Rect::default();
    }

    let x = footer
        .x
        .saturating_add(1)
        .min(footer.x.saturating_add(footer.width.saturating_sub(1)));
    let available = footer.x.saturating_add(footer.width).saturating_sub(x);
    let width = app.global_launcher_width(available);
    Rect::new(x, footer.y, width, footer.height)
}

fn client_view_sidebar_footer_rect(client_view: &ClientViewState) -> Rect {
    let sidebar = client_view.computed.sidebar_rect;
    if client_view.sidebar_collapsed || sidebar.width <= 1 || sidebar.height == 0 {
        return Rect::default();
    }

    Rect::new(
        sidebar.x,
        sidebar.y + sidebar.height.saturating_sub(1),
        sidebar.width,
        1,
    )
}

fn active_workspace_accent_color_for_view(
    app: &AppState,
    client_view: &ClientViewState,
) -> ratatui::style::Color {
    if !client_view.group_filter_enabled {
        if let Some(group_idx) = client_view
            .active_workspace
            .and_then(|idx| app.workspaces.get(idx))
            .map(|workspace| workspace.group_id.as_str())
            .and_then(|group_id| app.group_index_by_id(group_id))
        {
            return app.group_accent_color(group_idx);
        }
    }

    app.group_accent_color(client_view.active_group)
}

fn sidebar_is_combined_right_for_view(app: &AppState, client_view: &ClientViewState) -> bool {
    app.sidebar_arrangement == crate::config::SidebarArrangementConfig::CombinedRight
        && !client_view.right_sidebar_collapsed
        && client_view.computed.right_sidebar_rect == Rect::default()
}

pub(super) fn render_sidebar_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    client_view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    let p = &app.palette;
    fill_rect(frame, area, Style::default().bg(p.panel_bg));
    let is_navigating = matches!(client_view.mode, Mode::Navigate);
    let sep_style = if is_navigating {
        Style::default()
            .fg(active_workspace_accent_color_for_view(app, client_view))
            .bg(p.panel_bg)
    } else {
        Style::default().fg(p.overlay0).bg(p.panel_bg)
    };

    let combined_right = sidebar_is_combined_right_for_view(app, client_view);
    let sep_x = if combined_right {
        area.x
    } else {
        area.x + area.width.saturating_sub(1)
    };
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(sep_x, y)].set_symbol("│");
        buf[(sep_x, y)].set_style(sep_style);
    }

    if client_view.computed.right_sidebar_rect != Rect::default() {
        let ws_area = left_sidebar_workspace_rect(area);
        render_workspace_list_from_for_view(
            app,
            terminal_runtimes,
            client_view,
            frame,
            ws_area,
            is_navigating,
        );
    } else if combined_right {
        let (ws_area, detail_area) =
            right_aligned_expanded_sidebar_sections(area, client_view.sidebar_section_split);
        render_agent_detail_from_for_view(
            app,
            terminal_runtimes,
            client_view,
            frame,
            detail_area,
            true,
        );
        render_workspace_list_from_for_view(
            app,
            terminal_runtimes,
            client_view,
            frame,
            ws_area,
            is_navigating,
        );
    } else {
        let (ws_area, detail_area) =
            expanded_sidebar_sections(area, client_view.sidebar_section_split);
        render_agent_detail_from_for_view(
            app,
            terminal_runtimes,
            client_view,
            frame,
            detail_area,
            true,
        );
        render_workspace_list_from_for_view(
            app,
            terminal_runtimes,
            client_view,
            frame,
            ws_area,
            is_navigating,
        );
    }
    render_global_launcher_for_view(app, client_view, frame);
    render_sidebar_toggle(app, frame, area, false, p);
}

pub(super) fn render_sidebar(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    let p = &app.palette;
    fill_rect(frame, area, Style::default().bg(p.panel_bg));
    let is_navigating = matches!(app.mode, Mode::Navigate);
    let sep_style = if is_navigating {
        Style::default()
            .fg(app.active_workspace_accent_color())
            .bg(p.panel_bg)
    } else {
        Style::default().fg(p.overlay0).bg(p.panel_bg)
    };

    let combined_right = sidebar_is_combined_right(app);
    let sep_x = if combined_right {
        area.x
    } else {
        area.x + area.width.saturating_sub(1)
    };
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(sep_x, y)].set_symbol("│");
        buf[(sep_x, y)].set_style(sep_style);
    }

    if app.view.right_sidebar_rect != Rect::default() {
        let ws_area = left_sidebar_workspace_rect(area);
        render_workspace_list_from(app, terminal_runtimes, frame, ws_area, is_navigating);
    } else if combined_right {
        let (ws_area, detail_area) =
            right_aligned_expanded_sidebar_sections(area, app.sidebar_section_split);
        render_agent_detail_from(app, terminal_runtimes, frame, detail_area, true);
        render_workspace_list_from(app, terminal_runtimes, frame, ws_area, is_navigating);
    } else {
        let (ws_area, detail_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        render_agent_detail_from(app, terminal_runtimes, frame, detail_area, true);
        render_workspace_list_from(app, terminal_runtimes, frame, ws_area, is_navigating);
    }
    render_global_launcher(app, frame);
    render_sidebar_toggle(app, frame, area, false, p);
}

fn render_collapsed_agent_rail(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
    p: &Palette,
) {
    render_collapsed_agent_panel(
        app,
        terminal_runtimes,
        frame,
        right_sidebar_content_rect(area),
        p,
    );
}

pub(super) fn render_right_sidebar_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    client_view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    if area == Rect::default() {
        return;
    }
    let p = &app.palette;
    fill_rect(frame, area, Style::default().bg(p.panel_bg));
    let has_active_workspace = client_view
        .active_workspace
        .and_then(|idx| app.workspaces.get(idx))
        .is_some();
    let sep_style = if !has_active_workspace && matches!(client_view.mode, Mode::Navigate) {
        Style::default()
            .fg(active_workspace_accent_color_for_view(app, client_view))
            .bg(p.panel_bg)
    } else {
        Style::default().fg(p.overlay0).bg(p.panel_bg)
    };
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(area.x, y)].set_symbol("│");
        buf[(area.x, y)].set_style(sep_style);
    }
    if client_view.right_sidebar_collapsed {
        render_collapsed_agent_panel_for_view(
            app,
            terminal_runtimes,
            client_view,
            frame,
            right_sidebar_content_rect(area),
            p,
        );
        render_right_sidebar_toggle(app, frame, area, true, p);
    } else {
        render_agent_detail_from_for_view(
            app,
            terminal_runtimes,
            client_view,
            frame,
            right_sidebar_content_rect(area),
            false,
        );
        render_right_sidebar_toggle(app, frame, area, false, p);
    }
}

pub(super) fn render_right_sidebar(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    if area == Rect::default() {
        return;
    }
    let p = &app.palette;
    fill_rect(frame, area, Style::default().bg(p.panel_bg));
    let has_active_workspace = app.active.and_then(|idx| app.workspaces.get(idx)).is_some();
    let sep_style = if !has_active_workspace && matches!(app.mode, Mode::Navigate) {
        Style::default()
            .fg(app.active_workspace_accent_color())
            .bg(p.panel_bg)
    } else {
        Style::default().fg(p.overlay0).bg(p.panel_bg)
    };
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(area.x, y)].set_symbol("│");
        buf[(area.x, y)].set_style(sep_style);
    }
    if app.right_sidebar_collapsed {
        render_collapsed_agent_rail(app, terminal_runtimes, frame, area, p);
        render_right_sidebar_toggle(app, frame, area, true, p);
    } else {
        render_agent_detail_from(
            app,
            terminal_runtimes,
            frame,
            right_sidebar_content_rect(area),
            false,
        );
        render_right_sidebar_toggle(app, frame, area, false, p);
    }
}

#[cfg(test)]
fn render_workspace_list(app: &AppState, frame: &mut Frame, area: Rect, is_navigating: bool) {
    let empty_runtimes = TerminalRuntimeRegistry::new();
    render_workspace_list_from(app, &empty_runtimes, frame, area, is_navigating);
}

fn render_workspace_list_from(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
    is_navigating: bool,
) {
    let p = &app.palette;
    let dragged_ws_idx = match app.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::WorkspaceReorder { source_ws_idx, .. }) => {
            Some(*source_ws_idx)
        }
        _ => None,
    };
    let dragged_group_idx = match app.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::GroupReorder {
            source_group_idx, ..
        }) => Some(*source_group_idx),
        _ => None,
    };
    let insertion_row = match app.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::WorkspaceReorder { indicator_row, .. })
        | Some(crate::app::state::DragTarget::GroupReorder { indicator_row, .. }) => *indicator_row,
        _ => None,
    };

    let list_bottom = area.y + area.height.saturating_sub(1);
    if area.height > 0 {
        let selector_rect = app.group_selector_rect();
        frame.render_widget(
            Paragraph::new(Span::styled(
                if app.group_filter_enabled {
                    "Spaces"
                } else {
                    "Groups"
                },
                Style::default().fg(p.overlay1).add_modifier(Modifier::BOLD),
            )),
            Rect::new(area.x, area.y, area.width, 1),
        );

        if selector_rect != Rect::default() {
            let group_color = if app.group_filter_enabled {
                app.group_accent_color(app.active_group)
            } else {
                app.active_workspace_accent_color()
            };
            let base = Style::default().fg(group_color).bg(p.surface0);
            let count = Style::default().fg(p.overlay0).bg(p.surface0);

            frame.render_widget(
                Paragraph::new(centered_count_line(
                    &app.group_selector_label(),
                    selector_rect.width,
                    base,
                    count,
                )),
                selector_rect,
            );
        }

        if area.height > 1 {
            let sep_line = "─".repeat(area.width as usize);
            frame.render_widget(
                Paragraph::new(Span::styled(&sep_line, Style::default().fg(p.overlay0))),
                Rect::new(area.x, area.y + 1, area.width, 1),
            );
        }
    }

    let metrics = workspace_list_scroll_metrics(app, area);
    let scrollbar_rect = workspace_list_scrollbar_rect(app, area);
    let body = workspace_list_body_rect(area, should_show_scrollbar(metrics));
    let cards = &app.view.workspace_card_areas;
    let headers = &app.view.workspace_group_header_areas;
    let empty_rows = &app.view.workspace_group_empty_areas;
    if cards.is_empty()
        && headers.is_empty()
        && empty_rows.is_empty()
        && body.height > 0
        && body.width > 10
    {
        let title = if app.workspaces.is_empty() {
            " No Spaces"
        } else {
            " Empty Group"
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                title,
                Style::default().fg(p.overlay0).add_modifier(Modifier::BOLD),
            ))),
            Rect::new(body.x, body.y, body.width, 1),
        );
    }

    for header in headers {
        let Some(group) = app.groups.get(header.group_idx) else {
            continue;
        };
        let is_dragged_group = dragged_group_idx == Some(header.group_idx);
        let count = app
            .workspaces
            .iter()
            .filter(|workspace| workspace.group_id == group.id)
            .count();
        let chevron = if app.workspace_group_collapsed(&group.id) {
            "▸"
        } else {
            "▾"
        };
        let group_style = Style::default()
            .fg(app.group_accent_color(header.group_idx))
            .add_modifier(Modifier::BOLD);
        if is_dragged_group {
            let buf = frame.buffer_mut();
            for x in header.rect.x..header.rect.x + header.rect.width {
                buf[(x, header.rect.y)].set_style(Style::default().bg(p.surface1));
            }
        }
        let line = Line::from(vec![
            Span::styled(chevron.to_string(), Style::default().fg(p.overlay1)),
            Span::styled(
                " ".repeat(
                    SIDEBAR_GROUP_ICON_COL.saturating_sub(SIDEBAR_GROUP_CHEVRON_COL + 1) as usize,
                ),
                Style::default(),
            ),
            Span::styled(group.icon.clone(), group_style),
            Span::styled(
                " ".repeat(
                    SIDEBAR_GROUP_NAME_COL.saturating_sub(SIDEBAR_GROUP_ICON_COL + 1) as usize,
                ),
                Style::default(),
            ),
            Span::styled(group.name.clone(), group_style),
        ]);
        frame.render_widget(Paragraph::new(line), header.rect);
        if app.show_counters {
            let count_label = count.to_string();
            let count_width = count_label.chars().count() as u16;
            if header.rect.width > count_width + SIDEBAR_GROUP_COUNT_RIGHT_PAD {
                frame.render_widget(
                    Paragraph::new(Span::styled(count_label, Style::default().fg(p.overlay0))),
                    Rect::new(
                        header.rect.x
                            + header
                                .rect
                                .width
                                .saturating_sub(count_width + SIDEBAR_GROUP_COUNT_RIGHT_PAD),
                        header.rect.y,
                        count_width,
                        1,
                    ),
                );
            }
        }
    }

    for empty in empty_rows {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(
                    "{}No Spaces",
                    " ".repeat(SIDEBAR_WORKSPACE_NAME_COL as usize)
                ),
                Style::default().fg(p.overlay0).add_modifier(Modifier::DIM),
            )),
            empty.rect,
        );
    }

    for card in cards {
        let i = card.ws_idx;
        let ws = &app.workspaces[i];
        let row_y = card.rect.y;
        let row_height = card.rect.height;
        let selected = i == app.selected && is_navigating;
        let is_active = Some(i) == app.active;
        let is_dragged = dragged_ws_idx == Some(i);
        let highlighted = selected || is_active || is_dragged;

        if highlighted {
            let bg = if selected {
                p.surface0
            } else if is_dragged {
                p.surface1
            } else {
                p.surface_dim
            };
            let buf = frame.buffer_mut();
            for y in row_y..row_y + row_height {
                if y >= list_bottom {
                    break;
                }
                for x in card.rect.x..card.rect.x + card.rect.width {
                    buf[(x, y)].set_style(Style::default().bg(bg));
                }
            }
        }

        if is_active {
            let buf = frame.buffer_mut();
            for y in row_y..row_y + row_height {
                if y >= list_bottom {
                    break;
                }
                buf[(card.rect.x, y)].set_symbol("▌");
                buf[(card.rect.x, y)]
                    .set_style(Style::default().fg(app.active_workspace_accent_color()));
            }
        }

        let name_style = if selected || is_active || is_dragged {
            Style::default().fg(p.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.subtext0)
        };

        render_workspace_token_rows(
            app,
            frame,
            ws,
            terminal_runtimes,
            card.rect,
            row_y,
            row_height,
            name_style,
        );
    }

    if let Some(y) = insertion_row.filter(|y| *y < list_bottom) {
        let indicator_right = scrollbar_rect
            .map(|rect| rect.x)
            .unwrap_or(area.x + area.width);
        render_drop_indicator(
            frame,
            area.x,
            indicator_right,
            y,
            app.active_workspace_accent_color(),
        );
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }
}

fn render_workspace_list_from_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    client_view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
    is_navigating: bool,
) {
    let p = &app.palette;
    let dragged_ws_idx = match client_view.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::WorkspaceReorder { source_ws_idx, .. }) => {
            Some(*source_ws_idx)
        }
        _ => None,
    };
    let dragged_group_idx = match client_view.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::GroupReorder {
            source_group_idx, ..
        }) => Some(*source_group_idx),
        _ => None,
    };
    let insertion_row = match client_view.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::WorkspaceReorder { indicator_row, .. })
        | Some(crate::app::state::DragTarget::GroupReorder { indicator_row, .. }) => *indicator_row,
        _ => None,
    };

    let list_bottom = area.y + area.height.saturating_sub(1);
    if area.height > 0 {
        let selector_rect = group_selector_rect_for_view(app, client_view);
        frame.render_widget(
            Paragraph::new(Span::styled(
                if client_view.group_filter_enabled {
                    "Spaces"
                } else {
                    "Groups"
                },
                Style::default().fg(p.overlay1).add_modifier(Modifier::BOLD),
            )),
            Rect::new(area.x, area.y, area.width, 1),
        );

        if selector_rect != Rect::default() {
            let group_color = if client_view.group_filter_enabled {
                app.group_accent_color(client_view.active_group)
            } else {
                active_workspace_accent_color_for_view(app, client_view)
            };
            let base = Style::default().fg(group_color).bg(p.surface0);
            let count = Style::default().fg(p.overlay0).bg(p.surface0);

            frame.render_widget(
                Paragraph::new(centered_count_line(
                    &group_selector_label_for_view(app, client_view),
                    selector_rect.width,
                    base,
                    count,
                )),
                selector_rect,
            );
        }

        if area.height > 1 {
            let sep_line = "─".repeat(area.width as usize);
            frame.render_widget(
                Paragraph::new(Span::styled(&sep_line, Style::default().fg(p.overlay0))),
                Rect::new(area.x, area.y + 1, area.width, 1),
            );
        }
    }

    let metrics = workspace_list_scroll_metrics_for_view(app, client_view, area);
    let scrollbar_rect = workspace_list_scrollbar_rect_for_view(app, client_view, area);
    let body = workspace_list_body_rect(area, should_show_scrollbar(metrics));
    let cards = &client_view.computed.workspace_card_areas;
    let headers = &client_view.computed.workspace_group_header_areas;
    let empty_rows = &client_view.computed.workspace_group_empty_areas;
    if cards.is_empty()
        && headers.is_empty()
        && empty_rows.is_empty()
        && body.height > 0
        && body.width > 10
    {
        let title = if app.workspaces.is_empty() {
            " No Spaces"
        } else {
            " Empty Group"
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                title,
                Style::default().fg(p.overlay0).add_modifier(Modifier::BOLD),
            ))),
            Rect::new(body.x, body.y, body.width, 1),
        );
    }

    for header in headers {
        let Some(group) = app.groups.get(header.group_idx) else {
            continue;
        };
        let is_dragged_group = dragged_group_idx == Some(header.group_idx);
        let count = app
            .workspaces
            .iter()
            .filter(|workspace| workspace.group_id == group.id)
            .count();
        let chevron = if workspace_group_collapsed_for_view(client_view, &group.id) {
            "▸"
        } else {
            "▾"
        };
        let group_style = Style::default()
            .fg(app.group_accent_color(header.group_idx))
            .add_modifier(Modifier::BOLD);
        if is_dragged_group {
            let buf = frame.buffer_mut();
            for x in header.rect.x..header.rect.x + header.rect.width {
                buf[(x, header.rect.y)].set_style(Style::default().bg(p.surface1));
            }
        }
        let line = Line::from(vec![
            Span::styled(chevron.to_string(), Style::default().fg(p.overlay1)),
            Span::styled(
                " ".repeat(
                    SIDEBAR_GROUP_ICON_COL.saturating_sub(SIDEBAR_GROUP_CHEVRON_COL + 1) as usize,
                ),
                Style::default(),
            ),
            Span::styled(group.icon.clone(), group_style),
            Span::styled(
                " ".repeat(
                    SIDEBAR_GROUP_NAME_COL.saturating_sub(SIDEBAR_GROUP_ICON_COL + 1) as usize,
                ),
                Style::default(),
            ),
            Span::styled(group.name.clone(), group_style),
        ]);
        frame.render_widget(Paragraph::new(line), header.rect);
        if app.show_counters {
            let count_label = count.to_string();
            let count_width = count_label.chars().count() as u16;
            if header.rect.width > count_width + SIDEBAR_GROUP_COUNT_RIGHT_PAD {
                frame.render_widget(
                    Paragraph::new(Span::styled(count_label, Style::default().fg(p.overlay0))),
                    Rect::new(
                        header.rect.x
                            + header
                                .rect
                                .width
                                .saturating_sub(count_width + SIDEBAR_GROUP_COUNT_RIGHT_PAD),
                        header.rect.y,
                        count_width,
                        1,
                    ),
                );
            }
        }
    }

    for empty in empty_rows {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(
                    "{}no spaces",
                    " ".repeat(SIDEBAR_WORKSPACE_NAME_COL as usize)
                ),
                Style::default().fg(p.overlay0).add_modifier(Modifier::DIM),
            )),
            empty.rect,
        );
    }

    for card in cards {
        let i = card.ws_idx;
        let ws = &app.workspaces[i];
        let row_y = card.rect.y;
        let row_height = card.rect.height;
        let selected = i == client_view.selected_workspace && is_navigating;
        let is_active = Some(i) == client_view.active_workspace;
        let is_dragged = dragged_ws_idx == Some(i);
        let highlighted = selected || is_active || is_dragged;

        if highlighted {
            let bg = if selected {
                p.surface0
            } else if is_dragged {
                p.surface1
            } else {
                p.surface_dim
            };
            let buf = frame.buffer_mut();
            for y in row_y..row_y + row_height {
                if y >= list_bottom {
                    break;
                }
                for x in card.rect.x..card.rect.x + card.rect.width {
                    buf[(x, y)].set_style(Style::default().bg(bg));
                }
            }
        }

        if is_active {
            let buf = frame.buffer_mut();
            for y in row_y..row_y + row_height {
                if y >= list_bottom {
                    break;
                }
                buf[(card.rect.x, y)].set_symbol("▌");
                buf[(card.rect.x, y)].set_style(
                    Style::default().fg(active_workspace_accent_color_for_view(app, client_view)),
                );
            }
        }

        let name_style = if selected || is_active || is_dragged {
            Style::default().fg(p.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.subtext0)
        };

        render_workspace_token_rows(
            app,
            frame,
            ws,
            terminal_runtimes,
            card.rect,
            row_y,
            row_height,
            name_style,
        );
    }

    if let Some(y) = insertion_row.filter(|y| *y < list_bottom) {
        let indicator_right = scrollbar_rect
            .map(|rect| rect.x)
            .unwrap_or(area.x + area.width);
        render_drop_indicator(
            frame,
            area.x,
            indicator_right,
            y,
            active_workspace_accent_color_for_view(app, client_view),
        );
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }
}

pub(crate) fn group_selector_rect_for_view(app: &AppState, client_view: &ClientViewState) -> Rect {
    if client_view.computed.layout == crate::app::state::ViewLayout::Mobile {
        return Rect::default();
    }
    if client_view.sidebar_collapsed {
        return crate::ui::collapsed_group_header_rect(client_view.computed.sidebar_rect);
    }
    let sidebar = client_view.computed.sidebar_rect;
    let workspace_area = if client_view.computed.right_sidebar_rect != Rect::default() {
        crate::ui::left_sidebar_workspace_rect(sidebar)
    } else if app.sidebar_arrangement == crate::config::SidebarArrangementConfig::CombinedRight {
        crate::ui::right_aligned_workspace_list_rect(sidebar, client_view.sidebar_section_split)
    } else {
        crate::ui::workspace_list_rect(sidebar, client_view.sidebar_section_split)
    };
    if workspace_area == Rect::default() {
        return Rect::default();
    }
    let label_width = if client_view.group_filter_enabled {
        app.groups
            .get(client_view.active_group)
            .map(|group| format!("{} {}", group.icon, group.name).chars().count() as u16)
            .unwrap_or(3)
    } else {
        3
    };
    let width = label_width.saturating_add(2).min(workspace_area.width);
    Rect::new(
        workspace_area.x + workspace_area.width.saturating_sub(width),
        workspace_area.y,
        width,
        1,
    )
}

fn group_selector_label_for_view(app: &AppState, client_view: &ClientViewState) -> String {
    if client_view.group_filter_enabled {
        let Some(group) = app.groups.get(client_view.active_group) else {
            return "All".to_string();
        };
        return format!("{} {}", group.icon, group.name);
    }

    "All".to_string()
}

fn workspace_summary_spans(
    ws: &crate::workspace::Workspace,
    p: &Palette,
    max_width: usize,
) -> Vec<Span<'static>> {
    let Some(summary) = ws.cached_git_work_summary else {
        return Vec::new();
    };

    if summary.conflicted + summary.added + summary.modified + summary.deleted == 0 {
        if summary.repo_count > 1 {
            return vec![summary_span(
                &format!("{} Repos", summary.repo_count),
                p.overlay0,
                max_width,
            )];
        }
        return Vec::new();
    }

    let mut pieces = Vec::new();
    if summary.repo_count > 1 {
        pieces.push((format!("{} Repos ·", summary.repo_count), p.overlay0));
    }
    if summary.conflicted > 0 {
        pieces.push((format!("!{}", summary.conflicted), p.red));
    }
    if summary.added > 0 {
        pieces.push((format!("+{}", summary.added), p.green));
    }
    if summary.modified > 0 {
        pieces.push((format!("~{}", summary.modified), p.yellow));
    }
    if summary.deleted > 0 {
        pieces.push((format!("-{}", summary.deleted), p.red));
    }

    let mut remaining = max_width;
    let mut spans = Vec::new();
    for (idx, (piece, color)) in pieces.into_iter().enumerate() {
        if remaining == 0 {
            break;
        }
        if idx > 0 {
            spans.push(Span::styled(" ", Style::default().fg(p.overlay0)));
            remaining = remaining.saturating_sub(1);
            if remaining == 0 {
                break;
            }
        }
        let display = truncate_text(&piece, remaining);
        remaining = remaining.saturating_sub(display.chars().count());
        spans.push(Span::styled(display, Style::default().fg(color)));
    }
    spans
}

fn summary_span(text: &str, color: ratatui::style::Color, max_width: usize) -> Span<'static> {
    Span::styled(truncate_text(text, max_width), Style::default().fg(color))
}

fn render_agent_entry(
    app: &AppState,
    frame: &mut Frame,
    show_status: bool,
    show_agent_label: bool,
    detail: &AgentPanelEntry,
    is_active: bool,
    area: Rect,
    row_y: u16,
) {
    let p = &app.palette;
    let row_style = if is_active {
        Style::default().bg(p.surface_dim)
    } else {
        Style::default()
    };
    let name_style = if is_active {
        Style::default().fg(p.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.subtext0)
    };
    let agent_style = Style::default().fg(p.overlay0).add_modifier(Modifier::DIM);
    let mut rows = resolved_agent_rows(app, detail);
    if rows.is_empty() {
        rows.push(Vec::new());
    }

    let mut extra = Vec::new();
    if let Some(group_idx) = detail.group_context_idx {
        if let Some(group) = app.groups.get(group_idx) {
            extra.push(Span::styled(
                group.name.clone(),
                Style::default()
                    .fg(app.group_accent_color(group_idx))
                    .add_modifier(Modifier::DIM),
            ));
        }
    }
    if show_agent_label {
        if let Some(agent_label) = &detail.agent_label {
            if !extra.is_empty() {
                extra.push(Span::styled(" · ", agent_style));
            }
            extra.push(Span::styled(agent_label.clone(), agent_style));
        }
    }
    if show_status {
        if !extra.is_empty() {
            extra.push(Span::styled(" · ", agent_style));
        }
        let label = detail
            .state_labels
            .get(agent_panel_status_key(detail.state, detail.seen))
            .cloned()
            .unwrap_or_else(|| agent_panel_entry_status_label(detail).to_string());
        extra.push(Span::styled(
            label,
            Style::default().fg(state_label_color(detail.state, detail.seen, p)),
        ));
    }
    if let Some(custom_status) = &detail.custom_status {
        if !extra.is_empty() {
            extra.push(Span::styled(" · ", agent_style));
        }
        extra.push(Span::styled(custom_status.clone(), agent_style));
    }
    let age_secs = detail
        .follow_up_added_at_unix_secs
        .or(detail.last_meaningful_agent_activity_unix_secs);
    if let Some(age_label) = format_agent_activity_age(age_secs, current_unix_secs()) {
        if !extra.is_empty() {
            extra.push(Span::styled(" · ", agent_style));
        }
        extra.push(Span::styled(age_label, agent_style));
    }
    if !extra.is_empty() {
        if app.sidebar_config.agents == crate::config::AgentsSidebarConfig::default() {
            rows.push(Vec::new());
        }
        rows.last_mut()
            .unwrap()
            .push(ResolvedToken::Custom(String::new()));
    }

    for (index, row) in rows.iter().enumerate() {
        let line = if row
            .iter()
            .any(|token| matches!(token, ResolvedToken::Custom(value) if value.is_empty()))
        {
            let mut line = agent_token_line(
                &row[..row.len().saturating_sub(1)],
                detail,
                app,
                name_style,
                agent_style,
            );
            line.spans.extend(extra.clone());
            line
        } else {
            agent_token_line(row, detail, app, name_style, agent_style)
        };
        let mut prefixed = right_entry_detail_prefix(p);
        prefixed.extend(line.spans);
        frame.render_widget(
            Paragraph::new(Line::from(prefixed)).style(row_style),
            Rect::new(area.x, row_y.saturating_add(index as u16), area.width, 1),
        );
    }
}

fn render_agent_empty_row(
    app: &AppState,
    frame: &mut Frame,
    label: &'static str,
    area: Rect,
    row_y: u16,
) {
    frame.render_widget(
        Paragraph::new(format!("    {label}")).style(
            Style::default()
                .fg(app.palette.overlay0)
                .add_modifier(Modifier::DIM),
        ),
        Rect::new(area.x, row_y, area.width, 1),
    );
}

pub(crate) fn agent_panel_entry_at_row(
    app: &AppState,
    body: Rect,
    row: u16,
) -> Option<AgentPanelEntry> {
    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut skip = app.agent_panel_scroll;
    let sections = agent_panel_sections(app);
    let show_agent_labels = agent_panel_should_show_agent_labels(&sections);

    for section in sections {
        let collapsed = agent_panel_section_collapsed(app, section.group);
        if collapsed {
            row_y = row_y.saturating_add(1);
            continue;
        }
        let item_count = agent_panel_section_item_count(&section);
        if item_count > 0 && skip >= item_count {
            skip -= item_count;
            continue;
        }
        if row_y >= body_bottom {
            break;
        }

        row_y = row_y.saturating_add(1);
        if agent_panel_empty_row(&section).is_some() {
            row_y = row_y.saturating_add(1);
        }
        let show_status = agent_panel_section_shows_entry_status(section.group);
        for detail in section.entries.iter().skip(skip) {
            let row_height =
                agent_panel_entry_row_height(app, show_status, show_agent_labels, detail);
            if row_y.saturating_add(row_height) > body_bottom {
                break;
            }
            if row >= row_y && row < row_y.saturating_add(row_height) {
                return Some(detail.clone());
            }
            row_y = row_y.saturating_add(row_height);
        }
        skip = 0;
    }

    None
}

fn render_agent_section_header(
    app: &AppState,
    frame: &mut Frame,
    section: &AgentPanelSection,
    collapsed: bool,
    body: Rect,
    row_y: u16,
) {
    if body.width == 0 {
        return;
    }
    let style = agent_panel_section_header_style(section, &app.palette);
    let dim = Style::default().fg(app.palette.overlay0);
    let marker = if collapsed { "▸" } else { "▾" };
    frame.render_widget(
        Paragraph::new(Span::styled(marker, dim)),
        Rect::new(body.x + RIGHT_SUBSECTION_MARKER_COL, row_y, 1, 1),
    );
    let (section_icon, section_icon_style) = agent_panel_section_icon(
        section,
        app.spinner_tick,
        app.status_indicators,
        &app.palette,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(section_icon, section_icon_style)),
        Rect::new(
            body.x + RIGHT_SUBSECTION_ICON_COL.min(body.width.saturating_sub(1)),
            row_y,
            1,
            1,
        ),
    );

    let count_label = if app.show_counters {
        section.entries.len().to_string()
    } else {
        String::new()
    };
    let count_width = count_label.chars().count() as u16;
    let count_reserve = u16::from(app.show_counters)
        .saturating_mul(count_width + RIGHT_SECTION_COUNT_RIGHT_PAD + 1);
    let label_width = body
        .width
        .saturating_sub(RIGHT_SUBSECTION_LABEL_COL + count_reserve);
    frame.render_widget(
        Paragraph::new(Span::styled(
            truncate_text(section.group.label(), label_width as usize),
            style,
        )),
        Rect::new(
            body.x + RIGHT_SUBSECTION_LABEL_COL.min(body.width.saturating_sub(1)),
            row_y,
            label_width,
            1,
        ),
    );
    if app.show_counters && body.width > count_width + RIGHT_SECTION_COUNT_RIGHT_PAD {
        frame.render_widget(
            Paragraph::new(Span::styled(count_label, dim)),
            Rect::new(
                body.x
                    + body
                        .width
                        .saturating_sub(count_width + RIGHT_SECTION_COUNT_RIGHT_PAD),
                row_y,
                count_width,
                1,
            ),
        );
    }
}

pub(crate) fn agent_panel_header_target_at_row(
    app: &AppState,
    body: Rect,
    row: u16,
) -> Option<AgentPanelHeaderTarget> {
    if body == Rect::default() || row < body.y || row >= body.y + body.height {
        return None;
    }

    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut skip = app.agent_panel_scroll;
    let sections = agent_panel_sections(app);
    let show_agent_labels = agent_panel_should_show_agent_labels(&sections);

    for section in sections {
        let collapsed = agent_panel_section_collapsed(app, section.group);
        let item_count = agent_panel_section_item_count(&section);
        if !collapsed && item_count > 0 && skip >= item_count {
            skip -= item_count;
            continue;
        }
        if row_y >= body_bottom {
            break;
        }

        if row == row_y {
            return Some(AgentPanelHeaderTarget {
                section: section.group.label().to_string(),
            });
        }
        row_y = row_y.saturating_add(1);

        if collapsed {
            continue;
        }
        if agent_panel_empty_row(&section).is_some() {
            row_y = row_y.saturating_add(1);
        }
        let show_status = agent_panel_section_shows_entry_status(section.group);
        for detail in section.entries.iter().skip(skip) {
            row_y = row_y.saturating_add(agent_panel_entry_row_height(
                app,
                show_status,
                show_agent_labels,
                detail,
            ));
        }
        skip = 0;
    }

    None
}

pub(crate) fn agent_panel_entry_at_row_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    body: Rect,
    row: u16,
) -> Option<AgentPanelEntry> {
    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut skip = view.agent_panel_scroll;
    let sections = agent_panel_sections_for_view(app, terminal_runtimes, view);
    let show_agent_labels = agent_panel_should_show_agent_labels(&sections);

    for section in sections {
        let collapsed = agent_panel_section_collapsed_for_view(view, section.group);
        if collapsed {
            row_y = row_y.saturating_add(1);
            continue;
        }
        let item_count = agent_panel_section_item_count(&section);
        if item_count > 0 && skip >= item_count {
            skip -= item_count;
            continue;
        }
        if row_y >= body_bottom {
            break;
        }

        row_y = row_y.saturating_add(1);
        if agent_panel_empty_row(&section).is_some() {
            row_y = row_y.saturating_add(1);
        }
        let show_status = agent_panel_section_shows_entry_status(section.group);
        for detail in section.entries.iter().skip(skip) {
            let row_height =
                agent_panel_entry_row_height(app, show_status, show_agent_labels, detail);
            if row_y.saturating_add(row_height) > body_bottom {
                break;
            }
            if row >= row_y && row < row_y.saturating_add(row_height) {
                return Some(detail.clone());
            }
            row_y = row_y.saturating_add(row_height);
        }
        skip = 0;
    }

    None
}

pub(crate) fn agent_panel_header_target_at_row_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    body: Rect,
    row: u16,
) -> Option<AgentPanelHeaderTarget> {
    if body == Rect::default() || row < body.y || row >= body.y + body.height {
        return None;
    }

    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut skip = view.agent_panel_scroll;
    let sections = agent_panel_sections_for_view(app, terminal_runtimes, view);
    let show_agent_labels = agent_panel_should_show_agent_labels(&sections);

    for section in sections {
        let collapsed = agent_panel_section_collapsed_for_view(view, section.group);
        let item_count = agent_panel_section_item_count(&section);
        if !collapsed && item_count > 0 && skip >= item_count {
            skip -= item_count;
            continue;
        }
        if row_y >= body_bottom {
            break;
        }

        if row == row_y {
            return Some(AgentPanelHeaderTarget {
                section: section.group.label().to_string(),
            });
        }
        row_y = row_y.saturating_add(1);

        if collapsed {
            continue;
        }
        if agent_panel_empty_row(&section).is_some() {
            row_y = row_y.saturating_add(1);
        }
        let show_status = agent_panel_section_shows_entry_status(section.group);
        for detail in section.entries.iter().skip(skip) {
            row_y = row_y.saturating_add(agent_panel_entry_row_height(
                app,
                show_status,
                show_agent_labels,
                detail,
            ));
        }
        skip = 0;
    }

    None
}

fn agent_panel_empty_row_at_in_sections(
    app: &AppState,
    sections: &[AgentPanelSection],
    body: Rect,
    row: u16,
    mut skip: usize,
    section_collapsed: impl Fn(AgentStatusGroup) -> bool,
) -> bool {
    if body == Rect::default() || row < body.y || row >= body.y + body.height {
        return false;
    }

    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let show_agent_labels = agent_panel_should_show_agent_labels(sections);
    for section in sections {
        if section_collapsed(section.group) {
            row_y = row_y.saturating_add(1);
            continue;
        }
        let item_count = agent_panel_section_item_count(section);
        if item_count > 0 && skip >= item_count {
            skip -= item_count;
            continue;
        }
        if row_y >= body_bottom {
            break;
        }

        row_y = row_y.saturating_add(1);
        if agent_panel_empty_row(section).is_some() {
            return row_y < body_bottom && row == row_y;
        }
        let show_status = agent_panel_section_shows_entry_status(section.group);
        for detail in section.entries.iter().skip(skip) {
            row_y = row_y.saturating_add(agent_panel_entry_row_height(
                app,
                show_status,
                show_agent_labels,
                detail,
            ));
        }
        skip = 0;
    }

    false
}

pub(crate) fn agent_panel_empty_row_at(app: &AppState, body: Rect, row: u16) -> bool {
    let sections = agent_panel_sections(app);
    agent_panel_empty_row_at_in_sections(
        app,
        &sections,
        body,
        row,
        app.agent_panel_scroll,
        |group| agent_panel_section_collapsed(app, group),
    )
}

pub(crate) fn agent_panel_empty_row_at_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    body: Rect,
    row: u16,
) -> bool {
    let sections = agent_panel_sections_for_view(app, terminal_runtimes, view);
    agent_panel_empty_row_at_in_sections(
        app,
        &sections,
        body,
        row,
        view.agent_panel_scroll,
        |group| agent_panel_section_collapsed_for_view(view, group),
    )
}

fn render_agent_detail_from(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
    leading_separator: bool,
) {
    let p = &app.palette;

    if area.height <= u16::from(leading_separator) {
        return;
    }

    let header_y = area.y;
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Agents",
            Style::default().fg(p.overlay1).add_modifier(Modifier::BOLD),
        )),
        Rect::new(area.x, header_y, area.width, 1),
    );
    let sep_line = "─".repeat(area.width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(&sep_line, Style::default().fg(p.overlay0))),
        Rect::new(area.x, header_y.saturating_add(1), area.width, 1),
    );
    let toggle_rect = agent_panel_toggle_rect(area, app.agent_panel_scope, leading_separator);
    if toggle_rect != Rect::default() {
        let style = Style::default().fg(p.overlay1).bg(p.surface0);
        frame.render_widget(
            Paragraph::new(centered_count_line(
                agent_panel_toggle_label(app.agent_panel_scope),
                toggle_rect.width,
                style,
                style,
            )),
            toggle_rect,
        );
    }

    if !leading_separator && !app.activity_agents_expanded {
        return;
    }

    let metrics = agent_panel_scroll_metrics(app, area, leading_separator);
    let scrollbar_rect = agent_panel_scrollbar_rect(app, area, leading_separator);
    let body = agent_panel_body_rect(area, should_show_scrollbar(metrics), leading_separator);
    if body == Rect::default() {
        return;
    }

    let sections = agent_panel_sections_from(app, terminal_runtimes);
    let follow_up_drop_indicator_row = match app.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::AgentFollowUp {
            drop_indicator_row, ..
        }) => *drop_indicator_row,
        _ => None,
    };
    if sections.is_empty() && body.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " No Agents",
                Style::default().fg(p.overlay0).add_modifier(Modifier::DIM),
            )),
            Rect::new(body.x, body.y, body.width, 1),
        );
        if let Some(track) = scrollbar_rect {
            render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
        }
        return;
    }

    let show_agent_labels = agent_panel_should_show_agent_labels(&sections);
    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut skip = app.agent_panel_scroll;
    for section in sections {
        let collapsed = agent_panel_section_collapsed(app, section.group);
        if collapsed {
            if row_y >= body_bottom {
                break;
            }
            render_agent_section_header(app, frame, &section, true, body, row_y);
            row_y = row_y.saturating_add(1);
            continue;
        }
        let item_count = agent_panel_section_item_count(&section);
        if item_count > 0 && skip >= item_count {
            skip -= item_count;
            continue;
        }
        if row_y >= body_bottom {
            break;
        }

        render_agent_section_header(app, frame, &section, false, body, row_y);
        row_y = row_y.saturating_add(1);
        if let Some(label) = agent_panel_empty_row(&section) {
            if row_y < body_bottom {
                render_agent_empty_row(app, frame, label, body, row_y);
            }
            row_y = row_y.saturating_add(1);
        }
        let show_status = agent_panel_section_shows_entry_status(section.group);

        for detail in section.entries.iter().skip(skip) {
            let row_height =
                agent_panel_entry_row_height(app, show_status, show_agent_labels, detail);
            if row_y.saturating_add(row_height) > body_bottom {
                break;
            }
            render_agent_entry(
                app,
                frame,
                show_status,
                show_agent_labels,
                detail,
                app.is_active_pane(detail.ws_idx, detail.tab_idx, detail.pane_id),
                body,
                row_y,
            );
            row_y = row_y.saturating_add(row_height);
        }
        skip = 0;
    }

    if let Some(row) = follow_up_drop_indicator_row {
        render_drop_indicator(frame, body.x, body.x + body.width, row, p.accent);
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }
}

fn render_agent_detail_from_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    client_view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
    leading_separator: bool,
) {
    let p = &app.palette;

    if area.height <= u16::from(leading_separator) {
        return;
    }

    let header_y = area.y;
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Agents",
            Style::default().fg(p.overlay1).add_modifier(Modifier::BOLD),
        )),
        Rect::new(area.x, header_y, area.width, 1),
    );
    let sep_line = "─".repeat(area.width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(&sep_line, Style::default().fg(p.overlay0))),
        Rect::new(area.x, header_y.saturating_add(1), area.width, 1),
    );
    let toggle_rect =
        agent_panel_toggle_rect(area, client_view.agent_panel_scope, leading_separator);
    if toggle_rect != Rect::default() {
        let style = Style::default().fg(p.overlay1).bg(p.surface0);
        frame.render_widget(
            Paragraph::new(centered_count_line(
                agent_panel_toggle_label(client_view.agent_panel_scope),
                toggle_rect.width,
                style,
                style,
            )),
            toggle_rect,
        );
    }

    if !leading_separator && !app.activity_agents_expanded {
        return;
    }

    let metrics = agent_panel_scroll_metrics_for_view(
        app,
        terminal_runtimes,
        client_view,
        area,
        leading_separator,
    );
    let scrollbar_rect = agent_panel_scrollbar_rect_for_view(
        app,
        terminal_runtimes,
        client_view,
        area,
        leading_separator,
    );
    let body = agent_panel_body_rect(area, should_show_scrollbar(metrics), leading_separator);
    if body == Rect::default() {
        return;
    }

    let sections = agent_panel_sections_for_view(app, terminal_runtimes, client_view);
    let follow_up_drop_indicator_row = match client_view.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::AgentFollowUp {
            drop_indicator_row, ..
        }) => *drop_indicator_row,
        _ => None,
    };
    if sections.is_empty() && body.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " No Agents",
                Style::default().fg(p.overlay0).add_modifier(Modifier::DIM),
            )),
            Rect::new(body.x, body.y, body.width, 1),
        );
        if let Some(track) = scrollbar_rect {
            render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
        }
        return;
    }

    let show_agent_labels = agent_panel_should_show_agent_labels(&sections);
    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut skip = client_view.agent_panel_scroll;
    for section in sections {
        let collapsed = agent_panel_section_collapsed_for_view(client_view, section.group);
        if collapsed {
            if row_y >= body_bottom {
                break;
            }
            render_agent_section_header(app, frame, &section, true, body, row_y);
            row_y = row_y.saturating_add(1);
            continue;
        }
        let item_count = agent_panel_section_item_count(&section);
        if item_count > 0 && skip >= item_count {
            skip -= item_count;
            continue;
        }
        if row_y >= body_bottom {
            break;
        }

        render_agent_section_header(app, frame, &section, false, body, row_y);
        row_y = row_y.saturating_add(1);
        if let Some(label) = agent_panel_empty_row(&section) {
            if row_y < body_bottom {
                render_agent_empty_row(app, frame, label, body, row_y);
            }
            row_y = row_y.saturating_add(1);
        }
        let show_status = agent_panel_section_shows_entry_status(section.group);

        for detail in section.entries.iter().skip(skip) {
            let row_height =
                agent_panel_entry_row_height(app, show_status, show_agent_labels, detail);
            if row_y.saturating_add(row_height) > body_bottom {
                break;
            }
            render_agent_entry(
                app,
                frame,
                show_status,
                show_agent_labels,
                detail,
                agent_entry_is_active_for_view(app, client_view, detail),
                body,
                row_y,
            );
            row_y = row_y.saturating_add(row_height);
        }
        skip = 0;
    }

    if let Some(row) = follow_up_drop_indicator_row {
        render_drop_indicator(frame, body.x, body.x + body.width, row, p.accent);
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }
}

fn agent_panel_scrollbar_rect_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    client_view: &ClientViewState,
    area: Rect,
    leading_separator: bool,
) -> Option<Rect> {
    let metrics = agent_panel_scroll_metrics_for_view(
        app,
        terminal_runtimes,
        client_view,
        area,
        leading_separator,
    );
    let body = agent_panel_body_rect(area, true, leading_separator);
    (should_show_scrollbar(metrics) && body.width > 0 && body.height > 0).then_some(Rect::new(
        area.x + area.width.saturating_sub(1),
        body.y,
        1,
        body.height,
    ))
}

pub(crate) fn collapsed_sidebar_toggle_rect(area: Rect) -> Rect {
    let bottom_y = area.y + area.height.saturating_sub(1);
    let content_w = area.width.saturating_sub(1);
    if content_w == 0 || area.height == 0 {
        return Rect::default();
    }
    let x = area.x + content_w / 2;
    Rect::new(x, bottom_y, 1, 1)
}

pub(crate) fn collapsed_sidebar_launcher_rect(area: Rect) -> Rect {
    let toggle = collapsed_sidebar_toggle_rect(area);
    if toggle == Rect::default() || toggle.y == area.y {
        return Rect::default();
    }
    Rect::new(toggle.x, toggle.y - 1, 1, 1)
}

pub(crate) fn expanded_sidebar_toggle_rect(area: Rect) -> Rect {
    let bottom_y = area.y + area.height.saturating_sub(1);
    let content_w = area.width.saturating_sub(1);
    if content_w == 0 || area.height == 0 {
        return Rect::default();
    }
    Rect::new(area.x + content_w.saturating_sub(1), bottom_y, 1, 1)
}

fn render_sidebar_toggle(
    _app: &AppState,
    frame: &mut Frame,
    area: Rect,
    collapsed: bool,
    p: &Palette,
) {
    let toggle_area = if collapsed {
        collapsed_sidebar_toggle_rect(area)
    } else {
        expanded_sidebar_toggle_rect(area)
    };
    if toggle_area == Rect::default() {
        return;
    }
    let icon_style = Style::default().fg(p.overlay0);
    let icon = if collapsed { "»" } else { "«" };
    frame.render_widget(Paragraph::new(Span::styled(icon, icon_style)), toggle_area);
}

pub(crate) fn right_sidebar_toggle_rect(area: Rect, collapsed: bool) -> Rect {
    let bottom_y = area.y + area.height.saturating_sub(1);
    let content_w = area.width.saturating_sub(1);
    if content_w == 0 || area.height == 0 {
        return Rect::default();
    }

    if collapsed {
        Rect::new(area.x + 1 + content_w / 2, bottom_y, 1, 1)
    } else {
        Rect::new(area.x + 1, bottom_y, 1, 1)
    }
}

fn render_right_sidebar_toggle(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    collapsed: bool,
    p: &Palette,
) {
    let toggle_area = right_sidebar_toggle_rect(area, collapsed);
    if toggle_area == Rect::default() {
        return;
    }
    let icon_style = if app.update_available.is_some() {
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.overlay0)
    };
    let icon = if collapsed { "«" } else { "»" };
    frame.render_widget(Paragraph::new(Span::styled(icon, icon_style)), toggle_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::state::Group, detect::Agent, workspace::Workspace};
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    #[test]
    fn workspace_badge_reports_mixed_local_and_remote_hosts() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("mixed");
        let remote_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);
        let remote_terminal_id = workspace.tabs[0].panes[&remote_pane]
            .attached_terminal_id
            .clone();
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        app.terminals.get_mut(&remote_terminal_id).unwrap().location =
            crate::execution_host::ResourceLocation::new(
                crate::execution_host::ExecutionHostId::new("ssh:workbox:1").unwrap(),
                crate::execution_host::HostPath::new("/srv/work").unwrap(),
            );

        assert_eq!(
            workspace_host_badge(&app, &app.workspaces[0])
                .map(|(label, _)| label)
                .as_deref(),
            Some("ssh:workbox:1 · test-host")
        );
    }

    #[test]
    fn offline_remote_badge_does_not_hide_local_workspace_state() {
        let mut app = crate::app::state::AppState::test_new();
        let local = Workspace::test_new("local");
        let remote = Workspace::test_new("remote");
        let remote_terminal_id = remote.tabs[0].panes[&remote.tabs[0].root_pane]
            .attached_terminal_id
            .clone();
        app.workspaces = vec![local, remote];
        app.ensure_test_terminals();
        let host_id = crate::execution_host::ExecutionHostId::new("ssh:workbox:1").unwrap();
        app.terminals.get_mut(&remote_terminal_id).unwrap().location =
            crate::execution_host::ResourceLocation::new(
                host_id.clone(),
                crate::execution_host::HostPath::new("/srv/work").unwrap(),
            );
        app.host_connection_states.insert(
            host_id,
            crate::execution_host::ConnectionStatus::Disconnected,
        );

        assert_eq!(
            workspace_host_badge(&app, &app.workspaces[0])
                .map(|(label, _)| label)
                .as_deref(),
            Some("test-host")
        );
        assert_eq!(
            workspace_host_badge(&app, &app.workspaces[1])
                .map(|(label, _)| label)
                .as_deref(),
            Some("ssh:workbox:1 · Offline")
        );
    }

    #[test]
    fn empty_workspace_badge_uses_default_location_host() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("empty");
        app.workspaces = vec![workspace];
        assert_eq!(
            workspace_host_badge(&app, &app.workspaces[0])
                .map(|(label, _)| label)
                .as_deref(),
            Some("test-host")
        );
    }

    #[test]
    fn agent_panel_toggle_labels_match_control_center_scope() {
        assert_eq!(
            agent_panel_toggle_label(AgentPanelScope::CurrentWorkspace),
            "Space"
        );
        assert_eq!(
            agent_panel_toggle_label(AgentPanelScope::CurrentGroup),
            "Group"
        );
        assert_eq!(
            agent_panel_toggle_label(AgentPanelScope::AllWorkspaces),
            "All"
        );
    }

    #[test]
    fn client_workspace_header_reflects_group_filter_scope() {
        let app = crate::app::state::AppState::test_new();
        let runtimes = TerminalRuntimeRegistry::new();
        let area = Rect::new(0, 0, 28, 12);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let mut client = ClientViewState::from_default_client_state(&app);
        client.group_filter_enabled = false;

        terminal
            .draw(|frame| {
                render_workspace_list_from_for_view(&app, &runtimes, &client, frame, area, false);
            })
            .expect("render all-groups workspace list");
        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text
            .lines()
            .next()
            .is_some_and(|line| line.starts_with("Groups")));

        client.group_filter_enabled = true;
        terminal
            .draw(|frame| {
                render_workspace_list_from_for_view(&app, &runtimes, &client, frame, area, false);
            })
            .expect("render group workspace list");
        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text
            .lines()
            .next()
            .is_some_and(|line| line.starts_with("Spaces")));
    }

    #[test]
    fn workspace_list_empty_state_mentions_empty_group() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("hidden")];
        app.create_group("work".to_string());
        app.active_group = 1;
        app.group_filter_enabled = true;

        let backend = TestBackend::new(28, 12);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_workspace_list(&app, frame, Rect::new(0, 0, 28, 12), false))
            .expect("render workspace list");

        let text = buffer_text(terminal.backend().buffer(), 28, 12);
        let rows = text.lines().collect::<Vec<_>>();
        assert!(text.contains("Empty Group"));
        assert!(rows[2].contains("Empty Group"));
        assert!(!text.contains("New Space Adds One Here"));
    }

    #[test]
    fn workspace_list_does_not_render_footer_actions() {
        let mut app = crate::app::state::AppState::test_new();
        app.mouse_capture = true;
        app.workspaces = vec![Workspace::test_new("visible")];
        app.active = Some(0);
        app.selected = 0;

        let backend = TestBackend::new(28, 12);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_workspace_list(&app, frame, Rect::new(0, 0, 28, 12), false))
            .expect("render workspace list");

        let text = buffer_text(terminal.backend().buffer(), 28, 12);
        assert!(!text.contains("New Space"));
        assert!(!text.contains(" New"));
        assert!(!text.contains("Menu"));
    }

    #[test]
    fn all_spaces_workspace_list_groups_rows_by_group() {
        let mut app = crate::app::state::AppState::test_new();
        app.group_filter_enabled = false;
        app.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_location: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.workspaces = vec![Workspace::test_new("home"), Workspace::test_new("api")];
        app.workspaces[1].group_id = "work".into();
        let area = Rect::new(0, 0, 32, 14);
        app.view.workspace_card_areas = compute_workspace_card_areas_in_list(&app, area);
        app.view.workspace_group_header_areas =
            compute_workspace_group_header_areas_in_list(&app, area);
        app.view.workspace_group_empty_areas =
            compute_workspace_group_empty_areas_in_list(&app, area);

        let backend = TestBackend::new(32, 14);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_workspace_list(&app, frame, area, false))
            .expect("render workspace list");

        let text = buffer_text(terminal.backend().buffer(), 32, 14);
        assert!(text.contains("▾ ☀ Group 1"));
        assert!(text.contains("▾ ■ work"));
        assert!(text.contains("home"));
        assert!(text.contains("api"));

        let buffer = terminal.backend().buffer();
        let home_card = app
            .view
            .workspace_card_areas
            .iter()
            .find(|card| card.ws_idx == 0)
            .expect("home card")
            .rect;
        let group_header = app
            .view
            .workspace_group_header_areas
            .iter()
            .find(|header| header.group_idx == 0)
            .expect("default group header")
            .rect;
        assert_eq!(
            buffer[(group_header.x + SIDEBAR_GROUP_NAME_COL, group_header.y)].symbol(),
            "G"
        );
        assert_eq!(
            buffer[(group_header.x + group_header.width - 2, group_header.y)].symbol(),
            " "
        );
        assert_eq!(
            buffer[(home_card.x + SIDEBAR_WORKSPACE_NAME_COL, home_card.y)].symbol(),
            "h"
        );
        assert_eq!(
            buffer[(home_card.x + SIDEBAR_WORKSPACE_STATE_COL, home_card.y + 1)].symbol(),
            " "
        );
        assert_eq!(
            buffer[(home_card.x + SIDEBAR_WORKSPACE_NAME_COL, home_card.y + 1)].symbol(),
            "t"
        );

        app.show_counters = true;
        terminal
            .draw(|frame| render_workspace_list(&app, frame, area, false))
            .expect("render workspace list with counters");
        assert_eq!(
            terminal.backend().buffer()[(group_header.x + group_header.width - 2, group_header.y)]
                .symbol(),
            "1"
        );
    }

    #[test]
    fn all_spaces_group_header_uses_group_accent() {
        let mut app = crate::app::state::AppState::test_new();
        app.group_filter_enabled = false;
        app.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: Some(crate::config::TerminalAccent::Red),
            default_location: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.workspaces = vec![Workspace::test_new("home"), Workspace::test_new("api")];
        app.workspaces[1].group_id = "work".into();
        let area = Rect::new(0, 0, 32, 14);
        app.view.workspace_card_areas = compute_workspace_card_areas_in_list(&app, area);
        app.view.workspace_group_header_areas =
            compute_workspace_group_header_areas_in_list(&app, area);
        app.view.workspace_group_empty_areas =
            compute_workspace_group_empty_areas_in_list(&app, area);
        let work_header = app
            .view
            .workspace_group_header_areas
            .iter()
            .find(|header| header.group_idx == 1)
            .expect("work group header")
            .rect;

        let backend = TestBackend::new(32, 14);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_workspace_list(&app, frame, area, false))
            .expect("render workspace list");

        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(work_header.x + SIDEBAR_GROUP_ICON_COL, work_header.y)].symbol(),
            "■"
        );
        assert_eq!(
            buffer[(work_header.x + SIDEBAR_GROUP_ICON_COL, work_header.y)]
                .style()
                .fg,
            Some(app.group_accent_color(1))
        );
        assert_eq!(
            buffer[(work_header.x + SIDEBAR_GROUP_NAME_COL, work_header.y)]
                .style()
                .fg,
            Some(app.group_accent_color(1))
        );
    }

    #[test]
    fn collapsed_agent_status_hover_shows_icon_and_name() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("checkout");
        let pane_id = workspace.tabs[0].root_pane;
        let pane = workspace.tabs[0]
            .panes
            .get_mut(&pane_id)
            .expect("root pane");
        pane.detected_agent = Some(Agent::Pi);
        pane.state = AgentState::Working;
        pane.seen = true;
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.sidebar_collapsed = true;
        app.sidebar_arrangement = crate::config::SidebarArrangementConfig::CombinedLeft;
        app.view.sidebar_rect = Rect::new(0, 0, 4, 24);
        app.collapsed_sidebar_hover = Some(CollapsedSidebarHover::AgentStatus {
            section: "Working".to_string(),
        });

        let backend = TestBackend::new(60, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_collapsed_sidebar_hover(&app, frame))
            .expect("render collapsed status hover");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, 60, 24);
        assert!(
            text.contains("Working"),
            "status hover should name the group; rendered:\n{text}"
        );
        let (x, y) = first_cell_with_symbol(buffer, 60, 24, "W").expect("Working label");
        assert_eq!(
            buffer[(x, y)].style().fg,
            Some(app.palette.yellow),
            "status hover should use the section color"
        );
    }

    #[test]
    fn collapsed_sidebar_group_icon_uses_group_accent() {
        let mut app = crate::app::state::AppState::test_new();
        let group_idx = app.create_group("work".to_string());
        app.groups[group_idx].icon = "■".to_string();
        app.set_group_accent(group_idx, Some(crate::config::TerminalAccent::Cyan));
        app.active_group = group_idx;
        app.group_filter_enabled = true;

        let backend = TestBackend::new(8, 8);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, Rect::new(0, 0, 8, 8)))
            .expect("render collapsed sidebar");

        let buffer = terminal.backend().buffer();
        let (x, y) = first_cell_with_symbol(buffer, 8, 8, "■").expect("group icon");
        assert_eq!(
            buffer[(x, y)].style().fg,
            Some(app.group_accent_color(group_idx))
        );
    }

    #[test]
    fn collapsed_agent_hover_shows_space_agent_and_status() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("checkout");
        let pane_id = workspace.tabs[0].root_pane;
        let pane = workspace.tabs[0]
            .panes
            .get_mut(&pane_id)
            .expect("root pane");
        pane.detected_agent = Some(Agent::Pi);
        pane.state = AgentState::Working;
        pane.seen = true;
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;
        app.sidebar_collapsed = true;
        app.sidebar_arrangement = crate::config::SidebarArrangementConfig::CombinedLeft;
        app.view.sidebar_rect = Rect::new(0, 0, 4, 24);
        app.collapsed_sidebar_hover = Some(CollapsedSidebarHover::Agent { ws_idx: 0, pane_id });

        let backend = TestBackend::new(60, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_collapsed_sidebar_hover(&app, frame))
            .expect("render collapsed agent hover");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, 60, 24);
        assert!(
            text.contains("checkout"),
            "hover should name the space; rendered:\n{text}"
        );
        assert!(
            text.contains("pi"),
            "hover should name the agent; rendered:\n{text}"
        );
        assert!(
            text.contains("Working"),
            "hover should show status; rendered:\n{text}"
        );
        let (status_x, status_y) =
            first_cell_with_symbol(buffer, 60, 24, "W").expect("Working status");
        assert_eq!(
            buffer[(status_x, status_y)].style().fg,
            Some(state_label_color(AgentState::Working, true, &app.palette))
        );
    }

    #[test]
    fn collapsed_sidebar_stacks_help_above_expand_control() {
        let app = crate::app::state::AppState::test_new();
        let mut view = ClientViewState::from_default_client_state(&app);
        view.sidebar_collapsed = true;
        let area = Rect::new(0, 0, 4, 8);
        view.computed.sidebar_rect = area;
        let runtimes = TerminalRuntimeRegistry::new();
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");

        terminal
            .draw(|frame| {
                render_sidebar_collapsed_for_view(&app, &runtimes, &view, frame, area);
            })
            .expect("render collapsed sidebar footer");

        let launcher = global_launcher_rect_for_view(&app, &view);
        let toggle = collapsed_sidebar_toggle_rect(area);
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(launcher.x, launcher.y)].symbol(), "?");
        assert_eq!(buffer[(toggle.x, toggle.y)].symbol(), "»");
        assert_eq!(launcher.x, toggle.x);
        assert_eq!(launcher.y + 1, toggle.y);
    }

    #[test]
    fn collapsed_left_rail_renders_header_divider_before_workspace_rows() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.active = Some(0);
        app.selected = 0;
        app.group_filter_enabled = false;

        let area = Rect::new(0, 0, 4, 20);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("render collapsed sidebar");

        let buffer = terminal.backend().buffer();
        let rows = buffer_text(buffer, area.width, area.height)
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();

        assert_eq!(rows[0], "All│");
        assert_eq!(rows[1], "───│");
        assert_eq!(buffer[(0, 2)].symbol(), "▾");
        assert_eq!(
            buffer[(2, 2)].symbol(),
            crate::app::state::DEFAULT_GROUP_ICON
        );
        assert_eq!(buffer[(0, 3)].symbol(), "1");
        assert_eq!(buffer[(0, 4)].symbol(), "2");
        assert_eq!(buffer[(3, 2)].symbol(), "│");
        assert_eq!(buffer[(3, 4)].symbol(), "│");
    }

    #[test]
    fn separate_collapsed_spaces_rail_uses_full_height_without_agent_rows() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = (0..10)
            .map(|idx| Workspace::test_new(&format!("space-{idx}")))
            .collect();
        app.active = Some(0);
        app.selected = 0;
        app.group_filter_enabled = false;
        app.sidebar_collapsed = true;
        app.sidebar_arrangement = crate::config::SidebarArrangementConfig::Separate;
        app.view.right_sidebar_rect = Rect::new(80, 0, 4, 20);

        let area = Rect::new(0, 0, 4, 20);
        let tenth_workspace_row = area.y + COLLAPSED_SECTION_HEADER_ROWS + 10;

        assert_eq!(
            collapsed_workspace_at_row(&app, area, tenth_workspace_row),
            Some(9),
            "separate agent sidebar should leave the full left rail to spaces"
        );
    }

    #[test]
    fn collapsed_sidebar_keeps_workspace_status_visible_for_two_digit_positions() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = (1..=10)
            .map(|idx| Workspace::test_new(&format!("workspace-{idx}")))
            .collect();
        app.ensure_test_terminals();

        for ws_idx in 0..app.workspaces.len() {
            let pane = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);
        }

        let area = Rect::new(0, 0, 4, 25);
        let (ws_area, _, _) = collapsed_sidebar_sections(area, true);
        let rows_y = ws_area.y + COLLAPSED_SECTION_HEADER_ROWS;
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("collapsed sidebar should render");

        let tenth_row = rows_y + 9;
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(ws_area.x, rows_y)].symbol(), "1");
        assert_eq!(buffer[(ws_area.x + 1, rows_y)].symbol(), "·");
        assert_ne!(
            buffer[(ws_area.x + 2, rows_y)].symbol(),
            "·",
            "single-digit status should leave a gutter before the rail border"
        );
        assert_eq!(buffer[(ws_area.x, tenth_row)].symbol(), "1");
        assert_eq!(buffer[(ws_area.x + 1, tenth_row)].symbol(), "0");
        assert_eq!(buffer[(ws_area.x + 2, tenth_row)].symbol(), "·");
    }

    #[test]
    fn collapsed_sidebar_blocked_status_does_not_overwrite_border() {
        let mut app = crate::app::state::AppState::test_new();
        app.status_indicators = crate::config::StatusIndicatorStyle::Symbols;
        let mut workspace = Workspace::test_new("blocked");
        let pane_id = workspace.tabs[0].root_pane;
        {
            let pane = workspace.tabs[0]
                .panes
                .get_mut(&pane_id)
                .expect("root pane");
            pane.detected_agent = Some(Agent::Claude);
            pane.state = AgentState::Blocked;
            pane.seen = true;
        }
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        let attached = app
            .terminals
            .get_mut(&terminal_id)
            .expect("attached terminal");
        attached.detected_agent = Some(Agent::Claude);
        attached.state = AgentState::Blocked;

        let area = Rect::new(0, 0, 4, 12);
        let (ws_area, _, _) = collapsed_sidebar_sections(area, true);
        let rows_y = ws_area.y + COLLAPSED_SECTION_HEADER_ROWS;
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("collapsed sidebar should render");

        let buffer = terminal.backend().buffer();
        let border_x = area.x + area.width - 1;
        assert_eq!(buffer[(ws_area.x, rows_y)].symbol(), "1");
        assert_eq!(buffer[(ws_area.x + 1, rows_y)].symbol(), "●");
        assert_eq!(buffer[(border_x, rows_y)].symbol(), "│");
    }

    #[test]
    fn collapsed_workspace_hover_shows_group_name_and_colored_status() {
        let mut app = crate::app::state::AppState::test_new();
        app.groups[0].name = "Core".to_string();
        app.set_group_accent(0, Some(crate::config::TerminalAccent::Cyan));
        let mut workspace = Workspace::test_new("desktop-client");
        let pane_id = workspace.tabs[0].root_pane;
        let pane = workspace.tabs[0]
            .panes
            .get_mut(&pane_id)
            .expect("root pane");
        pane.detected_agent = Some(Agent::Codex);
        pane.state = AgentState::Working;
        pane.seen = true;
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;
        app.sidebar_collapsed = true;
        app.sidebar_arrangement = crate::config::SidebarArrangementConfig::CombinedLeft;
        app.view.sidebar_rect = Rect::new(0, 0, 4, 20);
        app.collapsed_sidebar_hover = Some(CollapsedSidebarHover::Workspace(0));

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_collapsed_sidebar_hover(&app, frame))
            .expect("render collapsed sidebar hover");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, 60, 20);
        assert!(text.contains("Core"));
        assert!(text.contains("desktop-client · Working"));
        let (group_x, group_y) = first_cell_with_symbol(buffer, 60, 20, "C").expect("group name");
        assert_eq!(
            buffer[(group_x, group_y)].style().fg,
            Some(app.group_accent_color(0))
        );
        let (status_x, status_y) =
            first_cell_with_symbol(buffer, 60, 20, "W").expect("Working status");
        assert_eq!(
            buffer[(status_x, status_y)].style().fg,
            Some(state_label_color(AgentState::Working, true, &app.palette))
        );
    }

    #[test]
    fn collapsed_group_hover_uses_group_accent() {
        let mut app = crate::app::state::AppState::test_new();
        app.groups[0].name = "Archive".to_string();
        app.groups[0].icon = "✿".to_string();
        app.set_group_accent(0, Some(crate::config::TerminalAccent::Magenta));
        app.group_filter_enabled = false;
        app.sidebar_collapsed = true;
        app.sidebar_arrangement = crate::config::SidebarArrangementConfig::CombinedLeft;
        app.view.sidebar_rect = Rect::new(0, 0, 4, 20);
        app.collapsed_sidebar_hover = Some(CollapsedSidebarHover::Group(0));

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_collapsed_sidebar_hover(&app, frame))
            .expect("render collapsed group hover");

        let buffer = terminal.backend().buffer();
        assert!(buffer_text(buffer, 60, 20).contains("Archive"));
        let (x, y) = first_cell_with_symbol(buffer, 60, 20, "A").expect("group name");
        assert_eq!(buffer[(x, y)].style().fg, Some(app.group_accent_color(0)));
    }

    #[test]
    fn collapsed_all_groups_rail_resets_ordinals_after_each_group_header() {
        let mut app = crate::app::state::AppState::test_new();
        app.group_filter_enabled = false;
        app.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_location: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.workspaces = vec![
            Workspace::test_new("home"),
            Workspace::test_new("notes"),
            Workspace::test_new("api"),
        ];
        app.workspaces[2].group_id = "work".into();

        let area = Rect::new(0, 0, 4, 20);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("render collapsed sidebar");

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 2)].symbol(), "▾");
        assert_eq!(
            buffer[(2, 2)].symbol(),
            crate::app::state::DEFAULT_GROUP_ICON
        );
        assert_eq!(buffer[(0, 3)].symbol(), "1");
        assert_eq!(buffer[(0, 4)].symbol(), "2");
        assert_eq!(buffer[(0, 5)].symbol(), "▾");
        assert_eq!(buffer[(2, 5)].symbol(), "■");
        assert_eq!(buffer[(0, 6)].symbol(), "1");
    }

    #[test]
    fn collapsed_all_groups_rail_keeps_collapsed_group_header_and_hides_its_ordinals() {
        let mut app = crate::app::state::AppState::test_new();
        app.group_filter_enabled = false;
        app.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_location: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.collapsed_workspace_groups.push("work".into());
        app.workspaces = vec![Workspace::test_new("home"), Workspace::test_new("api")];
        app.workspaces[1].group_id = "work".into();

        let area = Rect::new(0, 0, 4, 20);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("render collapsed sidebar");

        let buffer = terminal.backend().buffer();
        let rows = buffer_text(buffer, area.width, area.height)
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();

        assert_eq!(buffer[(0, 2)].symbol(), "▾");
        assert_eq!(
            buffer[(2, 2)].symbol(),
            crate::app::state::DEFAULT_GROUP_ICON
        );
        assert_eq!(buffer[(0, 3)].symbol(), "1");
        assert_eq!(buffer[(0, 4)].symbol(), "▸");
        assert_eq!(buffer[(2, 4)].symbol(), "■");
        assert_eq!(rows[5], "   │");
    }

    #[test]
    fn collapsed_sidebar_matches_expanded_agent_divider_y_without_pre_header_separator() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.active = Some(0);
        app.selected = 0;

        let area = Rect::new(0, 0, 8, 20);
        let (_, expanded_agent_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        let (_, _, collapsed_agent_area) = collapsed_sidebar_sections(area, true);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("render collapsed sidebar");

        let buffer = terminal.backend().buffer();
        let row_above_agent_header = collapsed_agent_area.y.saturating_sub(1);

        assert_eq!(
            collapsed_agent_area.y + 1,
            expanded_agent_area.y + 1,
            "collapsed agent header divider should stay at the same Y as expanded for the same rect/split"
        );
        for x in collapsed_agent_area.x..collapsed_agent_area.x + collapsed_agent_area.width {
            assert_ne!(
                buffer[(x, row_above_agent_header)].symbol(),
                "─",
                "collapsed sidebar should not draw a separator row immediately above the agent scope header"
            );
        }
    }

    #[test]
    fn collapsed_left_agent_area_renders_scope_and_clickable_status_rows() {
        let mut app = crate::app::state::AppState::test_new();

        let mut triage = Workspace::test_new("Done");
        let triage_pane = triage.tabs[0].root_pane;
        let triage_state = triage.tabs[0].panes.get_mut(&triage_pane).unwrap();
        triage_state.detected_agent = Some(Agent::Pi);
        triage_state.state = AgentState::Idle;
        triage_state.seen = false;

        let mut working = Workspace::test_new("build");
        let working_pane = working.tabs[0].root_pane;
        let working_state = working.tabs[0].panes.get_mut(&working_pane).unwrap();
        working_state.detected_agent = Some(Agent::Codex);
        working_state.state = AgentState::Working;

        let mut idle = Workspace::test_new("Idle");
        let idle_pane = idle.tabs[0].root_pane;
        let idle_state = idle.tabs[0].panes.get_mut(&idle_pane).unwrap();
        idle_state.detected_agent = Some(Agent::Claude);
        idle_state.state = AgentState::Idle;
        idle_state.seen = true;

        app.workspaces = vec![triage, working, idle];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        app.collapsed_agent_sections.push("Working".to_string());

        let area = Rect::new(0, 0, 8, 24);
        let (_, _, detail_area) = collapsed_sidebar_sections(area, true);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("render collapsed sidebar");

        let buffer = terminal.backend().buffer();
        let rows = buffer_text(buffer, area.width, area.height)
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let agent_header = &rows[detail_area.y as usize];
        let triage_header_row = detail_area.y + 2;
        let triage_agent_row = triage_header_row + 1;
        let follow_up_header_row = triage_agent_row + 1;
        let working_header_row = follow_up_header_row + 1;
        let idle_header_row = working_header_row + 1;
        let idle_agent_row = idle_header_row + 1;

        assert!(
            agent_header.contains("All"),
            "collapsed agent header should expose the scope affordance; rendered row: {agent_header:?}"
        );
        assert!(
            !agent_header.contains("agt"),
            "collapsed agent header should not render the static agt label; rendered row: {agent_header:?}"
        );
        assert_eq!(buffer[(detail_area.x, triage_header_row)].symbol(), "▾");
        assert_eq!(buffer[(detail_area.x + 2, triage_agent_row)].symbol(), "1");
        assert_eq!(buffer[(detail_area.x, follow_up_header_row)].symbol(), "▾");
        assert_eq!(
            collapsed_agent_panel_header_target_at_row(&app, detail_area, follow_up_header_row)
                .map(|target| target.section),
            Some("Follow Up".to_string())
        );
        assert_eq!(buffer[(detail_area.x, working_header_row)].symbol(), "▸");
        assert_eq!(
            buffer[(detail_area.x + 2, working_header_row)].symbol(),
            agent_section_icon(
                AgentStatusGroup::Working,
                app.spinner_tick,
                app.status_indicators,
                &app.palette,
            )
            .0
        );
        assert_eq!(buffer[(detail_area.x, idle_header_row)].symbol(), "▾");
        assert_eq!(buffer[(detail_area.x + 2, idle_agent_row)].symbol(), "3");
        assert_eq!(
            collapsed_agent_panel_header_target_at_row(&app, detail_area, working_header_row)
                .map(|target| target.section),
            Some("Working".to_string())
        );
        assert!(
            collapsed_agent_panel_entry_at_row(&app, detail_area, working_header_row + 1).is_none(),
            "collapsed working section should hide its agent row"
        );
    }

    #[test]
    fn collapsed_agent_scope_labels_use_consistent_text_abbreviations() {
        let mut app = crate::app::state::AppState::test_new();
        app.sidebar_collapsed = true;
        app.workspaces = vec![Workspace::test_new("one")];
        app.active = Some(0);
        app.selected = 0;
        app.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "*".into(),
            accent: None,
            default_location: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.workspaces[0].group_id = "work".into();
        app.active_group = 1;

        let area = Rect::new(0, 0, 8, 24);
        let (_, _, detail_area) = collapsed_sidebar_sections(area, true);

        for (scope, expected) in [
            (AgentPanelScope::AllWorkspaces, "All"),
            (AgentPanelScope::CurrentGroup, "f:g"),
            (AgentPanelScope::CurrentWorkspace, "f:s"),
        ] {
            app.agent_panel_scope = scope;
            let backend = TestBackend::new(area.width, area.height);
            let mut terminal = Terminal::new(backend).expect("test backend");
            terminal
                .draw(|frame| render_sidebar_collapsed(&app, frame, area))
                .expect("render collapsed sidebar");
            let row = buffer_text(terminal.backend().buffer(), area.width, area.height)
                .lines()
                .nth(detail_area.y as usize)
                .expect("detail header row")
                .to_string();

            assert!(
                row.contains(expected),
                "{scope:?} should render compact text label {expected:?}; row: {row:?}"
            );
            assert!(
                !row.contains('*'),
                "{scope:?} should not switch to a group icon while the other scopes use text; row: {row:?}"
            );
        }
    }

    #[test]
    fn collapsed_all_agent_ordinals_use_workspace_group_accent() {
        let mut app = crate::app::state::AppState::test_new();
        app.sidebar_collapsed = true;

        let default_group_id = app.groups[0].id.clone();
        app.groups.push(Group {
            id: "ops".into(),
            name: "ops".into(),
            icon: "o".into(),
            accent: None,
            default_location: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });

        let mut triage = Workspace::test_new("Triage");
        triage.group_id = default_group_id;
        let triage_pane = triage.tabs[0].root_pane;
        let triage_state = triage.tabs[0].panes.get_mut(&triage_pane).unwrap();
        triage_state.detected_agent = Some(Agent::Pi);
        triage_state.state = AgentState::Idle;
        triage_state.seen = false;

        let mut working = Workspace::test_new("Working");
        working.group_id = app.groups[1].id.clone();
        let working_pane = working.tabs[0].root_pane;
        let working_state = working.tabs[0].panes.get_mut(&working_pane).unwrap();
        working_state.detected_agent = Some(Agent::Codex);
        working_state.state = AgentState::Working;

        app.workspaces = vec![triage, working];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let area = Rect::new(0, 0, 8, 24);
        let (_, _, detail_area) = collapsed_sidebar_sections(area, true);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("render sidebar");
        let buffer = terminal.backend().buffer();

        let triage_header_row = detail_area.y + 2;
        let triage_agent_row = triage_header_row + 1;
        let follow_up_header_row = triage_agent_row + 1;
        let working_header_row = follow_up_header_row + 1;
        let working_agent_row = working_header_row + 1;
        assert_eq!(
            collapsed_agent_panel_header_target_at_row(&app, detail_area, follow_up_header_row)
                .map(|target| target.section),
            Some("Follow Up".to_string())
        );
        assert_eq!(
            buffer[(detail_area.x + 2, triage_agent_row)].style().fg,
            Some(app.group_accent_color(0))
        );
        assert_eq!(
            buffer[(detail_area.x + 2, working_agent_row)].style().fg,
            Some(app.group_accent_color(1))
        );
        assert_eq!(
            buffer[(detail_area.x + 2, triage_header_row)].style().fg,
            agent_section_icon(
                AgentStatusGroup::Triage,
                app.spinner_tick,
                app.status_indicators,
                &app.palette,
            )
            .1
            .fg
        );
        assert_eq!(
            buffer[(detail_area.x + 2, working_header_row)].style().fg,
            agent_section_icon(
                AgentStatusGroup::Working,
                app.spinner_tick,
                app.status_indicators,
                &app.palette,
            )
            .1
            .fg
        );
    }

    #[test]
    fn collapsed_workspace_group_hides_its_rows() {
        let mut app = crate::app::state::AppState::test_new();
        app.group_filter_enabled = false;
        app.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_location: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.collapsed_workspace_groups.push("work".into());
        app.workspaces = vec![Workspace::test_new("home"), Workspace::test_new("api")];
        app.workspaces[1].group_id = "work".into();
        let area = Rect::new(0, 0, 32, 14);
        app.view.workspace_card_areas = compute_workspace_card_areas_in_list(&app, area);
        app.view.workspace_group_header_areas =
            compute_workspace_group_header_areas_in_list(&app, area);
        app.view.workspace_group_empty_areas =
            compute_workspace_group_empty_areas_in_list(&app, area);

        let backend = TestBackend::new(32, 14);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_workspace_list(&app, frame, area, false))
            .expect("render workspace list");
        let text = buffer_text(terminal.backend().buffer(), 32, 14);
        assert!(text.contains("▸ ■ work"));
        assert!(text.contains("home"));
        assert!(!text.contains("api"));
    }

    #[test]
    fn empty_groups_render_empty_row_in_all_spaces() {
        let mut app = crate::app::state::AppState::test_new();
        app.group_filter_enabled = false;
        app.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_location: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.workspaces = vec![Workspace::test_new("home")];
        let area = Rect::new(0, 0, 32, 14);
        app.view.workspace_card_areas = compute_workspace_card_areas_in_list(&app, area);
        app.view.workspace_group_header_areas =
            compute_workspace_group_header_areas_in_list(&app, area);
        app.view.workspace_group_empty_areas =
            compute_workspace_group_empty_areas_in_list(&app, area);

        let backend = TestBackend::new(32, 14);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_workspace_list(&app, frame, area, false))
            .expect("render workspace list");

        let text = buffer_text(terminal.backend().buffer(), 32, 14);
        assert!(text.contains("▾ ■ work"));
        assert!(text.contains("No Spaces"));
    }

    #[test]
    fn activity_age_label_uses_compact_units() {
        let now = 1_000_000;

        assert_eq!(
            format_agent_activity_age(Some(now - 59), now),
            Some("Now".to_string())
        );
        assert_eq!(
            format_agent_activity_age(Some(now - 60), now),
            Some("1m".to_string())
        );
        assert_eq!(
            format_agent_activity_age(Some(now - 7200), now),
            Some("2h".to_string())
        );
        assert_eq!(
            format_agent_activity_age(Some(now - 172800), now),
            Some("2d".to_string())
        );
    }

    #[test]
    fn all_workspaces_agent_panel_entries_use_workspace_and_tab_labels() {
        let mut app = crate::app::state::AppState::test_new();
        let first = Workspace::test_new("one");
        let first_pane = first.tabs[0].root_pane;
        let mut second = Workspace::test_new("two");
        let second_tab = second.test_add_tab(Some("logs"));
        let second_pane = second.tabs[second_tab].root_pane;

        app.workspaces = vec![first, second];
        app.ensure_test_terminals();
        let first_terminal_id = app.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        let first_terminal = app.terminals.get_mut(&first_terminal_id).unwrap();
        first_terminal.cwd = std::path::PathBuf::from("/tmp/one");
        first_terminal.detected_agent = Some(Agent::OhMyPi);
        let second_terminal_id = app.workspaces[1].tabs[second_tab].panes[&second_pane]
            .attached_terminal_id
            .clone();
        let second_terminal = app.terminals.get_mut(&second_terminal_id).unwrap();
        second_terminal.cwd = std::path::PathBuf::from("/tmp/two");
        second_terminal.detected_agent = Some(Agent::Claude);
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let entries = agent_panel_entries(&app);
        assert_eq!(entries[0].primary_label, "one");
        assert_eq!(entries[0].primary_tab_label.as_deref(), Some("1"));
        assert_eq!(entries[0].agent_label.as_deref(), Some("omp"));
        assert_eq!(entries[1].primary_label, "two/logs");
        assert_eq!(entries[1].primary_tab_label.as_deref(), Some("logs"));
        assert_eq!(entries[1].agent_label.as_deref(), Some("claude"));
    }

    #[test]
    fn agent_panel_disambiguates_agents_in_different_tabs() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("personal");
        let first_pane = workspace.tabs[0].root_pane;
        let second_tab = workspace.test_add_tab(None);
        let second_pane = workspace.tabs[second_tab].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();

        let first_terminal_id = app.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Codex);
        let second_terminal_id = app.workspaces[0].tabs[second_tab].panes[&second_pane]
            .attached_terminal_id
            .clone();
        app.terminals
            .get_mut(&second_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::CurrentWorkspace;

        let entries = agent_panel_entries(&app);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].primary_label, "personal/1");
        assert_eq!(entries[1].primary_label, "personal/2");
    }

    #[test]
    fn agent_panel_includes_tab_when_space_has_non_agent_tabs() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("personal");
        let agent_pane = workspace.tabs[0].root_pane;
        workspace.test_add_tab(Some("shell"));
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&agent_pane]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Codex);
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::CurrentWorkspace;

        let entries = agent_panel_entries(&app);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].primary_label, "personal/1");
    }

    #[test]
    fn agent_panel_title_mutes_text_before_the_last_slash() {
        let spans =
            agent_panel_title_spans("repo/2/Pane 1", None, Style::default(), Style::default());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content.as_ref(), "repo/2/");
        assert_eq!(spans[1].content.as_ref(), "Pane 1");

        let single = agent_panel_title_spans("repo", None, Style::default(), Style::default());
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].content.as_ref(), "repo");
    }

    #[test]
    fn agent_panel_renumbers_unnamed_panes_after_one_closes() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("personal");
        let first_pane = workspace.tabs[0].root_pane;
        let closed_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);
        let last_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);
        assert!(!workspace.close_pane(closed_pane));
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();

        for (pane_id, agent) in [(first_pane, Agent::Codex), (last_pane, Agent::Claude)] {
            let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(agent);
        }
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::CurrentWorkspace;

        let entries = agent_panel_entries(&app);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].primary_label, "personal/Pane 1");
        assert_eq!(entries[1].primary_label, "personal/Pane 2");
    }

    #[test]
    fn agent_panel_entry_uses_pane_specific_row_labels_when_split_focus_is_elsewhere() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("ignored");
        workspace.custom_name = None;
        workspace.identity_cwd = std::path::PathBuf::from("/tmp/identity");
        let left_pane = workspace.tabs[0].root_pane;
        let right_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);

        app.workspaces = vec![workspace];
        app.ensure_test_terminals();

        let left_terminal_id = app.workspaces[0].tabs[0].panes[&left_pane]
            .attached_terminal_id
            .clone();
        let left_terminal = app.terminals.get_mut(&left_terminal_id).unwrap();
        left_terminal.cwd = std::path::PathBuf::from("/tmp/gardn");
        left_terminal.detected_agent = Some(Agent::Pi);
        left_terminal.state = AgentState::Working;

        let right_terminal_id = app.workspaces[0].tabs[0].panes[&right_pane]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&right_terminal_id).unwrap().cwd =
            std::path::PathBuf::from("/tmp/showcode");

        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::CurrentWorkspace;

        let entries = agent_panel_entries(&app);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pane_id, left_pane);
        assert_eq!(entries[0].primary_label, "gardn/Pane 1");
        assert_eq!(entries[0].primary_tab_label.as_deref(), Some("1"));
        assert_eq!(entries[0].agent_label.as_deref(), Some("pi"));
        assert_eq!(entries[0].state, AgentState::Working);
    }

    #[test]
    fn agent_panel_entries_use_terminal_cwd_basename_not_workspace_identity() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("stale-name");
        workspace.custom_name = None;
        workspace.identity_cwd = std::path::PathBuf::from("/tmp/issue-264-nix-support");
        let pane = workspace.tabs[0].root_pane;

        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane]
            .attached_terminal_id
            .clone();
        let terminal = app.terminals.get_mut(&terminal_id).unwrap();
        terminal.cwd = std::path::PathBuf::from("/tmp/Gardn");
        terminal.detected_agent = Some(Agent::Pi);
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let entries = agent_panel_entries(&app);

        assert_eq!(entries[0].primary_label, "Gardn");
    }

    #[test]
    fn agent_panel_entries_include_named_terminals_before_detection() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("one");
        let pane = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().agent_name = Some("codex".into());
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::CurrentWorkspace;

        let entries = agent_panel_entries(&app);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].agent_label.as_deref(), Some("codex"));
        assert_eq!(entries[0].primary_label, "one");
    }

    #[test]
    fn all_workspaces_agent_panel_entries_include_hidden_groups() {
        let mut app = crate::app::state::AppState::test_new();
        let hidden_group = app.create_group("Work".to_string());

        let mut first = Workspace::test_new("one");
        let first_pane = first.tabs[0].root_pane;
        first.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);

        let mut second = Workspace::test_new("two");
        second.group_id = app.groups[hidden_group].id.clone();
        let second_pane = second.tabs[0].root_pane;
        second.tabs[0]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);

        app.workspaces = vec![first, second];
        app.active = Some(0);
        app.selected = 0;
        app.active_group = 0;
        app.group_filter_enabled = true;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        assert_eq!(app.visible_workspace_indices(), vec![0]);
        let entries = agent_panel_entries(&app);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].primary_label, "one");
        assert_eq!(entries[0].group_context_idx, Some(0));
        assert_eq!(entries[1].primary_label, "two");
        assert_eq!(entries[1].group_context_idx, Some(hidden_group));
    }

    #[test]
    fn current_group_agent_panel_entries_use_active_workspace_group() {
        let mut app = crate::app::state::AppState::test_new();
        let work_group = app.create_group("Work".to_string());

        let mut first = Workspace::test_new("one");
        let first_pane = first.tabs[0].root_pane;
        first.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);

        let mut second = Workspace::test_new("two");
        second.group_id = app.groups[work_group].id.clone();
        let second_pane = second.tabs[0].root_pane;
        second.tabs[0]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);

        let mut third = Workspace::test_new("three");
        third.group_id = app.groups[work_group].id.clone();
        let third_pane = third.tabs[0].root_pane;
        third.tabs[0]
            .panes
            .get_mut(&third_pane)
            .unwrap()
            .detected_agent = Some(Agent::Codex);

        app.workspaces = vec![first, second, third];
        app.active = Some(1);
        app.selected = 1;
        app.agent_panel_scope = AgentPanelScope::CurrentGroup;

        let entries = agent_panel_entries(&app);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].primary_label, "two");
        assert_eq!(entries[1].primary_label, "three");
    }

    #[test]
    fn triage_agent_panel_entries_follow_current_group_scope() {
        let mut app = crate::app::state::AppState::test_new();
        let work_group = app.create_group("Work".to_string());

        let mut first = Workspace::test_new("Done");
        let first_pane = first.tabs[0].root_pane;
        let first_pane_state = first.tabs[0].panes.get_mut(&first_pane).unwrap();
        first_pane_state.detected_agent = Some(Agent::Pi);
        first_pane_state.state = AgentState::Idle;
        first_pane_state.seen = false;

        let mut second = Workspace::test_new("Blocked");
        second.group_id = app.groups[work_group].id.clone();
        let second_pane = second.tabs[0].root_pane;
        let second_pane_state = second.tabs[0].panes.get_mut(&second_pane).unwrap();
        second_pane_state.detected_agent = Some(Agent::Claude);
        second_pane_state.state = AgentState::Blocked;
        second_pane_state.seen = true;

        let mut third = Workspace::test_new("Working");
        let third_pane = third.tabs[0].root_pane;
        let third_pane_state = third.tabs[0].panes.get_mut(&third_pane).unwrap();
        third_pane_state.detected_agent = Some(Agent::Codex);
        third_pane_state.state = AgentState::Working;
        third_pane_state.seen = false;

        app.workspaces = vec![first, second, third];
        app.active_group = 0;
        app.agent_panel_scope = AgentPanelScope::CurrentGroup;

        let entries = agent_panel_triage_entries(&app);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].primary_label, "Done");
        assert_eq!(entries[0].group_context_idx, None);
    }

    #[test]
    fn agent_panel_sections_order_actionable_before_working_and_idle() {
        let mut app = crate::app::state::AppState::test_new();

        let mut triage = Workspace::test_new("Done");
        let triage_pane = triage.tabs[0].root_pane;
        let triage_state = triage.tabs[0].panes.get_mut(&triage_pane).unwrap();
        triage_state.detected_agent = Some(Agent::Pi);
        triage_state.state = AgentState::Idle;
        triage_state.seen = false;

        let mut working = Workspace::test_new("Working");
        let working_pane = working.tabs[0].root_pane;
        let working_state = working.tabs[0].panes.get_mut(&working_pane).unwrap();
        working_state.detected_agent = Some(Agent::Claude);
        working_state.state = AgentState::Working;

        let mut idle = Workspace::test_new("Idle");
        let idle_pane = idle.tabs[0].root_pane;
        let idle_state = idle.tabs[0].panes.get_mut(&idle_pane).unwrap();
        idle_state.detected_agent = Some(Agent::Codex);
        idle_state.state = AgentState::Idle;
        idle_state.seen = true;

        app.workspaces = vec![triage, working, idle];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let sections = agent_panel_sections(&app);

        assert_eq!(sections.len(), 4);
        assert_eq!(sections[0].group, AgentStatusGroup::Triage);
        assert_eq!(sections[0].entries[0].primary_label, "Done");
        assert_eq!(sections[1].group, AgentStatusGroup::FollowUp);
        assert!(sections[1].entries.is_empty());
        assert_eq!(sections[2].group, AgentStatusGroup::Working);
        assert_eq!(sections[2].entries[0].primary_label, "Working");
        assert_eq!(sections[3].group, AgentStatusGroup::Idle);
        assert_eq!(sections[3].entries[0].primary_label, "Idle");
    }

    #[test]
    fn focusing_unseen_idle_agent_keeps_it_in_triage_until_focus_leaves() {
        let mut app = crate::app::state::AppState::test_new();

        let mut done = Workspace::test_new("Done");
        let done_pane = done.tabs[0].root_pane;
        let done_state = done.tabs[0].panes.get_mut(&done_pane).unwrap();
        done_state.detected_agent = Some(Agent::Pi);
        done_state.state = AgentState::Idle;
        done_state.seen = false;

        let mut idle = Workspace::test_new("Idle");
        let idle_pane = idle.tabs[0].root_pane;
        let idle_state = idle.tabs[0].panes.get_mut(&idle_pane).unwrap();
        idle_state.detected_agent = Some(Agent::Codex);
        idle_state.state = AgentState::Idle;
        idle_state.seen = true;

        app.workspaces = vec![done, idle];
        app.active = Some(1);
        app.selected = 1;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        app.focus_workspace_tab_pane(0, 0, done_pane);

        let sections = agent_panel_sections(&app);
        let triage = sections
            .iter()
            .find(|section| section.group == AgentStatusGroup::Triage)
            .expect("triage");
        assert_eq!(triage.entries.len(), 1);
        assert_eq!(triage.entries[0].primary_label, "Done");
        assert!(triage.entries[0].seen);
        assert!(app.workspaces[0].tabs[0].panes[&done_pane].seen);

        app.switch_workspace(1);

        let sections = agent_panel_sections(&app);
        assert!(sections
            .iter()
            .all(|section| section.group != AgentStatusGroup::Triage));
        let idle_section = sections
            .iter()
            .find(|section| section.group == AgentStatusGroup::Idle)
            .expect("idle");
        let labels: Vec<_> = idle_section
            .entries
            .iter()
            .map(|entry| entry.primary_label.as_str())
            .collect();
        assert_eq!(labels, vec!["Done", "Idle"]);
    }

    #[test]
    fn focusing_unseen_idle_agent_leaves_triage_when_it_starts_working() {
        let mut app = crate::app::state::AppState::test_new();

        let mut done = Workspace::test_new("Done");
        let done_pane = done.tabs[0].root_pane;
        let done_state = done.tabs[0].panes.get_mut(&done_pane).unwrap();
        done_state.detected_agent = Some(Agent::Pi);
        done_state.state = AgentState::Idle;
        done_state.seen = false;

        app.workspaces = vec![done];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        app.focus_workspace_tab_pane(0, 0, done_pane);
        app.workspaces[0].tabs[0]
            .panes
            .get_mut(&done_pane)
            .unwrap()
            .state = AgentState::Working;

        let sections = agent_panel_sections(&app);
        assert!(sections
            .iter()
            .all(|section| section.group != AgentStatusGroup::Triage));
        let working = sections
            .iter()
            .find(|section| section.group == AgentStatusGroup::Working)
            .expect("working");
        assert_eq!(working.entries[0].primary_label, "Done");
    }

    #[test]
    fn focusing_unseen_idle_agent_not_already_focused_keeps_it_in_triage() {
        let mut app = crate::app::state::AppState::test_new();

        let mut workspace = Workspace::test_new("split");
        let first_pane = workspace.tabs[0].root_pane;
        let second_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);
        workspace.tabs[0].layout.focus_pane(second_pane);

        let first_state = workspace.tabs[0].panes.get_mut(&first_pane).unwrap();
        first_state.detected_agent = Some(Agent::Pi);
        first_state.state = AgentState::Idle;
        first_state.seen = false;

        let second_state = workspace.tabs[0].panes.get_mut(&second_pane).unwrap();
        second_state.detected_agent = Some(Agent::Codex);
        second_state.state = AgentState::Idle;
        second_state.seen = true;

        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        app.focus_workspace_tab_pane(0, 0, first_pane);

        let sections = agent_panel_sections(&app);
        let triage = sections
            .iter()
            .find(|section| section.group == AgentStatusGroup::Triage)
            .expect("triage");
        assert_eq!(triage.entries.len(), 1);
        assert_eq!(triage.entries[0].pane_id, first_pane);
        assert!(triage.entries[0].seen);
        let idle = sections
            .iter()
            .find(|section| section.group == AgentStatusGroup::Idle)
            .expect("idle");
        assert_eq!(idle.entries.len(), 1);
        assert_eq!(idle.entries[0].pane_id, second_pane);
    }

    #[test]
    fn agent_panel_sections_order_entries_newest_activity_first() {
        let mut app = crate::app::state::AppState::test_new();
        let old = Workspace::test_new("old");
        let old_pane = old.tabs[0].root_pane;
        let new = Workspace::test_new("New");
        let new_pane = new.tabs[0].root_pane;
        app.workspaces = vec![old, new];
        app.ensure_test_terminals();
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let now = std::time::Instant::now();
        app.handle_app_event(crate::events::AppEvent::StateChanged {
            pane_id: old_pane,
            agent: Some(Agent::Codex),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: now,
        });
        app.handle_app_event(crate::events::AppEvent::StateChanged {
            pane_id: new_pane,
            agent: Some(Agent::Claude),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: now + std::time::Duration::from_millis(1),
        });

        let sections = agent_panel_sections(&app);
        let idle = sections
            .iter()
            .find(|section| section.group == AgentStatusGroup::Idle)
            .expect("idle section");

        assert_eq!(idle.entries[0].primary_label, "New");
        assert_eq!(idle.entries[1].primary_label, "old");
    }

    #[test]
    fn stable_visible_refresh_does_not_make_agent_newest() {
        let mut app = crate::app::state::AppState::test_new();
        let first = Workspace::test_new("first");
        let first_pane = first.tabs[0].root_pane;
        let second = Workspace::test_new("second");
        let second_pane = second.tabs[0].root_pane;
        app.workspaces = vec![first, second];
        app.ensure_test_terminals();
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let now = std::time::Instant::now();
        app.handle_app_event(crate::events::AppEvent::StateChanged {
            pane_id: first_pane,
            agent: Some(Agent::Codex),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: true,
            process_exited: false,
            observed_at: now,
        });
        app.handle_app_event(crate::events::AppEvent::StateChanged {
            pane_id: second_pane,
            agent: Some(Agent::Claude),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: true,
            process_exited: false,
            observed_at: now + std::time::Duration::from_millis(1),
        });
        app.handle_app_event(crate::events::AppEvent::StateChanged {
            pane_id: first_pane,
            agent: Some(Agent::Codex),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: true,
            process_exited: false,
            observed_at: now + std::time::Duration::from_secs(1),
        });

        let sections = agent_panel_sections(&app);
        let working = sections
            .iter()
            .find(|section| section.group == AgentStatusGroup::Working)
            .expect("working section");

        assert_eq!(working.entries[0].primary_label, "second");
        assert_eq!(working.entries[1].primary_label, "first");
    }

    #[test]
    fn agent_rename_does_not_count_as_activity() {
        let mut app = crate::app::state::AppState::test_new();
        let old = Workspace::test_new("old");
        let old_pane = old.tabs[0].root_pane;
        let new = Workspace::test_new("New");
        let new_pane = new.tabs[0].root_pane;
        app.workspaces = vec![old, new];
        app.ensure_test_terminals();
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let now = std::time::Instant::now();
        app.handle_app_event(crate::events::AppEvent::StateChanged {
            pane_id: old_pane,
            agent: Some(Agent::Codex),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: now,
        });
        app.handle_app_event(crate::events::AppEvent::StateChanged {
            pane_id: new_pane,
            agent: Some(Agent::Claude),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: now + std::time::Duration::from_millis(1),
        });
        let old_terminal_id = app.workspaces[0].tabs[0].panes[&old_pane]
            .attached_terminal_id
            .clone();
        app.terminals
            .get_mut(&old_terminal_id)
            .expect("old terminal")
            .set_agent_name("renamed".into());

        let sections = agent_panel_sections(&app);
        let idle = sections
            .iter()
            .find(|section| section.group == AgentStatusGroup::Idle)
            .expect("idle section");

        assert_eq!(idle.entries[0].primary_label, "New");
        assert_eq!(idle.entries[1].primary_label, "old");
        assert_eq!(idle.entries[1].agent_label.as_deref(), Some("renamed"));
    }

    #[test]
    fn agent_panel_renders_tab_suffixes_for_duplicate_workspace_labels() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("personal");
        let first_pane = workspace.tabs[0].root_pane;
        workspace.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Codex);
        workspace.tabs[0].panes.get_mut(&first_pane).unwrap().state = AgentState::Working;
        let second_tab = workspace.test_add_tab(None);
        let second_pane = workspace.tabs[second_tab].root_pane;
        workspace.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        workspace.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .state = AgentState::Working;
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::CurrentWorkspace;

        let backend = TestBackend::new(34, 22);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_sidebar(&app, &terminal_runtimes, frame, Rect::new(0, 0, 34, 22)))
            .expect("render sidebar");

        let text = buffer_text(terminal.backend().buffer(), 34, 22);
        assert!(text.contains("personal/1"), "rendered UI:\n{text}");
        assert!(text.contains("personal/2"), "rendered UI:\n{text}");
    }

    #[test]
    fn non_triage_agent_rows_omit_redundant_status() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("worker");
        let pane = workspace.tabs[0].root_pane;
        let pane_state = workspace.tabs[0].panes.get_mut(&pane).unwrap();
        pane_state.detected_agent = Some(Agent::Codex);
        pane_state.state = AgentState::Working;
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;

        let backend = TestBackend::new(34, 12);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_sidebar(&app, &terminal_runtimes, frame, Rect::new(0, 0, 34, 12)))
            .expect("render sidebar");

        let text = buffer_text(terminal.backend().buffer(), 34, 12);
        assert!(text.contains("Working"));
        assert!(!text.contains("codex"));
        assert!(!text.contains("working · codex"));
    }

    #[test]
    fn agent_rows_show_agent_names_when_visible_entries_mix_agent_types() {
        let mut app = crate::app::state::AppState::test_new();
        let mut codex_workspace = Workspace::test_new("codex-worker");
        let codex_pane = codex_workspace.tabs[0].root_pane;
        let codex_state = codex_workspace.tabs[0].panes.get_mut(&codex_pane).unwrap();
        codex_state.detected_agent = Some(Agent::Codex);
        codex_state.state = AgentState::Working;

        let mut claude_workspace = Workspace::test_new("claude-worker");
        let claude_pane = claude_workspace.tabs[0].root_pane;
        let claude_state = claude_workspace.tabs[0]
            .panes
            .get_mut(&claude_pane)
            .unwrap();
        claude_state.detected_agent = Some(Agent::Claude);
        claude_state.state = AgentState::Working;
        app.workspaces = vec![codex_workspace, claude_workspace];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let backend = TestBackend::new(34, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_sidebar(&app, &terminal_runtimes, frame, Rect::new(0, 0, 34, 24)))
            .expect("render sidebar");

        let text = buffer_text(terminal.backend().buffer(), 34, 24);
        assert!(text.contains("codex"));
        assert!(text.contains("claude"));
        assert!(!text.contains("working · codex"));
    }

    #[test]
    fn dense_agent_rows_do_not_render_behind_configuration_issue_footer() {
        let mut app = crate::app::state::AppState::test_new();
        app.config_issue = Some(crate::app::state::ConfigIssue::from_details(
            "config.toml: unknown key `colour`".to_string(),
        ));
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        app.workspaces = (0..12)
            .map(|_| {
                let mut workspace = Workspace::test_new("agent-with-a-long-name");
                let pane = workspace.tabs[0].root_pane;
                let state = workspace.tabs[0].panes.get_mut(&pane).unwrap();
                state.detected_agent = Some(Agent::Codex);
                state.state = AgentState::Working;
                workspace
            })
            .collect();
        app.active = Some(0);
        app.selected = 0;

        let area = Rect::new(0, 0, 34, 12);
        app.view.sidebar_rect = area;
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_sidebar(&app, &terminal_runtimes, frame, area))
            .expect("render dense sidebar");

        let launcher = app.global_launcher_rect();
        let buffer = terminal.backend().buffer();
        for x in launcher.x..launcher.x + launcher.width {
            let cell = &buffer[(x, launcher.y)];
            assert_eq!(cell.style().fg, Some(app.palette.yellow));
            assert_eq!(cell.style().bg, Some(app.palette.panel_bg));
            assert_eq!(cell.style().add_modifier, Modifier::BOLD);
        }
        let toggle = expanded_sidebar_toggle_rect(area);
        for x in launcher.x + launcher.width..toggle.x {
            assert_eq!(buffer[(x, launcher.y)].symbol(), " ");
        }
    }

    #[test]
    fn triage_agent_rows_keep_status_reason() {
        let mut app = crate::app::state::AppState::test_new();
        app.show_counters = true;
        let mut workspace = Workspace::test_new("needs-action");
        let pane = workspace.tabs[0].root_pane;
        let pane_state = workspace.tabs[0].panes.get_mut(&pane).unwrap();
        pane_state.detected_agent = Some(Agent::Claude);
        pane_state.state = AgentState::Idle;
        pane_state.seen = false;
        app.workspaces = vec![workspace];
        app.active = None;
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let backend = TestBackend::new(34, 12);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_sidebar(&app, &terminal_runtimes, frame, Rect::new(0, 0, 34, 12)))
            .expect("render sidebar");

        let text = buffer_text(terminal.backend().buffer(), 34, 12);
        assert!(text.contains("Triage"));
        assert!(!text.contains("Triage · All Spaces"));
        assert!(text.contains("Done"));
        assert!(!text.contains("Claude · Done"));
        assert!(!text.contains("Done · Claude"));
        let (_, agent_area) =
            expanded_sidebar_sections(Rect::new(0, 0, 34, 12), app.sidebar_section_split);
        let body = agent_panel_body_rect(agent_area, false, true);
        let buffer = terminal.backend().buffer();
        assert!(!buffer[(body.x + body.width - 2, body.y)]
            .style()
            .add_modifier
            .contains(Modifier::BOLD));
        assert_eq!(
            buffer[(body.x + RIGHT_SUBSECTION_LABEL_COL, body.y)].symbol(),
            "T"
        );
        let triage_row = text
            .lines()
            .find(|line| line.contains("Triage"))
            .expect("triage section should be visible");
        assert!(triage_row.contains('1'), "rendered UI:\n{text}");
        assert_eq!(
            buffer[(body.x + RIGHT_SUBSECTION_MARKER_COL, body.y)].symbol(),
            "▾"
        );
        assert_eq!(
            buffer[(body.x + RIGHT_ENTRY_PRIMARY_COL, body.y + 1)].symbol(),
            "n"
        );
        assert!(!buffer[(body.x + RIGHT_ENTRY_PRIMARY_COL, body.y + 1)]
            .style()
            .add_modifier
            .contains(Modifier::BOLD));
        assert_eq!(
            buffer[(body.x + RIGHT_ENTRY_PRIMARY_COL, body.y + 2)].symbol(),
            "D"
        );
    }

    #[test]
    fn idle_agent_rows_start_under_section_label_without_status_icons() {
        let mut app = crate::app::state::AppState::test_new();
        let mut first = Workspace::test_new("first");
        let first_pane = first.tabs[0].root_pane;
        let first_state = first.tabs[0].panes.get_mut(&first_pane).unwrap();
        first_state.detected_agent = Some(Agent::Codex);
        first_state.state = AgentState::Idle;
        first_state.seen = true;

        let mut second = Workspace::test_new("second");
        let second_pane = second.tabs[0].root_pane;
        let second_state = second.tabs[0].panes.get_mut(&second_pane).unwrap();
        second_state.detected_agent = Some(Agent::Codex);
        second_state.state = AgentState::Idle;
        second_state.seen = true;
        app.workspaces = vec![first, second];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let area = Rect::new(0, 0, 34, 24);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_sidebar(&app, &terminal_runtimes, frame, area))
            .expect("render sidebar");

        let (_, agent_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        let body = agent_panel_body_rect(agent_area, false, true);
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(body.x + RIGHT_SUBSECTION_LABEL_COL, body.y)].symbol(),
            "F"
        );
        assert_eq!(
            buffer[(body.x + RIGHT_ENTRY_PRIMARY_COL, body.y + 3)].symbol(),
            "f"
        );
        assert_eq!(
            buffer[(body.x + RIGHT_ENTRY_PRIMARY_COL, body.y + 4)].symbol(),
            "s"
        );
    }

    #[test]
    fn agent_section_headers_use_status_colors() {
        let mut triage = AgentPanelSection {
            group: AgentStatusGroup::Triage,
            entries: vec![AgentPanelEntry {
                ws_idx: 0,
                tab_idx: 0,
                pane_id: crate::layout::PaneId::from_raw(1),
                group_context_idx: None,
                primary_label: "Blocked".into(),
                pane_label: None,
                primary_tab_label: None,
                agent_label: Some("opencode".into()),
                terminal_title: None,
                terminal_title_stripped: None,
                state: AgentState::Blocked,
                seen: false,
                agent: None,
                custom_status: None,
                state_labels: std::collections::HashMap::new(),
                last_meaningful_agent_activity_seq: 0,
                last_meaningful_agent_activity_unix_secs: None,
                follow_up_added_at_unix_secs: None,
                tokens: std::collections::HashMap::new(),
            }],
        };
        let p = crate::app::state::Palette::catppuccin();

        assert_eq!(
            agent_panel_section_header_style(&triage, &p).fg,
            Some(p.peach)
        );
        assert_eq!(
            agent_panel_section_header_style(
                &AgentPanelSection {
                    group: AgentStatusGroup::Working,
                    entries: Vec::new(),
                },
                &p,
            )
            .fg,
            Some(p.yellow)
        );
        assert_eq!(
            agent_panel_section_header_style(
                &AgentPanelSection {
                    group: AgentStatusGroup::Idle,
                    entries: Vec::new(),
                },
                &p,
            )
            .fg,
            Some(p.green)
        );

        triage.entries[0].state = AgentState::Idle;
        triage.entries[0].seen = false;
        assert_eq!(
            agent_panel_section_header_style(&triage, &p).fg,
            Some(p.peach)
        );
    }

    fn rendered_agent_section_indicator(
        indicator_style: crate::config::StatusIndicatorStyle,
        spinner_tick: u32,
        group: AgentStatusGroup,
    ) -> (String, Option<ratatui::style::Color>) {
        let mut app = crate::app::state::AppState::test_new();
        app.status_indicators = indicator_style;
        app.spinner_tick = spinner_tick;
        let section = AgentPanelSection {
            group,
            entries: Vec::new(),
        };
        let area = Rect::new(0, 0, 12, 1);
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");
        terminal
            .draw(|frame| render_agent_section_header(&app, frame, &section, false, area, 0))
            .expect("render agent section header");
        let cell = &terminal.backend().buffer()[(RIGHT_SUBSECTION_ICON_COL, 0)];
        (cell.symbol().to_string(), cell.style().fg)
    }

    #[test]
    fn follow_up_drop_target_replaces_empty_row_with_workspace_style_indicator() {
        let area = Rect::new(0, 0, 12, 8);
        let body = agent_panel_body_rect(area, false, true);
        let mut app = crate::app::state::AppState::test_new();
        app.drag = Some(crate::app::state::DragState {
            target: crate::app::state::DragTarget::AgentFollowUp {
                workspace_id: "workspace".into(),
                pane_number: 1,
                drop_indicator_row: Some(body.y + 1),
            },
        });
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");

        terminal
            .draw(|frame| render_agent_detail_from(&app, &terminal_runtimes, frame, area, true))
            .expect("render Follow Up drop target");

        let buffer = terminal.backend().buffer();
        for x in body.x..body.x + body.width {
            assert_eq!(buffer[(x, body.y + 1)].symbol(), "─");
            assert_eq!(buffer[(x, body.y + 1)].style().fg, Some(app.palette.accent));
        }
    }

    #[test]
    fn follow_up_drop_target_replaces_agent_row_with_workspace_style_indicator() {
        let area = Rect::new(0, 0, 18, 8);
        let body = agent_panel_body_rect(area, false, true);
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("agent");
        let pane = workspace.tabs[0].root_pane;
        workspace.tabs[0]
            .panes
            .get_mut(&pane)
            .unwrap()
            .detected_agent = Some(Agent::Codex);
        workspace.tabs[0].panes.get_mut(&pane).unwrap().state = AgentState::Working;
        app.workspaces = vec![workspace];
        app.active = Some(0);
        assert!(app.insert_agent_follow_up(0, pane));
        app.drag = Some(crate::app::state::DragState {
            target: crate::app::state::DragTarget::AgentFollowUp {
                workspace_id: app.workspaces[0].id.clone(),
                pane_number: 1,
                drop_indicator_row: Some(body.y + 1),
            },
        });
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");

        terminal
            .draw(|frame| render_agent_detail_from(&app, &terminal_runtimes, frame, area, true))
            .expect("render Follow Up agent drop target");

        let buffer = terminal.backend().buffer();
        for x in body.x..body.x + body.width {
            assert_eq!(buffer[(x, body.y + 1)].symbol(), "─");
            assert_eq!(buffer[(x, body.y + 1)].style().fg, Some(app.palette.accent));
        }
    }

    #[test]
    fn agent_group_headers_follow_the_complete_status_indicator_table() {
        let palette = crate::app::state::Palette::catppuccin();
        for (indicator_style, expected) in [
            (
                crate::config::StatusIndicatorStyle::Dots,
                [
                    (AgentStatusGroup::Triage, "●", palette.peach),
                    (AgentStatusGroup::FollowUp, "●", palette.mauve),
                    (AgentStatusGroup::Working, "●", palette.yellow),
                    (AgentStatusGroup::Idle, "○", palette.green),
                ],
            ),
            (
                crate::config::StatusIndicatorStyle::Symbols,
                [
                    (AgentStatusGroup::Triage, "!", palette.peach),
                    (AgentStatusGroup::FollowUp, "*", palette.mauve),
                    (AgentStatusGroup::Working, "⠋", palette.yellow),
                    (AgentStatusGroup::Idle, "✓", palette.green),
                ],
            ),
        ] {
            for (group, expected_symbol, expected_color) in expected {
                assert_eq!(
                    rendered_agent_section_indicator(indicator_style, 0, group),
                    (expected_symbol.to_string(), Some(expected_color)),
                    "{indicator_style:?} {}",
                    group.label()
                );
            }
        }
    }

    #[test]
    fn symbols_working_agent_group_uses_exact_braille_frames() {
        assert_eq!(
            rendered_agent_section_indicator(
                crate::config::StatusIndicatorStyle::Symbols,
                0,
                AgentStatusGroup::Working,
            )
            .0,
            "⠋"
        );
        assert_eq!(
            rendered_agent_section_indicator(
                crate::config::StatusIndicatorStyle::Symbols,
                8,
                AgentStatusGroup::Working,
            )
            .0,
            "⠙"
        );
    }

    #[test]
    fn all_workspaces_agent_panel_entries_prefer_agent_names_for_agent_identity() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("bridge");
        let first_pane = workspace.tabs[0].root_pane;

        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let first_terminal_id = app.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        app.terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .set_agent_name("planner".into());
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let entries = agent_panel_entries(&app);
        assert_eq!(entries[0].primary_label, "bridge");
        assert_eq!(entries[0].agent_label.as_deref(), Some("planner"));
    }

    #[test]
    fn agent_header_and_scope_badge_share_row_above_divider_without_header_chevron() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("worker");
        let pane = workspace.tabs[0].root_pane;
        let pane_state = workspace.tabs[0].panes.get_mut(&pane).unwrap();
        pane_state.detected_agent = Some(Agent::Codex);
        pane_state.state = AgentState::Working;
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let area = Rect::new(0, 0, 34, 18);
        let (_, agent_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_sidebar(&app, &terminal_runtimes, frame, area))
            .expect("render sidebar");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, area.width, area.height);
        let rows = text.lines().collect::<Vec<_>>();
        let header = rows[agent_area.y as usize];
        let divider = rows[agent_area.y.saturating_add(1) as usize];
        let toggle_rect = agent_panel_toggle_rect(agent_area, app.agent_panel_scope, true);

        assert_eq!(toggle_rect.y, agent_area.y);
        assert!(header.contains("Agents"));
        assert!(header.contains("All"));
        assert!(!divider.contains("Agents"));
        assert!(!divider.contains("All"));
        assert!(!header.contains("▾ agents"));
        assert!(!header.contains("▸ agents"));
        assert_eq!(buffer[(agent_area.x, agent_area.y)].symbol(), "A");
        for x in agent_area.x..agent_area.x + agent_area.width {
            assert_eq!(buffer[(x, agent_area.y + 1)].symbol(), "─");
        }
    }

    #[test]
    fn agent_status_group_headers_use_real_chevrons_at_workspace_group_indent() {
        let mut app = crate::app::state::AppState::test_new();
        app.group_filter_enabled = false;

        let mut triage = Workspace::test_new("Done");
        let triage_pane = triage.tabs[0].root_pane;
        let triage_pane_state = triage.tabs[0].panes.get_mut(&triage_pane).unwrap();
        triage_pane_state.detected_agent = Some(Agent::Claude);
        triage_pane_state.state = AgentState::Idle;
        triage_pane_state.seen = false;

        let mut working = Workspace::test_new("worker");
        let working_pane = working.tabs[0].root_pane;
        let working_pane_state = working.tabs[0].panes.get_mut(&working_pane).unwrap();
        working_pane_state.detected_agent = Some(Agent::Codex);
        working_pane_state.state = AgentState::Working;

        app.workspaces = vec![triage, working];
        app.active = None;
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        app.collapsed_agent_sections.push("Working".to_string());

        let area = Rect::new(0, 0, 36, 22);
        let (workspace_area, agent_area) =
            expanded_sidebar_sections(area, app.sidebar_section_split);
        app.view.workspace_card_areas = compute_workspace_card_areas_in_list(&app, workspace_area);
        app.view.workspace_group_header_areas =
            compute_workspace_group_header_areas_in_list(&app, workspace_area);
        app.view.workspace_group_empty_areas =
            compute_workspace_group_empty_areas_in_list(&app, workspace_area);

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_sidebar(&app, &terminal_runtimes, frame, area))
            .expect("render sidebar");

        let buffer = terminal.backend().buffer();
        let workspace_header = app
            .view
            .workspace_group_header_areas
            .iter()
            .find(|header| header.group_idx == 0)
            .expect("default workspace group header")
            .rect;
        let body = agent_panel_body_rect(
            agent_area,
            agent_panel_scrollbar_rect(&app, agent_area, true).is_some(),
            true,
        );
        let workspace_chevron_x = workspace_header.x + SIDEBAR_GROUP_CHEVRON_COL;
        let expanded_agent_header_y = body.y;
        let follow_up_header_y = body.y + 3;
        let collapsed_agent_header_y = body.y + 5;

        assert_eq!(
            buffer[(workspace_chevron_x, workspace_header.y)].symbol(),
            "▾"
        );
        assert_eq!(workspace_chevron_x, body.x + RIGHT_SUBSECTION_MARKER_COL);
        assert_eq!(
            buffer[(workspace_chevron_x, expanded_agent_header_y)].symbol(),
            "▾"
        );
        assert_eq!(
            buffer[(workspace_chevron_x, follow_up_header_y)].symbol(),
            "▾"
        );
        assert_eq!(
            buffer[(body.x + RIGHT_SUBSECTION_LABEL_COL, follow_up_header_y)].symbol(),
            "F"
        );
        assert_eq!(
            buffer[(workspace_chevron_x, collapsed_agent_header_y)].symbol(),
            "▸"
        );
        assert_ne!(
            buffer[(workspace_chevron_x, expanded_agent_header_y)].symbol(),
            "›"
        );
        assert_ne!(
            buffer[(workspace_chevron_x, collapsed_agent_header_y)].symbol(),
            "›"
        );
        assert_eq!(
            buffer[(body.x + RIGHT_SUBSECTION_LABEL_COL, expanded_agent_header_y)].symbol(),
            "T"
        );
        assert_eq!(
            buffer[(
                body.x + RIGHT_SUBSECTION_LABEL_COL,
                collapsed_agent_header_y
            )]
                .symbol(),
            "W"
        );
    }

    #[test]
    fn right_sidebar_without_context_renders_agent_panel_shell() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("web")];
        app.active = Some(0);
        app.selected = 0;

        let backend = TestBackend::new(32, 18);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_right_sidebar(&app, &runtimes, frame, Rect::new(0, 0, 32, 18)))
            .expect("render right sidebar");

        let text = buffer_text(terminal.backend().buffer(), 32, 18);
        assert!(text.contains("Agents"));
        assert!(!text.contains("commands"));
        assert!(!text.contains("ports"));
    }

    #[test]
    fn collapsed_right_sidebar_renders_agent_status_groups_and_expand_toggle() {
        let mut app = crate::app::state::AppState::test_new();
        let mut triage = Workspace::test_new("Done");
        let triage_pane = triage.tabs[0].root_pane;
        let triage_pane_state = triage.tabs[0].panes.get_mut(&triage_pane).unwrap();
        triage_pane_state.detected_agent = Some(Agent::Claude);
        triage_pane_state.state = AgentState::Idle;
        triage_pane_state.seen = false;

        let mut working = Workspace::test_new("build");
        let working_pane = working.tabs[0].root_pane;
        let working_pane_state = working.tabs[0].panes.get_mut(&working_pane).unwrap();
        working_pane_state.detected_agent = Some(Agent::Codex);
        working_pane_state.state = AgentState::Working;

        app.workspaces = vec![triage, working];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        app.right_sidebar_collapsed = true;

        let area = Rect::new(0, 0, 4, 12);
        let content = right_sidebar_content_rect(area);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_right_sidebar(&app, &runtimes, frame, area))
            .expect("render right sidebar");

        let buffer = terminal.backend().buffer();
        let rows = buffer_text(buffer, area.width, area.height)
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let toggle = right_sidebar_toggle_rect(area, true);
        let triage_header_row = content.y + 2;
        let triage_agent_row = triage_header_row + 1;
        let follow_up_header_row = triage_agent_row + 1;
        let working_header_row = follow_up_header_row + 1;
        let working_agent_row = working_header_row + 1;

        assert_eq!(rows[0], "│All");
        assert_eq!(rows[1], "│───");
        assert_eq!(buffer[(content.x, triage_header_row)].symbol(), "▾");
        assert_eq!(buffer[(content.x + 2, triage_agent_row)].symbol(), "1");
        assert_eq!(
            buffer[(content.x + 2, triage_agent_row)].style().bg,
            Some(app.palette.surface_dim)
        );
        assert_eq!(buffer[(content.x, follow_up_header_row)].symbol(), "▾");
        assert_eq!(
            collapsed_agent_panel_header_target_at_row(&app, content, follow_up_header_row)
                .map(|target| target.section),
            Some("Follow Up".to_string())
        );
        assert_eq!(buffer[(content.x, working_header_row)].symbol(), "▾");
        assert_eq!(
            buffer[(content.x + 2, working_header_row)].symbol(),
            agent_section_icon(
                AgentStatusGroup::Working,
                app.spinner_tick,
                app.status_indicators,
                &app.palette,
            )
            .0
        );
        assert_eq!(buffer[(content.x + 2, working_agent_row)].symbol(), "2");
        assert_eq!(buffer[(toggle.x, toggle.y)].symbol(), "«");
    }

    #[test]
    fn expanded_sidebar_sections_handle_tiny_heights() {
        let (ws_area, detail_area) = expanded_sidebar_sections(Rect::new(0, 0, 20, 5), 0.9);

        assert_eq!(ws_area, Rect::new(0, 0, 19, 3));
        assert_eq!(detail_area, Rect::new(0, 3, 19, 2));
    }

    #[test]
    fn sidebar_section_divider_is_hidden_for_tiny_heights() {
        let divider = sidebar_section_divider_rect(Rect::new(0, 0, 20, 5), 0.5);

        assert_eq!(divider, Rect::default());
    }

    #[test]
    fn follow_up_section_is_always_present_and_header_hit_testable_when_empty() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("idle")];
        let pane = app.workspaces[0].tabs[0].root_pane;
        app.workspaces[0].tabs[0]
            .panes
            .get_mut(&pane)
            .unwrap()
            .detected_agent = Some(Agent::Codex);
        app.workspaces[0].tabs[0]
            .panes
            .get_mut(&pane)
            .unwrap()
            .state = AgentState::Idle;
        app.workspaces[0].tabs[0].panes.get_mut(&pane).unwrap().seen = true;
        app.active = Some(0);
        app.agent_panel_scope = AgentPanelScope::CurrentWorkspace;

        let sections = agent_panel_sections(&app);
        assert_eq!(sections[0].group, AgentStatusGroup::FollowUp);
        assert!(sections[0].entries.is_empty());
        assert_eq!(sections[1].group, AgentStatusGroup::Idle);

        let area = Rect::new(0, 0, 40, 24);
        crate::ui::compute_view(&mut app, area);
        let (_, agent_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        let body = agent_panel_body_rect(agent_area, false, true);
        let header = agent_panel_header_target_at_row(&app, body, body.y);
        assert_eq!(
            header.map(|target| target.section),
            Some("Follow Up".into())
        );
        assert!(agent_panel_entry_at_row(&app, body, body.y).is_none());
    }

    #[test]
    fn queued_follow_up_keeps_runtime_state_and_oldest_added_order() {
        let mut app = crate::app::state::AppState::test_new();
        let mut first = Workspace::test_new("first");
        let first_pane = first.tabs[0].root_pane;
        first.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Codex);
        first.tabs[0].panes.get_mut(&first_pane).unwrap().state = AgentState::Working;
        let mut second = Workspace::test_new("second");
        let second_pane = second.tabs[0].root_pane;
        second.tabs[0]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        second.tabs[0].panes.get_mut(&second_pane).unwrap().state = AgentState::Blocked;
        second.tabs[0].panes.get_mut(&second_pane).unwrap().seen = false;
        app.workspaces = vec![first, second];
        app.active = Some(0);
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        assert!(app.insert_agent_follow_up(1, second_pane));
        app.agent_follow_up[0].added_at_unix_secs = 20;
        assert!(app.insert_agent_follow_up(0, first_pane));
        app.agent_follow_up[1].added_at_unix_secs = 10;

        let sections = agent_panel_sections(&app);
        let follow_up = sections
            .iter()
            .find(|section| section.group == AgentStatusGroup::FollowUp)
            .expect("follow up");
        assert_eq!(follow_up.entries.len(), 2);
        assert_eq!(follow_up.entries[0].primary_label, "first");
        assert_eq!(follow_up.entries[0].state, AgentState::Working);
        assert_eq!(follow_up.entries[1].primary_label, "second");
        assert_eq!(follow_up.entries[1].state, AgentState::Blocked);
        assert!(sections
            .iter()
            .all(|section| section.group != AgentStatusGroup::Working));
        assert!(sections
            .iter()
            .all(|section| section.group != AgentStatusGroup::Triage));
    }

    #[test]
    fn follow_up_equal_added_timestamps_keep_insertion_order() {
        let mut app = crate::app::state::AppState::test_new();
        let mut first = Workspace::test_new("first");
        let first_pane = first.tabs[0].root_pane;
        first.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Codex);
        first.tabs[0].panes.get_mut(&first_pane).unwrap().state = AgentState::Working;
        let mut second = Workspace::test_new("second");
        let second_pane = second.tabs[0].root_pane;
        second.tabs[0]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        second.tabs[0].panes.get_mut(&second_pane).unwrap().state = AgentState::Working;
        app.workspaces = vec![first, second];
        app.active = Some(0);
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        assert!(app.insert_agent_follow_up(0, first_pane));
        assert!(app.insert_agent_follow_up(1, second_pane));
        app.agent_follow_up[0].added_at_unix_secs = 40;
        app.agent_follow_up[1].added_at_unix_secs = 40;

        let sections = agent_panel_sections(&app);
        let follow_up = sections
            .iter()
            .find(|section| section.group == AgentStatusGroup::FollowUp)
            .expect("follow up");
        let labels: Vec<_> = follow_up
            .entries
            .iter()
            .map(|entry| entry.primary_label.as_str())
            .collect();
        assert_eq!(labels, vec!["first", "second"]);
    }

    #[test]
    fn triage_orders_oldest_meaningful_activity_first() {
        let mut app = crate::app::state::AppState::test_new();
        let mut older = Workspace::test_new("older");
        let older_pane = older.tabs[0].root_pane;
        older.tabs[0]
            .panes
            .get_mut(&older_pane)
            .unwrap()
            .detected_agent = Some(Agent::Codex);
        older.tabs[0].panes.get_mut(&older_pane).unwrap().state = AgentState::Blocked;
        let mut newer = Workspace::test_new("newer");
        let newer_pane = newer.tabs[0].root_pane;
        newer.tabs[0]
            .panes
            .get_mut(&newer_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        newer.tabs[0].panes.get_mut(&newer_pane).unwrap().state = AgentState::Blocked;
        let mut never = Workspace::test_new("never");
        let never_pane = never.tabs[0].root_pane;
        never.tabs[0]
            .panes
            .get_mut(&never_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        never.tabs[0].panes.get_mut(&never_pane).unwrap().state = AgentState::Blocked;
        app.workspaces = vec![older, newer, never];
        app.ensure_test_terminals();
        app.active = Some(0);
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        let terminals: Vec<_> = app
            .workspaces
            .iter()
            .map(|ws| {
                let pane = ws.tabs[0].root_pane;
                ws.tabs[0].panes[&pane].attached_terminal_id.clone()
            })
            .collect();
        app.terminals
            .get_mut(&terminals[0])
            .unwrap()
            .mark_meaningful_agent_activity(1, 20);
        app.terminals
            .get_mut(&terminals[1])
            .unwrap()
            .mark_meaningful_agent_activity(2, 30);
        let sections = agent_panel_sections(&app);
        let triage = sections
            .iter()
            .find(|section| section.group == AgentStatusGroup::Triage)
            .expect("triage");
        let labels: Vec<_> = triage
            .entries
            .iter()
            .map(|entry| entry.primary_label.as_str())
            .collect();
        assert_eq!(labels, vec!["never", "older", "newer"]);
    }

    #[test]
    fn follow_up_keeps_live_pane_after_agent_release() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("queued");
        let pane = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        app.active = Some(0);
        app.agent_panel_scope = AgentPanelScope::CurrentWorkspace;
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane]
            .attached_terminal_id
            .clone();
        let terminal = app.terminals.get_mut(&terminal_id).unwrap();
        terminal.set_hook_authority(
            "gardn:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            Some(1),
        );
        assert!(app.insert_agent_follow_up(0, pane));
        assert!(!agent_panel_sections(&app)
            .iter()
            .find(|section| section.group == AgentStatusGroup::FollowUp)
            .expect("follow up")
            .entries
            .is_empty());

        app.terminals
            .get_mut(&terminal_id)
            .unwrap()
            .release_agent("gardn:pi", "pi", Some(2));

        let follow_up = agent_panel_sections(&app)
            .into_iter()
            .find(|section| section.group == AgentStatusGroup::FollowUp)
            .expect("follow up");
        assert_eq!(follow_up.entries.len(), 1);
        assert_eq!(follow_up.entries[0].pane_id, pane);
        assert!(follow_up.entries[0].agent_label.is_none());

        let area = Rect::new(0, 0, 40, 24);
        crate::ui::compute_view(&mut app, area);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        let runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_sidebar(&app, &runtimes, frame, area))
            .expect("render sidebar");
        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("Follow Up"), "rendered UI:\n{text}");
        assert!(text.contains("queued"), "rendered UI:\n{text}");
    }

    #[test]
    fn expanded_empty_follow_up_renders_muted_indented_non_target_row() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("plain")];
        app.active = Some(0);
        app.selected = 0;

        let area = Rect::new(0, 0, 34, 18);
        let (_, agent_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");
        let runtimes = TerminalRuntimeRegistry::new();
        terminal
            .draw(|frame| render_sidebar(&app, &runtimes, frame, area))
            .expect("render sidebar");

        let body = agent_panel_body_rect(
            agent_area,
            agent_panel_scrollbar_rect(&app, agent_area, true).is_some(),
            true,
        );
        let empty_row = body.y + 1;
        let buffer = terminal.backend().buffer();
        assert_eq!(
            (0..4)
                .map(|offset| buffer[(body.x + offset, empty_row)].symbol())
                .collect::<String>(),
            "    "
        );
        assert_eq!(buffer[(body.x + 4, empty_row)].symbol(), "D");
        assert_eq!(
            buffer[(body.x + 4, empty_row)].style().fg,
            Some(app.palette.overlay0)
        );
        assert!(buffer[(body.x + 4, empty_row)]
            .style()
            .add_modifier
            .contains(Modifier::DIM));
        assert!(agent_panel_entry_at_row(&app, body, empty_row).is_none());
        assert!(agent_panel_header_target_at_row(&app, body, empty_row).is_none());
    }

    #[test]
    fn follow_up_empty_row_is_absent_when_collapsed_or_queued() {
        let mut collapsed = crate::app::state::AppState::test_new();
        collapsed.workspaces = vec![Workspace::test_new("plain")];
        collapsed.active = Some(0);
        collapsed.selected = 0;
        collapsed
            .collapsed_agent_sections
            .push("Follow Up".to_string());
        let area = Rect::new(0, 0, 34, 18);
        let runtimes = TerminalRuntimeRegistry::new();
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");
        terminal
            .draw(|frame| render_sidebar(&collapsed, &runtimes, frame, area))
            .expect("render collapsed Follow Up");
        assert!(
            !buffer_text(terminal.backend().buffer(), area.width, area.height)
                .contains("Drop an agent here")
        );

        let mut queued = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("queued");
        let pane = workspace.tabs[0].root_pane;
        let pane_state = workspace.tabs[0].panes.get_mut(&pane).unwrap();
        pane_state.detected_agent = Some(Agent::Codex);
        pane_state.state = AgentState::Working;
        queued.workspaces = vec![workspace];
        queued.active = Some(0);
        queued.selected = 0;
        assert!(queued.insert_agent_follow_up(0, pane));
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");
        terminal
            .draw(|frame| render_sidebar(&queued, &runtimes, frame, area))
            .expect("render queued Follow Up");
        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("queued"), "rendered UI:\n{text}");
        assert!(!text.contains("Drop an agent here"), "rendered UI:\n{text}");
    }

    #[test]
    fn empty_follow_up_counts_as_one_scroll_item_before_lower_sections() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("reachable");
        let pane = workspace.tabs[0].root_pane;
        let pane_state = workspace.tabs[0].panes.get_mut(&pane).unwrap();
        pane_state.detected_agent = Some(Agent::Codex);
        pane_state.state = AgentState::Working;
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;

        let area = Rect::new(0, 0, 30, 5);
        let body = agent_panel_body_rect(area, false, false);
        let metrics = agent_panel_scroll_metrics(&app, area, false);
        assert_eq!(metrics.viewport_rows, 1);
        assert_eq!(metrics.max_offset_from_bottom, 1);

        app.agent_panel_scroll = metrics.max_offset_from_bottom;
        let entry = agent_panel_entry_at_row(&app, body, body.y + 1)
            .expect("lower section entry should be reachable at maximum scroll");
        assert_eq!(entry.primary_label, "reachable");
    }

    fn first_cell_with_symbol(
        buffer: &Buffer,
        width: u16,
        height: u16,
        symbol: &str,
    ) -> Option<(u16, u16)> {
        for y in 0..height {
            for x in 0..width {
                if buffer[(x, y)].symbol() == symbol {
                    return Some((x, y));
                }
            }
        }
        None
    }

    fn buffer_text(buffer: &Buffer, width: u16, height: u16) -> String {
        let mut text = String::new();
        for y in 0..height {
            for x in 0..width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }
}
