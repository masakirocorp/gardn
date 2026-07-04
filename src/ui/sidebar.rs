use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::scrollbar::{render_scrollbar, should_show_scrollbar};
use super::status::{agent_icon, state_dot, state_label, state_label_color};
use super::widgets::fill_rect;
use crate::app::state::{AgentPanelScope, Palette};
use crate::app::{AppState, Mode};
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
const RIGHT_ENTRY_ICON_COL: u16 = 4;
const RIGHT_ENTRY_PRIMARY_COL: u16 = 6;
const RIGHT_SUBSECTION_MARKER_COL: u16 = 0;
const RIGHT_SUBSECTION_LABEL_COL: u16 = 2;

#[derive(Clone)]
pub(crate) struct AgentPanelEntry {
    pub ws_idx: usize,
    pub tab_idx: usize,
    pub pane_id: crate::layout::PaneId,
    pub group_context_idx: Option<usize>,
    pub primary_label: String,
    pub primary_tab_label: Option<String>,
    pub agent_label: Option<String>,
    pub state: AgentState,
    pub seen: bool,
    pub custom_status: Option<String>,
    pub state_labels: std::collections::HashMap<String, String>,
    pub last_meaningful_agent_activity_seq: u64,
    pub last_meaningful_agent_activity_unix_secs: Option<u64>,
}

pub(crate) struct AgentPanelSection {
    pub label: &'static str,
    pub entries: Vec<AgentPanelEntry>,
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

fn agent_panel_current_workspace_idx(app: &AppState) -> Option<usize> {
    let idx = if matches!(
        app.mode,
        Mode::Navigate
            | Mode::RenameWorkspace
            | Mode::RenameGroup
            | Mode::RenamePane
            | Mode::EditWorktreeDirectory
            | Mode::Resize
            | Mode::ConfirmClose
            | Mode::ConfirmDeleteGroup
            | Mode::ContextMenu
            | Mode::Settings
            | Mode::GlobalMenu
            | Mode::AgentMenu
            | Mode::KeybindHelp
            | Mode::CommandPalette
            | Mode::AgentProfilePicker
            | Mode::GitRepoPicker
            | Mode::ProductAnnouncement
    ) {
        Some(app.selected)
    } else {
        app.active
    }?;
    app.workspace_in_active_group(idx).then_some(idx)
}

fn agent_panel_toggle_label(scope: AgentPanelScope) -> &'static str {
    match scope {
        AgentPanelScope::CurrentWorkspace => "follow space",
        AgentPanelScope::CurrentGroup => "follow group",
        AgentPanelScope::AllWorkspaces => "all",
    }
}

fn agent_panel_group_idx(app: &AppState, ws_idx: usize) -> Option<usize> {
    let ws = app.workspaces.get(ws_idx)?;
    app.group_index_by_id(&ws.group_id)
}

fn agent_panel_workspace_label_from(
    app: &AppState,
    ws_idx: usize,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> String {
    app.workspaces
        .get(ws_idx)
        .map(|ws| ws.display_name_from(&app.terminals, terminal_runtimes))
        .unwrap_or_default()
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

fn agent_panel_entries_with_runtimes(
    app: &AppState,
    terminal_runtimes: Option<&TerminalRuntimeRegistry>,
    scope: AgentPanelScope,
) -> Vec<AgentPanelEntry> {
    let empty_runtimes;
    let terminal_runtimes = match terminal_runtimes {
        Some(terminal_runtimes) => terminal_runtimes,
        None => {
            empty_runtimes = TerminalRuntimeRegistry::new();
            &empty_runtimes
        }
    };

    match scope {
        AgentPanelScope::CurrentWorkspace => {
            let Some(ws_idx) = agent_panel_current_workspace_idx(app) else {
                return Vec::new();
            };
            let Some(ws) = app.workspaces.get(ws_idx) else {
                return Vec::new();
            };
            let multi_tab = ws.tabs.len() > 1;
            let workspace_label = ws.display_name_from(&app.terminals, terminal_runtimes);
            ws.pane_details(&app.terminals)
                .into_iter()
                .map(|detail| AgentPanelEntry {
                    ws_idx,
                    tab_idx: detail.tab_idx,
                    pane_id: detail.pane_id,
                    group_context_idx: None,
                    primary_label: workspace_label.clone(),
                    primary_tab_label: multi_tab.then_some(detail.tab_label),
                    agent_label: Some(detail.agent_label),
                    state: detail.state,
                    seen: detail.seen,
                    custom_status: detail.custom_status,
                    state_labels: detail.state_labels,
                    last_meaningful_agent_activity_seq: detail.last_meaningful_agent_activity_seq,
                    last_meaningful_agent_activity_unix_secs: detail
                        .last_meaningful_agent_activity_unix_secs,
                })
                .collect()
        }
        AgentPanelScope::CurrentGroup => {
            let group_id = app
                .active
                .and_then(|idx| app.workspaces.get(idx))
                .map(|ws| ws.group_id.as_str())
                .unwrap_or_else(|| app.active_group_id());
            app.workspaces
                .iter()
                .enumerate()
                .filter(|(_, ws)| ws.group_id == group_id)
                .flat_map(|(ws_idx, ws)| {
                    let multi_tab = ws.tabs.len() > 1;
                    let workspace_label = ws.display_name_from(&app.terminals, terminal_runtimes);
                    ws.pane_details(&app.terminals)
                        .into_iter()
                        .map(move |detail| AgentPanelEntry {
                            ws_idx,
                            tab_idx: detail.tab_idx,
                            pane_id: detail.pane_id,
                            group_context_idx: None,
                            primary_label: workspace_label.clone(),
                            primary_tab_label: multi_tab.then_some(detail.tab_label),
                            agent_label: Some(detail.agent_label),
                            state: detail.state,
                            seen: detail.seen,
                            custom_status: detail.custom_status,
                            state_labels: detail.state_labels,
                            last_meaningful_agent_activity_seq: detail
                                .last_meaningful_agent_activity_seq,
                            last_meaningful_agent_activity_unix_secs: detail
                                .last_meaningful_agent_activity_unix_secs,
                        })
                })
                .collect()
        }
        AgentPanelScope::AllWorkspaces => app
            .workspaces
            .iter()
            .enumerate()
            .flat_map(|(ws_idx, ws)| {
                let multi_tab = ws.tabs.len() > 1;
                let workspace_label =
                    agent_panel_workspace_label_from(app, ws_idx, terminal_runtimes);
                ws.pane_details(&app.terminals)
                    .into_iter()
                    .map(move |detail| AgentPanelEntry {
                        ws_idx,
                        tab_idx: detail.tab_idx,
                        pane_id: detail.pane_id,
                        group_context_idx: agent_panel_has_multiple_groups(app)
                            .then(|| agent_panel_group_idx(app, ws_idx))
                            .flatten(),
                        primary_label: workspace_label.clone(),
                        primary_tab_label: multi_tab.then_some(detail.tab_label),
                        agent_label: Some(detail.agent_label),
                        state: detail.state,
                        seen: detail.seen,
                        custom_status: detail.custom_status,
                        state_labels: detail.state_labels,
                        last_meaningful_agent_activity_seq: detail
                            .last_meaningful_agent_activity_seq,
                        last_meaningful_agent_activity_unix_secs: detail
                            .last_meaningful_agent_activity_unix_secs,
                    })
            })
            .collect(),
    }
}

fn agent_panel_entry_needs_triage(entry: &AgentPanelEntry) -> bool {
    entry.state == AgentState::Blocked || (entry.state == AgentState::Idle && !entry.seen)
}

#[cfg(test)]
pub(crate) fn agent_panel_triage_entries(app: &AppState) -> Vec<AgentPanelEntry> {
    let empty_runtimes = TerminalRuntimeRegistry::new();
    agent_panel_triage_entries_from(app, &empty_runtimes)
}

fn agent_panel_triage_entries_from(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Vec<AgentPanelEntry> {
    app.workspaces
        .iter()
        .enumerate()
        .flat_map(|(ws_idx, ws)| {
            let multi_tab = ws.tabs.len() > 1;
            let workspace_label = agent_panel_workspace_label_from(app, ws_idx, terminal_runtimes);
            ws.pane_details(&app.terminals)
                .into_iter()
                .map(move |detail| AgentPanelEntry {
                    ws_idx,
                    tab_idx: detail.tab_idx,
                    pane_id: detail.pane_id,
                    group_context_idx: agent_panel_has_multiple_groups(app)
                        .then(|| agent_panel_group_idx(app, ws_idx))
                        .flatten(),
                    primary_label: workspace_label.clone(),
                    primary_tab_label: multi_tab.then_some(detail.tab_label),
                    agent_label: Some(detail.agent_label),
                    state: detail.state,
                    seen: detail.seen,
                    custom_status: detail.custom_status,
                    state_labels: detail.state_labels,
                    last_meaningful_agent_activity_seq: detail.last_meaningful_agent_activity_seq,
                    last_meaningful_agent_activity_unix_secs: detail
                        .last_meaningful_agent_activity_unix_secs,
                })
        })
        .filter(agent_panel_entry_needs_triage)
        .collect()
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
    });
}

pub(crate) fn agent_panel_sections(app: &AppState) -> Vec<AgentPanelSection> {
    let empty_runtimes = TerminalRuntimeRegistry::new();
    agent_panel_sections_from(app, &empty_runtimes)
}

fn agent_panel_sections_from(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Vec<AgentPanelSection> {
    let scoped_entries = agent_panel_entries_from(app, terminal_runtimes);
    let mut sections = Vec::new();

    let mut triage = agent_panel_triage_entries_from(app, terminal_runtimes);
    sort_agent_panel_entries_by_recent_activity(&mut triage);
    if !triage.is_empty() {
        sections.push(AgentPanelSection {
            label: "triage",
            entries: triage,
        });
    }

    let mut working: Vec<_> = scoped_entries
        .iter()
        .filter(|entry| entry.state == AgentState::Working)
        .cloned()
        .collect();
    sort_agent_panel_entries_by_recent_activity(&mut working);
    if !working.is_empty() {
        sections.push(AgentPanelSection {
            label: "working",
            entries: working,
        });
    }

    let mut idle: Vec<_> = scoped_entries
        .into_iter()
        .filter(|entry| {
            entry.state != AgentState::Working && !agent_panel_entry_needs_triage(entry)
        })
        .collect();
    sort_agent_panel_entries_by_recent_activity(&mut idle);
    if !idle.is_empty() {
        sections.push(AgentPanelSection {
            label: "idle",
            entries: idle,
        });
    }

    sections
}
pub(super) fn agent_panel_status_key(state: AgentState, seen: bool) -> &'static str {
    match (state, seen) {
        (AgentState::Idle, false) => "done",
        (AgentState::Idle, true) => "idle",
        (AgentState::Working, _) => "working",
        (AgentState::Blocked, _) => "blocked",
        (AgentState::Unknown, _) => "unknown",
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

fn format_agent_panel_primary_label(entry: &AgentPanelEntry, max_width: usize) -> String {
    let Some(tab_label) = entry.primary_tab_label.as_deref() else {
        return truncate_text(&entry.primary_label, max_width);
    };

    let separator = " · ";
    let separator_width = separator.chars().count();
    if max_width <= separator_width + 2 {
        return truncate_text(
            &format!("{}{}{}", entry.primary_label, separator, tab_label),
            max_width,
        );
    }

    let available = max_width.saturating_sub(separator_width);
    let min_tab = 4.min(available.saturating_sub(1)).max(1);
    let preferred_workspace = ((available * 2) / 3).max(1);
    let mut workspace_budget = preferred_workspace
        .min(available.saturating_sub(min_tab))
        .max(1);
    let mut tab_budget = available.saturating_sub(workspace_budget);

    let workspace_len = entry.primary_label.chars().count();
    let tab_len = tab_label.chars().count();

    if workspace_len < workspace_budget {
        let spare = workspace_budget - workspace_len;
        workspace_budget = workspace_len;
        tab_budget = (tab_budget + spare).min(available.saturating_sub(workspace_budget));
    }
    if tab_len < tab_budget {
        let spare = tab_budget - tab_len;
        tab_budget = tab_len;
        workspace_budget = (workspace_budget + spare).min(available.saturating_sub(tab_budget));
    }

    format!(
        "{}{}{}",
        truncate_text(&entry.primary_label, workspace_budget),
        separator,
        truncate_text(tab_label, tab_budget)
    )
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
        return Some("now".to_string());
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

fn agent_panel_primary_label_line(
    entry: &AgentPanelEntry,
    max_width: usize,
    base_style: Style,
) -> Line<'static> {
    Line::from(Span::styled(
        format_agent_panel_primary_label(entry, max_width),
        base_style,
    ))
}

fn agent_panel_section_shows_entry_status(section_label: &str) -> bool {
    section_label == "triage"
}

fn agent_panel_section_collapsed(app: &AppState, section_label: &str) -> bool {
    app.agent_section_collapsed(section_label)
}

fn agent_panel_section_display_label(section_label: &str) -> &str {
    match section_label {
        "triage" => "triage · all spaces",
        label => label,
    }
}

fn agent_panel_entry_status_label(entry: &AgentPanelEntry) -> &'static str {
    state_label(entry.state, entry.seen)
}

fn agent_panel_section_header_style(section: &AgentPanelSection, p: &Palette) -> Style {
    let color = match section.label {
        "triage" => p.peach,
        "working" => state_label_color(AgentState::Working, true, p),
        "idle" => state_label_color(AgentState::Idle, true, p),
        _ => p.overlay0,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}
fn right_entry_primary_prefix(icon: &'static str, icon_style: Style) -> Vec<Span<'static>> {
    vec![
        Span::styled(" ".repeat(RIGHT_ENTRY_ICON_COL as usize), Style::default()),
        Span::styled(icon, icon_style),
        Span::styled(
            " ".repeat(RIGHT_ENTRY_PRIMARY_COL.saturating_sub(RIGHT_ENTRY_ICON_COL + 1) as usize),
            Style::default(),
        ),
    ]
}

fn right_entry_detail_prefix(p: &Palette) -> Vec<Span<'static>> {
    vec![
        Span::styled(" ".repeat(RIGHT_ENTRY_ICON_COL as usize), Style::default()),
        Span::styled(
            "│",
            Style::default().fg(p.overlay0).add_modifier(Modifier::DIM),
        ),
        Span::styled(
            " ".repeat(RIGHT_ENTRY_PRIMARY_COL.saturating_sub(RIGHT_ENTRY_ICON_COL + 1) as usize),
            Style::default(),
        ),
    ]
}

fn workspace_row_height(_ws: &crate::workspace::Workspace) -> u16 {
    2
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

pub(crate) fn agent_panel_body_rect(
    area: Rect,
    has_scrollbar: bool,
    _leading_separator: bool,
) -> Rect {
    let header_rows = AGENT_PANEL_HEADER_ROWS;
    if area.width == 0 || area.height <= header_rows {
        return Rect::default();
    }

    let body_y = area.y.saturating_add(header_rows);
    let body_height = (area.y + area.height).saturating_sub(body_y);
    let body_width = area.width.saturating_sub(u16::from(has_scrollbar));
    Rect::new(area.x, body_y, body_width, body_height)
}

fn agent_panel_visible_count(app: &AppState, area: Rect, leading_separator: bool) -> usize {
    let body = agent_panel_body_rect(area, false, leading_separator);
    if body.width == 0 || body.height < 2 {
        return 0;
    }

    let mut remaining_rows = body.height;
    let mut visible = 0usize;
    let mut skip = app.agent_panel_scroll;
    for section in agent_panel_sections(app) {
        if agent_panel_section_collapsed(app, section.label) {
            if remaining_rows < 1 {
                break;
            }
            remaining_rows = remaining_rows.saturating_sub(1);
            continue;
        }
        if skip >= section.entries.len() {
            skip -= section.entries.len();
            continue;
        }
        if remaining_rows < 3 {
            break;
        }

        remaining_rows = remaining_rows.saturating_sub(1);
        for _ in section.entries.iter().skip(skip) {
            if remaining_rows < 2 {
                break;
            }
            remaining_rows = remaining_rows.saturating_sub(2);
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
        .filter(|section| !agent_panel_section_collapsed(app, section.label))
        .map(|section| section.entries.len())
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
    for (entry_pos, entry) in entries
        .iter()
        .copied()
        .enumerate()
        .skip(app.workspace_scroll)
    {
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
                let row_height = workspace_row_height(ws);
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
                            workspace_after_drop_row(
                                &entries,
                                entry_pos,
                                row_y,
                                row_height,
                                body_bottom,
                            ),
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

fn workspace_after_drop_row(
    entries: &[WorkspaceListEntry],
    entry_pos: usize,
    row_y: u16,
    row_height: u16,
    body_bottom: u16,
) -> u16 {
    let after_row = row_y.saturating_add(row_height);
    if matches!(
        entries.get(entry_pos + 1),
        Some(WorkspaceListEntry::Workspace { .. })
    ) {
        after_row.min(body_bottom.saturating_sub(1))
    } else {
        after_row
            .saturating_sub(1)
            .min(body_bottom.saturating_sub(1))
    }
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
        match self {
            Self::GroupHeader { .. } | Self::EmptyGroup { .. } | Self::GroupGap => 1,
            Self::Workspace { ws_idx, .. } => app
                .workspaces
                .get(ws_idx)
                .map(workspace_row_height)
                .unwrap_or(0),
        }
    }
}

#[derive(Clone, Copy)]
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

pub(crate) fn collapsed_workspace_at_row(app: &AppState, area: Rect, row: u16) -> Option<usize> {
    let rows = collapsed_workspace_rows_rect(area, true);
    if rows == Rect::default() || row < rows.y || row >= rows.y + rows.height {
        return None;
    }
    let idx = (row - rows.y) as usize;
    match collapsed_workspace_row_entries(app).get(idx).copied()? {
        CollapsedWorkspaceRowEntry::Workspace { ws_idx, .. } => Some(ws_idx),
        CollapsedWorkspaceRowEntry::GroupHeader { .. } => None,
    }
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

/// Auto-scale sidebar width based on workspace identity + agent summary.
pub(crate) fn collapsed_sidebar_sections(
    area: Rect,
    show_agent_detail: bool,
) -> (Rect, Option<u16>, Rect) {
    collapsed_sidebar_sections_with_separator(area, show_agent_detail, false)
}

fn right_aligned_collapsed_sidebar_sections(
    area: Rect,
    show_agent_detail: bool,
) -> (Rect, Option<u16>, Rect) {
    collapsed_sidebar_sections_with_separator(area, show_agent_detail, true)
}

fn collapsed_sidebar_sections_with_separator(
    area: Rect,
    show_agent_detail: bool,
    separator_on_left: bool,
) -> (Rect, Option<u16>, Rect) {
    let content = sidebar_content_rect(area, separator_on_left);
    if content == Rect::default() {
        return (Rect::default(), None, Rect::default());
    }

    if !show_agent_detail {
        return (content, None, Rect::default());
    }

    if content.height < 7 {
        return (content, None, Rect::default());
    }

    let total_h = content.height as usize;
    let ws_h = total_h.div_ceil(2);
    let detail_h = total_h.saturating_sub(ws_h + 1);
    if ws_h == 0 || detail_h == 0 {
        return (content, None, Rect::default());
    }

    let divider_y = content.y + ws_h as u16;
    let ws_area = Rect::new(content.x, content.y, content.width, ws_h as u16);
    let detail_area = Rect::new(content.x, divider_y + 1, content.width, detail_h as u16);
    (ws_area, Some(divider_y), detail_area)
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

pub(crate) fn collapsed_workspace_rows_rect(area: Rect, show_agent_detail: bool) -> Rect {
    let (ws_area, _, _) = collapsed_sidebar_sections(area, show_agent_detail);
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
        "all".to_string()
    }
}

/// Collapsed sidebar: workspace glance plus compact agent list.
fn sidebar_is_combined_right(app: &AppState) -> bool {
    app.view.right_sidebar_rect == Rect::default()
        && app.sidebar_arrangement == crate::config::SidebarArrangementConfig::CombinedRight
}

pub(super) fn render_sidebar_collapsed(app: &AppState, frame: &mut Frame, area: Rect) {
    let is_navigating = matches!(app.mode, Mode::Navigate);
    let show_agent_detail = true;

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

    let (ws_area, divider_y, detail_area) = if combined_right {
        right_aligned_collapsed_sidebar_sections(area, show_agent_detail)
    } else {
        collapsed_sidebar_sections(area, show_agent_detail)
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
                let (icon, icon_style) = state_dot(agg_state, agg_seen, p);
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
                    Paragraph::new(Line::from(vec![
                        Span::styled(format!("{ordinal}"), num_style),
                        Span::styled(" ", row_style),
                        Span::styled(icon, icon_style),
                    ])),
                    Rect::new(workspace_rows.x, y, workspace_rows.width, 1),
                );
            }
        }
    }

    if let Some(divider_y) = divider_y {
        let buf = frame.buffer_mut();
        for x in ws_area.x..ws_area.x + ws_area.width {
            buf[(x, divider_y)].set_symbol("─");
            buf[(x, divider_y)].set_style(Style::default().fg(p.overlay0));
        }
    }

    if !show_agent_detail {
        render_sidebar_toggle(app, frame, area, true, p);
        return;
    }

    let detail_ws_idx = if is_navigating {
        Some(app.selected)
    } else {
        app.active
    };
    if detail_area != Rect::default() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "agt",
                Style::default().fg(p.overlay1).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            Rect::new(detail_area.x, detail_area.y, detail_area.width, 1),
        );
    }
    if detail_area.height > 1 {
        let buf = frame.buffer_mut();
        let divider_y = detail_area.y + 1;
        for x in detail_area.x..detail_area.x + detail_area.width {
            buf[(x, divider_y)].set_symbol("─");
            buf[(x, divider_y)].set_style(Style::default().fg(p.overlay0));
        }
    }
    let detail_content_area = Rect::new(
        detail_area.x,
        detail_area.y.saturating_add(COLLAPSED_SECTION_HEADER_ROWS),
        detail_area.width,
        detail_area
            .height
            .saturating_sub(COLLAPSED_SECTION_HEADER_ROWS),
    );
    if detail_content_area != Rect::default() {
        if let Some(ws_idx) = detail_ws_idx {
            if let Some(ws) = app.workspaces.get(ws_idx) {
                for (detail_idx, detail) in ws.pane_details(&app.terminals).iter().enumerate() {
                    let y = detail_content_area.y + detail_idx as u16;
                    if y >= detail_content_area.y + detail_content_area.height {
                        break;
                    }
                    let pane_num = ws
                        .public_pane_number(detail.pane_id)
                        .unwrap_or(detail_idx + 1);
                    let pane_style = Style::default().fg(p.overlay0);
                    let (icon, icon_style) =
                        agent_icon(detail.state, detail.seen, app.spinner_tick, p);
                    frame.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled(format!("{pane_num}"), pane_style),
                            Span::styled(" ", pane_style),
                            Span::styled(icon, icon_style),
                        ])),
                        Rect::new(detail_content_area.x, y, detail_content_area.width, 1),
                    );
                }
            }
        }
    }

    render_sidebar_toggle(app, frame, area, true, p);
}

pub(crate) fn workspace_drop_indicator_row(
    cards: &[crate::app::state::WorkspaceCardArea],
    _area: Rect,
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
            card.rect
                .y
                .saturating_add(card.rect.height.saturating_sub(1))
        })
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
    render_sidebar_toggle(app, frame, area, false, p);
}

fn render_collapsed_agent_rail(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
    p: &Palette,
) {
    let content = right_sidebar_content_rect(area);
    if content == Rect::default() {
        return;
    }

    frame.render_widget(
        Paragraph::new(Span::styled(
            "agt",
            Style::default().fg(p.overlay1).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Rect::new(content.x, content.y, content.width, 1),
    );
    if content.height > 1 {
        let buf = frame.buffer_mut();
        let divider_y = content.y + 1;
        for x in content.x..content.x + content.width {
            buf[(x, divider_y)].set_symbol("─");
            buf[(x, divider_y)].set_style(Style::default().fg(p.overlay0));
        }
    }

    let rows = Rect::new(
        content.x,
        content.y.saturating_add(COLLAPSED_SECTION_HEADER_ROWS),
        content.width,
        content
            .height
            .saturating_sub(COLLAPSED_SECTION_HEADER_ROWS + 1),
    );
    if rows == Rect::default() {
        return;
    }

    let entries = agent_panel_sections_from(app, terminal_runtimes)
        .into_iter()
        .flat_map(|section| section.entries)
        .collect::<Vec<_>>();
    for (idx, entry) in entries.iter().enumerate() {
        let y = rows.y + idx as u16;
        if y >= rows.y + rows.height {
            break;
        }
        let (icon, icon_style) = agent_icon(entry.state, entry.seen, app.spinner_tick, p);
        let num_style = Style::default().fg(p.overlay0);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{}", idx + 1), num_style),
                Span::styled(" ", num_style),
                Span::styled(icon, icon_style),
            ])),
            Rect::new(rows.x, y, rows.width, 1),
        );
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
                "spaces",
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
            " no spaces"
        } else {
            " empty group"
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
        let selected = i == app.selected && is_navigating;
        let is_active = Some(i) == app.active;
        let is_dragged = dragged_ws_idx == Some(i);
        let highlighted = selected || is_active || is_dragged;
        let (agg_state, agg_seen) = ws.aggregate_state(&app.terminals);

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

        let (icon, icon_style) = state_dot(agg_state, agg_seen, p);
        let line1 = vec![
            Span::styled(
                " ".repeat(SIDEBAR_WORKSPACE_STATE_COL as usize),
                Style::default(),
            ),
            Span::styled(icon, icon_style),
            Span::styled(
                " ".repeat(
                    SIDEBAR_WORKSPACE_NAME_COL.saturating_sub(SIDEBAR_WORKSPACE_STATE_COL + 1)
                        as usize,
                ),
                Style::default(),
            ),
            Span::styled(
                ws.display_name_from(&app.terminals, terminal_runtimes),
                name_style,
            ),
        ];

        frame.render_widget(
            Paragraph::new(Line::from(line1)),
            Rect::new(card.rect.x, row_y, card.rect.width, 1),
        );

        if row_height > 1 && row_y + 1 < list_bottom {
            let max_summary_len =
                (card.rect.width as usize).saturating_sub(SIDEBAR_WORKSPACE_NAME_COL as usize);
            let mut spans = vec![
                Span::styled(
                    " ".repeat(SIDEBAR_WORKSPACE_STATE_COL as usize),
                    Style::default(),
                ),
                Span::styled(
                    "│",
                    Style::default().fg(p.overlay0).add_modifier(Modifier::DIM),
                ),
                Span::styled(
                    " ".repeat(
                        SIDEBAR_WORKSPACE_NAME_COL.saturating_sub(SIDEBAR_WORKSPACE_STATE_COL + 1)
                            as usize,
                    ),
                    Style::default(),
                ),
            ];
            spans.extend(workspace_summary_spans(ws, p, max_summary_len));
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(card.rect.x, row_y + 1, card.rect.width, 1),
            );
        }
    }

    if let Some(y) = insertion_row.filter(|y| *y < list_bottom) {
        let indicator_right = scrollbar_rect
            .map(|rect| rect.x)
            .unwrap_or(area.x + area.width);
        let buf = frame.buffer_mut();
        for x in area.x..indicator_right {
            buf[(x, y)].set_symbol("─");
            buf[(x, y)].set_style(Style::default().fg(app.active_workspace_accent_color()));
        }
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }
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
                &format!("{} repos", summary.repo_count),
                p.overlay0,
                max_width,
            )];
        }
        return Vec::new();
    }

    let mut pieces = Vec::new();
    if summary.repo_count > 1 {
        pieces.push((format!("{} repos ·", summary.repo_count), p.overlay0));
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
    detail: &AgentPanelEntry,
    area: Rect,
    row_y: u16,
) {
    let p = &app.palette;
    let is_active = app.is_active_pane(detail.ws_idx, detail.tab_idx, detail.pane_id);
    let (icon, icon_style) = agent_icon(detail.state, detail.seen, app.spinner_tick, p);
    let label_color = state_label_color(detail.state, detail.seen, p);
    let label = detail
        .state_labels
        .get(agent_panel_status_key(detail.state, detail.seen))
        .map(String::as_str)
        .unwrap_or_else(|| agent_panel_entry_status_label(detail));

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
    let status_style = if is_active {
        Style::default().fg(label_color)
    } else {
        Style::default().fg(label_color).add_modifier(Modifier::DIM)
    };
    let agent_style = Style::default().fg(p.overlay0).add_modifier(Modifier::DIM);

    let mut primary_line = agent_panel_primary_label_line(
        detail,
        area.width.saturating_sub(RIGHT_ENTRY_PRIMARY_COL) as usize,
        name_style,
    );
    let mut name_spans = right_entry_primary_prefix(icon, icon_style);
    name_spans.append(&mut primary_line.spans);
    frame.render_widget(
        Paragraph::new(Line::from(name_spans)).style(row_style),
        Rect::new(area.x, row_y, area.width, 1),
    );

    let mut status_spans = right_entry_detail_prefix(p);
    let mut has_secondary_detail = false;
    if let Some(group_idx) = detail.group_context_idx {
        if let Some(group) = app.groups.get(group_idx) {
            status_spans.push(Span::styled(
                group.name.clone(),
                Style::default()
                    .fg(app.group_accent_color(group_idx))
                    .add_modifier(Modifier::DIM),
            ));
            has_secondary_detail = true;
        }
    }
    if let Some(agent_label) = &detail.agent_label {
        if has_secondary_detail {
            status_spans.push(Span::styled(" · ", agent_style));
        }
        status_spans.push(Span::styled(agent_label.clone(), agent_style));
        has_secondary_detail = true;
    }
    if show_status {
        if has_secondary_detail {
            status_spans.push(Span::styled(" · ", agent_style));
        }
        status_spans.push(Span::styled(label, status_style));
        has_secondary_detail = true;
    }
    if let Some(custom_status) = &detail.custom_status {
        if has_secondary_detail {
            status_spans.push(Span::styled(" · ", agent_style));
        }
        status_spans.push(Span::styled(custom_status.clone(), agent_style));
        has_secondary_detail = true;
    }
    if let Some(age_label) = format_agent_activity_age(
        detail.last_meaningful_agent_activity_unix_secs,
        current_unix_secs(),
    ) {
        if has_secondary_detail {
            status_spans.push(Span::styled(" · ", agent_style));
        }
        status_spans.push(Span::styled(age_label, agent_style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(status_spans)).style(row_style),
        Rect::new(area.x, row_y + 1, area.width, 1),
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

    for section in agent_panel_sections(app) {
        let collapsed = agent_panel_section_collapsed(app, section.label);
        if collapsed {
            row_y = row_y.saturating_add(1);
            continue;
        }
        if skip >= section.entries.len() {
            skip -= section.entries.len();
            continue;
        }
        if row_y >= body_bottom {
            break;
        }

        row_y = row_y.saturating_add(1);
        for detail in section.entries.iter().skip(skip) {
            if row_y.saturating_add(1) >= body_bottom {
                break;
            }
            if row == row_y || row == row_y + 1 {
                return Some(detail.clone());
            }
            row_y = row_y.saturating_add(2);
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

    let count_label = section.entries.len().to_string();
    let count_width = count_label.chars().count() as u16;
    let label_width = body.width.saturating_sub(
        RIGHT_SUBSECTION_LABEL_COL + count_width + RIGHT_SECTION_COUNT_RIGHT_PAD + 1,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            truncate_text(
                agent_panel_section_display_label(section.label),
                label_width as usize,
            ),
            style,
        )),
        Rect::new(
            body.x + RIGHT_SUBSECTION_LABEL_COL.min(body.width.saturating_sub(1)),
            row_y,
            label_width,
            1,
        ),
    );
    if body.width > count_width + RIGHT_SECTION_COUNT_RIGHT_PAD {
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

    for section in agent_panel_sections(app) {
        let collapsed = agent_panel_section_collapsed(app, section.label);
        if !collapsed && skip >= section.entries.len() {
            skip -= section.entries.len();
            continue;
        }
        if row_y >= body_bottom {
            break;
        }

        if row == row_y {
            return Some(AgentPanelHeaderTarget {
                section: section.label.to_string(),
            });
        }
        row_y = row_y.saturating_add(1);

        if collapsed {
            continue;
        }
        row_y = row_y
            .saturating_add((section.entries.len().saturating_sub(skip) as u16).saturating_mul(2));
        skip = 0;
    }

    None
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
            "agents",
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
    if sections.is_empty() && body.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " no agents",
                Style::default().fg(p.overlay0).add_modifier(Modifier::DIM),
            )),
            Rect::new(body.x, body.y, body.width, 1),
        );
        if let Some(track) = scrollbar_rect {
            render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
        }
        return;
    }

    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut skip = app.agent_panel_scroll;
    for section in sections {
        let collapsed = agent_panel_section_collapsed(app, section.label);
        if collapsed {
            if row_y >= body_bottom {
                break;
            }
            render_agent_section_header(app, frame, &section, true, body, row_y);
            row_y = row_y.saturating_add(1);
            continue;
        }
        if skip >= section.entries.len() {
            skip -= section.entries.len();
            continue;
        }
        if row_y >= body_bottom {
            break;
        }

        render_agent_section_header(app, frame, &section, false, body, row_y);
        row_y = row_y.saturating_add(1);
        let show_status = agent_panel_section_shows_entry_status(section.label);

        for detail in section.entries.iter().skip(skip) {
            if row_y.saturating_add(1) >= body_bottom {
                break;
            }
            render_agent_entry(app, frame, show_status, detail, body, row_y);
            row_y = row_y.saturating_add(2);
        }
        skip = 0;
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }
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

pub(crate) fn expanded_sidebar_toggle_rect(area: Rect) -> Rect {
    let bottom_y = area.y + area.height.saturating_sub(1);
    let content_w = area.width.saturating_sub(1);
    if content_w == 0 || area.height == 0 {
        return Rect::default();
    }
    Rect::new(area.x + content_w.saturating_sub(1), bottom_y, 1, 1)
}

fn render_sidebar_toggle(
    app: &AppState,
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
    let icon_style = if app.global_menu_attention_badge_visible() {
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.overlay0)
    };
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
    fn agent_panel_toggle_labels_match_control_center_scope() {
        assert_eq!(
            agent_panel_toggle_label(AgentPanelScope::CurrentWorkspace),
            "follow space"
        );
        assert_eq!(
            agent_panel_toggle_label(AgentPanelScope::CurrentGroup),
            "follow group"
        );
        assert_eq!(
            agent_panel_toggle_label(AgentPanelScope::AllWorkspaces),
            "all"
        );
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
        assert!(text.contains("empty group"));
        assert!(rows[2].contains("empty group"));
        assert!(!text.contains("new space adds one here"));
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
        assert!(!text.contains("new space"));
        assert!(!text.contains(" new"));
        assert!(!text.contains("menu"));
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
            default_directory: None,
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
        assert!(text.contains("▾ ☀ group 1"));
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
            "g"
        );
        assert_eq!(
            buffer[(group_header.x + group_header.width - 2, group_header.y)].symbol(),
            "1"
        );
        assert_eq!(
            buffer[(home_card.x + SIDEBAR_WORKSPACE_NAME_COL, home_card.y)].symbol(),
            "h"
        );
        assert_eq!(
            buffer[(home_card.x + SIDEBAR_WORKSPACE_STATE_COL, home_card.y + 1)].symbol(),
            "│"
        );
        assert_eq!(
            buffer[(home_card.x + SIDEBAR_WORKSPACE_NAME_COL, home_card.y + 1)].symbol(),
            " "
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
            default_directory: None,
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

        assert_eq!(rows[0], "all│");
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
    fn collapsed_all_groups_rail_resets_ordinals_after_each_group_header() {
        let mut app = crate::app::state::AppState::test_new();
        app.group_filter_enabled = false;
        app.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_directory: None,
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
            default_directory: None,
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
    fn collapsed_left_agent_area_renders_agt_divider_then_agent_rows_without_status_headers() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("agents");
        let first_pane = workspace.tabs[0].root_pane;
        let second_tab = workspace.test_add_tab(Some("logs"));
        let second_pane = workspace.tabs[second_tab].root_pane;
        workspace.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Codex);
        workspace.tabs[0].panes.get_mut(&first_pane).unwrap().state = AgentState::Working;
        workspace.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        workspace.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .state = AgentState::Idle;
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;

        let area = Rect::new(0, 0, 4, 20);
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

        assert_eq!(rows[detail_area.y as usize], "agt│");
        assert_eq!(rows[(detail_area.y + 1) as usize], "───│");
        assert_eq!(buffer[(detail_area.x, detail_area.y + 2)].symbol(), "1");
        assert_eq!(buffer[(detail_area.x, detail_area.y + 3)].symbol(), "2");
        assert_ne!(buffer[(detail_area.x, detail_area.y + 2)].symbol(), "▾");
        assert_ne!(buffer[(detail_area.x, detail_area.y + 2)].symbol(), "▸");
        assert_ne!(buffer[(detail_area.x, detail_area.y + 3)].symbol(), "▾");
        assert_ne!(buffer[(detail_area.x, detail_area.y + 3)].symbol(), "▸");
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
            default_directory: None,
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
            default_directory: None,
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
        assert!(text.contains("no spaces"));
    }

    #[test]
    fn activity_age_label_uses_compact_units() {
        let now = 1_000_000;

        assert_eq!(
            format_agent_activity_age(Some(now - 59), now),
            Some("now".to_string())
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
    fn all_workspaces_agent_panel_entries_use_workspace_and_optional_tab_labels() {
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
        app.terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::OhMyPi);
        let second_terminal_id = app.workspaces[1].tabs[second_tab].panes[&second_pane]
            .attached_terminal_id
            .clone();
        app.terminals
            .get_mut(&second_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let entries = agent_panel_entries(&app);
        assert_eq!(entries[0].primary_label, "one");
        assert!(entries[0].primary_tab_label.is_none());
        assert_eq!(entries[0].agent_label.as_deref(), Some("omp"));
        assert_eq!(entries[1].primary_label, "two");
        assert_eq!(entries[1].primary_tab_label.as_deref(), Some("logs"));
        assert_eq!(entries[1].agent_label.as_deref(), Some("claude"));
    }

    #[tokio::test]
    async fn all_workspaces_agent_panel_entries_use_live_root_runtime_cwd_for_workspace_label() {
        let unique = format!(
            "hako-agent-panel-runtime-cwd-{}-{}",
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

        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("stale-name");
        workspace.custom_name = None;
        workspace.identity_cwd = stale_cwd.clone();
        let pane = workspace.tabs[0].root_pane;

        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane]
            .attached_terminal_id
            .clone();
        let terminal = app.terminals.get_mut(&terminal_id).unwrap();
        terminal.cwd = stale_cwd;
        terminal.detected_agent = Some(Agent::Pi);
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

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

        let mut runtime_registry = TerminalRuntimeRegistry::new();
        runtime_registry.insert(terminal_id, runtime);
        let entries = agent_panel_entries_from(&app, &runtime_registry);
        let primary_label = entries[0].primary_label.clone();

        for (_, runtime) in runtime_registry.drain() {
            runtime.shutdown();
        }
        let _ = std::fs::remove_dir_all(root);

        assert_eq!(primary_label, "hako");
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
    fn triage_agent_panel_entries_include_actionable_agents_from_all_groups() {
        let mut app = crate::app::state::AppState::test_new();
        let work_group = app.create_group("Work".to_string());

        let mut first = Workspace::test_new("done");
        let first_pane = first.tabs[0].root_pane;
        let first_pane_state = first.tabs[0].panes.get_mut(&first_pane).unwrap();
        first_pane_state.detected_agent = Some(Agent::Pi);
        first_pane_state.state = AgentState::Idle;
        first_pane_state.seen = false;

        let mut second = Workspace::test_new("blocked");
        second.group_id = app.groups[work_group].id.clone();
        let second_pane = second.tabs[0].root_pane;
        let second_pane_state = second.tabs[0].panes.get_mut(&second_pane).unwrap();
        second_pane_state.detected_agent = Some(Agent::Claude);
        second_pane_state.state = AgentState::Blocked;
        second_pane_state.seen = true;

        let mut third = Workspace::test_new("working");
        let third_pane = third.tabs[0].root_pane;
        let third_pane_state = third.tabs[0].panes.get_mut(&third_pane).unwrap();
        third_pane_state.detected_agent = Some(Agent::Codex);
        third_pane_state.state = AgentState::Working;
        third_pane_state.seen = false;

        app.workspaces = vec![first, second, third];
        app.active_group = 0;
        app.group_filter_enabled = true;

        let entries = agent_panel_triage_entries(&app);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].primary_label, "done");
        assert_eq!(entries[0].group_context_idx, Some(0));
        assert_eq!(entries[1].primary_label, "blocked");
        assert_eq!(entries[1].group_context_idx, Some(work_group));
    }

    #[test]
    fn agent_panel_sections_order_actionable_before_working_and_idle() {
        let mut app = crate::app::state::AppState::test_new();

        let mut triage = Workspace::test_new("done");
        let triage_pane = triage.tabs[0].root_pane;
        let triage_state = triage.tabs[0].panes.get_mut(&triage_pane).unwrap();
        triage_state.detected_agent = Some(Agent::Pi);
        triage_state.state = AgentState::Idle;
        triage_state.seen = false;

        let mut working = Workspace::test_new("working");
        let working_pane = working.tabs[0].root_pane;
        let working_state = working.tabs[0].panes.get_mut(&working_pane).unwrap();
        working_state.detected_agent = Some(Agent::Claude);
        working_state.state = AgentState::Working;

        let mut idle = Workspace::test_new("idle");
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

        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].label, "triage");
        assert_eq!(sections[0].entries[0].primary_label, "done");
        assert_eq!(sections[1].label, "working");
        assert_eq!(sections[1].entries[0].primary_label, "working");
        assert_eq!(sections[2].label, "idle");
        assert_eq!(sections[2].entries[0].primary_label, "idle");
    }

    #[test]
    fn agent_panel_sections_order_entries_newest_activity_first() {
        let mut app = crate::app::state::AppState::test_new();
        let old = Workspace::test_new("old");
        let old_pane = old.tabs[0].root_pane;
        let new = Workspace::test_new("new");
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
            .find(|section| section.label == "idle")
            .expect("idle section");

        assert_eq!(idle.entries[0].primary_label, "new");
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
            .find(|section| section.label == "working")
            .expect("working section");

        assert_eq!(working.entries[0].primary_label, "second");
        assert_eq!(working.entries[1].primary_label, "first");
    }

    #[test]
    fn agent_rename_does_not_count_as_activity() {
        let mut app = crate::app::state::AppState::test_new();
        let old = Workspace::test_new("old");
        let old_pane = old.tabs[0].root_pane;
        let new = Workspace::test_new("new");
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
            .find(|section| section.label == "idle")
            .expect("idle section");

        assert_eq!(idle.entries[0].primary_label, "new");
        assert_eq!(idle.entries[1].primary_label, "old");
        assert_eq!(idle.entries[1].agent_label.as_deref(), Some("renamed"));
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
        assert!(text.contains("working"));
        assert!(text.contains("codex"));
        assert!(!text.contains("working · codex"));
    }

    #[test]
    fn triage_agent_rows_keep_status_reason() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("done");
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
        assert!(text.contains("triage · all spaces"));
        assert!(text.contains("claude · done"));
        assert!(!text.contains("done · claude"));
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
            "t"
        );
        assert_eq!(buffer[(body.x + body.width - 2, body.y)].symbol(), "1");
        assert_eq!(
            buffer[(body.x + RIGHT_SUBSECTION_MARKER_COL, body.y)].symbol(),
            "▾"
        );
        assert_eq!(
            buffer[(body.x + RIGHT_ENTRY_PRIMARY_COL, body.y + 1)].symbol(),
            "d"
        );
        assert_eq!(
            buffer[(body.x + RIGHT_ENTRY_ICON_COL, body.y + 2)].symbol(),
            "│"
        );
        assert!(!buffer[(body.x + RIGHT_ENTRY_PRIMARY_COL, body.y + 1)]
            .style()
            .add_modifier
            .contains(Modifier::BOLD));
        assert_eq!(
            buffer[(body.x + RIGHT_ENTRY_PRIMARY_COL, body.y + 2)].symbol(),
            "c"
        );
    }

    #[test]
    fn agent_section_headers_use_status_colors() {
        let mut triage = AgentPanelSection {
            label: "triage",
            entries: vec![AgentPanelEntry {
                ws_idx: 0,
                tab_idx: 0,
                pane_id: crate::layout::PaneId::from_raw(1),
                group_context_idx: None,
                primary_label: "blocked".into(),
                primary_tab_label: None,
                agent_label: Some("opencode".into()),
                state: AgentState::Blocked,
                seen: false,
                custom_status: None,
                state_labels: std::collections::HashMap::new(),
                last_meaningful_agent_activity_seq: 0,
                last_meaningful_agent_activity_unix_secs: None,
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
                    label: "working",
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
                    label: "idle",
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
    fn all_workspaces_primary_label_truncates_workspace_and_tab() {
        let entry = AgentPanelEntry {
            ws_idx: 0,
            tab_idx: 0,
            pane_id: crate::layout::PaneId::from_raw(1),
            group_context_idx: None,
            primary_label: "agent-browser".into(),
            primary_tab_label: Some("test-escalation".into()),
            agent_label: Some("claude".into()),
            state: AgentState::Idle,
            seen: true,
            custom_status: None,
            state_labels: std::collections::HashMap::new(),
            last_meaningful_agent_activity_seq: 0,
            last_meaningful_agent_activity_unix_secs: None,
        };

        let label = format_agent_panel_primary_label(&entry, 18);

        assert_eq!(label, "agent-bro… · test…");
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
        assert!(header.contains("agents"));
        assert!(header.contains("all"));
        assert!(!divider.contains("agents"));
        assert!(!divider.contains("all"));
        assert!(!header.contains("▾ agents"));
        assert!(!header.contains("▸ agents"));
        assert_eq!(buffer[(agent_area.x, agent_area.y)].symbol(), "a");
        for x in agent_area.x..agent_area.x + agent_area.width {
            assert_eq!(buffer[(x, agent_area.y + 1)].symbol(), "─");
        }
    }

    #[test]
    fn agent_status_group_headers_use_real_chevrons_at_workspace_group_indent() {
        let mut app = crate::app::state::AppState::test_new();
        app.group_filter_enabled = false;

        let mut triage = Workspace::test_new("done");
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
        app.collapsed_agent_sections.push("working".to_string());

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
        let collapsed_agent_header_y = body.y + 3;

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
            "t"
        );
        assert_eq!(
            buffer[(
                body.x + RIGHT_SUBSECTION_LABEL_COL,
                collapsed_agent_header_y
            )]
                .symbol(),
            "w"
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
        assert!(text.contains("agents"));
        assert!(!text.contains("commands"));
        assert!(!text.contains("ports"));
    }

    #[test]
    fn collapsed_right_sidebar_renders_compact_agent_rows_and_expand_toggle() {
        let mut app = crate::app::state::AppState::test_new();
        let mut triage = Workspace::test_new("done");
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

        let area = Rect::new(0, 0, 4, 8);
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

        assert_eq!(rows[0], "│agt");
        assert_eq!(rows[1], "│───");
        assert_eq!(buffer[(content.x, content.y + 2)].symbol(), "1");
        assert_eq!(buffer[(content.x, content.y + 3)].symbol(), "2");
        assert_ne!(buffer[(content.x, content.y + 2)].symbol(), "▾");
        assert_ne!(buffer[(content.x, content.y + 2)].symbol(), "▸");
        assert_ne!(buffer[(content.x, content.y + 3)].symbol(), "▾");
        assert_ne!(buffer[(content.x, content.y + 3)].symbol(), "▸");
        assert_eq!(buffer[(toggle.x, toggle.y)].symbol(), "«");
        assert!(!rows.join("\n").contains("triage"));
        assert!(!rows.join("\n").contains("working"));
        assert!(!rows.join("\n").contains("idle"));
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
