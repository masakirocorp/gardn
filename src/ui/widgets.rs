use std::ops::Range;

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

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

pub(super) fn render_modal_scroll_hints(frame: &mut Frame, area: Rect, p: &Palette) {
    render_modal_hint_line(
        frame,
        area,
        p,
        &[("scroll", "wheel ↑↓"), ("jump", "pgup / pgdn")],
    );
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
        let prefix_width = if current.is_empty() { 1 } else { 5 };
        let hint_width = prefix_width + hint.0.chars().count() + 1 + hint.1.chars().count();
        let would_overflow = !current.is_empty()
            && current_width + hint_width > max_width
            && lines.len() + 1 < max_rows as usize;
        if would_overflow {
            lines.push(current);
            current = Vec::new();
            current_width = 0;
        }

        let prefix_width = if current.is_empty() { 1 } else { 5 };
        current_width += prefix_width + hint.0.chars().count() + 1 + hint.1.chars().count();
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
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(format!(" {value}█")).style(Style::default().fg(p.text).bg(p.surface0)),
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
    action_button_text(hint, label).chars().count() as u16
}

pub(crate) struct ActionButtonSpec<'a> {
    pub hint: Option<&'a str>,
    pub label: &'a str,
}

pub(crate) fn action_button_row_rects(
    area: Rect,
    buttons: &[ActionButtonSpec<'_>],
    gap: u16,
    row_offset: u16,
) -> Vec<Rect> {
    let widths: Vec<u16> = buttons
        .iter()
        .map(|button| action_button_width(button.hint, button.label))
        .collect();
    centered_button_row(area, &widths, gap, row_offset)
}

pub(super) fn render_action_button(
    frame: &mut Frame,
    rect: Rect,
    hint: Option<&str>,
    label: &str,
    style: Style,
) {
    frame.render_widget(
        Paragraph::new(action_button_text(hint, label))
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
