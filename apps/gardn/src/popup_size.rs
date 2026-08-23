use ratatui::layout::Rect;

pub(crate) use crate::api::schema::PopupSize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PopupResolvedGeometry {
    pub outer: Rect,
    pub inner: Rect,
}

pub(crate) fn resolve_popup_geometry(
    width: Option<PopupSize>,
    height: Option<PopupSize>,
    area: Rect,
) -> Option<PopupResolvedGeometry> {
    let default_width = area.width.saturating_div(2).max(6);
    let default_height = area.height.saturating_div(2).max(4);
    let outer_width = width
        .map(|width| width.resolve(area.width))
        .unwrap_or(default_width)
        .max(6)
        .min(area.width);
    let outer_height = height
        .map(|height| height.resolve(area.height))
        .unwrap_or(default_height)
        .max(4)
        .min(area.height);
    if outer_width < 6 || outer_height < 4 {
        return None;
    }

    let outer_x = area.x + (area.width.saturating_sub(outer_width)) / 2;
    let outer_y = area.y + (area.height.saturating_sub(outer_height)) / 2;
    let pane_inner_width = outer_width.saturating_sub(2);
    let pane_inner_height = outer_height.saturating_sub(2);
    let terminal_cols = if pane_inner_width <= 4 {
        pane_inner_width
    } else {
        pane_inner_width.saturating_sub(1)
    };
    let inner = Rect::new(
        outer_x.saturating_add(1),
        outer_y.saturating_add(1),
        terminal_cols,
        pane_inner_height,
    );
    Some(PopupResolvedGeometry {
        outer: Rect::new(outer_x, outer_y, outer_width, outer_height),
        inner,
    })
}

#[cfg(test)]
mod tests {
    use super::PopupSize;

    #[test]
    fn parses_cells_and_percent() {
        assert_eq!(PopupSize::parse_cli("120"), Ok(PopupSize::Cells(120)));
        assert_eq!(PopupSize::parse_cli("80%"), Ok(PopupSize::Percent(80)));
        assert_eq!(PopupSize::Percent(80).resolve(100), 80);
    }

    #[test]
    fn rejects_invalid_percent() {
        assert!(PopupSize::parse_cli("0%").is_err());
        assert!(PopupSize::parse_cli("101%").is_err());
        assert!(PopupSize::parse_cli("%").is_err());
    }

    #[test]
    fn string_deserialization_requires_percent() {
        assert!(serde_json::from_value::<PopupSize>(serde_json::json!("120")).is_err());
        assert_eq!(
            serde_json::from_value::<PopupSize>(serde_json::json!("80%")).unwrap(),
            PopupSize::Percent(80)
        );
    }

    #[test]
    fn resolves_requested_outer_size_and_inner_terminal_area() {
        let resolved = super::resolve_popup_geometry(
            Some(PopupSize::Percent(80)),
            Some(PopupSize::Percent(40)),
            ratatui::layout::Rect::new(0, 0, 100, 30),
        )
        .unwrap();
        assert_eq!(resolved.outer, ratatui::layout::Rect::new(10, 9, 80, 12));
        assert_eq!(resolved.inner, ratatui::layout::Rect::new(11, 10, 77, 10));
    }
}
