use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::scrollbar::render_scrollbar;
use super::widgets::{
    modal_scroll_area, modal_stack_areas, render_modal_header_bar, render_modal_scroll_hints,
    render_modal_shell, render_modal_subtitle,
};
use crate::app::AppState;

fn optional_keybind_label(label: &Option<String>) -> String {
    label.clone().unwrap_or_else(|| "unset".to_string())
}

pub(super) fn keybind_help_groups(
    app: &AppState,
) -> Vec<(&'static str, Vec<(String, &'static str)>)> {
    let kb = &app.keybinds;
    let mut groups = Vec::new();

    groups.push((
        "global",
        vec![
            (
                crate::config::format_key_combo((app.prefix_code, app.prefix_mods)),
                "navigate mode",
            ),
            ("prefix + ?".to_string(), "keybinds"),
            (
                optional_keybind_label(&kb.reload_config_label),
                "reload config",
            ),
            (kb.command_palette_label.clone(), "command palette"),
        ],
    ));

    groups.push((
        "navigation",
        vec![
            ("esc".to_string(), "back"),
            ("↑ / ↓".to_string(), "space list"),
            ("h j k l / arrows".to_string(), "move focus"),
            ("tab / shift+tab".to_string(), "cycle pane"),
            ("enter".to_string(), "open space"),
            ("s".to_string(), "settings"),
            ("q".to_string(), "quit"),
        ],
    ));

    let mut workspace_tab = vec![
        (kb.new_workspace_label.clone(), "new space"),
        (kb.rename_workspace_label.clone(), "rename space"),
        (kb.close_workspace_label.clone(), "close space"),
        (
            optional_keybind_label(&kb.open_notification_target_label),
            "open notification target",
        ),
        (
            optional_keybind_label(&kb.previous_workspace_label),
            "previous space",
        ),
        (
            optional_keybind_label(&kb.next_workspace_label),
            "next space",
        ),
        (
            optional_keybind_label(&kb.indexed_workspaces_label),
            "switch workspace 1-9",
        ),
        (
            optional_keybind_label(&kb.previous_agent_label),
            "previous agent",
        ),
        (optional_keybind_label(&kb.next_agent_label), "next agent"),
        (
            optional_keybind_label(&kb.indexed_agents_label),
            "focus agent 1-9",
        ),
        (kb.new_tab_label.clone(), "new tab"),
        (optional_keybind_label(&kb.rename_tab_label), "rename tab"),
        (
            optional_keybind_label(&kb.previous_tab_label),
            "previous tab",
        ),
        (optional_keybind_label(&kb.next_tab_label), "next tab"),
        (
            optional_keybind_label(&kb.indexed_tabs_label),
            "switch tab 1-9",
        ),
        (optional_keybind_label(&kb.close_tab_label), "close tab"),
    ];
    if let Some(label) = &kb.detach_label {
        workspace_tab.insert(3, (label.clone(), "detach from server"));
    }
    groups.push(("spaces / tabs", workspace_tab));

    let group_keys = vec![
        (
            optional_keybind_label(&kb.open_group_menu_label),
            "open group menu",
        ),
        (optional_keybind_label(&kb.new_group_label), "new group"),
        (
            optional_keybind_label(&kb.rename_group_label),
            "rename group",
        ),
        (
            optional_keybind_label(&kb.delete_group_label),
            "delete group",
        ),
        (
            optional_keybind_label(&kb.toggle_group_filter_label),
            "toggle current/all groups",
        ),
        (
            optional_keybind_label(&kb.previous_group_label),
            "previous group",
        ),
        (optional_keybind_label(&kb.next_group_label), "next group"),
    ];
    groups.push(("groups", group_keys));

    let agents = vec![
        (
            optional_keybind_label(&kb.open_agent_menu_label),
            "open agent menu",
        ),
        (
            optional_keybind_label(&kb.previous_agent_label),
            "previous agent",
        ),
        (optional_keybind_label(&kb.next_agent_label), "next agent"),
    ];
    groups.push(("agents", agents));

    let panes = vec![
        (kb.split_vertical_label.clone(), "split vertical"),
        (kb.split_horizontal_label.clone(), "split horizontal"),
        (kb.close_pane_label.clone(), "close pane"),
        (optional_keybind_label(&kb.rename_pane_label), "rename pane"),
        (kb.fullscreen_label.clone(), "fullscreen"),
        (kb.resize_mode_label.clone(), "resize mode"),
        (kb.toggle_sidebar_label.clone(), "toggle sidebar"),
        (
            optional_keybind_label(&kb.toggle_right_sidebar_label),
            "toggle right sidebar",
        ),
        (
            optional_keybind_label(&kb.focus_pane_left_label),
            "focus pane left",
        ),
        (
            optional_keybind_label(&kb.focus_pane_down_label),
            "focus pane down",
        ),
        (
            optional_keybind_label(&kb.focus_pane_up_label),
            "focus pane up",
        ),
        (
            optional_keybind_label(&kb.focus_pane_right_label),
            "focus pane right",
        ),
    ];
    groups.push(("panes", panes));

    if !kb.custom_commands.is_empty() {
        groups.push((
            "custom",
            kb.custom_commands
                .iter()
                .map(|binding| (binding.label.clone(), "custom command"))
                .collect(),
        ));
    }

    groups
}

pub(crate) fn keybind_help_lines(app: &AppState) -> Vec<(usize, Line<'static>)> {
    let heading_style = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default()
        .fg(app.palette.mauve)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(app.palette.text);

    let groups = keybind_help_groups(app);
    let key_width = groups
        .iter()
        .flat_map(|(_, entries)| entries.iter().map(|(key, _)| key.chars().count()))
        .max()
        .unwrap_or(8);

    let mut lines = Vec::new();

    for (group, entries) in groups {
        lines.push((
            group.len() + 1,
            Line::from(vec![Span::styled(format!(" {group}"), heading_style)]),
        ));
        for (key, label) in entries {
            let padded_key = format!(" {:<width$} ", key, width = key_width);
            let width = padded_key.chars().count() + label.chars().count();
            lines.push((
                width,
                Line::from(vec![
                    Span::styled(padded_key, key_style),
                    Span::styled(label.to_string(), label_style),
                ]),
            ));
        }
        lines.push((0, Line::raw("")));
    }

    lines
}

pub(super) fn render_keybind_help_overlay(app: &AppState, frame: &mut Frame) {
    super::dim_background(frame, frame.area());

    let Some(inner) = render_modal_shell(frame, frame.area(), 76, 22, &app.palette) else {
        return;
    };
    if inner.height < 6 || inner.width < 20 {
        return;
    }

    let stack = modal_stack_areas(inner, 2, 1, 0, 1);
    let header_rows =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas::<2>(stack.header);

    render_modal_header_bar(frame, header_rows[0], "keybinds", &app.palette, true);
    render_modal_subtitle(
        frame,
        header_rows[1],
        " available commands and configured shortcuts",
        &app.palette,
    );

    let body_area = stack.content;
    let viewport_rows = body_area.height.max(1) as usize;
    let metrics = crate::ui::modal_scroll_metrics(
        app.keybind_help_max_scroll() as usize + viewport_rows,
        viewport_rows,
        app.keybind_help.scroll as usize,
    );
    let scroll_area = modal_scroll_area(body_area, metrics);

    let body = Paragraph::new(
        keybind_help_lines(app)
            .into_iter()
            .map(|(_, line)| line)
            .collect::<Vec<_>>(),
    )
    .wrap(Wrap { trim: false })
    .scroll((app.keybind_help.scroll, 0));
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

    render_modal_scroll_hints(frame, stack.footer.unwrap_or_default(), &app.palette);
}
