const ACTIVITY_GLYPHS: &str = "·✢✳✶✻✽◐◓◑◒";

pub(crate) fn stripped_terminal_title(title: &str) -> Option<String> {
    let title = title
        .strip_prefix("Administrator: ")
        .unwrap_or(title)
        .trim();
    let mut chars = title.char_indices();
    let (_, first) = chars.next()?;
    let after_first = &title[first.len_utf8()..];
    let recognized = matches!(first, '\u{2800}'..='\u{28ff}') || ACTIVITY_GLYPHS.contains(first);
    let stripped = if recognized
        && (after_first.is_empty() || after_first.chars().next().is_some_and(char::is_whitespace))
    {
        after_first.trim()
    } else {
        title
    };
    (!stripped.is_empty()).then(|| stripped.to_string())
}

#[cfg(test)]
mod tests {
    use super::stripped_terminal_title;

    #[test]
    fn strips_one_recognized_leading_activity_glyph() {
        for title in [
            "⠋ task",
            "✳ task",
            "  ⠙   task  ",
            "✢ task",
            "✻ task",
            "◐ task",
            "◓ task",
            "◑ task",
            "◒ task",
        ] {
            assert_eq!(stripped_terminal_title(title).as_deref(), Some("task"));
        }
        assert_eq!(stripped_terminal_title("task").as_deref(), Some("task"));
        assert_eq!(stripped_terminal_title("◐task").as_deref(), Some("◐task"));
    }

    #[test]
    fn strips_windows_administrator_prefix() {
        assert_eq!(
            stripped_terminal_title("Administrator: C:\\Windows\\system32\\cmd.exe"),
            Some("C:\\Windows\\system32\\cmd.exe".to_string())
        );
    }

    #[test]
    fn leaves_non_admin_titles_alone() {
        assert_eq!(
            stripped_terminal_title("user@host: ~/project"),
            Some("user@host: ~/project".to_string())
        );
        assert_eq!(
            stripped_terminal_title("Administrators only"),
            Some("Administrators only".to_string())
        );
    }

    #[test]
    fn admin_prefix_combines_with_activity_glyph_stripping() {
        assert_eq!(
            stripped_terminal_title("Administrator: \u{2733} build"),
            Some("build".to_string())
        );
    }
}
