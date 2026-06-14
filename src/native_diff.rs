use std::path::{Path, PathBuf};

use patchkit::unified::{HunkLine, PlainOrBinaryPatch, UnifiedPatch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffBucket {
    Changed,
    Staged,
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
pub(crate) struct NativeDiffSelection {
    pub(crate) bucket: DiffBucket,
    pub(crate) file_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDiffPaneState {
    pub(crate) session: NativeDiffSession,
    pub(crate) selected_file: Option<NativeDiffSelection>,
    pub(crate) file_scroll: usize,
    pub(crate) diff_scroll: usize,
}

impl NativeDiffPaneState {
    pub(crate) fn new(session: NativeDiffSession) -> Self {
        let selected_file = first_selection(&session);
        Self {
            session,
            selected_file,
            file_scroll: 0,
            diff_scroll: 0,
        }
    }

    pub(crate) fn selected_file(&self) -> Option<&NativeDiffFile> {
        let selection = self.selected_file?;
        self.session
            .files
            .get(selection.file_index)
            .filter(|file| file.bucket == selection.bucket)
    }
}

fn first_selection(session: &NativeDiffSession) -> Option<NativeDiffSelection> {
    session
        .files
        .iter()
        .enumerate()
        .find(|(_, file)| file.bucket == DiffBucket::Changed)
        .or_else(|| session.files.iter().enumerate().next())
        .map(|(file_index, file)| NativeDiffSelection {
            bucket: file.bucket,
            file_index,
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDiffParseError(pub(crate) String);

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
        session
            .files
            .extend(parse_native_diff_bucket(DiffBucket::Changed, &patch)?);
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
            file.bucket == DiffBucket::Changed
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
}
