use std::path::Path;
use std::sync::LazyLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};

const MAX_SYNTAX_BYTES: usize = 512 * 1024;
const MAX_SYNTAX_LINE_BYTES: usize = 4 * 1024;

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "comment",
    "constant",
    "constructor",
    "function",
    "keyword",
    "markup",
    "module",
    "number",
    "operator",
    "property",
    "punctuation",
    "string",
    "tag",
    "type",
    "variable",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeDiffSyntaxRole {
    Text,
    Comment,
    Keyword,
    String,
    Number,
    Type,
    Function,
    Property,
    Punctuation,
    Markup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeDiffSyntaxEngine {
    TreeSitter,
    Syntect,
    Plain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDiffHighlightRange {
    pub(crate) line: usize,
    pub(crate) start_col: usize,
    pub(crate) end_col: usize,
    pub(crate) role: NativeDiffSyntaxRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDiffSyntaxDocument {
    pub(crate) engine: NativeDiffSyntaxEngine,
    pub(crate) degraded: bool,
    pub(crate) ranges: Vec<NativeDiffHighlightRange>,
}

impl NativeDiffSyntaxDocument {
    pub(crate) fn plain(degraded: bool) -> Self {
        Self {
            engine: NativeDiffSyntaxEngine::Plain,
            degraded,
            ranges: Vec::new(),
        }
    }

    pub(crate) fn ranges_for_line(
        &self,
        line: usize,
        start_col: usize,
        end_col: usize,
    ) -> impl Iterator<Item = NativeDiffHighlightRange> + '_ {
        let start = self.ranges.partition_point(|range| range.line < line);
        let end = self.ranges.partition_point(|range| range.line <= line);
        self.ranges[start..end].iter().filter_map(move |range| {
            if range.end_col <= start_col || range.start_col >= end_col {
                return None;
            }
            Some(NativeDiffHighlightRange {
                line,
                start_col: range.start_col.max(start_col) - start_col,
                end_col: range.end_col.min(end_col) - start_col,
                role: range.role,
            })
        })
    }
}

pub(crate) fn analyze_source(path: &Path, source: &[u8]) -> NativeDiffSyntaxDocument {
    if source.len() > MAX_SYNTAX_BYTES
        || source
            .split(|byte| *byte == b'\n')
            .any(|line| line.len() > MAX_SYNTAX_LINE_BYTES)
    {
        return NativeDiffSyntaxDocument::plain(true);
    }

    if let Some(language) = LanguageId::for_path(path) {
        if let Some(doc) = analyze_tree_sitter(language, source) {
            return doc;
        }
    }

    analyze_syntect(path, source).unwrap_or_else(|| NativeDiffSyntaxDocument::plain(false))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LanguageId {
    JavaScript,
    Jsx,
    TypeScript,
    Tsx,
    Python,
    Rust,
    Go,
    Java,
    Json,
    Markdown,
    Mdx,
    Yaml,
    Toml,
    Shell,
    Html,
    Css,
}

impl LanguageId {
    fn for_path(path: &Path) -> Option<Self> {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if matches!(file_name, "Dockerfile" | "Brewfile") {
            return Some(Self::Shell);
        }
        let extension = path.extension().and_then(|extension| extension.to_str())?;
        if eq_ext(extension, "js")
            || eq_ext(extension, "mjs")
            || eq_ext(extension, "cjs")
            || eq_ext(extension, "es6")
        {
            Some(Self::JavaScript)
        } else if eq_ext(extension, "jsx") {
            Some(Self::Jsx)
        } else if eq_ext(extension, "ts") || eq_ext(extension, "mts") || eq_ext(extension, "cts") {
            Some(Self::TypeScript)
        } else if eq_ext(extension, "tsx") {
            Some(Self::Tsx)
        } else if eq_ext(extension, "py") || eq_ext(extension, "pyw") {
            Some(Self::Python)
        } else if eq_ext(extension, "rs") {
            Some(Self::Rust)
        } else if eq_ext(extension, "go") {
            Some(Self::Go)
        } else if eq_ext(extension, "java") {
            Some(Self::Java)
        } else if eq_ext(extension, "json") || eq_ext(extension, "jsonc") {
            Some(Self::Json)
        } else if eq_ext(extension, "md")
            || eq_ext(extension, "markdown")
            || eq_ext(extension, "mdown")
            || eq_ext(extension, "mkd")
            || eq_ext(extension, "mkdn")
        {
            Some(Self::Markdown)
        } else if eq_ext(extension, "mdx") {
            Some(Self::Mdx)
        } else if eq_ext(extension, "yml") || eq_ext(extension, "yaml") {
            Some(Self::Yaml)
        } else if eq_ext(extension, "toml") {
            Some(Self::Toml)
        } else if eq_ext(extension, "sh")
            || eq_ext(extension, "bash")
            || eq_ext(extension, "zsh")
            || eq_ext(extension, "fish")
            || eq_ext(extension, "ksh")
            || eq_ext(extension, "bats")
        {
            Some(Self::Shell)
        } else if eq_ext(extension, "html") || eq_ext(extension, "htm") {
            Some(Self::Html)
        } else if eq_ext(extension, "css") {
            Some(Self::Css)
        } else {
            None
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::Jsx => "jsx",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Python => "python",
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Java => "java",
            Self::Json => "json",
            Self::Markdown | Self::Mdx => "markdown",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Shell => "bash",
            Self::Html => "html",
            Self::Css => "css",
        }
    }
}

fn eq_ext(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn analyze_tree_sitter(language: LanguageId, source: &[u8]) -> Option<NativeDiffSyntaxDocument> {
    let config = tree_sitter_config_for(language)?;
    let mut highlighter = Highlighter::new();
    let events = highlighter
        .highlight(config, source, None, |name| {
            tree_sitter_injection_config(name)
        })
        .ok()?;
    let line_starts = line_starts(source);
    let mut stack = Vec::new();
    let mut ranges = Vec::new();
    for event in events {
        match event.ok()? {
            HighlightEvent::HighlightStart(Highlight(index)) => stack.push(index),
            HighlightEvent::HighlightEnd => {
                stack.pop();
            }
            HighlightEvent::Source { start, end } => {
                let role = stack
                    .last()
                    .copied()
                    .map(role_for_highlight_index)
                    .unwrap_or(NativeDiffSyntaxRole::Text);
                if role != NativeDiffSyntaxRole::Text {
                    push_byte_range(&mut ranges, source, &line_starts, start, end, role);
                }
            }
        }
    }
    Some(NativeDiffSyntaxDocument {
        engine: NativeDiffSyntaxEngine::TreeSitter,
        degraded: false,
        ranges,
    })
}

static TREE_SITTER_CONFIGS: LazyLock<Vec<(LanguageId, HighlightConfiguration)>> =
    LazyLock::new(|| {
        [
            LanguageId::JavaScript,
            LanguageId::Jsx,
            LanguageId::TypeScript,
            LanguageId::Tsx,
            LanguageId::Python,
            LanguageId::Rust,
            LanguageId::Go,
            LanguageId::Java,
            LanguageId::Json,
            LanguageId::Markdown,
            LanguageId::Mdx,
            LanguageId::Yaml,
            LanguageId::Toml,
            LanguageId::Shell,
            LanguageId::Html,
            LanguageId::Css,
        ]
        .into_iter()
        .filter_map(|language| {
            let mut config = tree_sitter_config(language)?;
            config.configure(HIGHLIGHT_NAMES);
            Some((language, config))
        })
        .collect()
    });

fn tree_sitter_config_for(language: LanguageId) -> Option<&'static HighlightConfiguration> {
    TREE_SITTER_CONFIGS
        .iter()
        .find_map(|(candidate, config)| (*candidate == language).then_some(config))
}

fn tree_sitter_injection_config(name: &str) -> Option<&'static HighlightConfiguration> {
    TREE_SITTER_CONFIGS
        .iter()
        .find_map(|(language, config)| (language.name() == name).then_some(config))
}

fn tree_sitter_config(language: LanguageId) -> Option<HighlightConfiguration> {
    let result = match language {
        LanguageId::JavaScript => HighlightConfiguration::new(
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        ),
        LanguageId::Jsx => HighlightConfiguration::new(
            tree_sitter_javascript::LANGUAGE.into(),
            "jsx",
            &format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
            ),
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        ),
        LanguageId::TypeScript => HighlightConfiguration::new(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        ),
        LanguageId::Tsx => HighlightConfiguration::new(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            "tsx",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        ),
        LanguageId::Python => HighlightConfiguration::new(
            tree_sitter_python::LANGUAGE.into(),
            "python",
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        LanguageId::Rust => HighlightConfiguration::new(
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        ),
        LanguageId::Go => HighlightConfiguration::new(
            tree_sitter_go::LANGUAGE.into(),
            "go",
            tree_sitter_go::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        LanguageId::Java => HighlightConfiguration::new(
            tree_sitter_java::LANGUAGE.into(),
            "java",
            tree_sitter_java::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        LanguageId::Json => HighlightConfiguration::new(
            tree_sitter_json::LANGUAGE.into(),
            "json",
            tree_sitter_json::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        LanguageId::Markdown | LanguageId::Mdx => HighlightConfiguration::new(
            tree_sitter_md::LANGUAGE.into(),
            "markdown",
            tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
            tree_sitter_md::INJECTION_QUERY_BLOCK,
            "",
        ),
        LanguageId::Yaml => HighlightConfiguration::new(
            tree_sitter_yaml::LANGUAGE.into(),
            "yaml",
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        LanguageId::Toml => HighlightConfiguration::new(
            tree_sitter_toml_ng::LANGUAGE.into(),
            "toml",
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        LanguageId::Shell => HighlightConfiguration::new(
            tree_sitter_bash::LANGUAGE.into(),
            "bash",
            tree_sitter_bash::HIGHLIGHT_QUERY,
            "",
            "",
        ),
        LanguageId::Html => HighlightConfiguration::new(
            tree_sitter_html::LANGUAGE.into(),
            "html",
            tree_sitter_html::HIGHLIGHTS_QUERY,
            tree_sitter_html::INJECTIONS_QUERY,
            "",
        ),
        LanguageId::Css => HighlightConfiguration::new(
            tree_sitter_css::LANGUAGE.into(),
            "css",
            tree_sitter_css::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    };
    result.ok()
}

fn role_for_highlight_index(index: usize) -> NativeDiffSyntaxRole {
    match HIGHLIGHT_NAMES.get(index).copied().unwrap_or_default() {
        "comment" => NativeDiffSyntaxRole::Comment,
        "keyword" | "operator" => NativeDiffSyntaxRole::Keyword,
        "string" => NativeDiffSyntaxRole::String,
        "number" | "boolean" | "constant" => NativeDiffSyntaxRole::Number,
        "type" | "constructor" => NativeDiffSyntaxRole::Type,
        "function" => NativeDiffSyntaxRole::Function,
        "property" | "attribute" | "variable" | "module" => NativeDiffSyntaxRole::Property,
        "punctuation" => NativeDiffSyntaxRole::Punctuation,
        "markup" | "tag" => NativeDiffSyntaxRole::Markup,
        _ => NativeDiffSyntaxRole::Text,
    }
}

fn push_byte_range(
    ranges: &mut Vec<NativeDiffHighlightRange>,
    source: &[u8],
    line_starts: &[usize],
    start: usize,
    end: usize,
    role: NativeDiffSyntaxRole,
) {
    if start >= end || start >= source.len() {
        return;
    }
    let mut cursor = start;
    let end = end.min(source.len());
    while cursor < end {
        let line_index = line_index_for_byte(line_starts, cursor);
        let line_start = line_starts[line_index];
        let line_end = line_starts
            .get(line_index + 1)
            .copied()
            .unwrap_or(source.len())
            .saturating_sub(usize::from(line_starts.get(line_index + 1).is_some()));
        let segment_end = end.min(line_end);
        if cursor < segment_end {
            ranges.push(NativeDiffHighlightRange {
                line: line_index + 1,
                start_col: char_col(&source[line_start..cursor]),
                end_col: char_col(&source[line_start..segment_end]),
                role,
            });
        }
        cursor = if segment_end == cursor {
            cursor + 1
        } else {
            segment_end
        };
    }
}

fn line_starts(source: &[u8]) -> Vec<usize> {
    let mut starts = vec![0];
    for (idx, byte) in source.iter().enumerate() {
        if *byte == b'\n' && idx + 1 < source.len() {
            starts.push(idx + 1);
        }
    }
    starts
}

fn line_index_for_byte(line_starts: &[usize], byte: usize) -> usize {
    line_starts
        .partition_point(|start| *start <= byte)
        .saturating_sub(1)
}

fn char_col(bytes: &[u8]) -> usize {
    String::from_utf8_lossy(bytes).chars().count()
}

fn analyze_syntect(path: &Path, source: &[u8]) -> Option<NativeDiffSyntaxDocument> {
    let syntax = syntect_syntax_for_path(path)?;
    let source_text = std::str::from_utf8(source).ok()?;
    let mut highlighter = HighlightLines::new(syntax, &SYNTAX_THEME);
    let mut ranges = Vec::new();
    for (line_idx, line) in source_text.lines().enumerate() {
        let highlighted = highlighter.highlight_line(line, &SYNTAX_SET).ok()?;
        let mut col = 0;
        for (style, segment) in highlighted {
            let len = segment.chars().count();
            let role = syntect_role(style);
            if role != NativeDiffSyntaxRole::Text && len > 0 {
                ranges.push(NativeDiffHighlightRange {
                    line: line_idx + 1,
                    start_col: col,
                    end_col: col + len,
                    role,
                });
            }
            col += len;
        }
    }
    Some(NativeDiffSyntaxDocument {
        engine: NativeDiffSyntaxEngine::Syntect,
        degraded: false,
        ranges,
    })
}

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
static SYNTAX_THEME: LazyLock<Theme> = LazyLock::new(|| {
    let themes = ThemeSet::load_defaults();
    themes
        .themes
        .get("base16-ocean.dark")
        .cloned()
        .or_else(|| themes.themes.values().next().cloned())
        .unwrap_or_default()
});

fn syntect_syntax_for_path(path: &Path) -> Option<&'static SyntaxReference> {
    SYNTAX_SET
        .find_syntax_for_file(path)
        .ok()
        .flatten()
        .or_else(|| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .and_then(|extension| SYNTAX_SET.find_syntax_by_extension(extension))
        })
        .or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(syntect_syntax_for_special_name)
        })
}

fn syntect_syntax_for_special_name(file_name: &str) -> Option<&'static SyntaxReference> {
    let name = file_name.to_ascii_lowercase();
    if matches!(name.as_str(), "dockerfile" | "brewfile" | "makefile") {
        return SYNTAX_SET.find_syntax_by_name(&name);
    }
    None
}

fn syntect_role(style: SyntectStyle) -> NativeDiffSyntaxRole {
    let fg = style.foreground;
    let max = fg.r.max(fg.g).max(fg.b);
    let min = fg.r.min(fg.g).min(fg.b);
    if max.saturating_sub(min) < 18 {
        return NativeDiffSyntaxRole::Text;
    }
    if fg.r >= fg.g && fg.r >= fg.b {
        if fg.g > fg.b.saturating_add(24) {
            NativeDiffSyntaxRole::String
        } else {
            NativeDiffSyntaxRole::Keyword
        }
    } else if fg.g >= fg.r && fg.g >= fg.b {
        NativeDiffSyntaxRole::Function
    } else if fg.b >= fg.r && fg.b >= fg.g {
        NativeDiffSyntaxRole::Type
    } else {
        NativeDiffSyntaxRole::Text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_sitter_highlights_first_class_frontend_extensions() {
        for path in ["src/main.ts", "src/App.tsx", "src/App.jsx"] {
            let doc = analyze_source(
                Path::new(path),
                b"export function App() { return <main>{count}</main>; }\n",
            );
            assert_eq!(doc.engine, NativeDiffSyntaxEngine::TreeSitter, "{path}");
            assert!(
                doc.ranges
                    .iter()
                    .any(|range| range.role != NativeDiffSyntaxRole::Text),
                "{path}"
            );
        }
    }

    #[test]
    fn syntax_budget_degrades_to_plain_text() {
        let source = vec![b'a'; MAX_SYNTAX_BYTES + 1];
        let doc = analyze_source(Path::new("src/main.rs"), &source);
        assert_eq!(doc.engine, NativeDiffSyntaxEngine::Plain);
        assert!(doc.degraded);
    }
}
