use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::scrollbar::render_scrollbar;
use super::widgets::{
    modal_close_button_rect, modal_frame_areas, modal_hint_line_count, modal_scroll_area,
    modal_section_heading_style, modal_stack_areas, render_modal_divider, render_modal_frame,
    ModalFrameSpec,
};
use crate::app::{state::ConfigIssue, view_state::ClientViewState, AppState};

const MODAL_WIDTH: u16 = 92;
const MODAL_HEIGHT: u16 = 26;
const HEADER_ROWS: u16 = 4;
const FOOTER_HINTS: &[(&str, &str)] = &[
    ("reload", "r"),
    ("scroll", "wheel / ↑↓"),
    ("jump", "pgup / pgdn"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigDiagnosticsAction {
    Close,
}

fn config_diagnostics_frame_spec() -> ModalFrameSpec<'static> {
    ModalFrameSpec {
        title: "configuration issue",
        width: MODAL_WIDTH,
        height: MODAL_HEIGHT,
        header_rows: HEADER_ROWS,
        footer_hints: FOOTER_HINTS,
        footer_max_rows: 2,
        gap: 1,
        actions_rows: 0,
        show_close: true,
    }
}

pub(crate) fn config_diagnostics_popup_rect(area: Rect) -> Option<Rect> {
    modal_frame_areas(area, config_diagnostics_frame_spec()).map(|areas| areas.popup)
}

pub(crate) fn config_diagnostics_inner_rect(area: Rect) -> Option<Rect> {
    modal_frame_areas(area, config_diagnostics_frame_spec()).map(|areas| areas.inner)
}

fn config_diagnostics_areas(inner: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
    let footer_rows = modal_hint_line_count(inner.width, FOOTER_HINTS, 2);
    let stack = modal_stack_areas(inner, HEADER_ROWS, footer_rows, 0, 1);
    let header = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<4>(stack.header);
    let content = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas::<3>(stack.content);
    (header[0], header[2], header[3], content[0], content[2])
}

fn details_lines<'a>(
    issue: &'a ConfigIssue,
    palette: &crate::app::state::Palette,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    for (index, entry) in issue.entries.iter().enumerate() {
        if index > 0 {
            lines.push(Line::raw(""));
        }
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                entry.number.as_str(),
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(". ", Style::default().fg(palette.overlay1)),
            Span::styled(
                entry.title.as_str(),
                Style::default()
                    .fg(palette.text)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        for detail in &entry.details {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(detail.as_str(), Style::default().fg(palette.subtext0)),
            ]));
        }
    }
    lines
}

fn details_paragraph<'a>(
    issue: &'a ConfigIssue,
    palette: &crate::app::state::Palette,
) -> Paragraph<'a> {
    Paragraph::new(details_lines(issue, palette)).wrap(Wrap { trim: false })
}

pub(crate) fn config_diagnostics_max_scroll(
    area: Rect,
    issue: Option<&ConfigIssue>,
    palette: &crate::app::state::Palette,
) -> u16 {
    let Some(issue) = issue else {
        return 0;
    };
    let Some(inner) = config_diagnostics_inner_rect(area) else {
        return 0;
    };
    let (_, _, _, _, body) = config_diagnostics_areas(inner);
    let line_count = details_paragraph(issue, palette).line_count(body.width.max(1));
    line_count.saturating_sub(body.height as usize) as u16
}

pub(crate) fn config_diagnostics_action_at(
    area: Rect,
    col: u16,
    row: u16,
) -> Option<ConfigDiagnosticsAction> {
    let inner = config_diagnostics_inner_rect(area)?;
    let (header, _, _, _, _) = config_diagnostics_areas(inner);
    let close = modal_close_button_rect(header);
    [(close, ConfigDiagnosticsAction::Close)]
        .into_iter()
        .find_map(|(rect, action)| {
            (col >= rect.x
                && col < rect.x.saturating_add(rect.width)
                && row >= rect.y
                && row < rect.y.saturating_add(rect.height))
            .then_some(action)
        })
}

pub(super) fn render_config_diagnostics_overlay(app: &AppState, frame: &mut Frame) {
    render_config_diagnostics_overlay_from(app, frame, frame.area(), app.config_diagnostics_scroll);
}

pub(super) fn render_config_diagnostics_overlay_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
) {
    render_config_diagnostics_overlay_from(
        app,
        frame,
        view.screen_rect(),
        view.config_diagnostics_scroll,
    );
}

fn render_config_diagnostics_overlay_from(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    scroll: u16,
) {
    let Some(issue) = app.config_issue.as_ref() else {
        return;
    };

    super::dim_background(frame, area);
    let Some(frame_areas) =
        render_modal_frame(frame, area, &app.palette, config_diagnostics_frame_spec())
    else {
        return;
    };
    let inner = frame_areas.inner;
    if inner.width < 24 || inner.height < 8 {
        return;
    }

    let (_, subtitle, divider, heading, body) = config_diagnostics_areas(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " Fix entries below, then reload  ",
                Style::default().fg(app.palette.overlay1),
            ),
            Span::styled(
                " CLI ",
                Style::default()
                    .fg(app.palette.surface0)
                    .bg(app.palette.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  $ omh config check",
                Style::default()
                    .fg(app.palette.text)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        subtitle,
    );
    render_modal_divider(frame, divider, &app.palette);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" diagnostics", modal_section_heading_style(&app.palette)),
            Span::styled(" · ", Style::default().fg(app.palette.overlay0)),
            Span::styled(
                issue.entries.len().to_string(),
                Style::default()
                    .fg(app.palette.subtext0)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        heading,
    );

    let max_scroll = config_diagnostics_max_scroll(area, Some(issue), &app.palette);
    let scroll = scroll.min(max_scroll);
    let line_count = details_paragraph(issue, &app.palette).line_count(body.width.max(1));
    let metrics =
        crate::ui::modal_scroll_metrics(line_count, body.height.max(1) as usize, scroll as usize);
    let scroll_area = modal_scroll_area(body, metrics);
    frame.render_widget(
        details_paragraph(issue, &app.palette).scroll((scroll, 0)),
        scroll_area.body,
    );
    if let Some(track) = scroll_area.track {
        render_scrollbar(
            frame,
            metrics,
            track,
            app.palette.surface_dim,
            app.palette.overlay0,
            "▐",
        );
    }
}
