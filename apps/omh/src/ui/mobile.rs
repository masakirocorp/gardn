use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use super::status::{agent_icon, state_dot, state_label, toast_kind_color};
use super::text::truncate_end;
use super::widgets::fill_rect;
use crate::app::state::{
    MobileSwitcherLevel, NavigatorRow, NavigatorTarget, Palette, ToastKind, ToastNotification,
};
use crate::app::{AppState, ClientViewState};
use crate::detect::AgentState;
use crate::layout::PaneId;
use crate::terminal::TerminalRuntimeRegistry;

const SWITCH_BUTTON_WIDTH: u16 = 10;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MobileHeaderHitAreas {
    pub menu: Rect,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MobileSwitcherAreas {
    pub panel: Rect,
    pub close: Rect,
    pub breadcrumb: Rect,
    pub viewport: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileSwitcherTarget {
    Back,
    ToggleAgents,
    Agent {
        ws_idx: usize,
        tab_idx: usize,
        pane_id: PaneId,
    },
    Group(usize),
    NewSpace,
    Workspace(usize),
    NewTab,
    Tab {
        ws_idx: usize,
        tab_idx: usize,
    },
    Pane {
        ws_idx: usize,
        tab_idx: usize,
        pane_id: PaneId,
    },
    OpenActions,
    Menu(usize),
}

#[derive(Debug, PartialEq, Eq)]
enum MobileNavigationRow {
    Section(&'static str),
    AgentSummary {
        triage: usize,
        working: usize,
        idle: usize,
        expanded: bool,
    },
    Agent {
        label: String,
        meta: String,
        state: AgentState,
        seen: bool,
        target: MobileSwitcherTarget,
    },
    Divider,
    Action {
        label: &'static str,
        target: MobileSwitcherTarget,
    },
    Hierarchy {
        row: NavigatorRow,
        target: MobileSwitcherTarget,
    },
    Menu {
        label: String,
        target: MobileSwitcherTarget,
    },
}

impl MobileNavigationRow {
    fn target(&self) -> Option<MobileSwitcherTarget> {
        match self {
            Self::Section(_) | Self::Divider => None,
            Self::AgentSummary { .. } => Some(MobileSwitcherTarget::ToggleAgents),
            Self::Action { target, .. }
            | Self::Agent { target, .. }
            | Self::Hierarchy { target, .. }
            | Self::Menu { target, .. } => Some(*target),
        }
    }
}

fn mobile_navigation_rows(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: Option<&ClientViewState>,
) -> Vec<MobileNavigationRow> {
    let hierarchy = view.map_or_else(
        || app.mobile_navigation_rows(terminal_runtimes),
        |view| app.mobile_navigation_rows_for_view(view, terminal_runtimes),
    );
    let level = view.map_or(app.mobile_switcher_level, |view| view.mobile_switcher_level);
    let active_group = view.map_or(app.active_group, |view| view.active_group);
    let mut rows = Vec::new();
    let agents_expanded = view.map_or(app.mobile_agents_expanded, |view| {
        view.mobile_agents_expanded
    });
    let agent_sections = view.map_or_else(
        || super::sidebar::agent_panel_sections_all_workspaces(app, terminal_runtimes),
        |view| {
            super::sidebar::agent_panel_sections_all_workspaces_for_view(
                app,
                terminal_runtimes,
                view,
            )
        },
    );
    let section_count = |label: &str| {
        agent_sections
            .iter()
            .find(|section| section.label == label)
            .map_or(0, |section| section.entries.len())
    };
    rows.push(MobileNavigationRow::AgentSummary {
        triage: section_count("triage"),
        working: section_count("working"),
        idle: section_count("idle"),
        expanded: agents_expanded,
    });
    if agents_expanded {
        for section in agent_sections {
            for entry in section.entries {
                let label = entry
                    .agent_label
                    .clone()
                    .unwrap_or_else(|| entry.primary_label.clone());
                let location = if entry.primary_label == label {
                    state_label(entry.state, entry.seen).to_string()
                } else {
                    format!(
                        "{} · {}",
                        entry.primary_label,
                        state_label(entry.state, entry.seen)
                    )
                };
                rows.push(MobileNavigationRow::Agent {
                    label,
                    meta: location,
                    state: entry.state,
                    seen: entry.seen,
                    target: MobileSwitcherTarget::Agent {
                        ws_idx: entry.ws_idx,
                        tab_idx: entry.tab_idx,
                        pane_id: entry.pane_id,
                    },
                });
            }
        }
    }
    rows.push(MobileNavigationRow::Divider);

    match level {
        MobileSwitcherLevel::Groups => {
            rows.push(MobileNavigationRow::Section("GROUPS"));
            rows.extend(hierarchy.into_iter().filter_map(|row| match row.target {
                NavigatorTarget::Group { group_idx } => Some(MobileNavigationRow::Hierarchy {
                    row,
                    target: MobileSwitcherTarget::Group(group_idx),
                }),
                _ => None,
            }));
            append_mobile_footer(&mut rows, None);
        }
        MobileSwitcherLevel::Workspaces => {
            rows.push(MobileNavigationRow::Section("SPACES"));
            rows.extend(hierarchy.into_iter().filter_map(|row| {
                match row.target {
                    NavigatorTarget::Workspace { ws_idx }
                        if app
                            .workspaces
                            .get(ws_idx)
                            .and_then(|workspace| app.group_index_by_id(&workspace.group_id))
                            == Some(active_group) =>
                    {
                        Some(MobileNavigationRow::Hierarchy {
                            row,
                            target: MobileSwitcherTarget::Workspace(ws_idx),
                        })
                    }
                    _ => None,
                }
            }));
            append_mobile_footer(
                &mut rows,
                Some(("+ New space", MobileSwitcherTarget::NewSpace)),
            );
        }
        MobileSwitcherLevel::Tabs { ws_idx } => {
            rows.push(MobileNavigationRow::Section("TABS"));
            rows.extend(hierarchy.into_iter().filter_map(|row| match row.target {
                NavigatorTarget::Tab {
                    ws_idx: row_ws_idx,
                    tab_idx,
                } if row_ws_idx == ws_idx => Some(MobileNavigationRow::Hierarchy {
                    row,
                    target: MobileSwitcherTarget::Tab { ws_idx, tab_idx },
                }),
                _ => None,
            }));
            append_mobile_footer(&mut rows, Some(("+ New tab", MobileSwitcherTarget::NewTab)));
        }
        MobileSwitcherLevel::Panes { ws_idx, tab_idx } => {
            rows.push(MobileNavigationRow::Section("PANES"));
            rows.extend(hierarchy.into_iter().filter_map(|row| match row.target {
                NavigatorTarget::Pane {
                    ws_idx: row_ws_idx,
                    tab_idx: row_tab_idx,
                    pane_id,
                } if row_ws_idx == ws_idx && row_tab_idx == tab_idx => {
                    Some(MobileNavigationRow::Hierarchy {
                        row,
                        target: MobileSwitcherTarget::Pane {
                            ws_idx,
                            tab_idx,
                            pane_id,
                        },
                    })
                }
                _ => None,
            }));
            append_mobile_footer(&mut rows, None);
        }
        MobileSwitcherLevel::Actions => {
            rows.push(MobileNavigationRow::Section("ACTIONS"));
            rows.extend(
                app.global_menu_labels()
                    .into_iter()
                    .enumerate()
                    .map(|(idx, label)| MobileNavigationRow::Menu {
                        label: label.to_string(),
                        target: MobileSwitcherTarget::Menu(idx),
                    }),
            );
        }
    }
    rows
}

fn append_mobile_footer(
    rows: &mut Vec<MobileNavigationRow>,
    contextual_action: Option<(&'static str, MobileSwitcherTarget)>,
) {
    rows.push(MobileNavigationRow::Divider);
    if let Some((label, target)) = contextual_action {
        rows.push(MobileNavigationRow::Action { label, target });
    }
    rows.push(MobileNavigationRow::Action {
        label: "More actions",
        target: MobileSwitcherTarget::OpenActions,
    });
}

pub(crate) fn initial_mobile_switcher_level(_app: &AppState) -> MobileSwitcherLevel {
    MobileSwitcherLevel::Groups
}

pub(crate) fn parent_mobile_switcher_level(
    app: &AppState,
    level: MobileSwitcherLevel,
) -> MobileSwitcherLevel {
    match level {
        MobileSwitcherLevel::Groups => MobileSwitcherLevel::Groups,
        MobileSwitcherLevel::Workspaces => MobileSwitcherLevel::Groups,
        MobileSwitcherLevel::Tabs { .. } => MobileSwitcherLevel::Workspaces,
        MobileSwitcherLevel::Panes { ws_idx, .. } => MobileSwitcherLevel::Tabs { ws_idx },
        MobileSwitcherLevel::Actions => initial_mobile_switcher_level(app),
    }
}

pub(crate) fn is_mobile_width(area: Rect, threshold: u16) -> bool {
    area.width > 0 && area.width <= threshold
}

pub(crate) fn compute_mobile_header_hit_areas(_app: &AppState, area: Rect) -> MobileHeaderHitAreas {
    if area.width == 0 || area.height == 0 {
        return MobileHeaderHitAreas::default();
    }

    let width = SWITCH_BUTTON_WIDTH.min(area.width);
    let switch = Rect::new(
        area.x + area.width.saturating_sub(width),
        area.y,
        width,
        area.height,
    );

    MobileHeaderHitAreas { menu: switch }
}

pub(crate) fn mobile_switcher_areas(app: &AppState) -> MobileSwitcherAreas {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    let row_count = mobile_navigation_rows(app, &terminal_runtimes, None).len();
    mobile_switcher_areas_for_rows(mobile_screen_rect(app), row_count)
}

pub(crate) fn mobile_switcher_areas_for_view(
    app: &AppState,
    view: &ClientViewState,
) -> MobileSwitcherAreas {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    let row_count = mobile_navigation_rows(app, &terminal_runtimes, Some(view)).len();
    mobile_switcher_areas_for_rows(view.screen_rect(), row_count)
}

fn mobile_switcher_areas_for_rows(screen: Rect, row_count: usize) -> MobileSwitcherAreas {
    const CHROME_HEIGHT: u16 = 3;
    if screen.width == 0 || screen.height <= CHROME_HEIGHT {
        return MobileSwitcherAreas::default();
    }

    let viewport_height = (row_count.max(1) as u16).min(screen.height - CHROME_HEIGHT);
    let panel = Rect::new(
        screen.x,
        screen.y,
        screen.width,
        CHROME_HEIGHT + viewport_height,
    );
    let close_width = 3.min(screen.width);
    let close = Rect::new(
        panel.x + panel.width.saturating_sub(close_width),
        panel.y,
        close_width,
        1,
    );
    let breadcrumb = Rect::new(panel.x, panel.y + 1, panel.width, 1);
    let viewport = Rect::new(
        panel.x,
        panel.y + CHROME_HEIGHT,
        panel.width,
        viewport_height,
    );

    MobileSwitcherAreas {
        panel,
        close,
        breadcrumb,
        viewport,
    }
}

pub(crate) fn mobile_switcher_max_scroll_for_height(app: &AppState, viewport_height: u16) -> usize {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    mobile_navigation_rows(app, &terminal_runtimes, None)
        .len()
        .saturating_sub(viewport_height as usize)
}

pub(crate) fn mobile_switcher_workspace_doc_row(app: &AppState, idx: usize) -> Option<usize> {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    mobile_navigation_rows(app, &terminal_runtimes, None)
        .iter()
        .position(|row| row.target() == Some(MobileSwitcherTarget::Workspace(idx)))
}

pub(crate) fn mobile_switcher_max_scroll(app: &AppState) -> usize {
    mobile_switcher_max_scroll_for_height(app, mobile_switcher_areas(app).viewport.height)
}

pub(crate) fn mobile_switcher_max_scroll_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
) -> usize {
    mobile_navigation_rows(app, terminal_runtimes, Some(view))
        .len()
        .saturating_sub(mobile_switcher_areas_for_view(app, view).viewport.height as usize)
}

pub(crate) fn mobile_switcher_max_scroll_for_view_height(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    viewport_height: u16,
) -> usize {
    mobile_navigation_rows(app, terminal_runtimes, Some(view))
        .len()
        .saturating_sub(viewport_height as usize)
}

fn mobile_switcher_target_from_rows(
    rows: &[MobileNavigationRow],
    scroll: usize,
    viewport: Rect,
    col: u16,
    row: u16,
) -> Option<MobileSwitcherTarget> {
    let content = inset_for_left_scrollbar(viewport);
    if !rect_contains(content, col, row) {
        return None;
    }
    let doc_row = scroll.saturating_add(row.saturating_sub(viewport.y) as usize);
    rows.get(doc_row)?.target()
}

fn mobile_switcher_content_target_index_from_rows(rows: &[MobileNavigationRow]) -> usize {
    rows.iter()
        .filter_map(MobileNavigationRow::target)
        .position(|target| {
            !matches!(
                target,
                MobileSwitcherTarget::ToggleAgents | MobileSwitcherTarget::Agent { .. }
            )
        })
        .unwrap_or(0)
}

pub(crate) fn mobile_switcher_content_target_index(app: &AppState) -> usize {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    mobile_switcher_content_target_index_from_rows(&mobile_navigation_rows(
        app,
        &terminal_runtimes,
        None,
    ))
}

pub(crate) fn mobile_switcher_content_target_index_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
) -> usize {
    mobile_switcher_content_target_index_from_rows(&mobile_navigation_rows(
        app,
        terminal_runtimes,
        Some(view),
    ))
}

pub(crate) fn mobile_switcher_target_count(app: &AppState) -> usize {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    mobile_navigation_rows(app, &terminal_runtimes, None)
        .iter()
        .filter(|row| row.target().is_some())
        .count()
}

pub(crate) fn mobile_switcher_target_count_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
) -> usize {
    mobile_navigation_rows(app, terminal_runtimes, Some(view))
        .iter()
        .filter(|row| row.target().is_some())
        .count()
}

pub(crate) fn mobile_switcher_selected_target(app: &AppState) -> Option<MobileSwitcherTarget> {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    mobile_navigation_rows(app, &terminal_runtimes, None)
        .iter()
        .filter_map(MobileNavigationRow::target)
        .nth(app.mobile_switcher_selected)
}

pub(crate) fn mobile_switcher_selected_target_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
) -> Option<MobileSwitcherTarget> {
    mobile_navigation_rows(app, terminal_runtimes, Some(view))
        .iter()
        .filter_map(MobileNavigationRow::target)
        .nth(view.mobile_switcher_selected)
}

fn mobile_switcher_selected_doc_row(rows: &[MobileNavigationRow], selected: usize) -> usize {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.target().is_some())
        .nth(selected)
        .map_or(0, |(doc_row, _)| doc_row)
}

pub(crate) fn keep_mobile_switcher_selection_visible(app: &mut AppState) {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    let rows = mobile_navigation_rows(app, &terminal_runtimes, None);
    let viewport_height = mobile_switcher_areas(app).viewport.height as usize;
    let selected_row = mobile_switcher_selected_doc_row(&rows, app.mobile_switcher_selected);
    if selected_row < app.mobile_switcher_scroll {
        app.mobile_switcher_scroll = selected_row;
    } else if selected_row >= app.mobile_switcher_scroll.saturating_add(viewport_height) {
        app.mobile_switcher_scroll = selected_row.saturating_sub(viewport_height.saturating_sub(1));
    }
}

pub(crate) fn keep_mobile_switcher_selection_visible_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &mut ClientViewState,
) {
    let rows = mobile_navigation_rows(app, terminal_runtimes, Some(view));
    let viewport_height = mobile_switcher_areas_for_view(app, view).viewport.height as usize;
    let selected_row = mobile_switcher_selected_doc_row(&rows, view.mobile_switcher_selected);
    if selected_row < view.mobile_switcher_scroll {
        view.mobile_switcher_scroll = selected_row;
    } else if selected_row >= view.mobile_switcher_scroll.saturating_add(viewport_height) {
        view.mobile_switcher_scroll =
            selected_row.saturating_sub(viewport_height.saturating_sub(1));
    }
}

pub(crate) fn mobile_switcher_target_at(
    app: &AppState,
    col: u16,
    row: u16,
) -> Option<MobileSwitcherTarget> {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    let areas = mobile_switcher_areas(app);
    if app.mobile_switcher_level != MobileSwitcherLevel::Groups
        && rect_contains(areas.breadcrumb, col, row)
    {
        return Some(MobileSwitcherTarget::Back);
    }
    let rows = mobile_navigation_rows(app, &terminal_runtimes, None);
    mobile_switcher_target_from_rows(&rows, app.mobile_switcher_scroll, areas.viewport, col, row)
}

pub(crate) fn mobile_switcher_target_at_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    col: u16,
    row: u16,
) -> Option<MobileSwitcherTarget> {
    let areas = mobile_switcher_areas_for_view(app, view);
    if view.mobile_switcher_level != MobileSwitcherLevel::Groups
        && rect_contains(areas.breadcrumb, col, row)
    {
        return Some(MobileSwitcherTarget::Back);
    }
    let rows = mobile_navigation_rows(app, terminal_runtimes, Some(view));
    mobile_switcher_target_from_rows(&rows, view.mobile_switcher_scroll, areas.viewport, col, row)
}

pub(crate) fn render_mobile_header(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let p = &app.palette;
    fill_rect(frame, area, Style::default().bg(p.panel_bg));

    let switch = app.view.mobile_menu_hit_area;
    let status_w = switch.x.saturating_sub(area.x).saturating_sub(1);
    let status = Rect::new(area.x, area.y, status_w, area.height);

    render_header_status(app, terminal_runtimes, frame, status);
    render_switch_button(app, frame, switch);
}

pub(crate) fn render_mobile_header_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let p = &app.palette;
    fill_rect(frame, area, Style::default().bg(p.panel_bg));

    let switch = view.computed.mobile_menu_hit_area;
    let status_w = switch.x.saturating_sub(area.x).saturating_sub(1);
    let status = Rect::new(area.x, area.y, status_w, area.height);

    render_header_status_for_view(app, terminal_runtimes, view, frame, status);
    render_switch_button(app, frame, switch);
}

pub(crate) fn mobile_toast_banner_rect(area: Rect) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect::default();
    }

    let y = area.y + area.height.saturating_sub(1);
    Rect::new(area.x, y, area.width, 1)
}

pub(crate) fn render_mobile_toast_banner(
    frame: &mut Frame,
    area: Rect,
    toast: &ToastNotification,
    p: &Palette,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let dot_color = toast_kind_color(toast.kind, p);
    let banner = mobile_toast_banner_rect(area);
    let bg = p.surface0;

    frame.render_widget(Clear, banner);
    fill_rect(frame, banner, Style::default().bg(bg));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default().bg(bg)),
            Span::styled("●", Style::default().fg(dot_color).bg(bg)),
            Span::styled(" ", Style::default().bg(bg)),
            Span::styled(
                mobile_toast_title(toast),
                Style::default()
                    .fg(p.text)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · ", Style::default().fg(p.overlay0).bg(bg)),
            Span::styled(&toast.context, Style::default().fg(p.overlay0).bg(bg)),
        ])),
        banner,
    );
}

pub(crate) fn render_mobile_panel(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    _area: Rect,
) {
    let areas = mobile_switcher_areas(app);
    render_mobile_panel_shell(
        app,
        terminal_runtimes,
        app.mobile_switcher_level,
        app.active,
        app.active_group,
        frame,
        areas,
    );
    render_mobile_switcher_content(app, terminal_runtimes, frame, areas.viewport);
}

pub(crate) fn render_mobile_panel_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    frame: &mut Frame,
    _area: Rect,
) {
    let areas = mobile_switcher_areas_for_view(app, view);
    render_mobile_panel_shell(
        app,
        terminal_runtimes,
        view.mobile_switcher_level,
        view.active_workspace,
        view.active_group,
        frame,
        areas,
    );
    render_mobile_switcher_content_for_view(app, terminal_runtimes, view, frame, areas.viewport);
}

fn render_mobile_panel_shell(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    level: MobileSwitcherLevel,
    active_workspace: Option<usize>,
    active_group: usize,
    frame: &mut Frame,
    areas: MobileSwitcherAreas,
) {
    if areas.panel == Rect::default() {
        return;
    }
    let p = &app.palette;
    frame.render_widget(Clear, areas.panel);
    fill_rect(frame, areas.panel, Style::default().bg(p.panel_bg));
    frame.render_widget(
        Paragraph::new(" switch").style(
            Style::default()
                .fg(p.text)
                .bg(p.panel_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(
            areas.panel.x,
            areas.panel.y,
            areas.close.x.saturating_sub(areas.panel.x),
            1,
        ),
    );
    render_close_button(app, frame, areas.close);
    render_mobile_breadcrumb(
        app,
        terminal_runtimes,
        level,
        active_workspace,
        active_group,
        frame,
        areas.breadcrumb,
    );
    draw_horizontal_rule(
        frame,
        Rect::new(areas.panel.x, areas.panel.y + 2, areas.panel.width, 1),
        p,
    );
}

fn render_mobile_breadcrumb(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    level: MobileSwitcherLevel,
    active_workspace: Option<usize>,
    active_group: usize,
    frame: &mut Frame,
    area: Rect,
) {
    if area.width == 0 {
        return;
    }
    let p = &app.palette;
    let workspace_for_level = match level {
        MobileSwitcherLevel::Tabs { ws_idx } | MobileSwitcherLevel::Panes { ws_idx, .. } => {
            app.workspaces.get(ws_idx)
        }
        _ => active_workspace.and_then(|ws_idx| app.workspaces.get(ws_idx)),
    };
    let group_idx = workspace_for_level
        .and_then(|workspace| app.group_index_by_id(&workspace.group_id))
        .unwrap_or(active_group);
    let group = app.groups.get(group_idx);
    let group_name = group.map(|group| group.name.as_str()).unwrap_or("groups");
    let group_icon = group.map(|group| group.icon.as_str()).unwrap_or("●");
    let accent = app.group_accent_color(group_idx);
    let label = match level {
        MobileSwitcherLevel::Groups => "All groups".to_string(),
        MobileSwitcherLevel::Workspaces => group_name.to_string(),
        MobileSwitcherLevel::Tabs { ws_idx } => app
            .workspaces
            .get(ws_idx)
            .map(|workspace| {
                format!(
                    "{group_name} / {}",
                    workspace.display_name_from(&app.terminals, terminal_runtimes)
                )
            })
            .unwrap_or_else(|| group_name.to_string()),
        MobileSwitcherLevel::Panes { ws_idx, tab_idx } => app
            .workspaces
            .get(ws_idx)
            .map(|workspace| {
                let workspace_name = workspace.display_name_from(&app.terminals, terminal_runtimes);
                let tab_name = workspace
                    .tab_display_name(tab_idx)
                    .unwrap_or_else(|| (tab_idx + 1).to_string());
                format!("{group_name} / {workspace_name} / {tab_name}")
            })
            .unwrap_or_else(|| group_name.to_string()),
        MobileSwitcherLevel::Actions => "Actions".to_string(),
    };
    let can_go_back = level != MobileSwitcherLevel::Groups;
    let prefix = if can_go_back { " ‹ " } else { "   " };
    let mut spans = vec![Span::styled(
        prefix,
        Style::default().fg(p.overlay1).bg(p.panel_bg),
    )];
    if !matches!(
        level,
        MobileSwitcherLevel::Groups | MobileSwitcherLevel::Actions
    ) {
        spans.push(Span::styled(
            group_icon.to_string(),
            Style::default()
                .fg(accent)
                .bg(p.panel_bg)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        truncate_end(&label, area.width.saturating_sub(6) as usize),
        Style::default().fg(p.text).bg(p.panel_bg),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_header_status(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    let active_tab = app
        .active
        .and_then(|ws_idx| app.workspaces.get(ws_idx))
        .map(|workspace| workspace.active_tab);
    render_header_status_for_selection(app, terminal_runtimes, app.active, active_tab, frame, area);
}

fn render_header_status_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    let active_tab = view
        .active_workspace
        .and_then(|ws_idx| view.active_tab_index_for_workspace(app, ws_idx));
    render_header_status_for_selection(
        app,
        terminal_runtimes,
        view.active_workspace,
        active_tab,
        frame,
        area,
    );
}

fn render_header_status_for_selection(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    active_workspace: Option<usize>,
    active_tab: Option<usize>,
    frame: &mut Frame,
    area: Rect,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let p = &app.palette;
    let Some(ws) = active_workspace.and_then(|idx| app.workspaces.get(idx)) else {
        frame.render_widget(Paragraph::new(" no space"), area);
        return;
    };
    let group_idx = app.group_index_by_id(&ws.group_id);
    let group_icon = group_idx
        .and_then(|idx| app.groups.get(idx))
        .map(|group| group.icon.as_str())
        .unwrap_or("●");
    let accent = group_idx
        .map(|idx| app.group_accent_color(idx))
        .unwrap_or(p.accent);
    let tab_idx = active_tab
        .unwrap_or(ws.active_tab)
        .min(ws.tabs.len().saturating_sub(1));
    let space_name = ws.display_name_from(&app.terminals, terminal_runtimes);
    let tab_name = ws
        .tab_display_name(tab_idx)
        .unwrap_or_else(|| (tab_idx + 1).to_string());
    let path = format!("{space_name} / {tab_name}");
    let indicator = if ws.tabs.is_empty() {
        String::new()
    } else {
        format!("{}/{}", tab_idx + 1, ws.tabs.len())
    };
    let indicator_width = super::text::display_width_u16(&indicator)
        .saturating_add(u16::from(!indicator.is_empty()))
        .min(area.width);
    let path_width = area.width.saturating_sub(indicator_width);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                group_icon.to_string(),
                Style::default()
                    .fg(accent)
                    .bg(p.panel_bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                truncate_end(&path, path_width.saturating_sub(3) as usize),
                Style::default()
                    .fg(p.text)
                    .bg(p.panel_bg)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        Rect::new(area.x, area.y, path_width, 1),
    );
    if indicator_width > 0 {
        frame.render_widget(
            Paragraph::new(indicator)
                .style(Style::default().fg(p.overlay1).bg(p.panel_bg))
                .alignment(Alignment::Right),
            Rect::new(area.x + path_width, area.y, indicator_width, 1),
        );
    }
}

fn render_switch_button(app: &AppState, frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let p = &app.palette;
    fill_rect(frame, area, Style::default().bg(p.surface0));
    for y in area.y..area.y + area.height {
        frame.buffer_mut()[(area.x, y)]
            .set_symbol("│")
            .set_style(Style::default().fg(p.surface_dim).bg(p.surface0));
    }
    let label_y = if area.height > 1 { area.y + 1 } else { area.y };
    frame.render_widget(
        Paragraph::new("switch")
            .style(
                Style::default()
                    .fg(p.text)
                    .bg(p.surface0)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center),
        Rect::new(area.x + 1, label_y, area.width.saturating_sub(1), 1),
    );

    // A blocked pane anywhere remains visible as a compact attention badge.
    if app.workspaces.iter().any(|workspace| {
        matches!(
            workspace.aggregate_state(&app.terminals).0,
            AgentState::Blocked
        )
    }) {
        let bx = area.x + area.width.saturating_sub(1);
        frame.buffer_mut()[(bx, area.y)]
            .set_symbol("●")
            .set_style(Style::default().fg(p.red).bg(p.surface0));
    }
}

fn render_close_button(app: &AppState, frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let p = &app.palette;
    frame.render_widget(
        Paragraph::new("×")
            .style(
                Style::default()
                    .fg(p.overlay1)
                    .bg(p.panel_bg)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center),
        area,
    );
}

fn render_mobile_switcher_content(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    viewport: Rect,
) {
    let rows = mobile_navigation_rows(app, terminal_runtimes, None);
    render_mobile_navigation_rows(
        app,
        frame,
        viewport,
        &rows,
        app.mobile_switcher_scroll,
        app.mobile_switcher_selected,
    );
}

fn render_mobile_switcher_content_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    frame: &mut Frame,
    viewport: Rect,
) {
    let rows = mobile_navigation_rows(app, terminal_runtimes, Some(view));
    render_mobile_navigation_rows(
        app,
        frame,
        viewport,
        &rows,
        view.mobile_switcher_scroll,
        view.mobile_switcher_selected,
    );
}

fn render_mobile_navigation_rows(
    app: &AppState,
    frame: &mut Frame,
    viewport: Rect,
    rows: &[MobileNavigationRow],
    scroll: usize,
    selected: usize,
) {
    if viewport.width == 0 || viewport.height == 0 {
        return;
    }
    let p = &app.palette;
    render_left_scrollbar(
        frame,
        viewport,
        rows.len(),
        viewport.height as usize,
        scroll,
        p,
    );
    let content = inset_for_left_scrollbar(viewport);
    if content == Rect::default() {
        return;
    }
    let mut target_idx = 0;
    for (doc_y, row) in rows.iter().enumerate() {
        let is_selected = if row.target().is_some() {
            let is_selected = target_idx == selected;
            target_idx += 1;
            is_selected
        } else {
            false
        };
        match row {
            MobileNavigationRow::Section(title) => {
                render_section_title_at(frame, viewport, content, doc_y, scroll, title, p);
            }
            MobileNavigationRow::AgentSummary {
                triage,
                working,
                idle,
                expanded,
            } => {
                render_mobile_agent_summary(
                    app,
                    frame,
                    viewport,
                    content,
                    doc_y,
                    scroll,
                    (*triage, *working, *idle),
                    *expanded,
                    is_selected,
                );
            }
            MobileNavigationRow::Agent {
                label,
                meta,
                state,
                seen,
                ..
            } => {
                render_mobile_agent_row(
                    app,
                    frame,
                    viewport,
                    content,
                    doc_y,
                    scroll,
                    label,
                    meta,
                    (*state, *seen),
                    is_selected,
                );
            }
            MobileNavigationRow::Divider => {
                let Some(y) = visible_y(viewport, scroll, doc_y) else {
                    continue;
                };
                draw_horizontal_rule(
                    frame,
                    Rect::new(content.x + 1, y, content.width.saturating_sub(2), 1),
                    p,
                );
            }
            MobileNavigationRow::Action { label, target } => {
                let Some(y) = visible_y(viewport, scroll, doc_y) else {
                    continue;
                };
                let bg = mobile_item_bg(is_selected, false, p);
                fill_rect(
                    frame,
                    Rect::new(content.x, y, content.width, 1),
                    Style::default().bg(bg),
                );
                let label = if *target == MobileSwitcherTarget::OpenActions {
                    format!("  {label}  ›")
                } else {
                    format!("  {label}")
                };
                frame.render_widget(
                    Paragraph::new(label).style(
                        Style::default()
                            .fg(if *target == MobileSwitcherTarget::OpenActions {
                                p.overlay1
                            } else {
                                p.text
                            })
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Rect::new(content.x, y, content.width, 1),
                );
            }
            MobileNavigationRow::Hierarchy { row, .. } => {
                render_mobile_hierarchy_row(
                    app,
                    frame,
                    viewport,
                    content,
                    doc_y,
                    scroll,
                    row,
                    is_selected,
                );
            }
            MobileNavigationRow::Menu { label, .. } => {
                let Some(y) = visible_y(viewport, scroll, doc_y) else {
                    continue;
                };
                let bg = mobile_item_bg(is_selected, false, p);
                fill_rect(
                    frame,
                    Rect::new(content.x, y, content.width, 1),
                    Style::default().bg(bg),
                );
                frame.render_widget(
                    Paragraph::new(format!("  {label}")).style(Style::default().fg(p.text).bg(bg)),
                    Rect::new(content.x, y, content.width, 1),
                );
            }
        }
    }
}

fn render_mobile_agent_summary(
    app: &AppState,
    frame: &mut Frame,
    viewport: Rect,
    content: Rect,
    doc_y: usize,
    scroll: usize,
    counts: (usize, usize, usize),
    expanded: bool,
    selected: bool,
) {
    let Some(y) = visible_y(viewport, scroll, doc_y) else {
        return;
    };
    let p = &app.palette;
    let bg = mobile_item_bg(selected, false, p);
    fill_rect(
        frame,
        Rect::new(content.x, y, content.width, 1),
        Style::default().bg(bg),
    );
    let (working_icon, working_style) = agent_icon(AgentState::Working, true, app.spinner_tick, p);
    let (idle_icon, idle_style) = agent_icon(AgentState::Idle, true, app.spinner_tick, p);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                if expanded { " ▾ " } else { " ▸ " },
                Style::default().fg(p.overlay1).bg(bg),
            ),
            Span::styled(
                "Agents ",
                Style::default()
                    .fg(p.text)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("◉", Style::default().fg(p.peach).bg(bg)),
            Span::styled(
                format!("{} triage ", counts.0),
                Style::default().fg(p.subtext0).bg(bg),
            ),
            Span::styled(working_icon, working_style.bg(bg)),
            Span::styled(
                format!("{} working ", counts.1),
                Style::default().fg(p.subtext0).bg(bg),
            ),
            Span::styled(idle_icon, idle_style.bg(bg)),
            Span::styled(
                format!("{} idle", counts.2),
                Style::default().fg(p.subtext0).bg(bg),
            ),
        ])),
        Rect::new(content.x, y, content.width, 1),
    );
}

fn render_mobile_agent_row(
    app: &AppState,
    frame: &mut Frame,
    viewport: Rect,
    content: Rect,
    doc_y: usize,
    scroll: usize,
    label: &str,
    meta: &str,
    status: (AgentState, bool),
    selected: bool,
) {
    let Some(y) = visible_y(viewport, scroll, doc_y) else {
        return;
    };
    let p = &app.palette;
    let bg = mobile_item_bg(selected, false, p);
    fill_rect(
        frame,
        Rect::new(content.x, y, content.width, 1),
        Style::default().bg(bg),
    );
    let meta_width = super::text::display_width_u16(meta)
        .saturating_add(1)
        .min(content.width / 2);
    let label_width = content.width.saturating_sub(meta_width);
    let (icon, icon_style) = agent_icon(status.0, status.1, app.spinner_tick, p);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("    ", Style::default().bg(bg)),
            Span::styled(icon, icon_style.bg(bg)),
            Span::styled(" ", Style::default().bg(bg)),
            Span::styled(
                truncate_end(label, label_width.saturating_sub(6) as usize),
                Style::default().fg(p.text).bg(bg),
            ),
        ])),
        Rect::new(content.x, y, label_width, 1),
    );
    frame.render_widget(
        Paragraph::new(meta)
            .style(Style::default().fg(p.overlay0).bg(bg))
            .alignment(Alignment::Right),
        Rect::new(content.x + label_width, y, meta_width, 1),
    );
}

fn render_mobile_hierarchy_row(
    app: &AppState,
    frame: &mut Frame,
    viewport: Rect,
    content: Rect,
    doc_y: usize,
    scroll: usize,
    row: &NavigatorRow,
    selected: bool,
) {
    let Some(y) = visible_y(viewport, scroll, doc_y) else {
        return;
    };
    let p = &app.palette;
    let bg = mobile_item_bg(selected, row.is_current, p);
    fill_rect(
        frame,
        Rect::new(content.x, y, content.width, 1),
        Style::default().bg(bg),
    );
    let group_idx = match row.target {
        NavigatorTarget::Group { group_idx } => Some(group_idx),
        NavigatorTarget::Workspace { ws_idx }
        | NavigatorTarget::Tab { ws_idx, .. }
        | NavigatorTarget::Pane { ws_idx, .. } => app
            .workspaces
            .get(ws_idx)
            .and_then(|workspace| app.group_index_by_id(&workspace.group_id)),
    };
    let accent = group_idx
        .map(|idx| app.group_accent_color(idx))
        .unwrap_or(p.accent);
    let (marker, marker_style) = match row.target {
        NavigatorTarget::Pane { .. } => {
            let (dot, style) = state_dot(row.status, row.seen, p);
            (dot.to_string(), style.bg(bg))
        }
        NavigatorTarget::Group { .. } => (" ".to_string(), Style::default().bg(bg)),
        _ if row.is_current => ("●".to_string(), Style::default().fg(accent).bg(bg)),
        _ => (" ".to_string(), Style::default().bg(bg)),
    };
    let label_style = if row.is_current {
        Style::default()
            .fg(accent)
            .bg(bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.text).bg(bg)
    };
    let (label, meta) = mobile_hierarchy_label_and_meta(app, row);
    let meta_width = super::text::display_width_u16(&meta)
        .saturating_add(u16::from(!meta.is_empty()))
        .min(content.width / 2);
    let label_width = content.width.saturating_sub(meta_width);
    let mut spans = vec![
        Span::styled("  ", Style::default().bg(bg)),
        Span::styled(marker, marker_style),
        Span::styled(" ", Style::default().bg(bg)),
    ];
    let label_room = label_width.saturating_sub(4);
    if let NavigatorTarget::Group { group_idx } = row.target {
        if let Some(group) = app.groups.get(group_idx) {
            let icon_width = super::text::display_width_u16(&group.icon);
            spans.push(Span::styled(
                group.icon.clone(),
                Style::default()
                    .fg(accent)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(" ", Style::default().bg(bg)));
            spans.push(Span::styled(
                truncate_end(
                    &group.name,
                    label_room.saturating_sub(icon_width.saturating_add(1)) as usize,
                ),
                label_style,
            ));
        } else {
            spans.push(Span::styled(
                truncate_end(&label, label_room as usize),
                label_style,
            ));
        }
    } else {
        spans.push(Span::styled(
            truncate_end(&label, label_room as usize),
            label_style,
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(content.x, y, label_width, 1),
    );
    if meta_width > 0 {
        frame.render_widget(
            Paragraph::new(meta)
                .style(Style::default().fg(p.overlay0).bg(bg))
                .alignment(Alignment::Right),
            Rect::new(content.x + label_width, y, meta_width, 1),
        );
    }
}

fn mobile_hierarchy_label_and_meta(app: &AppState, row: &NavigatorRow) -> (String, String) {
    match row.target {
        NavigatorTarget::Group { group_idx } => {
            let count = app
                .groups
                .get(group_idx)
                .map(|group| {
                    app.workspaces
                        .iter()
                        .filter(|workspace| workspace.group_id == group.id)
                        .count()
                })
                .unwrap_or(0);
            (row.label.clone(), count_label(count, "space", "spaces"))
        }
        NavigatorTarget::Workspace { ws_idx } => {
            let count = app
                .workspaces
                .get(ws_idx)
                .map(|workspace| workspace.tabs.len())
                .unwrap_or(0);
            (
                strip_trailing_count(&row.label).to_string(),
                count_label(count, "tab", "tabs"),
            )
        }
        NavigatorTarget::Tab { ws_idx, tab_idx } => {
            let count = app
                .workspaces
                .get(ws_idx)
                .and_then(|workspace| workspace.tabs.get(tab_idx))
                .map(|tab| tab.panes.len())
                .unwrap_or(0);
            (row.label.clone(), count_label(count, "pane", "panes"))
        }
        NavigatorTarget::Pane { .. } => (row.label.clone(), row.meta.clone()),
    }
}

fn strip_trailing_count(label: &str) -> &str {
    label
        .rsplit_once(" (")
        .filter(|(_, suffix)| {
            suffix
                .strip_suffix(')')
                .is_some_and(|count| count.chars().all(|ch| ch.is_ascii_digit()))
        })
        .map_or(label, |(name, _)| name)
}

fn count_label(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

fn render_section_title_at(
    frame: &mut Frame,
    viewport: Rect,
    content: Rect,
    doc_y: usize,
    scroll: usize,
    title: &str,
    p: &Palette,
) {
    let Some(y) = visible_y(viewport, scroll, doc_y) else {
        return;
    };
    render_section_title(
        frame,
        Rect::new(content.x, y, content.width.saturating_sub(1), 1),
        title,
        p,
    );
}

fn visible_y(viewport: Rect, scroll: usize, doc_y: usize) -> Option<u16> {
    let offset = doc_y.checked_sub(scroll)?;
    (offset < viewport.height as usize).then_some(viewport.y + offset as u16)
}

fn mobile_item_bg(selected: bool, active: bool, p: &Palette) -> ratatui::style::Color {
    if selected {
        p.surface0
    } else if active {
        p.surface_dim
    } else {
        p.panel_bg
    }
}

fn inset_for_left_scrollbar(area: Rect) -> Rect {
    if area.width <= 1 {
        return Rect::default();
    }
    Rect::new(area.x + 1, area.y, area.width - 1, area.height)
}

fn render_left_scrollbar(
    frame: &mut Frame,
    area: Rect,
    total_rows: usize,
    visible_rows: usize,
    scroll: usize,
    p: &Palette,
) {
    if area.width == 0 || area.height == 0 || visible_rows == 0 || total_rows <= visible_rows {
        return;
    }

    let track = Rect::new(area.x, area.y, 1, area.height);
    let max_scroll = total_rows.saturating_sub(visible_rows);
    let thumb_len = ((track.height as usize * visible_rows).div_ceil(total_rows))
        .max(1)
        .min(track.height as usize) as u16;
    let travel = track.height.saturating_sub(thumb_len);
    let thumb_top = track.y + ((travel as usize * scroll.min(max_scroll)) / max_scroll) as u16;

    for y in track.y..track.y + track.height {
        let is_thumb = y >= thumb_top && y < thumb_top + thumb_len;
        frame.buffer_mut()[(track.x, y)]
            .set_symbol(if is_thumb { "▌" } else { "│" })
            .set_style(
                Style::default()
                    .fg(if is_thumb { p.accent } else { p.surface_dim })
                    .bg(p.panel_bg),
            );
    }
}

fn render_section_title(frame: &mut Frame, area: Rect, title: &str, p: &Palette) {
    frame.render_widget(
        Paragraph::new(format!(" {title} ")).style(
            Style::default()
                .fg(p.overlay1)
                .bg(p.panel_bg)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ),
        Rect::new(area.x, area.y, area.width, 1),
    );
}

fn rect_contains(rect: Rect, col: u16, row: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && col >= rect.x
        && col < rect.x + rect.width
        && row >= rect.y
        && row < rect.y + rect.height
}

fn mobile_screen_rect(app: &AppState) -> Rect {
    let header = app.view.mobile_header_rect;
    let terminal = app.view.terminal_area;
    let x = header.x.min(terminal.x);
    let y = header.y.min(terminal.y);
    let right = (header.x + header.width).max(terminal.x + terminal.width);
    let bottom = (header.y + header.height).max(terminal.y + terminal.height);
    Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
}

fn mobile_toast_title(toast: &ToastNotification) -> String {
    match toast.kind {
        ToastKind::NeedsAttention => toast
            .title
            .strip_suffix(" needs attention")
            .map(|agent| format!("{agent} waiting"))
            .unwrap_or_else(|| toast.title.clone()),
        ToastKind::Finished => toast
            .title
            .strip_suffix(" finished")
            .map(|agent| format!("{agent} done"))
            .unwrap_or_else(|| toast.title.clone()),
        ToastKind::UpdateInstalled => "update ready".to_string(),
    }
}

fn draw_horizontal_rule(frame: &mut Frame, area: Rect, p: &Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let buf = frame.buffer_mut();
    for x in area.x..area.x + area.width {
        buf[(x, area.y)]
            .set_symbol("─")
            .set_style(Style::default().fg(p.surface_dim).bg(p.panel_bg));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hierarchy_fixture() -> (AppState, usize, PaneId) {
        let mut app = crate::app::state::AppState::test_new();
        let group_idx = app.create_group("Infrastructure".to_string());
        app.groups[group_idx].icon = "■".to_string();
        app.groups[group_idx].accent = Some(crate::config::TerminalAccent::Red);

        let default_space = crate::workspace::Workspace::test_new("default");
        let mut active_space = crate::workspace::Workspace::test_new("observability");
        active_space.group_id = app.groups[group_idx].id.clone();
        active_space.custom_name = Some("Observability".to_string());
        active_space.tabs[0].set_custom_name("dashboards".to_string());
        let focused_pane = active_space.test_split(ratatui::layout::Direction::Horizontal);
        active_space.test_add_tab(Some("logs"));
        active_space.active_tab = 0;
        active_space.tabs[0].layout.focus_pane(focused_pane);

        let mut sibling_space = crate::workspace::Workspace::test_new("alerts");
        sibling_space.group_id = app.groups[group_idx].id.clone();
        sibling_space.custom_name = Some("Alerts".to_string());

        app.workspaces = vec![default_space, active_space, sibling_space];
        app.active = Some(1);
        app.selected = 1;
        app.active_group = group_idx;
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[1].tabs[0].panes[&focused_pane]
            .attached_terminal_id
            .clone();
        app.terminals
            .get_mut(&terminal_id)
            .expect("focused pane terminal")
            .set_detected_state(Some(crate::detect::Agent::Claude), AgentState::Working);
        (app, group_idx, focused_pane)
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn mobile_navigation_exposes_only_the_current_hierarchy_level() {
        let (mut app, group_idx, focused_pane) = hierarchy_fixture();
        let terminal_runtimes = TerminalRuntimeRegistry::new();

        app.mobile_switcher_level = MobileSwitcherLevel::Groups;
        let group_targets = mobile_navigation_rows(&app, &terminal_runtimes, None)
            .iter()
            .filter_map(MobileNavigationRow::target)
            .collect::<Vec<_>>();
        assert!(group_targets.contains(&MobileSwitcherTarget::Group(group_idx)));
        assert!(!group_targets
            .iter()
            .any(|target| matches!(target, MobileSwitcherTarget::Workspace(_))));

        app.mobile_switcher_level = MobileSwitcherLevel::Workspaces;
        let workspace_targets = mobile_navigation_rows(&app, &terminal_runtimes, None)
            .iter()
            .filter_map(MobileNavigationRow::target)
            .collect::<Vec<_>>();
        assert!(workspace_targets.contains(&MobileSwitcherTarget::Workspace(1)));
        assert!(workspace_targets.contains(&MobileSwitcherTarget::Workspace(2)));
        assert!(workspace_targets.contains(&MobileSwitcherTarget::NewSpace));
        assert!(!workspace_targets
            .iter()
            .any(|target| matches!(target, MobileSwitcherTarget::Tab { .. })));

        app.mobile_switcher_level = MobileSwitcherLevel::Tabs { ws_idx: 1 };
        let tab_targets = mobile_navigation_rows(&app, &terminal_runtimes, None)
            .iter()
            .filter_map(MobileNavigationRow::target)
            .collect::<Vec<_>>();
        assert!(tab_targets.contains(&MobileSwitcherTarget::Tab {
            ws_idx: 1,
            tab_idx: 0,
        }));
        assert!(tab_targets.contains(&MobileSwitcherTarget::Tab {
            ws_idx: 1,
            tab_idx: 1,
        }));
        assert!(tab_targets.contains(&MobileSwitcherTarget::NewTab));
        assert!(!tab_targets
            .iter()
            .any(|target| matches!(target, MobileSwitcherTarget::Pane { .. })));

        app.mobile_switcher_level = MobileSwitcherLevel::Panes {
            ws_idx: 1,
            tab_idx: 0,
        };
        let pane_rows = mobile_navigation_rows(&app, &terminal_runtimes, None);
        assert!(pane_rows.iter().any(|row| {
            row.target()
                == Some(MobileSwitcherTarget::Pane {
                    ws_idx: 1,
                    tab_idx: 0,
                    pane_id: focused_pane,
                })
        }));
        assert!(pane_rows.iter().any(|row| {
            matches!(
                row,
                MobileNavigationRow::Hierarchy { row, .. }
                    if row.label.eq_ignore_ascii_case("claude") && row.meta.contains("working")
            )
        }));
        assert!(!pane_rows
            .iter()
            .any(|row| matches!(row.target(), Some(MobileSwitcherTarget::Tab { .. }))));
    }

    #[test]
    fn mobile_header_is_one_row_and_uses_the_active_group_identity() {
        let (mut app, group_idx, _) = hierarchy_fixture();
        app.view.mobile_menu_hit_area = Rect::new(34, 0, 10, 1);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let backend = ratatui::backend::TestBackend::new(44, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_mobile_header(&app, &terminal_runtimes, frame, Rect::new(0, 0, 44, 1))
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer);

        assert!(text.contains("Observability"), "header: {text:?}");
        assert!(text.contains("dashboards"), "header: {text:?}");
        assert!(!text.contains("no agents"), "header: {text:?}");
        let icon_x = (0..44)
            .find(|x| buffer[(*x, 0)].symbol() == "■")
            .expect("group icon");
        assert_eq!(
            buffer[(icon_x, 0)].style().fg,
            Some(app.group_accent_color(group_idx))
        );
    }

    #[test]
    fn mobile_group_rows_use_their_icons_as_accent_markers() {
        let (mut app, active_group, _) = hierarchy_fixture();
        app.groups[0].icon = "✚".to_string();
        app.groups[0].accent = Some(crate::config::TerminalAccent::Green);
        app.view.mobile_header_rect = Rect::new(0, 0, 44, 1);
        app.view.terminal_area = Rect::new(0, 1, 44, 19);
        app.mobile_switcher_level = MobileSwitcherLevel::Groups;
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(44, 20)).unwrap();

        terminal
            .draw(|frame| {
                render_mobile_panel(&app, &terminal_runtimes, frame, Rect::new(0, 0, 44, 20))
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let find_symbol = |symbol: &str| {
            (0..20)
                .find_map(|y| {
                    (0..44)
                        .find(|x| buffer[(*x, y)].symbol() == symbol)
                        .map(|x| (x, y))
                })
                .unwrap_or_else(|| panic!("group icon {symbol:?}"))
        };
        let default_icon = find_symbol("✚");
        let active_icon = find_symbol("■");

        assert_eq!(
            buffer[default_icon].style().fg,
            Some(app.group_accent_color(0))
        );
        assert_eq!(
            buffer[active_icon].style().fg,
            Some(app.group_accent_color(active_group))
        );
        assert_ne!(buffer[(active_icon.0 - 2, active_icon.1)].symbol(), "●");
    }

    #[test]
    fn mobile_agent_summary_is_persistent_and_expands_to_navigable_agents() {
        let (mut app, _, focused_pane) = hierarchy_fixture();
        app.view.mobile_header_rect = Rect::new(0, 0, 44, 1);
        app.view.terminal_area = Rect::new(0, 1, 44, 19);
        app.mobile_switcher_level = MobileSwitcherLevel::Groups;
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(44, 20)).unwrap();

        terminal
            .draw(|frame| {
                render_mobile_panel(&app, &terminal_runtimes, frame, Rect::new(0, 0, 44, 20))
            })
            .unwrap();
        let collapsed = buffer_text(terminal.backend().buffer());
        assert!(collapsed.contains("Agents"), "switcher: {collapsed:?}");
        assert!(collapsed.contains("1 working"), "switcher: {collapsed:?}");
        assert!(!collapsed.to_lowercase().contains("claude"));

        app.mobile_agents_expanded = true;
        terminal
            .draw(|frame| {
                render_mobile_panel(&app, &terminal_runtimes, frame, Rect::new(0, 0, 44, 20))
            })
            .unwrap();
        let expanded = buffer_text(terminal.backend().buffer());
        assert!(expanded.to_lowercase().contains("claude"));
        assert!(expanded.contains("working"));
        assert!(mobile_navigation_rows(&app, &terminal_runtimes, None)
            .iter()
            .any(|row| {
                row.target()
                    == Some(MobileSwitcherTarget::Agent {
                        ws_idx: 1,
                        tab_idx: 0,
                        pane_id: focused_pane,
                    })
            }));
    }

    #[test]
    fn mobile_switcher_renders_progressive_levels_identically_for_monolithic_and_attached_views() {
        let (mut app, group_idx, focused_pane) = hierarchy_fixture();
        app.view.mobile_header_rect = Rect::new(0, 0, 44, 1);
        app.view.terminal_area = Rect::new(0, 1, 44, 19);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut view = ClientViewState::from_default_client_state(&app);
        view.computed.mobile_header_rect = app.view.mobile_header_rect;
        view.computed.terminal_area = app.view.terminal_area;

        let cases = [
            (
                MobileSwitcherLevel::Groups,
                "Infrastructure",
                "Observability",
                MobileSwitcherTarget::Group(group_idx),
            ),
            (
                MobileSwitcherLevel::Workspaces,
                "Observability",
                "dashboards",
                MobileSwitcherTarget::Workspace(1),
            ),
            (
                MobileSwitcherLevel::Tabs { ws_idx: 1 },
                "dashboards",
                "claude",
                MobileSwitcherTarget::Tab {
                    ws_idx: 1,
                    tab_idx: 0,
                },
            ),
            (
                MobileSwitcherLevel::Panes {
                    ws_idx: 1,
                    tab_idx: 0,
                },
                "claude",
                "logs",
                MobileSwitcherTarget::Pane {
                    ws_idx: 1,
                    tab_idx: 0,
                    pane_id: focused_pane,
                },
            ),
        ];

        for (level, visible, hidden, target) in cases {
            app.mobile_switcher_level = level;
            view.mobile_switcher_level = level;

            let mut monolithic =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(44, 20)).unwrap();
            monolithic
                .draw(|frame| {
                    render_mobile_panel(&app, &terminal_runtimes, frame, Rect::new(0, 0, 44, 20))
                })
                .unwrap();
            let mut attached =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(44, 20)).unwrap();
            attached
                .draw(|frame| {
                    render_mobile_panel_for_view(
                        &app,
                        &terminal_runtimes,
                        &view,
                        frame,
                        Rect::new(0, 0, 44, 20),
                    )
                })
                .unwrap();

            assert_eq!(monolithic.backend().buffer(), attached.backend().buffer());
            let text = buffer_text(monolithic.backend().buffer());
            assert!(text.contains(visible), "{level:?} switcher: {text:?}");
            assert!(!text.contains(hidden), "{level:?} switcher: {text:?}");

            let rows = mobile_navigation_rows(&app, &terminal_runtimes, None);
            let row = rows
                .iter()
                .position(|row| row.target() == Some(target))
                .expect("target row");
            let viewport = mobile_switcher_areas(&app).viewport;
            assert_eq!(
                mobile_switcher_target_at(&app, viewport.x + 1, viewport.y + row as u16),
                Some(target)
            );
            assert_eq!(
                mobile_switcher_target_at_for_view(
                    &app,
                    &terminal_runtimes,
                    &view,
                    viewport.x + 1,
                    viewport.y + row as u16,
                ),
                Some(target)
            );
        }
    }

    #[test]
    fn mobile_new_tab_action_renders_after_the_tab_list() {
        let (mut app, _, _) = hierarchy_fixture();
        app.view.mobile_header_rect = Rect::new(0, 0, 44, 1);
        app.view.terminal_area = Rect::new(0, 1, 44, 19);
        app.mobile_switcher_level = MobileSwitcherLevel::Tabs { ws_idx: 1 };
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(44, 20)).unwrap();

        terminal
            .draw(|frame| {
                render_mobile_panel(&app, &terminal_runtimes, frame, Rect::new(0, 0, 44, 20))
            })
            .unwrap();

        let lines = buffer_text(terminal.backend().buffer())
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let dashboards = lines
            .iter()
            .position(|line| line.contains("dashboards"))
            .expect("dashboards tab");
        let logs = lines
            .iter()
            .position(|line| line.contains("logs"))
            .expect("logs tab");
        let new_tab = lines
            .iter()
            .position(|line| line.contains("+ New tab"))
            .expect("new tab action");
        let more_actions = lines
            .iter()
            .position(|line| line.contains("More actions"))
            .expect("more actions");

        assert!(dashboards < logs);
        assert!(logs + 1 < new_tab, "new tab must be outside the tab list");
        assert!(
            lines[new_tab - 1].contains('─'),
            "new tab must be separated from tab rows"
        );
        assert!(new_tab < more_actions);
    }

    #[tokio::test]
    async fn mobile_header_uses_live_root_runtime_cwd_for_workspace_label() {
        let unique = format!(
            "omh-mobile-header-runtime-cwd-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let stale_cwd = root.join("issue-264-nix-support");
        let live_cwd = root.join("omh");
        std::fs::create_dir_all(stale_cwd.join(".git")).unwrap();
        std::fs::create_dir_all(live_cwd.join(".git")).unwrap();

        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = crate::workspace::Workspace::test_new("stale-name");
        workspace.custom_name = None;
        workspace.identity_cwd = stale_cwd.clone();
        let pane = workspace.tabs[0].root_pane;

        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().cwd = stale_cwd;
        app.active = Some(0);
        app.selected = 0;
        app.view.mobile_menu_hit_area = Rect::new(30, 0, 10, 2);

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
        let backend = ratatui::backend::TestBackend::new(40, 2);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_mobile_header(&app, &runtime_registry, frame, Rect::new(0, 0, 40, 2))
            })
            .unwrap();
        let row = (0..40)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
            .collect::<String>();

        for (_, runtime) in runtime_registry.drain() {
            runtime.shutdown();
        }
        let _ = std::fs::remove_dir_all(root);

        assert!(row.contains("omh"), "header row: {row:?}");
        assert!(
            !row.contains("issue-264-nix-support"),
            "header row: {row:?}"
        );
    }
}
