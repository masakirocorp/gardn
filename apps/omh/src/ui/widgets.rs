use std::ops::Range;

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use super::text::{display_width, display_width_u16, truncate_end, truncate_start};
use crate::app::state::Palette;

pub(super) fn render_panel_shell(
    frame: &mut Frame,
    area: Rect,
    border_color: Color,
    bg: Color,
) -> Option<Rect> {
    if area.width < 2 || area.height < 2 {
        return None;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .border_set(ratatui::symbols::border::PLAIN)
        .style(Style::default().bg(bg));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    Some(inner)
}

pub(super) fn fill_rect(frame: &mut Frame, area: Rect, style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            buf[(x, y)].set_symbol(" ");
            buf[(x, y)].set_style(style);
        }
    }
}

pub(super) fn panel_contrast_fg(p: &Palette) -> Color {
    match p.panel_bg {
        Color::Reset => p.surface_dim,
        color => color,
    }
}

pub(crate) fn centered_popup_rect(area: Rect, popup_w: u16, popup_h: u16) -> Option<Rect> {
    let popup_w = popup_w.min(area.width.saturating_sub(4));
    let popup_h = popup_h.min(area.height.saturating_sub(2));
    if popup_w < 4 || popup_h < 4 {
        return None;
    }

    let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    Some(Rect::new(popup_x, popup_y, popup_w, popup_h))
}

pub(super) fn render_modal_shell(
    frame: &mut Frame,
    area: Rect,
    popup_w: u16,
    popup_h: u16,
    p: &Palette,
) -> Option<Rect> {
    let popup = centered_popup_rect(area, popup_w, popup_h)?;
    render_panel_shell(frame, popup, p.accent, p.panel_bg)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModalFrameAreas {
    pub popup: Rect,
    pub inner: Rect,
    pub header: Rect,
    pub content: Rect,
    pub footer: Option<Rect>,
    pub actions: Option<Rect>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModalFrameSpec<'a> {
    pub title: &'a str,
    pub width: u16,
    pub height: u16,
    pub header_rows: u16,
    pub footer_hints: &'a [(&'a str, &'a str)],
    pub footer_max_rows: u16,
    pub gap: u16,
    pub actions_rows: u16,
    pub show_close: bool,
}

pub(crate) fn modal_frame_areas(area: Rect, spec: ModalFrameSpec<'_>) -> Option<ModalFrameAreas> {
    let popup = centered_popup_rect(area, spec.width, spec.height)?;
    if popup.width < 4 || popup.height < 4 {
        return None;
    }
    let inner = Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );
    let footer_rows = if spec.footer_hints.is_empty() {
        0
    } else {
        modal_hint_line_count(inner.width, spec.footer_hints, spec.footer_max_rows)
    };
    let stack = modal_stack_areas(
        inner,
        spec.header_rows,
        footer_rows,
        spec.actions_rows,
        spec.gap,
    );
    Some(ModalFrameAreas {
        popup,
        inner,
        header: stack.header,
        footer: stack.footer,
        content: stack.content,
        actions: stack.actions,
    })
}

pub(crate) fn render_modal_frame(
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
    spec: ModalFrameSpec<'_>,
) -> Option<ModalFrameAreas> {
    let areas = modal_frame_areas(area, spec)?;
    render_panel_shell(frame, areas.popup, palette.accent, palette.panel_bg)?;
    let header = Rect::new(areas.header.x, areas.header.y, areas.header.width, 1);
    render_modal_header_bar(frame, header, spec.title, palette, spec.show_close);
    if let Some(footer) = areas.footer {
        render_modal_hint_lines(
            frame,
            footer,
            palette,
            spec.footer_hints,
            spec.footer_max_rows,
        );
    }
    Some(areas)
}

pub(super) fn render_modal_header(frame: &mut Frame, area: Rect, title: &str, p: &Palette) {
    let line = Line::from(vec![Span::styled(
        format!(" {title}"),
        Style::default().fg(p.text).add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(Paragraph::new(line), area);
}

pub(crate) fn modal_close_button_rect(area: Rect) -> Rect {
    let width = action_button_width(Some("esc"), "close");
    Rect::new(area.x + area.width.saturating_sub(width), area.y, width, 1)
}

pub(super) fn render_modal_header_bar(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    p: &Palette,
    show_close: bool,
) {
    let close = show_close.then(|| modal_close_button_rect(area));
    let title_width = close
        .map(|button| button.x.saturating_sub(area.x).saturating_sub(1))
        .unwrap_or(area.width);
    render_modal_header(
        frame,
        Rect::new(area.x, area.y, title_width, area.height),
        title,
        p,
    );
    if let Some(button) = close {
        render_action_button(
            frame,
            button,
            Some("esc"),
            "close",
            secondary_action_style(p),
        );
    }
}

pub(super) fn render_modal_subtitle(
    frame: &mut Frame,
    area: Rect,
    text: impl Into<String>,
    p: &Palette,
) {
    let text = text.into();
    frame.render_widget(
        Paragraph::new(format!(" {}", text.trim_start())).style(Style::default().fg(p.overlay1)),
        area,
    );
}

pub(super) fn modal_section_heading_style(p: &Palette) -> Style {
    Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
}

pub(super) fn render_modal_divider(frame: &mut Frame, area: Rect, p: &Palette) {
    frame.render_widget(
        Paragraph::new(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(p.surface0),
        )),
        area,
    );
}

pub(super) const MODAL_SCROLL_HINTS: &[(&str, &str)] =
    &[("scroll", "wheel ↑↓"), ("jump", "pgup / pgdn")];

pub(super) fn modal_scroll_hint_line_count(area_width: u16, max_rows: u16) -> u16 {
    modal_hint_line_count(area_width, MODAL_SCROLL_HINTS, max_rows)
}

pub(super) fn render_modal_scroll_hints(frame: &mut Frame, area: Rect, p: &Palette) {
    render_modal_hint_lines(frame, area, p, MODAL_SCROLL_HINTS, 2);
}

pub(super) fn modal_hint_line_count(area_width: u16, hints: &[(&str, &str)], max_rows: u16) -> u16 {
    if max_rows <= 1 {
        return 1;
    }

    let mut line_count = 1u16;
    let mut current_width = 0usize;
    let max_width = area_width as usize;

    for hint in hints {
        let prefix_width = if current_width == 0 { 1 } else { 5 };
        let hint_width = prefix_width + display_width(hint.0) + 1 + display_width(hint.1);
        let would_overflow =
            current_width != 0 && current_width + hint_width > max_width && line_count < max_rows;
        if would_overflow {
            line_count += 1;
            current_width = 0;
        }

        let prefix_width = if current_width == 0 { 1 } else { 5 };
        current_width += prefix_width + display_width(hint.0) + 1 + display_width(hint.1);
    }

    line_count
}

pub(super) fn render_modal_hint_lines(
    frame: &mut Frame,
    area: Rect,
    p: &Palette,
    hints: &[(&str, &str)],
    max_rows: u16,
) {
    if max_rows <= 1 || area.height <= 1 {
        render_modal_hint_line(frame, area, p, hints);
        return;
    }

    let mut lines: Vec<Vec<(&str, &str)>> = Vec::new();
    let mut current = Vec::new();
    let mut current_width = 0usize;
    let max_width = area.width as usize;

    for hint in hints.iter().copied() {
        let prefix_width = if current_width == 0 { 1 } else { 5 };
        let hint_width = prefix_width + display_width(hint.0) + 1 + display_width(hint.1);
        let would_overflow = current_width != 0
            && current_width + hint_width > max_width
            && lines.len() + 1 < max_rows as usize;
        if would_overflow {
            lines.push(current);
            current = Vec::new();
            current_width = 0;
        }

        let prefix_width = if current_width == 0 { 1 } else { 5 };
        current_width += prefix_width + display_width(hint.0) + 1 + display_width(hint.1);
        current.push(hint);
    }

    if !current.is_empty() {
        lines.push(current);
    }

    for (idx, line_hints) in lines.into_iter().take(max_rows as usize).enumerate() {
        let row = Rect::new(area.x, area.y + idx as u16, area.width, 1);
        render_modal_hint_line(frame, row, p, &line_hints);
    }
}
pub(super) fn render_modal_hint_line(
    frame: &mut Frame,
    area: Rect,
    p: &Palette,
    hints: &[(&str, &str)],
) {
    let mut spans = Vec::new();
    for (idx, (label, keys)) in hints.iter().enumerate() {
        if idx == 0 {
            spans.push(Span::styled(" ", Style::default().fg(p.overlay0)));
        } else {
            spans.push(Span::styled("  ·  ", Style::default().fg(p.overlay0)));
        }
        spans.push(Span::styled(*label, Style::default().fg(p.overlay0)));
        spans.push(Span::styled(" ", Style::default().fg(p.overlay0)));
        spans.push(Span::styled(*keys, Style::default().fg(p.text)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub(super) fn render_modal_text_input(frame: &mut Frame, area: Rect, value: &str, p: &Palette) {
    let value_width = area.width.saturating_sub(2) as usize;
    let visible = truncate_start(value, value_width);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(format!(" {visible}█")).style(Style::default().fg(p.text).bg(p.surface0)),
        area,
    );
}

pub(super) fn render_modal_text_value(frame: &mut Frame, area: Rect, value: &str, p: &Palette) {
    let visible = truncate_end(value, area.width.saturating_sub(1) as usize);
    frame.render_widget(
        Paragraph::new(format!(" {visible}")).style(Style::default().fg(p.text)),
        area,
    );
}

pub(super) fn primary_action_style(p: &Palette) -> Style {
    Style::default()
        .fg(panel_contrast_fg(p))
        .bg(p.accent)
        .add_modifier(Modifier::BOLD)
}

pub(super) fn secondary_action_style(p: &Palette) -> Style {
    Style::default()
        .fg(p.text)
        .bg(p.surface0)
        .add_modifier(Modifier::BOLD)
}

pub(super) fn danger_action_style(p: &Palette) -> Style {
    Style::default()
        .fg(panel_contrast_fg(p))
        .bg(p.red)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn modal_option_line<'a>(
    title: &str,
    metadata: Vec<Span<'a>>,
    width: usize,
    title_style: Style,
    row_style: Style,
) -> Line<'a> {
    let title_width = width.saturating_sub(2);
    let metadata_width = metadata
        .iter()
        .map(|span| display_width(span.content.as_ref()))
        .sum::<usize>();
    let show_metadata = metadata_width > 0 && metadata_width + 3 <= width;
    let available_title = if show_metadata {
        width.saturating_sub(metadata_width + 3)
    } else {
        title_width
    };
    let title = truncate_end(title, available_title);
    let used_title = display_width(&title);
    let mut spans = vec![
        Span::styled("  ", row_style),
        Span::styled(title, title_style),
    ];

    if show_metadata {
        spans.push(Span::styled(
            " ".repeat(width.saturating_sub(2 + used_title + metadata_width)),
            row_style,
        ));
        spans.extend(metadata);
    } else {
        spans.push(Span::styled(
            " ".repeat(width.saturating_sub(2 + used_title)),
            row_style,
        ));
    }
    Line::from(spans)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModalListGeometry {
    pub rect: Rect,
    pub viewport: ModalListViewport,
    pub scroll_area: ModalScrollArea,
}

impl ModalListGeometry {
    pub(crate) fn new(rect: Rect, total_rows: usize, scroll: usize) -> Self {
        let viewport = ModalListViewport::new(total_rows, rect.height as usize, scroll);
        let scroll_area = viewport.scroll_area(rect);
        Self {
            rect,
            viewport,
            scroll_area,
        }
    }

    pub(crate) fn visible_range(self) -> Range<usize> {
        self.viewport.visible_range()
    }

    pub(crate) fn hit_visual_row(self, col: u16, row: u16) -> Option<usize> {
        self.viewport.hit_visual_row(self.rect, col, row)
    }

    pub(crate) fn metrics(self) -> crate::pane::ScrollMetrics {
        self.viewport.metrics()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModalScrollArea {
    pub body: Rect,
    pub track: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModalListViewport {
    total_rows: usize,
    viewport_rows: usize,
    scroll: usize,
}

impl ModalListViewport {
    pub(crate) fn new(total_rows: usize, viewport_rows: usize, scroll: usize) -> Self {
        Self {
            total_rows,
            viewport_rows,
            scroll: scroll.min(total_rows.saturating_sub(viewport_rows)),
        }
    }

    pub(crate) fn scroll(self) -> usize {
        self.scroll
    }

    pub(crate) fn max_scroll(self) -> usize {
        self.total_rows.saturating_sub(self.viewport_rows)
    }

    pub(crate) fn visible_range(self) -> Range<usize> {
        if self.viewport_rows == 0 {
            return self.scroll..self.scroll;
        }

        let end = self
            .scroll
            .saturating_add(self.viewport_rows)
            .min(self.total_rows);
        self.scroll..end
    }

    pub(crate) fn ensure_visible(self, selected_row: usize, context_row: Option<usize>) -> usize {
        if self.viewport_rows == 0 || self.total_rows == 0 {
            return 0;
        }

        let selected_row = selected_row.min(self.total_rows - 1);
        let context_row = context_row
            .unwrap_or(selected_row)
            .min(selected_row)
            .min(self.total_rows - 1);
        let max_scroll = self.max_scroll();
        let scroll = if context_row < self.scroll {
            context_row
        } else if selected_row >= self.scroll.saturating_add(self.viewport_rows) {
            selected_row + 1 - self.viewport_rows
        } else {
            self.scroll
        };
        scroll.min(max_scroll)
    }

    pub(crate) fn metrics(self) -> crate::pane::ScrollMetrics {
        modal_scroll_metrics(self.total_rows, self.viewport_rows, self.scroll)
    }

    pub(crate) fn scroll_from_offset_from_bottom(self, offset_from_bottom: usize) -> usize {
        modal_scroll_from_offset_from_bottom(
            self.total_rows,
            self.viewport_rows,
            offset_from_bottom,
        )
    }

    pub(crate) fn scroll_area(self, area: Rect) -> ModalScrollArea {
        modal_scroll_area(area, self.metrics())
    }

    pub(crate) fn hit_visual_row(self, area: Rect, col: u16, row: u16) -> Option<usize> {
        if area.height == 0
            || area.width == 0
            || row < area.y
            || row >= area.y + area.height
            || col < area.x
            || col >= area.x + area.width
        {
            return None;
        }

        if modal_scrollbar_rect(area, self.metrics()).is_some_and(|track| {
            col >= track.x
                && col < track.x + track.width
                && row >= track.y
                && row < track.y + track.height
        }) {
            return None;
        }

        let visual_row = self.scroll + row.saturating_sub(area.y) as usize;
        (visual_row < self.total_rows).then_some(visual_row)
    }
}

pub(crate) fn modal_scroll_area(
    area: Rect,
    metrics: crate::pane::ScrollMetrics,
) -> ModalScrollArea {
    let track = modal_scrollbar_rect(area, metrics);
    let body = if track.is_some() {
        Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height)
    } else {
        area
    };
    ModalScrollArea { body, track }
}

pub(crate) fn modal_scroll_metrics(
    total_rows: usize,
    viewport_rows: usize,
    scroll: usize,
) -> crate::pane::ScrollMetrics {
    let viewport_rows = viewport_rows.max(1);
    let max_offset_from_bottom = total_rows.saturating_sub(viewport_rows);
    let scroll = scroll.min(max_offset_from_bottom);
    crate::pane::ScrollMetrics {
        offset_from_bottom: max_offset_from_bottom.saturating_sub(scroll),
        max_offset_from_bottom,
        viewport_rows,
    }
}

pub(crate) fn modal_scroll_from_offset_from_bottom(
    total_rows: usize,
    viewport_rows: usize,
    offset_from_bottom: usize,
) -> usize {
    let max_scroll = total_rows.saturating_sub(viewport_rows.max(1));
    max_scroll.saturating_sub(offset_from_bottom.min(max_scroll))
}

pub(crate) fn modal_scrollbar_rect(
    area: Rect,
    metrics: crate::pane::ScrollMetrics,
) -> Option<Rect> {
    (metrics.max_offset_from_bottom > 0 && area.width > 1).then_some(Rect::new(
        area.x + area.width.saturating_sub(1),
        area.y,
        1,
        area.height,
    ))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModalStackAreas {
    pub header: Rect,
    pub content: Rect,
    pub footer: Option<Rect>,
    pub actions: Option<Rect>,
}

pub(crate) fn modal_stack_areas(
    inner: Rect,
    header_height: u16,
    footer_height: u16,
    actions_height: u16,
    gap: u16,
) -> ModalStackAreas {
    #[derive(Clone, Copy)]
    enum Slot {
        Header,
        Content,
        Footer,
        Actions,
    }

    let mut constraints = Vec::new();
    let mut slots = Vec::new();
    let mut push = |slot: Slot, constraint: Constraint| {
        if !slots.is_empty() {
            constraints.push(Constraint::Length(gap));
        }
        constraints.push(constraint);
        slots.push(slot);
    };

    push(Slot::Header, Constraint::Length(header_height));
    push(Slot::Content, Constraint::Min(0));
    if footer_height > 0 {
        push(Slot::Footer, Constraint::Length(footer_height));
    }
    if actions_height > 0 {
        push(Slot::Actions, Constraint::Length(actions_height));
    }

    let areas = Layout::vertical(constraints).split(inner);
    let mut header = Rect::default();
    let mut content = Rect::default();
    let mut footer = None;
    let mut actions = None;

    for (slot, area) in slots.into_iter().zip(areas.iter().step_by(2).copied()) {
        match slot {
            Slot::Header => header = area,
            Slot::Content => content = area,
            Slot::Footer => footer = Some(area),
            Slot::Actions => actions = Some(area),
        }
    }

    ModalStackAreas {
        header,
        content,
        footer,
        actions,
    }
}

pub(crate) fn action_button_text(hint: Option<&str>, label: &str) -> String {
    match hint {
        Some(hint) => format!(" {hint} {label} "),
        None => format!(" {label} "),
    }
}

pub(crate) fn action_button_width(hint: Option<&str>, label: &str) -> u16 {
    display_width_u16(&action_button_text(hint, label))
}

pub(crate) struct ActionButtonSpec<'a> {
    pub hint: Option<&'a str>,
    pub label: &'a str,
}

pub(crate) fn action_button_row_height(
    area_width: u16,
    buttons: &[ActionButtonSpec<'_>],
    gap: u16,
) -> u16 {
    let fits = |hinted: bool| {
        let total = buttons
            .iter()
            .map(|button| {
                action_button_width(hinted.then_some(button.hint).flatten(), button.label)
            })
            .sum::<u16>()
            .saturating_add(gap.saturating_mul(buttons.len().saturating_sub(1) as u16));
        total <= area_width
    };
    if fits(true) || fits(false) {
        1
    } else {
        buttons.len().min(u16::MAX as usize) as u16
    }
}

pub(crate) fn action_button_row_rects(
    area: Rect,
    buttons: &[ActionButtonSpec<'_>],
    gap: u16,
    row_offset: u16,
) -> Vec<Rect> {
    let full_widths = buttons
        .iter()
        .map(|button| action_button_width(button.hint, button.label))
        .collect::<Vec<_>>();
    let compact_widths = buttons
        .iter()
        .map(|button| action_button_width(None, button.label))
        .collect::<Vec<_>>();
    let total_width = |widths: &[u16]| {
        widths
            .iter()
            .copied()
            .sum::<u16>()
            .saturating_add(gap.saturating_mul(widths.len().saturating_sub(1) as u16))
    };
    if action_button_row_height(area.width, buttons, gap) == 1 {
        if total_width(&full_widths) <= area.width {
            return centered_button_row(area, &full_widths, gap, row_offset);
        }
        return centered_button_row(area, &compact_widths, gap, row_offset);
    }
    let end_y = area.y + row_offset.min(area.height.saturating_sub(1));
    let start_y = end_y.saturating_sub(compact_widths.len().saturating_sub(1) as u16);
    compact_widths
        .into_iter()
        .enumerate()
        .map(|(idx, width)| {
            let width = width.min(area.width);
            Rect::new(
                area.x + area.width.saturating_sub(width) / 2,
                start_y.saturating_add(idx as u16),
                width,
                1,
            )
        })
        .collect()
}

pub(super) fn render_action_button(
    frame: &mut Frame,
    rect: Rect,
    hint: Option<&str>,
    label: &str,
    style: Style,
) {
    let visible_hint = (action_button_width(hint, label) <= rect.width)
        .then_some(hint)
        .flatten();
    frame.render_widget(
        Paragraph::new(action_button_text(visible_hint, label))
            .style(style)
            .alignment(Alignment::Center),
        rect,
    );
}

pub(crate) fn render_modal_description(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    frame.render_widget(
        Paragraph::new(format!(" {text}"))
            .style(style)
            .wrap(Wrap { trim: false }),
        area,
    );
}

pub(super) fn centered_button_row(
    inner: Rect,
    widths: &[u16],
    gap: u16,
    row_offset: u16,
) -> Vec<Rect> {
    let total_w = widths
        .iter()
        .copied()
        .sum::<u16>()
        .saturating_add(gap.saturating_mul(widths.len().saturating_sub(1) as u16));
    let mut x = inner.x + inner.width.saturating_sub(total_w) / 2;
    let y = inner.y + row_offset.min(inner.height.saturating_sub(1));
    widths
        .iter()
        .map(|w| {
            let rect = Rect::new(
                x,
                y,
                (*w).min(inner.width.saturating_sub(x.saturating_sub(inner.x))),
                1,
            );
            x = x.saturating_add(*w).saturating_add(gap);
            rect
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    use super::*;

    #[test]
    fn modal_option_line_keeps_unicode_title_and_metadata_within_width() {
        let line = modal_option_line(
            "開発チームのとても長い名前",
            vec![Span::raw("alt+1")],
            20,
            Style::default(),
            Style::default(),
        );
        let rendered = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert_eq!(display_width(&rendered), 20);
        assert!(rendered.ends_with("alt+1"));
        assert!(rendered.contains('…'));
    }

    #[test]
    fn narrow_action_buttons_stack_without_overlapping() {
        let area = Rect::new(4, 7, 8, 2);
        let buttons = [
            ActionButtonSpec {
                hint: Some("enter"),
                label: "confirm",
            },
            ActionButtonSpec {
                hint: Some("esc"),
                label: "cancel",
            },
        ];

        assert_eq!(action_button_row_height(area.width, &buttons, 2), 2);
        let rects = action_button_row_rects(area, &buttons, 2, 1);
        assert_eq!(rects.len(), 2);
        assert_ne!(rects[0].y, rects[1].y);
        assert!(rects.iter().all(|rect| area.contains(rect.as_position())));
        assert!(rects.iter().all(|rect| rect.width > 0));
    }

    #[test]
    fn modal_text_input_keeps_the_end_of_long_unicode_values_visible() {
        let app = crate::app::state::AppState::test_new();
        let backend = TestBackend::new(14, 1);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| {
                render_modal_text_input(
                    frame,
                    Rect::new(0, 0, 14, 1),
                    "前方の長い名前-visible",
                    &app.palette,
                );
            })
            .expect("render text input");
        let rendered = (0..14)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
            .collect::<String>();

        assert!(rendered.contains("…"));
        assert!(rendered.contains("visible"));
    }
}
