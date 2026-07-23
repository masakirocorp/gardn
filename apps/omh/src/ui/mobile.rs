use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use super::status::{
    agent_icon, agent_section_icon, agent_section_style, state_dot, state_label, toast_kind_color,
};
use super::text::truncate_end;
use super::widgets::fill_rect;
use crate::app::state::{
    MobileSwitcherLevel, NavigatorRow, NavigatorTarget, Palette, ToastKind, ToastNotification,
};
use crate::app::{AppState, ClientViewState};
use crate::detect::AgentState;
use crate::layout::PaneId;
use crate::terminal::TerminalRuntimeRegistry;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MobileSwitcherAreas {
    pub panel: Rect,
    pub close: Rect,
    pub breadcrumb: Rect,
    pub viewport: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileSwitcherTarget {
    ToggleAgents,
    Agent {
        ws_idx: usize,
        tab_idx: usize,
        pane_id: PaneId,
    },
    Group(usize),
    NewGroup,
    Workspace(usize),
    NewSpace {
        group_idx: usize,
    },
    Tab {
        ws_idx: usize,
        tab_idx: usize,
    },
    NewTab {
        ws_idx: usize,
    },
    Pane {
        ws_idx: usize,
        tab_idx: usize,
        pane_id: PaneId,
    },
    SplitRight,
    SplitDown,
}

#[derive(Debug, PartialEq, Eq)]
enum MobileNavigationRow {
    AgentSummary {
        triage: usize,
        working: usize,
        idle: usize,
    },
    Agent {
        label: String,
        meta: String,
        state: AgentState,
        seen: bool,
        target: MobileSwitcherTarget,
    },
    Empty(&'static str),
    Divider,
    Action {
        label: &'static str,
        target: MobileSwitcherTarget,
    },
    Hierarchy {
        row: NavigatorRow,
        target: MobileSwitcherTarget,
    },
}

impl MobileNavigationRow {
    fn target(&self) -> Option<MobileSwitcherTarget> {
        match self {
            Self::Empty(_) | Self::Divider => None,
            Self::AgentSummary { .. } => Some(MobileSwitcherTarget::ToggleAgents),
            Self::Action { target, .. }
            | Self::Agent { target, .. }
            | Self::Hierarchy { target, .. } => Some(*target),
        }
    }
}

fn mobile_navigation_rows(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: Option<&ClientViewState>,
) -> Vec<MobileNavigationRow> {
    let agents_expanded = view.map_or(app.mobile_agents_expanded, |view| {
        view.mobile_agents_expanded
    });
    if agents_expanded {
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
        let mut rows = vec![MobileNavigationRow::AgentSummary {
            triage: section_count("triage"),
            working: section_count("working"),
            idle: section_count("idle"),
        }];
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
        return rows;
    }

    let hierarchy = view.map_or_else(
        || app.mobile_navigation_rows(terminal_runtimes),
        |view| app.mobile_navigation_rows_for_view(view, terminal_runtimes),
    );
    let level = view.map_or(app.mobile_switcher_level, |view| view.mobile_switcher_level);
    let mut rows = Vec::new();

    match level {
        MobileSwitcherLevel::Groups => {
            rows.extend(hierarchy.into_iter().filter_map(|row| match row.target {
                NavigatorTarget::Group { group_idx } => Some(MobileNavigationRow::Hierarchy {
                    row,
                    target: MobileSwitcherTarget::Group(group_idx),
                }),
                _ => None,
            }));
            append_mobile_footer(&mut rows, [("+ New group", MobileSwitcherTarget::NewGroup)]);
        }
        MobileSwitcherLevel::Workspaces { group_idx } => {
            rows.extend(hierarchy.into_iter().filter_map(|row| {
                match row.target {
                    NavigatorTarget::Workspace { ws_idx }
                        if app
                            .workspaces
                            .get(ws_idx)
                            .and_then(|workspace| app.group_index_by_id(&workspace.group_id))
                            == Some(group_idx) =>
                    {
                        Some(MobileNavigationRow::Hierarchy {
                            row,
                            target: MobileSwitcherTarget::Workspace(ws_idx),
                        })
                    }
                    _ => None,
                }
            }));
            if rows.is_empty() {
                rows.push(MobileNavigationRow::Empty("No spaces"));
            }
            append_mobile_footer(
                &mut rows,
                [("+ New space", MobileSwitcherTarget::NewSpace { group_idx })],
            );
        }
        MobileSwitcherLevel::Tabs { ws_idx } => {
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
            if rows.is_empty() {
                rows.push(MobileNavigationRow::Empty("No tabs"));
            }
            append_mobile_footer(
                &mut rows,
                [("+ New tab", MobileSwitcherTarget::NewTab { ws_idx })],
            );
        }
        MobileSwitcherLevel::Panes { ws_idx, tab_idx } => {
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
            if rows.is_empty() {
                rows.push(MobileNavigationRow::Empty("No panes"));
            }
            let split_area = app
                .workspaces
                .get(ws_idx)
                .and_then(|workspace| {
                    let tab = workspace.tabs.get(tab_idx)?;
                    let focused_pane = view
                        .and_then(|view| view.focused_pane_for_tab(&workspace.id, tab_idx + 1))
                        .unwrap_or_else(|| tab.layout.focused());
                    let pane_infos = view
                        .map(|view| view.computed.pane_infos.as_slice())
                        .unwrap_or(app.view.pane_infos.as_slice());
                    pane_infos
                        .iter()
                        .find(|pane| pane.id == focused_pane)
                        .map(|pane| pane.inner_rect)
                })
                .unwrap_or_else(|| {
                    view.map(|view| view.computed.terminal_area)
                        .unwrap_or(app.view.terminal_area)
                });
            if split_area.width >= 2 || split_area.height >= 2 {
                rows.push(MobileNavigationRow::Divider);
                if split_area.width >= 2 {
                    rows.push(MobileNavigationRow::Action {
                        label: "+ Split right",
                        target: MobileSwitcherTarget::SplitRight,
                    });
                }
                if split_area.height >= 2 {
                    rows.push(MobileNavigationRow::Action {
                        label: "+ Split down",
                        target: MobileSwitcherTarget::SplitDown,
                    });
                }
            }
        }
    }
    rows
}

fn append_mobile_footer<const N: usize>(
    rows: &mut Vec<MobileNavigationRow>,
    actions: [(&'static str, MobileSwitcherTarget); N],
) {
    rows.push(MobileNavigationRow::Divider);
    rows.extend(
        actions
            .into_iter()
            .map(|(label, target)| MobileNavigationRow::Action { label, target }),
    );
}

pub(crate) fn is_mobile_width(area: Rect, threshold: u16) -> bool {
    area.width > 0 && area.width <= threshold
}

pub(crate) fn mobile_agent_strip_rect(header: Rect) -> Rect {
    if header.height < 2 {
        return Rect::default();
    }
    Rect::new(header.x, header.y + 1, header.width, 1)
}

pub(crate) fn mobile_switcher_areas(app: &AppState) -> MobileSwitcherAreas {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    let row_count = mobile_navigation_rows(app, &terminal_runtimes, None).len();
    mobile_switcher_areas_for_rows(
        mobile_screen_rect(app),
        row_count,
        app.mobile_agents_expanded,
    )
}

pub(crate) fn mobile_switcher_areas_for_view(
    app: &AppState,
    view: &ClientViewState,
) -> MobileSwitcherAreas {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    let row_count = mobile_navigation_rows(app, &terminal_runtimes, Some(view)).len();
    mobile_switcher_areas_for_rows(view.screen_rect(), row_count, view.mobile_agents_expanded)
}

fn mobile_switcher_areas_for_rows(
    screen: Rect,
    row_count: usize,
    agents_expanded: bool,
) -> MobileSwitcherAreas {
    if screen.width == 0 || screen.height <= 1 {
        return MobileSwitcherAreas::default();
    }

    if !agents_expanded {
        let viewport_height = (row_count.max(1) as u16).min(screen.height - 1);
        let viewport = Rect::new(screen.x, screen.y + 1, screen.width, viewport_height);
        return MobileSwitcherAreas {
            panel: viewport,
            viewport,
            ..MobileSwitcherAreas::default()
        };
    }

    const CHROME_HEIGHT: u16 = 3;
    if screen.height <= CHROME_HEIGHT {
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

pub(crate) fn mobile_switcher_target_index(app: &AppState, target: MobileSwitcherTarget) -> usize {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    mobile_navigation_rows(app, &terminal_runtimes, None)
        .iter()
        .filter_map(MobileNavigationRow::target)
        .position(|candidate| candidate == target)
        .unwrap_or(0)
}

pub(crate) fn mobile_switcher_target_index_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    target: MobileSwitcherTarget,
) -> usize {
    mobile_navigation_rows(app, terminal_runtimes, Some(view))
        .iter()
        .filter_map(MobileNavigationRow::target)
        .position(|candidate| candidate == target)
        .unwrap_or(0)
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

    super::render_context_bar(app, &app.view.context_bar, frame);
    render_mobile_agent_strip(
        app,
        terminal_runtimes,
        None,
        frame,
        mobile_agent_strip_rect(area),
    );
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

    super::render_context_bar(app, &view.computed.context_bar, frame);
    render_mobile_agent_strip(
        app,
        terminal_runtimes,
        Some(view),
        frame,
        mobile_agent_strip_rect(area),
    );
}

fn render_mobile_agent_strip(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: Option<&ClientViewState>,
    frame: &mut Frame,
    area: Rect,
) {
    if area == Rect::default() {
        return;
    }
    let sections = view.map_or_else(
        || super::sidebar::agent_panel_sections_all_workspaces(app, terminal_runtimes),
        |view| {
            super::sidebar::agent_panel_sections_all_workspaces_for_view(
                app,
                terminal_runtimes,
                view,
            )
        },
    );
    let count = |label: &str| {
        sections
            .iter()
            .find(|section| section.label == label)
            .map_or(0, |section| section.entries.len())
    };
    render_mobile_agent_summary(
        app,
        frame,
        area,
        area,
        0,
        0,
        (count("triage"), count("working"), count("idle")),
        view.map_or(app.mobile_agents_expanded, |view| {
            view.mobile_agents_expanded
        }),
        false,
    );
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
    render_mobile_panel_shell(app, app.mobile_agents_expanded, frame, areas);
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
    render_mobile_panel_shell(app, view.mobile_agents_expanded, frame, areas);
    render_mobile_switcher_content_for_view(app, terminal_runtimes, view, frame, areas.viewport);
}

fn render_mobile_panel_shell(
    app: &AppState,
    agents_expanded: bool,
    frame: &mut Frame,
    areas: MobileSwitcherAreas,
) {
    if areas.panel == Rect::default() {
        return;
    }
    let p = &app.palette;
    frame.render_widget(Clear, areas.panel);
    fill_rect(frame, areas.panel, Style::default().bg(p.panel_bg));
    if !agents_expanded {
        return;
    }

    frame.render_widget(
        Paragraph::new(" agents").style(
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
    frame.render_widget(
        Paragraph::new("   All agents").style(Style::default().fg(p.text).bg(p.panel_bg)),
        areas.breadcrumb,
    );
    draw_horizontal_rule(
        frame,
        Rect::new(areas.panel.x, areas.panel.y + 2, areas.panel.width, 1),
        p,
    );
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
            MobileNavigationRow::Empty(label) => {
                let Some(y) = visible_y(viewport, scroll, doc_y) else {
                    continue;
                };
                frame.render_widget(
                    Paragraph::new(format!("  {label}")).style(
                        Style::default()
                            .fg(p.overlay0)
                            .bg(p.panel_bg)
                            .add_modifier(Modifier::DIM),
                    ),
                    Rect::new(content.x, y, content.width, 1),
                );
            }
            MobileNavigationRow::AgentSummary {
                triage,
                working,
                idle,
            } => {
                render_mobile_agent_summary(
                    app,
                    frame,
                    viewport,
                    content,
                    doc_y,
                    scroll,
                    (*triage, *working, *idle),
                    true,
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
            MobileNavigationRow::Action { label, .. } => {
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
                    Paragraph::new(format!("  {label}")).style(
                        Style::default()
                            .fg(p.text)
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
    let mut spans = vec![
        Span::styled(
            if expanded { " ▾ " } else { " ▸ " },
            Style::default().fg(p.overlay1).bg(bg),
        ),
        Span::styled(
            "Agents",
            Style::default()
                .fg(p.text)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if counts == (0, 0, 0) {
        spans.push(Span::styled(
            " no agents",
            Style::default()
                .fg(p.overlay0)
                .bg(bg)
                .add_modifier(Modifier::DIM),
        ));
    } else {
        for (label, count) in [
            ("triage", counts.0),
            ("working", counts.1),
            ("idle", counts.2),
        ] {
            if count == 0 {
                continue;
            }
            let (icon, icon_style) = agent_section_icon(label, app.spinner_tick, p);
            spans.push(Span::styled(" ", Style::default().bg(bg)));
            spans.push(Span::styled(icon, icon_style.bg(bg)));
            spans.push(Span::styled(
                format!(" {count} {label}"),
                agent_section_style(label, p).bg(bg),
            ));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
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

        app.mobile_switcher_level = MobileSwitcherLevel::Workspaces { group_idx };
        let workspace_targets = mobile_navigation_rows(&app, &terminal_runtimes, None)
            .iter()
            .filter_map(MobileNavigationRow::target)
            .collect::<Vec<_>>();
        assert!(workspace_targets.contains(&MobileSwitcherTarget::Workspace(1)));
        assert!(workspace_targets.contains(&MobileSwitcherTarget::Workspace(2)));
        assert!(workspace_targets.contains(&MobileSwitcherTarget::NewSpace { group_idx }));
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
        assert!(tab_targets.contains(&MobileSwitcherTarget::NewTab { ws_idx: 1 }));
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
    fn pane_dropdown_split_actions_follow_available_geometry() {
        let (mut app, _, _) = hierarchy_fixture();
        app.mobile_switcher_level = MobileSwitcherLevel::Panes {
            ws_idx: 1,
            tab_idx: 0,
        };
        let terminal_runtimes = TerminalRuntimeRegistry::new();

        app.view.terminal_area = Rect::new(0, 1, 1, 1);
        let targets = mobile_navigation_rows(&app, &terminal_runtimes, None)
            .iter()
            .filter_map(MobileNavigationRow::target)
            .collect::<Vec<_>>();
        assert!(!targets.contains(&MobileSwitcherTarget::SplitRight));
        assert!(!targets.contains(&MobileSwitcherTarget::SplitDown));

        app.view.terminal_area = Rect::new(0, 1, 2, 1);
        let targets = mobile_navigation_rows(&app, &terminal_runtimes, None)
            .iter()
            .filter_map(MobileNavigationRow::target)
            .collect::<Vec<_>>();
        assert!(targets.contains(&MobileSwitcherTarget::SplitRight));
        assert!(!targets.contains(&MobileSwitcherTarget::SplitDown));
    }

    #[test]
    fn mobile_header_keeps_agent_summary_visible_below_context() {
        let (mut app, _, _) = hierarchy_fixture();
        app.view.mobile_header_rect = Rect::new(0, 0, 44, 2);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let active_tab = app
            .active
            .and_then(|ws_idx| app.workspaces.get(ws_idx))
            .map(crate::workspace::Workspace::active_tab_index);
        let focused_pane = app
            .active
            .and_then(|ws_idx| app.workspaces.get(ws_idx))
            .and_then(crate::workspace::Workspace::focused_pane_id);
        app.view.context_bar = super::super::compute_mobile_breadcrumb(
            &app,
            &terminal_runtimes,
            app.active,
            app.active_group,
            active_tab,
            focused_pane,
            crate::app::ClientTabControl::default(),
            Rect::new(0, 0, 44, 1),
        );
        let backend = ratatui::backend::TestBackend::new(44, 2);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_mobile_header(&app, &terminal_runtimes, frame, Rect::new(0, 0, 44, 2))
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer);

        assert!(text.contains("Obser"), "header: {text:?}");
        assert!(text.contains("dashb"), "header: {text:?}");
        assert!(!text.contains("no agents"), "header: {text:?}");
        assert!(text.contains("Agents"), "header: {text:?}");
        assert!(text.contains("1 working"), "header: {text:?}");
        assert!(!text.contains("triage"), "header: {text:?}");
        assert!(!text.contains("idle"), "header: {text:?}");
        let working_x = (0..44)
            .find(|x| buffer[(*x, 1)].symbol() == "w")
            .expect("working label");
        assert_eq!(
            buffer[(working_x, 1)].style().fg,
            agent_section_style("working", &app.palette).fg
        );
        assert!(buffer[(working_x, 1)]
            .style()
            .add_modifier
            .contains(Modifier::BOLD));
    }

    #[test]
    fn mobile_agent_summary_uses_sidebar_status_colors() {
        let app = crate::app::state::AppState::test_new();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 1)).unwrap();

        terminal
            .draw(|frame| {
                render_mobile_agent_summary(
                    &app,
                    frame,
                    Rect::new(0, 0, 60, 1),
                    Rect::new(0, 0, 60, 1),
                    0,
                    0,
                    (2, 1, 3),
                    false,
                    false,
                )
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer);
        for (label, count) in [("triage", 2), ("working", 1), ("idle", 3)] {
            assert!(
                text.contains(&format!("{count} {label}")),
                "summary: {text:?}"
            );
            let label_x = (0..60)
                .find(|x| {
                    label.chars().enumerate().all(|(offset, ch)| {
                        buffer[(*x + offset as u16, 0)].symbol() == ch.to_string()
                    })
                })
                .unwrap_or_else(|| panic!("{label} label"));
            let style = buffer[(label_x, 0)].style();
            assert_eq!(style.fg, agent_section_style(label, &app.palette).fg);
            assert!(style.add_modifier.contains(Modifier::BOLD));
        }
        assert!(text.contains("! 2 triage"), "summary: {text:?}");
        assert!(text.contains("✓ 3 idle"), "summary: {text:?}");
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
        assert!(
            collapsed.contains("Infrastructure"),
            "groups: {collapsed:?}"
        );
        assert!(collapsed.contains("group 1"), "groups: {collapsed:?}");
        assert!(!collapsed.contains("Agents"), "groups: {collapsed:?}");

        app.mobile_agents_expanded = true;
        terminal
            .draw(|frame| {
                render_mobile_panel(&app, &terminal_runtimes, frame, Rect::new(0, 0, 44, 20))
            })
            .unwrap();
        let expanded = buffer_text(terminal.backend().buffer());
        assert!(expanded.to_lowercase().contains("claude"));
        assert!(expanded.contains("working"));
        assert!(expanded.contains("agents"), "agents: {expanded:?}");
        assert!(!expanded.contains("switch"), "agents: {expanded:?}");
        assert!(!expanded.contains("GROUPS"), "agents: {expanded:?}");
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
                MobileSwitcherLevel::Workspaces { group_idx },
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

        assert!(dashboards < logs);
        assert!(logs + 1 < new_tab, "new tab must be outside the tab list");
        assert!(
            lines[new_tab - 1].contains('─'),
            "new tab must be separated from tab rows"
        );
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
        let active_tab = app
            .active
            .and_then(|ws_idx| app.workspaces.get(ws_idx))
            .map(crate::workspace::Workspace::active_tab_index);
        let focused_pane = app
            .active
            .and_then(|ws_idx| app.workspaces.get(ws_idx))
            .and_then(crate::workspace::Workspace::focused_pane_id);
        app.view.context_bar = super::super::compute_mobile_breadcrumb(
            &app,
            &runtime_registry,
            app.active,
            app.active_group,
            active_tab,
            focused_pane,
            crate::app::ClientTabControl::default(),
            Rect::new(0, 0, 40, 1),
        );
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

    #[test]
    fn mobile_header_carries_control_chip_right_aligned_for_watcher() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = crate::workspace::Workspace::test_new("ignored");
        workspace.custom_name = Some("website".into());
        workspace.tabs[0].custom_name = Some("release".into());
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;

        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut watcher = ClientViewState::from_default_client_state(&app);
        watcher.set_tab_control(crate::app::ClientTabControl::WatchingControlled { epoch: 5 });
        let mut controller = ClientViewState::from_default_client_state(&app);
        super::super::compute_view_for_client_without_resizing_panes(
            &app,
            &mut watcher,
            &terminal_runtimes,
            Rect::new(0, 0, 44, 20),
        );
        super::super::compute_view_for_client_without_resizing_panes(
            &app,
            &mut controller,
            &terminal_runtimes,
            Rect::new(0, 0, 44, 20),
        );

        let bar = &watcher.computed.context_bar;
        let chip = bar.segments.last().expect("watching chip segment");
        assert_eq!(chip.target, crate::app::state::ContextBarTarget::TabControl);
        assert_eq!(chip.label, " WATCHING ");
        assert_eq!(
            chip.rect.x + chip.rect.width + 1,
            bar.rect.x + bar.rect.width,
            "chip should be right-aligned with a one-column margin: {chip:?}"
        );
        for segment in &bar.segments[..bar.segments.len() - 1] {
            assert!(
                segment.rect.x + segment.rect.width <= chip.rect.x,
                "breadcrumb segment overlaps chip: {segment:?} vs {chip:?}"
            );
        }

        // The controller's mobile header carries no chip.
        assert!(controller
            .computed
            .context_bar
            .segments
            .iter()
            .all(|segment| segment.target != crate::app::state::ContextBarTarget::TabControl));

        let backend = ratatui::backend::TestBackend::new(44, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_mobile_header_for_view(
                    &app,
                    &terminal_runtimes,
                    &watcher,
                    frame,
                    watcher.computed.mobile_header_rect,
                )
            })
            .unwrap();
        let header_line = (0..44)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
            .collect::<String>();
        assert!(header_line.contains("WATCHING"), "{header_line:?}");
    }

    #[test]
    fn mobile_header_free_chip_and_tiny_rects() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = crate::workspace::Workspace::test_new("ignored");
        workspace.custom_name = Some("website".into());
        workspace.tabs[0].custom_name = Some("release".into());
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;

        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut watcher = ClientViewState::from_default_client_state(&app);
        watcher.set_tab_control(crate::app::ClientTabControl::WatchingFree { epoch: 2 });
        super::super::compute_view_for_client_without_resizing_panes(
            &app,
            &mut watcher,
            &terminal_runtimes,
            Rect::new(0, 0, 44, 20),
        );
        let bar = &watcher.computed.context_bar;
        let chip = bar.segments.last().expect("free chip segment");
        assert_eq!(chip.target, crate::app::state::ContextBarTarget::TabControl);
        assert_eq!(chip.label, " FREE ");

        let backend = ratatui::backend::TestBackend::new(44, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_mobile_header_for_view(
                    &app,
                    &terminal_runtimes,
                    &watcher,
                    frame,
                    watcher.computed.mobile_header_rect,
                )
            })
            .unwrap();
        let header_line = (0..44)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
            .collect::<String>();
        assert!(header_line.contains("FREE"), "{header_line:?}");

        // Tiny rectangles never break: the chip drops below badge width and
        // every surviving segment stays inside the header row.
        let mut tiny = ClientViewState::from_default_client_state(&app);
        tiny.set_tab_control(crate::app::ClientTabControl::WatchingControlled { epoch: 3 });
        for width in [8u16, 12, 20] {
            super::super::compute_view_for_client_without_resizing_panes(
                &app,
                &mut tiny,
                &terminal_runtimes,
                Rect::new(0, 0, width, 6),
            );
            let bar = &tiny.computed.context_bar;
            assert!(
                bar.segments.iter().all(|segment| {
                    segment.rect.width > 0
                        && segment.rect.x + segment.rect.width <= bar.rect.x + bar.rect.width
                }),
                "width {width}: {:?}",
                bar.segments
            );
            if width >= 12 {
                assert_eq!(
                    bar.segments.last().map(|segment| segment.target),
                    Some(crate::app::state::ContextBarTarget::TabControl),
                    "width {width} should fit the watching badge"
                );
            } else {
                assert!(
                    bar.segments
                        .iter()
                        .all(|segment| segment.target
                            != crate::app::state::ContextBarTarget::TabControl),
                    "width {width} cannot fit the watching badge"
                );
            }

            let backend = ratatui::backend::TestBackend::new(width, 6);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| {
                    render_mobile_header_for_view(
                        &app,
                        &terminal_runtimes,
                        &tiny,
                        frame,
                        tiny.computed.mobile_header_rect,
                    )
                })
                .unwrap();
        }
    }
}
