use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CommandSource {
    BuiltIn,
    Vscode,
    PackageJson,
    Composer,
    Just,
    Make,
    Cargo,
    Go,
    Maven,
    Gradle,
    Dotnet,
    Python,
    Php,
    Ruby,
}

impl CommandSource {
    pub(crate) fn label(self) -> &'static str {
        match self {
            CommandSource::BuiltIn => "omh",
            CommandSource::Vscode => "vscode",
            CommandSource::PackageJson => "package.json",
            CommandSource::Composer => "composer",
            CommandSource::Just => "just",
            CommandSource::Make => "make",
            CommandSource::Cargo => "cargo",
            CommandSource::Go => "go",
            CommandSource::Maven => "maven",
            CommandSource::Gradle => "gradle",
            CommandSource::Dotnet => "dotnet",
            CommandSource::Python => "python",
            CommandSource::Php => "php",
            CommandSource::Ruby => "ruby",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CommandConfidence {
    Explicit,
    NativeDefault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectCommand {
    pub id: String,
    pub location: crate::execution_host::ResourceLocation,
    pub source: CommandSource,
    pub name: String,
    pub command: String,
    pub confidence: CommandConfidence,
}

impl ProjectCommand {
    pub(crate) fn root(&self) -> &Path {
        self.location.path.as_path()
    }

    pub(crate) fn new(
        location: crate::execution_host::ResourceLocation,
        source: CommandSource,
        name: impl Into<String>,
        command_text: impl Into<String>,
        confidence: CommandConfidence,
    ) -> Self {
        command(&location, source, name, command_text, confidence)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandRunStatus {
    Running,
    Stopped,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandRun {
    pub command_id: String,
    pub execution_host_id: crate::execution_host::ExecutionHostId,
    pub terminal_id: crate::terminal::TerminalId,
    pub status: CommandRunStatus,
}

pub(crate) fn project_root_from_cwd(cwd: &Path) -> PathBuf {
    let mut current = if cwd.is_dir() {
        cwd.to_path_buf()
    } else {
        cwd.parent().unwrap_or(cwd).to_path_buf()
    };

    loop {
        if has_project_marker(&current) {
            return current;
        }
        if !current.pop() {
            return cwd.to_path_buf();
        }
    }
}

pub(crate) fn discover_project_commands(root: &Path) -> Vec<ProjectCommand> {
    let Ok(location) = crate::execution_host::ResourceLocation::local(root.to_path_buf()) else {
        return Vec::new();
    };
    discover_project_commands_at(&location)
}

pub(crate) fn discover_project_commands_at(
    location: &crate::execution_host::ResourceLocation,
) -> Vec<ProjectCommand> {
    let root = location.path.as_path();
    let mut commands = Vec::new();
    commands.extend(vscode_tasks(root, location));
    commands.extend(package_json_scripts(root, location));
    commands.extend(composer_scripts(root, location));
    commands.extend(just_recipes(root, location));
    commands.extend(make_targets(root, location));
    commands.extend(native_defaults(root, location));
    dedupe_commands(commands)
}

pub(crate) fn project_command_from_snapshot(
    snapshot: crate::execution_host::protocol::ProjectCommandSnapshot,
) -> Option<ProjectCommand> {
    use crate::execution_host::protocol::{ProjectCommandConfidence, ProjectCommandSource};
    let source = match snapshot.source {
        ProjectCommandSource::BuiltIn => CommandSource::BuiltIn,
        ProjectCommandSource::Vscode => CommandSource::Vscode,
        ProjectCommandSource::PackageJson => CommandSource::PackageJson,
        ProjectCommandSource::Composer => CommandSource::Composer,
        ProjectCommandSource::Just => CommandSource::Just,
        ProjectCommandSource::Make => CommandSource::Make,
        ProjectCommandSource::Cargo => CommandSource::Cargo,
        ProjectCommandSource::Go => CommandSource::Go,
        ProjectCommandSource::Maven => CommandSource::Maven,
        ProjectCommandSource::Gradle => CommandSource::Gradle,
        ProjectCommandSource::Dotnet => CommandSource::Dotnet,
        ProjectCommandSource::Python => CommandSource::Python,
        ProjectCommandSource::Php => CommandSource::Php,
        ProjectCommandSource::Ruby => CommandSource::Ruby,
    };
    let confidence = match snapshot.confidence {
        ProjectCommandConfidence::Explicit => CommandConfidence::Explicit,
        ProjectCommandConfidence::NativeDefault => CommandConfidence::NativeDefault,
    };
    Some(ProjectCommand::new(
        snapshot.location,
        source,
        snapshot.name,
        snapshot.command,
        confidence,
    ))
}

pub(crate) fn project_command_to_snapshot(
    command: &ProjectCommand,
) -> crate::execution_host::protocol::ProjectCommandSnapshot {
    use crate::execution_host::protocol::{ProjectCommandConfidence, ProjectCommandSource};
    let source = match command.source {
        CommandSource::BuiltIn => ProjectCommandSource::BuiltIn,
        CommandSource::Vscode => ProjectCommandSource::Vscode,
        CommandSource::PackageJson => ProjectCommandSource::PackageJson,
        CommandSource::Composer => ProjectCommandSource::Composer,
        CommandSource::Just => ProjectCommandSource::Just,
        CommandSource::Make => ProjectCommandSource::Make,
        CommandSource::Cargo => ProjectCommandSource::Cargo,
        CommandSource::Go => ProjectCommandSource::Go,
        CommandSource::Maven => ProjectCommandSource::Maven,
        CommandSource::Gradle => ProjectCommandSource::Gradle,
        CommandSource::Dotnet => ProjectCommandSource::Dotnet,
        CommandSource::Python => ProjectCommandSource::Python,
        CommandSource::Php => ProjectCommandSource::Php,
        CommandSource::Ruby => ProjectCommandSource::Ruby,
    };
    let confidence = match command.confidence {
        CommandConfidence::Explicit => ProjectCommandConfidence::Explicit,
        CommandConfidence::NativeDefault => ProjectCommandConfidence::NativeDefault,
    };
    crate::execution_host::protocol::ProjectCommandSnapshot {
        location: command.location.clone(),
        source,
        name: command.name.clone(),
        command: command.command.clone(),
        confidence,
    }
}

fn command(
    location: &crate::execution_host::ResourceLocation,
    source: CommandSource,
    name: impl Into<String>,
    command: impl Into<String>,
    confidence: CommandConfidence,
) -> ProjectCommand {
    let name = name.into();
    let command = command.into();
    let id = format!(
        "{}:{}:{}:{}:{}",
        location.execution_host_id,
        location.path.as_path().display(),
        source.label(),
        name,
        command
    );
    ProjectCommand {
        id,
        location: location.clone(),
        source,
        name,
        command,
        confidence,
    }
}

fn has_project_marker(path: &Path) -> bool {
    [
        ".git",
        ".mise.toml",
        "mise.toml",
        "justfile",
        "Justfile",
        "Taskfile.yml",
        "Taskfile.yaml",
        "package.json",
        "composer.json",
        "Makefile",
        "Cargo.toml",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "pyproject.toml",
        "Gemfile",
        "Rakefile",
        "mix.exs",
    ]
    .iter()
    .any(|marker| path.join(marker).exists())
}

fn read_to_string(path: impl AsRef<Path>) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn vscode_tasks(
    root: &Path,
    location: &crate::execution_host::ResourceLocation,
) -> Vec<ProjectCommand> {
    let Some(text) = read_to_string(root.join(".vscode/tasks.json")) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(tasks) = json.get("tasks").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };

    tasks
        .iter()
        .filter_map(|task| {
            let label = task.get("label")?.as_str()?.trim();
            let command_text = task.get("command")?.as_str()?.trim();
            if label.is_empty() || command_text.is_empty() {
                return None;
            }
            let args = task
                .get("args")
                .and_then(serde_json::Value::as_array)
                .map(|args| {
                    args.iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .filter(|args| !args.is_empty());
            let run = args.map_or_else(
                || command_text.to_string(),
                |args| format!("{command_text} {args}"),
            );
            Some(command(
                location,
                CommandSource::Vscode,
                label,
                run,
                CommandConfidence::Explicit,
            ))
        })
        .collect()
}

fn package_json_scripts(
    root: &Path,
    location: &crate::execution_host::ResourceLocation,
) -> Vec<ProjectCommand> {
    let Some(text) = read_to_string(root.join("package.json")) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = json.get("scripts").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };

    sorted_script_names(scripts)
        .into_iter()
        .filter(|name| script_is_user_facing(name))
        .map(|name| {
            command(
                location,
                CommandSource::PackageJson,
                name,
                format!("npm run {name}"),
                CommandConfidence::Explicit,
            )
        })
        .collect()
}

fn composer_scripts(
    root: &Path,
    location: &crate::execution_host::ResourceLocation,
) -> Vec<ProjectCommand> {
    let Some(text) = read_to_string(root.join("composer.json")) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = json.get("scripts").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };

    sorted_script_names(scripts)
        .into_iter()
        .filter(|name| script_is_user_facing(name))
        .map(|name| {
            command(
                location,
                CommandSource::Composer,
                name,
                format!("composer {name}"),
                CommandConfidence::Explicit,
            )
        })
        .collect()
}

fn sorted_script_names(scripts: &serde_json::Map<String, serde_json::Value>) -> Vec<&str> {
    let mut names = scripts.keys().map(String::as_str).collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn script_is_user_facing(name: &str) -> bool {
    !name.starts_with('_') && !name.starts_with("pre") && !name.starts_with("post")
}

fn just_recipes(
    root: &Path,
    location: &crate::execution_host::ResourceLocation,
) -> Vec<ProjectCommand> {
    let path = ["justfile", "Justfile"]
        .iter()
        .map(|name| root.join(name))
        .find(|path| path.exists());
    let Some(path) = path else {
        return Vec::new();
    };
    let Some(text) = read_to_string(path) else {
        return Vec::new();
    };

    text.lines()
        .filter_map(parse_just_recipe)
        .map(|name| {
            command(
                location,
                CommandSource::Just,
                name,
                format!("just {name}"),
                CommandConfidence::Explicit,
            )
        })
        .collect()
}

fn parse_just_recipe(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with('@') || line.contains(":=") {
        return None;
    }
    let (name, _) = line.split_once(':')?;
    let name = name.split_whitespace().next()?.trim();
    valid_task_name(name).then_some(name)
}

fn make_targets(
    root: &Path,
    location: &crate::execution_host::ResourceLocation,
) -> Vec<ProjectCommand> {
    let Some(text) = read_to_string(root.join("Makefile")) else {
        return Vec::new();
    };
    let phony = parse_phony_targets(&text);
    let common = [
        "dev", "run", "serve", "test", "build", "lint", "format", "clean",
    ];
    let mut targets = text
        .lines()
        .filter_map(parse_make_target)
        .filter(|name| phony.contains(*name) || common.contains(name))
        .collect::<Vec<_>>();
    targets.sort_unstable();
    targets.dedup();

    targets
        .into_iter()
        .map(|name| {
            command(
                location,
                CommandSource::Make,
                name,
                format!("make {name}"),
                CommandConfidence::Explicit,
            )
        })
        .collect()
}

fn parse_phony_targets(text: &str) -> HashSet<&str> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix(".PHONY:"))
        .flat_map(str::split_whitespace)
        .collect()
}

fn parse_make_target(line: &str) -> Option<&str> {
    let line = line.trim_end();
    if line.starts_with('#') || line.starts_with('\t') || line.starts_with('.') {
        return None;
    }
    let (name, rest) = line.split_once(':')?;
    if rest.starts_with('=') || name.contains('$') || name.contains('%') {
        return None;
    }
    let name = name.trim();
    valid_task_name(name).then_some(name)
}

fn valid_task_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn native_defaults(
    root: &Path,
    location: &crate::execution_host::ResourceLocation,
) -> Vec<ProjectCommand> {
    let mut commands = Vec::new();
    if root.join("Cargo.toml").exists() {
        commands.push(native(
            location,
            CommandSource::Cargo,
            "build",
            "cargo build",
        ));
        commands.push(native(location, CommandSource::Cargo, "test", "cargo test"));
        if root.join("src/main.rs").exists() {
            commands.push(native(location, CommandSource::Cargo, "run", "cargo run"));
        }
    }
    if root.join("go.mod").exists() {
        commands.push(native(location, CommandSource::Go, "test", "go test ./..."));
        commands.push(native(
            location,
            CommandSource::Go,
            "build",
            "go build ./...",
        ));
        if root.join("main.go").exists() {
            commands.push(native(location, CommandSource::Go, "run", "go run ."));
        }
    }
    if root.join("pom.xml").exists() {
        let mvn = if root.join("mvnw").exists() {
            "./mvnw"
        } else {
            "mvn"
        };
        commands.push(native(
            location,
            CommandSource::Maven,
            "test",
            format!("{mvn} test"),
        ));
        commands.push(native(
            location,
            CommandSource::Maven,
            "package",
            format!("{mvn} package"),
        ));
        commands.push(native(
            location,
            CommandSource::Maven,
            "verify",
            format!("{mvn} verify"),
        ));
        if read_to_string(root.join("pom.xml")).is_some_and(|text| text.contains("spring-boot")) {
            commands.push(native(
                location,
                CommandSource::Maven,
                "spring-boot:run",
                format!("{mvn} spring-boot:run"),
            ));
        }
    }
    if root.join("build.gradle").exists() || root.join("build.gradle.kts").exists() {
        let gradle = if root.join("gradlew").exists() {
            "./gradlew"
        } else {
            "gradle"
        };
        let gradle_text = read_to_string(root.join("build.gradle"))
            .or_else(|| read_to_string(root.join("build.gradle.kts")))
            .unwrap_or_default();
        commands.push(native(
            location,
            CommandSource::Gradle,
            "test",
            format!("{gradle} test"),
        ));
        commands.push(native(
            location,
            CommandSource::Gradle,
            "build",
            format!("{gradle} build"),
        ));
        if gradle_text.contains("org.springframework.boot") {
            commands.push(native(
                location,
                CommandSource::Gradle,
                "bootRun",
                format!("{gradle} bootRun"),
            ));
        }
        if gradle_text.contains("application") {
            commands.push(native(
                location,
                CommandSource::Gradle,
                "run",
                format!("{gradle} run"),
            ));
        }
    }
    if root.join("pyproject.toml").exists()
        && (root.join("tests").exists()
            || read_to_string(root.join("pyproject.toml"))
                .is_some_and(|text| text.contains("pytest")))
    {
        commands.push(native(
            location,
            CommandSource::Python,
            "test",
            "python -m pytest",
        ));
    }
    if has_dotnet_project(root) {
        commands.push(native(
            location,
            CommandSource::Dotnet,
            "build",
            "dotnet build",
        ));
        commands.push(native(
            location,
            CommandSource::Dotnet,
            "test",
            "dotnet test",
        ));
    }
    if root.join("artisan").exists() {
        commands.push(native(
            location,
            CommandSource::Php,
            "serve",
            "php artisan serve",
        ));
    }
    if root.join("Gemfile").exists() || root.join("Rakefile").exists() {
        if root.join("spec").exists() {
            commands.push(native(
                location,
                CommandSource::Ruby,
                "spec",
                "bundle exec rspec",
            ));
        }
        if root.join("Rakefile").exists() {
            commands.push(native(
                location,
                CommandSource::Ruby,
                "rake",
                "bundle exec rake",
            ));
        }
    }
    commands
}

fn has_dotnet_project(root: &Path) -> bool {
    std::fs::read_dir(root).ok().is_some_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.path().extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("csproj") || ext.eq_ignore_ascii_case("sln")
            })
        })
    })
}

fn native(
    location: &crate::execution_host::ResourceLocation,
    source: CommandSource,
    name: impl Into<String>,
    run: impl Into<String>,
) -> ProjectCommand {
    command(
        location,
        source,
        name,
        run,
        CommandConfidence::NativeDefault,
    )
}

fn dedupe_commands(commands: Vec<ProjectCommand>) -> Vec<ProjectCommand> {
    let mut by_key = BTreeMap::new();
    for command in commands {
        by_key
            .entry((
                command.location.execution_host_id.as_str().to_string(),
                command.location.path.as_path().to_path_buf(),
                command.source,
                command.name.clone(),
                command.command.clone(),
            ))
            .or_insert(command);
    }
    by_key.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "omh-commands-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, text).unwrap();
    }

    fn names(commands: &[ProjectCommand]) -> Vec<String> {
        commands
            .iter()
            .map(|command| command.name.clone())
            .collect()
    }

    #[test]
    fn package_json_discovers_user_scripts_without_lifecycle_noise() {
        let root = temp_root("package-json");
        write(
            &root,
            "package.json",
            r#"{"scripts":{"dev":"vite","predev":"x","posttest":"x","_hidden":"x","test":"vitest"}}"#,
        );

        let commands = discover_project_commands(&root);

        assert_eq!(names(&commands), vec!["dev", "test"]);
        assert_eq!(commands[0].command, "npm run dev");
    }

    #[test]
    fn vscode_tasks_are_explicit_commands() {
        let root = temp_root("vscode");
        write(
            &root,
            ".vscode/tasks.json",
            r#"{"tasks":[{"label":"serve","type":"shell","command":"bin/server","args":["--port","3000"]}]}"#,
        );

        let commands = discover_project_commands(&root);

        assert_eq!(commands[0].source, CommandSource::Vscode);
        assert_eq!(commands[0].name, "serve");
        assert_eq!(commands[0].command, "bin/server --port 3000");
    }

    #[test]
    fn just_and_make_discovers_explicit_tasks() {
        let root = temp_root("just-make");
        write(
            &root,
            "justfile",
            "dev:\n    npm run dev\n_private:\n    true\n",
        );
        write(
            &root,
            "Makefile",
            ".PHONY: deploy\ndeploy:\n\ttrue\ninternal:\n\ttrue\n",
        );

        let commands = discover_project_commands(&root);

        assert!(commands
            .iter()
            .any(|command| command.source == CommandSource::Just && command.name == "dev"));
        assert!(commands
            .iter()
            .any(|command| command.source == CommandSource::Make && command.name == "deploy"));
        assert!(!commands.iter().any(|command| command.name == "internal"));
    }

    #[test]
    fn native_defaults_cover_non_js_repos_conservatively() {
        let root = temp_root("native");
        write(
            &root,
            "pom.xml",
            "<project><artifactId>spring-boot-starter-web</artifactId></project>",
        );
        write(&root, "mvnw", "#!/bin/sh\n");
        write(&root, "go.mod", "module example.com/api\n");
        write(&root, "main.go", "package main\n");

        let commands = discover_project_commands(&root);

        assert!(commands
            .iter()
            .any(|command| command.name == "spring-boot:run"
                && command.command == "./mvnw spring-boot:run"));
        assert!(commands
            .iter()
            .any(|command| command.name == "run" && command.command == "go run ."));
    }

    #[test]
    fn command_identity_separates_equal_paths_on_different_hosts() {
        let path = PathBuf::from("/srv/project");
        let local = ProjectCommand::new(
            crate::execution_host::ResourceLocation::local(path.clone()).unwrap(),
            CommandSource::PackageJson,
            "dev",
            "npm run dev",
            CommandConfidence::Explicit,
        );
        let remote = ProjectCommand::new(
            crate::execution_host::ResourceLocation::new(
                crate::execution_host::ExecutionHostId::new("ssh:workbox").unwrap(),
                crate::execution_host::HostPath::new(path).unwrap(),
            ),
            CommandSource::PackageJson,
            "dev",
            "npm run dev",
            CommandConfidence::Explicit,
        );

        assert_ne!(local.id, remote.id);
        assert_eq!(local.root(), remote.root());
    }

    #[test]
    fn project_root_walks_up_to_marker() {
        let root = temp_root("root");
        write(&root, "Cargo.toml", "[package]\nname = \"x\"\n");
        let nested = root.join("src/bin");
        std::fs::create_dir_all(&nested).unwrap();

        assert_eq!(project_root_from_cwd(&nested), root);
    }
}
