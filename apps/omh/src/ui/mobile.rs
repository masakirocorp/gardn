use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use super::status::{state_dot, toast_kind_color};
use super::text::truncate_end;
use super::widgets::fill_rect;
use crate::app::state::{NavigatorRow, NavigatorTarget, Palette, ToastKind, ToastNotification};
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
    pub close: Rect,
    pub viewport: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileSwitcherTarget {
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
    Menu(usize),
}

#[derive(Debug, PartialEq, Eq)]
enum MobileNavigationRow {
    Section(&'static str),
    Action {
        label: &'static str,
        target: MobileSwitcherTarget,
        depth: u8,
        group_idx: Option<usize>,
    },
    Hierarchy(NavigatorRow),
    Menu {
        label: String,
        target: MobileSwitcherTarget,
    },
}

impl MobileNavigationRow {
    fn target(&self) -> Option<MobileSwitcherTarget> {
        match self {
            Self::Section(_) => None,
            Self::Action { target, .. } | Self::Menu { target, .. } => Some(*target),
            Self::Hierarchy(row) => Some(match row.target {
                NavigatorTarget::Group { group_idx } => MobileSwitcherTarget::Group(group_idx),
                NavigatorTarget::Workspace { ws_idx } => MobileSwitcherTarget::Workspace(ws_idx),
                NavigatorTarget::Tab { ws_idx, tab_idx } => {
                    MobileSwitcherTarget::Tab { ws_idx, tab_idx }
                }
                NavigatorTarget::Pane {
                    ws_idx,
                    tab_idx,
                    pane_id,
                } => MobileSwitcherTarget::Pane {
                    ws_idx,
                    tab_idx,
                    pane_id,
                },
            }),
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
    let active_workspace = view.map_or(app.active, |view| view.active_workspace);
    let active_group = view.map_or(app.active_group, |view| view.active_group);
    let mut rows = Vec::with_capacity(
        hierarchy.len() + app.global_menu_labels().len() + usize::from(!app.groups.is_empty()) + 3,
    );
    rows.push(MobileNavigationRow::Section("navigate"));
    for row in hierarchy {
        let group_target = matches!(
            row.target,
            NavigatorTarget::Group { group_idx } if group_idx == active_group
        );
        let workspace_target = matches!(
            row.target,
            NavigatorTarget::Workspace { ws_idx } if Some(ws_idx) == active_workspace
        );
        rows.push(MobileNavigationRow::Hierarchy(row));
        if group_target {
            rows.push(MobileNavigationRow::Action {
                label: "+ new space",
                target: MobileSwitcherTarget::NewSpace,
                depth: 1,
                group_idx: Some(active_group),
            });
        }
        if workspace_target {
            let group_idx = active_workspace
                .and_then(|ws_idx| app.workspaces.get(ws_idx))
                .and_then(|workspace| app.group_index_by_id(&workspace.group_id));
            rows.push(MobileNavigationRow::Action {
                label: "+ new tab",
                target: MobileSwitcherTarget::NewTab,
                depth: 2,
                group_idx,
            });
        }
    }
    rows.push(MobileNavigationRow::Section("menu"));
    rows.extend(
        app.global_menu_labels()
            .into_iter()
            .enumerate()
            .map(|(idx, label)| MobileNavigationRow::Menu {
                label: label.to_string(),
                target: MobileSwitcherTarget::Menu(idx),
            }),
    );
    rows
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
    let screen = mobile_screen_rect(app);
    if screen.width == 0 || screen.height <= 2 {
        return MobileSwitcherAreas::default();
    }

    let header_h = screen.height.min(2);
    let close_w = 10u16.min(screen.width);
    let close = Rect::new(
        screen.x + screen.width.saturating_sub(close_w),
        screen.y,
        close_w,
        header_h,
    );
    let viewport = Rect::new(
        screen.x,
        screen.y + header_h + 1,
        screen.width,
        screen.height.saturating_sub(header_h + 1),
    );

    MobileSwitcherAreas { close, viewport }
}

pub(crate) fn mobile_switcher_areas_for_view(view: &ClientViewState) -> MobileSwitcherAreas {
    let screen = view.screen_rect();
    if screen.width == 0 || screen.height <= 2 {
        return MobileSwitcherAreas::default();
    }

    let header_h = screen.height.min(2);
    let close_w = 10u16.min(screen.width);
    let close = Rect::new(
        screen.x + screen.width.saturating_sub(close_w),
        screen.y,
        close_w,
        header_h,
    );
    let viewport = Rect::new(
        screen.x,
        screen.y + header_h + 1,
        screen.width,
        screen.height.saturating_sub(header_h + 1),
    );

    MobileSwitcherAreas { close, viewport }
}

pub(crate) fn mobile_switcher_max_scroll_for_height(app: &AppState, viewport_height: u16) -> usize {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    mobile_navigation_rows(app, &terminal_runtimes, None)
        .len()
        .saturating_sub(viewport_height as usize)
}

pub(crate) fn mobile_switcher_workspace_doc_range(
    app: &AppState,
    idx: usize,
) -> std::ops::Range<usize> {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    let start = mobile_navigation_rows(app, &terminal_runtimes, None)
        .iter()
        .position(|row| row.target() == Some(MobileSwitcherTarget::Workspace(idx)))
        .unwrap_or(idx);
    start..start + 1
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
        .saturating_sub(mobile_switcher_areas_for_view(view).viewport.height as usize)
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

pub(crate) fn mobile_switcher_target_at(
    app: &AppState,
    col: u16,
    row: u16,
) -> Option<MobileSwitcherTarget> {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    let rows = mobile_navigation_rows(app, &terminal_runtimes, None);
    mobile_switcher_target_from_rows(
        &rows,
        app.mobile_switcher_scroll,
        mobile_switcher_areas(app).viewport,
        col,
        row,
    )
}

pub(crate) fn mobile_switcher_target_at_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    col: u16,
    row: u16,
) -> Option<MobileSwitcherTarget> {
    let rows = mobile_navigation_rows(app, terminal_runtimes, Some(view));
    mobile_switcher_target_from_rows(
        &rows,
        view.mobile_switcher_scroll,
        mobile_switcher_areas_for_view(view).viewport,
        col,
        row,
    )
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
    area: Rect,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let p = &app.palette;
    frame.render_widget(Clear, area);
    fill_rect(frame, area, Style::default().bg(p.panel_bg));

    let areas = mobile_switcher_areas(app);
    frame.render_widget(
        Paragraph::new(" switch").style(
            Style::default()
                .fg(p.text)
                .bg(p.panel_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(area.x, area.y, areas.close.x.saturating_sub(area.x), 1),
    );
    render_close_button(app, frame, areas.close);

    if area.height > areas.close.height {
        draw_horizontal_rule(
            frame,
            Rect::new(area.x, area.y + areas.close.height, area.width, 1),
            p,
        );
    }

    render_mobile_switcher_content(app, terminal_runtimes, frame, areas.viewport);
}

pub(crate) fn render_mobile_panel_for_view(
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
    frame.render_widget(Clear, area);
    fill_rect(frame, area, Style::default().bg(p.panel_bg));

    let areas = mobile_switcher_areas_for_view(view);
    frame.render_widget(
        Paragraph::new(" switch").style(
            Style::default()
                .fg(p.text)
                .bg(p.panel_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(area.x, area.y, areas.close.x.saturating_sub(area.x), 1),
    );
    render_close_button(app, frame, areas.close);

    if area.height > areas.close.height {
        draw_horizontal_rule(
            frame,
            Rect::new(area.x, area.y + areas.close.height, area.width, 1),
            p,
        );
    }

    render_mobile_switcher_content_for_view(app, terminal_runtimes, view, frame, areas.viewport);
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
    fill_rect(frame, area, Style::default().bg(p.surface0));
    for y in area.y..area.y + area.height {
        frame.buffer_mut()[(area.x, y)]
            .set_symbol("│")
            .set_style(Style::default().fg(p.surface_dim).bg(p.surface0));
    }
    frame.render_widget(
        Paragraph::new("close")
            .style(
                Style::default()
                    .fg(p.overlay1)
                    .bg(p.surface0)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center),
        Rect::new(area.x + 1, area.y, area.width.saturating_sub(1), 1),
    );
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new("×")
                .style(
                    Style::default()
                        .fg(p.text)
                        .bg(p.surface0)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center),
            Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(1), 1),
        );
    }
}

fn render_mobile_switcher_content(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    viewport: Rect,
) {
    let rows = mobile_navigation_rows(app, terminal_runtimes, None);
    render_mobile_navigation_rows(app, frame, viewport, &rows, app.mobile_switcher_scroll);
}

fn render_mobile_switcher_content_for_view(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
    frame: &mut Frame,
    viewport: Rect,
) {
    let rows = mobile_navigation_rows(app, terminal_runtimes, Some(view));
    render_mobile_navigation_rows(app, frame, viewport, &rows, view.mobile_switcher_scroll);
}

fn render_mobile_navigation_rows(
    app: &AppState,
    frame: &mut Frame,
    viewport: Rect,
    rows: &[MobileNavigationRow],
    scroll: usize,
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
    for (doc_y, row) in rows.iter().enumerate() {
        match row {
            MobileNavigationRow::Section(title) => {
                render_section_title_at(frame, viewport, content, doc_y, scroll, title, p);
            }
            MobileNavigationRow::Action {
                label,
                depth,
                group_idx,
                ..
            } => {
                let Some(y) = visible_y(viewport, scroll, doc_y) else {
                    continue;
                };
                let accent = group_idx
                    .map(|idx| app.group_accent_color(idx))
                    .unwrap_or(p.accent);
                frame.render_widget(
                    Paragraph::new(format!("{}{}", "  ".repeat(*depth as usize + 1), label)).style(
                        Style::default()
                            .fg(accent)
                            .bg(p.panel_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Rect::new(content.x, y, content.width, 1),
                );
            }
            MobileNavigationRow::Hierarchy(row) => {
                render_mobile_hierarchy_row(app, frame, viewport, content, doc_y, scroll, row);
            }
            MobileNavigationRow::Menu { label, .. } => {
                let Some(y) = visible_y(viewport, scroll, doc_y) else {
                    continue;
                };
                frame.render_widget(
                    Paragraph::new(format!("  {label}"))
                        .style(Style::default().fg(p.overlay1).bg(p.panel_bg)),
                    Rect::new(content.x, y, content.width, 1),
                );
            }
        }
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
) {
    let Some(y) = visible_y(viewport, scroll, doc_y) else {
        return;
    };
    let p = &app.palette;
    let bg = mobile_item_bg(false, row.is_current, p);
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
    let indent = "  ".repeat(row.depth as usize + 1);
    let (marker, marker_style) = match row.target {
        NavigatorTarget::Pane { .. } => {
            let (dot, style) = state_dot(row.status, row.seen, p);
            (dot.to_string(), style.bg(bg))
        }
        _ if row.has_children => (
            if row.expanded { "▾" } else { "▸" }.to_string(),
            Style::default().fg(accent).bg(bg),
        ),
        _ => ("•".to_string(), Style::default().fg(accent).bg(bg)),
    };
    let label_style = if row.is_group {
        Style::default()
            .fg(accent)
            .bg(bg)
            .add_modifier(Modifier::BOLD)
    } else if row.is_current {
        Style::default()
            .fg(p.text)
            .bg(bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.text).bg(bg)
    };
    let mut spans = vec![
        Span::styled(indent, Style::default().bg(bg)),
        Span::styled(marker, marker_style),
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled(row.label.clone(), label_style),
    ];
    if !row.meta.is_empty() {
        spans.push(Span::styled(
            format!(" · {}", row.meta),
            Style::default().fg(p.overlay0).bg(bg),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(content.x, y, content.width, 1),
    );
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
    fn mobile_navigation_exposes_the_active_group_space_tab_and_panes() {
        let (app, group_idx, focused_pane) = hierarchy_fixture();
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let rows = mobile_navigation_rows(&app, &terminal_runtimes, None);
        let hierarchy = rows
            .iter()
            .filter_map(|row| match row {
                MobileNavigationRow::Hierarchy(row) => Some(row),
                _ => None,
            })
            .collect::<Vec<_>>();

        let inactive_group = hierarchy
            .iter()
            .find(|row| row.target == (NavigatorTarget::Group { group_idx: 0 }))
            .expect("inactive group row");
        assert!(!inactive_group.expanded);
        let active_group = hierarchy
            .iter()
            .find(|row| row.target == (NavigatorTarget::Group { group_idx }))
            .expect("active group row");
        assert!(active_group.expanded);

        let active_space = hierarchy
            .iter()
            .find(|row| row.target == (NavigatorTarget::Workspace { ws_idx: 1 }))
            .expect("active space row");
        assert_eq!(active_space.depth, 1);
        assert!(active_space.expanded);
        let sibling_space = hierarchy
            .iter()
            .find(|row| row.target == (NavigatorTarget::Workspace { ws_idx: 2 }))
            .expect("sibling space row");
        assert!(!sibling_space.expanded);

        let active_tab = hierarchy
            .iter()
            .find(|row| {
                row.target
                    == (NavigatorTarget::Tab {
                        ws_idx: 1,
                        tab_idx: 0,
                    })
            })
            .expect("active tab row");
        assert_eq!(active_tab.depth, 2);
        assert!(active_tab.expanded);
        let inactive_tab = hierarchy
            .iter()
            .find(|row| {
                row.target
                    == (NavigatorTarget::Tab {
                        ws_idx: 1,
                        tab_idx: 1,
                    })
            })
            .expect("inactive tab row");
        assert!(!inactive_tab.expanded);

        let focused = hierarchy
            .iter()
            .find(|row| {
                row.target
                    == (NavigatorTarget::Pane {
                        ws_idx: 1,
                        tab_idx: 0,
                        pane_id: focused_pane,
                    })
            })
            .expect("focused pane row");
        assert_eq!(focused.depth, 3);
        assert!(focused.is_current);
        assert!(focused.label.eq_ignore_ascii_case("claude"));
        assert!(
            focused.meta.contains("working"),
            "pane metadata: {:?}",
            focused.meta
        );
        assert!(!hierarchy.iter().any(|row| {
            matches!(
                row.target,
                NavigatorTarget::Pane {
                    ws_idx: 1,
                    tab_idx: 1,
                    ..
                }
            )
        }));
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
    fn mobile_switcher_render_and_targets_match_for_monolithic_and_attached_views() {
        let (mut app, group_idx, focused_pane) = hierarchy_fixture();
        app.view.mobile_header_rect = Rect::new(0, 0, 44, 1);
        app.view.terminal_area = Rect::new(0, 1, 44, 19);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut view = ClientViewState::from_default_client_state(&app);
        view.computed.mobile_header_rect = app.view.mobile_header_rect;
        view.computed.terminal_area = app.view.terminal_area;

        let monolithic_rows = mobile_navigation_rows(&app, &terminal_runtimes, None);
        let attached_rows = mobile_navigation_rows(&app, &terminal_runtimes, Some(&view));
        assert_eq!(monolithic_rows, attached_rows);

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
        assert!(text.contains("Infrastructure"), "switcher: {text:?}");
        assert!(text.contains("Observability"), "switcher: {text:?}");
        assert!(text.contains("dashboards"), "switcher: {text:?}");
        assert!(text.to_lowercase().contains("claude"), "switcher: {text:?}");

        let group_row = monolithic_rows
            .iter()
            .position(|row| row.target() == Some(MobileSwitcherTarget::Group(group_idx)))
            .expect("group target row");
        let pane_target = MobileSwitcherTarget::Pane {
            ws_idx: 1,
            tab_idx: 0,
            pane_id: focused_pane,
        };
        let pane_row = monolithic_rows
            .iter()
            .position(|row| row.target() == Some(pane_target))
            .expect("pane target row");
        let viewport = mobile_switcher_areas(&app).viewport;
        assert_eq!(
            mobile_switcher_target_at(&app, viewport.x + 1, viewport.y + group_row as u16),
            Some(MobileSwitcherTarget::Group(group_idx))
        );
        assert_eq!(
            mobile_switcher_target_at(&app, viewport.x + 1, viewport.y + pane_row as u16),
            Some(pane_target)
        );
        assert_eq!(
            mobile_switcher_target_at_for_view(
                &app,
                &terminal_runtimes,
                &view,
                viewport.x + 1,
                viewport.y + pane_row as u16,
            ),
            Some(pane_target)
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
