use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use super::status::{
    agent_section_icon, agent_section_style, state_icon, toast_kind_color, AgentStatusGroup,
};
use super::text::{display_width_u16, truncate_end};
use super::widgets::{fill_rect, panel_contrast_fg, render_panel_shell};
use crate::app::state::{
    AgentPanelScope, MobileSwitcherLevel, NavigatorRow, NavigatorTarget, Palette, ToastKind,
    ToastNotification,
};
use crate::app::{AppState, ClientViewState};
use crate::layout::PaneId;
use crate::terminal::TerminalRuntimeRegistry;

pub(crate) const MOBILE_HEADER_HEIGHT: u16 = 2;
pub(crate) const MOBILE_AGENT_PANEL_CHROME_HEIGHT: u16 = 3;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MobileSwitcherAreas {
    pub panel: Rect,
    pub agent_toggle: Rect,
    pub agent_scope: Rect,
    pub viewport: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileSwitcherTarget {
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
    AgentSection {
        group: AgentStatusGroup,
        count: usize,
    },
    Agent {
        label: String,
        meta: String,
        target: MobileSwitcherTarget,
    },
    AgentEmpty(&'static str),
    Empty(&'static str),
    Subheader(&'static str),
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
            Self::Empty(_)
            | Self::AgentEmpty(_)
            | Self::AgentSection { .. }
            | Self::Subheader(_)
            | Self::Divider => None,
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
            || super::sidebar::agent_panel_sections_from(app, terminal_runtimes),
            |view| super::sidebar::agent_panel_sections_for_view(app, terminal_runtimes, view),
        );
        let mut rows = Vec::new();
        for section in agent_sections {
            let empty_row = super::sidebar::agent_panel_empty_row(&section);
            if section.entries.is_empty() && empty_row.is_none() {
                continue;
            }
            rows.push(MobileNavigationRow::AgentSection {
                group: section.group,
                count: section.entries.len(),
            });
            if let Some(label) = empty_row {
                rows.push(MobileNavigationRow::AgentEmpty(label));
            }
            for entry in section.entries {
                let (label, meta) = super::sidebar::compact_agent_entry_text(&entry);
                rows.push(MobileNavigationRow::Agent {
                    label,
                    meta,
                    target: MobileSwitcherTarget::Agent {
                        ws_idx: entry.ws_idx,
                        tab_idx: entry.tab_idx,
                        pane_id: entry.pane_id,
                    },
                });
            }
        }
        if rows.is_empty() {
            rows.push(MobileNavigationRow::Empty("No Agents"));
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
            rows.push(MobileNavigationRow::Subheader("Groups"));
            rows.extend(hierarchy.into_iter().filter_map(|row| match row.target {
                NavigatorTarget::Group { group_idx } => Some(MobileNavigationRow::Hierarchy {
                    row,
                    target: MobileSwitcherTarget::Group(group_idx),
                }),
                _ => None,
            }));
            append_mobile_footer(
                &mut rows,
                "New",
                [("Group", MobileSwitcherTarget::NewGroup)],
            );
        }
        MobileSwitcherLevel::Workspaces { group_idx } => {
            rows.push(MobileNavigationRow::Subheader("Spaces"));
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
            if rows.len() == 1 {
                rows.push(MobileNavigationRow::Empty("No Spaces"));
            }
            append_mobile_footer(
                &mut rows,
                "New",
                [("Space", MobileSwitcherTarget::NewSpace { group_idx })],
            );
        }
        MobileSwitcherLevel::Tabs { ws_idx } => {
            rows.push(MobileNavigationRow::Subheader("Tabs"));
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
                rows.push(MobileNavigationRow::Empty("No Tabs"));
            }
            append_mobile_footer(
                &mut rows,
                "New",
                [("Tab", MobileSwitcherTarget::NewTab { ws_idx })],
            );
            let active_tab_idx = view
                .and_then(|view| view.active_tab_index_for_workspace(app, ws_idx))
                .or_else(|| {
                    let workspace = app.workspaces.get(ws_idx)?;
                    workspace
                        .tabs
                        .get(workspace.active_tab)
                        .map(|_| workspace.active_tab)
                });
            if let Some(tab_idx) = active_tab_idx.filter(|tab_idx| {
                app.workspaces
                    .get(ws_idx)
                    .and_then(|workspace| workspace.tabs.get(*tab_idx))
                    .is_some_and(|tab| tab.layout.pane_count() == 1)
            }) {
                append_mobile_split_actions(&mut rows, app, view, ws_idx, tab_idx);
            }
        }
        MobileSwitcherLevel::Panes { ws_idx, tab_idx } => {
            rows.push(MobileNavigationRow::Subheader("Panes"));
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
                rows.push(MobileNavigationRow::Empty("No Panes"));
            }
            append_mobile_split_actions(&mut rows, app, view, ws_idx, tab_idx);
        }
    }
    rows
}

fn append_mobile_footer<const N: usize>(
    rows: &mut Vec<MobileNavigationRow>,
    section: &'static str,
    actions: [(&'static str, MobileSwitcherTarget); N],
) {
    rows.push(MobileNavigationRow::Divider);
    rows.push(MobileNavigationRow::Subheader(section));
    rows.extend(
        actions
            .into_iter()
            .map(|(label, target)| MobileNavigationRow::Action { label, target }),
    );
}

fn append_mobile_split_actions(
    rows: &mut Vec<MobileNavigationRow>,
    app: &AppState,
    view: Option<&ClientViewState>,
    ws_idx: usize,
    tab_idx: usize,
) {
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
    if split_area.width < 2 && split_area.height < 2 {
        return;
    }

    rows.push(MobileNavigationRow::Divider);
    rows.push(MobileNavigationRow::Subheader("Split"));
    if split_area.width >= 2 {
        rows.push(MobileNavigationRow::Action {
            label: "Right",
            target: MobileSwitcherTarget::SplitRight,
        });
    }
    if split_area.height >= 2 {
        rows.push(MobileNavigationRow::Action {
            label: "Down",
            target: MobileSwitcherTarget::SplitDown,
        });
    }
}

pub(crate) fn is_mobile_width(area: Rect, threshold: u16) -> bool {
    area.width > 0 && area.width <= threshold
}

pub(crate) fn mobile_agent_strip_rect(header: Rect) -> Rect {
    if header.height < MOBILE_HEADER_HEIGHT {
        return Rect::default();
    }
    Rect::new(header.x, header.y, header.width, 1)
}

pub(crate) fn mobile_agent_scope_rect(area: Rect, scope: AgentPanelScope) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect::default();
    }
    let label_width = display_width_u16(super::sidebar::agent_panel_toggle_label(scope));
    let width = label_width.saturating_add(2).min(area.width);
    Rect::new(area.x + area.width.saturating_sub(width), area.y, width, 1)
}

pub(crate) fn mobile_switcher_areas(app: &AppState) -> MobileSwitcherAreas {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    let rows = mobile_navigation_rows(app, &terminal_runtimes, None);
    mobile_switcher_areas_for_rows(
        app,
        mobile_screen_rect(app),
        &rows,
        app.mobile_agents_expanded,
        app.agent_panel_scope,
        &app.view.context_bar,
        app.mobile_switcher_level,
    )
}

pub(crate) fn mobile_switcher_areas_for_view(
    app: &AppState,
    view: &ClientViewState,
) -> MobileSwitcherAreas {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    let rows = mobile_navigation_rows(app, &terminal_runtimes, Some(view));
    mobile_switcher_areas_for_rows(
        app,
        view.screen_rect(),
        &rows,
        view.mobile_agents_expanded,
        view.agent_panel_scope,
        &view.computed.context_bar,
        view.mobile_switcher_level,
    )
}

fn mobile_switcher_anchor(
    context_bar: &crate::app::state::ContextBarView,
    level: MobileSwitcherLevel,
) -> Rect {
    use crate::app::state::ContextBarTarget;

    let target = match level {
        MobileSwitcherLevel::Groups => ContextBarTarget::Group,
        MobileSwitcherLevel::Workspaces { .. } => ContextBarTarget::Workspace,
        MobileSwitcherLevel::Tabs { .. } => ContextBarTarget::Tab,
        MobileSwitcherLevel::Panes { .. } => ContextBarTarget::Pane,
    };
    context_bar
        .segments
        .iter()
        .find(|segment| segment.target == target)
        .map_or(context_bar.rect, |segment| segment.rect)
}

fn mobile_navigation_row_width(app: &AppState, row: &MobileNavigationRow) -> u16 {
    match row {
        MobileNavigationRow::Empty(label)
        | MobileNavigationRow::Subheader(label)
        | MobileNavigationRow::Action { label, .. } => display_width_u16(label).saturating_add(2),
        MobileNavigationRow::AgentEmpty(label) => display_width_u16(label).saturating_add(4),
        MobileNavigationRow::Hierarchy { row, .. } => {
            let (label, meta) = mobile_hierarchy_label_and_meta(app, row);
            display_width_u16(&label)
                .saturating_add(display_width_u16(&meta))
                .saturating_add(3 + u16::from(!meta.is_empty()))
        }
        MobileNavigationRow::AgentSection { group, count } => {
            let counter_width = if app.show_counters {
                count.to_string().chars().count() as u16 + 2
            } else {
                0
            };
            display_width_u16(group.label())
                .saturating_add(counter_width)
                .saturating_add(5)
        }
        MobileNavigationRow::Agent { label, meta, .. } => display_width_u16(label)
            .saturating_add(display_width_u16(meta))
            .saturating_add(6),
        MobileNavigationRow::Divider => 0,
    }
}

fn mobile_switcher_areas_for_rows(
    app: &AppState,
    screen: Rect,
    rows: &[MobileNavigationRow],
    agents_expanded: bool,
    agent_scope: AgentPanelScope,
    context_bar: &crate::app::state::ContextBarView,
    level: MobileSwitcherLevel,
) -> MobileSwitcherAreas {
    if screen.width == 0 || screen.height <= 1 {
        return MobileSwitcherAreas::default();
    }

    if !agents_expanded {
        let panel_y = screen.y.saturating_add(MOBILE_HEADER_HEIGHT);
        let available_height = screen
            .y
            .saturating_add(screen.height)
            .saturating_sub(panel_y);
        if screen.width < 3 || available_height < 3 {
            return MobileSwitcherAreas::default();
        }

        const MIN_CONTENT_WIDTH: u16 = 18;
        const MAX_CONTENT_WIDTH: u16 = 40;
        let content_width = rows
            .iter()
            .map(|row| mobile_navigation_row_width(app, row))
            .max()
            .unwrap_or(MIN_CONTENT_WIDTH)
            .clamp(MIN_CONTENT_WIDTH, MAX_CONTENT_WIDTH)
            .min(screen.width.saturating_sub(2));
        let panel_width = content_width.saturating_add(2).min(screen.width);
        let anchor = mobile_switcher_anchor(context_bar, level);
        let screen_right = screen.x.saturating_add(screen.width);
        let panel_x = anchor
            .x
            .max(screen.x)
            .min(screen_right.saturating_sub(panel_width));
        let viewport_height = (rows.len().max(1) as u16).min(available_height.saturating_sub(2));
        let panel = Rect::new(
            panel_x,
            panel_y,
            panel_width,
            viewport_height.saturating_add(2),
        );
        let viewport = Rect::new(
            panel.x.saturating_add(1),
            panel.y.saturating_add(1),
            panel.width.saturating_sub(2),
            panel.height.saturating_sub(2),
        );
        return MobileSwitcherAreas {
            panel,
            viewport,
            ..MobileSwitcherAreas::default()
        };
    }

    const TOP_CHROME_HEIGHT: u16 = 2;
    if screen.height <= MOBILE_AGENT_PANEL_CHROME_HEIGHT {
        return MobileSwitcherAreas::default();
    }
    let viewport_height =
        (rows.len().max(1) as u16).min(screen.height - MOBILE_AGENT_PANEL_CHROME_HEIGHT);
    let panel = Rect::new(
        screen.x,
        screen.y,
        screen.width,
        MOBILE_AGENT_PANEL_CHROME_HEIGHT + viewport_height,
    );
    let agent_toggle = Rect::new(panel.x, panel.y, panel.width, 1);
    let agent_scope = mobile_agent_scope_rect(agent_toggle, agent_scope);
    let viewport = Rect::new(
        panel.x,
        panel.y + TOP_CHROME_HEIGHT,
        panel.width,
        viewport_height,
    );

    MobileSwitcherAreas {
        panel,
        agent_toggle,
        agent_scope,
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

    render_mobile_agent_strip(
        app,
        terminal_runtimes,
        None,
        frame,
        mobile_agent_strip_rect(area),
    );
    super::render_context_bar(app, &app.view.context_bar, frame);
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

    render_mobile_agent_strip(
        app,
        terminal_runtimes,
        Some(view),
        frame,
        mobile_agent_strip_rect(area),
    );
    super::render_context_bar(app, &view.computed.context_bar, frame);
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
    let count = |group: AgentStatusGroup| {
        sections
            .iter()
            .find(|section| section.group == group)
            .map_or(0, |section| section.entries.len())
    };
    let expanded = view.map_or(app.mobile_agents_expanded, |view| {
        view.mobile_agents_expanded
    });
    let scope = view.map_or(app.agent_panel_scope, |view| view.agent_panel_scope);
    let scope_rect = if expanded {
        mobile_agent_scope_rect(area, scope)
    } else {
        Rect::default()
    };
    let summary_area = if scope_rect == Rect::default() {
        area
    } else {
        Rect::new(
            area.x,
            area.y,
            scope_rect.x.saturating_sub(area.x),
            area.height,
        )
    };
    render_mobile_agent_summary(
        app,
        frame,
        summary_area,
        summary_area,
        0,
        0,
        (
            count(AgentStatusGroup::Triage),
            count(AgentStatusGroup::Working),
            count(AgentStatusGroup::Idle),
        ),
        expanded,
        false,
    );
    if scope_rect != Rect::default() {
        let p = &app.palette;
        frame.render_widget(
            Paragraph::new(format!(
                " {} ",
                super::sidebar::agent_panel_toggle_label(scope)
            ))
            .style(Style::default().fg(p.overlay1).bg(p.surface0))
            .alignment(Alignment::Center),
            scope_rect,
        );
    }
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
    if app.mobile_agents_expanded {
        render_mobile_agent_strip(app, terminal_runtimes, None, frame, areas.agent_toggle);
    }
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
    if view.mobile_agents_expanded {
        render_mobile_agent_strip(
            app,
            terminal_runtimes,
            Some(view),
            frame,
            areas.agent_toggle,
        );
    }
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
    if !agents_expanded {
        let _ = render_panel_shell(frame, areas.panel, p.accent, p.panel_bg);
        return;
    }

    frame.render_widget(Clear, areas.panel);
    fill_rect(frame, areas.panel, Style::default().bg(p.panel_bg));

    draw_horizontal_rule(
        frame,
        Rect::new(areas.panel.x, areas.panel.y + 1, areas.panel.width, 1),
        p,
    );
    draw_horizontal_rule(
        frame,
        Rect::new(
            areas.panel.x,
            areas
                .panel
                .y
                .saturating_add(areas.panel.height.saturating_sub(1)),
            areas.panel.width,
            1,
        ),
        p,
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
            MobileNavigationRow::AgentEmpty(label) => {
                let Some(y) = visible_y(viewport, scroll, doc_y) else {
                    continue;
                };
                frame.render_widget(
                    Paragraph::new(format!("    {label}")).style(
                        Style::default()
                            .fg(p.overlay0)
                            .bg(p.panel_bg)
                            .add_modifier(Modifier::DIM),
                    ),
                    Rect::new(content.x, y, content.width, 1),
                );
            }
            MobileNavigationRow::Subheader(label) => {
                let Some(y) = visible_y(viewport, scroll, doc_y) else {
                    continue;
                };
                frame.render_widget(
                    Paragraph::new(format!(" {label}")).style(
                        Style::default()
                            .fg(p.overlay0)
                            .bg(p.panel_bg)
                            .add_modifier(Modifier::DIM),
                    ),
                    Rect::new(content.x, y, content.width, 1),
                );
            }
            MobileNavigationRow::AgentSection { group, count } => {
                render_mobile_agent_section(
                    app, frame, viewport, content, doc_y, scroll, *group, *count,
                );
            }
            MobileNavigationRow::Agent { label, meta, .. } => {
                render_mobile_agent_row(
                    app,
                    frame,
                    viewport,
                    content,
                    doc_y,
                    scroll,
                    label,
                    meta,
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
                let fg = if is_selected {
                    panel_contrast_fg(p)
                } else {
                    p.text
                };
                fill_rect(
                    frame,
                    Rect::new(content.x, y, content.width, 1),
                    Style::default().bg(bg),
                );
                frame.render_widget(
                    Paragraph::new(format!("  {label}")).style(
                        Style::default().fg(fg).bg(bg).add_modifier(if is_selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
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
    let selected_fg = panel_contrast_fg(p);
    fill_rect(
        frame,
        Rect::new(content.x, y, content.width, 1),
        Style::default().bg(bg),
    );
    let mut spans = vec![
        Span::styled(
            if expanded { " ▾ " } else { " ▸ " },
            Style::default()
                .fg(if selected { selected_fg } else { p.overlay1 })
                .bg(bg),
        ),
        Span::styled(
            "Agents",
            Style::default()
                .fg(if selected { selected_fg } else { p.text })
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if counts == (0, 0, 0) {
        spans.push(Span::styled(
            " No Agents",
            Style::default()
                .fg(if selected { selected_fg } else { p.overlay0 })
                .bg(bg)
                .add_modifier(if selected {
                    Modifier::empty()
                } else {
                    Modifier::DIM
                }),
        ));
    } else {
        for (group, count) in [
            (AgentStatusGroup::Triage, counts.0),
            (AgentStatusGroup::Working, counts.1),
            (AgentStatusGroup::Idle, counts.2),
        ] {
            if count == 0 {
                continue;
            }
            let label = group.label();
            let (icon, icon_style) =
                agent_section_icon(group, app.spinner_tick, app.status_indicators, p);
            spans.push(Span::styled(" ", Style::default().bg(bg)));
            let selected_style = Style::default()
                .fg(selected_fg)
                .bg(bg)
                .add_modifier(Modifier::BOLD);
            spans.push(Span::styled(
                icon,
                if selected {
                    selected_style
                } else {
                    icon_style.bg(bg)
                },
            ));
            spans.push(Span::styled(
                format!(" {count} {label}"),
                if selected {
                    selected_style
                } else {
                    agent_section_style(group, p).bg(bg)
                },
            ));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(content.x, y, content.width, 1),
    );
}

fn render_mobile_agent_section(
    app: &AppState,
    frame: &mut Frame,
    viewport: Rect,
    content: Rect,
    doc_y: usize,
    scroll: usize,
    group: AgentStatusGroup,
    count: usize,
) {
    let Some(y) = visible_y(viewport, scroll, doc_y) else {
        return;
    };
    let p = &app.palette;
    let dim = Style::default().fg(p.overlay0).bg(p.panel_bg);
    frame.render_widget(
        Paragraph::new(Span::styled("▾", dim)),
        Rect::new(content.x, y, content.width.min(1), 1),
    );
    if content.width > 2 {
        let (icon, icon_style) =
            agent_section_icon(group, app.spinner_tick, app.status_indicators, p);
        frame.render_widget(
            Paragraph::new(Span::styled(icon, icon_style.bg(p.panel_bg))),
            Rect::new(content.x + 2, y, 1, 1),
        );
    }

    let count_label = if app.show_counters {
        count.to_string()
    } else {
        String::new()
    };
    let count_width = display_width_u16(&count_label);
    let count_reserve = u16::from(app.show_counters).saturating_mul(count_width + 2);
    let label_width = content.width.saturating_sub(4 + count_reserve);
    if label_width > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                truncate_end(group.label(), label_width as usize),
                agent_section_style(group, p).bg(p.panel_bg),
            )),
            Rect::new(content.x + 4, y, label_width, 1),
        );
    }
    if app.show_counters && content.width > count_width + 1 {
        frame.render_widget(
            Paragraph::new(Span::styled(count_label, dim)),
            Rect::new(
                content.x + content.width.saturating_sub(count_width + 1),
                y,
                count_width,
                1,
            ),
        );
    }
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
    selected: bool,
) {
    let Some(y) = visible_y(viewport, scroll, doc_y) else {
        return;
    };
    let p = &app.palette;
    let bg = mobile_item_bg(selected, false, p);
    let selected_fg = panel_contrast_fg(p);
    fill_rect(
        frame,
        Rect::new(content.x, y, content.width, 1),
        Style::default().bg(bg),
    );
    let meta_width = if meta.is_empty() {
        0
    } else {
        display_width_u16(meta)
            .saturating_add(1)
            .min(content.width / 2)
    };
    let label_width = content.width.saturating_sub(meta_width);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("    ", Style::default().bg(bg)),
            Span::styled(
                truncate_end(label, label_width.saturating_sub(4) as usize),
                Style::default()
                    .fg(if selected { selected_fg } else { p.subtext0 })
                    .bg(bg)
                    .add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ])),
        Rect::new(content.x, y, label_width, 1),
    );
    if meta_width > 0 {
        frame.render_widget(
            Paragraph::new(format!("{meta} "))
                .style(
                    Style::default()
                        .fg(if selected { selected_fg } else { p.overlay0 })
                        .bg(bg)
                        .add_modifier(if selected {
                            Modifier::empty()
                        } else {
                            Modifier::DIM
                        }),
                )
                .alignment(Alignment::Right),
            Rect::new(content.x + label_width, y, meta_width, 1),
        );
    }
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
    let selected_fg = panel_contrast_fg(p);
    let (marker, marker_style) = match row.target {
        _ if selected => (" ".to_string(), Style::default().fg(selected_fg).bg(bg)),
        NavigatorTarget::Pane { .. } => {
            let (dot, style) = state_icon(
                row.status,
                row.seen,
                app.spinner_tick,
                app.status_indicators,
                p,
            );
            (dot.to_string(), style.bg(bg))
        }
        NavigatorTarget::Group { .. } => (" ".to_string(), Style::default().bg(bg)),
        _ if row.is_current => ("●".to_string(), Style::default().fg(accent).bg(bg)),
        _ => (" ".to_string(), Style::default().bg(bg)),
    };
    let label_style = if selected {
        Style::default()
            .fg(selected_fg)
            .bg(bg)
            .add_modifier(Modifier::BOLD)
    } else if row.is_current {
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
        Span::styled(marker, marker_style),
        Span::styled(" ", Style::default().bg(bg)),
    ];
    let label_room = label_width.saturating_sub(2);
    if let NavigatorTarget::Group { group_idx } = row.target {
        if let Some(group) = app.groups.get(group_idx) {
            let icon_width = super::text::display_width_u16(&group.icon);
            spans.push(Span::styled(
                group.icon.clone(),
                Style::default()
                    .fg(if selected { selected_fg } else { accent })
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
                .style(
                    Style::default()
                        .fg(if selected { selected_fg } else { p.overlay0 })
                        .bg(bg),
                )
                .alignment(Alignment::Right),
            Rect::new(content.x + label_width, y, meta_width, 1),
        );
    }
}

fn mobile_hierarchy_label_and_meta(app: &AppState, row: &NavigatorRow) -> (String, String) {
    match row.target {
        NavigatorTarget::Group { group_idx } => {
            let count = app.show_counters.then(|| {
                app.groups
                    .get(group_idx)
                    .map(|group| {
                        app.workspaces
                            .iter()
                            .filter(|workspace| workspace.group_id == group.id)
                            .count()
                    })
                    .unwrap_or(0)
                    .to_string()
            });
            (row.label.clone(), count.unwrap_or_default())
        }
        NavigatorTarget::Workspace { ws_idx } => {
            let count = app.show_counters.then(|| {
                app.workspaces
                    .get(ws_idx)
                    .map(|workspace| workspace.tabs.len())
                    .unwrap_or(0)
                    .to_string()
            });
            (
                strip_trailing_count(&row.label).to_string(),
                count.unwrap_or_default(),
            )
        }
        NavigatorTarget::Tab { ws_idx, tab_idx } => {
            let count = app.show_counters.then(|| {
                app.workspaces
                    .get(ws_idx)
                    .and_then(|workspace| workspace.tabs.get(tab_idx))
                    .map(|tab| tab.panes.len())
                    .unwrap_or(0)
                    .to_string()
            });
            (row.label.clone(), count.unwrap_or_default())
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

fn visible_y(viewport: Rect, scroll: usize, doc_y: usize) -> Option<u16> {
    let offset = doc_y.checked_sub(scroll)?;
    (offset < viewport.height as usize).then_some(viewport.y + offset as u16)
}

fn mobile_item_bg(selected: bool, _active: bool, p: &Palette) -> ratatui::style::Color {
    if selected {
        p.accent
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
            .strip_suffix(" Needs Attention")
            .map(|agent| format!("{agent} Waiting"))
            .unwrap_or_else(|| toast.title.clone()),
        ToastKind::Finished => toast
            .title
            .strip_suffix(" Finished")
            .map(|agent| format!("{agent} Done"))
            .unwrap_or_else(|| toast.title.clone()),
        ToastKind::UpdateInstalled => "Update Ready".to_string(),
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
            .set_style(Style::default().fg(p.overlay0).bg(p.panel_bg));
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

        let default_space = crate::workspace::Workspace::test_new("Default");
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
            .set_detected_state(
                Some(crate::detect::Agent::Claude),
                crate::detect::AgentState::Working,
            );
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
                    if row.label.eq_ignore_ascii_case("claude") && row.meta.contains("Working")
            )
        }));
        assert!(!pane_rows
            .iter()
            .any(|row| matches!(row.target(), Some(MobileSwitcherTarget::Tab { .. }))));
    }

    #[test]
    fn attached_mobile_workspace_level_lists_spaces_from_the_selected_group() {
        let (app, group_idx, _) = hierarchy_fixture();
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut view = ClientViewState::from_default_client_state(&app);
        view.active_workspace = Some(0);
        view.active_group = 0;
        view.mobile_switcher_level = MobileSwitcherLevel::Workspaces { group_idx };

        let workspace_targets = mobile_navigation_rows(&app, &terminal_runtimes, Some(&view))
            .iter()
            .filter_map(MobileNavigationRow::target)
            .collect::<Vec<_>>();

        assert!(workspace_targets.contains(&MobileSwitcherTarget::Workspace(1)));
        assert!(workspace_targets.contains(&MobileSwitcherTarget::Workspace(2)));
    }

    #[test]
    fn split_actions_follow_pane_count_and_available_geometry() {
        let (mut app, _, _) = hierarchy_fixture();
        let terminal_runtimes = TerminalRuntimeRegistry::new();

        app.workspaces[1].active_tab = 1;
        app.mobile_switcher_level = MobileSwitcherLevel::Tabs { ws_idx: 1 };
        app.view.terminal_area = Rect::new(0, 1, 2, 1);
        let targets = mobile_navigation_rows(&app, &terminal_runtimes, None)
            .iter()
            .filter_map(MobileNavigationRow::target)
            .collect::<Vec<_>>();
        assert!(targets.contains(&MobileSwitcherTarget::SplitRight));
        assert!(!targets.contains(&MobileSwitcherTarget::SplitDown));

        app.workspaces[1].active_tab = 0;
        let targets = mobile_navigation_rows(&app, &terminal_runtimes, None)
            .iter()
            .filter_map(MobileNavigationRow::target)
            .collect::<Vec<_>>();
        assert!(!targets.contains(&MobileSwitcherTarget::SplitRight));

        app.mobile_switcher_level = MobileSwitcherLevel::Panes {
            ws_idx: 1,
            tab_idx: 0,
        };
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
    fn mobile_header_keeps_agent_summary_above_context() {
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
            Rect::new(0, 1, 44, 1),
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
        assert!(!text.contains("No Agents"), "header: {text:?}");
        assert!(text.contains("Agents"), "header: {text:?}");
        assert!(text.contains("1 Working"), "header: {text:?}");
        assert!(!text.contains("Triage"), "header: {text:?}");
        assert!(!text.contains("Idle"), "header: {text:?}");
        let working_x = (0..44)
            .find(|x| buffer[(*x, 0)].symbol() == "W")
            .expect("working label");
        assert_eq!(
            buffer[(working_x, 0)].style().fg,
            agent_section_style(AgentStatusGroup::Working, &app.palette).fg
        );
        assert!(buffer[(working_x, 0)]
            .style()
            .add_modifier
            .contains(Modifier::BOLD));
    }

    #[test]
    fn mobile_header_keeps_empty_agent_row_above_breadcrumbs() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![crate::workspace::Workspace::test_new("personal")];
        app.active = Some(0);
        app.selected = 0;
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        super::super::compute_view(&mut app, Rect::new(0, 0, 44, 20));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(44, 2)).unwrap();

        terminal
            .draw(|frame| {
                render_mobile_header(&app, &terminal_runtimes, frame, Rect::new(0, 0, 44, 2))
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let first_row = (0..44).map(|x| buffer[(x, 0)].symbol()).collect::<String>();
        let second_row = (0..44).map(|x| buffer[(x, 1)].symbol()).collect::<String>();
        assert!(
            first_row.contains("Agents No Agents"),
            "agents: {first_row:?}"
        );
        assert!(
            second_row.contains("personal"),
            "breadcrumbs: {second_row:?}"
        );
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
        for (group, count) in [
            (AgentStatusGroup::Triage, 2),
            (AgentStatusGroup::Working, 1),
            (AgentStatusGroup::Idle, 3),
        ] {
            let label = group.label();
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
            assert_eq!(style.fg, agent_section_style(group, &app.palette).fg);
            assert!(style.add_modifier.contains(Modifier::BOLD));
        }
        assert!(text.contains("● 2 Triage"), "summary: {text:?}");
        assert!(text.contains("○ 3 Idle"), "summary: {text:?}");
    }

    #[test]
    fn mobile_agent_section_counter_follows_global_setting() {
        let mut app = AppState::test_new();
        let area = Rect::new(0, 0, 20, 1);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(20, 1)).unwrap();

        terminal
            .draw(|frame| {
                render_mobile_agent_section(
                    &app,
                    frame,
                    area,
                    area,
                    0,
                    0,
                    AgentStatusGroup::Working,
                    3,
                )
            })
            .unwrap();
        assert_eq!(terminal.backend().buffer()[(18, 0)].symbol(), " ");

        app.show_counters = true;
        terminal
            .draw(|frame| {
                render_mobile_agent_section(
                    &app,
                    frame,
                    area,
                    area,
                    0,
                    0,
                    AgentStatusGroup::Working,
                    3,
                )
            })
            .unwrap();
        assert_eq!(terminal.backend().buffer()[(18, 0)].symbol(), "3");
    }

    #[test]
    fn mobile_group_dropdown_uses_compact_rows_counts_and_visible_separator() {
        let (mut app, active_group, _) = hierarchy_fixture();
        app.groups[0].icon = "✚".to_string();
        app.groups[0].accent = Some(crate::config::TerminalAccent::Green);
        app.view.mobile_header_rect = Rect::new(0, 0, 44, 2);
        app.view.terminal_area = Rect::new(0, 2, 44, 18);
        app.mobile_switcher_level = MobileSwitcherLevel::Groups;
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let rows = mobile_navigation_rows(&app, &terminal_runtimes, None);
        let group_row = rows
            .iter()
            .find_map(|row| match row {
                MobileNavigationRow::Hierarchy { row, .. } => Some(row),
                _ => None,
            })
            .expect("group row");
        assert!(mobile_hierarchy_label_and_meta(&app, group_row)
            .1
            .is_empty());

        app.show_counters = true;
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(44, 20)).unwrap();

        terminal
            .draw(|frame| {
                render_mobile_panel(&app, &terminal_runtimes, frame, Rect::new(0, 0, 44, 20))
            })
            .unwrap();

        let areas = mobile_switcher_areas(&app);

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
        let content = inset_for_left_scrollbar(areas.viewport);

        assert_eq!(areas.panel.y, 2);
        assert!(areas.panel.width < 44);
        assert_eq!(buffer[(areas.panel.x, areas.panel.y)].symbol(), "┌");
        assert_eq!(
            buffer[(areas.panel.x + areas.panel.width - 1, areas.panel.y)].symbol(),
            "┐"
        );
        let subheader = (areas.viewport.x + 2, areas.viewport.y);
        assert_eq!(buffer[subheader].symbol(), "G");
        assert_eq!(buffer[subheader].style().fg, Some(app.palette.overlay0));
        assert!(buffer[subheader]
            .style()
            .add_modifier
            .contains(Modifier::DIM));

        assert_eq!(buffer[default_icon].style().bg, Some(app.palette.accent));
        assert_eq!(
            buffer[default_icon].style().fg,
            Some(panel_contrast_fg(&app.palette))
        );
        assert_eq!(
            buffer[active_icon].style().fg,
            Some(app.group_accent_color(active_group))
        );
        assert_eq!(default_icon.0, content.x + 2);
        assert_eq!(active_icon.0, content.x + 2);

        let active_row = (content.x..content.x + content.width)
            .map(|x| buffer[(x, active_icon.1)].symbol())
            .collect::<String>();
        assert!(!active_row.contains("Spaces"), "group row: {active_row:?}");
        assert!(
            active_row.trim_end().ends_with('2'),
            "group row: {active_row:?}"
        );

        let separator_y = (areas.viewport.y..areas.viewport.y + areas.viewport.height)
            .find(|y| {
                (content.x + 1..content.x + content.width.saturating_sub(1))
                    .all(|x| buffer[(x, *y)].symbol() == "─")
            })
            .expect("horizontal section separator");
        assert_eq!(
            buffer[(content.x + 1, separator_y)].style().fg,
            Some(app.palette.overlay0)
        );
    }

    #[test]
    fn mobile_agent_dropdown_matches_sidebar_hierarchy_in_one_row() {
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
        assert!(!collapsed.contains("Agents"), "groups: {collapsed:?}");

        let other_pane = app.workspaces[0].tabs[0].root_pane;
        let other_terminal = app.workspaces[0].tabs[0].panes[&other_pane]
            .attached_terminal_id
            .clone();
        app.terminals
            .get_mut(&other_terminal)
            .expect("other agent terminal")
            .set_detected_state(
                Some(crate::detect::Agent::Codex),
                crate::detect::AgentState::Working,
            );

        app.mobile_agents_expanded = true;
        let rows = mobile_navigation_rows(&app, &terminal_runtimes, None);
        assert!(!rows.iter().any(|row| {
            matches!(
                row.target(),
                Some(MobileSwitcherTarget::Agent { ws_idx: 0, .. })
            )
        }));
        app.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        let all_rows = mobile_navigation_rows(&app, &terminal_runtimes, None);
        assert!(all_rows.iter().any(|row| {
            matches!(
                row.target(),
                Some(MobileSwitcherTarget::Agent { ws_idx: 0, .. })
            )
        }));
        app.agent_panel_scope = AgentPanelScope::CurrentWorkspace;
        let (agent_label, agent_meta) = match rows.as_slice() {
            [MobileNavigationRow::AgentSection {
                group: AgentStatusGroup::FollowUp,
                count: 0,
            }, MobileNavigationRow::AgentEmpty("Drop an agent here"), MobileNavigationRow::AgentSection { group, count }, MobileNavigationRow::Agent {
                label: agent_label,
                meta,
                target:
                    MobileSwitcherTarget::Agent {
                        ws_idx: 1,
                        tab_idx: 0,
                        pane_id,
                    },
            }] => {
                assert_eq!(*group, AgentStatusGroup::Working);
                assert_eq!(*count, 1);
                assert_eq!(*pane_id, focused_pane);
                (agent_label.clone(), meta.clone())
            }
            other => panic!("unexpected agent hierarchy: {other:?}"),
        };
        assert!(agent_meta.contains("claude"), "metadata: {agent_meta:?}");

        terminal
            .draw(|frame| {
                render_mobile_panel(&app, &terminal_runtimes, frame, Rect::new(0, 0, 44, 20))
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let expanded = buffer_text(buffer);
        assert_eq!(
            expanded.matches("Agents").count(),
            1,
            "agents: {expanded:?}"
        );
        assert!(!expanded.contains("All Agents"), "agents: {expanded:?}");

        let areas = mobile_switcher_areas(&app);
        assert_eq!(areas.agent_toggle, Rect::new(0, 0, 44, 1));
        assert_eq!(
            buffer[(areas.agent_toggle.x + 1, areas.agent_toggle.y)].symbol(),
            "▾"
        );
        assert_eq!(
            buffer[(areas.agent_toggle.x + 3, areas.agent_toggle.y)].symbol(),
            "A"
        );
        let scope_label = (areas.agent_scope.x..areas.agent_scope.x + areas.agent_scope.width)
            .map(|x| buffer[(x, areas.agent_scope.y)].symbol())
            .collect::<String>();
        assert_eq!(scope_label.trim(), "Space");
        let separator_y = areas.panel.y + 1;
        assert!(
            (areas.panel.x..areas.panel.x + areas.panel.width)
                .all(|x| buffer[(x, separator_y)].symbol() == "─"),
            "expanded agents separator must span the panel"
        );
        let bottom_separator_y = areas.panel.y + areas.panel.height - 1;
        assert_eq!(
            bottom_separator_y,
            areas.viewport.y + areas.viewport.height,
            "bottom separator must occupy its own row after the final agent"
        );
        assert!(
            (areas.panel.x..areas.panel.x + areas.panel.width)
                .all(|x| buffer[(x, bottom_separator_y)].symbol() == "─"),
            "bottom separator must span the panel"
        );
        assert_eq!(
            buffer[(areas.panel.x, bottom_separator_y)].style().fg,
            Some(app.palette.overlay0)
        );
        let content = inset_for_left_scrollbar(areas.viewport);
        let section_y = areas.viewport.y + 2;
        let agent_y = section_y + 1;
        let (section_icon, section_icon_style) = agent_section_icon(
            AgentStatusGroup::Working,
            app.spinner_tick,
            app.status_indicators,
            &app.palette,
        );
        assert_eq!(buffer[(content.x, section_y)].symbol(), "▾");
        assert_eq!(buffer[(content.x + 2, section_y)].symbol(), section_icon);
        assert_eq!(
            buffer[(content.x + 2, section_y)].style().fg,
            section_icon_style.fg,
            "expanded agents panel: {expanded:?}"
        );
        assert_eq!(buffer[(content.x + 4, section_y)].symbol(), "W");
        assert_eq!(
            buffer[(content.x + 4, section_y)].style().fg,
            agent_section_style(AgentStatusGroup::Working, &app.palette).fg
        );

        assert_eq!(buffer[(content.x + 2, agent_y)].symbol(), " ");
        assert_eq!(buffer[(content.x + 3, agent_y)].symbol(), " ");
        assert_eq!(
            buffer[(content.x + 4, agent_y)].symbol(),
            agent_label.chars().next().unwrap().to_string()
        );
        let agent_line = (content.x..content.x + content.width)
            .map(|x| buffer[(x, agent_y)].symbol())
            .collect::<String>();
        assert!(
            agent_line.contains(&agent_label),
            "agent row: {agent_line:?}"
        );
        assert!(
            agent_line.contains(&agent_meta),
            "agent row: {agent_line:?}"
        );
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
    fn attached_mobile_workspace_level_renders_empty_group_state() {
        let mut app = crate::app::state::AppState::test_new();
        let group_idx = app.create_group("Empty".to_string());
        app.view.mobile_header_rect = Rect::new(0, 0, 44, 1);
        app.view.terminal_area = Rect::new(0, 1, 44, 19);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut view = ClientViewState::from_default_client_state(&app);
        view.computed.mobile_header_rect = app.view.mobile_header_rect;
        view.computed.terminal_area = app.view.terminal_area;
        view.mobile_switcher_level = MobileSwitcherLevel::Workspaces { group_idx };
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(44, 20)).unwrap();

        terminal
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

        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("No Spaces"), "workspace menu: {text:?}");
        assert!(text.contains("New"), "workspace menu: {text:?}");
        assert!(text.contains("Space"), "workspace menu: {text:?}");
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
        let new_heading = lines
            .iter()
            .position(|line| line.contains("  New"))
            .expect("New section");
        let new_tab = lines
            .iter()
            .position(|line| line.contains("  Tab") && !line.contains("Tabs"))
            .expect("Tab action");

        assert!(dashboards < logs);
        assert!(logs + 1 < new_heading, "new must be outside the tab list");
        assert!(
            lines[new_heading - 1].contains('─'),
            "new must be separated from tab rows"
        );
        assert!(
            new_heading < new_tab,
            "tab action must follow new subheader"
        );
    }

    #[tokio::test]
    async fn mobile_header_uses_live_root_runtime_cwd_for_workspace_label() {
        let unique = format!(
            "gardn-mobile-header-runtime-cwd-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let stale_cwd = root.join("issue-264-nix-support");
        let live_cwd = root.join("Gardn");
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
            std::sync::Arc::new(crate::render_signal::RenderSignal::new()),
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

        assert!(row.contains("Gardn"), "header row: {row:?}");
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
        assert_eq!(chip.label, " Watching ");
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
            .map(|x| terminal.backend().buffer()[(x, bar.rect.y)].symbol())
            .collect::<String>();
        assert!(header_line.contains("Watching"), "{header_line:?}");
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
        assert_eq!(chip.label, " Free ");

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
            .map(|x| terminal.backend().buffer()[(x, bar.rect.y)].symbol())
            .collect::<String>();
        assert!(header_line.contains("Free"), "{header_line:?}");

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

    #[test]
    fn expanded_mobile_follow_up_renders_muted_indented_non_target_row() {
        let mut app = AppState::test_new();
        app.workspaces = vec![crate::workspace::Workspace::test_new("plain")];
        app.active = Some(0);
        app.selected = 0;
        app.mobile_agents_expanded = true;
        let area = Rect::new(0, 0, 44, 20);
        super::super::compute_view(&mut app, area);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(44, 20)).unwrap();

        terminal
            .draw(|frame| render_mobile_panel(&app, &terminal_runtimes, frame, area))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let (empty_x, empty_y) = (0..area.height)
            .find_map(|y| {
                let line = (0..area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>();
                line.find("Drop an agent here").map(|x| (x as u16, y))
            })
            .expect("mobile Follow Up empty row");
        let line = (0..area.width)
            .map(|x| buffer[(x, empty_y)].symbol())
            .collect::<String>();
        assert!(line[..empty_x as usize].ends_with("    "), "{line:?}");
        assert_eq!(
            buffer[(empty_x, empty_y)].style().fg,
            Some(app.palette.overlay0)
        );
        assert!(buffer[(empty_x, empty_y)]
            .style()
            .add_modifier
            .contains(Modifier::DIM));
        assert!(mobile_switcher_target_at(&app, empty_x, empty_y).is_none());
    }

    #[test]
    fn mobile_follow_up_empty_row_is_absent_when_panel_is_collapsed_or_queued() {
        let area = Rect::new(0, 0, 44, 20);
        let terminal_runtimes = TerminalRuntimeRegistry::new();

        let mut collapsed = AppState::test_new();
        collapsed.workspaces = vec![crate::workspace::Workspace::test_new("plain")];
        collapsed.active = Some(0);
        collapsed.selected = 0;
        super::super::compute_view(&mut collapsed, area);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(44, 20)).unwrap();
        terminal
            .draw(|frame| render_mobile_panel(&collapsed, &terminal_runtimes, frame, area))
            .unwrap();
        assert!(!buffer_text(terminal.backend().buffer()).contains("Drop an agent here"));

        let mut queued = AppState::test_new();
        let mut workspace = crate::workspace::Workspace::test_new("queued");
        let pane = workspace.tabs[0].root_pane;
        let pane_state = workspace.tabs[0].panes.get_mut(&pane).unwrap();
        pane_state.detected_agent = Some(crate::detect::Agent::Codex);
        pane_state.state = crate::detect::AgentState::Working;
        queued.workspaces = vec![workspace];
        queued.active = Some(0);
        queued.selected = 0;
        queued.mobile_agents_expanded = true;
        assert!(queued.insert_agent_follow_up(0, pane));
        super::super::compute_view(&mut queued, area);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(44, 20)).unwrap();
        terminal
            .draw(|frame| render_mobile_panel(&queued, &terminal_runtimes, frame, area))
            .unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("queued"), "{text:?}");
        assert!(!text.contains("Drop an agent here"), "{text:?}");
    }
}
