use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use patchkit::unified::{HunkLine, PlainOrBinaryPatch, UnifiedPatch};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DiffBucket {
    Changed,
    Untracked,
    Staged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeDiffViewMode {
    Unified,
    Split,
    Auto,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeDiffScope {
    All,
    Unstaged,
    Untracked,
    Staged,
}

impl NativeDiffScope {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Unstaged => "unstaged",
            Self::Untracked => "untracked",
            Self::Staged => "staged",
        }
    }

    pub(crate) fn includes(self, bucket: DiffBucket) -> bool {
        match self {
            Self::All => true,
            Self::Unstaged => bucket == DiffBucket::Changed,
            Self::Untracked => bucket == DiffBucket::Untracked,
            Self::Staged => bucket == DiffBucket::Staged,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::All => Self::Unstaged,
            Self::Unstaged => Self::Untracked,
            Self::Untracked => Self::Staged,
            Self::Staged => Self::All,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NativeDiffContextKey {
    pub(crate) file_index: usize,
    pub(crate) hunk_index: usize,
    pub(crate) run_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDiffSession {
    pub(crate) repo_root: PathBuf,
    pub(crate) files: Vec<NativeDiffFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDiffFile {
    pub(crate) bucket: DiffBucket,
    pub(crate) old_path: Option<PathBuf>,
    pub(crate) new_path: Option<PathBuf>,
    pub(crate) status: DiffFileStatus,
    pub(crate) added: usize,
    pub(crate) deleted: usize,
    pub(crate) hunks: Vec<NativeDiffHunk>,
    pub(crate) binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NativeDiffSyntaxKey {
    bucket: DiffBucket,
    path: PathBuf,
    old_side: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NativeDiffSyntaxCache {
    entries: BTreeMap<NativeDiffSyntaxKey, crate::native_diff_syntax::NativeDiffSyntaxDocument>,
}

impl NativeDiffSyntaxCache {
    pub(crate) fn get(
        &self,
        file: &NativeDiffFile,
        old_side: bool,
    ) -> Option<&crate::native_diff_syntax::NativeDiffSyntaxDocument> {
        let path = if old_side {
            file.old_path.as_ref()?
        } else {
            file.new_path.as_ref()?
        };
        self.entries.get(&NativeDiffSyntaxKey {
            bucket: file.bucket,
            path: path.clone(),
            old_side,
        })
    }

    pub(crate) fn insert(
        &mut self,
        file: &NativeDiffFile,
        old_side: bool,
        document: crate::native_diff_syntax::NativeDiffSyntaxDocument,
    ) {
        let path = if old_side {
            file.old_path.as_ref()
        } else {
            file.new_path.as_ref()
        };
        if let Some(path) = path {
            self.entries.insert(
                NativeDiffSyntaxKey {
                    bucket: file.bucket,
                    path: path.clone(),
                    old_side,
                },
                document,
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDiffHunk {
    pub(crate) old_start: usize,
    pub(crate) old_count: usize,
    pub(crate) new_start: usize,
    pub(crate) new_count: usize,
    pub(crate) lines: Vec<NativeDiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDiffLine {
    pub(crate) kind: DiffLineKind,
    pub(crate) old_line: Option<usize>,
    pub(crate) new_line: Option<usize>,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeDiffRowId {
    FileHeader {
        bucket: DiffBucket,
        file_index: usize,
    },
    Binary {
        bucket: DiffBucket,
        file_index: usize,
    },
    Hunk {
        bucket: DiffBucket,
        file_index: usize,
        hunk_index: usize,
    },
    Line {
        bucket: DiffBucket,
        file_index: usize,
        old_line: Option<usize>,
        new_line: Option<usize>,
        kind: DiffLineKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeDiffRowKind {
    FileHeader,
    Binary,
    Hunk,
    Line(DiffLineKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeDiffRow {
    pub(crate) id: NativeDiffRowId,
    pub(crate) kind: NativeDiffRowKind,
    pub(crate) old_line: Option<usize>,
    pub(crate) new_line: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeDiffSelection {
    pub(crate) bucket: DiffBucket,
    pub(crate) file_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDiffPaneState {
    pub(crate) session: NativeDiffSession,
    pub(crate) syntax: NativeDiffSyntaxCache,
    pub(crate) selected_file: Option<NativeDiffSelection>,
    pub(crate) selected_hunk: Option<usize>,
    pub(crate) file_scroll: usize,
    pub(crate) diff_scroll: usize,
    pub(crate) diff_col_scroll: usize,
    pub(crate) show_file_list: bool,
    pub(crate) wrap_lines: bool,
    pub(crate) word_diff: bool,
    pub(crate) view_mode: NativeDiffViewMode,
    pub(crate) scope: NativeDiffScope,
    pub(crate) expanded_context: BTreeSet<NativeDiffContextKey>,
    pub(crate) last_error: Option<String>,
}

impl NativeDiffPaneState {
    pub(crate) fn new(session: NativeDiffSession) -> Self {
        Self::new_with_syntax(session, NativeDiffSyntaxCache::default())
    }

    pub(crate) fn with_syntax(session: NativeDiffSession) -> Self {
        let syntax = load_syntax_for_session(&session);
        Self::new_with_syntax(session, syntax)
    }

    fn new_with_syntax(session: NativeDiffSession, syntax: NativeDiffSyntaxCache) -> Self {
        let selected_file = first_selection_for_scope(&session, NativeDiffScope::All);
        let show_file_list = session.files.len() != 1;
        Self {
            syntax,
            session,
            selected_file,
            selected_hunk: None,
            file_scroll: 0,
            diff_scroll: 0,
            diff_col_scroll: 0,
            show_file_list,
            wrap_lines: false,
            word_diff: false,
            view_mode: NativeDiffViewMode::Auto,
            scope: NativeDiffScope::All,
            expanded_context: BTreeSet::new(),
            last_error: None,
        }
    }

    pub(crate) fn selected_file(&self) -> Option<&NativeDiffFile> {
        let selection = self.selected_file?;
        self.session
            .files
            .get(selection.file_index)
            .filter(|file| file.bucket == selection.bucket)
    }
    pub(crate) fn selected_path(&self) -> Option<PathBuf> {
        let file = self.selected_file()?;
        file.new_path.as_ref().or(file.old_path.as_ref()).cloned()
    }

    fn selected_key(&self) -> Option<(DiffBucket, PathBuf)> {
        let selection = self.selected_file?;
        Some((selection.bucket, self.selected_path()?))
    }

    fn visible_file_indices(&self) -> Vec<(usize, &NativeDiffFile)> {
        self.session
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| self.scope.includes(file.bucket))
            .collect()
    }

    pub(crate) fn replace_session(
        &mut self,
        session: NativeDiffSession,
        syntax: NativeDiffSyntaxCache,
    ) {
        let previous_key = self.selected_key();
        let previous_top_row = self
            .visible_diff_rows()
            .get(self.diff_scroll)
            .map(|row| row.id);
        self.session = session;
        self.syntax = syntax;
        self.selected_file = previous_key
            .and_then(|(bucket, path)| {
                self.session
                    .files
                    .iter()
                    .enumerate()
                    .find_map(|(index, file)| {
                        if file.bucket != bucket {
                            return None;
                        }
                        let candidate = file.new_path.as_ref().or(file.old_path.as_ref())?;
                        (candidate == &path).then_some(NativeDiffSelection {
                            bucket: file.bucket,
                            file_index: index,
                        })
                    })
            })
            .or_else(|| first_selection(&self.session));
        self.file_scroll = self
            .file_scroll
            .min(self.file_list_row_count().saturating_sub(1));
        let rows = self.visible_diff_rows();
        self.diff_scroll = previous_top_row
            .and_then(|id| rows.iter().position(|row| row.id == id))
            .unwrap_or_else(|| self.diff_scroll.min(rows.len().saturating_sub(1)));
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        let visible = self.visible_file_indices();
        if visible.is_empty() {
            self.selected_file = None;
            return;
        }
        let current = self
            .selected_file
            .and_then(|selection| {
                visible.iter().position(|(index, file)| {
                    *index == selection.file_index && file.bucket == selection.bucket
                })
            })
            .unwrap_or(0);
        let next = current
            .saturating_add_signed(delta)
            .min(visible.len().saturating_sub(1));
        let (file_index, file) = visible[next];
        self.selected_file = Some(NativeDiffSelection {
            bucket: file.bucket,
            file_index,
        });
        self.diff_col_scroll = 0;
    }

    pub(crate) fn cycle_scope(&mut self) {
        self.scope = self.scope.next();
        if self
            .selected_file
            .is_none_or(|selection| !self.scope.includes(selection.bucket))
        {
            self.selected_file = first_selection_for_scope(&self.session, self.scope);
        }
        self.file_scroll = 0;
        self.diff_scroll = 0;
        self.diff_col_scroll = 0;
    }

    pub(crate) fn move_hunk_selection(&mut self, delta: isize) {
        let Some(file) = self.selected_file() else {
            self.selected_hunk = None;
            return;
        };
        if file.hunks.is_empty() {
            self.selected_hunk = None;
            return;
        }
        let current = self.selected_hunk.unwrap_or(0);
        let next = current
            .saturating_add_signed(delta)
            .min(file.hunks.len().saturating_sub(1));
        self.selected_hunk = Some(next);
        self.scroll_to_hunk(next);
    }

    fn scroll_to_hunk(&mut self, hunk_index: usize) {
        let Some(selection) = self.selected_file else {
            return;
        };
        let target = NativeDiffRowId::Hunk {
            bucket: selection.bucket,
            file_index: selection.file_index,
            hunk_index,
        };
        if let Some(row_index) = self
            .visible_diff_rows()
            .iter()
            .position(|row| row.id == target)
        {
            self.diff_scroll = row_index.saturating_sub(1);
        }
    }
    pub(crate) fn toggle_word_diff(&mut self) {
        self.word_diff = !self.word_diff;
    }

    pub(crate) fn refresh(&mut self) {
        match load_native_diff_session_metadata(self.session.repo_root.clone()) {
            Ok(session) => {
                if session != self.session {
                    let syntax = load_syntax_for_session(&session);
                    self.replace_session(session, syntax);
                }
                self.last_error = None;
            }
            Err(err) => self.last_error = Some(err.0),
        }
    }

    pub(crate) fn stage_selected_file(&mut self) {
        self.apply_git_to_selected(&["add", "--"]);
    }

    pub(crate) fn unstage_selected_file(&mut self) {
        self.apply_git_to_selected(&["restore", "--staged", "--"]);
    }

    pub(crate) fn stage_selected_hunk(&mut self) {
        self.apply_selected_hunk(false);
    }

    pub(crate) fn unstage_selected_hunk(&mut self) {
        self.apply_selected_hunk(true);
    }

    pub(crate) fn toggle_file_list(&mut self) {
        self.show_file_list = !self.show_file_list;
    }

    pub(crate) fn toggle_wrap_lines(&mut self) {
        self.wrap_lines = !self.wrap_lines;
        if self.wrap_lines {
            self.diff_col_scroll = 0;
        }
    }

    pub(crate) fn context_expanded(&self, key: NativeDiffContextKey) -> bool {
        self.expanded_context.contains(&key)
    }

    pub(crate) fn toggle_context(&mut self, key: NativeDiffContextKey) {
        if !self.expanded_context.remove(&key) {
            self.expanded_context.insert(key);
        }
    }

    pub(crate) fn cycle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            NativeDiffViewMode::Unified => NativeDiffViewMode::Split,
            NativeDiffViewMode::Split => NativeDiffViewMode::Auto,
            NativeDiffViewMode::Auto => NativeDiffViewMode::Unified,
        };
        self.diff_col_scroll = 0;
    }

    fn apply_selected_hunk(&mut self, reverse: bool) {
        let Some(file) = self.selected_file() else {
            return;
        };
        let hunk_index = self.selected_hunk.unwrap_or(0);
        let Some(hunk) = file.hunks.get(hunk_index) else {
            return;
        };
        let patch = single_hunk_patch(file, hunk);
        let result = if reverse {
            git_apply(&self.session.repo_root, &patch, &["--cached", "--reverse"])
        } else {
            git_apply(&self.session.repo_root, &patch, &["--cached"])
        };
        match result {
            Ok(()) => self.refresh(),
            Err(err) => self.last_error = Some(err),
        }
    }

    fn apply_git_to_selected(&mut self, args: &[&str]) {
        let Some(path) = self.selected_path() else {
            return;
        };
        match run_git_path(&self.session.repo_root, args, &path) {
            Ok(()) => self.refresh(),
            Err(err) => self.last_error = Some(err),
        }
    }
    pub(crate) fn selected_agent_payload(&self) -> Option<String> {
        let file = self.selected_file()?;
        let path = file
            .new_path
            .as_ref()
            .or(file.old_path.as_ref())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "(unknown)".to_string());
        let mut payload = String::new();
        payload.push_str("Review this diff from Hako.\n\n");
        payload.push_str(&format!("Repo: {}\n", self.session.repo_root.display()));
        payload.push_str(&format!("File: {path}\n"));
        if let Some(hunk_index) = self.selected_hunk {
            if let Some(hunk) = file.hunks.get(hunk_index) {
                payload.push_str(&format!(
                    "Hunk: -{},{} +{},{}\n",
                    hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
                ));
                payload.push('\n');
                push_agent_hunk_patch(&mut payload, file, hunk);
                return Some(payload);
            }
        }
        payload.push('\n');
        push_agent_file_patch(&mut payload, file);
        Some(payload)
    }

    pub(crate) fn file_list_row_count(&self) -> usize {
        let mut rows = 0;
        for bucket in [
            DiffBucket::Changed,
            DiffBucket::Untracked,
            DiffBucket::Staged,
        ] {
            if !self.scope.includes(bucket) {
                continue;
            }
            let count = self
                .session
                .files
                .iter()
                .filter(|file| file.bucket == bucket)
                .count();
            if count > 0 {
                rows += 1 + count + 1;
            }
        }
        rows.saturating_sub(1)
    }

    pub(crate) fn visible_diff_rows(&self) -> Vec<NativeDiffRow> {
        let Some(selection) = self.selected_file else {
            return Vec::new();
        };
        let Some(file) = self.selected_file() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        rows.push(NativeDiffRow {
            id: NativeDiffRowId::FileHeader {
                bucket: selection.bucket,
                file_index: selection.file_index,
            },
            kind: NativeDiffRowKind::FileHeader,
            old_line: None,
            new_line: None,
        });
        if file.binary {
            rows.push(NativeDiffRow {
                id: NativeDiffRowId::Binary {
                    bucket: selection.bucket,
                    file_index: selection.file_index,
                },
                kind: NativeDiffRowKind::Binary,
                old_line: None,
                new_line: None,
            });
        }
        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            rows.push(NativeDiffRow {
                id: NativeDiffRowId::Hunk {
                    bucket: selection.bucket,
                    file_index: selection.file_index,
                    hunk_index,
                },
                kind: NativeDiffRowKind::Hunk,
                old_line: None,
                new_line: None,
            });
            for line in &hunk.lines {
                rows.push(NativeDiffRow {
                    id: NativeDiffRowId::Line {
                        bucket: selection.bucket,
                        file_index: selection.file_index,
                        old_line: line.old_line,
                        new_line: line.new_line,
                        kind: line.kind,
                    },
                    kind: NativeDiffRowKind::Line(line.kind),
                    old_line: line.old_line,
                    new_line: line.new_line,
                });
            }
        }
        rows
    }

    pub(crate) fn toggle_visible_context_row(&mut self, visible_row: usize) -> bool {
        const CONTEXT_EDGE: usize = 3;
        const MIN_FOLD: usize = CONTEXT_EDGE * 2 + 4;

        if visible_row == 0 {
            return false;
        }
        let Some(selection) = self.selected_file else {
            return false;
        };
        let Some(file) = self.selected_file() else {
            return false;
        };
        let target_row = self
            .diff_scroll
            .saturating_add(visible_row.saturating_sub(1));
        let mut row = 0;
        let mut matched = None;
        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            if row == target_row {
                return false;
            }
            row += 1;
            let mut index = 0;
            let mut run_index = 0;
            while index < hunk.lines.len() {
                if hunk.lines[index].kind != DiffLineKind::Context {
                    row += 1;
                    index += 1;
                    continue;
                }
                let start = index;
                while index < hunk.lines.len() && hunk.lines[index].kind == DiffLineKind::Context {
                    index += 1;
                }
                let count = index - start;
                let key = NativeDiffContextKey {
                    file_index: selection.file_index,
                    hunk_index,
                    run_index,
                };
                run_index += 1;
                if count >= MIN_FOLD && self.context_expanded(key) {
                    if row == target_row {
                        matched = Some(key);
                        break;
                    }
                    row += 1 + count;
                } else if count >= MIN_FOLD {
                    row += CONTEXT_EDGE;
                    if row == target_row {
                        matched = Some(key);
                        break;
                    }
                    row += 1 + CONTEXT_EDGE;
                } else {
                    row += count;
                }
            }
        }
        if let Some(key) = matched {
            self.toggle_context(key);
            true
        } else {
            false
        }
    }

    pub(crate) fn select_visible_diff_row(&mut self, visible_row: usize) -> bool {
        if visible_row == 0 {
            return false;
        }
        let target_row = self
            .diff_scroll
            .saturating_add(visible_row.saturating_sub(1));
        let Some(NativeDiffRow {
            id: NativeDiffRowId::Hunk { hunk_index, .. },
            ..
        }) = self.visible_diff_rows().get(target_row).copied()
        else {
            return false;
        };
        self.selected_hunk = Some(hunk_index);
        true
    }

    pub(crate) fn scroll_diff(&mut self, delta: isize, viewport_rows: usize) {
        let max_scroll = self.max_diff_scroll(viewport_rows);
        self.diff_scroll = self
            .diff_scroll
            .saturating_add_signed(delta)
            .min(max_scroll);
    }

    pub(crate) fn scroll_diff_columns(&mut self, delta: isize, viewport_cols: usize) {
        if self.wrap_lines {
            self.diff_col_scroll = 0;
            return;
        }
        let max_scroll = self.max_diff_col_scroll(viewport_cols);
        self.diff_col_scroll = self
            .diff_col_scroll
            .saturating_add_signed(delta)
            .min(max_scroll);
    }

    pub(crate) fn max_diff_col_scroll(&self, viewport_cols: usize) -> usize {
        if self.wrap_lines {
            return 0;
        }
        let max_width = self
            .selected_file()
            .into_iter()
            .flat_map(|file| file.hunks.iter())
            .flat_map(|hunk| hunk.lines.iter())
            .map(|line| unicode_width::UnicodeWidthStr::width(line.text.as_str()))
            .max()
            .unwrap_or(0);
        max_width.saturating_sub(viewport_cols)
    }
    fn max_diff_scroll(&self, viewport_rows: usize) -> usize {
        let body_rows = self.visible_diff_rows().len().saturating_sub(1);
        body_rows.saturating_sub(viewport_rows)
    }
    pub(crate) fn select_visible_file_row(&mut self, visible_row: usize) -> bool {
        let target_row = self.file_scroll.saturating_add(visible_row);
        let mut row = 0;
        for bucket in [
            DiffBucket::Changed,
            DiffBucket::Untracked,
            DiffBucket::Staged,
        ] {
            if !self.scope.includes(bucket) {
                continue;
            }
            let files = self
                .session
                .files
                .iter()
                .enumerate()
                .filter(|(_, file)| file.bucket == bucket);
            let mut saw_bucket = false;
            for (file_index, file) in files {
                if !saw_bucket {
                    if row == target_row {
                        return false;
                    }

                    row += 1;
                    saw_bucket = true;
                }
                if row == target_row {
                    self.selected_file = Some(NativeDiffSelection {
                        bucket: file.bucket,
                        file_index,
                    });
                    return true;
                }
                row += 1;
            }
            if saw_bucket {
                row += 1;
            }
        }
        false
    }

    pub(crate) fn scroll_file_list(&mut self, delta: isize, viewport_rows: usize) {
        let max_scroll = self.file_list_row_count().saturating_sub(viewport_rows);
        self.file_scroll = self
            .file_scroll
            .saturating_add_signed(delta)
            .min(max_scroll);
    }
}

fn push_agent_file_patch(payload: &mut String, file: &NativeDiffFile) {
    payload.push_str("```diff\n");
    push_file_patch_header(payload, file);
    for hunk in &file.hunks {
        push_hunk_patch_body(payload, hunk);
    }
    payload.push_str("```\n");
}

fn push_agent_hunk_patch(payload: &mut String, file: &NativeDiffFile, hunk: &NativeDiffHunk) {
    payload.push_str("```diff\n");
    push_file_patch_header(payload, file);
    push_hunk_patch_body(payload, hunk);
    payload.push_str("```\n");
}

fn push_file_patch_header(payload: &mut String, file: &NativeDiffFile) {
    let old = file
        .old_path
        .as_ref()
        .map(|path| format!("a/{}", path.display()))
        .unwrap_or_else(|| "/dev/null".to_string());
    let new = file
        .new_path
        .as_ref()
        .map(|path| format!("b/{}", path.display()))
        .unwrap_or_else(|| "/dev/null".to_string());
    payload.push_str(&format!("--- {old}\n+++ {new}\n"));
}

fn push_hunk_patch_body(payload: &mut String, hunk: &NativeDiffHunk) {
    payload.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
    ));
    for line in &hunk.lines {
        let marker = match line.kind {
            DiffLineKind::Added => '+',
            DiffLineKind::Removed => '-',
            DiffLineKind::Context => ' ',
        };
        payload.push(marker);
        payload.push_str(&line.text);
        payload.push('\n');
    }
}

fn first_selection(session: &NativeDiffSession) -> Option<NativeDiffSelection> {
    first_selection_for_scope(session, NativeDiffScope::All)
}

fn first_selection_for_scope(
    session: &NativeDiffSession,
    scope: NativeDiffScope,
) -> Option<NativeDiffSelection> {
    session
        .files
        .iter()
        .enumerate()
        .find(|(_, file)| file.bucket == DiffBucket::Changed && scope.includes(file.bucket))
        .or_else(|| {
            session
                .files
                .iter()
                .enumerate()
                .find(|(_, file)| scope.includes(file.bucket))
        })
        .map(|(file_index, file)| NativeDiffSelection {
            bucket: file.bucket,
            file_index,
        })
}
fn single_hunk_patch(file: &NativeDiffFile, hunk: &NativeDiffHunk) -> Vec<u8> {
    let old_path = file
        .old_path
        .as_ref()
        .map(|path| format!("a/{}", path.display()))
        .unwrap_or_else(|| "/dev/null".to_string());
    let new_path = file
        .new_path
        .as_ref()
        .map(|path| format!("b/{}", path.display()))
        .unwrap_or_else(|| "/dev/null".to_string());
    let mut patch = Vec::new();
    patch.extend_from_slice(format!("--- {old_path}\n+++ {new_path}\n").as_bytes());
    patch.extend_from_slice(
        format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
        )
        .as_bytes(),
    );
    for line in &hunk.lines {
        let marker = match line.kind {
            DiffLineKind::Context => ' ',
            DiffLineKind::Added => '+',
            DiffLineKind::Removed => '-',
        };
        patch.extend_from_slice(format!("{marker}{}\n", line.text).as_bytes());
    }
    patch
}

fn git_apply(repo_root: &Path, patch: &[u8], args: &[&str]) -> Result<(), String> {
    use std::io::Write;

    let mut child = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("apply")
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to run git apply: {err}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(patch)
            .map_err(|err| format!("failed to write patch: {err}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("failed to wait for git apply: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git apply {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn run_git_path(repo_root: &Path, args: &[&str], path: &Path) -> Result<(), String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .arg(path)
        .output()
        .map_err(|err| format!("failed to run git: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDiffParseError(pub(crate) String);

pub(crate) fn native_diff_file_list_width(total: u16) -> u16 {
    total.clamp(28, 36).min(total.saturating_sub(10))
}

pub(crate) fn parse_native_diff_session(
    repo_root: impl Into<PathBuf>,
    changed_patch: &[u8],
    staged_patch: &[u8],
) -> Result<NativeDiffSession, NativeDiffParseError> {
    let mut files = Vec::new();
    files.extend(parse_patch_bucket(DiffBucket::Changed, changed_patch)?);
    files.extend(parse_patch_bucket(DiffBucket::Staged, staged_patch)?);
    Ok(NativeDiffSession {
        repo_root: repo_root.into(),
        files,
    })
}

pub(crate) fn parse_native_diff_bucket(
    bucket: DiffBucket,
    patch: &[u8],
) -> Result<Vec<NativeDiffFile>, NativeDiffParseError> {
    parse_patch_bucket(bucket, patch)
}

pub(crate) fn load_native_diff_session(
    repo_root: impl Into<PathBuf>,
) -> Result<NativeDiffSession, NativeDiffParseError> {
    load_native_diff_session_metadata(repo_root)
}

pub(crate) fn load_native_diff_session_metadata(
    repo_root: impl Into<PathBuf>,
) -> Result<NativeDiffSession, NativeDiffParseError> {
    let repo_root = repo_root.into();
    let changed_patch = git_output(
        &repo_root,
        &["diff", "--no-color", "--find-renames", "--binary"],
    )?;
    let staged_patch = git_output(
        &repo_root,
        &[
            "diff",
            "--cached",
            "--no-color",
            "--find-renames",
            "--binary",
        ],
    )?;
    let mut session = parse_native_diff_session(&repo_root, &changed_patch, &staged_patch)?;
    for (path, contents) in untracked_files(&repo_root)? {
        let patch = synthetic_untracked_file_patch(&path, &contents)?;
        let files = parse_native_diff_bucket(DiffBucket::Untracked, &patch)?;
        session.files.extend(files);
    }
    Ok(session)
}

fn git_output(repo_root: &Path, args: &[&str]) -> Result<Vec<u8>, NativeDiffParseError> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .map_err(|err| NativeDiffParseError(format!("failed to run git: {err}")))?;
    if !output.status.success() {
        return Err(NativeDiffParseError(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

const MAX_SYNTAX_SIDES_PER_SESSION: usize = 96;
const MAX_SYNTAX_BYTES_PER_SESSION: usize = 4 * 1024 * 1024;

struct SyntaxAnalysisBudget {
    sides_remaining: usize,
    bytes_remaining: usize,
}

impl SyntaxAnalysisBudget {
    fn new() -> Self {
        Self {
            sides_remaining: MAX_SYNTAX_SIDES_PER_SESSION,
            bytes_remaining: MAX_SYNTAX_BYTES_PER_SESSION,
        }
    }

    fn reserve(&mut self, bytes: usize) -> bool {
        if self.sides_remaining == 0 || bytes > self.bytes_remaining {
            return false;
        }
        self.sides_remaining -= 1;
        self.bytes_remaining -= bytes;
        true
    }

    fn exhausted(&self) -> bool {
        self.sides_remaining == 0 || self.bytes_remaining == 0
    }
}

pub(crate) fn load_syntax_for_session(session: &NativeDiffSession) -> NativeDiffSyntaxCache {
    let mut budget = SyntaxAnalysisBudget::new();
    let mut cache = NativeDiffSyntaxCache::default();
    for file in &session.files {
        if file.binary {
            continue;
        }
        if let Some(document) = load_syntax_side(&session.repo_root, file, true, &mut budget) {
            cache.insert(file, true, document);
        }
        if let Some(document) = load_syntax_side(&session.repo_root, file, false, &mut budget) {
            cache.insert(file, false, document);
        }
    }
    cache
}

fn load_syntax_side(
    repo_root: &Path,
    file: &NativeDiffFile,
    old_side: bool,
    budget: &mut SyntaxAnalysisBudget,
) -> Option<crate::native_diff_syntax::NativeDiffSyntaxDocument> {
    if budget.exhausted() {
        return Some(crate::native_diff_syntax::NativeDiffSyntaxDocument::plain(
            true,
        ));
    }
    let size = source_side_size(repo_root, file, old_side)?;
    if !budget.reserve(size) {
        return Some(crate::native_diff_syntax::NativeDiffSyntaxDocument::plain(
            true,
        ));
    }
    let (path, contents) = load_source_side(repo_root, file, old_side)?;
    Some(crate::native_diff_syntax::analyze_source(&path, &contents))
}

fn source_side_size(repo_root: &Path, file: &NativeDiffFile, old_side: bool) -> Option<usize> {
    let path = if old_side {
        file.old_path.as_ref()?
    } else {
        file.new_path.as_ref()?
    };
    match (file.bucket, old_side) {
        (DiffBucket::Changed | DiffBucket::Untracked, false) => {
            std::fs::metadata(repo_root.join(path))
                .ok()?
                .len()
                .try_into()
                .ok()
        }
        (DiffBucket::Changed, true) | (DiffBucket::Staged, true) | (DiffBucket::Staged, false) => {
            let rev_path = source_side_rev_path(file, path, old_side)?;
            let output = git_output(repo_root, &["cat-file", "-s", &rev_path]).ok()?;
            String::from_utf8_lossy(&output).trim().parse().ok()
        }
        (DiffBucket::Untracked, true) => None,
    }
}

fn source_side_rev_path(file: &NativeDiffFile, path: &Path, old_side: bool) -> Option<String> {
    match (file.bucket, old_side) {
        (DiffBucket::Changed, true) => Some(format!(":0:{}", path.display())),
        (DiffBucket::Staged, true) => Some(format!("HEAD:{}", path.display())),
        (DiffBucket::Staged, false) => Some(format!(":0:{}", path.display())),
        _ => None,
    }
}

fn load_source_side(
    repo_root: &Path,
    file: &NativeDiffFile,
    old_side: bool,
) -> Option<(PathBuf, Vec<u8>)> {
    let path = if old_side {
        file.old_path.as_ref()?
    } else {
        file.new_path.as_ref()?
    };
    let contents = match (file.bucket, old_side) {
        (DiffBucket::Changed | DiffBucket::Untracked, false) => {
            std::fs::read(repo_root.join(path)).ok()?
        }
        (DiffBucket::Changed, true) | (DiffBucket::Staged, true) | (DiffBucket::Staged, false) => {
            git_blob(repo_root, &source_side_rev_path(file, path, old_side)?).ok()?
        }
        (DiffBucket::Untracked, true) => return None,
    };
    Some((path.clone(), contents))
}

fn git_blob(repo_root: &Path, rev_path: &str) -> Result<Vec<u8>, NativeDiffParseError> {
    git_output(
        repo_root,
        &["show", "--no-ext-diff", "--no-textconv", rev_path],
    )
}

fn untracked_files(repo_root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, NativeDiffParseError> {
    let output = git_output(
        repo_root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            let relative = PathBuf::from(String::from_utf8_lossy(path).into_owned());
            let absolute = repo_root.join(&relative);
            let contents = std::fs::read(&absolute).map_err(|err| {
                NativeDiffParseError(format!(
                    "failed to read untracked file {}: {err}",
                    absolute.display()
                ))
            })?;
            Ok((relative, contents))
        })
        .collect()
}
pub(crate) fn synthetic_untracked_file_patch(
    path: &Path,
    contents: &[u8],
) -> Result<Vec<u8>, NativeDiffParseError> {
    let path = path
        .to_str()
        .ok_or_else(|| NativeDiffParseError("untracked path is not utf-8".to_string()))?;
    let mut patch = Vec::new();
    patch.extend_from_slice(format!("--- /dev/null\n+++ b/{path}\n").as_bytes());
    let lines = patchkit::unified::splitlines(contents).collect::<Vec<_>>();
    patch.extend_from_slice(format!("@@ -0,0 +1,{} @@\n", lines.len()).as_bytes());
    for line in lines {
        patch.extend_from_slice(b"+");
        patch.extend_from_slice(line);
    }
    Ok(patch)
}

fn parse_patch_bucket(
    bucket: DiffBucket,
    patch: &[u8],
) -> Result<Vec<NativeDiffFile>, NativeDiffParseError> {
    if patch.is_empty() {
        return Ok(Vec::new());
    }
    UnifiedPatch::parse_patches(patchkit::unified::splitlines(patch).map(|line| line.to_vec()))
        .map_err(|err| NativeDiffParseError(format!("failed to parse git patch: {err:?}")))?
        .into_iter()
        .map(|patch| native_file_from_patch(bucket, patch))
        .collect()
}

fn native_file_from_patch(
    bucket: DiffBucket,
    patch: PlainOrBinaryPatch,
) -> Result<NativeDiffFile, NativeDiffParseError> {
    match patch {
        PlainOrBinaryPatch::Plain(patch) => Ok(native_file_from_plain_patch(bucket, patch)),
        PlainOrBinaryPatch::Binary(patch) => Ok(NativeDiffFile {
            bucket,
            old_path: normalize_patch_path(&patch.0),
            new_path: normalize_patch_path(&patch.1),
            status: DiffFileStatus::Binary,
            added: 0,
            deleted: 0,
            hunks: Vec::new(),
            binary: true,
        }),
    }
}

fn native_file_from_plain_patch(bucket: DiffBucket, patch: UnifiedPatch) -> NativeDiffFile {
    let old_path = normalize_patch_path(&patch.orig_name);
    let new_path = normalize_patch_path(&patch.mod_name);
    let status = file_status(old_path.as_deref(), new_path.as_deref(), false);
    let mut added = 0;
    let mut deleted = 0;
    let hunks = patch
        .hunks
        .into_iter()
        .map(|hunk| {
            let mut old_line = hunk.orig_pos;
            let mut new_line = hunk.mod_pos;
            let lines = hunk
                .lines
                .into_iter()
                .map(|line| match line {
                    HunkLine::ContextLine(text) => {
                        let line = NativeDiffLine {
                            kind: DiffLineKind::Context,
                            old_line: Some(old_line),
                            new_line: Some(new_line),
                            text: lossy_line_text(&text),
                        };
                        old_line += 1;
                        new_line += 1;
                        line
                    }
                    HunkLine::InsertLine(text) => {
                        added += 1;
                        let line = NativeDiffLine {
                            kind: DiffLineKind::Added,
                            old_line: None,
                            new_line: Some(new_line),
                            text: lossy_line_text(&text),
                        };
                        new_line += 1;
                        line
                    }
                    HunkLine::RemoveLine(text) => {
                        deleted += 1;
                        let line = NativeDiffLine {
                            kind: DiffLineKind::Removed,
                            old_line: Some(old_line),
                            new_line: None,
                            text: lossy_line_text(&text),
                        };
                        old_line += 1;
                        line
                    }
                })
                .collect();
            NativeDiffHunk {
                old_start: hunk.orig_pos,
                old_count: hunk.orig_range,
                new_start: hunk.mod_pos,
                new_count: hunk.mod_range,
                lines,
            }
        })
        .collect();

    NativeDiffFile {
        bucket,
        old_path,
        new_path,
        status,
        added,
        deleted,
        hunks,
        binary: false,
    }
}

fn file_status(old_path: Option<&Path>, new_path: Option<&Path>, binary: bool) -> DiffFileStatus {
    if binary {
        return DiffFileStatus::Binary;
    }
    match (old_path, new_path) {
        (None, Some(_)) => DiffFileStatus::Added,
        (Some(_), None) => DiffFileStatus::Deleted,
        (Some(old), Some(new)) if old != new => DiffFileStatus::Renamed,
        _ => DiffFileStatus::Modified,
    }
}

fn normalize_patch_path(raw: &[u8]) -> Option<PathBuf> {
    if raw == b"/dev/null" {
        return None;
    }
    let text = String::from_utf8_lossy(raw);
    let stripped = text
        .strip_prefix("a/")
        .or_else(|| text.strip_prefix("b/"))
        .unwrap_or(&text);

    Some(PathBuf::from(stripped))
}

fn lossy_line_text(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    text.trim_end_matches('\n').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_changed_staged_and_untracked_files_from_git() {
        let repo =
            std::env::temp_dir().join(format!("hako-native-diff-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("create repo");
        run_git(&repo, &["init"]);
        run_git(&repo, &["config", "user.email", "hako@example.com"]);
        run_git(&repo, &["config", "user.name", "Hako"]);
        std::fs::write(repo.join("changed.txt"), "old\n").expect("write tracked");
        std::fs::write(repo.join("staged.txt"), "old\n").expect("write staged");
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-m", "initial"]);
        std::fs::write(repo.join("changed.txt"), "new\n").expect("modify tracked");
        std::fs::write(repo.join("staged.txt"), "new\n").expect("modify staged");
        run_git(&repo, &["add", "staged.txt"]);
        std::fs::write(repo.join("untracked.txt"), "fresh\n").expect("write untracked");

        let session = load_native_diff_session(&repo).expect("load native diff");

        assert!(session.files.iter().any(|file| {
            file.bucket == DiffBucket::Changed
                && file.new_path.as_deref() == Some(Path::new("changed.txt"))
        }));
        assert!(session.files.iter().any(|file| {
            file.bucket == DiffBucket::Staged
                && file.new_path.as_deref() == Some(Path::new("staged.txt"))
        }));
        assert!(session.files.iter().any(|file| {
            file.bucket == DiffBucket::Untracked
                && file.status == DiffFileStatus::Added
                && file.new_path.as_deref() == Some(Path::new("untracked.txt"))
        }));
        let _ = std::fs::remove_dir_all(repo);
    }

    fn run_git(repo: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn source_side_loading_matches_git_bucket_storage() {
        let repo =
            std::env::temp_dir().join(format!("hako-native-diff-sources-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("create repo");
        run_git(&repo, &["init"]);
        run_git(&repo, &["config", "user.email", "hako@example.com"]);
        run_git(&repo, &["config", "user.name", "Hako"]);
        std::fs::write(repo.join("changed.rs"), "fn base_changed() {}\n").expect("write changed");
        std::fs::write(repo.join("staged.rs"), "fn base_staged() {}\n").expect("write staged");
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-m", "initial"]);
        std::fs::write(repo.join("changed.rs"), "fn worktree_changed() {}\n")
            .expect("modify changed");
        std::fs::write(repo.join("staged.rs"), "fn index_staged() {}\n").expect("modify staged");
        run_git(&repo, &["add", "staged.rs"]);
        std::fs::write(repo.join("untracked.rs"), "fn untracked_new() {}\n")
            .expect("write untracked");

        let session = load_native_diff_session(&repo).expect("load native diff");
        let changed = session
            .files
            .iter()
            .find(|file| file.bucket == DiffBucket::Changed)
            .expect("changed file");
        let staged = session
            .files
            .iter()
            .find(|file| file.bucket == DiffBucket::Staged)
            .expect("staged file");
        let untracked = session
            .files
            .iter()
            .find(|file| file.bucket == DiffBucket::Untracked)
            .expect("untracked file");

        let (_, changed_old) = load_source_side(&repo, changed, true).expect("changed old");
        let (_, changed_new) = load_source_side(&repo, changed, false).expect("changed new");
        let (_, staged_old) = load_source_side(&repo, staged, true).expect("staged old");
        let (_, staged_new) = load_source_side(&repo, staged, false).expect("staged new");
        let (_, untracked_new) = load_source_side(&repo, untracked, false).expect("untracked new");

        assert_eq!(
            String::from_utf8_lossy(&changed_old),
            "fn base_changed() {}\n"
        );
        assert_eq!(
            String::from_utf8_lossy(&changed_new),
            "fn worktree_changed() {}\n"
        );
        assert_eq!(
            String::from_utf8_lossy(&staged_old),
            "fn base_staged() {}\n"
        );
        assert_eq!(
            String::from_utf8_lossy(&staged_new),
            "fn index_staged() {}\n"
        );
        assert_eq!(
            String::from_utf8_lossy(&untracked_new),
            "fn untracked_new() {}\n"
        );
        assert!(load_source_side(&repo, untracked, true).is_none());
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn syntax_cache_lives_outside_diff_identity() {
        let patch = b"--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n";
        let session = parse_native_diff_session("/repo", patch, b"").expect("parse");
        let mut state = NativeDiffPaneState::new(session.clone());
        state.syntax.insert(
            &session.files[0],
            false,
            crate::native_diff_syntax::NativeDiffSyntaxDocument {
                engine: crate::native_diff_syntax::NativeDiffSyntaxEngine::TreeSitter,
                degraded: false,
                ranges: vec![crate::native_diff_syntax::NativeDiffHighlightRange {
                    line: 1,
                    start_col: 0,
                    end_col: 3,
                    role: crate::native_diff_syntax::NativeDiffSyntaxRole::Keyword,
                }],
            },
        );

        assert_eq!(state.session, session);
    }

    #[test]
    fn replace_session_preserves_bucket_when_path_exists_in_multiple_sources() {
        let patch = b"--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n";
        let mut state = NativeDiffPaneState::new(
            parse_native_diff_session("/repo", patch, patch).expect("parse initial"),
        );
        let staged_index = state
            .session
            .files
            .iter()
            .position(|file| file.bucket == DiffBucket::Staged)
            .expect("staged copy");
        state.selected_file = Some(NativeDiffSelection {
            bucket: DiffBucket::Staged,
            file_index: staged_index,
        });

        state.replace_session(
            parse_native_diff_session("/repo", patch, patch).expect("parse next"),
            NativeDiffSyntaxCache::default(),
        );

        assert_eq!(
            state.selected_file().map(|file| file.bucket),
            Some(DiffBucket::Staged)
        );
    }
    #[test]
    fn parses_changed_and_staged_buckets() {
        let changed = b"--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,2 +1,2 @@\n fn main() {\n-    old();\n+    new();\n }\n";
        let staged = b"--- /dev/null\n+++ b/README.md\n@@ -0,0 +1 @@\n+hello\n";

        let session = parse_native_diff_session("/repo", changed, staged).expect("parse patch");

        assert_eq!(session.files.len(), 2);
        assert_eq!(session.files[0].bucket, DiffBucket::Changed);
        assert_eq!(session.files[0].status, DiffFileStatus::Modified);
        assert_eq!(session.files[0].added, 1);
        assert_eq!(session.files[0].deleted, 1);
        assert_eq!(
            session.files[0].hunks[0].lines[1].kind,
            DiffLineKind::Removed
        );
        assert_eq!(session.files[1].bucket, DiffBucket::Staged);
        assert_eq!(session.files[1].status, DiffFileStatus::Added);
        assert_eq!(session.files[1].new_path, Some(PathBuf::from("README.md")));
    }

    #[test]
    fn parses_rename_without_counting_it_as_add_delete() {
        let patch = b"--- a/old.rs\n+++ b/new.rs\n@@ -1 +1 @@\n-old\n+new\n";

        let session = parse_native_diff_session("/repo", patch, b"").expect("parse patch");

        let file = &session.files[0];
        assert_eq!(file.status, DiffFileStatus::Renamed);
        assert_eq!(file.old_path, Some(PathBuf::from("old.rs")));
        assert_eq!(file.new_path, Some(PathBuf::from("new.rs")));
        assert_eq!(file.added, 1);
        assert_eq!(file.deleted, 1);
    }

    #[test]
    fn parses_deleted_file() {
        let patch = b"--- a/dead.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-dead\n";

        let session = parse_native_diff_session("/repo", patch, b"").expect("parse patch");

        let file = &session.files[0];
        assert_eq!(file.status, DiffFileStatus::Deleted);
        assert_eq!(file.old_path, Some(PathBuf::from("dead.rs")));
        assert_eq!(file.new_path, None);
        assert_eq!(file.deleted, 1);
    }

    #[test]
    fn synthesizes_untracked_file_as_added_patch() {
        let patch = synthetic_untracked_file_patch(Path::new("notes/todo.txt"), b"first\nsecond\n")
            .expect("synthetic untracked patch");

        let files = parse_native_diff_bucket(DiffBucket::Changed, &patch).expect("parse patch");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, DiffFileStatus::Added);
        assert_eq!(files[0].new_path, Some(PathBuf::from("notes/todo.txt")));
        assert_eq!(files[0].added, 2);
        assert_eq!(files[0].hunks[0].lines[0].text, "first");
    }

    #[test]
    fn native_diff_horizontal_scroll_is_disabled_when_wrapping() {
        let patch = b"--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n";
        let mut state = NativeDiffPaneState::new(
            parse_native_diff_session("/repo", patch, b"").expect("parse patch"),
        );

        state.scroll_diff_columns(12, 1);
        assert_eq!(state.diff_col_scroll, 2);

        state.toggle_wrap_lines();
        assert_eq!(state.diff_col_scroll, 0);

        state.scroll_diff_columns(12, 1);
        assert_eq!(state.diff_col_scroll, 0);
    }

    #[test]
    fn native_diff_horizontal_scroll_clamps_to_visible_text() {
        let patch = b"--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+abcdefghijkl\n";
        let mut state = NativeDiffPaneState::new(
            parse_native_diff_session("/repo", patch, b"").expect("parse patch"),
        );

        state.scroll_diff_columns(40, 5);

        assert_eq!(state.diff_col_scroll, 7);
    }

    #[test]
    fn moving_hunk_selection_scrolls_to_selected_hunk() {
        let patch = b"--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old_one\n+new_one\n@@ -20 +20 @@\n-old_two\n+new_two\n@@ -40 +40 @@\n-old_three\n+new_three\n";
        let mut state = NativeDiffPaneState::new(
            parse_native_diff_session("/repo", patch, b"").expect("parse patch"),
        );

        state.move_hunk_selection(1);

        let rows = state.visible_diff_rows();
        assert_eq!(state.selected_hunk, Some(1));
        assert!(matches!(
            rows[state.diff_scroll + 1].id,
            NativeDiffRowId::Hunk { hunk_index: 1, .. }
        ));

        state.move_hunk_selection(1);

        let rows = state.visible_diff_rows();
        assert_eq!(state.selected_hunk, Some(2));
        assert!(matches!(
            rows[state.diff_scroll + 1].id,
            NativeDiffRowId::Hunk { hunk_index: 2, .. }
        ));
    }

    #[test]
    fn replace_session_preserves_visible_diff_row_when_file_still_exists() {
        let patch =
            b"--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,3 @@\n one\n-two\n+deux\n three\n";
        let mut state = NativeDiffPaneState::new(
            parse_native_diff_session("/repo", patch, b"").expect("parse"),
        );
        state.diff_scroll = state
            .visible_diff_rows()
            .iter()
            .position(|row| row.old_line == Some(2) && row.new_line.is_none())
            .expect("removed row");

        let updated = b"--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,4 @@\n one\n-two\n+deux\n three\n+four\n";
        state.replace_session(
            parse_native_diff_session("/repo", updated, b"").expect("parse"),
            NativeDiffSyntaxCache::default(),
        );

        let top = state.visible_diff_rows()[state.diff_scroll];
        assert_eq!(top.old_line, Some(2));
        assert_eq!(top.new_line, None);
    }

    #[test]
    fn diff_scroll_clamps_to_last_visible_viewport() {
        let patch = b"--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,5 +1,5 @@\n one\n two\n-three\n+trois\n four\n five\n";
        let mut state = NativeDiffPaneState::new(
            parse_native_diff_session("/repo", patch, b"").expect("parse"),
        );

        state.scroll_diff(100, 3);

        assert_eq!(state.diff_scroll, 4);
    }

    #[test]
    fn toggles_collapsed_and_expanded_context_rows() {
        let patch =
            b"--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,12 +1,12 @@\n one\n two\n three\n four\n five\n six\n seven\n eight\n nine\n ten\n-old\n+new\n twelve\n";
        let mut state = NativeDiffPaneState::new(
            parse_native_diff_session("/repo", patch, b"").expect("parse"),
        );

        assert!(state.toggle_visible_context_row(5));
        assert!(state.context_expanded(NativeDiffContextKey {
            file_index: 0,
            hunk_index: 0,
            run_index: 0,
        }));
        assert!(state.toggle_visible_context_row(2));
        assert!(!state.context_expanded(NativeDiffContextKey {
            file_index: 0,
            hunk_index: 0,
            run_index: 0,
        }));
    }

    #[test]
    fn stage_selected_hunk_only_stages_that_hunk() {
        let repo =
            std::env::temp_dir().join(format!("hako-native-diff-hunk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("create repo");
        run_git(&repo, &["init"]);
        run_git(&repo, &["config", "user.email", "hako@example.com"]);
        run_git(&repo, &["config", "user.name", "Hako"]);
        let original = (1..=20)
            .map(|line| format!("line {line}\n"))
            .collect::<String>();
        std::fs::write(repo.join("file.txt"), original).expect("write file");
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-m", "initial"]);
        let changed = (1..=20)
            .map(|line| match line {
                2 => "line two\n".to_string(),
                19 => "line nineteen\n".to_string(),
                _ => format!("line {line}\n"),
            })
            .collect::<String>();
        std::fs::write(repo.join("file.txt"), changed).expect("modify file");
        let mut state = NativeDiffPaneState::new(load_native_diff_session(&repo).expect("load"));
        state.selected_hunk = Some(0);

        state.stage_selected_hunk();

        let staged = git_output(
            &repo,
            &[
                "diff",
                "--cached",
                "--no-color",
                "--find-renames",
                "--binary",
            ],
        )
        .expect("read staged diff");
        let staged = String::from_utf8_lossy(&staged);
        assert!(staged.contains("line two"));
        assert!(!staged.contains("line nineteen"));
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn scope_cycle_filters_file_selection() {
        let mut state = NativeDiffPaneState::new(NativeDiffSession {
            repo_root: PathBuf::from("/repo"),
            files: vec![
                NativeDiffFile {
                    bucket: DiffBucket::Changed,
                    old_path: Some(PathBuf::from("changed.rs")),
                    new_path: Some(PathBuf::from("changed.rs")),
                    status: DiffFileStatus::Modified,
                    added: 1,
                    deleted: 0,
                    hunks: Vec::new(),
                    binary: false,
                },
                NativeDiffFile {
                    bucket: DiffBucket::Untracked,
                    old_path: None,
                    new_path: Some(PathBuf::from("new.rs")),
                    status: DiffFileStatus::Added,
                    added: 1,
                    deleted: 0,
                    hunks: Vec::new(),
                    binary: false,
                },
            ],
        });

        state.cycle_scope();
        assert_eq!(state.scope, NativeDiffScope::Unstaged);
        assert_eq!(
            state.selected_file().map(|file| file.bucket),
            Some(DiffBucket::Changed)
        );

        state.cycle_scope();
        assert_eq!(state.scope, NativeDiffScope::Untracked);
        assert_eq!(
            state.selected_file().map(|file| file.bucket),
            Some(DiffBucket::Untracked)
        );
        assert_eq!(state.file_list_row_count(), 2);
    }
}
