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

const WORKSPACE_SECTION_HEADER_ROWS: u16 = 2;
const AGENT_PANEL_HEADER_ROWS: u16 = 2;

#[derive(Clone)]
pub(crate) struct AgentPanelEntry {
    pub ws_idx: usize,
    pub tab_idx: usize,
    pub pane_id: crate::layout::PaneId,
    pub primary_label: String,
    pub primary_tab_label: Option<String>,
    pub agent_label: Option<String>,
    pub state: AgentState,
    pub seen: bool,
}

pub(crate) struct AgentPanelSection {
    pub label: &'static str,
    pub entries: Vec<AgentPanelEntry>,
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

pub(crate) fn expanded_sidebar_sections(area: Rect, split_ratio: f32) -> (Rect, Rect) {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height == 0 {
        return (Rect::default(), Rect::default());
    }

    let (ws_h, detail_h) = sidebar_section_heights(content.height, split_ratio);
    let ws_area = Rect::new(content.x, content.y, content.width, ws_h);
    let detail_area = Rect::new(content.x, content.y + ws_h, content.width, detail_h);
    (ws_area, detail_area)
}

pub(crate) fn sidebar_section_divider_rect(area: Rect, split_ratio: f32) -> Rect {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height < 6 {
        return Rect::default();
    }

    let (ws_h, _) = sidebar_section_heights(content.height, split_ratio);
    Rect::new(content.x, content.y + ws_h, content.width, 1)
}

fn agent_panel_current_workspace_idx(app: &AppState) -> Option<usize> {
    let idx = if matches!(
        app.mode,
        Mode::Navigate
            | Mode::RenameWorkspace
            | Mode::RenameGroup
            | Mode::RenamePane
            | Mode::Resize
            | Mode::ConfirmClose
            | Mode::ConfirmDeleteGroup
            | Mode::ContextMenu
            | Mode::Settings
            | Mode::GlobalMenu
            | Mode::GroupMenu
            | Mode::AgentMenu
            | Mode::KeybindHelp
            | Mode::CommandPalette
    ) {
        Some(app.selected)
    } else {
        app.active
    }?;
    app.workspace_in_active_group(idx).then_some(idx)
}

fn agent_panel_toggle_label(scope: AgentPanelScope) -> &'static str {
    match scope {
        AgentPanelScope::CurrentWorkspace => "this space",
        AgentPanelScope::CurrentGroup => "this group",
        AgentPanelScope::AllWorkspaces => "all agents",
    }
}

fn agent_panel_group_label(app: &AppState, ws_idx: usize) -> String {
    let Some(ws) = app.workspaces.get(ws_idx) else {
        return "group 1".to_string();
    };
    app.groups
        .iter()
        .find(|group| group.id == ws.group_id)
        .map(|group| group.name.clone())
        .unwrap_or_else(|| "group 1".to_string())
}

fn agent_panel_workspace_context_label(app: &AppState, ws_idx: usize) -> String {
    let Some(ws) = app.workspaces.get(ws_idx) else {
        return String::new();
    };
    format!(
        "{} / {}",
        agent_panel_group_label(app, ws_idx),
        ws.display_name()
    )
}

pub(crate) fn agent_panel_toggle_rect(
    area: Rect,
    scope: AgentPanelScope,
    leading_separator: bool,
) -> Rect {
    if area.width == 0 || area.height < 2 {
        return Rect::default();
    }

    let label = agent_panel_toggle_label(scope);
    let width = (label.chars().count() as u16 + 2).min(area.width);
    let y_offset = u16::from(leading_separator);
    Rect::new(
        area.x + area.width.saturating_sub(width),
        area.y + y_offset,
        width,
        1,
    )
}

fn agent_panel_entries_for_scope(app: &AppState, scope: AgentPanelScope) -> Vec<AgentPanelEntry> {
    match scope {
        AgentPanelScope::CurrentWorkspace => {
            let Some(ws_idx) = agent_panel_current_workspace_idx(app) else {
                return Vec::new();
            };
            let Some(ws) = app.workspaces.get(ws_idx) else {
                return Vec::new();
            };
            ws.pane_details()
                .into_iter()
                .map(|detail| AgentPanelEntry {
                    ws_idx,
                    tab_idx: detail.tab_idx,
                    pane_id: detail.pane_id,
                    primary_label: detail.label,
                    primary_tab_label: None,
                    agent_label: None,
                    state: detail.state,
                    seen: detail.seen,
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
                    let workspace_label = ws.display_name();
                    ws.pane_details()
                        .into_iter()
                        .map(move |detail| AgentPanelEntry {
                            ws_idx,
                            tab_idx: detail.tab_idx,
                            pane_id: detail.pane_id,
                            primary_label: workspace_label.clone(),
                            primary_tab_label: multi_tab.then_some(detail.tab_label),
                            agent_label: Some(detail.agent_label),
                            state: detail.state,
                            seen: detail.seen,
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
                let workspace_label = agent_panel_workspace_context_label(app, ws_idx);
                ws.pane_details()
                    .into_iter()
                    .map(move |detail| AgentPanelEntry {
                        ws_idx,
                        tab_idx: detail.tab_idx,
                        pane_id: detail.pane_id,
                        primary_label: workspace_label.clone(),
                        primary_tab_label: multi_tab.then_some(detail.tab_label),
                        agent_label: Some(detail.agent_label),
                        state: detail.state,
                        seen: detail.seen,
                    })
            })
            .collect(),
    }
}

pub(crate) fn agent_panel_entries(app: &AppState) -> Vec<AgentPanelEntry> {
    agent_panel_entries_for_scope(app, app.agent_panel_scope)
}

fn agent_panel_entry_needs_triage(entry: &AgentPanelEntry) -> bool {
    entry.state == AgentState::Blocked || (entry.state == AgentState::Idle && !entry.seen)
}

pub(crate) fn agent_panel_triage_entries(app: &AppState) -> Vec<AgentPanelEntry> {
    app.workspaces
        .iter()
        .enumerate()
        .flat_map(|(ws_idx, ws)| {
            let multi_tab = ws.tabs.len() > 1;
            let context_label = agent_panel_workspace_context_label(app, ws_idx);
            ws.pane_details()
                .into_iter()
                .map(move |detail| AgentPanelEntry {
                    ws_idx,
                    tab_idx: detail.tab_idx,
                    pane_id: detail.pane_id,
                    primary_label: context_label.clone(),
                    primary_tab_label: multi_tab.then_some(detail.tab_label),
                    agent_label: Some(detail.agent_label),
                    state: detail.state,
                    seen: detail.seen,
                })
        })
        .filter(agent_panel_entry_needs_triage)
        .collect()
}

pub(crate) fn agent_panel_scope_count(app: &AppState, scope: AgentPanelScope) -> usize {
    agent_panel_entries_for_scope(app, scope)
        .into_iter()
        .filter(|entry| !agent_panel_entry_needs_triage(entry))
        .count()
}

pub(crate) fn agent_panel_sections(app: &AppState) -> Vec<AgentPanelSection> {
    let scoped_entries = agent_panel_entries(app);
    let mut sections = Vec::new();

    let triage = agent_panel_triage_entries(app);
    if !triage.is_empty() {
        sections.push(AgentPanelSection {
            label: "triage",
            entries: triage,
        });
    }

    let working: Vec<_> = scoped_entries
        .iter()
        .filter(|entry| entry.state == AgentState::Working)
        .cloned()
        .collect();
    if !working.is_empty() {
        sections.push(AgentPanelSection {
            label: "working",
            entries: working,
        });
    }

    let idle: Vec<_> = scoped_entries
        .into_iter()
        .filter(|entry| {
            entry.state != AgentState::Working && !agent_panel_entry_needs_triage(entry)
        })
        .collect();
    if !idle.is_empty() {
        sections.push(AgentPanelSection {
            label: "idle",
            entries: idle,
        });
    }

    sections
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

fn workspace_row_height(ws: &crate::workspace::Workspace) -> u16 {
    if ws.branch().is_some() {
        2
    } else {
        1
    }
}

pub(crate) fn workspace_list_rect(area: Rect, split_ratio: f32) -> Rect {
    let (ws_area, _) = expanded_sidebar_sections(area, split_ratio);
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

pub(crate) fn collapsed_right_sidebar_agent_rows_rect(area: Rect) -> Rect {
    let content = right_sidebar_content_rect(area);
    if content == Rect::default() || content.height <= 1 {
        return Rect::default();
    }
    Rect::new(content.x, content.y, content.width, content.height - 1)
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
    for ws_idx in app.visible_workspace_indices().into_iter().skip(scroll) {
        let Some(ws) = app.workspaces.get(ws_idx) else {
            continue;
        };
        let needed = workspace_row_height(ws).saturating_add(1);
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
    let total_rows = app.visible_workspace_indices().len();
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
    leading_separator: bool,
) -> Rect {
    let header_rows = AGENT_PANEL_HEADER_ROWS + u16::from(leading_separator);
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
            if remaining_rows > 0 {
                remaining_rows = remaining_rows.saturating_sub(1);
            }
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

pub(crate) fn compute_workspace_card_areas_in_list(
    app: &AppState,
    ws_area: Rect,
) -> Vec<crate::app::state::WorkspaceCardArea> {
    if ws_area == Rect::default() {
        return Vec::new();
    }

    let metrics = workspace_list_scroll_metrics(app, ws_area);
    let body = workspace_list_body_rect(ws_area, should_show_scrollbar(metrics));
    if body.width == 0 || body.height == 0 {
        return Vec::new();
    }

    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut cards = Vec::new();

    for ws_idx in app
        .visible_workspace_indices()
        .into_iter()
        .skip(app.workspace_scroll)
    {
        let Some(ws) = app.workspaces.get(ws_idx) else {
            continue;
        };
        let row_height = workspace_row_height(ws);
        if row_y.saturating_add(row_height).saturating_add(1) > body_bottom {
            break;
        }
        cards.push(crate::app::state::WorkspaceCardArea {
            ws_idx,
            rect: Rect::new(body.x, row_y, body.width, row_height),
        });
        row_y = row_y.saturating_add(row_height + 1);
    }

    cards
}

/// Auto-scale sidebar width based on workspace identity + agent summary.
pub(crate) fn collapsed_sidebar_sections(
    area: Rect,
    show_agent_detail: bool,
) -> (Rect, Option<u16>, Rect) {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height == 0 {
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
    let content_w = area.width.saturating_sub(1);
    if content_w == 0 || area.height == 0 {
        return Rect::default();
    }
    Rect::new(area.x, area.y, content_w, 1)
}

pub(crate) fn collapsed_workspace_rows_rect(area: Rect, show_agent_detail: bool) -> Rect {
    let (ws_area, _, _) = collapsed_sidebar_sections(area, show_agent_detail);
    if ws_area == Rect::default() || ws_area.height <= 1 {
        return Rect::default();
    }
    Rect::new(
        ws_area.x,
        ws_area.y + 1,
        ws_area.width,
        ws_area.height.saturating_sub(1),
    )
}

fn collapsed_group_label(app: &AppState) -> String {
    if app.group_filter_enabled {
        app.active_group_icon().to_string()
    } else {
        "all".to_string()
    }
}

/// Collapsed sidebar: workspace glance, plus compact agent list only when no right sidebar exists.
pub(super) fn render_sidebar_collapsed(app: &AppState, frame: &mut Frame, area: Rect) {
    let is_navigating = matches!(app.mode, Mode::Navigate);
    let show_agent_detail = app.view.right_sidebar_rect == Rect::default();

    let p = &app.palette;
    fill_rect(frame, area, Style::default().bg(p.panel_bg));
    let sep_style = if is_navigating {
        Style::default().fg(p.accent).bg(p.panel_bg)
    } else {
        Style::default().fg(p.overlay0).bg(p.panel_bg)
    };
    let sep_x = area.x + area.width.saturating_sub(1);
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(sep_x, y)].set_symbol("│");
        buf[(sep_x, y)].set_style(sep_style);
    }

    let (ws_area, divider_y, detail_area) = collapsed_sidebar_sections(area, show_agent_detail);
    let group_header = collapsed_group_header_rect(area);
    if group_header != Rect::default() {
        let label = collapsed_group_label(app);
        let style = if app.group_filter_enabled {
            Style::default().fg(p.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(label, style)).alignment(Alignment::Center),
            group_header,
        );
    }

    let workspace_rows = collapsed_workspace_rows_rect(area, show_agent_detail);
    if ws_area == Rect::default() || workspace_rows == Rect::default() {
        render_sidebar_toggle(app, frame, area, true, p);
        return;
    }

    for (visible_idx, ws_idx) in app.visible_workspace_indices().into_iter().enumerate() {
        let Some(ws) = app.workspaces.get(ws_idx) else {
            continue;
        };
        let y = workspace_rows.y + visible_idx as u16;
        if y >= workspace_rows.y + workspace_rows.height {
            break;
        }
        let (agg_state, agg_seen) = ws.aggregate_state();
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
                Span::styled(format!("{}", visible_idx + 1), num_style),
                Span::styled(" ", row_style),
                Span::styled(icon, icon_style),
            ])),
            Rect::new(workspace_rows.x, y, workspace_rows.width, 1),
        );
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
    let detail_content_area = Rect::new(
        detail_area.x,
        detail_area.y,
        detail_area.width,
        detail_area.height.saturating_sub(1),
    );
    if detail_content_area != Rect::default() {
        if let Some(ws_idx) = detail_ws_idx {
            if let Some(ws) = app.workspaces.get(ws_idx) {
                for (detail_idx, detail) in ws.pane_details().iter().enumerate() {
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
    area: Rect,
    insert_idx: usize,
) -> Option<u16> {
    if area.height == 0 {
        return None;
    }
    let list_bottom = area.y + area.height.saturating_sub(1);

    let first = cards.first()?;
    if insert_idx == first.ws_idx {
        return first.rect.y.checked_sub(1).filter(|y| *y < list_bottom);
    }

    if let Some(card) = cards.iter().find(|card| card.ws_idx == insert_idx) {
        return card.rect.y.checked_sub(1).filter(|y| *y < list_bottom);
    }

    cards
        .last()
        .filter(|card| insert_idx == card.ws_idx.saturating_add(1))
        .map(|card| card.rect.y.saturating_add(card.rect.height))
        .filter(|y| *y < list_bottom)
}

pub(super) fn render_sidebar(app: &AppState, frame: &mut Frame, area: Rect) {
    let p = &app.palette;
    fill_rect(frame, area, Style::default().bg(p.panel_bg));
    let is_navigating = matches!(app.mode, Mode::Navigate);
    let sep_style = if is_navigating {
        Style::default().fg(p.accent).bg(p.panel_bg)
    } else {
        Style::default().fg(p.overlay0).bg(p.panel_bg)
    };

    let sep_x = area.x + area.width.saturating_sub(1);
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(sep_x, y)].set_symbol("│");
        buf[(sep_x, y)].set_style(sep_style);
    }

    let ws_area = if app.view.right_sidebar_rect == Rect::default() {
        let (ws_area, detail_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        render_agent_detail(app, frame, detail_area, true);
        ws_area
    } else {
        left_sidebar_workspace_rect(area)
    };
    render_workspace_list(app, frame, ws_area, is_navigating);
    render_sidebar_toggle(app, frame, area, false, p);
}

pub(super) fn render_right_sidebar(app: &AppState, frame: &mut Frame, area: Rect) {
    if area == Rect::default() {
        return;
    }
    let p = &app.palette;
    fill_rect(frame, area, Style::default().bg(p.panel_bg));
    let has_active_workspace = app.active.and_then(|idx| app.workspaces.get(idx)).is_some();
    let sep_style = if !has_active_workspace && matches!(app.mode, Mode::Navigate) {
        Style::default().fg(p.accent).bg(p.panel_bg)
    } else {
        Style::default().fg(p.overlay0).bg(p.panel_bg)
    };
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(area.x, y)].set_symbol("│");
        buf[(area.x, y)].set_style(sep_style);
    }
    if app.right_sidebar_collapsed {
        render_right_sidebar_collapsed_agents(app, frame, area);
        render_right_sidebar_toggle(app, frame, area, true, p);
        return;
    }
    render_agent_detail(app, frame, right_sidebar_content_rect(area), false);
    render_right_sidebar_toggle(app, frame, area, false, p);
}

fn render_right_sidebar_collapsed_agents(app: &AppState, frame: &mut Frame, area: Rect) {
    let rows = collapsed_right_sidebar_agent_rows_rect(area);
    if rows == Rect::default() {
        return;
    }

    let p = &app.palette;
    for (visible_idx, detail) in agent_panel_entries(app)
        .iter()
        .skip(app.agent_panel_scroll)
        .enumerate()
    {
        let y = rows.y + visible_idx as u16;
        if y >= rows.y + rows.height {
            break;
        }

        let is_active = app.is_active_pane(detail.ws_idx, detail.tab_idx, detail.pane_id);
        let row_style = if is_active {
            Style::default().bg(p.surface_dim)
        } else {
            Style::default()
        };
        if is_active {
            let buf = frame.buffer_mut();
            for x in rows.x..rows.x + rows.width {
                buf[(x, y)].set_style(row_style);
            }
        }

        let (icon, icon_style) = agent_icon(detail.state, detail.seen, app.spinner_tick, p);
        let number_style = if is_active {
            Style::default().fg(p.text).bg(p.surface_dim)
        } else {
            Style::default().fg(p.overlay0)
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{}", visible_idx + 1), number_style),
                Span::styled(" ", row_style),
                Span::styled(icon, icon_style),
            ])),
            Rect::new(rows.x, y, rows.width, 1),
        );
    }
}

fn render_workspace_list(app: &AppState, frame: &mut Frame, area: Rect, is_navigating: bool) {
    let p = &app.palette;
    let dragged_ws_idx = match app.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::WorkspaceReorder { source_ws_idx, .. }) => {
            Some(*source_ws_idx)
        }
        _ => None,
    };
    let insertion_row = match app.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::WorkspaceReorder {
            insert_idx: Some(insert_idx),
            ..
        }) => workspace_drop_indicator_row(&app.view.workspace_card_areas, area, *insert_idx),
        _ => None,
    };

    let list_bottom = area.y + area.height.saturating_sub(1);
    if area.height > 0 {
        let selector_rect = app.group_selector_rect();
        frame.render_widget(
            Paragraph::new(Span::styled(
                " spaces",
                Style::default().fg(p.overlay1).add_modifier(Modifier::BOLD),
            )),
            Rect::new(area.x, area.y, area.width, 1),
        );

        if selector_rect != Rect::default() {
            let base = Style::default().fg(p.overlay1).bg(p.surface0);
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

    if cards.is_empty() && body.height > 0 && body.width > 10 {
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

    for card in cards {
        let i = card.ws_idx;
        let ws = &app.workspaces[i];
        let row_y = card.rect.y;
        let row_height = card.rect.height;
        let selected = i == app.selected && is_navigating;
        let is_active = Some(i) == app.active;
        let is_dragged = dragged_ws_idx == Some(i);
        let highlighted = selected || is_active || is_dragged;
        let (agg_state, agg_seen) = ws.aggregate_state();

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
                buf[(card.rect.x, y)].set_style(Style::default().fg(p.accent));
            }
        }

        let name_style = if selected || is_active || is_dragged {
            Style::default().fg(p.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.subtext0)
        };

        let (icon, icon_style) = state_dot(agg_state, agg_seen, p);
        let line1 = vec![
            Span::styled(" ", Style::default()),
            Span::styled(icon, icon_style),
            Span::styled(" ", Style::default()),
            Span::styled(ws.display_name(), name_style),
        ];

        frame.render_widget(
            Paragraph::new(Line::from(line1)),
            Rect::new(card.rect.x, row_y, card.rect.width, 1),
        );

        if row_height > 1 && row_y + 1 < list_bottom {
            if let Some(branch) = ws.branch() {
                let upstream_label = ws.git_ahead_behind().and_then(|(ahead, behind)| {
                    let mut parts = Vec::new();
                    if ahead > 0 {
                        parts.push((format!("↑{}", ahead), p.green));
                    }
                    if behind > 0 {
                        parts.push((format!("↓{}", behind), p.red));
                    }
                    (!parts.is_empty()).then_some(parts)
                });
                let reserved = upstream_label
                    .as_ref()
                    .map(|parts| {
                        parts.iter().map(|(label, _)| label.len()).sum::<usize>() + parts.len()
                    })
                    .unwrap_or(0);
                let max_branch_len = (card.rect.width as usize).saturating_sub(5 + reserved);
                let branch_display = if branch.len() > max_branch_len {
                    format!("{}…", &branch[..max_branch_len.saturating_sub(1)])
                } else {
                    branch
                };
                let branch_color = if selected || is_active {
                    p.mauve
                } else {
                    p.overlay0
                };
                let mut spans = vec![
                    Span::styled("   ", Style::default()),
                    Span::styled(branch_display, Style::default().fg(branch_color)),
                ];
                if let Some(parts) = upstream_label {
                    spans.push(Span::styled(" ", Style::default()));
                    for (idx, (label, color)) in parts.into_iter().enumerate() {
                        if idx > 0 {
                            spans.push(Span::styled(" ", Style::default()));
                        }
                        spans.push(Span::styled(label, Style::default().fg(color)));
                    }
                }
                frame.render_widget(
                    Paragraph::new(Line::from(spans)),
                    Rect::new(card.rect.x, row_y + 1, card.rect.width, 1),
                );
            }
        }
    }

    if let Some(y) = insertion_row.filter(|y| *y < list_bottom) {
        let indicator_right = scrollbar_rect
            .map(|rect| rect.x)
            .unwrap_or(area.x + area.width);
        let buf = frame.buffer_mut();
        for x in area.x..indicator_right {
            buf[(x, y)].set_symbol("─");
            buf[(x, y)].set_style(Style::default().fg(p.accent));
        }
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }
}

fn render_agent_entry(
    app: &AppState,
    frame: &mut Frame,
    detail: &AgentPanelEntry,
    area: Rect,
    row_y: u16,
) {
    let p = &app.palette;
    let is_active = app.is_active_pane(detail.ws_idx, detail.tab_idx, detail.pane_id);
    let (icon, icon_style) = agent_icon(detail.state, detail.seen, app.spinner_tick, p);
    let label_color = state_label_color(detail.state, detail.seen, p);
    let label = state_label(detail.state, detail.seen);

    let row_style = if is_active {
        Style::default().bg(p.surface_dim)
    } else {
        Style::default()
    };
    let name_style = if is_active {
        Style::default().fg(p.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.subtext0).add_modifier(Modifier::BOLD)
    };
    let status_style = if is_active {
        Style::default().fg(label_color)
    } else {
        Style::default().fg(label_color).add_modifier(Modifier::DIM)
    };
    let agent_style = Style::default().fg(p.overlay0).add_modifier(Modifier::DIM);

    let primary_label =
        format_agent_panel_primary_label(detail, area.width.saturating_sub(3) as usize);
    let name_line = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(icon, icon_style),
        Span::styled(" ", Style::default()),
        Span::styled(primary_label, name_style),
    ]);
    frame.render_widget(
        Paragraph::new(name_line).style(row_style),
        Rect::new(area.x, row_y, area.width, 1),
    );

    let mut status_spans = vec![
        Span::styled("   ", Style::default()),
        Span::styled(label, status_style),
    ];
    if let Some(agent_label) = &detail.agent_label {
        status_spans.push(Span::styled(" · ", agent_style));
        status_spans.push(Span::styled(agent_label, agent_style));
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
            if row_y < body_bottom {
                row_y = row_y.saturating_add(1);
            }
        }
        skip = 0;
    }

    None
}

fn render_agent_detail(app: &AppState, frame: &mut Frame, area: Rect, leading_separator: bool) {
    let p = &app.palette;

    if area.height < 3 {
        return;
    }

    if leading_separator {
        let sep_line = "─".repeat(area.width as usize);
        frame.render_widget(
            Paragraph::new(Span::styled(&sep_line, Style::default().fg(p.overlay0))),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }

    let header_y = area.y + u16::from(leading_separator);
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            " agents",
            Style::default().fg(p.overlay1).add_modifier(Modifier::BOLD),
        )])),
        Rect::new(area.x, header_y, area.width, 1),
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

    let sep_y = header_y + 1;
    if sep_y < area.y + area.height {
        let sep_line = "─".repeat(area.width as usize);
        frame.render_widget(
            Paragraph::new(Span::styled(&sep_line, Style::default().fg(p.overlay0))),
            Rect::new(area.x, sep_y, area.width, 1),
        );
    }

    let metrics = agent_panel_scroll_metrics(app, area, leading_separator);
    let scrollbar_rect = agent_panel_scrollbar_rect(app, area, leading_separator);
    let body = agent_panel_body_rect(area, should_show_scrollbar(metrics), leading_separator);
    if body == Rect::default() {
        return;
    }

    let sections = agent_panel_sections(app);
    if sections.is_empty() && body.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " no agents",
                Style::default().fg(p.overlay0).add_modifier(Modifier::BOLD),
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
        if skip >= section.entries.len() {
            skip -= section.entries.len();
            continue;
        }
        if row_y >= body_bottom {
            break;
        }

        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(" {}", section.label),
                Style::default().fg(p.overlay0).add_modifier(Modifier::BOLD),
            )),
            Rect::new(body.x, row_y, body.width, 1),
        );
        row_y = row_y.saturating_add(1);

        for detail in section.entries.iter().skip(skip) {
            if row_y.saturating_add(1) >= body_bottom {
                break;
            }
            render_agent_entry(app, frame, detail, body, row_y);
            row_y = row_y.saturating_add(2);
            if row_y < body_bottom {
                row_y = row_y.saturating_add(1);
            }
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
    let icon_style = if app.update_available.is_some() {
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
    use crate::{detect::Agent, workspace::Workspace};
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    #[test]
    fn agent_panel_toggle_labels_match_control_center_scope() {
        assert_eq!(
            agent_panel_toggle_label(AgentPanelScope::CurrentWorkspace),
            "this space"
        );
        assert_eq!(
            agent_panel_toggle_label(AgentPanelScope::CurrentGroup),
            "this group"
        );
        assert_eq!(
            agent_panel_toggle_label(AgentPanelScope::AllWorkspaces),
            "all agents"
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

        let backend = TestBackend::new(28, 12);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_workspace_list(&app, frame, Rect::new(0, 0, 28, 12), false))
            .expect("render workspace list");

        let text = buffer_text(terminal.backend().buffer(), 28, 12);
        assert!(!text.contains("new space"));
        assert!(!text.contains("menu"));
    }

    #[test]
    fn agent_panel_empty_state_mentions_current_scope() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("test")];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::CurrentWorkspace;

        let backend = TestBackend::new(30, 12);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_agent_detail(&app, frame, Rect::new(0, 0, 30, 12), false))
            .expect("render agent panel");

        let text = buffer_text(terminal.backend().buffer(), 30, 12);
        let rows = text.lines().collect::<Vec<_>>();
        assert!(text.contains("no agents"));
        assert!(rows[2].contains("no agents"));
        assert!(!text.contains("this space has none"));
    }

    #[test]
    fn all_workspaces_agent_panel_entries_use_workspace_and_optional_tab_labels() {
        let mut app = crate::app::state::AppState::test_new();
        let mut first = Workspace::test_new("one");
        let first_pane = first.tabs[0].root_pane;
        first.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);

        let mut second = Workspace::test_new("two");
        let second_tab = second.test_add_tab(Some("logs"));
        let second_pane = second.tabs[second_tab].root_pane;
        second.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);

        app.workspaces = vec![first, second];
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let entries = agent_panel_entries(&app);
        assert_eq!(entries[0].primary_label, "group 1 / one");
        assert!(entries[0].primary_tab_label.is_none());
        assert_eq!(entries[0].agent_label.as_deref(), Some("pi"));
        assert_eq!(entries[1].primary_label, "group 1 / two");
        assert_eq!(entries[1].primary_tab_label.as_deref(), Some("logs"));
        assert_eq!(entries[1].agent_label.as_deref(), Some("claude"));
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
        assert_eq!(entries[0].primary_label, "group 1 / one");
        assert_eq!(entries[1].primary_label, "Work / two");
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
        assert_eq!(entries[0].primary_label, "group 1 / done");
        assert_eq!(entries[1].primary_label, "Work / blocked");
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
        assert_eq!(sections[0].entries[0].primary_label, "group 1 / done");
        assert_eq!(sections[1].label, "working");
        assert_eq!(sections[1].entries[0].primary_label, "group 1 / working");
        assert_eq!(sections[2].label, "idle");
        assert_eq!(sections[2].entries[0].primary_label, "group 1 / idle");
    }

    #[test]
    fn all_workspaces_primary_label_truncates_workspace_and_tab() {
        let entry = AgentPanelEntry {
            ws_idx: 0,
            tab_idx: 0,
            pane_id: crate::layout::PaneId::from_raw(1),
            primary_label: "agent-browser".into(),
            primary_tab_label: Some("test-escalation".into()),
            agent_label: Some("claude".into()),
            state: AgentState::Idle,
            seen: true,
        };

        let label = format_agent_panel_primary_label(&entry, 18);

        assert_eq!(label, "agent-bro… · test…");
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
