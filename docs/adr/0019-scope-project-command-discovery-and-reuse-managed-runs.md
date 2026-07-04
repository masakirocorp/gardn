---
status: accepted
---

# Scope project command discovery and reuse managed runs

Hako treats project commands as workspace-context actions, not as a global task registry. `AppState::refresh_command_catalog` collects cwd values from panes in the active workspace, or the selected workspace while navigating, walks each cwd up to a known project marker with `project_root_from_cwd`, falls back to the original cwd when no marker exists, sorts/dedupes those derived `PathBuf` roots, and discovers commands per root. Discovery sources are explicit tasks/scripts first (`.vscode/tasks.json`, `package.json`, `composer.json`, `justfile`, `Makefile`) plus conservative native defaults for common project types such as Cargo, Go, Maven, Gradle, .NET, Python, PHP, and Ruby.

Git actions follow the same workspace-context rule but use observed repos rather than project markers. An observed repo is a Git root learned from the workspace default directory, from pane cwd values in that workspace, or from direct children of a non-Git cwd. The direct-child rule supports intentionally opened "fake monorepo" parent folders without recursively crawling arbitrary descendants. If a workspace has one observed repo, Git actions such as the built-in configured Git diff command can launch directly. If a workspace has several observed repos, Hako must ask which repo to target instead of letting the focused pane silently decide. If a workspace has no observed repo, Git actions are unavailable. Observed repo status is background-refreshed per repo root and is presentation context, not a blocking prerequisite for opening a Git action.

`ProjectCommand` is the catalog contract: id, root, source, name, shell command, and confidence. Discovered catalog command ids include root, source label, name, and command text so managed run state follows the exact discovered command definition instead of only the display name. Built-in entries can define their own ids, such as the Git diff command's `builtin:git-diff:<root>` id. Discovery dedupes identical root/source/name/command tuples, sorts explicit commands before native defaults, and the right-sidebar command panel groups rows by repo/branch context with duplicate labels disambiguated by parent/name.

A command run is a managed pane tied to a command id. `run_project_command` resolves the command root back to a workspace, `open_command_tab` starts the command in a named command tab rooted at the project, and `command_runs` stores the command id, terminal id, and status. Rerunning a running command focuses the existing pane when it still exists. For any existing non-running run (`Stopped`, `Failed`, or `Unknown`) whose pane is still present, rerun restarts in the same pane/terminal id. If the pane disappeared, the run is marked `Unknown` and a new command tab is opened. `stop_project_command` removes the runtime and records `Stopped` or `Unknown`; `refresh_command_run_statuses` marks missing or non-live running runtimes `Stopped`; pane-death handling records `Stopped` or `Failed` from exit success and ignores stale pane-death events for a restarted command.

This is separate from ADR 0013's local API transport decision. The JSON API and CLI may expose commands, but this ADR records the product semantics for how commands are discovered from workspace context and how command runs are reused once launched.

## Current rationale

`[INFERENCE]` Hako scopes commands from the current workspace because commands are useful when they are near the pane/project the user is already working in. It keeps managed run state because dev servers, test watchers, and build tasks should be focusable and restartable without filling the workspace with duplicate terminal tabs. It keeps native defaults lower-confidence because inferred commands are helpful fallbacks but should not override explicit project tasks.

## Consequences

New command sources should produce `ProjectCommand` values with stable source labels and deterministic ids, and should be conservative about noisy lifecycle/private tasks. Discovery should stay tied to workspace pane cwd roots rather than scanning arbitrary directories.

Git action launchers should resolve targets from observed repos. A direct launcher is correct only for a single observed repo. Multiple observed repos are a product state, common when a workspace intentionally represents a non-Git parent folder that contains several cloned repositories, and should be resolved through explicit user choice rather than focus-dependent behavior. Git action discovery may inspect direct children of a non-Git cwd, but should not recurse beyond that without a separate product decision. Any repo status shown in a target picker should come from the background observed-repo status cache: known dirty and known clean can be displayed, unknown should stay blank, and refresh-in-progress should not add loaders or block the picker.

Command execution features should preserve the one-managed-run-per-command-id invariant. For discovered catalog commands, a changed definition changes the id and Hako should treat it as a different managed run; built-in command ids must define their own stability boundary explicitly. If a command pane is reused, status and terminal ownership must update without replacing the surrounding workspace/tab identity.
