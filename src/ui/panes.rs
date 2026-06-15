use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::scrollbar::{render_pane_scrollbar, render_scrollbar, should_show_scrollbar};
use super::widgets::{fill_rect, panel_contrast_fg};
use crate::app::state::Palette;
use crate::app::{AppState, Mode};
use crate::layout::PaneInfo;
use crate::terminal::{TerminalRuntime, TerminalRuntimeRegistry};

pub(crate) fn pane_is_scrolled_back(rt: &TerminalRuntime) -> bool {
    rt.scroll_metrics()
        .is_some_and(|metrics| metrics.offset_from_bottom > 0)
}

fn truncate_label(text: &str, max_width: usize) -> String {
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

fn pane_border_title(label: &str, pane_width: u16) -> Option<String> {
    let label = label.trim();
    if label.is_empty() || pane_width <= 4 {
        return None;
    }
    let max_label_width = pane_width.saturating_sub(4) as usize;
    Some(format!(" {} ", truncate_label(label, max_label_width)))
}

fn stable_terminal_inner_rect(pane_inner: Rect) -> Rect {
    if pane_inner.width <= 4 {
        return pane_inner;
    }

    Rect::new(
        pane_inner.x,
        pane_inner.y,
        pane_inner.width.saturating_sub(1),
        pane_inner.height,
    )
}

fn pane_inner_rect(area: Rect, framed: bool) -> Rect {
    if framed {
        Block::default().borders(Borders::ALL).inner(area)
    } else {
        area
    }
}

fn runtime_for_tab_pane<'a>(
    terminal_runtimes: &'a TerminalRuntimeRegistry,
    tab: &'a crate::workspace::Tab,
    pane_id: crate::layout::PaneId,
) -> Option<(&'a crate::terminal::TerminalId, &'a TerminalRuntime)> {
    let terminal_id = tab.terminal_id(pane_id)?;
    #[cfg(test)]
    if let Some(runtime) = tab.runtimes.get(&pane_id) {
        return Some((terminal_id, runtime));
    }
    terminal_runtimes
        .get(terminal_id)
        .map(|runtime| (terminal_id, runtime))
}

fn stable_scrollbar_gutter(rt: &TerminalRuntime, pane_inner: Rect) -> (Rect, Option<Rect>) {
    let inner_rect = stable_terminal_inner_rect(pane_inner);
    if inner_rect == pane_inner {
        return (inner_rect, None);
    }
    let gutter = Rect::new(
        pane_inner.x + pane_inner.width.saturating_sub(1),
        pane_inner.y,
        1,
        pane_inner.height,
    );
    let scrollbar_rect = rt
        .scroll_metrics()
        .filter(|metrics| should_show_scrollbar(*metrics))
        .map(|_| gutter);

    (inner_rect, scrollbar_rect)
}

fn pane_theme_background(p: &Palette) -> Option<Color> {
    match p.panel_bg {
        Color::Reset => None,
        color => Some(color),
    }
}

/// Resize every visible runtime in a tab to the geometry it would receive if the tab were selected.
pub(super) fn resize_tab_panes(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    tab: &crate::workspace::Tab,
    area: Rect,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    let multi_pane = tab.layout.pane_count() > 1;

    if tab.zoomed {
        let focused_id = tab.layout.focused();
        if let Some((terminal_id, rt)) = runtime_for_tab_pane(terminal_runtimes, tab, focused_id) {
            let pane_inner = pane_inner_rect(area, multi_pane);
            let inner_rect = stable_terminal_inner_rect(pane_inner);
            if !app.direct_attach_resize_locks.contains(terminal_id) {
                rt.resize(
                    inner_rect.height,
                    inner_rect.width,
                    cell_size.width_px,
                    cell_size.height_px,
                );
            }
        }
        return;
    }

    for info in tab.layout.panes(area) {
        let pane_inner = if multi_pane {
            Block::default().borders(Borders::ALL).inner(info.rect)
        } else {
            area
        };

        if let Some((terminal_id, rt)) = runtime_for_tab_pane(terminal_runtimes, tab, info.id) {
            let inner_rect = stable_terminal_inner_rect(pane_inner);
            if !app.direct_attach_resize_locks.contains(terminal_id) {
                rt.resize(
                    inner_rect.height,
                    inner_rect.width,
                    cell_size.width_px,
                    cell_size.height_px,
                );
            }
        }
    }
}

/// Compute pane layout info and optionally resize pane runtimes to match.
pub(super) fn compute_pane_infos(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
    resize_panes: bool,
    cell_size: crate::kitty_graphics::HostCellSize,
) -> Vec<PaneInfo> {
    let Some(ws_idx) = app.active else {
        return Vec::new();
    };
    let Some(ws) = app.workspaces.get(ws_idx) else {
        return Vec::new();
    };
    let Some(tab) = ws.active_tab() else {
        return Vec::new();
    };

    let multi_pane = tab.layout.pane_count() > 1;
    let terminal_active = app.mode == Mode::Terminal;

    if tab.zoomed {
        let focused_id = tab.layout.focused();
        let pane_inner = pane_inner_rect(area, multi_pane);
        let mut inner_rect = pane_inner;
        let mut scrollbar_rect = None;
        if let Some(rt) = app.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, focused_id) {
            (inner_rect, scrollbar_rect) = stable_scrollbar_gutter(rt, pane_inner);
            if resize_panes
                && ws.terminal_id(focused_id).is_some_and(|terminal_id| {
                    !app.direct_attach_resize_locks.contains(terminal_id)
                })
            {
                rt.resize(
                    inner_rect.height,
                    inner_rect.width,
                    cell_size.width_px,
                    cell_size.height_px,
                );
            }
        }
        return vec![PaneInfo {
            id: focused_id,
            rect: area,
            inner_rect,
            scrollbar_rect,
            is_focused: true,
        }];
    }

    let mut pane_infos = tab.layout.panes(area);

    for info in &mut pane_infos {
        let pane_inner = if multi_pane {
            let border_set = if info.is_focused && terminal_active {
                ratatui::symbols::border::THICK
            } else {
                ratatui::symbols::border::PLAIN
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .border_set(border_set);
            block.inner(info.rect)
        } else {
            area
        };

        let mut inner_rect = pane_inner;
        let mut scrollbar_rect = None;
        if let Some(rt) = app.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id) {
            (inner_rect, scrollbar_rect) = stable_scrollbar_gutter(rt, pane_inner);
            if resize_panes
                && ws.terminal_id(info.id).is_some_and(|terminal_id| {
                    !app.direct_attach_resize_locks.contains(terminal_id)
                })
            {
                rt.resize(
                    inner_rect.height,
                    inner_rect.width,
                    cell_size.width_px,
                    cell_size.height_px,
                );
            }
        }

        info.inner_rect = inner_rect;
        info.scrollbar_rect = scrollbar_rect;
    }

    pane_infos
}

pub(super) fn render_panes(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    let Some(ws_idx) = app.active else {
        render_empty(app, frame, area);
        return;
    };
    let Some(ws) = app.workspaces.get(ws_idx) else {
        render_empty(app, frame, area);
        return;
    };
    let Some(tab) = ws.active_tab() else {
        render_empty(app, frame, area);
        return;
    };

    let multi_pane = tab.layout.pane_count() > 1;
    let active_accent = app.active_workspace_accent_color();
    let terminal_active = app.mode == Mode::Terminal;

    for info in &app.view.pane_infos {
        let pane_state = ws.pane_state(info.id);
        if let Some(diff) = pane_state.and_then(|pane| pane.native_diff()) {
            if multi_pane {
                let (border_style, border_set) = if info.is_focused && terminal_active {
                    (
                        Style::default().fg(active_accent),
                        ratatui::symbols::border::THICK,
                    )
                } else if info.is_focused {
                    (
                        Style::default().fg(active_accent),
                        ratatui::symbols::border::PLAIN,
                    )
                } else {
                    (
                        Style::default().fg(app.palette.overlay0),
                        ratatui::symbols::border::PLAIN,
                    )
                };
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .border_set(border_set)
                    .title(Line::from(Span::styled(" diff ", border_style)));
                frame.render_widget(block, info.rect);
            }
            render_native_diff_pane(app, diff, frame, info.inner_rect, active_accent);
            continue;
        }

        if let Some(rt) = app.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id) {
            if multi_pane {
                let (border_style, border_set) = if info.is_focused && terminal_active {
                    (
                        Style::default().fg(active_accent),
                        ratatui::symbols::border::THICK,
                    )
                } else if info.is_focused {
                    (
                        Style::default().fg(active_accent),
                        ratatui::symbols::border::PLAIN,
                    )
                } else {
                    (
                        Style::default().fg(app.palette.overlay0),
                        ratatui::symbols::border::PLAIN,
                    )
                };

                let mut block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .border_set(border_set);
                if let Some(title) = pane_state
                    .and_then(|pane| app.terminals.get(&pane.attached_terminal_id))
                    .and_then(|terminal| {
                        terminal.border_label(app.show_agent_labels_on_pane_borders)
                    })
                    .and_then(|label| pane_border_title(&label, info.rect.width))
                {
                    block = block.title(Line::from(Span::styled(title, border_style)));
                }
                frame.render_widget(block, info.rect);
            }

            let show_cursor = info.is_focused && terminal_active && !pane_is_scrolled_back(rt);
            rt.render_with_theme_background(
                frame,
                info.inner_rect,
                show_cursor,
                pane_theme_background(&app.palette),
            );
            render_pane_scrollbar(app, frame, info, rt);

            let should_dim = !info.is_focused && multi_pane && !terminal_active;
            if should_dim {
                let inner = info.inner_rect;
                let buf = frame.buffer_mut();
                for y in inner.y..inner.y + inner.height {
                    for x in inner.x..inner.x + inner.width {
                        let cell = &mut buf[(x, y)];
                        cell.set_style(cell.style().add_modifier(Modifier::DIM));
                    }
                }
            }

            render_selection_highlight(
                &app.selection,
                frame,
                info.id,
                info.inner_rect,
                rt.scroll_metrics(),
                &app.palette,
                app.host_terminal_theme,
            );
            render_copy_mode_cursor(app, frame, info);
        }
    }
}
fn render_native_diff_pane(
    app: &AppState,
    diff: &crate::native_diff::NativeDiffPaneState,
    frame: &mut Frame,
    area: Rect,
    accent: Color,
) {
    fill_rect(frame, area, Style::default().bg(app.palette.panel_bg));
    if area.width == 0 || area.height == 0 {
        return;
    }

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    if diff.show_file_list {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(file_list_width(vertical[0].width)),
                Constraint::Length(1),
                Constraint::Min(10),
            ])
            .split(vertical[0]);
        render_native_diff_file_list(app, diff, frame, chunks[0], accent);
        render_native_diff_separator(app, frame, chunks[1]);
        render_native_diff_file_patch(app, diff, frame, chunks[2], accent);
    } else {
        render_native_diff_file_patch(app, diff, frame, vertical[0], accent);
    }
    render_native_diff_footer(app, diff, frame, vertical[1]);
}
fn render_native_diff_separator(app: &AppState, frame: &mut Frame, area: Rect) {
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        let cell = &mut buf[(area.x, y)];
        cell.set_symbol("│");
        cell.set_style(
            Style::default()
                .fg(app.palette.surface_dim)
                .bg(app.palette.panel_bg),
        );
    }
}

fn file_list_width(total: u16) -> u16 {
    total.clamp(28, 36).min(total.saturating_sub(10))
}

fn render_native_diff_file_list(
    app: &AppState,
    diff: &crate::native_diff::NativeDiffPaneState,
    frame: &mut Frame,
    area: Rect,
    accent: Color,
) {
    let mut lines = Vec::new();
    push_native_diff_bucket_lines(
        &mut lines,
        diff,
        crate::native_diff::DiffBucket::Changed,
        "changed",
        accent,
        &app.palette,
    );
    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    push_native_diff_bucket_lines(
        &mut lines,
        diff,
        crate::native_diff::DiffBucket::Staged,
        "staged",
        accent,
        &app.palette,
    );
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "no changes",
            Style::default().fg(app.palette.subtext0),
        )));
    }
    let metrics = scroll_metrics(lines.len(), area.height as usize, diff.file_scroll);
    let (body, track) = split_scrollbar_area(area, metrics);
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .skip(diff.file_scroll)
                .take(body.height as usize)
                .collect::<Vec<_>>(),
        )
        .style(
            Style::default()
                .fg(app.palette.text)
                .bg(app.palette.panel_bg),
        ),
        body,
    );
    if let Some(track) = track {
        render_scrollbar(
            frame,
            metrics,
            track,
            app.palette.surface_dim,
            app.palette.overlay1,
            "▐",
        );
    }
}

fn push_native_diff_bucket_lines(
    lines: &mut Vec<Line<'static>>,
    diff: &crate::native_diff::NativeDiffPaneState,
    bucket: crate::native_diff::DiffBucket,
    label: &'static str,
    accent: Color,
    p: &Palette,
) {
    let mut bucket_files = diff
        .session
        .files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.bucket == bucket)
        .peekable();
    if bucket_files.peek().is_none() {
        return;
    }
    lines.push(Line::from(Span::styled(
        label,
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    )));
    for (index, file) in bucket_files {
        let selected = diff
            .selected_file
            .is_some_and(|selection| selection.file_index == index && selection.bucket == bucket);
        let path = file
            .new_path
            .as_ref()
            .or(file.old_path.as_ref())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "(unknown)".to_string());
        let row_bg = selected.then_some(p.surface0);
        let bg = row_bg.unwrap_or(p.panel_bg);
        let path_style = if selected {
            Style::default()
                .fg(accent)
                .bg(bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.text)
        };
        let marker_style = Style::default()
            .fg(file_status_color(file, p))
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        let mut spans = vec![
            Span::styled(" ", Style::default().bg(bg)),
            Span::styled(file_status_marker(file), marker_style),
            Span::styled(" ", Style::default().bg(bg)),
            Span::styled(path, path_style),
        ];
        spans.extend(native_diff_stats_spans(file.added, file.deleted, p, row_bg));
        lines.push(Line::from(spans));
    }
}
fn file_status_marker(file: &crate::native_diff::NativeDiffFile) -> &'static str {
    match file.status {
        crate::native_diff::DiffFileStatus::Added => "+",
        crate::native_diff::DiffFileStatus::Deleted => "-",
        crate::native_diff::DiffFileStatus::Renamed => "→",
        crate::native_diff::DiffFileStatus::Binary => "■",
        crate::native_diff::DiffFileStatus::Modified => "~",
    }
}

fn file_status_color(file: &crate::native_diff::NativeDiffFile, p: &Palette) -> Color {
    match file.status {
        crate::native_diff::DiffFileStatus::Added => p.green,
        crate::native_diff::DiffFileStatus::Deleted => p.red,
        crate::native_diff::DiffFileStatus::Renamed => p.teal,
        crate::native_diff::DiffFileStatus::Binary => p.mauve,
        crate::native_diff::DiffFileStatus::Modified => p.yellow,
    }
}

fn native_diff_stats_spans(
    added: usize,
    deleted: usize,
    p: &Palette,
    bg: Option<Color>,
) -> Vec<Span<'static>> {
    let bg = bg.unwrap_or(p.panel_bg);
    let mut spans = vec![Span::styled(" ", Style::default().bg(bg))];
    if added > 0 {
        spans.push(Span::styled(
            format!("+{added}"),
            Style::default().fg(p.green).bg(bg),
        ));
    }
    if added > 0 && deleted > 0 {
        spans.push(Span::styled(" ", Style::default().bg(bg)));
    }
    if deleted > 0 {
        spans.push(Span::styled(
            format!("-{deleted}"),
            Style::default().fg(p.red).bg(bg),
        ));
    }
    spans
}

fn render_native_diff_footer(
    app: &AppState,
    diff: &crate::native_diff::NativeDiffPaneState,
    frame: &mut Frame,
    area: Rect,
) {
    let line = if let Some(error) = &diff.last_error {
        Line::from(Span::styled(
            error.clone(),
            Style::default().fg(app.palette.red),
        ))
    } else {
        Line::from(vec![
            Span::styled("move ", Style::default().fg(app.palette.subtext0)),
            Span::styled("↑↓", Style::default().fg(app.palette.text)),
            Span::styled(" · hunk ", Style::default().fg(app.palette.subtext0)),
            Span::styled("[]", Style::default().fg(app.palette.text)),
            Span::styled(" · file ", Style::default().fg(app.palette.subtext0)),
            Span::styled("stage s", Style::default().fg(app.palette.text)),
            Span::styled(" / ", Style::default().fg(app.palette.subtext0)),
            Span::styled("unstage u", Style::default().fg(app.palette.text)),
            Span::styled(" · hunk ", Style::default().fg(app.palette.subtext0)),
            Span::styled("stage S", Style::default().fg(app.palette.text)),
            Span::styled(" / ", Style::default().fg(app.palette.subtext0)),
            Span::styled("unstage U", Style::default().fg(app.palette.text)),
            Span::styled(
                if diff.show_file_list {
                    " · hide files "
                } else {
                    " · show files "
                },
                Style::default().fg(app.palette.subtext0),
            ),
            Span::styled("b", Style::default().fg(app.palette.text)),
            Span::styled(
                if diff.wrap_lines {
                    " · nowrap "
                } else {
                    " · wrap "
                },
                Style::default().fg(app.palette.subtext0),
            ),
            Span::styled("w", Style::default().fg(app.palette.text)),
            Span::styled(" · mode ", Style::default().fg(app.palette.subtext0)),
            Span::styled(
                native_diff_mode_label(diff),
                Style::default().fg(app.palette.text),
            ),
            Span::styled(" m", Style::default().fg(app.palette.text)),
            Span::styled(" · refresh ", Style::default().fg(app.palette.subtext0)),
            Span::styled("r", Style::default().fg(app.palette.text)),
        ])
    };
    frame.render_widget(
        Paragraph::new(line).style(
            Style::default()
                .fg(app.palette.text)
                .bg(app.palette.panel_bg),
        ),
        area,
    );
}

fn render_native_diff_file_patch(
    app: &AppState,
    diff: &crate::native_diff::NativeDiffPaneState,
    frame: &mut Frame,
    area: Rect,
    accent: Color,
) {
    let Some(file) = diff.selected_file() else {
        return;
    };
    let split = native_diff_uses_split_mode(diff, area.width);
    let mut lines = native_diff_patch_lines(app, diff, file, accent, area.width);
    let header = lines.first().cloned().unwrap_or_else(|| Line::from(""));
    let body_lines = if lines.is_empty() {
        Vec::new()
    } else {
        lines.split_off(1)
    };
    let header_area = Rect::new(area.x, area.y, area.width, area.height.min(1));
    frame.render_widget(
        Paragraph::new(vec![header]).style(
            Style::default()
                .fg(app.palette.text)
                .bg(app.palette.panel_bg),
        ),
        header_area,
    );
    let body_area = Rect::new(
        area.x,
        area.y.saturating_add(1),
        area.width,
        area.height.saturating_sub(1),
    );
    let effective_scroll = diff
        .diff_scroll
        .min(body_lines.len().saturating_sub(body_area.height as usize));
    let metrics = scroll_metrics(
        body_lines.len(),
        body_area.height as usize,
        effective_scroll,
    );
    let (body, track) = split_scrollbar_area(body_area, metrics);
    let paragraph = Paragraph::new(
        body_lines
            .into_iter()
            .skip(effective_scroll)
            .take(body.height as usize)
            .collect::<Vec<_>>(),
    )
    .style(
        Style::default()
            .fg(app.palette.text)
            .bg(app.palette.panel_bg),
    );
    let paragraph = if diff.wrap_lines && !split {
        paragraph.wrap(Wrap { trim: false })
    } else {
        paragraph
    };
    frame.render_widget(paragraph, body);
    if split {
        render_native_diff_split_divider(app, frame, body);
    }
    if let Some(track) = track {
        render_scrollbar(
            frame,
            metrics,
            track,
            app.palette.surface_dim,
            app.palette.overlay1,
            "▐",
        );
    }
}
fn native_diff_uses_split_mode(
    diff: &crate::native_diff::NativeDiffPaneState,
    area_width: u16,
) -> bool {
    match diff.view_mode {
        crate::native_diff::NativeDiffViewMode::Unified => false,
        crate::native_diff::NativeDiffViewMode::Split => true,
        crate::native_diff::NativeDiffViewMode::Auto => area_width >= 110,
    }
}

fn render_native_diff_split_divider(app: &AppState, frame: &mut Frame, area: Rect) {
    let divider_x = area.x.saturating_add(area.width / 2);
    if divider_x >= area.x.saturating_add(area.width) {
        return;
    }
    let buf = frame.buffer_mut();
    for y in area.y..area.y.saturating_add(area.height) {
        let cell = &mut buf[(divider_x, y)];
        cell.set_symbol("│");
        cell.set_style(
            Style::default()
                .fg(app.palette.surface_dim)
                .bg(app.palette.panel_bg),
        );
    }
}

fn native_diff_mode_label(diff: &crate::native_diff::NativeDiffPaneState) -> &'static str {
    match diff.view_mode {
        crate::native_diff::NativeDiffViewMode::Unified => "unified",
        crate::native_diff::NativeDiffViewMode::Split => "split",
        crate::native_diff::NativeDiffViewMode::Auto => "auto",
    }
}

fn native_diff_patch_lines(
    app: &AppState,
    diff: &crate::native_diff::NativeDiffPaneState,
    file: &crate::native_diff::NativeDiffFile,
    accent: Color,
    area_width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let path = native_diff_header_path(file);
    let mut header_spans = vec![Span::styled(
        path,
        Style::default()
            .fg(app.palette.text)
            .add_modifier(Modifier::BOLD),
    )];
    if file.deleted > 0 {
        header_spans.push(Span::styled(
            format!(" -{}", file.deleted),
            Style::default().fg(app.palette.red),
        ));
    }
    if file.added > 0 {
        header_spans.push(Span::styled(
            format!(" +{}", file.added),
            Style::default().fg(app.palette.green),
        ));
    }
    lines.push(Line::from(header_spans));
    if file.binary {
        lines.push(Line::from(Span::styled(
            "binary file changed",
            Style::default().fg(app.palette.subtext0),
        )));
    }
    let split = native_diff_uses_split_mode(diff, area_width);
    if split {
        push_native_diff_split_lines(&mut lines, app, diff, file, accent, area_width);
    } else {
        push_native_diff_unified_lines(&mut lines, app, diff, file, accent, area_width);
    }
    lines
}

fn push_native_diff_unified_lines(
    lines: &mut Vec<Line<'static>>,
    app: &AppState,
    diff: &crate::native_diff::NativeDiffPaneState,
    file: &crate::native_diff::NativeDiffFile,
    accent: Color,
    area_width: u16,
) {
    let gutter_width = line_number_gutter_width(file);
    let text_width = (area_width as usize).saturating_sub(gutter_width * 2 + 4);
    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        push_native_diff_hunk_header(lines, app, diff, hunk_index, hunk, accent, area_width);
        for row in native_diff_hunk_rows(diff, hunk_index, hunk) {
            match row {
                RenderDiffRow::Line(line) => {
                    let (marker, marker_style, text_style) =
                        native_diff_line_styles(app, line.kind);
                    let bg = native_diff_line_bg(app, line.kind);
                    let chunks = if diff.wrap_lines {
                        wrap_native_diff_text(&line.text, text_width)
                    } else {
                        vec![truncate_label(&line.text, text_width)]
                    };
                    for (index, chunk) in chunks.into_iter().enumerate() {
                        let first = index == 0;
                        lines.push(Line::from(vec![
                            native_diff_rail_span(app, line.kind),
                            Span::styled(
                                if first {
                                    format_line_number(line.old_line, gutter_width)
                                } else {
                                    " ".repeat(gutter_width)
                                },
                                native_diff_gutter_style(app, line.kind, true).bg(bg),
                            ),
                            Span::styled(
                                if first {
                                    format_line_number(line.new_line, gutter_width)
                                } else {
                                    " ".repeat(gutter_width)
                                },
                                native_diff_gutter_style(app, line.kind, false).bg(bg),
                            ),
                            Span::styled(
                                if first {
                                    format!(" {marker}")
                                } else {
                                    "  ".to_string()
                                },
                                marker_style.bg(bg),
                            ),
                            Span::styled(pad_truncate_label(&chunk, text_width), text_style.bg(bg)),
                        ]));
                    }
                }
                RenderDiffRow::Fold {
                    key,
                    count,
                    expanded,
                } => {
                    push_native_diff_fold_line(lines, app, key, count, expanded, area_width);
                }
            }
        }
    }
}

fn push_native_diff_split_lines(
    lines: &mut Vec<Line<'static>>,
    app: &AppState,
    diff: &crate::native_diff::NativeDiffPaneState,
    file: &crate::native_diff::NativeDiffFile,
    accent: Color,
    area_width: u16,
) {
    let gutter_width = line_number_gutter_width(file);
    let left_width = split_left_text_width(area_width, gutter_width);
    let right_width = split_right_text_width(area_width, gutter_width);
    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        push_native_diff_hunk_header(lines, app, diff, hunk_index, hunk, accent, area_width);
        for row in native_diff_hunk_rows(diff, hunk_index, hunk) {
            match row {
                RenderDiffRow::Line(line) => {
                    let removed = line.kind == crate::native_diff::DiffLineKind::Removed;
                    let added = line.kind == crate::native_diff::DiffLineKind::Added;
                    let left_text = if added { "" } else { &line.text };
                    let right_text = if removed { "" } else { &line.text };
                    let left_bg = if removed {
                        native_diff_removed_bg(app)
                    } else if added {
                        app.palette.surface_dim
                    } else {
                        app.palette.panel_bg
                    };
                    let right_bg = if added {
                        native_diff_added_bg(app)
                    } else if removed {
                        app.palette.surface_dim
                    } else {
                        app.palette.panel_bg
                    };
                    let left_struct_bg = if added { app.palette.panel_bg } else { left_bg };
                    let right_struct_bg = if removed {
                        app.palette.panel_bg
                    } else {
                        right_bg
                    };
                    let left_style = if removed {
                        Style::default().fg(app.palette.red).bg(left_bg)
                    } else {
                        Style::default().fg(app.palette.text).bg(left_bg)
                    };
                    let right_style = if added {
                        Style::default().fg(app.palette.green).bg(right_bg)
                    } else {
                        Style::default().fg(app.palette.text).bg(right_bg)
                    };
                    let left_chunks = if diff.wrap_lines {
                        wrap_native_diff_text(left_text, left_width)
                    } else {
                        vec![truncate_label(left_text, left_width)]
                    };
                    let right_chunks = if diff.wrap_lines {
                        wrap_native_diff_text(right_text, right_width)
                    } else {
                        vec![truncate_label(right_text, right_width)]
                    };
                    let rows = left_chunks.len().max(right_chunks.len()).max(1);
                    for index in 0..rows {
                        let first = index == 0;
                        let left_chunk = left_chunks.get(index).cloned().unwrap_or_default();
                        let right_chunk = right_chunks.get(index).cloned().unwrap_or_default();
                        lines.push(Line::from(vec![
                            native_diff_split_rail_span(app, line.kind, true, left_struct_bg),
                            Span::styled(
                                if first {
                                    format_line_number(line.old_line, gutter_width)
                                } else {
                                    " ".repeat(gutter_width)
                                },
                                native_diff_gutter_style(app, line.kind, true).bg(left_struct_bg),
                            ),
                            Span::styled(
                                if first && removed { " -" } else { "  " },
                                Style::default().fg(app.palette.red).bg(left_struct_bg),
                            ),
                            Span::styled(pad_truncate_label(&left_chunk, left_width), left_style),
                            Span::styled("│", Style::default().fg(app.palette.surface_dim)),
                            native_diff_split_rail_span(app, line.kind, false, right_struct_bg),
                            Span::styled(
                                if first {
                                    format_line_number(line.new_line, gutter_width)
                                } else {
                                    " ".repeat(gutter_width)
                                },
                                native_diff_gutter_style(app, line.kind, false).bg(right_struct_bg),
                            ),
                            Span::styled(
                                if first && added { " +" } else { "  " },
                                Style::default().fg(app.palette.green).bg(right_struct_bg),
                            ),
                            Span::styled(
                                pad_truncate_label(&right_chunk, right_width),
                                right_style,
                            ),
                        ]));
                    }
                }
                RenderDiffRow::Fold {
                    key,
                    count,
                    expanded,
                } => {
                    push_native_diff_fold_line(lines, app, key, count, expanded, area_width);
                }
            }
        }
    }
}

enum RenderDiffRow<'a> {
    Line(&'a crate::native_diff::NativeDiffLine),
    Fold {
        key: crate::native_diff::NativeDiffContextKey,
        count: usize,
        expanded: bool,
    },
}

fn native_diff_hunk_rows<'a>(
    diff: &crate::native_diff::NativeDiffPaneState,
    hunk_index: usize,
    hunk: &'a crate::native_diff::NativeDiffHunk,
) -> Vec<RenderDiffRow<'a>> {
    const CONTEXT_EDGE: usize = 3;
    const MIN_FOLD: usize = CONTEXT_EDGE * 2 + 4;

    let mut rows = Vec::new();
    let mut index = 0;
    let mut run_index = 0;
    while index < hunk.lines.len() {
        if hunk.lines[index].kind != crate::native_diff::DiffLineKind::Context {
            rows.push(RenderDiffRow::Line(&hunk.lines[index]));

            index += 1;
            continue;
        }

        let start = index;
        while index < hunk.lines.len()
            && hunk.lines[index].kind == crate::native_diff::DiffLineKind::Context
        {
            index += 1;
        }
        let count = index - start;
        let key = crate::native_diff::NativeDiffContextKey {
            file_index: diff
                .selected_file
                .map(|selection| selection.file_index)
                .unwrap_or_default(),
            hunk_index,
            run_index,
        };
        run_index += 1;
        if count >= MIN_FOLD && diff.context_expanded(key) {
            rows.push(RenderDiffRow::Fold {
                key,
                count,
                expanded: true,
            });
            for line in &hunk.lines[start..index] {
                rows.push(RenderDiffRow::Line(line));
            }
        } else if count >= MIN_FOLD {
            for line in &hunk.lines[start..start + CONTEXT_EDGE] {
                rows.push(RenderDiffRow::Line(line));
            }
            rows.push(RenderDiffRow::Fold {
                key,
                count: count - CONTEXT_EDGE * 2,
                expanded: false,
            });
            for line in &hunk.lines[index - CONTEXT_EDGE..index] {
                rows.push(RenderDiffRow::Line(line));
            }
        } else {
            for line in &hunk.lines[start..index] {
                rows.push(RenderDiffRow::Line(line));
            }
        }
    }
    rows
}

fn native_diff_header_path(file: &crate::native_diff::NativeDiffFile) -> String {
    match (&file.old_path, &file.new_path) {
        (Some(old), Some(new)) if old != new => format!("{} → {}", old.display(), new.display()),
        _ => file
            .new_path
            .as_ref()
            .or(file.old_path.as_ref())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "(unknown)".to_string()),
    }
}

fn push_native_diff_hunk_header(
    lines: &mut Vec<Line<'static>>,
    app: &AppState,
    diff: &crate::native_diff::NativeDiffPaneState,
    hunk_index: usize,
    hunk: &crate::native_diff::NativeDiffHunk,
    accent: Color,
    area_width: u16,
) {
    let bg = if diff.selected_hunk == Some(hunk_index) {
        app.palette.surface0
    } else {
        app.palette.surface_dim
    };
    let marker = if diff.selected_hunk == Some(hunk_index) {
        "▸"
    } else {
        "›"
    };
    let text = format!(
        " {marker} hunk {}  -{},{} +{},{}",
        hunk_index + 1,
        hunk.old_start,
        hunk.old_count,
        hunk.new_start,
        hunk.new_count
    );
    lines.push(Line::from(vec![Span::styled(
        pad_truncate_label(&text, area_width as usize),
        Style::default()
            .fg(accent)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    )]));
}

fn native_diff_rail_span(app: &AppState, kind: crate::native_diff::DiffLineKind) -> Span<'static> {
    match kind {
        crate::native_diff::DiffLineKind::Added => Span::styled(
            "▌",
            Style::default()
                .fg(app.palette.green)
                .bg(native_diff_added_bg(app)),
        ),
        crate::native_diff::DiffLineKind::Removed => Span::styled(
            "▌",
            Style::default()
                .fg(app.palette.red)
                .bg(native_diff_removed_bg(app)),
        ),
        crate::native_diff::DiffLineKind::Context => {
            Span::styled(" ", Style::default().bg(app.palette.panel_bg))
        }
    }
}
fn native_diff_split_rail_span(
    app: &AppState,
    kind: crate::native_diff::DiffLineKind,
    old_side: bool,
    bg: Color,
) -> Span<'static> {
    match (kind, old_side) {
        (crate::native_diff::DiffLineKind::Removed, true) => {
            Span::styled("█", Style::default().fg(app.palette.red).bg(bg))
        }
        (crate::native_diff::DiffLineKind::Added, false) => {
            Span::styled("█", Style::default().fg(app.palette.green).bg(bg))
        }
        (crate::native_diff::DiffLineKind::Added, true)
        | (crate::native_diff::DiffLineKind::Removed, false) => {
            Span::styled("█", Style::default().fg(app.palette.surface_dim).bg(bg))
        }
        _ => Span::styled(" ", Style::default().bg(bg)),
    }
}

fn native_diff_line_bg(app: &AppState, kind: crate::native_diff::DiffLineKind) -> Color {
    match kind {
        crate::native_diff::DiffLineKind::Added => native_diff_added_bg(app),
        crate::native_diff::DiffLineKind::Removed => native_diff_removed_bg(app),
        crate::native_diff::DiffLineKind::Context => app.palette.panel_bg,
    }
}

fn native_diff_added_bg(app: &AppState) -> Color {
    mix_palette_color(app.palette.panel_bg, app.palette.green, 0.16)
}

fn native_diff_removed_bg(app: &AppState) -> Color {
    mix_palette_color(app.palette.panel_bg, app.palette.red, 0.18)
}

fn mix_palette_color(base: Color, tint: Color, amount: f32) -> Color {
    match (base, tint) {
        (Color::Rgb(base_r, base_g, base_b), Color::Rgb(tint_r, tint_g, tint_b)) => Color::Rgb(
            mix_channel(base_r, tint_r, amount),
            mix_channel(base_g, tint_g, amount),
            mix_channel(base_b, tint_b, amount),
        ),
        _ => base,
    }
}

fn mix_channel(base: u8, tint: u8, amount: f32) -> u8 {
    let base = base as f32;
    let tint = tint as f32;
    (base + (tint - base) * amount).round().clamp(0.0, 255.0) as u8
}

fn push_native_diff_fold_line(
    lines: &mut Vec<Line<'static>>,
    app: &AppState,
    key: crate::native_diff::NativeDiffContextKey,
    count: usize,
    expanded: bool,
    area_width: u16,
) {
    let marker = if count == 1 { "line" } else { "lines" };
    let arrow = if expanded { "⌃" } else { "⌄" };
    let action = if expanded { "collapse" } else { "expand" };
    let text = format!("  {arrow} {action} {count} unmodified {marker}");
    let _ = key;
    lines.push(Line::from(vec![Span::styled(
        pad_truncate_label(&text, area_width as usize),
        Style::default()
            .fg(app.palette.subtext0)
            .bg(app.palette.surface0),
    )]));
}

fn split_left_text_width(area_width: u16, gutter_width: usize) -> usize {
    (area_width as usize / 2).saturating_sub(gutter_width + 3)
}

fn split_right_text_width(area_width: u16, gutter_width: usize) -> usize {
    (area_width as usize)
        .saturating_sub(area_width as usize / 2)
        .saturating_sub(gutter_width + 4)
}

fn wrap_native_diff_text(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let width = width.max(1);
    let chars = text.chars().collect::<Vec<_>>();
    chars
        .chunks(width)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect()
}

fn native_diff_gutter_style(
    app: &AppState,
    kind: crate::native_diff::DiffLineKind,
    old_side: bool,
) -> Style {
    match (kind, old_side) {
        (crate::native_diff::DiffLineKind::Removed, true) => Style::default().fg(app.palette.red),
        (crate::native_diff::DiffLineKind::Added, false) => Style::default().fg(app.palette.green),
        _ => Style::default().fg(app.palette.overlay0),
    }
}

fn pad_truncate_label(text: &str, width: usize) -> String {
    let truncated = truncate_label(text, width);
    let pad = width.saturating_sub(truncated.chars().count());
    format!("{truncated}{}", " ".repeat(pad))
}

fn native_diff_line_styles(
    app: &AppState,
    kind: crate::native_diff::DiffLineKind,
) -> (&'static str, Style, Style) {
    match kind {
        crate::native_diff::DiffLineKind::Context => (
            " ",
            Style::default().fg(app.palette.overlay0),
            Style::default().fg(app.palette.text),
        ),
        crate::native_diff::DiffLineKind::Added => (
            "+",
            Style::default()
                .fg(app.palette.green)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(app.palette.green),
        ),
        crate::native_diff::DiffLineKind::Removed => (
            "-",
            Style::default()
                .fg(app.palette.red)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(app.palette.red),
        ),
    }
}

fn line_number_gutter_width(file: &crate::native_diff::NativeDiffFile) -> usize {
    let max_line = file
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .flat_map(|line| [line.old_line, line.new_line])
        .flatten()
        .max()
        .unwrap_or(0);
    max_line.to_string().len().max(2) + 1
}

fn format_line_number(line: Option<usize>, width: usize) -> String {
    match line {
        Some(line) => format!("{line:>width$}"),
        None => " ".repeat(width),
    }
}

fn scroll_metrics(
    total_rows: usize,
    viewport_rows: usize,
    scroll_top: usize,
) -> crate::pane::ScrollMetrics {
    let max_offset = total_rows.saturating_sub(viewport_rows);
    crate::pane::ScrollMetrics {
        offset_from_bottom: max_offset.saturating_sub(scroll_top.min(max_offset)),
        max_offset_from_bottom: max_offset,
        viewport_rows,
    }
}

fn split_scrollbar_area(area: Rect, metrics: crate::pane::ScrollMetrics) -> (Rect, Option<Rect>) {
    if metrics.max_offset_from_bottom == 0 || area.width <= 1 {
        return (area, None);
    }
    (
        Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height),
        Some(Rect::new(
            area.x + area.width.saturating_sub(1),
            area.y,
            1,
            area.height,
        )),
    )
}

fn render_copy_mode_cursor(app: &AppState, frame: &mut Frame, info: &PaneInfo) {
    if app.mode != Mode::Copy {
        return;
    }
    let Some(copy_mode) = app.copy_mode else {
        return;
    };
    if copy_mode.pane_id != info.id
        || copy_mode.cursor_row >= info.inner_rect.height
        || copy_mode.cursor_col >= info.inner_rect.width
    {
        return;
    }

    let x = info.inner_rect.x + copy_mode.cursor_col;
    let y = info.inner_rect.y + copy_mode.cursor_row;
    let cell = &mut frame.buffer_mut()[(x, y)];
    cell.set_style(
        Style::default()
            .fg(panel_contrast_fg(&app.palette))
            .bg(app.active_workspace_accent_color())
            .add_modifier(Modifier::BOLD),
    );
}

fn render_selection_highlight(
    selection: &Option<crate::selection::Selection>,
    frame: &mut Frame,
    pane_id: crate::layout::PaneId,
    inner: Rect,
    scroll_metrics: Option<crate::pane::ScrollMetrics>,
    p: &Palette,
    host_theme: crate::terminal_theme::TerminalTheme,
) {
    if let Some(sel) = selection {
        if sel.is_visible() && sel.pane_id == pane_id {
            let buf = frame.buffer_mut();
            let style = automatic_selection_style(p, host_theme);
            for y in 0..inner.height {
                for x in 0..inner.width {
                    if sel.contains(y, x, scroll_metrics) {
                        let cell = &mut buf[(inner.x + x, inner.y + y)];
                        cell.set_style(style);
                    }
                }
            }
        }
    }
}

type Rgb = (u8, u8, u8);

fn automatic_selection_style(
    p: &Palette,
    host_theme: crate::terminal_theme::TerminalTheme,
) -> Style {
    let bg = automatic_selection_bg(p, host_theme);
    Style::reset().fg(selection_fg_for_bg(bg, p)).bg(bg)
}

fn automatic_selection_bg(p: &Palette, host_theme: crate::terminal_theme::TerminalTheme) -> Color {
    let Some(background) = host_theme.background.map(terminal_theme_to_rgb) else {
        return selection_palette_background(p);
    };

    let target = if relative_luminance(background) < 0.5 {
        (255, 255, 255)
    } else {
        (0, 0, 0)
    };
    let selected = mix_rgb(background, target, 0.28);
    Color::Rgb(selected.0, selected.1, selected.2)
}

fn selection_palette_background(p: &Palette) -> Color {
    if p.panel_bg == Color::Reset {
        p.surface_dim
    } else {
        p.panel_bg
    }
}

fn terminal_theme_to_rgb(color: crate::terminal_theme::RgbColor) -> Rgb {
    (color.r, color.g, color.b)
}

fn selection_fg_for_bg(bg: Color, p: &Palette) -> Color {
    color_to_rgb(bg)
        .map(|bg| {
            if relative_luminance(bg) < 0.5 {
                Color::White
            } else {
                Color::Black
            }
        })
        .unwrap_or_else(|| panel_contrast_fg(p))
}

fn mix_rgb(base: Rgb, target: Rgb, amount: f32) -> Rgb {
    fn channel(base: u8, target: u8, amount: f32) -> u8 {
        (f32::from(base) + (f32::from(target) - f32::from(base)) * amount).round() as u8
    }
    (
        channel(base.0, target.0, amount),
        channel(base.1, target.1, amount),
        channel(base.2, target.2, amount),
    )
}

fn relative_luminance(color: Rgb) -> f32 {
    fn channel(value: u8) -> f32 {
        let value = f32::from(value) / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(color.0) + 0.7152 * channel(color.1) + 0.0722 * channel(color.2)
}

fn color_to_rgb(color: Color) -> Option<Rgb> {
    match color {
        Color::Reset => None,
        Color::Black => Some((0, 0, 0)),
        Color::Red => Some((128, 0, 0)),
        Color::Green => Some((0, 128, 0)),
        Color::Yellow => Some((128, 128, 0)),
        Color::Blue => Some((0, 0, 128)),
        Color::Magenta => Some((128, 0, 128)),
        Color::Cyan => Some((0, 128, 128)),
        Color::Gray => Some((192, 192, 192)),
        Color::DarkGray => Some((128, 128, 128)),
        Color::LightRed => Some((255, 0, 0)),
        Color::LightGreen => Some((0, 255, 0)),
        Color::LightYellow => Some((255, 255, 0)),
        Color::LightBlue => Some((0, 0, 255)),
        Color::LightMagenta => Some((255, 0, 255)),
        Color::LightCyan => Some((0, 255, 255)),
        Color::White => Some((255, 255, 255)),
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Indexed(_) => None,
    }
}

fn render_empty(app: &AppState, frame: &mut Frame, area: Rect) {
    let p = &app.palette;
    let active_workspace_has_no_tabs = app
        .active
        .and_then(|idx| app.workspaces.get(idx))
        .is_some_and(|ws| ws.tabs.is_empty());
    let (title, detail, context, action_label) = if active_workspace_has_no_tabs {
        (
            "  no tabs in this space",
            "  the space is still here.",
            "  create a tab to keep working in this context.",
            app.keybinds
                .new_tab
                .label()
                .unwrap_or_else(|| "unset".to_string()),
        )
    } else if app.workspaces.is_empty() {
        (
            "  no spaces yet",
            "  a space is one project context.",
            "  its root pane sets the default repo or folder name.",
            app.keybinds
                .new_workspace
                .label()
                .unwrap_or_else(|| "unset".to_string()),
        )
    } else if app.group_filter_enabled {
        (
            "  no spaces in this group",
            "  switch groups or create one here.",
            "  hidden spaces stay in the group menu.",
            app.keybinds
                .new_workspace
                .label()
                .unwrap_or_else(|| "unset".to_string()),
        )
    } else {
        (
            "  no active space",
            "  select a space from the sidebar.",
            "  create one if you want a fresh context.",
            app.keybinds
                .new_workspace
                .label()
                .unwrap_or_else(|| "unset".to_string()),
        )
    };
    let lines = vec![
        Line::from(""),
        Line::from(""),
        Line::from(Span::styled(title, Style::default().fg(p.overlay0))),
        Line::from(""),
        Line::from(Span::styled(detail, Style::default().fg(p.overlay1))),
        Line::from(Span::styled(context, Style::default().fg(p.overlay1))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  press ", Style::default().fg(p.overlay0)),
            Span::styled(
                action_label,
                Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to create one", Style::default().fg(p.overlay0)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(p.surface_dim)),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PaneId;
    use crate::selection::Selection;
    use crate::terminal::TerminalRuntime;
    use crate::workspace::Workspace;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    #[test]
    fn pane_border_title_trims_and_truncates() {
        assert_eq!(
            pane_border_title(" claude ", 20).as_deref(),
            Some(" claude ")
        );
        assert_eq!(pane_border_title("", 20), None);
        assert_eq!(pane_border_title("abcdef", 8).as_deref(), Some(" abc… "));
        assert_eq!(pane_border_title("abcdef", 4), None);
    }

    #[test]
    fn main_empty_state_mentions_empty_active_workspace() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("empty");
        workspace.tabs.clear();
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let backend = TestBackend::new(72, 14);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_empty(&app, frame, Rect::new(0, 0, 72, 14)))
            .expect("render empty pane");

        let text = buffer_text(terminal.backend().buffer(), 72, 14);
        assert!(text.contains("no tabs in this space"));
        assert!(text.contains("the space is still here"));
        assert!(text.contains("create a tab to keep working"));
    }

    #[test]
    fn main_empty_state_mentions_empty_group_when_spaces_are_hidden() {
        let mut app = AppState::test_new();
        app.workspaces = vec![Workspace::test_new("hidden")];
        app.create_group("work".to_string());
        app.active_group = 1;
        app.group_filter_enabled = true;
        app.active = None;

        let backend = TestBackend::new(72, 14);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_empty(&app, frame, Rect::new(0, 0, 72, 14)))
            .expect("render empty pane");

        let text = buffer_text(terminal.backend().buffer(), 72, 14);
        assert!(text.contains("no spaces in this group"));
        assert!(text.contains("switch groups or create one here"));
        assert!(text.contains("hidden spaces stay in the group menu"));
    }

    #[tokio::test]
    async fn pane_scrollbar_gutter_is_reserved_before_scrollback_exists() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        let root_pane = workspace.tabs[0].root_pane;
        workspace.tabs[0].runtimes.insert(
            root_pane,
            TerminalRuntime::test_with_scrollback_bytes(40, 8, 1024, b"ready\n"),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(10, 3, 40, 8);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let infos = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            true,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let info = &infos[0];

        assert_eq!(info.rect, area);
        assert_eq!(info.scrollbar_rect, None);
        assert_eq!(
            app.workspaces[0].tabs[0].runtimes[&root_pane].current_size(),
            (area.height, area.width.saturating_sub(1))
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
    #[test]
    fn native_diff_render_includes_line_numbers_and_scrollbars() {
        let app = AppState::test_new();
        let mut diff =
            crate::native_diff::NativeDiffPaneState::new(crate::native_diff::NativeDiffSession {
                repo_root: std::path::PathBuf::from("/repo"),
                files: vec![crate::native_diff::NativeDiffFile {
                    bucket: crate::native_diff::DiffBucket::Changed,
                    old_path: Some(std::path::PathBuf::from("src/lib.rs")),
                    new_path: Some(std::path::PathBuf::from("src/lib.rs")),
                    status: crate::native_diff::DiffFileStatus::Modified,
                    added: 2,
                    deleted: 1,
                    binary: false,
                    hunks: vec![crate::native_diff::NativeDiffHunk {
                        old_start: 10,
                        old_count: 3,
                        new_start: 10,
                        new_count: 4,
                        lines: vec![
                            crate::native_diff::NativeDiffLine {
                                kind: crate::native_diff::DiffLineKind::Context,
                                old_line: Some(10),
                                new_line: Some(10),
                                text: "fn demo() {".to_string(),
                            },
                            crate::native_diff::NativeDiffLine {
                                kind: crate::native_diff::DiffLineKind::Removed,
                                old_line: Some(11),
                                new_line: None,
                                text: "old();".to_string(),
                            },
                            crate::native_diff::NativeDiffLine {
                                kind: crate::native_diff::DiffLineKind::Added,
                                old_line: None,
                                new_line: Some(11),
                                text: "new();".to_string(),
                            },
                            crate::native_diff::NativeDiffLine {
                                kind: crate::native_diff::DiffLineKind::Added,
                                old_line: None,
                                new_line: Some(12),
                                text: "again();".to_string(),
                            },
                        ],
                    }],
                }],
            });
        diff.diff_scroll = 2;
        let backend = TestBackend::new(60, 6);
        let mut terminal = Terminal::new(backend).expect("test backend");

        terminal
            .draw(|frame| {
                render_native_diff_pane(
                    &app,
                    &diff,
                    frame,
                    Rect::new(0, 0, 60, 6),
                    app.palette.accent,
                )
            })
            .expect("render native diff");

        let text = buffer_text(terminal.backend().buffer(), 60, 6);
        assert!(text.contains("src/lib.rs"));
        assert!(text.contains("11    -old();"));
        assert!(text.contains("   11 +new();"));
        assert!(text.contains("▐"));
    }

    #[tokio::test]
    async fn zoomed_pane_scrollbar_gutter_is_reserved_before_scrollback_exists() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        workspace.zoomed = true;
        let root_pane = workspace.tabs[0].root_pane;
        workspace.tabs[0].runtimes.insert(
            root_pane,
            TerminalRuntime::test_with_scrollback_bytes(40, 8, 1024, b"ready\n"),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(10, 3, 40, 8);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let infos = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let info = &infos[0];

        assert_eq!(info.rect, area);
        assert_eq!(info.scrollbar_rect, None);
        assert_eq!(info.inner_rect, Rect::new(10, 3, 39, 8));
    }

    #[tokio::test]
    async fn zoomed_multi_pane_keeps_border_space() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        let focused_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);
        workspace.zoomed = true;
        workspace.tabs[0].runtimes.insert(
            focused_pane,
            TerminalRuntime::test_with_scrollback_bytes(40, 8, 1024, b"ready\n"),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(10, 3, 40, 8);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let infos = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let info = &infos[0];

        assert_eq!(info.id, focused_pane);
        assert_eq!(info.rect, area);
        assert_eq!(info.scrollbar_rect, None);
        assert_eq!(info.inner_rect, Rect::new(11, 4, 37, 6));
    }

    #[tokio::test]
    async fn tiny_pane_does_not_reserve_scrollbar_gutter() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        let root_pane = workspace.tabs[0].root_pane;
        workspace.tabs[0].runtimes.insert(
            root_pane,
            TerminalRuntime::test_with_scrollback_bytes(4, 8, 1024, b"ready\n"),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(10, 3, 4, 8);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let infos = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let info = &infos[0];

        assert_eq!(info.rect, area);
        assert_eq!(info.scrollbar_rect, None);
        assert_eq!(info.inner_rect, area);
    }

    #[tokio::test]
    async fn pane_scrollbar_reserves_last_column_from_terminal_area() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        let root_pane = workspace.tabs[0].root_pane;
        workspace.tabs[0].runtimes.insert(
            root_pane,
            TerminalRuntime::test_with_scrollback_bytes(
                40,
                8,
                1024,
                b"one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
            ),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(10, 3, 40, 8);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let infos = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let info = &infos[0];

        assert_eq!(info.rect, area);
        assert_eq!(info.scrollbar_rect, Some(Rect::new(49, 3, 1, 8)));
        assert_eq!(info.inner_rect, Rect::new(10, 3, 39, 8));
    }

    #[test]
    fn selection_highlight_uses_one_uniform_style() {
        let palette = Palette::catppuccin();
        let host_theme = crate::terminal_theme::TerminalTheme {
            foreground: None,
            background: Some(crate::terminal_theme::RgbColor {
                r: 12,
                g: 14,
                b: 16,
            }),
            ..crate::terminal_theme::TerminalTheme::default()
        };
        let expected_style = automatic_selection_style(&palette, host_theme);
        let selection = Some(Selection::range(PaneId::from_raw(1), 0, 0, 2, None));
        let backend = TestBackend::new(4, 1);
        let mut terminal = Terminal::new(backend).expect("test backend");

        terminal
            .draw(|frame| {
                let buf = frame.buffer_mut();
                buf[(0, 0)].set_style(
                    Style::default()
                        .fg(Color::Rgb(10, 220, 120))
                        .bg(Color::Black),
                );
                buf[(1, 0)].set_style(
                    Style::default()
                        .fg(Color::Rgb(220, 180, 40))
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                );
                buf[(2, 0)].set_style(Style::default().fg(Color::Blue).bg(Color::Reset));
                render_selection_highlight(
                    &selection,
                    frame,
                    PaneId::from_raw(1),
                    Rect::new(0, 0, 4, 1),
                    None,
                    &palette,
                    host_theme,
                );
            })
            .expect("render selection highlight");

        let buffer = terminal.backend().buffer();
        let first = buffer[(0, 0)].style();
        let second = buffer[(1, 0)].style();
        let third = buffer[(2, 0)].style();

        assert_eq!(first.fg, expected_style.fg);
        assert_eq!(second.fg, expected_style.fg);
        assert_eq!(third.fg, expected_style.fg);
        assert_eq!(first.bg, expected_style.bg);
        assert_eq!(second.bg, expected_style.bg);
        assert_eq!(third.bg, expected_style.bg);
        assert_eq!(first.add_modifier, expected_style.add_modifier);
        assert_eq!(second.add_modifier, expected_style.add_modifier);
        assert_eq!(third.add_modifier, expected_style.add_modifier);
        assert!(!second.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn automatic_selection_background_uses_host_background() {
        let bg = automatic_selection_bg(
            &Palette::terminal(),
            crate::terminal_theme::TerminalTheme {
                foreground: Some(crate::terminal_theme::RgbColor {
                    r: 230,
                    g: 230,
                    b: 230,
                }),
                background: Some(crate::terminal_theme::RgbColor {
                    r: 12,
                    g: 14,
                    b: 16,
                }),
                ..crate::terminal_theme::TerminalTheme::default()
            },
        );

        let Color::Rgb(r, g, b) = bg else {
            panic!("selection background should resolve to rgb");
        };
        assert!(relative_luminance((r, g, b)) > relative_luminance((12, 14, 16)));
    }
}
