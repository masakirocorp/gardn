use super::{ghostty_line_from_cells, GhosttyPaneCore};

const CACHE_LINES: usize = 2000;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Cache {
    rows: Vec<RenderedLine>,
    last_snapshot: Vec<RenderedLine>,
    pub(super) usable: bool,
    last_scrollbar: Option<(usize, usize)>,
    pub(super) needs_refresh: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderedLine {
    text: String,
    soft_wrapped: bool,
    wrap_continuation: bool,
}

pub(super) fn update(core: &mut GhosttyPaneCore) {
    if !primary_screen_active(core) {
        return;
    }
    let scrollbar = core.terminal.scrollbar().ok();
    let pending_refresh = std::mem::take(&mut core.recent_fallback.needs_refresh);
    let total = scrollbar.map(|metrics| metrics.total).unwrap_or_default();
    let len = scrollbar.map(|metrics| metrics.len).unwrap_or_default();
    let (old_total, old_len) = core.recent_fallback.last_scrollbar.unwrap_or((total, len));
    core.recent_fallback.last_scrollbar = scrollbar.map(|metrics| (metrics.total, metrics.len));
    if scrollbar.is_some_and(|metrics| !at_bottom(metrics)) {
        core.recent_fallback.usable = false;
        return;
    }
    let tracked_row = core.terminal.track_row(len.saturating_sub(1) as u32);
    let history_growth = if pending_refresh {
        tracked_row.map_or(CACHE_LINES, |row| total.saturating_sub(row + 1))
    } else {
        total.saturating_sub(old_total)
    };
    let history_changed = history_growth > 0 || old_total != total || old_len != len;
    let Ok(snapshot) = visible_render_lines(core) else {
        core.recent_fallback.usable = false;
        return;
    };
    if snapshot.is_empty() {
        core.recent_fallback.rows.clear();
        core.recent_fallback.last_snapshot.clear();
        core.recent_fallback.usable = false;
        return;
    }
    if snapshot == core.recent_fallback.last_snapshot && !history_changed {
        core.recent_fallback.usable = true;
        return;
    }

    let mut moved_snapshot = Vec::new();
    if pending_refresh && history_changed {
        let viewport_rows = scrollbar.map_or(1, |metrics| metrics.len.max(1));
        let recovery_rows = history_growth.max(viewport_rows).min(CACHE_LINES);
        for offset in (1..=recovery_rows).rev().step_by(viewport_rows) {
            super::ghostty_set_scroll_offset_from_bottom(&mut core.terminal, offset);
            let Ok(moved) = visible_render_lines(core) else {
                core.terminal.scroll_viewport_bottom();
                core.recent_fallback.usable = false;
                return;
            };
            merge_snapshot(&mut moved_snapshot, &moved);
        }
        core.terminal.scroll_viewport_bottom();
    }
    let cache = &mut core.recent_fallback;
    if pending_refresh && cache.rows.ends_with(&cache.last_snapshot) {
        cache
            .rows
            .truncate(cache.rows.len() - cache.last_snapshot.len());
    }
    merge_snapshot(&mut cache.rows, &moved_snapshot);
    merge_snapshot(&mut cache.rows, &snapshot);
    cache.last_snapshot = snapshot;
    cache.usable = true;
}

pub(super) fn update_after_write(core: &mut GhosttyPaneCore) {
    let scrollbar = core.terminal.scrollbar().ok();
    let old_total = core.recent_fallback.last_scrollbar.map(|old| old.0);
    if primary_screen_active(core)
        && core.terminal.max_scrollback() > 0
        && scrollbar.is_some_and(|metrics| old_total == Some(metrics.total) && at_bottom(metrics))
    {
        core.recent_fallback.needs_refresh = true;
    } else {
        update(core);
    }
}

pub(super) fn refresh_if_needed(core: &mut GhosttyPaneCore) {
    let bottom = core.terminal.scrollbar().map_or(true, at_bottom);
    if core.recent_fallback.needs_refresh && primary_screen_active(core) && bottom {
        update(core);
    }
}

pub(super) fn recent_text(core: &GhosttyPaneCore, lines: usize, unwrap: bool) -> String {
    if !primary_screen_active(core) {
        return String::new();
    }
    if unwrap {
        unwrapped_text(core, lines)
    } else {
        wrapped_text(core, lines)
    }
}

fn primary_screen_active(core: &GhosttyPaneCore) -> bool {
    matches!(
        core.terminal.active_screen(),
        Ok(crate::ghostty::ActiveScreen::Primary)
    )
}

fn wrapped_text(core: &GhosttyPaneCore, lines: usize) -> String {
    if !core.recent_fallback.usable {
        return String::new();
    }
    let rows: Vec<&str> = core
        .recent_fallback
        .rows
        .iter()
        .map(|line| line.text.as_str())
        .collect();
    cache_text(&rows, lines)
}

fn unwrapped_text(core: &GhosttyPaneCore, lines: usize) -> String {
    if !core.recent_fallback.usable {
        return String::new();
    }
    let unwrapped = unwrap_render_lines(&core.recent_fallback.rows);
    let rows: Vec<&str> = unwrapped.iter().map(String::as_str).collect();
    cache_text(&rows, lines)
}

fn at_bottom(scrollbar: crate::ghostty::TerminalScrollbar) -> bool {
    scrollbar.offset.saturating_add(scrollbar.len) >= scrollbar.total
}

fn visible_render_lines(
    core: &mut GhosttyPaneCore,
) -> Result<Vec<RenderedLine>, crate::ghostty::Error> {
    let GhosttyPaneCore {
        terminal,
        render_state,
        ..
    } = core;
    render_state.update(terminal)?;
    let mut row_iterator = crate::ghostty::RowIterator::new()?;
    let mut row_cells = crate::ghostty::RowCells::new()?;
    let mut rows = render_state.populate_row_iterator(&mut row_iterator)?;
    let mut lines = Vec::new();
    while rows.next() {
        let (soft_wrapped, wrap_continuation) = rows.wrap_state().unwrap_or((false, false));
        let mut cells = rows.populate_cells(&mut row_cells)?;
        let text = ghostty_line_from_cells(&mut cells)?;
        if !text.is_empty() {
            lines.push(RenderedLine {
                text,
                soft_wrapped,
                wrap_continuation,
            });
        }
    }
    Ok(lines)
}

fn unwrap_render_lines(snapshot: &[RenderedLine]) -> Vec<String> {
    let mut unwrapped = Vec::new();
    let mut current = String::new();
    for line in snapshot {
        if line.wrap_continuation && current.is_empty() {
            continue;
        }
        current.push_str(&line.text);
        if !line.soft_wrapped {
            unwrapped.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        unwrapped.push(current);
    }
    unwrapped
}

fn merge_snapshot(cache: &mut Vec<RenderedLine>, snapshot: &[RenderedLine]) {
    if snapshot.is_empty() {
        return;
    }

    let max_overlap = cache.len().min(snapshot.len());
    let overlap = (0..=max_overlap)
        .rev()
        .find(|count| *count == 0 || cache[cache.len() - count..] == snapshot[..*count])
        .unwrap_or(0);
    cache.extend(snapshot[overlap..].iter().cloned());
    let overflow = cache.len().saturating_sub(CACHE_LINES);
    if overflow > 0 {
        cache.drain(0..overflow);
    }
}

fn cache_text(cache: &[&str], lines: usize) -> String {
    if lines == 0 || cache.is_empty() {
        return String::new();
    }
    let start = cache.len().saturating_sub(lines);
    let text = cache[start..].join("\n");
    if text.is_empty() {
        text
    } else {
        format!("{text}\n")
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;
    use crate::layout::PaneId;
    use crate::pane::terminal::GhosttyPaneTerminal;

    fn rendered_line(
        text: impl Into<String>,
        soft_wrapped: bool,
        wrap_continuation: bool,
    ) -> RenderedLine {
        RenderedLine {
            text: text.into(),
            soft_wrapped,
            wrap_continuation,
        }
    }

    #[test]
    fn merges_overlapping_snapshots() {
        let mut cache = Vec::new();

        merge_snapshot(
            &mut cache,
            &[
                rendered_line("one", false, false),
                rendered_line("two", false, false),
            ],
        );
        merge_snapshot(
            &mut cache,
            &[
                rendered_line("two", false, false),
                rendered_line("three", false, false),
                rendered_line("four", false, false),
            ],
        );
        merge_snapshot(
            &mut cache,
            &[
                rendered_line("three", false, false),
                rendered_line("four", false, false),
                rendered_line("five", false, false),
            ],
        );

        let text: Vec<&str> = cache.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(text, vec!["one", "two", "three", "four", "five"]);
    }

    #[test]
    fn unwraps_soft_wrapped_rows() {
        let snapshot = vec![
            rendered_line("ABCDE", true, false),
            rendered_line("FGHIJ", false, true),
            rendered_line("next", false, false),
        ];

        assert_eq!(unwrap_render_lines(&snapshot), vec!["ABCDEFGHIJ", "next"]);
    }

    #[test]
    fn suppresses_leading_wrap_continuation() {
        let snapshot = vec![
            rendered_line("suffix", false, true),
            rendered_line("next", false, false),
        ];

        assert_eq!(unwrap_render_lines(&snapshot), vec!["next"]);
    }

    #[test]
    fn clears_on_blank_snapshot() {
        let (tx, _rx) = mpsc::channel(4);
        let terminal = crate::ghostty::Terminal::new(40, 3, 1024).expect("terminal");
        let pane = GhosttyPaneTerminal::new(terminal, tx.clone()).expect("pane");
        let pane_id = PaneId::from_raw(1);

        pane.process_pty_bytes(pane_id, 0, b"old\r\n", &tx);
        assert!(pane.recent_text(3).contains("old"));

        pane.process_pty_bytes(pane_id, 0, b"\x1b[2J\x1b[H", &tx);
        assert_eq!(pane.recent_text(3).trim(), "");
    }

    #[test]
    fn deferred_refresh_replaces_visible_tail_after_repaint() {
        let (tx, _rx) = mpsc::channel(4);
        let terminal = crate::ghostty::Terminal::new(40, 3, 1024).unwrap();
        let pane = GhosttyPaneTerminal::new(terminal, tx.clone()).unwrap();
        let pane_id = PaneId::from_raw(1);
        pane.process_pty_bytes(pane_id, 0, b"older", &tx);
        pane.process_pty_bytes(pane_id, 0, b"\r\nold\r\nprompt", &tx);
        let mut core = pane.core.lock().unwrap();
        let _ = super::super::finish_recent_snapshot(&mut core, String::new(), 3, false);
        drop(core);
        pane.resize(2, 40, 8, 16);
        pane.process_pty_bytes(pane_id, 0, b"\rupdated", &tx);
        let mut core = pane.core.lock().unwrap();
        let resized = super::super::finish_recent_snapshot(&mut core, String::new(), 10, false);
        assert_eq!(resized.text, "older\nold\nupdated\n");
        drop(core);
        pane.process_pty_bytes(pane_id, 0, b"\x1b[2J\x1b[H", &tx);
        pane.process_pty_bytes(pane_id, 0, b"updated", &tx);
        let mut core = pane.core.lock().unwrap();
        let refreshed = super::super::finish_recent_snapshot(&mut core, String::new(), 10, false);
        assert_eq!(refreshed.text, "older\nupdated\n");
    }

    #[test]
    fn ignores_alternate_screen_snapshots() {
        let (tx, _rx) = mpsc::channel(4);
        let terminal = crate::ghostty::Terminal::new(40, 3, 1024).expect("terminal");
        let pane = GhosttyPaneTerminal::new(terminal, tx.clone()).expect("pane");
        let pane_id = PaneId::from_raw(1);

        for line in 0..20 {
            pane.process_pty_bytes(pane_id, 0, format!("{line:06}\r\n").as_bytes(), &tx);
        }
        {
            let Ok(core) = pane.core.lock() else {
                panic!("core lock poisoned");
            };
            assert!(core
                .recent_fallback
                .rows
                .iter()
                .any(|line| line.text.contains("000000")));
        }

        pane.process_pty_bytes(pane_id, 0, b"\x1b[?1049h\x1b[2J\x1b[H", &tx);

        let Ok(core) = pane.core.lock() else {
            panic!("core lock poisoned");
        };
        assert!(core
            .recent_fallback
            .rows
            .iter()
            .any(|line| line.text.contains("000000")));
    }

    #[test]
    fn invalidates_fallback_when_output_arrives_while_scrolled_up() {
        let (tx, _rx) = mpsc::channel(4);
        let terminal = crate::ghostty::Terminal::new(40, 3, 1024).expect("terminal");
        let pane = GhosttyPaneTerminal::new(terminal, tx.clone()).expect("pane");
        let pane_id = PaneId::from_raw(1);

        for line in 0..20 {
            pane.process_pty_bytes(pane_id, 0, format!("{line:06}\r\n").as_bytes(), &tx);
        }
        pane.process_pty_bytes(pane_id, 0, b"\rredraw", &tx);
        let before = pane.scroll_metrics().expect("scroll metrics before scroll");
        pane.set_scroll_offset_from_bottom(before.max_offset_from_bottom);
        {
            let core = pane.core.lock().unwrap();
            assert!(!core.recent_fallback.needs_refresh);
            assert!(recent_text(&core, 3, false).contains("redraw"));
        }
        pane.resize(4, 40, 8, 16);
        pane.scroll_reset();
        assert!(recent_text(&pane.core.lock().unwrap(), 3, false).contains("redraw"));
        pane.set_scroll_offset_from_bottom(before.max_offset_from_bottom);
        pane.process_pty_bytes(pane_id, 0, b"new output\r\n", &tx);
        pane.resize(4, 40, 8, 16);

        let Ok(core) = pane.core.lock() else {
            panic!("core lock poisoned");
        };
        assert!(!core.recent_fallback.needs_refresh);
        assert_eq!(recent_text(&core, 3, false), "");
        assert_eq!(recent_text(&core, 3, true), "");
    }

    #[test]
    fn zero_scrollback_keeps_eager_snapshots_across_writes() {
        let (tx, _rx) = mpsc::channel(4);
        let terminal = crate::ghostty::Terminal::new(40, 2, 0).unwrap();
        let pane = GhosttyPaneTerminal::new(terminal, tx.clone()).unwrap();
        let pane_id = PaneId::from_raw(1);
        pane.process_pty_bytes(pane_id, 0, b"one\r\n", &tx);
        let total = pane.core.lock().unwrap().recent_fallback.last_scrollbar;
        pane.process_pty_bytes(pane_id, 0, b"two\r\n", &tx);
        pane.process_pty_bytes(pane_id, 0, b"three\r\n", &tx);

        let core = pane.core.lock().unwrap();
        assert_eq!(core.recent_fallback.last_scrollbar, total);
        assert!(!core.recent_fallback.needs_refresh);
        assert_eq!(recent_text(&core, 10, false), "one\ntwo\nthree\n");
    }

    #[test]
    fn seed_history_updates_fallback() {
        let (tx, _rx) = mpsc::channel(4);
        let terminal = crate::ghostty::Terminal::new(5, 2, 1024).expect("terminal");
        let pane = GhosttyPaneTerminal::new(terminal, tx).expect("pane");

        pane.seed_history_ansi("abcdefghij\r\nend");
        pane.resize(3, 10, 8, 16);

        let Ok(core) = pane.core.lock() else {
            panic!("core lock poisoned");
        };
        assert_eq!(recent_text(&core, 10, false), "abcdefghij\nend\n");
        assert_eq!(recent_text(&core, 10, true), "abcdefghij\nend\n");
    }

    #[test]
    fn recent_ansi_unwrapped_uses_unwrapped_render_cache() {
        let (tx, _rx) = mpsc::channel(4);
        let terminal = crate::ghostty::Terminal::new(40, 3, 1024).expect("terminal");
        let pane = GhosttyPaneTerminal::new(terminal, tx).expect("pane");

        {
            let Ok(mut core) = pane.core.lock() else {
                panic!("core lock poisoned");
            };
            core.recent_fallback.rows = vec![
                rendered_line("wrapped", true, false),
                rendered_line("rows", false, true),
            ];
            core.recent_fallback.usable = true;
        }

        assert_eq!(pane.recent_ansi(2), "wrapped\nrows\n");
        assert_eq!(pane.recent_unwrapped_ansi(2), "wrappedrows\n");
    }
}
