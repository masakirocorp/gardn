use crate::app::view_state::{CanvasOrigin, ClientTabContext, ClientTabViewKey, TabCanvasViewport};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

mod agent_profile_picker;
mod command_palette;
mod config_diagnostics;
mod dialogs;
pub(crate) mod git_repo_picker;
pub(crate) mod github;
mod keybind_help;
mod menus;
mod mobile;
mod modal_tabs;
mod navigator;
mod onboarding;
pub(crate) mod panes;
mod release_notes;
mod scrollbar;
mod settings;
mod sidebar;
mod status;
mod tabs;
mod text;
mod widgets;

use self::agent_profile_picker::render_agent_profile_picker_overlay_for_view;
use self::command_palette::render_command_palette_overlay_for_view;
use self::config_diagnostics::render_config_diagnostics_overlay_for_view;
pub(crate) use self::config_diagnostics::{
    config_diagnostics_action_at, config_diagnostics_max_scroll, config_diagnostics_popup_rect,
    ConfigDiagnosticsAction,
};
use self::dialogs::{
    render_confirm_close_overlay_for_view, render_confirm_delete_group_overlay_for_view,
    render_rename_overlay_for_view,
};
use self::git_repo_picker::render_git_repo_picker_overlay_for_view;
#[cfg(test)]
pub(crate) use self::keybind_help::keybind_help_lines;
use self::keybind_help::render_keybind_help_overlay_for_view;
pub(crate) use self::menus::global_menu_labels;
use self::menus::{
    render_agent_menu_for_view, render_context_menu_for_view, render_copy_mode_overlay_for_view,
    render_global_launcher_menu_for_view, render_group_menu_for_view,
    render_navigate_overlay_for_view, render_prefix_overlay_for_view,
    render_resize_overlay_for_view,
};
use self::mobile::{
    is_mobile_width, mobile_switcher_max_scroll_for_view_height, mobile_toast_banner_rect,
    render_mobile_header_for_view, render_mobile_panel_for_view, render_mobile_toast_banner,
    MOBILE_AGENT_PANEL_CHROME_HEIGHT, MOBILE_HEADER_HEIGHT,
};
use self::navigator::render_navigator_overlay_for_view;
pub(crate) use self::navigator::{navigator_layout, navigator_popup_rect};
pub(crate) use self::onboarding::onboarding_welcome_continue_rect;
use self::onboarding::render_onboarding_overlay;
pub(crate) use self::panes::popup_pane_rects_for_view;
use self::panes::{
    compute_pane_infos_for_view, render_panes_for_view, render_popup_pane_for_view,
    resize_popup_pane_for_view,
};
pub(crate) use self::release_notes::{
    product_announcement_display_lines, release_notes_close_button_rect,
    release_notes_display_lines, release_notes_wrapped_line_count, PRODUCT_ANNOUNCEMENT_MODAL_SIZE,
    RELEASE_NOTES_MODAL_SIZE,
};
use self::release_notes::{
    render_product_announcement_overlay_for_view, render_release_notes_overlay_for_view,
};
pub(crate) use self::scrollbar::{
    pane_scrollbar_rect, release_notes_scrollbar_rect, scrollbar_offset_from_drag_row,
    scrollbar_offset_from_row, scrollbar_thumb_grab_offset, should_show_scrollbar,
};
use self::settings::render_settings_overlay_for_view;
use self::sidebar::{
    render_collapsed_sidebar_hover_for_view, render_right_sidebar_for_view,
    render_sidebar_collapsed_for_view, render_sidebar_for_view,
};
use self::status::{
    copy_feedback_rect, render_copy_feedback, render_toast_notification, toast_notification_rect,
};
use self::tabs::render_tab_bar_for_view;
pub(crate) use self::text::display_width_u16;
use self::widgets::fill_rect;
pub(crate) use self::{
    agent_profile_picker::{
        agent_profile_picker_button_rects, agent_profile_picker_inner_rect,
        agent_profile_picker_list_geometry, agent_profile_picker_popup_rect,
        agent_profile_picker_tab_chevron_at_for_view, agent_profile_picker_tab_hit_areas_for_view,
    },
    command_palette::{
        command_palette_button_rects, command_palette_inner_rect, command_palette_list_geometry,
        command_palette_popup_rect,
    },
};
pub(crate) use self::{
    dialogs::{
        confirm_close_button_rects, confirm_close_popup_rect,
        group_default_directory_input_rect_for_view, group_default_host_rect_for_view,
        group_icon_button_rect_for_view, group_icon_picker_rects_at,
        group_icon_picker_rects_for_view, group_icon_picker_row_count,
        group_name_input_rect_for_view, rename_button_rects, rename_modal_size_for_view,
    },
    settings::{
        settings_close_button_rect, settings_section_list_rect, settings_sidebar_areas,
        settings_sidebar_entries, settings_sidebar_hit_areas, settings_subsection_anchor,
        SettingsSidebarEntry,
    },
    sidebar::{
        agent_panel_body_rect, agent_panel_empty_row_at_for_view, agent_panel_entries_for_view,
        agent_panel_entry_at_row_for_view, agent_panel_header_target_at_row_for_view,
        agent_panel_scroll_metrics_for_view, agent_panel_scrollbar_rect_for_view,
        agent_panel_toggle_rect, collapsed_agent_panel_entry_at_row_for_view,
        collapsed_agent_panel_header_target_at_row_for_view, collapsed_agent_panel_toggle_rect,
        collapsed_group_header_rect, collapsed_sidebar_sections_for_split,
        collapsed_sidebar_toggle_rect, collapsed_workspace_row_entry_at_for_view,
        compute_workspace_card_areas_in_list_for_view,
        compute_workspace_group_empty_areas_in_list_for_view,
        compute_workspace_group_header_areas_in_list_for_view, expanded_sidebar_sections,
        expanded_sidebar_toggle_rect, global_launcher_rect_for_view, group_selector_rect_for_view,
        left_sidebar_workspace_rect, right_aligned_expanded_sidebar_sections,
        right_aligned_sidebar_section_divider_rect, right_aligned_workspace_list_rect,
        right_sidebar_content_rect, right_sidebar_toggle_rect, sidebar_section_divider_rect,
        workspace_drop_indicator_row, workspace_list_body_rect,
        workspace_list_entry_count_for_view, workspace_list_rect,
        workspace_list_scroll_metrics_for_view, workspace_list_scrollbar_rect_for_view,
        AgentPanelEntry, AgentPanelHeaderTarget, CollapsedWorkspaceRowEntry,
    },
};
pub(crate) use self::{
    keybind_help::{keybind_help_layout, keybind_help_scroll_metrics, keybind_help_scrollbar_rect},
    mobile::{
        keep_mobile_switcher_selection_visible_for_view, mobile_agent_strip_rect,
        mobile_switcher_areas_for_view, mobile_switcher_max_scroll_for_view,
        mobile_switcher_selected_target_for_view, mobile_switcher_target_at_for_view,
        mobile_switcher_target_count_for_view, mobile_switcher_target_index_for_view,
        MobileSwitcherTarget,
    },
    panes::pane_is_scrolled_back,
    tabs::compute_tab_bar_view_for_view,
    widgets::{
        centered_popup_rect, modal_scroll_metrics, modal_stack_areas, ModalListGeometry,
        ModalListViewport,
    },
};
use crate::app::state::ViewLayout;
use crate::app::{AppState, ClientTabControl, ClientViewState, Mode};
use crate::terminal::TerminalRuntimeRegistry;

const COLLAPSED_WIDTH: u16 = 4; // num + space + dot + separator
const RIGHT_SIDEBAR_MIN_TERMINAL_WIDTH: u16 = 56;
pub(crate) const MIN_RIGHT_SIDEBAR_WIDTH: u16 = 18;
pub(crate) const MAX_RIGHT_SIDEBAR_WIDTH: u16 = 36;

const CONTEXT_BAR_SEPARATOR: &str = " / ";

fn desktop_content_areas(area: Rect, show_context_bar: bool) -> (Rect, Rect) {
    if !show_context_bar || area.height <= 1 {
        return (area, Rect::default());
    }
    let [content, context_bar] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    (content, context_bar)
}

fn count_label(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

const WATCHING_CHIP_BADGE: &str = " Watching ";
const FREE_CHIP_BADGE: &str = " Free ";

/// Copy for the per-client tab-control chip that trails the context bar.
/// The chip is client-local chrome: it only exists for the watching states
/// and is omitted entirely for the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TabControlChip {
    badge: &'static str,
    suffix: &'static str,
    free: bool,
}

impl TabControlChip {
    fn label(self) -> String {
        format!("{}{}", self.badge, self.suffix)
    }
}

pub(crate) fn tab_control_chip(tab_control: ClientTabControl) -> Option<TabControlChip> {
    match tab_control {
        ClientTabControl::WatchingControlled { .. } => Some(TabControlChip {
            badge: WATCHING_CHIP_BADGE,
            suffix: " Another Client Controls · Take Over",
            free: false,
        }),
        ClientTabControl::WatchingFree { .. } => Some(TabControlChip {
            badge: FREE_CHIP_BADGE,
            suffix: " Take Control",
            free: true,
        }),
        ClientTabControl::Controlling { .. } | ClientTabControl::Unavailable => None,
    }
}

/// Truncate the desktop chip label for a narrow bar: the hint suffix elides
/// first, then the badge.
fn truncate_tab_control_chip_label(label: &str, max_width: usize) -> String {
    for badge in [WATCHING_CHIP_BADGE, FREE_CHIP_BADGE] {
        if let Some(suffix) = label.strip_prefix(badge) {
            let badge_width = text::display_width(badge);
            if max_width >= badge_width.saturating_add(2) {
                return format!(
                    "{}{}",
                    badge,
                    text::truncate_end(suffix, max_width - badge_width)
                );
            }
            if max_width >= badge_width {
                return badge.to_string();
            }
            return text::truncate_end(label, max_width);
        }
    }
    text::truncate_end(label, max_width)
}

const TAB_CONTROL_ACTIONS: [&str; 2] = ["Take Over", "Take Control"];

fn tab_control_action_span(label: &str) -> Option<(u16, u16)> {
    for action in TAB_CONTROL_ACTIONS {
        if let Some(idx) = label.find(action) {
            let start = text::display_width_u16(&label[..idx]);
            let width = text::display_width_u16(action);
            if width > 0 {
                return Some((start, width));
            }
        }
    }
    None
}

fn tab_control_action_hit_rect(label: &str, rect: Rect) -> Option<Rect> {
    let (start, width) = tab_control_action_span(label)?;
    Some(Rect::new(rect.x.saturating_add(start), rect.y, width, 1))
}

fn context_bar_segment(
    target: crate::app::state::ContextBarTarget,
    label: String,
    rect: Rect,
) -> crate::app::state::ContextBarSegment {
    let hit_rect = (target == crate::app::state::ContextBarTarget::TabControl)
        .then(|| tab_control_action_hit_rect(&label, rect))
        .flatten();
    crate::app::state::ContextBarSegment {
        target,
        label,
        rect,
        hit_rect,
    }
}

fn compute_context_bar(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    active_workspace: Option<usize>,
    active_group: usize,
    active_tab: Option<usize>,
    focused_pane: Option<crate::layout::PaneId>,
    tab_control: ClientTabControl,
    rect: Rect,
) -> crate::app::state::ContextBarView {
    use crate::app::state::{ContextBarTarget, ContextBarView};

    if rect.width < 2 || rect.height == 0 {
        return ContextBarView::default();
    }

    let count_variants = if app.show_counters {
        let group_count = app.groups.len();
        let workspace_count = app.workspaces.len();
        let tab_count = app
            .workspaces
            .iter()
            .map(|workspace| workspace.tabs.len())
            .sum::<usize>();
        [
            format!(
                "{} · {} · {}",
                count_label(group_count, "Group", "Groups"),
                count_label(workspace_count, "Space", "Spaces"),
                count_label(tab_count, "Tab", "Tabs")
            ),
            format!(
                "{} · {}",
                count_label(group_count, "Group", "Groups"),
                count_label(workspace_count, "Space", "Spaces")
            ),
            count_label(group_count, "Group", "Groups"),
            String::new(),
        ]
    } else {
        std::array::from_fn(|_| String::new())
    };

    let workspace = active_workspace.and_then(|index| app.workspaces.get(index));
    let group = workspace
        .and_then(|workspace| app.group_index_by_id(&workspace.group_id))
        .or_else(|| (active_group < app.groups.len()).then_some(active_group))
        .and_then(|index| app.groups.get(index));
    let mut labels = Vec::with_capacity(4);
    if let Some(group) = group {
        labels.push((
            ContextBarTarget::Group,
            // An empty icon would otherwise leave a stray leading column.
            format!("{} {}", group.icon, group.name).trim().to_string(),
        ));
    }
    if let Some(workspace) = workspace {
        labels.push((
            ContextBarTarget::Workspace,
            workspace.display_name_from(&app.terminals, terminal_runtimes),
        ));
        if let Some(tab_idx) = active_tab.filter(|index| *index < workspace.tabs.len()) {
            if let Some(label) = workspace.tab_display_name(tab_idx) {
                labels.push((ContextBarTarget::Tab, label));
            }
        }
        if let Some(tab_idx) = active_tab.filter(|index| *index < workspace.tabs.len()) {
            if let Some(tab) = workspace.tabs[tab_idx].as_terminal() {
                if tab.panes.len() > 1 {
                    if let Some(pane_id) =
                        focused_pane.filter(|pane_id| tab.panes.contains_key(pane_id))
                    {
                        let label = workspace
                            .pane_state(pane_id)
                            .and_then(|pane| app.terminals.get(&pane.attached_terminal_id))
                            .and_then(|terminal| terminal.manual_label.clone())
                            .or_else(|| {
                                workspace
                                    .pane_display_number(pane_id)
                                    .map(|number| format!("Pane {number}"))
                            });
                        if let Some(label) = label {
                            labels.push((ContextBarTarget::Pane, label));
                        }
                    }
                }
            }
        }
    }
    // The control chip is always the trailing segment, so the left-drop loop
    // below sacrifices path segments first and keeps the chip longest.
    if let Some(chip) = tab_control_chip(tab_control) {
        labels.push((ContextBarTarget::TabControl, chip.label()));
    }

    let inner_width = rect.width.saturating_sub(2) as usize;
    let separator_width = CONTEXT_BAR_SEPARATOR.len() * labels.len().saturating_sub(1);
    let full_path_width = labels
        .iter()
        .map(|(_, label)| text::display_width(label))
        .sum::<usize>()
        .saturating_add(separator_width);
    let counts = count_variants
        .into_iter()
        .find(|counts| {
            let counts_width = text::display_width(counts);
            let gap = usize::from(!counts.is_empty() && !labels.is_empty()) * 2;
            counts_width
                .saturating_add(gap)
                .saturating_add(full_path_width)
                <= inner_width
        })
        .unwrap_or_default();
    let counts_width = text::display_width(&counts);
    let gap = usize::from(!counts.is_empty() && !labels.is_empty()) * 2;
    let path_available = inner_width.saturating_sub(counts_width.saturating_add(gap));

    while labels.len() > 1
        && labels
            .len()
            .saturating_add(CONTEXT_BAR_SEPARATOR.len() * labels.len().saturating_sub(1))
            > path_available
    {
        labels.remove(0);
    }
    if !labels.is_empty() && path_available > 0 {
        let separators = CONTEXT_BAR_SEPARATOR.len() * labels.len().saturating_sub(1);
        let label_budget = path_available.saturating_sub(separators);
        let mut widths = labels
            .iter()
            .map(|(_, label)| text::display_width(label))
            .collect::<Vec<_>>();
        while widths.iter().sum::<usize>() > label_budget {
            let Some((index, _)) = widths
                .iter()
                .enumerate()
                .filter(|(_, width)| **width > 1)
                .max_by_key(|(_, width)| **width)
            else {
                break;
            };
            widths[index] -= 1;
        }
        for ((target, label), width) in labels.iter_mut().zip(widths) {
            *label = if *target == ContextBarTarget::TabControl {
                truncate_tab_control_chip_label(label, width)
            } else {
                text::truncate_end(label, width)
            };
        }
    } else {
        labels.clear();
    }

    let path_x = rect.x.saturating_add(1);
    let mut cursor = path_x;
    let segments = labels
        .into_iter()
        .enumerate()
        .map(|(index, (target, label))| {
            if index > 0 {
                cursor = cursor.saturating_add(CONTEXT_BAR_SEPARATOR.len() as u16);
            }
            let width = text::display_width_u16(&label);
            let segment = context_bar_segment(target, label, Rect::new(cursor, rect.y, width, 1));
            cursor = cursor.saturating_add(width);
            segment
        })
        .collect();
    let counts_rect = if counts.is_empty() {
        Rect::default()
    } else {
        Rect::new(
            rect.x
                .saturating_add(rect.width)
                .saturating_sub(counts_width as u16)
                .saturating_sub(1),
            rect.y,
            counts_width as u16,
            1,
        )
    };

    ContextBarView {
        rect,
        counts,
        counts_rect,
        segments,
    }
}

fn compute_mobile_breadcrumb(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    active_workspace: Option<usize>,
    active_group: usize,
    active_tab: Option<usize>,
    focused_pane: Option<crate::layout::PaneId>,
    tab_control: ClientTabControl,
    rect: Rect,
) -> crate::app::state::ContextBarView {
    use crate::app::state::{ContextBarSegment, ContextBarTarget, ContextBarView};

    if rect.width < 3 || rect.height == 0 {
        return ContextBarView::default();
    }

    let workspace = active_workspace.and_then(|index| app.workspaces.get(index));
    let group = workspace
        .and_then(|workspace| app.group_index_by_id(&workspace.group_id))
        .or_else(|| (active_group < app.groups.len()).then_some(active_group))
        .and_then(|index| app.groups.get(index));
    let mut labels = Vec::with_capacity(4);
    if let Some(group) = group {
        labels.push((
            ContextBarTarget::Group,
            // An empty icon would otherwise leave a stray leading column.
            format!("{} {}", group.icon, group.name).trim().to_string(),
        ));
    }
    if let Some(workspace) = workspace {
        labels.push((
            ContextBarTarget::Workspace,
            workspace.display_name_from(&app.terminals, terminal_runtimes),
        ));
        if let Some(tab_idx) = active_tab.filter(|index| *index < workspace.tabs.len()) {
            if let Some(label) = workspace.tab_display_name(tab_idx) {
                labels.push((ContextBarTarget::Tab, label));
            }
            if let Some(tab) = workspace.tabs[tab_idx].as_terminal() {
                if tab.panes.len() > 1 {
                    if let Some(pane_id) =
                        focused_pane.filter(|pane_id| tab.panes.contains_key(pane_id))
                    {
                        if let Some(label) = workspace
                            .pane_state(pane_id)
                            .and_then(|pane| app.terminals.get(&pane.attached_terminal_id))
                            .and_then(|terminal| terminal.manual_label.clone())
                            .or_else(|| {
                                workspace
                                    .pane_display_number(pane_id)
                                    .map(|number| format!("Pane {number}"))
                            })
                        {
                            labels.push((ContextBarTarget::Pane, label));
                        }
                    }
                }
            }
        }
    }

    const PREFERRED_TAP_WIDTH: usize = 8;
    // Reserve the right edge for the control chip so breadcrumb labels never
    // slide under it; the chip only exists for watching clients and is
    // dropped entirely when the badge cannot fit. Mobile shows the concise
    // badge without the desktop suffix.
    let chip = tab_control_chip(tab_control).and_then(|chip| {
        let max_width = rect.width.saturating_sub(2) as usize;
        (max_width >= text::display_width(chip.badge)).then(|| chip.badge.to_string())
    });
    let chip_reserve = chip
        .as_ref()
        .map(|label| text::display_width(label).saturating_add(1))
        .unwrap_or(0);
    let available = (rect.width.saturating_sub(2) as usize).saturating_sub(chip_reserve);
    // Mirror the desktop left-drop: at tiny widths crumbs that cannot fit
    // even at minimum width ("… ▾") are dropped from the left instead of
    // overdrawing the header row.
    while !labels.is_empty()
        && 3 * labels.len() + CONTEXT_BAR_SEPARATOR.len() * labels.len().saturating_sub(1)
            > available
    {
        labels.remove(0);
    }
    let separator_width = CONTEXT_BAR_SEPARATOR.len() * labels.len().saturating_sub(1);
    let label_budget = available.saturating_sub(separator_width);
    let mut widths = labels
        .iter()
        .map(|(_, label)| {
            text::display_width(label)
                .saturating_add(2)
                .max(PREFERRED_TAP_WIDTH)
        })
        .collect::<Vec<_>>();
    for minimum_width in [PREFERRED_TAP_WIDTH, 3] {
        while widths.iter().sum::<usize>() > label_budget {
            let Some((index, _)) = widths
                .iter()
                .enumerate()
                .filter(|(_, width)| **width > minimum_width)
                .max_by_key(|(_, width)| **width)
            else {
                break;
            };
            widths[index] -= 1;
        }
    }

    let mut cursor = rect.x.saturating_add(1);
    let mut segments: Vec<ContextBarSegment> = labels
        .into_iter()
        .zip(widths)
        .enumerate()
        .map(|(index, ((target, label), width))| {
            if index > 0 {
                cursor = cursor.saturating_add(CONTEXT_BAR_SEPARATOR.len() as u16);
            }
            let text = format!("{} ▾", text::truncate_end(&label, width.saturating_sub(2)));
            let padding = width.saturating_sub(text::display_width(&text));
            let leading_padding = padding / 2;
            let label = format!(
                "{}{}{}",
                " ".repeat(leading_padding),
                text,
                " ".repeat(padding.saturating_sub(leading_padding))
            );
            let segment =
                context_bar_segment(target, label, Rect::new(cursor, rect.y, width as u16, 1));
            cursor = cursor.saturating_add(width as u16);
            segment
        })
        .collect();
    if let Some(chip_label) = chip {
        let width = text::display_width_u16(&chip_label);
        if width > 0 {
            let x = rect
                .x
                .saturating_add(rect.width)
                .saturating_sub(width)
                .saturating_sub(1);
            segments.push(context_bar_segment(
                ContextBarTarget::TabControl,
                chip_label,
                Rect::new(x, rect.y, width, 1),
            ));
        }
    }

    ContextBarView {
        rect,
        counts: String::new(),
        counts_rect: Rect::default(),
        segments,
    }
}

fn render_context_bar(
    app: &AppState,
    context_bar: &crate::app::state::ContextBarView,
    frame: &mut Frame,
) {
    if context_bar.rect == Rect::default() {
        return;
    }
    fill_rect(
        frame,
        context_bar.rect,
        Style::default()
            .fg(app.palette.overlay1)
            .bg(app.palette.surface0),
    );
    if context_bar.counts_rect != Rect::default() {
        frame.render_widget(
            Paragraph::new(context_bar.counts.as_str()).style(
                Style::default()
                    .fg(app.palette.overlay1)
                    .bg(app.palette.surface0),
            ),
            context_bar.counts_rect,
        );
    }
    for (index, segment) in context_bar.segments.iter().enumerate() {
        if index > 0 {
            let separator_x = segment
                .rect
                .x
                .saturating_sub(CONTEXT_BAR_SEPARATOR.len() as u16);
            let previous = context_bar.segments[index - 1].rect;
            // The right-aligned mobile chip leaves a gap before it; only draw
            // a separator between contiguous path segments.
            if separator_x == previous.x.saturating_add(previous.width) {
                let separator = Rect::new(
                    separator_x,
                    segment.rect.y,
                    CONTEXT_BAR_SEPARATOR.len() as u16,
                    1,
                );
                frame.render_widget(
                    Paragraph::new(CONTEXT_BAR_SEPARATOR).style(
                        Style::default()
                            .fg(app.palette.overlay0)
                            .bg(app.palette.surface0),
                    ),
                    separator,
                );
            }
        }
        if segment.target == crate::app::state::ContextBarTarget::TabControl {
            render_tab_control_chip_segment(app, segment, frame);
            continue;
        }
        let style = if index + 1 == context_bar.segments.len() {
            Style::default()
                .fg(app.palette.text)
                .bg(app.palette.surface0)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default()
                .fg(app.palette.overlay1)
                .bg(app.palette.surface0)
                .add_modifier(Modifier::UNDERLINED)
        };
        frame.render_widget(
            Paragraph::new(Line::from(segment.label.as_str())).style(style),
            segment.rect,
        );
    }
}

fn render_tab_control_chip_segment(
    app: &AppState,
    segment: &crate::app::state::ContextBarSegment,
    frame: &mut Frame,
) {
    if segment.rect.width == 0 || segment.rect.height == 0 {
        return;
    }
    let p = &app.palette;
    let (badge, suffix, free) =
        if let Some(suffix) = segment.label.strip_prefix(WATCHING_CHIP_BADGE) {
            (WATCHING_CHIP_BADGE, suffix, false)
        } else if let Some(suffix) = segment.label.strip_prefix(FREE_CHIP_BADGE) {
            (FREE_CHIP_BADGE, suffix, true)
        } else {
            // Extreme truncation cut into the badge itself; render what remains.
            (segment.label.as_str(), "", false)
        };
    let badge_bg = if free { p.teal } else { p.overlay0 };
    let (hint, action) = match tab_control_action_span(suffix) {
        Some((start, width)) => {
            let start = start as usize;
            let hint: String = suffix.chars().take(start).collect();
            let action: String = suffix.chars().skip(start).take(width as usize).collect();
            (hint, action)
        }
        None => (suffix.to_string(), String::new()),
    };
    let mut spans = Vec::new();
    let status_style = if action.is_empty() {
        Style::default()
            .fg(widgets::panel_contrast_fg(p))
            .bg(badge_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.overlay1).bg(p.surface0)
    };
    spans.push(Span::styled(badge.to_string(), status_style));
    if !hint.is_empty() {
        spans.push(Span::styled(
            hint,
            Style::default().fg(p.overlay0).bg(p.surface0),
        ));
    }
    if !action.is_empty() {
        spans.push(Span::styled(
            action,
            Style::default()
                .fg(widgets::panel_contrast_fg(p))
                .bg(badge_bg)
                .add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), segment.rect);
}

const SPINNERS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(super) fn spinner_frame(tick: u32) -> &'static str {
    SPINNERS[(tick as usize / crate::app::ANIMATION_TICK_STEP as usize) % SPINNERS.len()]
}

/// Whether this view computation may resize shared pane runtimes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaneResizeAuthority {
    Denied,
    Granted,
}

impl PaneResizeAuthority {
    fn may_resize(self) -> bool {
        matches!(self, Self::Granted)
    }
}

pub(crate) fn compute_view(
    app: &AppState,
    client_view: &mut ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
    cell_size: crate::kitty_graphics::HostCellSize,
    resize_authority: PaneResizeAuthority,
) {
    compute_view_with_tab_context(
        app,
        client_view,
        terminal_runtimes,
        ClientTabContext::default(),
        area,
        cell_size,
        resize_authority,
    );
}

/// Compute one client's view geometry and, when authorized, reconcile pane sizes.
pub(crate) fn compute_view_with_tab_context(
    app: &AppState,
    client_view: &mut ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    tab_context: ClientTabContext,
    area: Rect,
    cell_size: crate::kitty_graphics::HostCellSize,
    resize_authority: PaneResizeAuthority,
) {
    if is_mobile_width(area, app.mobile_width_threshold) {
        compute_mobile_view(
            app,
            client_view,
            terminal_runtimes,
            tab_context,
            area,
            resize_authority,
            cell_size,
        );
        return;
    }

    compute_desktop_view(
        app,
        client_view,
        terminal_runtimes,
        tab_context,
        area,
        resize_authority,
        cell_size,
    );
}

fn client_tab_canvas_view(
    app: &AppState,
    client_view: &mut ClientViewState,
    tab_context: ClientTabContext,
    terminal_area: Rect,
    resize_authority: PaneResizeAuthority,
) -> (Rect, bool) {
    let (canvas_width, canvas_height, resize_pane_runtimes) =
        if tab_context.control.can_mutate_tab() {
            (
                terminal_area.width,
                terminal_area.height,
                resize_authority.may_resize(),
            )
        } else {
            let (width, height) = tab_context
                .canvas_size
                .unwrap_or((terminal_area.width, terminal_area.height));
            (width, height, false)
        };
    let canvas_size = ratatui::layout::Size::new(canvas_width, canvas_height);
    let canvas_area = Rect::new(0, 0, canvas_width, canvas_height);
    let Some(ws_idx) = client_view.active_workspace else {
        client_view.tab_canvas_view = None;
        return (canvas_area, resize_pane_runtimes);
    };
    let Some(workspace) = app.workspaces.get(ws_idx) else {
        client_view.tab_canvas_view = None;
        return (canvas_area, resize_pane_runtimes);
    };
    let Some(tab_idx) = client_view.active_tab_index_for_workspace(app, ws_idx) else {
        client_view.tab_canvas_view = None;
        return (canvas_area, resize_pane_runtimes);
    };
    let Some(tab) = workspace.tabs.get(tab_idx) else {
        client_view.tab_canvas_view = None;
        return (canvas_area, resize_pane_runtimes);
    };
    let Some(tab) = tab.as_terminal() else {
        client_view.tab_canvas_view = None;
        return (canvas_area, false);
    };
    let key = ClientTabViewKey::new(&workspace.id, tab.number);
    let origin = if tab_context.control.can_mutate_tab() {
        CanvasOrigin::default()
    } else {
        client_view.tab_canvas_origin(&key)
    };
    let mut canvas_view = TabCanvasViewport::new(canvas_size, terminal_area, origin);
    let focused_pane = client_view
        .focused_pane_for_tab(&workspace.id, tab.number)
        .filter(|pane_id| tab.panes.contains_key(pane_id))
        .unwrap_or(tab.root_pane);
    let focused_rect = if client_view.tab_is_zoomed(&workspace.id, tab.number) {
        Some(canvas_area)
    } else {
        tab.layout
            .panes(canvas_area, focused_pane)
            .into_iter()
            .find(|info| info.id == focused_pane)
            .map(|info| info.rect)
    };
    if let Some(focused_rect) = focused_rect {
        let revealed_origin = canvas_view.reveal_focused(canvas_view.origin, focused_rect);
        if revealed_origin != canvas_view.origin {
            canvas_view = TabCanvasViewport::new(canvas_size, terminal_area, revealed_origin);
        }
    }
    client_view.set_tab_canvas_view(key, canvas_view);
    (canvas_area, resize_pane_runtimes)
}

fn hide_tab_bar_when_single_tab(app: &AppState, client_view: &ClientViewState) -> bool {
    client_view
        .settings
        .pending_hide_tab_bar_when_single_tab
        .unwrap_or(app.hide_tab_bar_when_single_tab)
}

fn tab_bar_layout(
    hide_when_single: bool,
    zen_mode: bool,
    tab_count: usize,
    main_area: Rect,
) -> (Rect, Rect) {
    let show =
        !zen_mode && main_area.height > 1 && tab_count > 0 && !(hide_when_single && tab_count <= 1);
    if show {
        let [tab_bar_rect, terminal_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(main_area);
        (tab_bar_rect, terminal_area)
    } else {
        (Rect::default(), main_area)
    }
}

fn compute_desktop_view(
    app: &AppState,
    client_view: &mut ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    tab_context: ClientTabContext,
    area: Rect,
    resize_authority: PaneResizeAuthority,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    let show_context_bar = !client_view.zen_mode
        && app.context_bar_is_visible(client_view.context_bar_visibility_override);
    let (content_area, context_bar_rect) = desktop_content_areas(area, show_context_bar);

    let sidebar_w = if client_view.sidebar_collapsed {
        COLLAPSED_WIDTH
    } else {
        client_view
            .sidebar_width
            .clamp(app.sidebar_min_width, app.sidebar_max_width)
    };
    let right_sidebar_w = if client_view.right_sidebar_collapsed {
        COLLAPSED_WIDTH
    } else {
        client_view
            .right_sidebar_width
            .clamp(MIN_RIGHT_SIDEBAR_WIDTH, MAX_RIGHT_SIDEBAR_WIDTH)
    };

    let auto_separate = area.width
        >= sidebar_w
            .saturating_add(right_sidebar_w)
            .saturating_add(RIGHT_SIDEBAR_MIN_TERMINAL_WIDTH);
    let separate_sidebars = match app.sidebar_arrangement {
        crate::config::SidebarArrangementConfig::Auto => auto_separate,
        crate::config::SidebarArrangementConfig::Separate => true,
        crate::config::SidebarArrangementConfig::CombinedLeft
        | crate::config::SidebarArrangementConfig::CombinedRight => false,
    };
    let combined_right =
        app.sidebar_arrangement == crate::config::SidebarArrangementConfig::CombinedRight;
    let (sidebar_area, main_area, right_sidebar_area) = if client_view.zen_mode {
        (Rect::default(), content_area, Rect::default())
    } else if separate_sidebars {
        let [sidebar_area, main_area, right_sidebar_area] = Layout::horizontal([
            Constraint::Length(sidebar_w),
            Constraint::Min(1),
            Constraint::Length(right_sidebar_w),
        ])
        .areas(content_area);
        (sidebar_area, main_area, right_sidebar_area)
    } else if combined_right {
        let [main_area, sidebar_area] =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(sidebar_w)])
                .areas(content_area);
        (sidebar_area, main_area, Rect::default())
    } else {
        let [sidebar_area, main_area] =
            Layout::horizontal([Constraint::Length(sidebar_w), Constraint::Min(1)])
                .areas(content_area);
        (sidebar_area, main_area, Rect::default())
    };

    let active_workspace = client_view
        .active_workspace
        .and_then(|idx| app.workspaces.get(idx));
    let (tab_bar_rect, terminal_area) = tab_bar_layout(
        hide_tab_bar_when_single_tab(app, client_view),
        client_view.zen_mode,
        active_workspace
            .map(|workspace| workspace.tabs.len())
            .unwrap_or(0),
        main_area,
    );

    client_view.workspace_scroll = client_view
        .workspace_scroll
        .min(workspace_list_entry_count_for_view(app, client_view).saturating_sub(1));
    if !client_view.zen_mode {
        if right_sidebar_area != Rect::default() && !client_view.right_sidebar_collapsed {
            let max_agent_scroll = agent_panel_scroll_metrics_for_view(
                app,
                terminal_runtimes,
                client_view,
                right_sidebar_content_rect(right_sidebar_area),
                false,
            )
            .max_offset_from_bottom;
            client_view.agent_panel_scroll = client_view.agent_panel_scroll.min(max_agent_scroll);
        } else if right_sidebar_area == Rect::default() && !client_view.sidebar_collapsed {
            let (_, agent_area) = if combined_right {
                right_aligned_expanded_sidebar_sections(
                    sidebar_area,
                    client_view.sidebar_section_split,
                )
            } else {
                expanded_sidebar_sections(sidebar_area, client_view.sidebar_section_split)
            };
            let max_agent_scroll = agent_panel_scroll_metrics_for_view(
                app,
                terminal_runtimes,
                client_view,
                agent_area,
                true,
            )
            .max_offset_from_bottom;
            client_view.agent_panel_scroll = client_view.agent_panel_scroll.min(max_agent_scroll);
        } else {
            client_view.agent_panel_scroll = 0;
        }
    }

    let (workspace_card_areas, workspace_group_header_areas, workspace_group_empty_areas) =
        if client_view.zen_mode || client_view.sidebar_collapsed {
            (Vec::new(), Vec::new(), Vec::new())
        } else if right_sidebar_area != Rect::default() {
            let ws_area = left_sidebar_workspace_rect(sidebar_area);
            (
                compute_workspace_card_areas_in_list_for_view(app, client_view, ws_area),
                compute_workspace_group_header_areas_in_list_for_view(app, client_view, ws_area),
                compute_workspace_group_empty_areas_in_list_for_view(app, client_view, ws_area),
            )
        } else if combined_right {
            let ws_area =
                right_aligned_workspace_list_rect(sidebar_area, client_view.sidebar_section_split);
            (
                compute_workspace_card_areas_in_list_for_view(app, client_view, ws_area),
                compute_workspace_group_header_areas_in_list_for_view(app, client_view, ws_area),
                compute_workspace_group_empty_areas_in_list_for_view(app, client_view, ws_area),
            )
        } else {
            let ws_area = workspace_list_rect(sidebar_area, client_view.sidebar_section_split);
            (
                compute_workspace_card_areas_in_list_for_view(app, client_view, ws_area),
                compute_workspace_group_header_areas_in_list_for_view(app, client_view, ws_area),
                compute_workspace_group_empty_areas_in_list_for_view(app, client_view, ws_area),
            )
        };

    let tab_bar_view = client_view
        .active_workspace
        .map(|ws_idx| compute_tab_bar_view_for_view(app, client_view, ws_idx, tab_bar_rect))
        .unwrap_or_default();
    client_view.tab_scroll = tab_bar_view.scroll;

    let (pane_area, resize_pane_runtimes) = client_tab_canvas_view(
        app,
        client_view,
        tab_context,
        terminal_area,
        resize_authority,
    );
    let split_borders = client_view
        .active_workspace
        .and_then(|idx| {
            let workspace = app.workspaces.get(idx)?;
            let tab_idx = client_view.active_tab_index_for_workspace(app, idx)?;
            workspace
                .tabs
                .get(tab_idx)
                .and_then(crate::workspace::WorkspaceTab::as_terminal)
        })
        .map(|tab| tab.layout.splits(pane_area))
        .unwrap_or_default();
    let pane_infos = compute_pane_infos_for_view(
        app,
        client_view,
        terminal_runtimes,
        pane_area,
        resize_pane_runtimes,
        cell_size,
    );
    if resize_pane_runtimes {
        resize_popup_pane_for_view(app, client_view, terminal_runtimes, area, cell_size);
    }

    let toast_hit_area = app
        .toast
        .as_ref()
        .map(|toast| {
            toast_notification_rect(
                area,
                toast,
                toast.position.unwrap_or(app.toast_config.gardn.position),
            )
        })
        .unwrap_or_default();

    let active_tab = client_view
        .active_workspace
        .and_then(|ws_idx| client_view.active_tab_index_for_workspace(app, ws_idx));
    let focused_pane = client_view
        .active_workspace
        .and_then(|ws_idx| client_view.focused_pane_for_workspace(app, ws_idx))
        .map(|(_, pane_id)| pane_id);
    let context_bar = compute_context_bar(
        app,
        terminal_runtimes,
        client_view.active_workspace,
        client_view.active_group,
        active_tab,
        focused_pane,
        tab_context.control,
        context_bar_rect,
    );

    client_view.computed = crate::app::ViewState {
        layout: ViewLayout::Desktop,
        sidebar_rect: sidebar_area,
        right_sidebar_rect: right_sidebar_area,
        workspace_card_areas,
        workspace_group_header_areas,
        workspace_group_empty_areas,
        tab_bar_rect,
        tab_hit_areas: tab_bar_view.tab_hit_areas,
        tab_close_hit_areas: tab_bar_view.tab_close_hit_areas,
        tab_scroll_left_hit_area: tab_bar_view.scroll_left_hit_area,
        tab_scroll_right_hit_area: tab_bar_view.scroll_right_hit_area,
        new_tab_hit_area: tab_bar_view.new_tab_hit_area,
        context_bar,
        terminal_area,
        mobile_header_rect: Rect::default(),
        toast_hit_area,
        pane_infos,
        split_borders,
    };
    client_view.compute_github(app);
    app.sync_copy_mode_search_geometry_for_view(client_view);
}

fn compute_mobile_view(
    app: &AppState,
    client_view: &mut ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    tab_context: ClientTabContext,
    area: Rect,
    resize_authority: PaneResizeAuthority,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    let header_h = if client_view.zen_mode {
        0
    } else {
        area.height.min(MOBILE_HEADER_HEIGHT)
    };
    let (header_rect, terminal_area) = if header_h == 0 {
        (Rect::default(), area)
    } else if area.height > header_h {
        let [header_rect, terminal_area] =
            Layout::vertical([Constraint::Length(header_h), Constraint::Min(1)]).areas(area);
        (header_rect, terminal_area)
    } else {
        (area, Rect::default())
    };

    if client_view.mode == Mode::Navigate {
        let chrome_height = if client_view.mobile_agents_expanded {
            MOBILE_AGENT_PANEL_CHROME_HEIGHT
        } else {
            MOBILE_HEADER_HEIGHT.saturating_add(2)
        };
        let switcher_viewport_h = area.height.saturating_sub(chrome_height);
        let max_scroll = mobile_switcher_max_scroll_for_view_height(
            app,
            terminal_runtimes,
            client_view,
            switcher_viewport_h,
        );
        client_view.mobile_switcher_scroll = client_view.mobile_switcher_scroll.min(max_scroll);
    }

    let (pane_area, resize_pane_runtimes) = client_tab_canvas_view(
        app,
        client_view,
        tab_context,
        terminal_area,
        resize_authority,
    );
    let split_borders = client_view
        .active_workspace
        .and_then(|idx| {
            let workspace = app.workspaces.get(idx)?;
            let tab_idx = client_view.active_tab_index_for_workspace(app, idx)?;
            workspace
                .tabs
                .get(tab_idx)
                .and_then(crate::workspace::WorkspaceTab::as_terminal)
        })
        .map(|tab| tab.layout.splits(pane_area))
        .unwrap_or_default();

    let pane_infos = compute_pane_infos_for_view(
        app,
        client_view,
        terminal_runtimes,
        pane_area,
        resize_pane_runtimes,
        cell_size,
    );
    let breadcrumb_rect = Rect::new(
        header_rect.x,
        header_rect.y.saturating_add(1),
        header_rect.width,
        1,
    );
    let active_tab = client_view
        .active_workspace
        .and_then(|ws_idx| client_view.active_tab_index_for_workspace(app, ws_idx));
    let focused_pane = client_view
        .active_workspace
        .and_then(|ws_idx| client_view.focused_pane_for_workspace(app, ws_idx))
        .map(|(_, pane_id)| pane_id);
    let breadcrumb = compute_mobile_breadcrumb(
        app,
        terminal_runtimes,
        client_view.active_workspace,
        client_view.active_group,
        active_tab,
        focused_pane,
        tab_context.control,
        breadcrumb_rect,
    );

    let toast_hit_area = app
        .toast
        .as_ref()
        .map(|_| mobile_toast_banner_rect(area))
        .unwrap_or_default();

    client_view.computed = crate::app::ViewState {
        layout: ViewLayout::Mobile,
        sidebar_rect: Rect::default(),
        right_sidebar_rect: Rect::default(),
        workspace_card_areas: Vec::new(),
        workspace_group_header_areas: Vec::new(),
        workspace_group_empty_areas: Vec::new(),
        tab_bar_rect: Rect::default(),
        tab_hit_areas: Vec::new(),
        tab_close_hit_areas: Vec::new(),
        tab_scroll_left_hit_area: Rect::default(),
        tab_scroll_right_hit_area: Rect::default(),
        new_tab_hit_area: Rect::default(),
        context_bar: breadcrumb,
        terminal_area,
        mobile_header_rect: header_rect,
        toast_hit_area,
        pane_infos,
        split_borders,
    };
    client_view.compute_github(app);
    app.sync_copy_mode_search_geometry_for_view(client_view);
}

pub(crate) fn render_loop_debug(frame: &mut Frame, line: &str, bg: Color, fg: Color) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = (line.chars().count() as u16).min(area.width).max(1);
    let x = area.x + area.width.saturating_sub(width);
    let y = area.y + area.height.saturating_sub(1);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().fg(fg).bg(bg)),
        Rect::new(x, y, width, 1),
    );
}

/// Draw one immutable client view from immutable shared state.
pub fn render(
    app: &AppState,
    client_view: &ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
) {
    render_with_tab_context(
        app,
        client_view,
        terminal_runtimes,
        ClientTabContext::default(),
        frame,
    );
}

pub(crate) fn render_with_tab_context(
    app: &AppState,
    client_view: &ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    tab_context: ClientTabContext,
    frame: &mut Frame,
) {
    fill_rect(
        frame,
        frame.area(),
        Style::default().bg(app.palette.panel_bg),
    );
    let sidebar_area = client_view.computed.sidebar_rect;
    let right_sidebar_area = client_view.computed.right_sidebar_rect;
    let tab_bar_area = client_view.computed.tab_bar_rect;
    let terminal_area = client_view.computed.terminal_area;

    if client_view.computed.layout == ViewLayout::Mobile {
        if !client_view.zen_mode {
            render_mobile_header_for_view(
                app,
                terminal_runtimes,
                client_view,
                frame,
                client_view.computed.mobile_header_rect,
            );
        }
    } else if !client_view.zen_mode {
        if client_view.sidebar_collapsed {
            render_sidebar_collapsed_for_view(
                app,
                terminal_runtimes,
                client_view,
                frame,
                sidebar_area,
            );
        } else {
            render_sidebar_for_view(app, terminal_runtimes, client_view, frame, sidebar_area);
        }
    }
    if !client_view.zen_mode && client_view.computed.layout != ViewLayout::Mobile {
        render_tab_bar_for_view(app, client_view, tab_context.control, frame, tab_bar_area);
    }
    if client_view.github_is_focused(app) {
        if let Some(screen) = &client_view.github {
            github::render(screen, &app.palette, frame);
        }
    } else {
        render_panes_for_view(
            app,
            client_view,
            terminal_runtimes,
            tab_context.control,
            frame,
            terminal_area,
        );
    }
    if tab_context.control.is_watching() {
        panes::wash_rect(frame, tab_bar_area, &app.palette);
        panes::wash_rect(frame, terminal_area, &app.palette);
    }
    if right_sidebar_area != Rect::default() {
        render_right_sidebar_for_view(
            app,
            terminal_runtimes,
            client_view,
            frame,
            right_sidebar_area,
        );
    }
    if !client_view.zen_mode
        && (client_view.sidebar_collapsed || client_view.right_sidebar_collapsed)
        && client_view.computed.layout != ViewLayout::Mobile
    {
        render_collapsed_sidebar_hover_for_view(app, terminal_runtimes, client_view, frame);
    }
    render_context_bar(app, &client_view.computed.context_bar, frame);

    match client_view.mode {
        Mode::Onboarding => render_onboarding_overlay(app, frame, frame.area()),
        Mode::ReleaseNotes => {
            render_release_notes_overlay_for_view(app, client_view, frame, frame.area())
        }
        Mode::ProductAnnouncement => {
            render_product_announcement_overlay_for_view(app, client_view, frame, frame.area())
        }
        Mode::Navigate if client_view.computed.layout == ViewLayout::Mobile => {
            render_mobile_panel_for_view(app, terminal_runtimes, client_view, frame, frame.area())
        }
        Mode::Navigate => render_navigate_overlay_for_view(app, client_view, frame, terminal_area),
        Mode::Prefix => render_prefix_overlay_for_view(app, client_view, frame, terminal_area),
        Mode::Copy => render_copy_mode_overlay_for_view(app, client_view, frame, terminal_area),
        Mode::Resize => render_resize_overlay_for_view(app, client_view, frame, terminal_area),
        Mode::ConfirmClose => render_confirm_close_overlay_for_view(
            app,
            client_view,
            terminal_runtimes,
            frame,
            terminal_area,
        ),
        Mode::ConfirmDeleteGroup => {
            render_confirm_delete_group_overlay_for_view(app, client_view, frame, terminal_area)
        }
        Mode::ContextMenu => render_context_menu_for_view(app, client_view, frame),
        Mode::Settings => render_settings_overlay_for_view(app, client_view, frame, frame.area()),
        Mode::RenameWorkspace | Mode::RenameGroup | Mode::RenameTab | Mode::RenamePane => {
            render_rename_overlay_for_view(app, client_view, frame, frame.area())
        }
        Mode::GlobalMenu => render_global_launcher_menu_for_view(app, client_view, frame),
        Mode::GroupMenu => render_group_menu_for_view(app, client_view, frame),
        Mode::AgentMenu => render_agent_menu_for_view(app, client_view, frame),
        Mode::KeybindHelp => render_keybind_help_overlay_for_view(app, client_view, frame),
        Mode::Navigator => {
            render_navigator_overlay_for_view(app, client_view, terminal_runtimes, frame)
        }
        Mode::CommandPalette => render_command_palette_overlay_for_view(app, client_view, frame),
        Mode::AgentProfilePicker => {
            render_agent_profile_picker_overlay_for_view(app, client_view, frame)
        }
        Mode::GitRepoPicker => render_git_repo_picker_overlay_for_view(app, client_view, frame),
        Mode::ConfigDiagnostics => {
            render_config_diagnostics_overlay_for_view(app, client_view, frame)
        }
        Mode::Terminal | Mode::Github => {}
    }
    render_notifications(app, client_view, frame, terminal_area);
    if client_view.popup_pane.is_some() {
        render_popup_pane_for_view(app, client_view, terminal_runtimes, frame, frame.area());
    }
    if client_view.authentication_prompt.is_some() {
        dialogs::render_authentication_overlay_for_view(app, client_view, frame, frame.area());
    }
}

fn render_notifications(
    app: &AppState,
    client_view: &ClientViewState,
    frame: &mut Frame,
    terminal_area: Rect,
) {
    let mut copy_feedback_offset = 0;
    let mut toast_rect = None;
    if let Some(toast) = &app.toast {
        if client_view.computed.layout == ViewLayout::Mobile {
            render_mobile_toast_banner(frame, frame.area(), toast, &app.palette);
            toast_rect = Some(mobile_toast_banner_rect(frame.area()));
        } else {
            let position = toast.position.unwrap_or(app.toast_config.gardn.position);
            render_toast_notification(frame, frame.area(), toast, position, &app.palette);
            toast_rect = Some(toast_notification_rect(frame.area(), toast, position));
        }
    }
    if let Some(feedback) = &app.copy_feedback {
        let area = if client_view.computed.layout == ViewLayout::Mobile {
            frame.area()
        } else {
            terminal_area
        };
        if let Some(toast_rect) = toast_rect {
            copy_feedback_offset = copy_feedback_offset_for_toast(
                area,
                feedback,
                copy_feedback_offset,
                app.toast_config.clipboard.position,
                toast_rect,
            );
        }
        render_copy_feedback(
            frame,
            area,
            feedback,
            copy_feedback_offset,
            app.toast_config.clipboard.position,
            &app.palette,
        );
    }
}

fn copy_feedback_offset_for_toast(
    area: Rect,
    feedback: &crate::app::state::CopyFeedback,
    base_offset: u16,
    position: crate::config::ToastClipboardPosition,
    toast_rect: Rect,
) -> u16 {
    let feedback_rect = copy_feedback_rect(area, feedback, base_offset, position);
    if rects_overlap(feedback_rect, toast_rect) {
        base_offset.saturating_add(toast_rect.height)
    } else {
        base_offset
    }
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x.saturating_add(b.width)
        && b.x < a.x.saturating_add(a.width)
        && a.y < b.y.saturating_add(b.height)
        && b.y < a.y.saturating_add(a.height)
}

fn dim_background(frame: &mut Frame, area: Rect) {
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            let cell = &mut buf[(x, y)];
            cell.set_style(cell.style().add_modifier(Modifier::DIM));
        }
    }
}

/// Floating overlay for navigate mode — appears at bottom of terminal area.
fn _build_hints(items: &[(&str, &str)], key_style: Style, dim_style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    spans.push(Span::raw(" "));
    for (i, (k, desc)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", dim_style));
        }
        spans.push(Span::styled(k.to_string(), key_style));
        spans.push(Span::styled(format!(" {desc}"), dim_style));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn settings_modal_keeps_toast_visible() {
        let mut app = AppState::test_new();
        app.toast = Some(crate::app::state::ToastNotification {
            kind: crate::app::state::ToastKind::NeedsAttention,
            title: "SSH connection test".to_string(),
            context: "execution worker setup is required".to_string(),
            position: Some(crate::config::ToastGardnPosition::BottomLeft),
            target: None,
        });
        let area = Rect::new(0, 0, 100, 30);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut view = ClientViewState::from_default_client_state(&app);
        view.mode = Mode::Settings;
        compute_view(
            &app,
            &mut view,
            &terminal_runtimes,
            area,
            crate::kitty_graphics::HostCellSize::default(),
            PaneResizeAuthority::Denied,
        );
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        terminal
            .draw(|frame| render(&app, &view, &terminal_runtimes, frame))
            .expect("render settings with toast");

        let rendered = buffer_text(terminal.backend().buffer(), area);
        assert!(rendered.contains("SSH connection test"), "{rendered:?}");
        assert!(
            rendered.contains("execution worker setup is required"),
            "{rendered:?}"
        );
    }

    #[test]
    fn context_bar_uses_each_clients_active_workspace() {
        let mut app = AppState::test_new();
        app.context_bar_visibility = crate::config::ContextBarVisibilityConfig::Always;
        let mut first = Workspace::test_new("ignored");
        first.custom_name = Some("frontend".into());
        let mut second = Workspace::test_new("ignored");
        second.custom_name = Some("backend".into());
        app.workspaces = vec![first, second];

        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut first_view = ClientViewState::from_default_client_state(&app);
        first_view.active_workspace = Some(0);
        first_view.selected_workspace = 0;
        let mut second_view = ClientViewState::from_default_client_state(&app);
        second_view.active_workspace = Some(1);
        second_view.selected_workspace = 1;
        for view in [&mut first_view, &mut second_view] {
            compute_view(
                &app,
                view,
                &terminal_runtimes,
                Rect::new(0, 0, 100, 20),
                crate::kitty_graphics::HostCellSize::default(),
                PaneResizeAuthority::Denied,
            );
        }

        let first_path = context_path(&first_view);
        let second_path = context_path(&second_view);
        assert!(first_path.contains("frontend"), "{first_path:?}");
        assert!(!first_path.contains("backend"), "{first_path:?}");
        assert!(second_path.contains("backend"), "{second_path:?}");
        assert!(!second_path.contains("frontend"), "{second_path:?}");
    }

    #[test]
    fn narrow_client_uses_mobile_geometry() {
        let mut app = AppState::test_new();
        app.mobile_width_threshold = 80;
        app.workspaces = vec![Workspace::test_new("mobile")];
        let area = Rect::new(0, 0, 60, 20);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let mut view = ClientViewState::from_default_client_state(&app);
        view.active_workspace = Some(0);
        view.selected_workspace = 0;

        compute_view(
            &app,
            &mut view,
            &terminal_runtimes,
            area,
            crate::kitty_graphics::HostCellSize::default(),
            PaneResizeAuthority::Denied,
        );

        assert_eq!(view.computed.layout, ViewLayout::Mobile);
        assert_eq!(view.computed.mobile_header_rect.width, area.width);
        assert_eq!(view.computed.terminal_area.width, area.width);
    }

    fn context_path(view: &ClientViewState) -> String {
        view.computed
            .context_bar
            .segments
            .iter()
            .map(|segment| segment.label.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer, area: Rect) -> String {
        let mut text = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }
}
