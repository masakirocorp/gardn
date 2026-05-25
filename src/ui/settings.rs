use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Tabs},
    Frame,
};

use super::scrollbar::render_scrollbar;
use super::widgets::{
    action_button_row_rects, centered_popup_rect, modal_close_button_rect, modal_scroll_area,
    modal_section_heading_style, modal_stack_areas, panel_contrast_fg, primary_action_style,
    render_action_button, render_modal_choice_list, render_modal_description, render_modal_divider,
    render_modal_header_bar, render_modal_hint_line, render_modal_scroll_hints,
    render_modal_subtitle, render_panel_shell, secondary_action_style, ActionButtonSpec,
};
use crate::{
    app::{
        state::{
            normalize_theme_name, theme_names_for_appearance, Palette, SettingsSection, THEME_NAMES,
        },
        AppState,
    },
    config::{ThemeMode, ToastDelivery},
    terminal_theme::ThemeAppearance,
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

    render_modal_header_bar(frame, header_rows[0], "settings", p, false);

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
        SettingsSection::Sound => {
            render_settings_toggle(
                frame,
                content_area,
                p,
                "sound alerts",
                "play sounds when agents change state in background",
                app.settings
                    .pending_sound_enabled
                    .unwrap_or_else(|| app.sound_enabled()),
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
                app.settings
                    .pending_toast_delivery
                    .unwrap_or_else(|| app.toast_delivery()),
                app.settings.list.selected,
                p,
                1,
            );
        }
        SettingsSection::PaneLabels => {
            render_settings_toggle(
                frame,
                content_area,
                p,
                "agent border labels",
                "show detected agent names in split pane borders",
                app.settings
                    .pending_agent_border_labels
                    .unwrap_or_else(|| app.agent_border_labels_enabled()),
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
            "cancel",
            Style::default()
                .fg(p.text)
                .bg(p.surface0)
                .add_modifier(Modifier::BOLD),
        );

        if app.settings.section == SettingsSection::Integrations {
            render_modal_hint_line(
                frame,
                footer_rows[0],
                p,
                &[("move", "↑↓"), ("action", "space/↵"), ("section", "tab")],
            );
        } else {
            render_modal_hint_line(
                frame,
                footer_rows[0],
                p,
                &[("move", "↑↓"), ("select", "space"), ("section", "tab")],
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

    render_modal_header_bar(frame, header_rows[0], "group theme", p, false);
    render_action_button(
        frame,
        modal_close_button_rect(header_rows[0]),
        Some("esc"),
        "cancel",
        secondary_action_style(p),
    );

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
                "save",
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
        _ => "save",
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
                label: "cancel",
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
                label: "cancel",
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
    for (idx, item) in app.integration_recommendations.iter().enumerate() {
        let selected = app.settings.list.selected == idx;
        let selected_style = modal_option_style(p, selected);
        let marker = match item.state {
            crate::integration::IntegrationStatusKind::Current => "✓",
            crate::integration::IntegrationStatusKind::Outdated => "↻",
            crate::integration::IntegrationStatusKind::NotInstalled if item.available => "+",
            crate::integration::IntegrationStatusKind::NotInstalled => "–",
        };
        let marker_style = if selected {
            selected_style
        } else {
            match item.state {
                crate::integration::IntegrationStatusKind::Current => Style::default().fg(p.green),
                crate::integration::IntegrationStatusKind::Outdated => {
                    Style::default().fg(p.yellow)
                }
                crate::integration::IntegrationStatusKind::NotInstalled if item.available => {
                    Style::default().fg(p.accent)
                }
                crate::integration::IntegrationStatusKind::NotInstalled => {
                    Style::default().fg(p.overlay0)
                }
            }
        };
        let label_style = if selected {
            selected_style
        } else {
            Style::default().fg(p.subtext0)
        };
        let status_style = if selected {
            selected_style
        } else {
            Style::default().fg(p.overlay1)
        };
        if selected {
            let text = format!(" {marker} {:<9}{}", item.label, item.status_label());
            lines.push(Line::from(Span::styled(
                format!("{text:<width$}", width = rows[3].width as usize),
                selected_style,
            )));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!(" {marker} "), marker_style),
                Span::styled(format!("{:<9}", item.label), label_style),
                Span::styled(item.status_label(), status_style),
            ]));
        }
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
        let hint = if let Some(item) = app
            .integration_recommendations
            .get(app.settings.list.selected)
        {
            match item.state {
                crate::integration::IntegrationStatusKind::Current => {
                    " press enter to uninstall selected integration"
                }
                crate::integration::IntegrationStatusKind::Outdated => {
                    " press enter to update selected integration"
                }
                crate::integration::IntegrationStatusKind::NotInstalled if item.available => {
                    " press enter to install selected integration"
                }
                crate::integration::IntegrationStatusKind::NotInstalled => {
                    " selected integration is unavailable"
                }
            }
        } else if app
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

fn theme_display_name(name: &str) -> &str {
    match name {
        "catppuccin-latte" => "catppuccin latte",
        "tokyo-night-day" => "tokyo night day",
        "gruvbox-light" => "gruvbox",
        "one-light" => "one",
        "solarized-light" => "solarized",
        "kanagawa-lotus" => "kanagawa lotus",
        "rose-pine-dawn" => "rose pine dawn",
        "tokyo-night" => "tokyo night",
        "one-dark" => "one dark",
        "rose-pine" => "rose pine",
        other => other,
    }
}
fn theme_mode_display_name(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::System => "automatic",
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
    }
}

fn render_settings_theme(app: &AppState, frame: &mut Frame, area: Rect) {
    let p = &app.palette;
    let [desc_area, _, list_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(2),
    ])
    .areas::<3>(area);
    let [title_area, description_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas::<2>(desc_area);

    let mode = app
        .settings
        .pending_theme_mode
        .unwrap_or(app.global_theme_mode);
    let pending_light_theme = app
        .settings
        .pending_light_theme_name
        .as_deref()
        .unwrap_or(&app.global_light_theme_name);
    let pending_dark_theme = app
        .settings
        .pending_dark_theme_name
        .as_deref()
        .unwrap_or(&app.global_dark_theme_name);
    let system_source = mode == ThemeMode::System
        && normalize_theme_name(pending_light_theme) == "system"
        && normalize_theme_name(pending_dark_theme) == "system";
    let description = if app.settings.group_theme_target.is_some() {
        "choose a theme override for this group"
    } else if system_source {
        "follow terminal colors directly"
    } else {
        match mode {
            ThemeMode::System => "choose custom palettes for automatic light and dark appearance",
            ThemeMode::Light => "choose the palette hako uses in light appearance",
            ThemeMode::Dark => "choose the palette hako uses in dark appearance",
        }
    };
    render_modal_description(frame, title_area, "theme", modal_section_heading_style(p));
    render_modal_description(
        frame,
        description_area,
        description,
        Style::default().fg(p.overlay1),
    );

    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_row = app.settings.list.selected;
    let list_width = list_area.width as usize;
    let option_item = |label: &str, marker: &str, selected: bool, indent: &str| {
        if selected {
            let text = format!("{indent}{label}{marker}");
            ListItem::new(Line::from(Span::styled(
                format!("{text:<list_width$}"),
                modal_option_style(p, true),
            )))
        } else {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{indent}{label}"), modal_option_style(p, false)),
                Span::styled(marker.to_string(), modal_option_marker_style(p, false)),
            ]))
        }
    };

    if app.settings.group_theme_target.is_some() {
        let pending_group_theme_name = app.settings.pending_theme_name.as_deref();
        let selected = app.settings.list.selected == 0;
        let marker = if pending_group_theme_name.is_none() {
            " ✓"
        } else {
            ""
        };
        items.push(option_item(
            &format!("default ({})", theme_display_name(&app.global_theme_name)),
            marker,
            selected,
            " ",
        ));

        items.extend(THEME_NAMES.iter().enumerate().map(|(idx, name)| {
            let selected = app.settings.list.selected == idx + 1;
            let marker = if pending_group_theme_name == Some(*name) {
                " ✓"
            } else {
                ""
            };
            option_item(theme_display_name(name), marker, selected, " ")
        }));
    } else {
        selected_row = if app.settings.list.selected < 2 {
            app.settings.list.selected + 1
        } else if system_source {
            1
        } else if app.settings.list.selected < 2 + ThemeMode::ALL.len() {
            app.settings.list.selected - 2 + 5
        } else {
            let theme_idx = app.settings.list.selected - 2 - ThemeMode::ALL.len();
            match mode {
                ThemeMode::System => {
                    let light_len = theme_names_for_appearance(ThemeAppearance::Light).len();
                    if theme_idx < light_len {
                        theme_idx + 10
                    } else {
                        theme_idx + 12
                    }
                }
                ThemeMode::Light | ThemeMode::Dark => theme_idx + 10,
            }
        };

        items.push(ListItem::new(Line::from(Span::styled(
            " colors",
            modal_section_heading_style(p),
        ))));
        let selected = selected_row == items.len();
        let marker = if system_source { " ✓" } else { "" };
        items.push(option_item("terminal", marker, selected, "  "));
        let selected = selected_row == items.len();
        let marker = if !system_source { " ✓" } else { "" };
        items.push(option_item("palettes", marker, selected, "  "));

        if !system_source {
            items.push(ListItem::new(Line::from("")));
            items.push(ListItem::new(Line::from(Span::styled(
                " appearance",
                modal_section_heading_style(p),
            ))));
            for candidate in ThemeMode::ALL {
                let selected = selected_row == items.len();
                let marker = if mode == *candidate { " ✓" } else { "" };
                items.push(option_item(
                    theme_mode_display_name(*candidate),
                    marker,
                    selected,
                    "  ",
                ));
            }

            items.push(ListItem::new(Line::from("")));
            match mode {
                ThemeMode::System => {
                    let light_names = theme_names_for_appearance(ThemeAppearance::Light);
                    items.push(ListItem::new(Line::from(Span::styled(
                        " light appearance",
                        modal_section_heading_style(p),
                    ))));
                    for name in light_names {
                        let selected = selected_row == items.len();
                        let marker = if pending_light_theme == *name {
                            " ✓"
                        } else {
                            ""
                        };
                        items.push(option_item(
                            theme_display_name(name),
                            marker,
                            selected,
                            "  ",
                        ));
                    }

                    items.push(ListItem::new(Line::from("")));
                    items.push(ListItem::new(Line::from(Span::styled(
                        " dark appearance",
                        modal_section_heading_style(p),
                    ))));
                    for name in theme_names_for_appearance(ThemeAppearance::Dark) {
                        let selected = selected_row == items.len();
                        let marker = if pending_dark_theme == *name {
                            " ✓"
                        } else {
                            ""
                        };
                        items.push(option_item(
                            theme_display_name(name),
                            marker,
                            selected,
                            "  ",
                        ));
                    }
                }
                ThemeMode::Light => {
                    items.push(ListItem::new(Line::from(Span::styled(
                        " light appearance",
                        modal_section_heading_style(p),
                    ))));
                    for name in theme_names_for_appearance(ThemeAppearance::Light) {
                        let selected = selected_row == items.len();
                        let marker = if pending_light_theme == *name {
                            " ✓"
                        } else {
                            ""
                        };
                        items.push(option_item(
                            theme_display_name(name),
                            marker,
                            selected,
                            "  ",
                        ));
                    }
                }
                ThemeMode::Dark => {
                    items.push(ListItem::new(Line::from(Span::styled(
                        " dark appearance",
                        modal_section_heading_style(p),
                    ))));
                    for name in theme_names_for_appearance(ThemeAppearance::Dark) {
                        let selected = selected_row == items.len();
                        let marker = if pending_dark_theme == *name {
                            " ✓"
                        } else {
                            ""
                        };
                        items.push(option_item(
                            theme_display_name(name),
                            marker,
                            selected,
                            "  ",
                        ));
                    }
                }
            }
        }
    }

    let total_items = items.len();
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(panel_contrast_fg(p))
                .bg(p.accent)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(p.subtext0));

    let viewport_rows = list_area.height as usize;
    let metrics = crate::ui::modal_scroll_metrics(total_items, viewport_rows, app.settings.scroll);
    let scroll = metrics
        .max_offset_from_bottom
        .saturating_sub(metrics.offset_from_bottom);
    let scroll_area = modal_scroll_area(list_area, metrics);

    let selected =
        (selected_row >= scroll && selected_row < scroll + viewport_rows).then_some(selected_row);
    let mut state = ListState::default()
        .with_selected(selected)
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
        1,
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

    #[test]
    fn theme_settings_light_mode_only_lists_light_themes() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::Light;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::Light);
        app.settings.pending_light_theme_name = Some("catppuccin-latte".to_string());

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("catppuccin latte"));
        assert!(text.contains("solarized"));
        assert!(!text.contains("dracula"));
        assert!(!text.contains("nord"));
        assert!(!text.contains("vesper"));
        assert_no_option_line(&text, "terminal");
    }

    #[test]
    fn theme_settings_dark_mode_only_lists_dark_themes() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::Dark;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::Dark);
        app.settings.pending_dark_theme_name = Some("catppuccin".to_string());

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("catppuccin"));
        assert!(text.contains("dracula"));
        assert!(!text.contains("catppuccin latte"));
        assert_no_option_line(&text, "terminal");
    }

    #[test]
    fn theme_settings_system_source_hides_appearance_sections() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::System);
        app.settings.pending_light_theme_name = Some("system".to_string());
        app.settings.pending_dark_theme_name = Some("system".to_string());

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("terminal ✓"));
        assert!(text.contains("palettes"));
        assert!(text.contains("colors"));
        assert!(!text.contains("light appearance"));
        assert!(!text.contains("dark appearance"));
    }

    #[test]
    fn theme_settings_system_mode_lists_light_and_dark_selections() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::System);
        app.settings.pending_light_theme_name = Some("solarized-light".to_string());
        app.settings.pending_dark_theme_name = Some("rose-pine".to_string());

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("terminal"));
        assert!(text.contains("palettes ✓"));
        assert!(text.contains("appearance"));
        assert!(text.contains("automatic ✓"));
        assert!(!text.contains("system terminal"));
        assert!(!text.contains(" mode"));
        assert!(text.contains("light appearance"));
        assert!(text.contains("solarized"));
        assert_no_option_line(&text, "terminal");

        app.settings.list.selected = theme_names_for_appearance(ThemeAppearance::Light).len();
        app.settings.scroll = theme_names_for_appearance(ThemeAppearance::Light).len() + 3;
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("dark appearance"));
        assert!(text.contains("rose pine"));
    }

    #[test]
    fn theme_settings_system_scroll_can_reveal_dark_section_while_mode_selected() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::System);
        app.settings.pending_light_theme_name = Some("solarized-light".to_string());
        app.settings.pending_dark_theme_name = Some("rose-pine".to_string());
        app.settings.list.selected = 0;
        app.settings.scroll = theme_names_for_appearance(ThemeAppearance::Light).len() + 2;

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("dark appearance"));
        assert!(text.contains("rose pine"));
    }

    #[test]
    fn theme_settings_marks_pending_values_not_hovered_cursor_values() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.global_light_theme_name = "catppuccin-latte".to_string();
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::Light);
        app.settings.pending_light_theme_name = Some("solarized-light".to_string());
        app.settings.list.selected = 1;

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(!text.contains("automatic ✓"));
        assert!(text.contains("light ✓"));
        assert!(!text.contains("catppuccin latte ✓"));
        assert!(text.contains("solarized ✓"));
    }

    #[test]
    fn theme_settings_selected_row_highlight_extends_to_row_end() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::Light);
        app.settings.list.selected = 3;

        let area = Rect::new(0, 0, 100, 50);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, area))
            .expect("render theme settings");

        let selected_row_y = 9;
        let selected_row_end = area.x + area.width.saturating_sub(1);
        assert_eq!(
            terminal.backend().buffer()[(selected_row_end, selected_row_y)]
                .style()
                .bg,
            Some(app.palette.accent)
        );
    }
    #[test]
    fn theme_settings_selected_row_does_not_shift_text() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::Light);
        app.settings.list.selected = 3;

        let area = Rect::new(0, 0, 100, 50);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, area))
            .expect("render theme settings");

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 9)].symbol(), " ");
        assert_eq!(buffer[(1, 9)].symbol(), " ");
        assert_eq!(buffer[(2, 9)].symbol(), "l");
    }

    #[test]
    fn settings_choice_tabs_use_single_row_options() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Toast;
        app.settings.pending_toast_delivery = Some(ToastDelivery::Off);

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let popup = centered_popup_rect(area, 76, 22).expect("popup");
        let inner = Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        let content = modal_stack_areas(inner, 3, 2, 0, 1).content;
        let list_y = content.y + 3;
        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(content.x + 1, list_y)].symbol(), "o");
        assert_eq!(buffer[(content.x + 1, list_y + 1)].symbol(), "i");
        assert_eq!(buffer[(content.x + 1, list_y + 2)].symbol(), "v");
        assert_eq!(buffer[(content.x + 1, list_y + 3)].symbol(), "v");
    }

    #[test]
    fn settings_renders_single_escape_cancel_label() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Theme;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert_eq!(text.matches("esc cancel").count(), 1);
        assert!(!text.contains("esc close"));
    }

    #[test]
    fn settings_primary_action_is_save_not_apply() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Theme;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("↵ save"));
        assert!(!text.contains("↵ apply"));
    }
    #[test]
    fn integrations_selected_row_highlight_extends_to_row_end() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Integrations;
        app.settings.list.selected = 0;
        app.integration_recommendations = vec![crate::integration::IntegrationRecommendation {
            target: crate::api::schema::IntegrationTarget::Omp,
            label: "omp",
            command: "omp",
            available: true,
            path: std::path::PathBuf::from("/tmp/hako-test-omp"),
            state: crate::integration::IntegrationStatusKind::NotInstalled,
        }];

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let popup = centered_popup_rect(area, 76, 22).expect("popup");
        let inner = Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        let content = modal_stack_areas(inner, 3, 2, 0, 1).content;
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas::<4>(content);
        let selected_row_end = rows[3].x + rows[3].width.saturating_sub(1);

        assert_eq!(
            terminal.backend().buffer()[(selected_row_end, rows[3].y)]
                .style()
                .bg,
            Some(app.palette.accent)
        );
    }

    fn assert_no_option_line(text: &str, option: &str) {
        let mut in_appearance_section = false;
        for line in text.lines() {
            let line = line.trim();
            if line == "light appearance" || line == "dark appearance" {
                in_appearance_section = true;
                continue;
            }
            if in_appearance_section && line.is_empty() {
                in_appearance_section = false;
                continue;
            }
            assert!(
                !in_appearance_section || (line != option && line != format!("{option} ✓")),
                "unexpected appearance option line {option:?} in:\n{text}"
            );
        }
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
