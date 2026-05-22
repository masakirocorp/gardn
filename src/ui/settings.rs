use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Tabs},
    Frame,
};

use super::scrollbar::render_scrollbar;
use super::widgets::{
    action_button_row_rects, centered_popup_rect, modal_scroll_area, modal_stack_areas,
    panel_contrast_fg, primary_action_style, render_action_button, render_modal_choice_list,
    render_modal_divider, render_modal_header_bar, render_modal_hint_line,
    render_modal_scroll_hints, render_modal_subtitle, render_panel_shell, ActionButtonSpec,
};
use crate::{
    app::{
        state::{Palette, SettingsSection},
        AppState,
    },
    config::{ThemeMode, ToastDelivery},
};

pub(super) fn render_settings_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    use crate::app::state::SettingsSection;

    if app.settings.group_theme_target.is_some() {
        render_group_theme_overlay(app, frame, area);
        return;
    }

    let p = &app.palette;
    let Some(popup) = centered_popup_rect(area, 76, 22) else {
        return;
    };

    super::dim_background(frame, area);

    let Some(inner) = render_panel_shell(frame, popup, p.accent, p.panel_bg) else {
        return;
    };
    if inner.height < 4 || inner.width < 10 {
        return;
    }

    let stack = modal_stack_areas(inner, 3, 2, 0, 1);
    let header_rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<3>(stack.header);

    render_modal_header_bar(frame, header_rows[0], "settings", p, true);

    let tab_labels = SettingsSection::ALL.iter().map(|section| {
        if app.settings_section_has_badge(*section) {
            Line::from(vec![
                Span::styled(
                    "● ",
                    Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
                ),
                Span::raw(section.label()),
            ])
        } else {
            Line::from(section.label())
        }
    });
    let tabs = Tabs::new(tab_labels)
        .select(
            SettingsSection::ALL
                .iter()
                .position(|section| *section == app.settings.section)
                .unwrap_or(0),
        )
        .style(Style::default().fg(p.overlay1))
        .highlight_style(
            Style::default()
                .fg(panel_contrast_fg(p))
                .bg(p.accent)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" ")
        .padding(" ", " ");
    frame.render_widget(tabs, header_rows[1]);

    render_modal_divider(frame, header_rows[2], p);

    let content_area = stack.content;

    match app.settings.section {
        SettingsSection::Theme => {
            render_settings_theme(app, frame, content_area);
        }
        SettingsSection::ThemeMode => {
            render_modal_choice_list(
                frame,
                content_area,
                "theme mode",
                "choose how hako resolves light and dark palettes",
                &[
                    ("system", ThemeMode::System),
                    ("light", ThemeMode::Light),
                    ("dark", ThemeMode::Dark),
                ],
                app.settings
                    .pending_theme_mode
                    .unwrap_or(app.global_theme_mode),
                app.settings.list.selected,
                p,
                2,
            );
        }
        SettingsSection::Sound => {
            render_settings_toggle(
                frame,
                content_area,
                p,
                "sound alerts",
                "play sounds when agents change state in background",
                app.sound_enabled(),
                app.settings.list.selected,
            );
        }
        SettingsSection::Toast => {
            render_modal_choice_list(
                frame,
                content_area,
                "notification popups",
                "choose where background popup notifications should appear",
                &[
                    ("off", ToastDelivery::Off),
                    ("inside hako", ToastDelivery::Hako),
                    ("via terminal", ToastDelivery::Terminal),
                    ("via system", ToastDelivery::System),
                ],
                app.toast_delivery(),
                app.settings.list.selected,
                p,
                2,
            );
        }
        SettingsSection::PaneLabels => {
            render_settings_toggle(
                frame,
                content_area,
                p,
                "agent border labels",
                "show detected agent names in split pane borders",
                app.agent_border_labels_enabled(),
                app.settings.list.selected,
            );
        }
        SettingsSection::Integrations => {
            render_settings_integrations(app, frame, content_area);
        }
    }

    if let Some(footer_area) = stack.footer {
        let footer_rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)])
            .areas::<2>(footer_area);
        let primary_label = settings_primary_button_label(app.settings.section);
        let show_primary = settings_show_primary_action(app);
        let (apply_rect, close_rect) =
            settings_button_rects(inner, app.settings.section, show_primary);
        if let Some(apply_rect) = apply_rect {
            render_action_button(
                frame,
                apply_rect,
                Some("↵"),
                primary_label,
                Style::default()
                    .fg(panel_contrast_fg(p))
                    .bg(p.accent)
                    .add_modifier(Modifier::BOLD),
            );
        }
        render_action_button(
            frame,
            close_rect,
            Some("esc"),
            "close",
            Style::default()
                .fg(p.text)
                .bg(p.surface0)
                .add_modifier(Modifier::BOLD),
        );

        if app.settings.section == SettingsSection::Theme {
            render_modal_scroll_hints(frame, footer_rows[0], p);
        } else {
            render_modal_hint_line(
                frame,
                footer_rows[0],
                p,
                &[("move", "↑↓"), ("section", "tab")],
            );
        }
    }
}

fn render_group_theme_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let p = &app.palette;
    let Some(popup) = centered_popup_rect(area, 56, 20) else {
        return;
    };

    super::dim_background(frame, area);

    let Some(inner) = render_panel_shell(frame, popup, p.accent, p.panel_bg) else {
        return;
    };
    if inner.height < 4 || inner.width < 10 {
        return;
    }

    let stack = modal_stack_areas(inner, 3, 2, 0, 1);
    let header_rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<3>(stack.header);

    render_modal_header_bar(frame, header_rows[0], "group theme", p, true);

    let group_label = app
        .settings
        .group_theme_target
        .and_then(|idx| app.groups.get(idx))
        .map(|group| format!(" {} {}", group.icon, group.name))
        .unwrap_or_else(|| " group".to_string());
    render_modal_subtitle(frame, header_rows[1], group_label, p);

    render_modal_divider(frame, header_rows[2], p);

    render_settings_theme(app, frame, stack.content);

    if let Some(footer_area) = stack.footer {
        let footer_rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)])
            .areas::<2>(footer_area);
        let (apply_rect, _) = settings_button_rects(inner, SettingsSection::Theme, true);
        if let Some(apply_rect) = apply_rect {
            render_action_button(
                frame,
                apply_rect,
                Some("↵"),
                "apply",
                primary_action_style(p),
            );
        }

        render_modal_scroll_hints(frame, footer_rows[0], p);
    }
}

pub(crate) fn settings_primary_button_label(
    section: crate::app::state::SettingsSection,
) -> &'static str {
    match section {
        crate::app::state::SettingsSection::Integrations => "install",
        _ => "apply",
    }
}

pub(crate) fn settings_show_primary_action(app: &AppState) -> bool {
    app.settings.section != crate::app::state::SettingsSection::Integrations
        || app
            .integration_recommendations
            .iter()
            .any(crate::integration::IntegrationRecommendation::needs_install)
}

pub(crate) fn settings_button_rects(
    inner: Rect,
    section: crate::app::state::SettingsSection,
    show_primary: bool,
) -> (Option<Rect>, Rect) {
    if !show_primary {
        let rects = action_button_row_rects(
            inner,
            &[ActionButtonSpec {
                hint: Some("esc"),
                label: "close",
            }],
            2,
            inner.height.saturating_sub(1),
        );
        return (None, rects[0]);
    }

    let rects = action_button_row_rects(
        inner,
        &[
            ActionButtonSpec {
                hint: Some("↵"),
                label: settings_primary_button_label(section),
            },
            ActionButtonSpec {
                hint: Some("esc"),
                label: "close",
            },
        ],
        2,
        inner.height.saturating_sub(1),
    );
    (Some(rects[0]), rects[1])
}

fn render_settings_integrations(app: &AppState, frame: &mut Frame, area: Rect) {
    let p = &app.palette;
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas::<4>(area);

    frame.render_widget(
        Paragraph::new("agent integrations")
            .style(Style::default().fg(p.text).add_modifier(Modifier::BOLD)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(
            "let agents report state directly instead of relying only on process detection",
        )
        .style(Style::default().fg(p.overlay1))
        .wrap(ratatui::widgets::Wrap { trim: false }),
        rows[1],
    );

    let mut lines = Vec::new();
    for item in &app.integration_recommendations {
        let marker = match item.state {
            crate::integration::IntegrationStatusKind::Current => "✓",
            crate::integration::IntegrationStatusKind::Outdated => "↻",
            crate::integration::IntegrationStatusKind::NotInstalled if item.available => "+",
            crate::integration::IntegrationStatusKind::NotInstalled => "–",
        };
        let marker_style = match item.state {
            crate::integration::IntegrationStatusKind::Current => Style::default().fg(p.green),
            crate::integration::IntegrationStatusKind::Outdated => Style::default().fg(p.yellow),
            crate::integration::IntegrationStatusKind::NotInstalled if item.available => {
                Style::default().fg(p.accent)
            }
            crate::integration::IntegrationStatusKind::NotInstalled => {
                Style::default().fg(p.overlay0)
            }
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {marker} "), marker_style),
            Span::styled(
                format!("{:<9}", item.label),
                Style::default().fg(p.subtext0),
            ),
            Span::styled(item.status_label(), Style::default().fg(p.overlay1)),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " no integration targets available",
            Style::default().fg(p.overlay1),
        )));
    }

    if !app.integration_install_messages.is_empty() {
        lines.push(Line::from(""));
        for message in &app.integration_install_messages {
            lines.push(Line::from(Span::styled(
                format!(" {message}"),
                Style::default().fg(p.overlay1),
            )));
        }
    } else {
        lines.push(Line::from(""));
        let found_any = app.integration_recommendations.iter().any(|item| {
            item.available || item.state != crate::integration::IntegrationStatusKind::NotInstalled
        });
        let hint = if app
            .integration_recommendations
            .iter()
            .any(crate::integration::IntegrationRecommendation::needs_install)
        {
            " press install to add available or outdated integrations"
        } else if found_any {
            " all detected integrations are installed"
        } else {
            " no supported agent CLIs found on PATH"
        };
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(p.overlay1),
        )));
    }

    frame.render_widget(Paragraph::new(lines), rows[3]);
}

fn render_settings_theme(app: &AppState, frame: &mut Frame, area: Rect) {
    use crate::app::state::THEME_NAMES;

    let p = &app.palette;
    let mut items: Vec<ListItem> = Vec::new();
    if app.settings.group_theme_target.is_some() {
        let selected = app.settings.list.selected == 0;
        let marker = if selected { " ✓" } else { "" };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("default ({})", app.global_theme_name),
                modal_option_style(p, selected),
            ),
            Span::styled(marker, modal_option_marker_style(p, selected)),
        ])));
    }

    let offset = usize::from(app.settings.group_theme_target.is_some());
    let selected_theme = app
        .settings
        .pending_theme_name
        .as_deref()
        .unwrap_or(&app.global_theme_name);
    items.extend(THEME_NAMES.iter().enumerate().map(|(idx, name)| {
        let selected = app.settings.list.selected == idx + offset;
        let marker = if selected {
            " ✓"
        } else if app.settings.group_theme_target.is_none() && selected_theme == *name {
            " ·"
        } else {
            ""
        };
        ListItem::new(Line::from(vec![
            Span::styled(*name, modal_option_style(p, selected)),
            Span::styled(marker, modal_option_marker_style(p, selected)),
        ]))
    }));

    let total_items = items.len();
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(panel_contrast_fg(p))
                .bg(p.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" ")
        .style(Style::default().fg(p.subtext0));

    let viewport_rows = area.height as usize;
    let metrics = crate::ui::modal_scroll_metrics(total_items, viewport_rows, app.settings.scroll);
    let scroll = metrics
        .max_offset_from_bottom
        .saturating_sub(metrics.offset_from_bottom);
    let scroll_area = modal_scroll_area(area, metrics);

    let mut state = ListState::default()
        .with_selected(Some(app.settings.list.selected))
        .with_offset(scroll);
    frame.render_stateful_widget(list, scroll_area.body, &mut state);
    if let Some(track) = scroll_area.track {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▐");
    }
}

fn render_settings_toggle(
    frame: &mut Frame,
    area: Rect,
    p: &Palette,
    title: &str,
    description: &str,
    current_value: bool,
    selected_idx: usize,
) {
    render_modal_choice_list(
        frame,
        area,
        title,
        description,
        &[("on", true), ("off", false)],
        current_value,
        selected_idx,
        p,
        2,
    );
}

fn modal_option_style(p: &Palette, selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(panel_contrast_fg(p))
            .bg(p.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.text)
    }
}

fn modal_option_marker_style(p: &Palette, selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(panel_contrast_fg(p))
            .bg(p.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.green)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    use super::*;
    use crate::app::state::{AppState, SettingsSection};

    #[test]
    fn group_theme_overlay_uses_focused_title_without_settings_tabs() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("Work".to_string());
        app.settings.group_theme_target = Some(group_idx);
        app.settings.section = SettingsSection::Theme;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, Rect::new(0, 0, 80, 24)))
            .expect("render group theme overlay");

        let text = buffer_text(terminal.backend().buffer(), 80, 24);
        assert!(text.contains("group theme"));
        assert!(text.contains("Work"));
        assert!(!text.contains("sound"));
        assert!(!text.contains("toasts"));
        assert!(!text.contains("pane labels"));
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
