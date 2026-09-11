#[cfg(test)]
mod autoscroll_tests {
    use crate::{
        app::{state::SelectionAutoscrollDirection, App, Mode},
        layout::{PaneId, PaneInfo},
        workspace::Workspace,
    };
    use crossterm::event::MouseEventKind;
    use ratatui::layout::Rect;

    fn make_app_with_pane(inner_rect: Rect) -> (App, PaneId) {
        let mut app = super::super::app_for_mouse_test();
        let ws = Workspace::test_new("test");
        let pane_id = ws.terminal_tab(0).unwrap().root_pane;
        app.state.workspaces.push(ws);
        app.default_client_view.reconcile(&app.state);
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.selected_workspace = 0;
        app.default_client_view.mode = Mode::Terminal;
        app.default_client_view.computed.pane_infos.push(PaneInfo {
            id: pane_id,
            rect: inner_rect,
            inner_rect,
            scrollbar_rect: None,
            is_focused: true,
        });
        (app, pane_id)
    }

    #[test]
    fn above_pane_sets_autoscroll_up() {
        let (mut app, _) = make_app_with_pane(Rect::new(0, 5, 80, 24));

        app.handle_mouse(super::super::mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            5,
            5,
        ));
        app.handle_mouse(super::super::mouse(
            MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            5,
            4,
        ));

        let autoscroll = app
            .default_client_view
            .selection_autoscroll
            .as_ref()
            .unwrap();
        assert_eq!(autoscroll.direction, SelectionAutoscrollDirection::Up);
    }

    #[test]
    fn top_hot_zone_sets_autoscroll_up_on_drag() {
        let (mut app, _) = make_app_with_pane(Rect::new(0, 0, 80, 24));

        app.handle_mouse(super::super::mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            5,
            10,
        ));
        app.handle_mouse(super::super::mouse(
            MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            0,
            0,
        ));

        let autoscroll = app
            .default_client_view
            .selection_autoscroll
            .as_ref()
            .unwrap();
        assert_eq!(autoscroll.direction, SelectionAutoscrollDirection::Up);
    }

    #[test]
    fn top_hot_zone_clears_autoscroll_on_click() {
        let (mut app, _) = make_app_with_pane(Rect::new(0, 0, 80, 24));

        app.handle_mouse(super::super::mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            0,
            0,
        ));
        app.handle_mouse(super::super::mouse(
            MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            0,
            0,
        ));
        assert!(app.default_client_view.selection_autoscroll.is_none());
    }

    #[test]
    fn bottom_hot_zone_sets_autoscroll_down_on_drag() {
        let (mut app, _) = make_app_with_pane(Rect::new(0, 0, 80, 24));

        app.handle_mouse(super::super::mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            0,
            0,
        ));
        app.handle_mouse(super::super::mouse(
            MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            0,
            23,
        ));

        let autoscroll = app
            .default_client_view
            .selection_autoscroll
            .as_ref()
            .unwrap();
        assert_eq!(autoscroll.direction, SelectionAutoscrollDirection::Down);
    }

    #[test]
    fn bottom_hot_zone_clears_autoscroll_on_click() {
        let (mut app, _) = make_app_with_pane(Rect::new(0, 0, 80, 24));

        app.handle_mouse(super::super::mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            0,
            23,
        ));
        app.handle_mouse(super::super::mouse(
            MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            0,
            23,
        ));
        assert!(app.default_client_view.selection_autoscroll.is_none());
    }

    #[test]
    fn below_pane_sets_autoscroll_down_on_drag() {
        let (mut app, _) = make_app_with_pane(Rect::new(0, 0, 80, 24));

        app.handle_mouse(super::super::mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            0,
            0,
        ));
        app.handle_mouse(super::super::mouse(
            MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            0,
            24,
        ));
        let autoscroll = app
            .default_client_view
            .selection_autoscroll
            .as_ref()
            .unwrap();
        assert_eq!(autoscroll.direction, SelectionAutoscrollDirection::Down);
    }

    #[test]
    fn safe_zone_clears_autoscroll() {
        let (mut app, _) = make_app_with_pane(Rect::new(0, 0, 80, 24));

        app.handle_mouse(super::super::mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            5,
            10,
        ));
        app.handle_mouse(super::super::mouse(
            MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            0,
            23,
        ));
        assert!(app.default_client_view.selection_autoscroll.is_some());

        app.handle_mouse(super::super::mouse(
            MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            5,
            12,
        ));
        assert!(app.default_client_view.selection_autoscroll.is_none());
    }
}
