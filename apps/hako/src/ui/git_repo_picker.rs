use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{view_state::ClientViewState, AppState};

use super::{
    scrollbar::render_scrollbar,
    widgets::{
        modal_hint_line_count, modal_section_heading_style, modal_stack_areas, panel_contrast_fg,
        render_modal_description, render_modal_divider, render_modal_frame, ModalFrameSpec,
        ModalListGeometry,
    },
};

const GIT_REPO_PICKER_HINTS: &[(&str, &str)] = &[("move", "↑↓"), ("open", "space/↵")];
const POPUP_WIDTH: u16 = 64;
const POPUP_HEIGHT: u16 = 20;
const HEADER_ROWS: u16 = 3;

fn repo_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
}

fn display_path(path: &std::path::Path) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

fn repo_status_spans(
    summary: crate::workspace::GitWorkSummary,
    selected: bool,
    palette: &crate::app::state::Palette,
) -> Vec<Span<'static>> {
    if summary.conflicted + summary.added + summary.modified + summary.deleted == 0 {
        let style = if selected {
            Style::default()
                .fg(panel_contrast_fg(palette))
                .bg(palette.accent)
        } else {
            Style::default().fg(palette.overlay0)
        };
        return vec![Span::styled("clean", style)];
    }

    let selected_style = || {
        Style::default()
            .fg(panel_contrast_fg(palette))
            .bg(palette.accent)
    };
    let mut spans = Vec::new();
    let mut push = |text: String, color| {
        if !spans.is_empty() {
            spans.push(Span::styled(
                " ",
                if selected {
                    selected_style()
                } else {
                    Style::default().fg(palette.overlay0)
                },
            ));
        }
        spans.push(Span::styled(
            text,
            if selected {
                selected_style()
            } else {
                Style::default().fg(color)
            },
        ));
    };
    if summary.conflicted > 0 {
        push(format!("!{}", summary.conflicted), palette.red);
    }
    if summary.added > 0 {
        push(format!("+{}", summary.added), palette.green);
    }
    if summary.modified > 0 {
        push(format!("~{}", summary.modified), palette.yellow);
    }
    if summary.deleted > 0 {
        push(format!("-{}", summary.deleted), palette.red);
    }
    spans
}

fn status_width(summary: crate::workspace::GitWorkSummary) -> usize {
    if summary.conflicted + summary.added + summary.modified + summary.deleted == 0 {
        return 5;
    }

    let mut width = 0;
    for count in [
        summary.conflicted,
        summary.added,
        summary.modified,
        summary.deleted,
    ] {
        if count == 0 {
            continue;
        }
        if width > 0 {
            width += 1;
        }
        width += 1 + count.to_string().len();
    }
    width
}

pub(crate) fn git_repo_picker_popup_rect(area: Rect) -> Option<Rect> {
    super::centered_popup_rect(area, POPUP_WIDTH, POPUP_HEIGHT)
}

pub(crate) fn git_repo_picker_inner_rect(area: Rect) -> Option<Rect> {
    let popup = git_repo_picker_popup_rect(area)?;
    Some(Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    ))
}

fn git_repo_picker_content_rows(inner: Rect) -> Option<[Rect; 5]> {
    let footer_rows = modal_hint_line_count(inner.width, GIT_REPO_PICKER_HINTS, 2);
    let stack = modal_stack_areas(inner, HEADER_ROWS, footer_rows, 0, 1);
    Some(
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas::<5>(stack.content),
    )
}

pub(crate) fn git_repo_picker_list_geometry(app: &AppState) -> Option<ModalListGeometry> {
    let layout = git_repo_picker_layout(app)?;
    Some(layout.list)
}

pub(crate) fn git_repo_picker_list_geometry_for_view(
    view: &crate::app::view_state::ClientViewState,
) -> Option<ModalListGeometry> {
    let inner = git_repo_picker_inner_rect(view.screen_rect())?;
    if inner.height < 12 || inner.width < 28 {
        return None;
    }
    let content_rows = git_repo_picker_content_rows(inner)?;
    Some(ModalListGeometry::new(
        content_rows[4],
        view.git_repo_picker.roots.len() * 2,
        view.git_repo_picker.scroll * 2,
    ))
}

pub(crate) fn git_repo_picker_index_at_for_view(
    view: &crate::app::view_state::ClientViewState,
    col: u16,
    row: u16,
) -> Option<usize> {
    let list = git_repo_picker_list_geometry_for_view(view)?;
    let visual_row = list.hit_visual_row(col, row)?;
    let index = visual_row / 2;
    (index < view.git_repo_picker.roots.len()).then_some(index)
}

pub(crate) fn git_repo_picker_index_at(app: &AppState, col: u16, row: u16) -> Option<usize> {
    let layout = git_repo_picker_layout(app)?;
    let visual_row = layout.list.hit_visual_row(col, row)?;
    let index = visual_row / 2;
    (index < app.git_repo_picker.roots.len()).then_some(index)
}

struct GitRepoPickerLayout {
    inner: Rect,
    stack: super::widgets::ModalStackAreas,
    content_rows: [Rect; 5],
    list: ModalListGeometry,
}

fn git_repo_picker_layout(app: &AppState) -> Option<GitRepoPickerLayout> {
    let inner = git_repo_picker_inner_rect(app.screen_rect())?;
    if inner.height < 12 || inner.width < 28 {
        return None;
    }
    let footer_rows = modal_hint_line_count(inner.width, GIT_REPO_PICKER_HINTS, 2);
    let stack = modal_stack_areas(inner, HEADER_ROWS, footer_rows, 0, 1);
    let content_rows = git_repo_picker_content_rows(inner)?;
    let list = ModalListGeometry::new(
        content_rows[4],
        app.git_repo_picker.roots.len() * 2,
        app.git_repo_picker.scroll * 2,
    );
    Some(GitRepoPickerLayout {
        inner,
        stack,
        content_rows,
        list,
    })
}

fn picker_palette(app: &AppState) -> crate::app::state::Palette {
    app.palette_for_workspace(app.git_repo_picker.ws_idx)
}

pub(super) fn render_git_repo_picker_overlay(app: &AppState, frame: &mut Frame) {
    super::dim_background(frame, frame.area());

    let palette = picker_palette(app);
    let Some(layout) = git_repo_picker_layout(app) else {
        return;
    };
    let Some(frame_areas) = render_modal_frame(
        frame,
        app.screen_rect(),
        &palette,
        ModalFrameSpec {
            title: "git diff",
            width: POPUP_WIDTH,
            height: POPUP_HEIGHT,
            header_rows: HEADER_ROWS,
            footer_hints: GIT_REPO_PICKER_HINTS,
            footer_max_rows: 2,
            reserve_footer_gap: 1,
            show_close: true,
        },
    ) else {
        return;
    };
    if frame_areas.inner != layout.inner {
        return;
    }

    let stack = layout.stack;
    let header_rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<3>(stack.header);
    render_modal_divider(frame, header_rows[2], &palette);

    let content_rows = layout.content_rows;
    frame.render_widget(
        Paragraph::new(Span::styled(
            " repositories",
            modal_section_heading_style(&palette),
        )),
        content_rows[0],
    );

    let workspace = app
        .workspaces
        .get(app.git_repo_picker.ws_idx)
        .map(|workspace| workspace.display_name())
        .unwrap_or_else(|| "workspace".to_string());
    render_modal_description(
        frame,
        content_rows[1],
        &format!("choose which repository to diff for {workspace}"),
        Style::default().fg(palette.overlay0),
    );

    frame.render_widget(
        Paragraph::new(Span::styled(
            " available",
            modal_section_heading_style(&palette),
        )),
        content_rows[3],
    );

    let list = layout.list;
    let scroll_area = list.scroll_area;
    let first_repo = list.viewport.scroll() / 2;
    let visible_repo_count = (scroll_area.body.height as usize).div_ceil(2);
    let last_repo = first_repo
        .saturating_add(visible_repo_count)
        .min(app.git_repo_picker.roots.len());
    let selected = app
        .git_repo_picker
        .list
        .visible()
        .filter(|idx| *idx < app.git_repo_picker.roots.len());
    let list_width = scroll_area.body.width as usize;
    let mut items = Vec::new();
    let mut selected_row = None;
    for (idx, root) in app.git_repo_picker.roots[first_repo..last_repo]
        .iter()
        .enumerate()
    {
        let repo_idx = first_repo + idx;
        let selected = selected == Some(repo_idx);
        if selected {
            selected_row = Some(items.len());
        }
        let row_style = if selected {
            Style::default().bg(palette.accent)
        } else {
            Style::default()
        };
        let summary = app.git_repo_summaries.get(root).copied();
        let name_text = repo_name(root);
        let status_width = summary.map(status_width).unwrap_or(0);
        let gap = list_width.saturating_sub(1 + name_text.len() + status_width);
        let name_style = if selected {
            Style::default()
                .fg(panel_contrast_fg(&palette))
                .bg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.text)
        };
        let mut name_spans = vec![Span::styled(format!(" {name_text}{:gap$}", ""), name_style)];
        if let Some(summary) = summary {
            name_spans.extend(repo_status_spans(summary, selected, &palette));
        }
        let path_style = if selected {
            Style::default()
                .fg(panel_contrast_fg(&palette))
                .bg(palette.accent)
        } else {
            Style::default().fg(palette.overlay0)
        };
        let path = format!("   {}", display_path(root));
        items.push(ListItem::new(Line::from(name_spans)).style(row_style));
        items.push(
            ListItem::new(Line::from(Span::styled(
                format!("{path:<list_width$}"),
                path_style,
            )))
            .style(row_style),
        );
    }
    let mut list_state = ListState::default().with_selected(selected_row);
    frame.render_stateful_widget(List::new(items), scroll_area.body, &mut list_state);
    if let Some(track) = scroll_area.track {
        render_scrollbar(
            frame,
            list.metrics(),
            track,
            palette.surface_dim,
            palette.overlay0,
            "▐",
        );
    }
}

pub(super) fn render_git_repo_picker_overlay_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
) {
    let area = view.screen_rect();
    super::dim_background(frame, area);

    let picker = &view.git_repo_picker;
    let palette = app.palette_for_workspace(picker.ws_idx);
    let Some(inner) = git_repo_picker_inner_rect(area) else {
        return;
    };
    if inner.height < 12 || inner.width < 28 {
        return;
    }
    let Some(frame_areas) = render_modal_frame(
        frame,
        area,
        &palette,
        ModalFrameSpec {
            title: "git diff",
            width: POPUP_WIDTH,
            height: POPUP_HEIGHT,
            header_rows: HEADER_ROWS,
            footer_hints: GIT_REPO_PICKER_HINTS,
            footer_max_rows: 2,
            reserve_footer_gap: 1,
            show_close: true,
        },
    ) else {
        return;
    };
    if frame_areas.inner != inner {
        return;
    }
    let footer_rows = modal_hint_line_count(inner.width, GIT_REPO_PICKER_HINTS, 2);
    let stack = modal_stack_areas(inner, HEADER_ROWS, footer_rows, 0, 1);
    let Some(content_rows) = git_repo_picker_content_rows(inner) else {
        return;
    };
    let header_rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<3>(stack.header);
    render_modal_divider(frame, header_rows[2], &palette);
    frame.render_widget(
        Paragraph::new(Span::styled(
            " repositories",
            modal_section_heading_style(&palette),
        )),
        content_rows[0],
    );
    let workspace = app
        .workspaces
        .get(picker.ws_idx)
        .map(|workspace| workspace.display_name())
        .unwrap_or_else(|| "workspace".to_string());
    render_modal_description(
        frame,
        content_rows[1],
        &format!("choose which repository to diff for {workspace}"),
        Style::default().fg(palette.overlay0),
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            " available",
            modal_section_heading_style(&palette),
        )),
        content_rows[3],
    );

    let list = ModalListGeometry::new(content_rows[4], picker.roots.len() * 2, picker.scroll * 2);
    let scroll_area = list.scroll_area;
    let first_repo = list.viewport.scroll() / 2;
    let visible_repo_count = (scroll_area.body.height as usize).div_ceil(2);
    let last_repo = first_repo
        .saturating_add(visible_repo_count)
        .min(picker.roots.len());
    let selected = picker
        .list
        .visible()
        .filter(|idx| *idx < picker.roots.len());
    let list_width = scroll_area.body.width as usize;
    let mut items = Vec::new();
    let mut selected_row = None;
    for (idx, root) in picker.roots[first_repo..last_repo].iter().enumerate() {
        let repo_idx = first_repo + idx;
        let selected = selected == Some(repo_idx);
        if selected {
            selected_row = Some(items.len());
        }
        let row_style = if selected {
            Style::default().bg(palette.accent)
        } else {
            Style::default()
        };
        let summary = app.git_repo_summaries.get(root).copied();
        let name_text = repo_name(root);
        let status_width = summary.map(status_width).unwrap_or(0);
        let gap = list_width.saturating_sub(1 + name_text.len() + status_width);
        let name_style = if selected {
            Style::default()
                .fg(panel_contrast_fg(&palette))
                .bg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.text)
        };
        let mut name_spans = vec![Span::styled(format!(" {name_text}{:gap$}", ""), name_style)];
        if let Some(summary) = summary {
            name_spans.extend(repo_status_spans(summary, selected, &palette));
        }
        let path_style = if selected {
            Style::default()
                .fg(panel_contrast_fg(&palette))
                .bg(palette.accent)
        } else {
            Style::default().fg(palette.overlay0)
        };
        let path = format!("   {}", display_path(root));
        items.push(ListItem::new(Line::from(name_spans)).style(row_style));
        items.push(
            ListItem::new(Line::from(Span::styled(
                format!("{path:<list_width$}"),
                path_style,
            )))
            .style(row_style),
        );
    }
    let mut list_state = ListState::default().with_selected(selected_row);
    frame.render_stateful_widget(List::new(items), scroll_area.body, &mut list_state);
    if let Some(track) = scroll_area.track {
        render_scrollbar(
            frame,
            list.metrics(),
            track,
            palette.surface_dim,
            palette.overlay0,
            "▐",
        );
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    use super::*;

    #[test]
    fn git_repo_picker_hit_test_uses_rendered_repo_row() {
        let mut app = AppState::test_new();
        crate::ui::compute_view(&mut app, Rect::new(0, 0, 119, 24));
        app.workspaces = vec![crate::workspace::Workspace::test_new("fake mono")];
        app.git_repo_picker.ws_idx = 0;
        app.git_repo_picker.roots = vec![
            std::path::PathBuf::from("/tmp/fake-mono/api"),
            std::path::PathBuf::from("/tmp/fake-mono/web"),
        ];

        let backend = TestBackend::new(119, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_git_repo_picker_overlay(&app, frame))
            .expect("render git repo picker");
        let buffer = terminal.backend().buffer();
        let (web_x, web_y) = (0..24)
            .find_map(|y| {
                (0..116).find_map(|x| {
                    ["w", "e", "b"]
                        .iter()
                        .enumerate()
                        .all(|(idx, ch)| buffer[(x + idx as u16, y)].symbol() == *ch)
                        .then_some((x, y))
                })
            })
            .expect("rendered repo");

        assert_eq!(git_repo_picker_index_at(&app, web_x, web_y), Some(1));
        assert_eq!(git_repo_picker_index_at(&app, web_x, web_y + 1), Some(1));
    }
}
