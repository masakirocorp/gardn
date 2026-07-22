use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use super::{
    scrollbar::{render_scrollbar, should_show_scrollbar},
    status::{agent_icon, state_label_color},
    text::{display_width, middle_elide, truncate_end},
    widgets::{
        modal_close_button_rect, modal_frame_areas, modal_hint_line_count, modal_stack_areas,
        panel_contrast_fg, render_modal_divider, render_modal_frame, ModalFrameSpec,
    },
};
use crate::app::{
    state::{AppState, NavigatorRow, NavigatorStateFilter, NavigatorTarget},
    view_state::ClientViewState,
};

const NAVIGATOR_WIDTH: u16 = 92;
const NAVIGATOR_HEIGHT: u16 = 30;
const NAVIGATOR_HEADER_ROWS: u16 = 4;
const NAVIGATOR_HINTS: &[(&str, &str)] = &[
    ("expand", "space · all e/c"),
    ("open", "enter / click"),
    ("search", "/"),
    ("move", "↑↓"),
];

#[derive(Debug, Clone, Copy)]
pub(crate) struct NavigatorLayout {
    pub popup: Rect,
    pub search: Rect,
    pub header_divider: Rect,
    pub body: Rect,
    pub detail_divider: Rect,
    pub detail: Rect,
    pub close: Rect,
}

fn navigator_frame_spec() -> ModalFrameSpec<'static> {
    ModalFrameSpec {
        title: "workspace navigator",
        width: NAVIGATOR_WIDTH,
        height: NAVIGATOR_HEIGHT,
        header_rows: NAVIGATOR_HEADER_ROWS,
        footer_hints: NAVIGATOR_HINTS,
        footer_max_rows: 2,
        gap: 1,
        actions_rows: 0,
        show_close: true,
    }
}

pub(crate) fn navigator_layout(area: Rect) -> Option<NavigatorLayout> {
    let frame = modal_frame_areas(area, navigator_frame_spec())?;
    let footer_rows = modal_hint_line_count(frame.inner.width, NAVIGATOR_HINTS, 2);
    let stack = modal_stack_areas(frame.inner, NAVIGATOR_HEADER_ROWS, footer_rows, 0, 1);
    let header = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<4>(stack.header);
    let content = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<3>(stack.content);
    Some(NavigatorLayout {
        popup: frame.popup,
        search: header[2],
        header_divider: header[3],
        body: content[0],
        detail_divider: content[1],
        detail: content[2],
        close: modal_close_button_rect(header[0]),
    })
}

pub(super) fn render_navigator_overlay(app: &AppState, frame: &mut Frame) {
    let rows = app.navigator_rows();
    render_navigator_overlay_from(app, &app.navigator, &rows, app.screen_rect(), frame);
}

pub(super) fn render_navigator_overlay_for_view(
    app: &AppState,
    view: &ClientViewState,
    terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    frame: &mut Frame,
) {
    let rows = app.navigator_rows_for_view(view, terminal_runtimes);
    render_navigator_overlay_from(app, &view.navigator, &rows, view.screen_rect(), frame);
}

fn render_navigator_overlay_from(
    app: &AppState,
    navigator: &crate::app::state::NavigatorState,
    rows: &[NavigatorRow],
    area: Rect,
    frame: &mut Frame,
) {
    super::dim_background(frame, area);
    let Some(layout) = navigator_layout(area) else {
        return;
    };
    if render_modal_frame(frame, area, &app.palette, navigator_frame_spec()).is_none() {
        return;
    }
    render_search_for_navigator(app, navigator, frame, layout.search);
    render_modal_divider(frame, layout.header_divider, &app.palette);
    let visible = navigator.list.visible();
    if layout.body.height > 0 {
        let start = navigator.scroll.min(rows.len());
        let end = rows
            .len()
            .min(start.saturating_add(layout.body.height as usize));
        for (visible_idx, row) in rows[start..end].iter().enumerate() {
            let idx = start + visible_idx;
            render_row(
                app,
                frame,
                Rect::new(
                    layout.body.x,
                    layout.body.y + visible_idx as u16,
                    layout.body.width,
                    1,
                ),
                row,
                Some(idx) == visible,
            );
        }
        render_navigator_scrollbar_for_view(app, navigator, frame, layout.body, rows.len());
    }
    render_modal_divider(frame, layout.detail_divider, &app.palette);
    render_navigator_detail_for_view(
        app,
        frame,
        layout.detail,
        visible.and_then(|idx| rows.get(idx)),
    );
}

pub(crate) fn navigator_popup_rect(area: Rect) -> Rect {
    navigator_layout(area)
        .map(|layout| layout.popup)
        .unwrap_or_default()
}

fn render_search_for_navigator(
    app: &AppState,
    navigator: &crate::app::state::NavigatorState,
    frame: &mut Frame,
    area: Rect,
) {
    let p = &app.palette;
    let focus_style = if navigator.search_focused {
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.overlay0)
    };
    let count = app
        .workspaces
        .iter()
        .flat_map(|workspace| workspace.tabs.iter())
        .map(|tab| tab.panes.len())
        .sum::<usize>();
    let count = count_label(count, "pane", "panes");
    let mut spans = vec![Span::styled(" / ", focus_style)];
    let query = navigator.query.trim();
    match navigator.state_filter {
        Some(NavigatorStateFilter::Blocked) => push_state_chip(
            &mut spans,
            crate::detect::AgentState::Blocked,
            true,
            app.spinner_tick,
            "blocked",
            app,
        ),
        Some(NavigatorStateFilter::Working) => push_state_chip(
            &mut spans,
            crate::detect::AgentState::Working,
            true,
            app.spinner_tick,
            "working",
            app,
        ),
        Some(NavigatorStateFilter::Idle) => push_state_chip(
            &mut spans,
            crate::detect::AgentState::Idle,
            true,
            app.spinner_tick,
            "idle",
            app,
        ),
        Some(NavigatorStateFilter::Done) => push_state_chip(
            &mut spans,
            crate::detect::AgentState::Idle,
            false,
            app.spinner_tick,
            "done",
            app,
        ),
        None if query.is_empty() => spans.push(Span::styled(
            "search groups, spaces, tabs, panes",
            Style::default().fg(p.overlay0),
        )),
        None => spans.push(Span::styled(query.to_string(), Style::default().fg(p.text))),
    }
    spans.push(Span::styled(
        format!(
            "{count:>width$}",
            width = area.width.saturating_sub(16) as usize
        ),
        Style::default().fg(p.overlay0),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_navigator_scrollbar_for_view(
    app: &AppState,
    navigator: &crate::app::state::NavigatorState,
    frame: &mut Frame,
    body: Rect,
    rows: usize,
) {
    if body.width <= 1 || body.height == 0 || rows <= body.height as usize {
        return;
    }
    let viewport = body.height as usize;
    let metrics = crate::pane::ScrollMetrics {
        viewport_rows: viewport,
        offset_from_bottom: rows
            .saturating_sub(viewport)
            .saturating_sub(navigator.scroll),
        max_offset_from_bottom: rows.saturating_sub(viewport),
    };
    if should_show_scrollbar(metrics) {
        render_scrollbar(
            frame,
            metrics,
            Rect::new(body.x + body.width - 1, body.y, 1, body.height),
            app.palette.surface_dim,
            app.palette.overlay0,
            "▕",
        );
    }
}

fn render_navigator_detail_for_view(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    row: Option<&NavigatorRow>,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let Some(row) = row else {
        return;
    };
    let detail = detail_for_row(app, row);
    let detail = middle_elide(&detail, area.width.saturating_sub(2) as usize);
    if !detail.is_empty() {
        frame.render_widget(
            Paragraph::new(format!(" {detail}")).style(Style::default().fg(app.palette.overlay0)),
            area,
        );
    }
}

fn push_state_chip(
    spans: &mut Vec<Span<'static>>,
    state: crate::detect::AgentState,
    seen: bool,
    tick: u32,
    label: &'static str,
    app: &AppState,
) {
    let (icon, icon_style) = agent_icon(state, seen, tick, &app.palette);
    spans.push(Span::styled(icon, icon_style.add_modifier(Modifier::BOLD)));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        label,
        Style::default()
            .fg(state_label_color(state, seen, &app.palette))
            .add_modifier(Modifier::BOLD),
    ));
}

fn render_row(app: &AppState, frame: &mut Frame, rect: Rect, row: &NavigatorRow, selected: bool) {
    let p = &app.palette;
    let group_accent = navigator_row_group_idx(app, row).map(|idx| app.group_accent_color(idx));
    frame.render_widget(Clear, rect);
    let base_style = if selected {
        Style::default().bg(p.accent).fg(panel_contrast_fg(p))
    } else {
        Style::default().bg(p.panel_bg).fg(p.text)
    };
    let dim_style = if selected {
        base_style
    } else {
        Style::default().fg(p.overlay0).bg(p.panel_bg)
    };
    let text_style = if selected {
        base_style.add_modifier(Modifier::BOLD)
    } else if row.is_group {
        Style::default()
            .fg(group_accent.unwrap_or(p.text))
            .bg(p.panel_bg)
            .add_modifier(Modifier::BOLD)
    } else if row.is_workspace {
        Style::default().fg(p.text).bg(p.panel_bg)
    } else if row.is_current {
        Style::default()
            .fg(p.text)
            .bg(p.panel_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.subtext0).bg(p.panel_bg)
    };
    let is_branch = row.is_group || row.is_workspace;
    let status = (!is_branch && row.status != crate::detect::AgentState::Unknown).then(|| {
        let (icon, style) = agent_icon(row.status, row.seen, app.spinner_tick, p);
        let style = if selected {
            base_style.add_modifier(Modifier::BOLD)
        } else {
            style.bg(p.panel_bg)
        };
        (icon, style)
    });

    let prefix = if row.is_group || row.is_workspace {
        if !row.has_children {
            "  "
        } else if row.expanded {
            "▾"
        } else {
            "▸"
        }
    } else if row.is_tab && !row.has_children {
        "  "
    } else if row.depth > 0 {
        "├─"
    } else {
        "  "
    };
    let prefix_style = if selected {
        base_style
    } else if is_branch {
        Style::default()
            .fg(group_accent.unwrap_or(p.overlay0))
            .bg(p.panel_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        dim_style
    };
    let indent = "  ".repeat(row.depth as usize);
    let left_fixed = format!(" {indent}{prefix} ");
    let meta_width = metadata_width(rect.width);
    let status_width = status
        .as_ref()
        .map_or(0, |(icon, _)| display_width(icon).saturating_add(1) as u16);
    let left_budget = rect
        .width
        .saturating_sub(meta_width)
        .saturating_sub(display_width(&left_fixed) as u16)
        .saturating_sub(status_width)
        .saturating_sub(3) as usize;
    let title = truncate_end(&row.label, left_budget);

    let mut spans = Vec::with_capacity(6);
    spans.push(Span::styled(format!(" {indent}"), dim_style));
    spans.push(Span::styled(prefix, prefix_style));
    spans.push(Span::raw(" "));
    if let Some((status_icon, status_style)) = status {
        spans.push(Span::styled(status_icon, status_style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(title, text_style));
    frame.render_widget(Paragraph::new(Line::from(spans)).style(base_style), rect);

    if meta_width > 0 {
        let meta_rect = Rect::new(
            rect.x + rect.width.saturating_sub(meta_width),
            rect.y,
            meta_width,
            1,
        );
        let meta = truncate_end(&row.meta, meta_width.saturating_sub(2) as usize);
        let meta_style = if selected {
            base_style
        } else if row.is_group || row.is_workspace || row.is_tab {
            Style::default().fg(p.overlay0).bg(p.panel_bg)
        } else {
            Style::default()
                .fg(state_label_color(row.status, row.seen, p))
                .bg(p.panel_bg)
        };
        frame.render_widget(
            Paragraph::new(format!(" {meta}")).style(meta_style),
            meta_rect,
        );
    }
}

fn navigator_row_group_idx(app: &AppState, row: &NavigatorRow) -> Option<usize> {
    match &row.target {
        NavigatorTarget::Group { group_idx } => Some(*group_idx),
        NavigatorTarget::Workspace { ws_idx }
        | NavigatorTarget::Tab { ws_idx, .. }
        | NavigatorTarget::Pane { ws_idx, .. } => app
            .workspaces
            .get(*ws_idx)
            .and_then(|workspace| app.group_index_by_id(&workspace.group_id)),
    }
}

fn metadata_width(width: u16) -> u16 {
    if width >= 90 {
        28
    } else if width >= 68 {
        20
    } else if width >= 52 {
        14
    } else {
        0
    }
}

fn detail_for_row(app: &AppState, row: &NavigatorRow) -> String {
    match row.target {
        NavigatorTarget::Group { group_idx } => app
            .groups
            .get(group_idx)
            .map(|group| format!("{} {} · {}", group.icon, group.name, row.meta))
            .unwrap_or_default(),
        NavigatorTarget::Workspace { ws_idx } => workspace_detail(app, ws_idx, &row.meta),
        NavigatorTarget::Tab { ws_idx, tab_idx } => tab_detail(app, ws_idx, tab_idx, &row.meta),
        NavigatorTarget::Pane {
            ws_idx,
            tab_idx,
            pane_id,
        } => pane_detail(app, ws_idx, tab_idx, pane_id),
    }
}

fn workspace_detail(app: &AppState, ws_idx: usize, activity: &str) -> String {
    let Some(ws) = app.workspaces.get(ws_idx) else {
        return String::new();
    };
    let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
    let label = ws.display_name_from(&app.terminals, &terminal_runtimes);
    let pane_count = ws.tabs.iter().map(|tab| tab.panes.len()).sum::<usize>();
    let mut parts = vec![label, count_label(pane_count, "pane", "panes")];
    if !activity.is_empty() {
        parts.push(activity.to_string());
    }
    parts.join(" · ")
}

fn tab_detail(app: &AppState, ws_idx: usize, tab_idx: usize, meta: &str) -> String {
    let Some(ws) = app.workspaces.get(ws_idx) else {
        return String::new();
    };
    let Some(tab) = ws.tabs.get(tab_idx) else {
        return String::new();
    };
    let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
    let mut parts = vec![
        ws.display_name_from(&app.terminals, &terminal_runtimes),
        format!("tab: {}", tab.display_name()),
    ];
    if !meta.is_empty() {
        parts.push(meta.to_string());
    }
    parts.join(" · ")
}

fn pane_detail(
    app: &AppState,
    ws_idx: usize,
    tab_idx: usize,
    pane_id: crate::layout::PaneId,
) -> String {
    let Some(ws) = app.workspaces.get(ws_idx) else {
        return String::new();
    };
    let Some(tab) = ws.tabs.get(tab_idx) else {
        return String::new();
    };
    let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
    let mut parts = vec![ws.display_name_from(&app.terminals, &terminal_runtimes)];
    if ws.tabs.len() > 1 {
        parts.push(format!("tab: {}", tab.display_name()));
    }
    let pane_label = tab
        .terminal_id(pane_id)
        .and_then(|terminal_id| app.terminals.get(terminal_id))
        .and_then(|terminal| terminal.manual_label.clone())
        .or_else(|| {
            ws.pane_display_number(pane_id)
                .map(|number| format!("pane {number}"))
        });
    if let Some(pane_label) = pane_label {
        parts.push(pane_label);
    }
    if let Some(terminal_id) = tab.terminal_id(pane_id) {
        if let Some(terminal) = app.terminals.get(terminal_id) {
            let presentation = terminal.effective_presentation();
            if let Some(title) = presentation.title {
                parts.push(title);
            }
            let display_agent = terminal.effective_display_agent();
            if let Some(agent) = display_agent.as_deref().or_else(|| {
                terminal
                    .agent_name
                    .as_deref()
                    .or_else(|| terminal.effective_agent_label())
            }) {
                parts.push(agent.to_string());
                let seen = tab
                    .panes
                    .get(&pane_id)
                    .map(|pane| pane.seen)
                    .unwrap_or(true);
                let state = row_state(app, ws_idx, tab_idx, pane_id);
                let status = presentation
                    .state_labels
                    .get(display_state(state, seen))
                    .cloned()
                    .unwrap_or_else(|| display_state(state, seen).to_string());
                parts.push(status);
            } else {
                parts.push("shell".to_string());
            }
            if let Some(status) = terminal.effective_custom_status() {
                parts.push(status.to_string());
            }
        }
    }
    parts.join(" · ")
}

fn row_state(
    app: &AppState,
    ws_idx: usize,
    tab_idx: usize,
    pane_id: crate::layout::PaneId,
) -> crate::detect::AgentState {
    app.workspaces
        .get(ws_idx)
        .and_then(|ws| ws.tabs.get(tab_idx))
        .and_then(|tab| tab.terminal_id(pane_id))
        .and_then(|terminal_id| app.terminals.get(terminal_id))
        .map(|terminal| terminal.state)
        .unwrap_or(crate::detect::AgentState::Unknown)
}

fn display_state(state: crate::detect::AgentState, seen: bool) -> &'static str {
    match (state, seen) {
        (crate::detect::AgentState::Blocked, _) => "blocked",
        (crate::detect::AgentState::Working, _) => "working",
        (crate::detect::AgentState::Idle, false) => "done",
        (crate::detect::AgentState::Idle, true) => "idle",
        (crate::detect::AgentState::Unknown, _) => "unknown",
    }
}

fn count_label(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, Terminal};

    use super::*;
    use crate::workspace::Workspace;

    #[test]
    fn client_navigator_detail_uses_the_client_selected_row() {
        let mut app = AppState::test_new();
        crate::ui::compute_view(&mut app, Rect::new(0, 0, 120, 30));
        app.workspaces = vec![
            Workspace::test_new("app-selected-workspace"),
            Workspace::test_new("client-selected-workspace"),
        ];
        app.navigator.query = "app-selected".to_string();

        let mut view = ClientViewState::from_default_client_state(&app);
        view.navigator.query = "client-selected".to_string();
        let selected = app
            .navigator_rows_for_view(&view, &crate::terminal::TerminalRuntimeRegistry::new())
            .iter()
            .position(|row| matches!(row.target, NavigatorTarget::Workspace { ws_idx: 1 }))
            .expect("client-selected workspace row");
        view.navigator.list.select(selected);
        view.navigator.list.show();

        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_navigator_overlay_for_view(&app, &view, &terminal_runtimes, frame))
            .expect("render client navigator");

        let text = buffer_text(terminal.backend().buffer(), 120, 30);
        assert!(
            text.contains("client-selected-workspace · 1 pane"),
            "client navigator detail: {text:?}"
        );
        assert!(!text.contains("app-selected-workspace · 1 pane"));
        assert!(
            text.contains("workspace navigator"),
            "modal title: {text:?}"
        );
        assert!(text.contains("esc close"), "modal close action: {text:?}");
        assert!(
            text.contains("expand space · all e/c"),
            "modal footer hints: {text:?}"
        );
    }

    #[test]
    fn navigator_detail_renumbers_unnamed_panes_after_one_closes() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("personal");
        let closed_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);
        let last_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);
        assert!(!workspace.close_pane(closed_pane));
        workspace.tabs[0].layout.focus_pane(last_pane);
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;
        let area = Rect::new(0, 0, 120, 30);
        crate::ui::compute_view(&mut app, area);
        app.open_navigator();

        let mut view = ClientViewState::from_default_client_state(&app);
        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let selected = app
            .navigator_rows_for_view(&view, &terminal_runtimes)
            .iter()
            .position(|row| {
                matches!(
                    row.target,
                    NavigatorTarget::Pane { pane_id, .. } if pane_id == last_pane
                )
            })
            .expect("remaining pane row");
        view.navigator.list.select(selected);
        view.navigator.list.show();

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_navigator_overlay_for_view(&app, &view, &terminal_runtimes, frame))
            .expect("render navigator");

        let layout = navigator_layout(area).expect("navigator layout");
        let detail = (layout.detail.x..layout.detail.x + layout.detail.width)
            .map(|x| terminal.backend().buffer()[(x, layout.detail.y)].symbol())
            .collect::<String>();
        assert!(detail.contains("personal · pane 2"), "{detail:?}");
    }

    #[test]
    fn navigator_branch_rows_only_show_disclosure_and_group_identity() {
        let app = AppState::test_new();
        let row = NavigatorRow {
            target: NavigatorTarget::Group { group_idx: 0 },
            depth: 0,
            label: "✿ Research Lab".to_string(),
            meta: String::new(),
            status: crate::detect::AgentState::Unknown,
            seen: true,
            is_current: true,
            is_group: true,
            is_workspace: false,
            is_tab: false,
            has_children: true,
            expanded: true,
            search_text: String::new(),
        };
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_row(&app, frame, frame.area(), &row, true))
            .expect("render branch row");

        let text = buffer_text(terminal.backend().buffer(), 80, 1);
        assert!(text.contains("▾ ✿ Research Lab"), "branch row: {text:?}");
        assert!(!text.contains(['→', '◆', '○']), "branch row: {text:?}");
    }

    #[test]
    fn navigator_leaf_workspace_rows_do_not_show_a_disclosure_prefix() {
        let app = AppState::test_new();
        let row = NavigatorRow {
            target: NavigatorTarget::Workspace { ws_idx: 0 },
            depth: 1,
            label: "autosave (1)".to_string(),
            meta: String::new(),
            status: crate::detect::AgentState::Unknown,
            seen: true,
            is_current: true,
            is_group: false,
            is_workspace: true,
            is_tab: false,
            has_children: false,
            expanded: true,
            search_text: String::new(),
        };
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_row(&app, frame, frame.area(), &row, true))
            .expect("render leaf workspace row");

        let text = buffer_text(terminal.backend().buffer(), 80, 1);
        assert!(text.contains("autosave (1)"), "workspace row: {text:?}");
        assert!(!text.contains(['▸', '▾']), "workspace row: {text:?}");
    }

    #[test]
    fn navigator_leaf_tab_rows_do_not_show_a_disclosure_prefix() {
        let app = AppState::test_new();
        let row = NavigatorRow {
            target: NavigatorTarget::Tab {
                ws_idx: 0,
                tab_idx: 0,
            },
            depth: 1,
            label: "personal".to_string(),
            meta: "1 pane".to_string(),
            status: crate::detect::AgentState::Unknown,
            seen: true,
            is_current: false,
            is_group: false,
            is_workspace: false,
            is_tab: true,
            has_children: false,
            expanded: false,
            search_text: String::new(),
        };
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_row(&app, frame, frame.area(), &row, false))
            .expect("render leaf tab row");

        let text = buffer_text(terminal.backend().buffer(), 80, 1);
        assert!(text.contains("personal"), "tab row: {text:?}");
        assert!(!text.contains(['▸', '▾', '├']), "tab row: {text:?}");
    }

    #[test]
    fn navigator_uses_group_accent_without_coloring_descendant_labels() {
        let mut app = AppState::test_new();
        app.set_group_accent(0, Some(crate::config::TerminalAccent::Magenta));
        let accent = app.group_accent_color(0);
        let mut workspace = Workspace::test_new("Agent Experiments");
        workspace.group_id = app.groups[0].id.clone();
        app.workspaces = vec![workspace];
        let group_row = NavigatorRow {
            target: NavigatorTarget::Group { group_idx: 0 },
            depth: 0,
            label: "✿ Research Lab".to_string(),
            meta: String::new(),
            status: crate::detect::AgentState::Unknown,
            seen: true,
            is_current: false,
            is_group: true,
            is_workspace: false,
            is_tab: false,
            has_children: true,
            expanded: true,
            search_text: String::new(),
        };
        let workspace_row = NavigatorRow {
            has_children: false,
            target: NavigatorTarget::Workspace { ws_idx: 0 },
            depth: 1,
            label: "Agent Experiments".to_string(),
            meta: String::new(),
            status: crate::detect::AgentState::Unknown,
            seen: true,
            is_current: false,
            is_group: false,
            is_workspace: true,
            is_tab: false,
            expanded: false,
            search_text: String::new(),
        };
        let backend = TestBackend::new(80, 2);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| {
                render_row(&app, frame, Rect::new(0, 0, 80, 1), &group_row, false);
                render_row(&app, frame, Rect::new(0, 1, 80, 1), &workspace_row, false);
            })
            .expect("render colored hierarchy");

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(3, 0)].style().fg, Some(accent));
        assert_eq!(buffer[(3, 1)].style().fg, Some(accent));
        assert_eq!(buffer[(5, 1)].style().fg, Some(app.palette.text));
    }

    #[test]
    fn navigator_uses_the_tall_shared_modal_frame() {
        let layout = navigator_layout(Rect::new(0, 0, 120, 40)).expect("navigator layout");
        assert_eq!(layout.popup, Rect::new(14, 5, 92, 30));
        assert_eq!(layout.search, Rect::new(15, 8, 90, 1));
        assert_eq!(layout.header_divider, Rect::new(15, 9, 90, 1));
        assert_eq!(layout.body, Rect::new(15, 11, 90, 19));
        assert_eq!(layout.detail_divider, Rect::new(15, 30, 90, 1));
        assert_eq!(layout.detail, Rect::new(15, 31, 90, 1));
        assert_eq!(layout.close.y, 6);

        assert_eq!(
            navigator_popup_rect(Rect::new(0, 0, 80, 14)),
            Rect::new(2, 1, 76, 12)
        );
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
