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

fn keybind_label(bindings: &crate::config::ActionKeybinds) -> String {
    bindings.label().unwrap_or_else(|| "unset".to_string())
}

fn indexed_label(bindings: &[crate::config::IndexedKeybind]) -> String {
    if bindings.is_empty() {
        "unset".to_string()
    } else if bindings.len() == 9 {
        let first = &bindings[0].label;
        if first.ends_with('1') {
            format!("{}1..9", first.trim_end_matches('1'))
        } else {
            bindings
                .iter()
                .map(|binding| binding.label.clone())
                .collect::<Vec<_>>()
                .join(" / ")
        }
    } else {
        bindings
            .iter()
            .map(|binding| binding.label.clone())
            .collect::<Vec<_>>()
            .join(" / ")
    }
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
                "prefix mode",
            ),
            (keybind_label(&kb.help), "keybinds"),
            (keybind_label(&kb.settings), "settings"),
            (keybind_label(&kb.detach), "detach"),
            (keybind_label(&kb.reload_config), "reload config"),
            (
                keybind_label(&kb.open_notification_target),
                "open notification target",
            ),
            (keybind_label(&kb.command_palette), "command palette"),
        ],
    ));

    groups.push((
        "navigation",
        vec![
            ("esc".to_string(), "back"),
            (
                format!(
                    "{} / {}",
                    keybind_label(&kb.navigate.workspace_up),
                    keybind_label(&kb.navigate.workspace_down)
                ),
                "space list",
            ),
            (
                format!(
                    "{} / {} / {} / {} / left / right",
                    keybind_label(&kb.navigate.pane_left),
                    keybind_label(&kb.navigate.pane_down),
                    keybind_label(&kb.navigate.pane_up),
                    keybind_label(&kb.navigate.pane_right)
                ),
                "move focus",
            ),
            ("tab / shift+tab".to_string(), "cycle pane"),
            ("enter".to_string(), "open workspace"),
            ("1..9".to_string(), "switch workspace"),
        ],
    ));

    let workspace_tab = vec![
        (keybind_label(&kb.workspace_picker), "workspace navigation"),
        (keybind_label(&kb.new_workspace), "new workspace"),
        (keybind_label(&kb.rename_workspace), "rename workspace"),
        (keybind_label(&kb.close_workspace), "close workspace"),
        (keybind_label(&kb.previous_workspace), "previous workspace"),
        (keybind_label(&kb.next_workspace), "next workspace"),
        (indexed_label(&kb.switch_workspace), "switch workspace 1-9"),
        (keybind_label(&kb.previous_agent), "previous agent"),
        (keybind_label(&kb.next_agent), "next agent"),
        (indexed_label(&kb.focus_agent), "focus agent 1-9"),
        (keybind_label(&kb.new_tab), "new tab"),
        (keybind_label(&kb.rename_tab), "rename tab"),
        (keybind_label(&kb.previous_tab), "previous tab"),
        (keybind_label(&kb.next_tab), "next tab"),
        (indexed_label(&kb.switch_tab), "switch tab 1-9"),
        (keybind_label(&kb.close_tab), "close tab"),
    ];
    groups.push(("workspaces / tabs", workspace_tab));

    let group_keys = vec![
        (keybind_label(&kb.open_group_menu), "open group menu"),
        (keybind_label(&kb.new_group), "new group"),
        (keybind_label(&kb.rename_group), "rename group"),
        (keybind_label(&kb.delete_group), "delete group"),
        (
            keybind_label(&kb.toggle_group_filter),
            "toggle current/all groups",
        ),
        (keybind_label(&kb.previous_group), "previous group"),
        (keybind_label(&kb.next_group), "next group"),
    ];
    groups.push(("groups", group_keys));

    let agents = vec![
        (keybind_label(&kb.open_agent_menu), "open agent menu"),
        (keybind_label(&kb.previous_agent), "previous agent"),
        (keybind_label(&kb.next_agent), "next agent"),
    ];
    groups.push(("agents", agents));

    let panes = vec![
        (keybind_label(&kb.split_vertical), "split vertical"),
        (keybind_label(&kb.split_horizontal), "split horizontal"),
        (keybind_label(&kb.close_pane), "close pane"),
        (keybind_label(&kb.rename_pane), "rename pane"),
        (keybind_label(&kb.edit_scrollback), "edit scrollback"),
        (keybind_label(&kb.zoom), "zoom pane"),
        (keybind_label(&kb.resize_mode), "resize mode"),
        (keybind_label(&kb.toggle_sidebar), "toggle sidebar"),
        (keybind_label(&kb.focus_pane_left), "focus pane left"),
        (keybind_label(&kb.focus_pane_down), "focus pane down"),
        (keybind_label(&kb.focus_pane_up), "focus pane up"),
        (keybind_label(&kb.focus_pane_right), "focus pane right"),
        (keybind_label(&kb.cycle_pane_next), "cycle pane next"),
        (
            keybind_label(&kb.cycle_pane_previous),
            "cycle pane previous",
        ),
        (keybind_label(&kb.last_pane), "last pane"),
        (
            keybind_label(&kb.toggle_right_sidebar),
            "toggle right sidebar",
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
