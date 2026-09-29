use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::SettingsListRow;

pub(super) fn rows(width: u16) -> Vec<SettingsListRow> {
    let mut rows = Vec::new();
    for text in [
        include_str!("../../assets/acknowledgments.md"),
        "\n# Retained license notices\n\n## ghui — Kit Langton",
        include_str!("../github/LICENSE"),
        "\n## Fly.io Sprites",
        include_str!("../sprites/assets/LICENSE"),
    ] {
        for line in text.lines() {
            if let Some(heading) = line
                .strip_prefix("### ")
                .or_else(|| line.strip_prefix("## "))
                .or_else(|| line.strip_prefix("# "))
            {
                rows.push(SettingsListRow::Header(heading));
            } else if line.is_empty() {
                rows.push(SettingsListRow::Spacer);
            } else {
                push_wrapped_line(&mut rows, line, usize::from(width.max(1)));
            }
        }
    }
    rows
}

fn push_wrapped_line(rows: &mut Vec<SettingsListRow>, mut line: &'static str, width: usize) {
    while !line.is_empty() {
        let mut used = 0;
        let mut last_space = None;
        let mut end = line.len();
        for (offset, grapheme) in line.grapheme_indices(true) {
            if used + grapheme.width() > width {
                end = last_space.filter(|offset| *offset > 0).unwrap_or(offset);
                if end == 0 {
                    end = grapheme.len();
                }
                break;
            }
            used += grapheme.width();
            if grapheme.chars().all(char::is_whitespace) {
                last_space = Some(offset);
            }
        }
        rows.push(SettingsListRow::Caption(line[..end].trim_end().into()));
        line = line[end..].trim_start();
    }
}
