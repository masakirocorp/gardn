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
            let shortcut_style = if shortcut == "unset" {
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
        title: "keybinds",
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
        "available commands and configured shortcuts",
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
