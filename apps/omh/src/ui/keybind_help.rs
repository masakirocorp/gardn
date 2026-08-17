use std::borrow::Cow;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::scrollbar::render_scrollbar;
use super::widgets::{
    modal_close_button_rect, modal_frame_areas, modal_scroll_area, modal_scroll_hint_line_count,
    modal_scroll_metrics, render_modal_frame, render_modal_subtitle, ModalFrameAreas,
    ModalFrameSpec, MODAL_SCROLL_HINTS,
};
use crate::app::{view_state::ClientViewState, AppState};

pub(super) type HelpEntry = (String, Cow<'static, str>);
pub(super) type HelpGroup = (&'static str, Vec<HelpEntry>);

fn help_entry(key: impl Into<String>, label: &'static str) -> HelpEntry {
    (key.into(), Cow::Borrowed(label))
}

fn keybind_label(bindings: &crate::config::ActionKeybinds) -> String {
    bindings.label().unwrap_or_else(|| "Unset".to_string())
}

fn indexed_label(bindings: &[crate::config::IndexedKeybind]) -> String {
    if bindings.is_empty() {
        return "Unset".to_string();
    }

    let range_label = match bindings.len() {
        10 => Some("1..0"),
        9 => Some("1..9"),
        _ => None,
    };
    if let Some(range_label) = range_label {
        let first = &bindings[0].label;
        let last_digit = range_label.chars().last().unwrap_or('9');
        let last = &bindings[bindings.len() - 1].label;
        if first.ends_with('1') && last.ends_with(last_digit) {
            return format!("{}{}", first.trim_end_matches('1'), range_label);
        }
    }

    bindings
        .iter()
        .map(|binding| binding.label.clone())
        .collect::<Vec<_>>()
        .join(" / ")
}

pub(super) fn keybind_help_groups(app: &AppState) -> Vec<HelpGroup> {
    let kb = &app.keybinds;
    let mut groups = Vec::new();

    groups.push((
        "Global",
        vec![
            help_entry(
                crate::config::format_key_combo((app.prefix_code, app.prefix_mods)),
                "Prefix Mode",
            ),
            help_entry(keybind_label(&kb.help), "Keybinds"),
            help_entry(keybind_label(&kb.settings), "Settings"),
            help_entry(keybind_label(&kb.detach), "Detach"),
            help_entry(keybind_label(&kb.reload_config), "Reload Config"),
            help_entry(
                keybind_label(&kb.open_notification_target),
                "Open Notification Target",
            ),
            help_entry(keybind_label(&kb.command_palette), "Command Palette"),
        ],
    ));

    groups.push((
        "Navigation",
        vec![
            help_entry("Esc", "Back"),
            help_entry(
                format!(
                    "{} / {}",
                    keybind_label(&kb.navigate.workspace_up),
                    keybind_label(&kb.navigate.workspace_down)
                ),
                "Space List",
            ),
            help_entry(
                format!(
                    "{} / {} / {} / {} / left / right",
                    keybind_label(&kb.navigate.pane_left),
                    keybind_label(&kb.navigate.pane_down),
                    keybind_label(&kb.navigate.pane_up),
                    keybind_label(&kb.navigate.pane_right)
                ),
                "Move Focus",
            ),
            help_entry("Tab / Shift+Tab", "Cycle Pane"),
            help_entry("Enter", "Open Workspace"),
            help_entry("1..0", "Switch Workspace"),
        ],
    ));

    let workspace_tab = vec![
        help_entry(keybind_label(&kb.workspace_picker), "Workspace Navigation"),
        help_entry(keybind_label(&kb.new_workspace), "New Workspace"),
        help_entry(keybind_label(&kb.rename_workspace), "Rename Workspace"),
        help_entry(keybind_label(&kb.close_workspace), "Close Workspace"),
        help_entry(keybind_label(&kb.previous_workspace), "Previous Workspace"),
        help_entry(keybind_label(&kb.next_workspace), "Next Workspace"),
        help_entry(indexed_label(&kb.switch_workspace), "Switch Space 1-10"),
        help_entry(keybind_label(&kb.previous_agent), "Previous Agent"),
        help_entry(keybind_label(&kb.next_agent), "Next Agent"),
        help_entry(indexed_label(&kb.focus_agent), "Focus Agent 1-9"),
        help_entry(keybind_label(&kb.new_tab), "New Tab"),
        help_entry(keybind_label(&kb.take_control), "Take Tab Control"),
        help_entry(keybind_label(&kb.rename_tab), "Rename Tab"),
        help_entry(keybind_label(&kb.previous_tab), "Previous Tab"),
        help_entry(keybind_label(&kb.next_tab), "Next Tab"),
        help_entry(indexed_label(&kb.switch_tab), "Switch Tab 1-10"),
        help_entry(keybind_label(&kb.close_tab), "Close Tab"),
    ];
    groups.push(("Workspaces / Tabs", workspace_tab));

    let group_keys = vec![
        help_entry(keybind_label(&kb.open_group_menu), "Open Group Menu"),
        help_entry(keybind_label(&kb.new_group), "New Group"),
        help_entry(keybind_label(&kb.rename_group), "Rename Group"),
        help_entry(keybind_label(&kb.delete_group), "Delete Group"),
        help_entry(
            keybind_label(&kb.toggle_group_filter),
            "Toggle Current/All Groups",
        ),
        help_entry(keybind_label(&kb.previous_group), "Previous Group"),
        help_entry(keybind_label(&kb.next_group), "Next Group"),
        help_entry(indexed_label(&kb.switch_group), "Switch Group 1-10"),
    ];
    groups.push(("Groups", group_keys));

    let agents = vec![
        help_entry(keybind_label(&kb.open_agent_menu), "Open Agent Menu"),
        help_entry(keybind_label(&kb.previous_agent), "Previous Agent"),
        help_entry(keybind_label(&kb.next_agent), "Next Agent"),
    ];
    groups.push(("Agents", agents));

    let panes = vec![
        help_entry(keybind_label(&kb.split_vertical), "Split Vertical"),
        help_entry(keybind_label(&kb.split_horizontal), "Split Horizontal"),
        help_entry(keybind_label(&kb.close_pane), "Close Pane"),
        help_entry(keybind_label(&kb.rename_pane), "Rename Pane"),
        help_entry(keybind_label(&kb.edit_scrollback), "Edit Scrollback"),
        help_entry(keybind_label(&kb.copy_mode), "Copy Mode"),
        help_entry(keybind_label(&kb.zoom), "Zoom Pane"),
        help_entry(keybind_label(&kb.resize_mode), "Resize Mode"),
        help_entry(keybind_label(&kb.toggle_sidebar), "Toggle Sidebar"),
        help_entry(keybind_label(&kb.toggle_context_bar), "Toggle Context Bar"),
        help_entry(keybind_label(&kb.zen_mode), "Toggle Zen Mode"),
        help_entry(keybind_label(&kb.focus_pane_left), "Focus Pane Left"),
        help_entry(keybind_label(&kb.focus_pane_down), "Focus Pane Down"),
        help_entry(keybind_label(&kb.focus_pane_up), "Focus Pane Up"),
        help_entry(keybind_label(&kb.focus_pane_right), "Focus Pane Right"),
        help_entry(keybind_label(&kb.cycle_pane_next), "Cycle Pane Next"),
        help_entry(
            keybind_label(&kb.cycle_pane_previous),
            "Cycle Pane Previous",
        ),
        help_entry(keybind_label(&kb.last_pane), "Last Pane"),
        help_entry(
            keybind_label(&kb.toggle_right_sidebar),
            "Toggle Right Sidebar",
        ),
    ];
    groups.push(("Panes", panes));

    if !kb.custom_commands.is_empty() {
        groups.push((
            "Custom",
            kb.custom_commands
                .iter()
                .map(|binding| {
                    (
                        binding.label.clone(),
                        binding
                            .description
                            .clone()
                            .map(Cow::Owned)
                            .unwrap_or(Cow::Borrowed("Custom Command")),
                    )
                })
                .collect(),
        ));
    }

    groups
}

pub(crate) fn keybind_help_lines(
    app: &AppState,
    option_width: usize,
) -> Vec<(usize, Line<'static>)> {
    let heading_style = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let shortcut_style = Style::default()
        .fg(app.palette.mauve)
        .add_modifier(Modifier::BOLD);
    let unset_style = Style::default().fg(app.palette.overlay0);
    let label_style = Style::default().fg(app.palette.text);

    let groups = keybind_help_groups(app);
    let mut lines = Vec::new();

    for (group_index, (group, entries)) in groups.into_iter().enumerate() {
        if group_index > 0 {
            lines.push((0, Line::raw("")));
        }
        lines.push((
            group.len() + 1,
            Line::from(vec![Span::styled(format!(" {group}"), heading_style)]),
        ));
        for (shortcut, label) in entries {
            let label = format!("  {label}");
            let label_width = label.chars().count();
            let shortcut_width = shortcut.chars().count();
            let minimum_width = label_width + 1 + shortcut_width;
            let gap = option_width
                .saturating_sub(label_width + shortcut_width)
                .max(1);
            let width = minimum_width.max(option_width);
            let shortcut_style = if shortcut == "Unset" {
                unset_style
            } else {
                shortcut_style
            };
            lines.push((
                width,
                Line::from(vec![
                    Span::styled(label, label_style),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(shortcut, shortcut_style),
                ]),
            ));
        }
    }

    lines
}

fn keybind_help_height(area: Rect) -> u16 {
    let popup_width = 76.min(area.width.saturating_sub(4));
    let inner_width = popup_width.saturating_sub(2);
    21 + modal_scroll_hint_line_count(inner_width, 2)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct KeybindHelpLayout {
    pub popup: Rect,
    pub close: Rect,
    pub body: Rect,
}

fn keybind_help_frame_spec(area: Rect) -> ModalFrameSpec<'static> {
    ModalFrameSpec {
        title: "Keybinds",
        width: 76,
        height: keybind_help_height(area),
        header_rows: 2,
        footer_hints: MODAL_SCROLL_HINTS,
        footer_max_rows: 2,
        gap: 1,
        actions_rows: 0,
        show_close: true,
    }
}

fn keybind_help_layout_from_frame(frame: ModalFrameAreas) -> Option<KeybindHelpLayout> {
    if frame.inner.height < 6 || frame.inner.width < 20 {
        return None;
    }
    let header = Rect::new(frame.header.x, frame.header.y, frame.header.width, 1);
    Some(KeybindHelpLayout {
        popup: frame.popup,
        close: modal_close_button_rect(header),
        body: frame.content,
    })
}

pub(crate) fn keybind_help_layout(area: Rect) -> Option<KeybindHelpLayout> {
    let frame = modal_frame_areas(area, keybind_help_frame_spec(area))?;
    keybind_help_layout_from_frame(frame)
}

pub(crate) fn keybind_help_scroll_metrics(
    app: &AppState,
    body: Rect,
    scroll: u16,
) -> crate::pane::ScrollMetrics {
    let viewport_rows = body.height.max(1) as usize;
    let rows_for_width = |wrap_width: usize| {
        let option_width = wrap_width.saturating_sub(1).max(1);
        keybind_help_lines(app, option_width)
            .iter()
            .map(|(width, _)| width.max(&1).div_ceil(wrap_width.max(1)))
            .sum::<usize>()
    };
    let full_width = body.width.max(1) as usize;
    let initial_rows = rows_for_width(full_width);
    let wrap_width = if initial_rows > viewport_rows && full_width > 1 {
        body.width.saturating_sub(1).max(1) as usize
    } else {
        full_width
    };
    modal_scroll_metrics(rows_for_width(wrap_width), viewport_rows, scroll as usize)
}

pub(crate) fn keybind_help_scrollbar_rect(
    body: Rect,
    metrics: crate::pane::ScrollMetrics,
) -> Option<Rect> {
    modal_scroll_area(body, metrics).track
}

pub(super) fn render_keybind_help_overlay(app: &AppState, frame: &mut Frame) {
    render_keybind_help_overlay_from(app, frame, frame.area(), app.keybind_help.scroll);
}

pub(super) fn render_keybind_help_overlay_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
) {
    render_keybind_help_overlay_from(app, frame, view.screen_rect(), view.keybind_help.scroll);
}

fn render_keybind_help_overlay_from(app: &AppState, frame: &mut Frame, area: Rect, scroll: u16) {
    super::dim_background(frame, area);

    let spec = keybind_help_frame_spec(area);
    let Some(frame_areas) = render_modal_frame(frame, area, &app.palette, spec) else {
        return;
    };
    let Some(layout) = keybind_help_layout_from_frame(frame_areas) else {
        return;
    };
    let header_rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)])
        .areas::<2>(frame_areas.header);
    render_modal_subtitle(
        frame,
        header_rows[1],
        "Available commands and configured shortcuts",
        &app.palette,
    );

    let metrics = keybind_help_scroll_metrics(app, layout.body, scroll);
    let scroll_area = modal_scroll_area(layout.body, metrics);
    let option_width = scroll_area.body.width.saturating_sub(1).max(1) as usize;
    let lines = keybind_help_lines(app, option_width);

    let body = Paragraph::new(lines.into_iter().map(|(_, line)| line).collect::<Vec<_>>())
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(body, scroll_area.body);
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
