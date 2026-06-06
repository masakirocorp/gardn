use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Tabs},
    Frame,
};

use super::scrollbar::render_scrollbar;
use super::widgets::{
    action_button_row_rects, centered_popup_rect, modal_section_heading_style, modal_stack_areas,
    panel_contrast_fg, render_action_button, render_modal_description, render_modal_divider,
    render_modal_header_bar, render_modal_hint_line, render_panel_shell, ActionButtonSpec,
};
use crate::{
    app::{
        state::{normalize_theme_name, Palette, SettingsSection},
        AppState,
    },
    config::ThemeMode,
    settings_rows::{
        rows_for_section, selected_visual_row, visual_row_count, SettingsListRow,
        SettingsMarkerTone,
    },
};

fn settings_title(app: &AppState) -> &'static str {
    if app.settings.group_settings_target.is_some() {
        "group settings"
    } else {
        "settings"
    }
}

fn settings_sections(app: &AppState) -> &'static [SettingsSection] {
    if app.settings.group_settings_target.is_some() {
        &[SettingsSection::Theme]
    } else {
        SettingsSection::ALL
    }
}

pub(super) fn render_settings_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    use crate::app::state::SettingsSection;

    let group_settings = app.settings.group_settings_target.is_some();

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

    render_modal_header_bar(frame, header_rows[0], settings_title(app), p, false);

    let tab_labels = settings_sections(app).iter().map(|section| {
        if app.settings_section_has_badge(*section) {
            let badge_style = settings_tab_badge_style(p, app.settings.section == *section);
            Line::from(vec![
                Span::styled("● ", badge_style),
                Span::raw(section.label()),
            ])
        } else {
            Line::from(section.label())
        }
    });
    let tabs = Tabs::new(tab_labels)
        .select(
            settings_sections(app)
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
        SettingsSection::Layout => {
            render_settings_layout(app, frame, content_area);
        }
        SettingsSection::Sound | SettingsSection::Toast => {
            render_settings_sectioned_toggle_list(app, frame, content_area);
        }
        SettingsSection::PaneLabels => {
            render_settings_behavior(app, frame, content_area);
        }
        SettingsSection::Experiments => {
            render_settings_experiments(app, frame, content_area);
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
        } else if group_settings {
            render_modal_hint_line(
                frame,
                footer_rows[0],
                p,
                &[("move", "↑↓"), ("select", "space")],
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

    let model_rows = rows_for_section(app, SettingsSection::Integrations).unwrap_or_default();
    let mut lines = Vec::new();
    for row in &model_rows {
        let SettingsListRow::StatusChoice {
            index,
            marker,
            label,
            tone,
        } = row
        else {
            continue;
        };
        let selected = app.settings.list.selected == *index;
        let selected_style = modal_option_style(p, selected);
        let marker_style = if selected {
            selected_style
        } else {
            match tone {
                SettingsMarkerTone::Good => Style::default().fg(p.green),
                SettingsMarkerTone::Warning => Style::default().fg(p.yellow),
                SettingsMarkerTone::Accent => Style::default().fg(p.accent),
                SettingsMarkerTone::Disabled => Style::default().fg(p.overlay0),
            }
        };
        let label_style = if selected {
            selected_style
        } else {
            Style::default().fg(p.subtext0)
        };
        if selected {
            let text = format!(" {marker} {label}");
            lines.push(Line::from(Span::styled(
                format!("{text:<width$}", width = rows[3].width as usize),
                selected_style,
            )));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!(" {marker} "), marker_style),
                Span::styled(label.as_ref(), label_style),
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

    let description = if app.settings.group_settings_target.is_some() {
        "choose an ANSI accent for this group, or inherit the global accent"
    } else if system_source {
        "follow terminal colors directly"
    } else {
        match mode {
            ThemeMode::System => "choose custom palettes for automatic light and dark appearance",
            ThemeMode::Light => "choose the palette hako uses in light appearance",
            ThemeMode::Dark => "choose the palette hako uses in dark appearance",
        }
    };
    render_modal_description(
        frame,
        title_area,
        if app.settings.group_settings_target.is_some() {
            "accent"
        } else {
            "theme"
        },
        modal_section_heading_style(p),
    );
    render_modal_description(
        frame,
        description_area,
        description,
        Style::default().fg(p.overlay1),
    );

    let Some(model_rows) = rows_for_section(app, SettingsSection::Theme) else {
        return;
    };
    let selected_row = selected_visual_row(&model_rows, app.settings.list.selected).unwrap_or(0);
    let mut items: Vec<ListItem> = Vec::with_capacity(model_rows.len());
    let list_width = list_area.width as usize;

    for row in &model_rows {
        match row {
            SettingsListRow::Header(title) => {
                items.push(ListItem::new(Line::from(Span::styled(
                    format!(" {title}"),
                    modal_section_heading_style(p),
                ))));
            }
            SettingsListRow::Spacer => items.push(ListItem::new(Line::from(""))),
            SettingsListRow::Choice { label, checked, .. } => {
                let selected = selected_row == items.len();
                let marker = if *checked { " ✓" } else { "" };
                if selected {
                    let text = format!("  {label}{marker}");
                    items.push(ListItem::new(Line::from(Span::styled(
                        format!("{text:<list_width$}"),
                        modal_option_style(p, true),
                    ))));
                } else {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled(format!("  {label}"), modal_option_style(p, false)),
                        Span::styled(marker.to_string(), modal_option_marker_style(p, false)),
                    ])));
                }
            }
            SettingsListRow::Option { .. } => {}
            SettingsListRow::StatusChoice { .. } => {}
        }
    }

    let total_items = visual_row_count(&model_rows);
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(panel_contrast_fg(p))
                .bg(p.accent)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(p.subtext0));

    let viewport = crate::ui::ModalListViewport::new(
        total_items,
        list_area.height as usize,
        app.settings.scroll,
    );
    let scroll = viewport.scroll();
    let scroll_area = viewport.scroll_area(list_area);
    let metrics = viewport.metrics();
    let viewport_rows = list_area.height as usize;

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

fn render_settings_layout(app: &AppState, frame: &mut Frame, area: Rect) {
    render_settings_sectioned_toggle_list(app, frame, area);
}

fn render_settings_behavior(app: &AppState, frame: &mut Frame, area: Rect) {
    render_settings_sectioned_toggle_list(app, frame, area);
}

fn render_settings_sectioned_toggle_list(app: &AppState, frame: &mut Frame, area: Rect) {
    let p = &app.palette;
    let selected_style = modal_option_style(p, true);
    let list_width = area.width as usize;
    let mut selected_row = None;
    let mut rows = Vec::new();

    let Some(model_rows) = rows_for_section(app, app.settings.section) else {
        return;
    };

    for row in &model_rows {
        match row {
            SettingsListRow::Header(title) => {
                rows.push(ListItem::new(Line::from(Span::styled(
                    format!(" {title}"),
                    modal_section_heading_style(p),
                ))));
            }
            SettingsListRow::Spacer => rows.push(ListItem::new(Line::from(""))),
            SettingsListRow::Option {
                index,
                title,
                description,
                enabled,
            } => {
                let selected = app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len());
                }
                let marker = if *enabled { "●" } else { "○" };
                let marker_style = settings_toggle_marker_style(p, *enabled, selected);

                let item = if selected {
                    ListItem::new(vec![
                        Line::from(vec![
                            Span::styled(marker, marker_style),
                            Span::styled(" ", selected_style),
                            Span::styled(
                                format!("{title:<width$}", width = list_width.saturating_sub(2)),
                                selected_style,
                            ),
                        ]),
                        Line::from(Span::styled(
                            format!(
                                "  {description:<width$}",
                                width = list_width.saturating_sub(2)
                            ),
                            selected_style,
                        )),
                    ])
                } else {
                    ListItem::new(vec![
                        Line::from(vec![
                            Span::styled(marker, marker_style),
                            Span::raw(" "),
                            Span::styled(title.as_ref(), Style::default().fg(p.text)),
                        ]),
                        Line::from(Span::styled(
                            description.as_ref(),
                            Style::default().fg(p.subtext0),
                        )),
                    ])
                };
                rows.push(item);
            }
            SettingsListRow::Choice {
                index,
                label,
                checked,
            } => {
                let selected = app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len());
                }
                let marker = if *checked { " ✓" } else { "" };
                if selected {
                    let text = format!("  {label}{marker}");
                    rows.push(ListItem::new(Line::from(Span::styled(
                        format!("{text:<list_width$}"),
                        selected_style,
                    ))));
                } else {
                    rows.push(ListItem::new(Line::from(vec![
                        Span::styled(format!("  {label}"), modal_option_style(p, false)),
                        Span::styled(marker.to_string(), modal_option_marker_style(p, false)),
                    ])));
                }
            }
            SettingsListRow::StatusChoice { .. } => {}
        }
    }

    let list = List::new(rows).highlight_symbol(" ");
    let mut state = ListState::default().with_selected(selected_row);
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_settings_experiments(app: &AppState, frame: &mut Frame, area: Rect) {
    render_settings_sectioned_toggle_list(app, frame, area);
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

fn settings_tab_badge_style(p: &Palette, selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(panel_contrast_fg(p))
            .bg(p.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
    }
}

fn settings_toggle_marker_style(p: &Palette, enabled: bool, selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(panel_contrast_fg(p))
            .bg(p.accent)
            .add_modifier(Modifier::BOLD)
    } else if enabled {
        Style::default().fg(p.green)
    } else {
        Style::default().fg(p.overlay0)
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
    use crate::{
        app::state::theme_names_for_appearance,
        config::{TerminalAccent, ToastDelivery},
        terminal_theme::ThemeAppearance,
    };

    #[test]
    fn group_settings_overlay_uses_main_settings_layout_with_group_tabs() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("Work".to_string());
        app.settings.group_settings_target = Some(group_idx);
        app.settings.section = SettingsSection::Theme;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, Rect::new(0, 0, 80, 24)))
            .expect("render group settings overlay");

        let text = buffer_text(terminal.backend().buffer(), 80, 24);
        assert!(text.contains("group settings"));
        assert!(text.contains("theme"));
        assert!(text.contains("accent"));
        assert!(!text.contains("sound"));
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
        assert!(text.contains("accent"));
        assert!(text.contains("blue ✓"));
        assert!(text.contains("magenta"));
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
    fn terminal_dark_accent_highlight_starts_on_first_option() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::System);
        app.settings.pending_light_theme_name = Some("system".to_string());
        app.settings.pending_dark_theme_name = Some("system".to_string());
        app.settings.list.selected = 2 + TerminalAccent::ALL.len();

        let area = Rect::new(0, 0, 100, 50);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, area))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let dark_heading_y = text
            .lines()
            .position(|line| line.contains("dark accent"))
            .expect("dark accent heading") as u16;
        let dark_blue_y = text
            .lines()
            .enumerate()
            .skip(dark_heading_y as usize + 1)
            .find_map(|(y, line)| line.contains("blue").then_some(y as u16))
            .expect("dark blue option");
        let selected_row_end = area.x + area.width.saturating_sub(1);

        assert_ne!(
            terminal.backend().buffer()[(selected_row_end, dark_heading_y)]
                .style()
                .bg,
            Some(app.palette.accent)
        );
        assert_eq!(
            terminal.backend().buffer()[(selected_row_end, dark_blue_y)]
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

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let lines: Vec<&str> = text.lines().collect();
        let header_row = lines
            .iter()
            .position(|line| line.contains("notification popups"))
            .expect("notification header row");
        let off_row = lines
            .iter()
            .position(|line| line.contains("off ✓"))
            .expect("off choice row");
        let hako_row = lines
            .iter()
            .position(|line| line.contains("inside hako"))
            .expect("hako choice row");
        let terminal_row = lines
            .iter()
            .position(|line| line.contains("via terminal"))
            .expect("terminal choice row");
        let system_row = lines
            .iter()
            .position(|line| line.contains("via system"))
            .expect("system choice row");

        assert_eq!(off_row, header_row + 1);
        assert_eq!(hako_row, off_row + 1);
        assert_eq!(terminal_row, hako_row + 1);
        assert_eq!(system_row, terminal_row + 1);
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
    fn layout_settings_render_sidebar_widths() {
        let mut app = AppState::test_new();
        app.default_sidebar_width = 26;
        app.sidebar_min_width = 18;
        app.sidebar_max_width = 36;
        app.settings.section = SettingsSection::Layout;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("sidebar"));
        assert!(text.contains("worktrees"));
        assert!(text.contains("● default sidebar width"));
        assert!(text.contains("26 columns"));
        assert!(text.contains("● minimum sidebar width"));
        assert!(text.contains("18 columns"));
        assert!(text.contains("● maximum sidebar width"));
        assert!(text.contains("36 columns"));
        assert!(text.contains("● worktree directory"));
        assert!(text.contains("/tmp/hako-worktrees"));
    }
    #[test]
    fn sectioned_settings_selected_text_uses_selected_foreground() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Layout;
        app.settings.list.selected = 0;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let (selected_y, selected_x) = text
            .lines()
            .enumerate()
            .find_map(|(y, line)| {
                line.find("default sidebar width")
                    .map(|x| (y as u16, x as u16))
            })
            .expect("selected layout row");

        assert_eq!(
            terminal.backend().buffer()[(selected_x, selected_y)]
                .style()
                .fg,
            Some(panel_contrast_fg(&app.palette))
        );
    }

    #[test]
    fn behavior_settings_render_close_prompt_and_agent_labels() {
        let mut app = AppState::test_new();
        app.confirm_close = true;
        app.prompt_new_tab_name = true;
        app.show_agent_labels_on_pane_borders = false;
        app.settings.section = SettingsSection::PaneLabels;
        app.settings.list.selected = 0;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("workspace"));
        assert!(text.contains("terminal"));
        assert!(text.contains("● confirm before closing workspaces"));
        assert!(text.contains("● name new tabs"));
        assert!(text.contains("● new terminal cwd"));
        assert!(text.contains("follow focused pane"));
        assert!(text.contains("● mouse wheel speed"));
        assert!(text.contains("3 lines per wheel notch"));
        assert!(text.contains("○ agent border labels"));
    }

    #[test]
    fn selected_section_markers_use_selected_foreground() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Layout;
        app.settings.list.selected = 0;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let (selected_y, marker_x) =
            find_text_cell(&text, "● default sidebar width").expect("selected layout row");

        assert_eq!(
            terminal.backend().buffer()[(marker_x, selected_y)].symbol(),
            "●"
        );
        assert_eq!(
            terminal.backend().buffer()[(marker_x, selected_y)]
                .style()
                .fg,
            Some(panel_contrast_fg(&app.palette))
        );
    }

    #[test]
    fn selected_disabled_section_markers_use_selected_foreground() {
        let mut app = AppState::test_new();
        app.switch_ascii_input_source_in_prefix = false;
        app.settings.section = SettingsSection::Experiments;
        app.settings.list.selected = 0;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let (selected_y, marker_x) =
            find_text_cell(&text, "○ switch to ascii input source in prefix (macOS)")
                .expect("selected experiment row");

        assert_eq!(
            terminal.backend().buffer()[(marker_x, selected_y)].symbol(),
            "○"
        );
        assert_eq!(
            terminal.backend().buffer()[(marker_x, selected_y)]
                .style()
                .fg,
            Some(panel_contrast_fg(&app.palette))
        );
    }

    #[test]
    fn selected_settings_tab_badge_uses_selected_foreground() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Integrations;
        app.integration_recommendations = vec![crate::integration::IntegrationRecommendation {
            target: crate::api::schema::IntegrationTarget::Omp,
            label: "omp",
            command: "omp",
            available: true,
            path: std::path::PathBuf::from("/tmp/hako-test-omp"),
            state: crate::integration::IntegrationStatusKind::Outdated,
        }];

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let (badge_y, badge_x) =
            find_text_cell(&text, "● integrations").expect("selected integrations badge");
        assert_eq!(
            terminal.backend().buffer()[(badge_x, badge_y)].style().fg,
            Some(panel_contrast_fg(&app.palette))
        );
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

    #[test]
    fn experiments_render_input_source_only() {
        let mut app = AppState::test_new();
        app.switch_ascii_input_source_in_prefix = true;
        app.settings.section = SettingsSection::Experiments;
        app.settings.list.selected = 0;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(!text.contains("restore"));
        assert!(!text.contains("history"));
        assert!(!text.contains("resume agent sessions"));
        assert!(!text.contains("pane screen history"));
        assert!(text.contains("input"));
        assert!(text.contains("● switch to ascii input source in prefix (macOS)"));
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
    fn find_text_cell(text: &str, needle: &str) -> Option<(u16, u16)> {
        text.lines().enumerate().find_map(|(y, line)| {
            let byte_x = line.find(needle)?;
            let cell_x = line[..byte_x].chars().count();
            Some((y as u16, cell_x as u16))
        })
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
