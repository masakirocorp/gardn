use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    Frame,
};

mod agent_profile_picker;
mod command_palette;
mod dialogs;
pub(crate) mod git_repo_picker;
mod keybind_help;
mod menus;
mod mobile;
mod modal_tabs;
mod navigator;
mod onboarding;
mod panes;
mod release_notes;
mod scrollbar;
mod settings;
mod sidebar;
mod status;
mod tabs;
mod widgets;

use self::agent_profile_picker::render_agent_profile_picker_overlay;
use self::command_palette::render_command_palette_overlay;
use self::dialogs::{
    render_confirm_close_overlay, render_confirm_delete_group_overlay, render_rename_overlay,
};
use self::git_repo_picker::render_git_repo_picker_overlay;
use self::keybind_help::render_keybind_help_overlay;
use self::menus::{
    render_agent_menu, render_context_menu, render_copy_mode_overlay, render_global_launcher_menu,
    render_group_menu, render_navigate_overlay, render_prefix_overlay, render_resize_overlay,
};
use self::mobile::{
    compute_mobile_header_hit_areas, is_mobile_width, mobile_switcher_max_scroll_for_height,
    mobile_toast_banner_rect, render_mobile_header, render_mobile_panel,
    render_mobile_toast_banner,
};
use self::navigator::render_navigator_overlay;
pub(crate) use self::onboarding::onboarding_welcome_continue_rect;
use self::onboarding::render_onboarding_overlay;
use self::panes::{
    compute_pane_infos, compute_pane_infos_for_view, render_panes, render_panes_for_view,
    resize_tab_panes,
};
pub(crate) use self::release_notes::{
    product_announcement_display_lines, release_notes_close_button_rect,
    release_notes_display_lines, release_notes_wrapped_line_count, PRODUCT_ANNOUNCEMENT_MODAL_SIZE,
    RELEASE_NOTES_MODAL_SIZE,
};
use self::release_notes::{render_product_announcement_overlay, render_release_notes_overlay};
pub(crate) use self::scrollbar::{
    pane_scrollbar_rect, release_notes_scrollbar_rect, scrollbar_offset_from_drag_row,
    scrollbar_offset_from_row, scrollbar_thumb_grab_offset, should_show_scrollbar,
};
use self::settings::render_settings_overlay;
#[cfg(test)]
pub(crate) use self::sidebar::collapsed_workspace_rows_rect;
use self::sidebar::{render_right_sidebar, render_sidebar, render_sidebar_collapsed};
use self::status::{
    copy_feedback_rect, render_config_diagnostic, render_copy_feedback, render_toast_notification,
    toast_notification_rect,
};
use self::tabs::render_tab_bar;
use self::widgets::fill_rect;
pub(crate) use self::{
    agent_profile_picker::{
        agent_profile_picker_button_rects, agent_profile_picker_inner_rect,
        agent_profile_picker_list_geometry, agent_profile_picker_popup_rect,
        agent_profile_picker_tab_chevron_at, agent_profile_picker_tab_hit_areas,
    },
    command_palette::{
        command_palette_button_rects, command_palette_inner_rect, command_palette_list_geometry,
        command_palette_popup_rect,
    },
};
pub(crate) use self::{
    dialogs::{
        confirm_close_button_rects, confirm_close_popup_rect, group_default_directory_input_rect,
        group_icon_button_rect, group_icon_picker_rects, group_name_input_rect,
        rename_button_rects, rename_modal_size,
    },
    settings::{
        settings_agents_editor_back_button_rect, settings_close_button_rect,
        settings_section_list_rect, settings_stack_areas, settings_tab_chevron_at,
        settings_tab_hit_areas,
    },
    sidebar::{
        agent_panel_body_rect, agent_panel_entries, agent_panel_entry_at_row,
        agent_panel_header_target_at_row, agent_panel_scroll_metrics, agent_panel_scrollbar_rect,
        agent_panel_toggle_rect, collapsed_agent_panel_entry_at_row,
        collapsed_agent_panel_header_target_at_row, collapsed_agent_panel_toggle_rect,
        collapsed_group_header_rect, collapsed_sidebar_sections_for_split,
        collapsed_sidebar_toggle_rect, collapsed_workspace_at_row,
        collapsed_workspace_group_header_at_row, compute_workspace_card_areas,
        compute_workspace_card_areas_in_list, compute_workspace_group_drop_areas_in_list,
        compute_workspace_group_empty_areas_in_list, compute_workspace_group_header_areas,
        compute_workspace_group_header_areas_in_list, expanded_sidebar_sections,
        expanded_sidebar_toggle_rect, left_sidebar_workspace_rect,
        right_aligned_expanded_sidebar_sections, right_aligned_sidebar_section_divider_rect,
        right_aligned_workspace_list_rect, right_sidebar_content_rect, right_sidebar_toggle_rect,
        sidebar_section_divider_rect, workspace_drop_indicator_row, workspace_list_entry_count,
        workspace_list_position_for_workspace, workspace_list_rect, workspace_list_scroll_metrics,
        workspace_list_scrollbar_rect, AgentPanelHeaderTarget,
    },
};
pub(crate) use self::{
    keybind_help::keybind_help_lines,
    mobile::{
        mobile_switcher_areas, mobile_switcher_max_scroll, mobile_switcher_target_at,
        mobile_switcher_workspace_doc_range, MobileSwitcherTarget,
    },
    panes::pane_is_scrolled_back,
    tabs::compute_tab_bar_view,
    widgets::{
        centered_popup_rect, modal_scroll_metrics, modal_stack_areas, ModalListGeometry,
        ModalListViewport,
    },
};
use crate::app::state::ViewLayout;
use crate::app::{AppState, ClientViewState, Mode};
use crate::terminal::TerminalRuntimeRegistry;

const COLLAPSED_WIDTH: u16 = 4; // num + space + dot + separator
const RIGHT_SIDEBAR_MIN_TERMINAL_WIDTH: u16 = 56;
#[allow(dead_code)]
pub(crate) const MIN_SIDEBAR_WIDTH: u16 = 18;
#[allow(dead_code)]
pub(crate) const MAX_SIDEBAR_WIDTH: u16 = 36;
pub(crate) const MIN_RIGHT_SIDEBAR_WIDTH: u16 = 18;
pub(crate) const MAX_RIGHT_SIDEBAR_WIDTH: u16 = 36;

// Braille spinner frames — smooth rotation
const SPINNERS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Map spinner_tick (incremented every frame at ~60fps) to a spinner frame.
/// We want ~8 updates/sec so divide by 8.
pub(super) fn spinner_frame(tick: u32) -> &'static str {
    SPINNERS[(tick as usize / 8) % SPINNERS.len()]
}

/// Compute view geometry and reconcile pane sizes.
/// Called before render to separate mutation from drawing.
#[cfg_attr(not(test), allow(dead_code))]
pub fn compute_view(app: &mut AppState, area: Rect) {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    compute_view_with_runtime_registry(app, &terminal_runtimes, area);
}

pub fn compute_view_with_runtime_registry(
    app: &mut AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
) {
    compute_view_internal(
        app,
        terminal_runtimes,
        area,
        true,
        crate::kitty_graphics::HostCellSize::default(),
    );
}

pub fn compute_view_with_cell_size(
    app: &mut AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    compute_view_internal(app, terminal_runtimes, area, true, cell_size);
}

/// Compute view geometry for a client-sized render without resizing pane runtimes.
///
/// This is used by the headless server when a non-foreground client needs its
/// own frame size while the shared pane runtimes stay pinned to the foreground
/// client.
pub(crate) fn compute_view_without_resizing_panes(
    app: &mut AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
) {
    compute_view_internal(
        app,
        terminal_runtimes,
        area,
        false,
        crate::kitty_graphics::HostCellSize::default(),
    );
}

pub(crate) fn compute_view_for_client_with_cell_size(
    app: &mut AppState,
    client_view: &mut ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    compute_view_for_client_internal(app, client_view, terminal_runtimes, area, true, cell_size);
}

pub(crate) fn compute_view_for_client_without_resizing_panes(
    app: &mut AppState,
    client_view: &mut ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
) {
    compute_view_for_client_internal(
        app,
        client_view,
        terminal_runtimes,
        area,
        false,
        crate::kitty_graphics::HostCellSize::default(),
    );
}

fn hydrate_client_render_state(state: &mut AppState, view: &ClientViewState) {
    state.active = view.active_workspace;
    state.selected = view
        .selected_workspace
        .min(state.workspaces.len().saturating_sub(1));
    state.active_group = view.active_group.min(state.groups.len().saturating_sub(1));
    state.group_filter_enabled = view.group_filter_enabled;
    state.agent_panel_scope = view.agent_panel_scope;
    state.workspace_scroll = view.workspace_scroll;
    state.agent_panel_scroll = view.agent_panel_scroll;
    state.tab_scroll = view.tab_scroll;
    state.tab_scroll_follow_active = view.tab_scroll_follow_active;
    state.hovered_tab = view.hovered_tab;
    state.mobile_switcher_scroll = view.mobile_switcher_scroll;
    state.sidebar_width = view.sidebar_width;
    state.sidebar_width_source = view.sidebar_width_source;
    state.sidebar_collapsed = view.sidebar_collapsed;
    state.right_sidebar_collapsed = view.right_sidebar_collapsed;
    state.right_sidebar_width = view.right_sidebar_width;
    state.sidebar_section_split = view.sidebar_section_split;
    state.activity_agents_expanded = view.activity_agents_expanded;
    state.activity_commands_expanded = view.activity_commands_expanded;
    state.activity_ports_expanded = view.activity_ports_expanded;
    state.collapsed_agent_sections = view.collapsed_agent_sections.clone();
    state.collapsed_command_groups = view.collapsed_command_groups.clone();
    state.collapsed_command_status_groups = view.collapsed_command_status_groups.clone();
    state.collapsed_workspace_groups = view.collapsed_workspace_groups.clone();
    state.mode = view.mode;
    state.settings = view.settings.clone();
    state.command_palette = view.command_palette.clone();
    state.navigator = view.navigator.clone();
    state.agent_profile_picker = view.agent_profile_picker.clone();
    state.git_repo_picker = view.git_repo_picker.clone();
    state.context_menu = view.context_menu.clone();
    state.selection = view.selection.clone();
    state.selection_autoscroll = view.selection_autoscroll.clone();
    state.drag = view.drag.clone();
    state.workspace_press = view.workspace_press.clone();
    state.tab_press = view.tab_press.clone();
    state.previous_pane_focus = view.previous_pane_focus.clone();
    state.keybind_help = view.keybind_help.clone();
    state.global_menu = view.global_menu;
    state.group_menu = view.group_menu;
    state.agent_menu = view.agent_menu;
    state.creating_new_tab = view.creating_new_tab;
    state.creating_new_group = view.creating_new_group;
    state.group_icon_input = view.group_icon_input.clone();
    state.group_default_directory_input = view.group_default_directory_input.clone();
    state.group_modal_selected_field = view.group_modal_selected_field;
    state.group_icon_picker_open = view.group_icon_picker_open;
    state.rename_group_target = view.rename_group_target;
    state.requested_new_tab_name = view.requested_new_tab_name.clone();
    state.rename_pane_target = view.rename_pane_target;
    state.confirm_delete_group = view.confirm_delete_group;
    state.name_input = view.name_input.clone();
    state.name_input_replace_on_type = view.name_input_replace_on_type;
    state.release_notes = view.release_notes.clone();
    state.product_announcement = view.product_announcement.clone();
    state.view = view.computed.clone();

    for workspace in &mut state.workspaces {
        if let Some(tab_idx) = view
            .active_tabs
            .get(&workspace.id)
            .copied()
            .filter(|idx| *idx < workspace.tabs.len())
        {
            workspace.switch_tab(tab_idx);
        }
        for (tab_idx, tab) in workspace.tabs.iter_mut().enumerate() {
            let tab_number = tab_idx + 1;
            if let Some(pane_id) = view.focused_pane_for_tab(&workspace.id, tab_number) {
                if tab.panes.contains_key(&pane_id) {
                    tab.layout.focus_pane(pane_id);
                }
            }
            tab.zoomed = view.tab_is_zoomed(&workspace.id, tab_number);
        }
    }
}

fn compute_view_for_client_internal(
    app: &mut AppState,
    client_view: &mut ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
    resize_panes: bool,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    let mut render_state = app.clone();
    hydrate_client_render_state(&mut render_state, client_view);
    compute_view_internal(
        &mut render_state,
        terminal_runtimes,
        area,
        resize_panes,
        cell_size,
    );
    client_view.computed = render_state.view.clone();
    client_view.computed.pane_infos = compute_pane_infos_for_view(
        &render_state,
        client_view,
        terminal_runtimes,
        client_view.computed.terminal_area,
        resize_panes,
        cell_size,
    );
    let tab_bar_view = client_view
        .active_workspace
        .and_then(|idx| render_state.workspaces.get(idx))
        .map(|workspace| {
            compute_tab_bar_view(
                workspace,
                client_view.computed.tab_bar_rect,
                client_view.tab_scroll,
                client_view.tab_scroll_follow_active,
                render_state.mouse_capture,
                client_view.hovered_tab,
            )
        })
        .unwrap_or_default();
    client_view.tab_scroll = tab_bar_view.scroll;
    client_view.computed.tab_hit_areas = tab_bar_view.tab_hit_areas;
    client_view.computed.tab_close_hit_areas = tab_bar_view.tab_close_hit_areas;
    client_view.computed.tab_scroll_left_hit_area = tab_bar_view.scroll_left_hit_area;
    client_view.computed.tab_scroll_right_hit_area = tab_bar_view.scroll_right_hit_area;
    client_view.computed.new_tab_hit_area = tab_bar_view.new_tab_hit_area;
}

fn resize_background_tab_panes_to_terminal_area(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    terminal_area: Rect,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    for (ws_idx, ws) in app.workspaces.iter().enumerate() {
        for (tab_idx, tab) in ws.tabs.iter().enumerate() {
            if app.active == Some(ws_idx) && tab_idx == ws.active_tab_index() {
                continue;
            }
            resize_tab_panes(app, terminal_runtimes, tab, terminal_area, cell_size);
        }
    }
}

fn compute_view_internal(
    app: &mut AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
    resize_panes: bool,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    if is_mobile_width(area, app.mobile_width_threshold) {
        compute_mobile_view(app, terminal_runtimes, area, resize_panes, cell_size);
        return;
    }

    let sidebar_w = if app.sidebar_collapsed {
        COLLAPSED_WIDTH
    } else {
        app.sidebar_width
            .clamp(app.sidebar_min_width, app.sidebar_max_width)
    };
    let right_sidebar_w = if app.right_sidebar_collapsed {
        COLLAPSED_WIDTH
    } else {
        app.right_sidebar_width
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
    let (sidebar_area, main_area, right_sidebar_area) = if separate_sidebars {
        let [sidebar_area, main_area, right_sidebar_area] = Layout::horizontal([
            Constraint::Length(sidebar_w),
            Constraint::Min(1),
            Constraint::Length(right_sidebar_w),
        ])
        .areas(area);
        (sidebar_area, main_area, right_sidebar_area)
    } else if combined_right {
        let [main_area, sidebar_area] =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(sidebar_w)]).areas(area);
        (sidebar_area, main_area, Rect::default())
    } else {
        let [sidebar_area, main_area] =
            Layout::horizontal([Constraint::Length(sidebar_w), Constraint::Min(1)]).areas(area);
        (sidebar_area, main_area, Rect::default())
    };

    let has_tabs = app.active.and_then(|i| app.workspaces.get(i)).is_some();
    let (tab_bar_rect, terminal_area) = if has_tabs && main_area.height > 1 {
        let [tab_bar_rect, terminal_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(main_area);
        (tab_bar_rect, terminal_area)
    } else {
        (Rect::default(), main_area)
    };

    app.workspace_scroll = app
        .workspace_scroll
        .min(workspace_list_entry_count(app).saturating_sub(1));
    if right_sidebar_area != Rect::default() && !app.right_sidebar_collapsed {
        let max_agent_scroll =
            agent_panel_scroll_metrics(app, right_sidebar_content_rect(right_sidebar_area), false)
                .max_offset_from_bottom;
        app.agent_panel_scroll = app.agent_panel_scroll.min(max_agent_scroll);
    } else if right_sidebar_area == Rect::default() && !app.sidebar_collapsed {
        let (_, agent_area) = if combined_right {
            right_aligned_expanded_sidebar_sections(sidebar_area, app.sidebar_section_split)
        } else {
            expanded_sidebar_sections(sidebar_area, app.sidebar_section_split)
        };
        let max_agent_scroll =
            agent_panel_scroll_metrics(app, agent_area, true).max_offset_from_bottom;
        app.agent_panel_scroll = app.agent_panel_scroll.min(max_agent_scroll);
    } else {
        app.agent_panel_scroll = 0;
    }

    let (workspace_card_areas, workspace_group_header_areas, workspace_group_empty_areas) = if app
        .sidebar_collapsed
    {
        (Vec::new(), Vec::new(), Vec::new())
    } else if right_sidebar_area != Rect::default() {
        let ws_area = left_sidebar_workspace_rect(sidebar_area);
        (
            compute_workspace_card_areas_in_list(app, ws_area),
            compute_workspace_group_header_areas_in_list(app, ws_area),
            compute_workspace_group_empty_areas_in_list(app, ws_area),
        )
    } else if combined_right {
        let ws_area = right_aligned_workspace_list_rect(sidebar_area, app.sidebar_section_split);
        (
            compute_workspace_card_areas_in_list(app, ws_area),
            compute_workspace_group_header_areas_in_list(app, ws_area),
            compute_workspace_group_empty_areas_in_list(app, ws_area),
        )
    } else {
        let ws_area = workspace_list_rect(sidebar_area, app.sidebar_section_split);
        (
            compute_workspace_card_areas_in_list(app, ws_area),
            compute_workspace_group_header_areas_in_list(app, ws_area),
            compute_workspace_group_empty_areas_in_list(app, ws_area),
        )
    };

    let tab_bar_view = app
        .active
        .and_then(|i| app.workspaces.get(i))
        .map(|ws| {
            compute_tab_bar_view(
                ws,
                tab_bar_rect,
                app.tab_scroll,
                app.tab_scroll_follow_active,
                app.mouse_capture,
                app.hovered_tab,
            )
        })
        .unwrap_or_default();
    app.tab_scroll = tab_bar_view.scroll;

    let split_borders = app
        .active
        .and_then(|i| app.workspaces.get(i))
        .and_then(|ws| ws.active_tab())
        .map(|tab| tab.layout.splits(terminal_area))
        .unwrap_or_default();

    let pane_infos = compute_pane_infos(
        app,
        terminal_runtimes,
        terminal_area,
        resize_panes,
        cell_size,
    );
    if resize_panes {
        resize_background_tab_panes_to_terminal_area(
            app,
            terminal_runtimes,
            terminal_area,
            cell_size,
        );
    }

    let toast_hit_area = app
        .toast
        .as_ref()
        .map(|toast| {
            toast_notification_rect(
                area,
                toast,
                app.config_diagnostic.is_some(),
                toast.position.unwrap_or(app.toast_config.hako.position),
            )
        })
        .unwrap_or_default();

    app.view = crate::app::ViewState {
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
        terminal_area,
        mobile_header_rect: Rect::default(),
        mobile_menu_hit_area: Rect::default(),
        toast_hit_area,
        pane_infos,
        split_borders,
    };
}

fn compute_mobile_view(
    app: &mut AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
    resize_panes: bool,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    let header_h = area.height.min(2);
    let (header_rect, terminal_area) = if area.height > header_h {
        let [header_rect, terminal_area] =
            Layout::vertical([Constraint::Length(header_h), Constraint::Min(1)]).areas(area);
        (header_rect, terminal_area)
    } else {
        (area, Rect::default())
    };

    if app.mode == Mode::Navigate {
        let switcher_viewport_h = area.height.saturating_sub(header_h + 1);
        let max_scroll = mobile_switcher_max_scroll_for_height(app, switcher_viewport_h);
        app.mobile_switcher_scroll = app.mobile_switcher_scroll.min(max_scroll);
    }

    let split_borders = app
        .active
        .and_then(|i| app.workspaces.get(i))
        .and_then(|ws| ws.active_tab())
        .map(|tab| tab.layout.splits(terminal_area))
        .unwrap_or_default();

    let pane_infos = compute_pane_infos(
        app,
        terminal_runtimes,
        terminal_area,
        resize_panes,
        cell_size,
    );
    if resize_panes {
        resize_background_tab_panes_to_terminal_area(
            app,
            terminal_runtimes,
            terminal_area,
            cell_size,
        );
    }
    let header_hits = compute_mobile_header_hit_areas(app, header_rect);

    let toast_hit_area = app
        .toast
        .as_ref()
        .map(|_| mobile_toast_banner_rect(area, app.config_diagnostic.is_some()))
        .unwrap_or_default();

    app.view = crate::app::ViewState {
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
        terminal_area,
        mobile_header_rect: header_rect,
        mobile_menu_hit_area: header_hits.menu,
        toast_hit_area,
        pane_infos,
        split_borders,
    };
}

/// Render the UI — reads AppState but does not mutate it.
#[cfg_attr(not(test), allow(dead_code))]
pub fn render(app: &AppState, frame: &mut Frame) {
    let terminal_runtimes = TerminalRuntimeRegistry::new();
    render_with_runtime_registry(app, &terminal_runtimes, frame);
}

pub fn render_with_runtime_registry(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
) {
    fill_rect(
        frame,
        frame.area(),
        Style::default().bg(app.palette.panel_bg),
    );
    let sidebar_area = app.view.sidebar_rect;
    let right_sidebar_area = app.view.right_sidebar_rect;
    let tab_bar_area = app.view.tab_bar_rect;
    let terminal_area = app.view.terminal_area;

    if app.view.layout == ViewLayout::Mobile {
        render_mobile_header(app, terminal_runtimes, frame, app.view.mobile_header_rect);
    } else if app.sidebar_collapsed {
        render_sidebar_collapsed(app, frame, sidebar_area);
    } else {
        render_sidebar(app, terminal_runtimes, frame, sidebar_area);
    }
    if app.view.layout != ViewLayout::Mobile {
        render_tab_bar(app, frame, tab_bar_area);
    }
    render_panes(app, terminal_runtimes, frame, terminal_area);
    if right_sidebar_area != Rect::default() {
        render_right_sidebar(app, terminal_runtimes, frame, right_sidebar_area);
    }

    // Ambient notifications sit above panes, but below interactive overlays.
    render_notifications(app, frame, terminal_area);

    match app.mode {
        Mode::Onboarding => render_onboarding_overlay(app, frame, frame.area()),
        Mode::ReleaseNotes => render_release_notes_overlay(app, frame, frame.area()),
        Mode::ProductAnnouncement => render_product_announcement_overlay(app, frame, frame.area()),
        Mode::Navigate if app.view.layout == ViewLayout::Mobile => {
            render_mobile_panel(app, terminal_runtimes, frame, frame.area())
        }
        Mode::Navigate => render_navigate_overlay(app, frame, terminal_area),
        Mode::Prefix => render_prefix_overlay(app, frame, terminal_area),
        Mode::Copy => render_copy_mode_overlay(app, frame, terminal_area),
        Mode::Resize => render_resize_overlay(app, frame, terminal_area),
        Mode::ConfirmClose => render_confirm_close_overlay(app, frame, terminal_area),
        Mode::ConfirmDeleteGroup => render_confirm_delete_group_overlay(app, frame, terminal_area),
        Mode::ContextMenu => render_context_menu(app, frame),
        Mode::Settings => render_settings_overlay(app, frame, frame.area()),
        Mode::RenameWorkspace
        | Mode::RenameGroup
        | Mode::RenameTab
        | Mode::RenamePane
        | Mode::EditWorktreeDirectory => render_rename_overlay(app, frame, frame.area()),
        Mode::GlobalMenu => render_global_launcher_menu(app, frame),
        Mode::GroupMenu => render_group_menu(app, frame),
        Mode::AgentMenu => render_agent_menu(app, frame),
        Mode::KeybindHelp => render_keybind_help_overlay(app, frame),
        Mode::Navigator => render_navigator_overlay(app, frame),
        Mode::CommandPalette => render_command_palette_overlay(app, frame),
        Mode::AgentProfilePicker => render_agent_profile_picker_overlay(app, frame),
        Mode::GitRepoPicker => render_git_repo_picker_overlay(app, frame),
        Mode::Terminal => {}
    }
}

pub fn render_with_runtime_registry_for_view(
    app: &AppState,
    client_view: &ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
) {
    let mut render_state = app.clone();
    hydrate_client_render_state(&mut render_state, client_view);
    fill_rect(
        frame,
        frame.area(),
        Style::default().bg(render_state.palette.panel_bg),
    );
    let sidebar_area = client_view.computed.sidebar_rect;
    let right_sidebar_area = client_view.computed.right_sidebar_rect;
    let tab_bar_area = client_view.computed.tab_bar_rect;
    let terminal_area = client_view.computed.terminal_area;

    if client_view.computed.layout == ViewLayout::Mobile {
        render_mobile_header(
            &render_state,
            terminal_runtimes,
            frame,
            client_view.computed.mobile_header_rect,
        );
    } else if client_view.sidebar_collapsed {
        render_sidebar_collapsed(&render_state, frame, sidebar_area);
    } else {
        render_sidebar(&render_state, terminal_runtimes, frame, sidebar_area);
    }
    if client_view.computed.layout != ViewLayout::Mobile {
        render_tab_bar(&render_state, frame, tab_bar_area);
    }
    render_panes_for_view(
        &render_state,
        client_view,
        terminal_runtimes,
        frame,
        terminal_area,
    );
    if right_sidebar_area != Rect::default() {
        render_right_sidebar(&render_state, terminal_runtimes, frame, right_sidebar_area);
    }

    render_notifications(&render_state, frame, terminal_area);

    match client_view.mode {
        Mode::Onboarding => render_onboarding_overlay(&render_state, frame, frame.area()),
        Mode::ReleaseNotes => render_release_notes_overlay(&render_state, frame, frame.area()),
        Mode::ProductAnnouncement => {
            render_product_announcement_overlay(&render_state, frame, frame.area())
        }
        Mode::Navigate if client_view.computed.layout == ViewLayout::Mobile => {
            render_mobile_panel(&render_state, terminal_runtimes, frame, frame.area())
        }
        Mode::Navigate => render_navigate_overlay(&render_state, frame, terminal_area),
        Mode::Prefix => render_prefix_overlay(&render_state, frame, terminal_area),
        Mode::Copy => render_copy_mode_overlay(&render_state, frame, terminal_area),
        Mode::Resize => render_resize_overlay(&render_state, frame, terminal_area),
        Mode::ConfirmClose => render_confirm_close_overlay(&render_state, frame, terminal_area),
        Mode::ConfirmDeleteGroup => {
            render_confirm_delete_group_overlay(&render_state, frame, terminal_area)
        }
        Mode::ContextMenu => render_context_menu(&render_state, frame),
        Mode::Settings => render_settings_overlay(&render_state, frame, frame.area()),
        Mode::RenameWorkspace
        | Mode::RenameGroup
        | Mode::RenameTab
        | Mode::RenamePane
        | Mode::EditWorktreeDirectory => render_rename_overlay(&render_state, frame, frame.area()),
        Mode::GlobalMenu => render_global_launcher_menu(&render_state, frame),
        Mode::GroupMenu => render_group_menu(&render_state, frame),
        Mode::AgentMenu => render_agent_menu(&render_state, frame),
        Mode::KeybindHelp => render_keybind_help_overlay(&render_state, frame),
        Mode::Navigator => render_navigator_overlay(&render_state, frame),
        Mode::CommandPalette => render_command_palette_overlay(&render_state, frame),
        Mode::AgentProfilePicker => render_agent_profile_picker_overlay(&render_state, frame),
        Mode::GitRepoPicker => render_git_repo_picker_overlay(&render_state, frame),
        Mode::Terminal => {}
    }
}

fn render_notifications(app: &AppState, frame: &mut Frame, terminal_area: Rect) {
    let has_config_diagnostic = app.config_diagnostic.is_some();
    if let Some(message) = &app.config_diagnostic {
        render_config_diagnostic(frame, terminal_area, message, &app.palette);
    }
    let mut copy_feedback_offset = u16::from(has_config_diagnostic);
    let mut toast_rect = None;
    if let Some(toast) = &app.toast {
        if app.view.layout == ViewLayout::Mobile {
            render_mobile_toast_banner(
                frame,
                frame.area(),
                toast,
                has_config_diagnostic,
                &app.palette,
            );
            toast_rect = Some(mobile_toast_banner_rect(
                frame.area(),
                has_config_diagnostic,
            ));
        } else {
            let position = toast.position.unwrap_or(app.toast_config.hako.position);
            render_toast_notification(
                frame,
                frame.area(),
                toast,
                has_config_diagnostic,
                position,
                &app.palette,
            );
            toast_rect = Some(toast_notification_rect(
                frame.area(),
                toast,
                has_config_diagnostic,
                position,
            ));
        }
    }
    if let Some(feedback) = &app.copy_feedback {
        let area = if app.view.layout == ViewLayout::Mobile {
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
    use super::keybind_help::keybind_help_groups;
    use super::scrollbar::scrollbar_thumb;
    use super::*;
    use crate::{
        app::state::ViewLayout,
        layout::PaneInfo,
        workspace::{GitWorkSummary, Workspace},
    };
    use ratatui::{backend::TestBackend, style::Color, Terminal};

    #[tokio::test]
    async fn focused_pane_cursor_wins_during_terminal_render() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("test");
        let first_pane = ws.tabs[0].root_pane;
        let second_pane = ws.test_split(ratatui::layout::Direction::Horizontal);

        ws.insert_test_runtime(
            first_pane,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(20, 5, b"left"),
        );
        ws.insert_test_runtime(
            second_pane,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(20, 5, b"r\r\nb"),
        );
        ws.tabs[0].layout.focus_pane(first_pane);

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;

        compute_view(&mut app, Rect::new(0, 0, 80, 20));
        let focused = app
            .view
            .pane_infos
            .iter()
            .find(|info| info.id == first_pane)
            .expect("focused pane info");

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();

        terminal
            .backend_mut()
            .assert_cursor_position((focused.inner_rect.x + 4, focused.inner_rect.y));
    }

    #[test]
    fn mobile_width_uses_header_and_full_width_terminal() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one")];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;

        compute_view(&mut app, Rect::new(0, 0, 44, 20));

        assert_eq!(app.view.layout, ViewLayout::Mobile);
        assert_eq!(app.view.sidebar_rect, Rect::default());
        assert_eq!(app.view.tab_bar_rect, Rect::default());
        assert_eq!(app.view.mobile_header_rect, Rect::new(0, 0, 44, 2));
        assert_eq!(app.view.terminal_area, Rect::new(0, 2, 44, 18));
        assert_eq!(app.view.mobile_menu_hit_area.height, 2);
        assert_eq!(
            app.view.mobile_menu_hit_area.x + app.view.mobile_menu_hit_area.width,
            44
        );
    }

    #[test]
    fn configured_mobile_width_threshold_controls_layout_switch() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one")];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;

        compute_view(&mut app, Rect::new(0, 0, 80, 20));
        assert_eq!(app.view.layout, ViewLayout::Desktop);

        app.mobile_width_threshold = 90;
        compute_view(&mut app, Rect::new(0, 0, 80, 20));
        assert_eq!(app.view.layout, ViewLayout::Mobile);
        assert_eq!(app.view.mobile_header_rect, Rect::new(0, 0, 80, 2));
        assert_eq!(app.view.terminal_area, Rect::new(0, 2, 80, 18));
    }

    #[test]
    fn desktop_layout_uses_full_terminal_frame_without_outer_inset() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one")];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;

        let area = Rect::new(0, 0, 80, 20);
        compute_view(&mut app, area);

        assert_eq!(app.view.layout, ViewLayout::Desktop);
        assert_eq!(app.view.sidebar_rect.x, area.x);
        assert_eq!(app.view.sidebar_rect.y, area.y);
        assert_eq!(app.view.sidebar_rect.height, area.height);
        assert_eq!(
            app.view.terminal_area.x + app.view.terminal_area.width,
            area.width
        );
        assert_eq!(
            app.view.terminal_area.y + app.view.terminal_area.height,
            area.height
        );
    }

    #[tokio::test]
    async fn desktop_theme_background_paints_chrome_and_pane_defaults() {
        let mut app = crate::app::state::AppState::test_new();
        app.palette.panel_bg = Color::Rgb(1, 2, 3);
        let mut ws = Workspace::test_new("test");
        let root = ws.tabs[0].root_pane;
        ws.tabs[0].runtimes.insert(
            root,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(20, 5, b""),
        );
        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;

        compute_view(&mut app, Rect::new(0, 0, 80, 20));
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(0, 0)].style().bg, Some(app.palette.panel_bg));
        assert_eq!(
            buffer[(app.view.sidebar_rect.x, app.view.sidebar_rect.y)]
                .style()
                .bg,
            Some(app.palette.panel_bg)
        );
        assert_eq!(
            buffer[(app.view.terminal_area.x, app.view.terminal_area.y)]
                .style()
                .bg,
            Some(app.palette.panel_bg)
        );
    }

    #[test]
    fn product_announcement_renders_above_config_diagnostic() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one")];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::ProductAnnouncement;
        app.product_announcement = Some(crate::app::state::ProductAnnouncementState {
            version: "0.6.0".into(),
            id: "keybinding-v2".into(),
            title: "Keybinding syntax changed".into(),
            body: "### Update\n- Body".into(),
            scroll: 0,
            preview: false,
        });
        app.config_diagnostic = Some(
            "unsafe direct keybinding: keys.new_workspace = \"n\"\nunsafe direct keybinding: keys.new_tab = \"c\""
                .into(),
        );

        let area = Rect::new(0, 0, 44, 20);
        compute_view(&mut app, area);

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();

        let popup = centered_popup_rect(
            area,
            PRODUCT_ANNOUNCEMENT_MODAL_SIZE.0,
            PRODUCT_ANNOUNCEMENT_MODAL_SIZE.1,
        )
        .expect("announcement popup");
        let title_row = popup.y + 1;
        let row = buffer_row_text(buffer, Rect::new(0, title_row, area.width, 1), title_row);

        assert!(row.contains("Keybinding syntax changed"));
        assert!(!row.contains("config warning"));
    }

    #[test]
    fn compute_view_clamps_sidebar_width_to_configured_max() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one")];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;
        app.sidebar_max_width = 30;
        app.sidebar_width = 999;

        compute_view(&mut app, Rect::new(0, 0, 100, 20));

        assert_eq!(app.view.sidebar_rect.width, 30);
    }

    #[test]
    fn compute_view_clamps_sidebar_width_to_configured_min() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one")];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;
        app.sidebar_min_width = 22;
        app.sidebar_width = 5;

        compute_view(&mut app, Rect::new(0, 0, 100, 20));

        assert_eq!(app.view.sidebar_rect.width, 22);
    }

    #[test]
    fn combined_right_sidebar_keeps_workspace_list_above_agent_panel() {
        let mut app = crate::app::state::AppState::test_new();
        app.sidebar_arrangement = crate::config::SidebarArrangementConfig::CombinedRight;
        app.workspaces = vec![Workspace::test_new("one")];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;

        compute_view(&mut app, Rect::new(0, 0, 100, 20));

        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let sidebar = app.view.sidebar_rect;
        let content = Rect::new(
            sidebar.x + 1,
            sidebar.y,
            sidebar.width.saturating_sub(1),
            sidebar.height,
        );
        let row_containing = |needle: &str| {
            (content.y..content.y + content.height)
                .find(|row| buffer_row_text(buffer, content, *row).contains(needle))
                .expect(needle)
        };
        let workspace_row = row_containing("one");
        let agents_row = row_containing("agents");

        assert_eq!(app.view.right_sidebar_rect, Rect::default());
        assert!(sidebar.x > 0);
        assert_eq!(buffer[(sidebar.x, sidebar.y)].symbol(), "│");
        assert!(workspace_row < agents_row);
    }

    #[test]
    fn collapsed_sidebar_keeps_active_workspace_highlight_in_terminal_mode() {
        let mut app = crate::app::state::AppState::test_new();
        app.sidebar_collapsed = true;
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.active = Some(1);
        app.selected = 0;
        app.mode = Mode::Terminal;

        compute_view(&mut app, Rect::new(0, 0, 80, 20));

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();

        let rows = collapsed_workspace_rows_rect(app.view.sidebar_rect, true);
        let active_row = rows.y + 1;
        let active_style = buffer[(rows.x, active_row)].style();

        assert_eq!(active_style.bg, Some(app.palette.surface_dim));
    }

    #[test]
    fn collapsed_sidebar_empty_state_keeps_agents_label() {
        let mut app = crate::app::state::AppState::test_new();
        app.sidebar_collapsed = true;
        app.workspaces.clear();
        app.active = None;
        app.mode = Mode::Terminal;

        compute_view(&mut app, Rect::new(0, 0, 80, 20));

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let text = (app.view.sidebar_rect.y
            ..app.view.sidebar_rect.y + app.view.sidebar_rect.height)
            .map(|row| buffer_row_text(buffer, app.view.sidebar_rect, row))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("f:s"));
        assert!(!text.contains("agt"));
    }

    #[test]
    fn expanded_sidebar_workspace_rows_hide_clean_work_summary() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("one");
        ws.cached_git_work_summary = Some(GitWorkSummary {
            repo_count: 1,
            ..GitWorkSummary::default()
        });

        app.workspaces = vec![ws];
        app.ensure_test_terminals();
        app.selected = 0;
        app.mode = Mode::Navigate;

        compute_view(&mut app, Rect::new(0, 0, 80, 20));

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();

        let card = app.view.workspace_card_areas[0].rect;
        let line1 = buffer_row_text(buffer, card, card.y);
        let line2 = buffer_row_text(buffer, card, card.y + 1);

        assert!(line1.starts_with("  · one"));
        assert!(!line1.contains("1 one"));
        assert_eq!(line2, "");
    }

    #[test]
    fn expanded_sidebar_work_summary_colors_stats_by_kind() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("one");
        ws.cached_git_work_summary = Some(GitWorkSummary {
            repo_count: 1,
            added: 2,
            modified: 1,
            deleted: 1,
            ..GitWorkSummary::default()
        });

        app.workspaces = vec![ws];
        compute_view(&mut app, Rect::new(0, 0, 80, 20));

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let card = app.view.workspace_card_areas[0].rect;
        let row = card.y + 1;

        assert_eq!(buffer_row_text(buffer, card, row), "    +2 ~1 -1");
        assert_eq!(
            buffer[(card.x + 4, row)].style().fg,
            Some(app.palette.green)
        );
        assert_eq!(
            buffer[(card.x + 7, row)].style().fg,
            Some(app.palette.yellow)
        );
        assert_eq!(buffer[(card.x + 10, row)].style().fg, Some(app.palette.red));
    }

    #[test]
    fn tab_bar_dims_auto_named_tabs_and_emphasizes_custom_tabs() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("test");
        let custom_tab = ws.test_add_tab(Some("logs"));
        ws.switch_tab(custom_tab);

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;

        compute_view(&mut app, Rect::new(0, 0, 80, 20));

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();

        let auto_rect = app.view.tab_hit_areas[0];
        let custom_rect = app.view.tab_hit_areas[1];
        let auto_style = buffer[(auto_rect.x + 1, auto_rect.y)].style();
        let custom_style = buffer[(custom_rect.x + 1, custom_rect.y)].style();

        assert_eq!(auto_style.fg, Some(app.palette.overlay0));
        assert!(auto_style.add_modifier.contains(Modifier::DIM));
        assert_eq!(custom_style.fg, Some(app.palette.panel_bg));
        assert!(custom_style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn tab_bar_uses_surface_dim_when_panel_background_resets() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("test");
        let custom_tab = ws.test_add_tab(Some("logs"));
        ws.switch_tab(custom_tab);

        app.palette.panel_bg = Color::Reset;
        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;

        compute_view(&mut app, Rect::new(0, 0, 80, 20));

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();

        let custom_rect = app.view.tab_hit_areas[1];
        let custom_style = buffer[(custom_rect.x + 1, custom_rect.y)].style();

        assert_eq!(custom_style.bg, Some(app.palette.accent));
        assert_eq!(custom_style.fg, Some(app.palette.surface_dim));
        assert!(custom_style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn tab_bar_shows_close_icon_only_for_hovered_tab() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.test_add_tab(Some("logs"));

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;

        compute_view(&mut app, Rect::new(0, 0, 80, 20));
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let tab_row = app.view.tab_bar_rect.y;
        assert!(!buffer_row_text(buffer, app.view.tab_bar_rect, tab_row).contains('×'));
        assert!(app
            .view
            .tab_close_hit_areas
            .iter()
            .all(|rect| rect.width == 0));

        app.hovered_tab = Some(1);
        compute_view(&mut app, Rect::new(0, 0, 80, 20));
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let close_rect = app.view.tab_close_hit_areas[1];

        assert_eq!(close_rect.width, 1);
        assert_eq!(buffer[(close_rect.x, close_rect.y)].symbol(), "✕");
        assert_eq!(
            buffer[(
                app.view.tab_hit_areas[0].x + app.view.tab_hit_areas[0].width - 1,
                tab_row
            )]
                .symbol(),
            " "
        );
    }

    #[test]
    fn hovered_truncated_tab_keeps_close_icon_visible() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("very-long-tab-name-0".into());
        for idx in 1..14 {
            ws.test_add_tab(Some(&format!("very-long-tab-name-{idx}")));
        }

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;
        app.mouse_capture = true;
        app.tab_scroll_follow_active = false;

        let (area, truncated_idx) = (44..=80)
            .find_map(|width| {
                compute_view(&mut app, Rect::new(0, 0, width, 20));
                let candidates = app
                    .view
                    .tab_hit_areas
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, rect)| (rect.width > 3 && rect.width < 8).then_some(idx))
                    .collect::<Vec<_>>();
                for idx in candidates {
                    app.hovered_tab = Some(idx);
                    let area = Rect::new(0, 0, width, 20);
                    compute_view(&mut app, area);
                    if app.view.tab_close_hit_areas[idx].width > 0 {
                        return Some((area, idx));
                    }
                }
                app.hovered_tab = None;
                None
            })
            .expect("naturally truncated visible tab with close affordance");

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let tab_rect = app.view.tab_hit_areas[truncated_idx];
        let rendered_symbols = (tab_rect.x..tab_rect.x + tab_rect.width)
            .map(|x| buffer[(x, tab_rect.y)].symbol().to_string())
            .collect::<Vec<_>>();
        let close_rect = app.view.tab_close_hit_areas[truncated_idx];
        let close_symbol = if close_rect.width > 0 {
            buffer[(close_rect.x, close_rect.y)].symbol().to_string()
        } else {
            String::new()
        };

        assert_eq!(
            close_symbol, "✕",
            "hovered truncated tab should keep the close icon visible: tab={rendered_symbols:?}, tab_rect={tab_rect:?}, close_rect={close_rect:?}"
        );
        let rendered = rendered_symbols.join("");
        assert!(
            !rendered.contains("very-long-tab-name"),
            "hovered truncated tab should not let the full label crowd out the close icon: {rendered_symbols:?}"
        );
    }

    #[test]
    fn new_tab_button_tracks_rightmost_tab_when_tabs_fit() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.test_add_tab(Some("logs"));

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;

        compute_view(&mut app, Rect::new(0, 0, 80, 20));

        let last_visible = app
            .view
            .tab_hit_areas
            .iter()
            .rev()
            .find(|rect| rect.width > 0)
            .copied()
            .expect("last visible tab");

        assert_eq!(
            app.view.new_tab_hit_area.x,
            last_visible.x + last_visible.width
        );
    }

    #[test]
    fn tab_bar_shows_scroll_controls_when_tabs_overflow() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("test");
        for name in ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta"] {
            ws.test_add_tab(Some(name));
        }

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;
        app.tab_scroll_follow_active = false;
        app.tab_scroll = 2;

        compute_view(&mut app, Rect::new(0, 0, 65, 20));

        assert!(app.view.tab_scroll_left_hit_area.width > 0);
        assert!(app.view.tab_scroll_right_hit_area.width > 0);
        assert_eq!(app.view.tab_hit_areas[0].width, 0);
        assert_eq!(app.view.tab_hit_areas[1].width, 0);
        assert!(app.view.tab_hit_areas[2].width > 0);
        assert!(app.view.new_tab_hit_area.width > 0);

        let last_visible = app
            .view
            .tab_hit_areas
            .iter()
            .rev()
            .find(|rect| rect.width > 0)
            .copied()
            .expect("last visible tab");

        assert_eq!(
            app.view.tab_scroll_right_hit_area.x,
            last_visible.x + last_visible.width
        );
        assert_eq!(
            app.view.new_tab_hit_area.x,
            app.view.tab_scroll_right_hit_area.x + app.view.tab_scroll_right_hit_area.width
        );
    }

    #[test]
    fn tab_bar_clamps_manual_scroll_at_last_visible_tab() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("test");
        for name in [
            "one", "two", "three", "four", "five", "six", "seven", "eight",
        ] {
            ws.test_add_tab(Some(name));
        }

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;
        app.tab_scroll_follow_active = false;
        app.tab_scroll = usize::MAX;

        compute_view(&mut app, Rect::new(0, 0, 65, 20));

        let last_idx = app.workspaces[0].tabs.len() - 1;
        assert!(app.view.tab_hit_areas[last_idx].width > 0);
        let clamped_scroll = app.tab_scroll;

        app.scroll_tabs_right();

        assert_eq!(app.tab_scroll, clamped_scroll);
        assert!(app.view.tab_hit_areas[last_idx].width > 0);
    }

    #[test]
    fn pane_scrollbar_rect_uses_reserved_rightmost_column() {
        let info = PaneInfo {
            id: crate::layout::PaneId::from_raw(1),
            rect: Rect::new(0, 0, 12, 8),
            inner_rect: Rect::new(1, 1, 9, 6),
            scrollbar_rect: Some(Rect::new(10, 1, 1, 6)),
            is_focused: true,
        };

        assert_eq!(pane_scrollbar_rect(&info), Some(Rect::new(10, 1, 1, 6)));
    }

    #[tokio::test]
    async fn compute_view_reserves_terminal_column_when_pane_scrollbar_is_visible() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        ws.insert_test_runtime(
            pane_id,
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(
                12,
                4,
                4096,
                b"000000000000\r\n111111111111\r\n222222222222\r\n333333333333\r\n444444444444\r\n",
            ),
        );

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;

        compute_view(&mut app, Rect::new(0, 0, 40, 12));

        let info = app.view.pane_infos.first().expect("pane info");
        assert_eq!(info.inner_rect.width + 1, app.view.terminal_area.width);
        assert_eq!(
            info.scrollbar_rect,
            Some(Rect::new(
                info.inner_rect.x + info.inner_rect.width,
                info.inner_rect.y,
                1,
                info.inner_rect.height,
            ))
        );
    }

    #[test]
    fn scrollbar_stays_hidden_without_scrollback() {
        let metrics = crate::pane::ScrollMetrics {
            offset_from_bottom: 0,
            max_offset_from_bottom: 0,
            viewport_rows: 5,
        };

        assert!(!should_show_scrollbar(metrics));
    }

    #[test]
    fn scrollbar_shows_with_scrollback() {
        let metrics = crate::pane::ScrollMetrics {
            offset_from_bottom: 0,
            max_offset_from_bottom: 20,
            viewport_rows: 5,
        };

        assert!(should_show_scrollbar(metrics));
    }

    #[test]
    fn modal_scroll_metrics_converts_top_scroll_to_offset_from_bottom() {
        let metrics = modal_scroll_metrics(20, 5, 3);

        assert_eq!(metrics.viewport_rows, 5);
        assert_eq!(metrics.max_offset_from_bottom, 15);
        assert_eq!(metrics.offset_from_bottom, 12);
        assert_eq!(widgets::modal_scroll_from_offset_from_bottom(20, 5, 12), 3);
    }

    #[test]
    fn modal_list_viewport_clamps_scroll_and_visible_range() {
        let viewport = ModalListViewport::new(20, 5, 99);

        assert_eq!(viewport.scroll(), 15);
        assert_eq!(viewport.max_scroll(), 15);
        assert_eq!(viewport.visible_range(), 15..20);
    }

    #[test]
    fn modal_list_viewport_keeps_selected_row_visible_with_context() {
        let viewport = ModalListViewport::new(20, 5, 6);

        assert_eq!(viewport.ensure_visible(6, Some(5)), 5);
        assert_eq!(viewport.ensure_visible(11, None), 7);
    }

    #[test]
    fn modal_list_viewport_hit_testing_rejects_scrollbar_column() {
        let viewport = ModalListViewport::new(20, 5, 3);
        let area = Rect::new(10, 4, 10, 5);

        assert_eq!(viewport.hit_visual_row(area, 11, 4), Some(3));
        assert_eq!(viewport.hit_visual_row(area, 11, 8), Some(7));
        assert_eq!(viewport.hit_visual_row(area, 19, 4), None);
        assert_eq!(viewport.hit_visual_row(area, 11, 9), None);
    }

    #[test]
    fn scrollbar_thumb_reaches_bottom_when_scrolled_to_bottom() {
        let metrics = crate::pane::ScrollMetrics {
            offset_from_bottom: 0,
            max_offset_from_bottom: 20,
            viewport_rows: 5,
        };
        let track = Rect::new(9, 4, 1, 5);

        let thumb = scrollbar_thumb(metrics, track).expect("thumb");
        assert_eq!(thumb.top + thumb.len, track.y + track.height);
    }

    #[test]
    fn scrollbar_offset_mapping_hits_top_middle_and_bottom() {
        let metrics = crate::pane::ScrollMetrics {
            offset_from_bottom: 0,
            max_offset_from_bottom: 20,
            viewport_rows: 5,
        };
        let track = Rect::new(9, 4, 1, 5);

        assert_eq!(scrollbar_offset_from_row(metrics, track, 4), 20);
        assert_eq!(scrollbar_offset_from_row(metrics, track, 6), 10);
        assert_eq!(scrollbar_offset_from_row(metrics, track, 8), 0);
    }

    #[test]
    fn dragging_from_current_thumb_row_preserves_offset() {
        let metrics = crate::pane::ScrollMetrics {
            offset_from_bottom: 7,
            max_offset_from_bottom: 20,
            viewport_rows: 5,
        };
        let track = Rect::new(9, 4, 1, 8);
        let thumb = scrollbar_thumb(metrics, track).expect("thumb");
        let row = thumb.top + thumb.len / 2;
        let grab = scrollbar_thumb_grab_offset(metrics, track, row).expect("grab");

        assert_eq!(scrollbar_offset_from_drag_row(metrics, track, row, grab), 7);
    }

    fn buffer_row_text(buffer: &ratatui::buffer::Buffer, area: Rect, row: u16) -> String {
        (area.x..area.x + area.width)
            .map(|x| buffer[(x, row)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]

    fn prefix_mode_renders_prefix_indicator() {
        let mut app = crate::app::state::AppState::test_new();
        app.mode = Mode::Prefix;
        app.view.terminal_area = ratatui::layout::Rect::new(0, 0, 60, 4);
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 4))
            .expect("test terminal");

        terminal
            .draw(|frame| render_prefix_overlay(&app, frame, app.view.terminal_area))
            .expect("draw prefix overlay");

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("PREFIX"));
        assert!(rendered.contains("esc"));
        assert!(rendered.contains("space"));
        assert!(rendered.contains("cmds"));
        assert!(rendered.contains("w"));
        assert!(rendered.contains("spaces"));
        assert!(rendered.contains("?"));
        assert!(rendered.contains("keys"));
        assert!(!rendered.contains("detach"));
    }

    #[test]
    fn prefix_mode_renders_indexed_navigation_hints_when_wide_enough() {
        let mut app = crate::app::state::AppState::test_new();
        app.mode = Mode::Prefix;
        app.view.terminal_area = ratatui::layout::Rect::new(0, 0, 120, 4);
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 4))
            .expect("test terminal");

        terminal
            .draw(|frame| render_prefix_overlay(&app, frame, app.view.terminal_area))
            .expect("draw prefix overlay");

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("1..0"));
        assert!(rendered.contains("tabs"));
        assert!(rendered.contains("shift+1..0"));
        assert!(rendered.contains("spaces"));
        assert!(rendered.contains("alt+1..0"));
        assert!(rendered.contains("groups"));
    }

    #[test]
    fn keybind_help_shows_defaults_and_unset_optional_actions() {
        let app = crate::app::state::AppState::test_new();
        let groups = keybind_help_groups(&app);

        let global = groups
            .iter()
            .find(|(name, _)| *name == "global")
            .expect("global group")
            .1
            .clone();
        let workspace_tab = groups
            .iter()
            .find(|(name, _)| *name == "workspaces / tabs")
            .expect("workspace tab group")
            .1
            .clone();
        let group_keys = groups
            .iter()
            .find(|(name, _)| *name == "groups")
            .expect("groups group")
            .1
            .clone();
        let agents = groups
            .iter()
            .find(|(name, _)| *name == "agents")
            .expect("agents group")
            .1
            .clone();
        let panes = groups
            .iter()
            .find(|(name, _)| *name == "panes")
            .expect("panes group")
            .1
            .clone();

        assert!(global
            .iter()
            .any(|(key, label)| key == "prefix+space" && label.as_ref() == "command palette"));
        assert!(agents
            .iter()
            .any(|(key, label)| key == "unset" && label.as_ref() == "open agent menu"));
        assert!(panes
            .iter()
            .any(|(key, label)| key == "unset" && label.as_ref() == "toggle right sidebar"));
        assert!(workspace_tab
            .iter()
            .any(|(key, label)| key == "unset" && label.as_ref() == "previous workspace"));
        assert!(workspace_tab
            .iter()
            .any(|(key, label)| key == "unset" && label.as_ref() == "next workspace"));
        assert!(workspace_tab
            .iter()
            .any(|(key, label)| key == "unset" && label.as_ref() == "previous agent"));
        assert!(workspace_tab
            .iter()
            .any(|(key, label)| key == "unset" && label.as_ref() == "next agent"));
        assert!(workspace_tab
            .iter()
            .any(|(key, label)| key == "unset" && label.as_ref() == "focus agent 1-9"));
        assert!(workspace_tab.iter().any(|(key, label)| {
            key == "prefix+shift+1..0" && label.as_ref() == "switch space 1-10"
        }));
        assert!(workspace_tab
            .iter()
            .any(|(key, label)| key == "prefix+1..0" && label.as_ref() == "switch tab 1-10"));
        assert!(group_keys.iter().any(|(key, label)| {
            key == "prefix+alt+1..0" && label.as_ref() == "switch group 1-10"
        }));
        assert!(panes
            .iter()
            .any(|(key, label)| key == "prefix+h" && label.as_ref() == "focus pane left"));
        assert!(panes
            .iter()
            .any(|(key, label)| key == "prefix+j" && label.as_ref() == "focus pane down"));
        assert!(panes
            .iter()
            .any(|(key, label)| key == "prefix+k" && label.as_ref() == "focus pane up"));
        assert!(panes
            .iter()
            .any(|(key, label)| key == "prefix+l" && label.as_ref() == "focus pane right"));
    }

    #[test]
    fn keybind_help_shows_custom_command_descriptions() {
        let mut app = crate::app::state::AppState::test_new();
        app.keybinds.custom_commands = vec![
            crate::config::CustomCommandKeybind {
                bindings: crate::config::ActionKeybinds::prefix("alt+g"),
                label: "prefix+alt+g".to_string(),
                command: "lazygit".to_string(),
                action: crate::config::CustomCommandAction::Pane,
                description: Some("open lazygit".to_string()),
            },
            crate::config::CustomCommandKeybind {
                bindings: crate::config::ActionKeybinds::prefix("alt+h"),
                label: "prefix+alt+h".to_string(),
                command: "echo hello".to_string(),
                action: crate::config::CustomCommandAction::Shell,
                description: None,
            },
        ];

        let groups = keybind_help_groups(&app);
        let custom = groups
            .iter()
            .find(|(name, _)| *name == "custom")
            .expect("custom group")
            .1
            .clone();
        assert!(custom
            .iter()
            .any(|(key, label)| key == "prefix+alt+g" && label.as_ref() == "open lazygit"));
        assert!(custom
            .iter()
            .any(|(key, label)| key == "prefix+alt+h" && label.as_ref() == "custom command"));

        let rendered_help = keybind_help_lines(&app)
            .into_iter()
            .flat_map(|(_, line)| line.spans)
            .map(|span| span.content.into_owned())
            .collect::<Vec<_>>()
            .join("");
        assert!(rendered_help.contains("open lazygit"));
        assert!(rendered_help.contains("custom command"));
    }
}
