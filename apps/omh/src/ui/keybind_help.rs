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
    modal_scroll_area, modal_scroll_hint_line_count, modal_stack_areas, render_modal_header_bar,
    render_modal_scroll_hints, render_modal_shell, render_modal_subtitle,
};
use crate::app::{view_state::ClientViewState, AppState};

pub(super) type HelpEntry = (String, Cow<'static, str>);
pub(super) type HelpGroup = (&'static str, Vec<HelpEntry>);

fn help_entry(key: impl Into<String>, label: &'static str) -> HelpEntry {
    (key.into(), Cow::Borrowed(label))
}

fn keybind_label(bindings: &crate::config::ActionKeybinds) -> String {
    bindings.label().unwrap_or_else(|| "unset".to_string())
}

fn indexed_label(bindings: &[crate::config::IndexedKeybind]) -> String {
    if bindings.is_empty() {
        return "unset".to_string();
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
        "global",
        vec![
            help_entry(
                crate::config::format_key_combo((app.prefix_code, app.prefix_mods)),
                "prefix mode",
            ),
            help_entry(keybind_label(&kb.help), "keybinds"),
            help_entry(keybind_label(&kb.settings), "settings"),
            help_entry(keybind_label(&kb.detach), "detach"),
            help_entry(keybind_label(&kb.reload_config), "reload config"),
            help_entry(
                keybind_label(&kb.open_notification_target),
                "open notification target",
            ),
            help_entry(keybind_label(&kb.command_palette), "command palette"),
        ],
    ));

    groups.push((
        "navigation",
        vec![
            help_entry("esc", "back"),
            help_entry(
                format!(
                    "{} / {}",
                    keybind_label(&kb.navigate.workspace_up),
                    keybind_label(&kb.navigate.workspace_down)
                ),
                "space list",
            ),
            help_entry(
                format!(
                    "{} / {} / {} / {} / left / right",
                    keybind_label(&kb.navigate.pane_left),
                    keybind_label(&kb.navigate.pane_down),
                    keybind_label(&kb.navigate.pane_up),
                    keybind_label(&kb.navigate.pane_right)
                ),
                "move focus",
            ),
            help_entry("tab / shift+tab", "cycle pane"),
            help_entry("enter", "open workspace"),
            help_entry("1..0", "switch workspace"),
        ],
    ));

    let workspace_tab = vec![
        help_entry(keybind_label(&kb.workspace_picker), "workspace navigation"),
        help_entry(keybind_label(&kb.new_workspace), "new workspace"),
        help_entry(keybind_label(&kb.rename_workspace), "rename workspace"),
        help_entry(keybind_label(&kb.close_workspace), "close workspace"),
        help_entry(keybind_label(&kb.previous_workspace), "previous workspace"),
        help_entry(keybind_label(&kb.next_workspace), "next workspace"),
        help_entry(indexed_label(&kb.switch_workspace), "switch space 1-10"),
        help_entry(keybind_label(&kb.previous_agent), "previous agent"),
        help_entry(keybind_label(&kb.next_agent), "next agent"),
        help_entry(indexed_label(&kb.focus_agent), "focus agent 1-9"),
        help_entry(keybind_label(&kb.new_tab), "new tab"),
        help_entry(keybind_label(&kb.rename_tab), "rename tab"),
        help_entry(keybind_label(&kb.previous_tab), "previous tab"),
        help_entry(keybind_label(&kb.next_tab), "next tab"),
        help_entry(indexed_label(&kb.switch_tab), "switch tab 1-10"),
        help_entry(keybind_label(&kb.close_tab), "close tab"),
    ];
    groups.push(("workspaces / tabs", workspace_tab));

    let group_keys = vec![
        help_entry(keybind_label(&kb.open_group_menu), "open group menu"),
        help_entry(keybind_label(&kb.new_group), "new group"),
        help_entry(keybind_label(&kb.rename_group), "rename group"),
        help_entry(keybind_label(&kb.delete_group), "delete group"),
        help_entry(
            keybind_label(&kb.toggle_group_filter),
            "toggle current/all groups",
        ),
        help_entry(keybind_label(&kb.previous_group), "previous group"),
        help_entry(keybind_label(&kb.next_group), "next group"),
        help_entry(indexed_label(&kb.switch_group), "switch group 1-10"),
    ];
    groups.push(("groups", group_keys));

    let agents = vec![
        help_entry(keybind_label(&kb.open_agent_menu), "open agent menu"),
        help_entry(keybind_label(&kb.previous_agent), "previous agent"),
        help_entry(keybind_label(&kb.next_agent), "next agent"),
    ];
    groups.push(("agents", agents));

    let panes = vec![
        help_entry(keybind_label(&kb.split_vertical), "split vertical"),
        help_entry(keybind_label(&kb.split_horizontal), "split horizontal"),
        help_entry(keybind_label(&kb.close_pane), "close pane"),
        help_entry(keybind_label(&kb.rename_pane), "rename pane"),
        help_entry(keybind_label(&kb.edit_scrollback), "edit scrollback"),
        help_entry(keybind_label(&kb.copy_mode), "copy mode"),
        help_entry(keybind_label(&kb.zoom), "zoom pane"),
        help_entry(keybind_label(&kb.resize_mode), "resize mode"),
        help_entry(keybind_label(&kb.toggle_sidebar), "toggle sidebar"),
        help_entry(keybind_label(&kb.toggle_context_bar), "toggle context bar"),
        help_entry(keybind_label(&kb.focus_pane_left), "focus pane left"),
        help_entry(keybind_label(&kb.focus_pane_down), "focus pane down"),
        help_entry(keybind_label(&kb.focus_pane_up), "focus pane up"),
        help_entry(keybind_label(&kb.focus_pane_right), "focus pane right"),
        help_entry(keybind_label(&kb.cycle_pane_next), "cycle pane next"),
        help_entry(
            keybind_label(&kb.cycle_pane_previous),
            "cycle pane previous",
        ),
        help_entry(keybind_label(&kb.last_pane), "last pane"),
        help_entry(
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
                .map(|binding| {
                    (
                        binding.label.clone(),
                        binding
                            .description
                            .clone()
                            .map(Cow::Owned)
                            .unwrap_or(Cow::Borrowed("custom command")),
                    )
                })
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
                    Span::styled(label.into_owned(), label_style),
                ]),
            ));
        }
        lines.push((0, Line::raw("")));
    }

    lines
}

fn keybind_help_height(area: Rect) -> u16 {
    let popup_width = 76.min(area.width.saturating_sub(4));
    let inner_width = popup_width.saturating_sub(2);
    21 + modal_scroll_hint_line_count(inner_width, 2)
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

    let Some(inner) = render_modal_shell(frame, area, 76, keybind_help_height(area), &app.palette)
    else {
        return;
    };
    if inner.height < 6 || inner.width < 20 {
        return;
    }

    let stack = modal_stack_areas(inner, 2, modal_scroll_hint_line_count(inner.width, 2), 0, 1);
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
    let lines = keybind_help_lines(app);
    let metrics = crate::ui::modal_scroll_metrics(lines.len(), viewport_rows, scroll as usize);
    let scroll_area = modal_scroll_area(body_area, metrics);

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

    render_modal_scroll_hints(frame, stack.footer.unwrap_or_default(), &app.palette);
}
