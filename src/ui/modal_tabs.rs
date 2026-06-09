use ratatui::layout::Rect;

pub(crate) fn visible_tab_range<F>(
    len: usize,
    selected: usize,
    row_width: u16,
    mut tab_width: F,
) -> (usize, usize)
where
    F: FnMut(usize) -> u16,
{
    if len == 0 {
        return (0, 0);
    }
    let selected = selected.min(len - 1);
    let mut start = selected;
    let mut end = selected + 1;

    loop {
        let mut expanded = false;
        if start > 0 && tabs_width(len, start - 1, end, &mut tab_width) <= row_width {
            start -= 1;
            expanded = true;
        }
        if end < len && tabs_width(len, start, end + 1, &mut tab_width) <= row_width {
            end += 1;
            expanded = true;
        }
        if !expanded {
            break;
        }
    }

    (start, end)
}

pub(crate) fn tab_hit_areas<F>(
    row: Rect,
    start: usize,
    end: usize,
    mut tab_width: F,
) -> Vec<(usize, Rect)>
where
    F: FnMut(usize) -> u16,
{
    let mut x = row.x;
    if start > 0 {
        x = x.saturating_add(2);
    }

    let mut areas = Vec::new();
    for (visible_idx, tab_idx) in (start..end).enumerate() {
        if visible_idx > 0 {
            x = x.saturating_add(1);
        }
        let width = tab_width(tab_idx);
        areas.push((tab_idx, Rect::new(x, row.y, width, 1)));
        x = x.saturating_add(width);
    }
    areas
}

pub(crate) fn chevron_tab_at<F>(
    len: usize,
    row: Rect,
    col: u16,
    start: usize,
    end: usize,
    tab_width: F,
) -> Option<usize>
where
    F: FnMut(usize) -> u16,
{
    if start > 0 && col >= row.x && col < row.x.saturating_add(2) {
        return Some(start - 1);
    }

    if end < len {
        let right_x = tab_hit_areas(row, start, end, tab_width)
            .last()
            .map(|(_, rect)| rect.x.saturating_add(rect.width))
            .unwrap_or(row.x);
        if col >= right_x && col < right_x.saturating_add(2) {
            return Some(end);
        }
    }

    None
}

fn tabs_width<F>(len: usize, start: usize, end: usize, tab_width: &mut F) -> u16
where
    F: FnMut(usize) -> u16,
{
    if start >= end {
        return 0;
    }

    let tab_widths = (start..end).map(tab_width).sum::<u16>();
    let gaps = end.saturating_sub(start + 1) as u16;
    let edge_hints = u16::from(start > 0) * 2 + u16::from(end < len) * 2;
    tab_widths.saturating_add(gaps).saturating_add(edge_hints)
}
