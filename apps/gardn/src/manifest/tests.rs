use super::*;

fn remote_manifest(version: &str, state: &str, contains: &str) -> String {
    format!(
        r#"
id = "codex"
version = "{version}"
min_engine_version = 1
updated_at = "2026-06-10T12:00:00Z"

[[rules]]
id = "test"
state = "{state}"
contains = ["{contains}"]
"#
    )
}

fn local_manifest(state: &str, contains: &str) -> String {
    format!(
        r#"
id = "codex"

[[rules]]
id = "test"
state = "{state}"
contains = ["{contains}"]
"#
    )
}

fn rules_manifest(rules: &str) -> String {
    format!(
        r#"
id = "codex"

{rules}
"#
    )
}

fn with_manifest_dirs<T>(name: &str, f: impl FnOnce() -> T) -> T {
    let _guard = crate::config::test_config_env_lock().lock().unwrap();
    let old_config = std::env::var_os("XDG_CONFIG_HOME");
    let old_state = std::env::var_os("XDG_STATE_HOME");
    let base = std::env::temp_dir().join(format!(
        "gardn-manifest-loader-{name}-{}",
        std::process::id()
    ));
    let config_dir = base.join("config");
    let state_dir = base.join("state");
    let _ = std::fs::remove_dir_all(&base);
    std::env::set_var("XDG_CONFIG_HOME", &config_dir);
    std::env::set_var("XDG_STATE_HOME", &state_dir);
    reload_manifests();
    let result = f();
    match old_config {
        Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
    match old_state {
        Some(value) => std::env::set_var("XDG_STATE_HOME", value),
        None => std::env::remove_var("XDG_STATE_HOME"),
    }
    reload_manifests();
    let _ = std::fs::remove_dir_all(&base);
    result
}

fn write_remote_codex(content: &str) {
    let path = crate::detect::manifest_update::remote_manifest_path(Agent::Codex);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
    reload_manifests();
}

fn write_remote_codex_without_reload(content: &str) {
    let path = crate::detect::manifest_update::remote_manifest_path(Agent::Codex);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn write_local_codex(content: &str) {
    let path = override_path(Agent::Codex).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
    reload_manifests();
}

#[test]
fn known_agent_no_match_defaults_to_idle_fallback() {
    let explain = explain(Agent::Codex, "ordinary prompt text");

    assert_eq!(explain.state, AgentState::Idle);
    assert!(!explain.visible_idle);
    assert_eq!(
        explain.fallback_reason.as_deref(),
        Some(DEFAULT_KNOWN_AGENT_IDLE_FALLBACK)
    );
}

#[test]
fn pi_omp_manifests_detect_core_resume_states() {
    for agent in [Agent::Pi, Agent::OhMyPi] {
        let approval = explain(
            agent,
            "Allow tool: bash\nReason: approval required\nApprove\nDeny",
        );
        assert_eq!(approval.state, AgentState::Blocked);
        assert!(approval.visible_blocker);

        let selector =
            "Plan mode\n❯ Approve and execute\n  Refine plan\nup/down navigate  enter select  esc cancel";
        let blocked = explain(agent, selector);
        assert_eq!(blocked.state, AgentState::Blocked);
        assert!(blocked.visible_blocker);
    }

    let maintenance = explain(Agent::OhMyPi, "Auto context-full maintenance…");
    assert_eq!(maintenance.state, AgentState::Working);
    assert!(maintenance.visible_working);
}

#[test]
fn devin_manifest_detects_working_blocked_and_idle_states() {
    let blocked = explain(
        Agent::Devin,
        "Do you trust the authors of this directory?\nwith untrusted content.\nyes, trust",
    );
    assert_eq!(blocked.state, AgentState::Blocked);
    assert!(blocked.visible_blocker);

    let working = explain(Agent::Devin, "running tools\nesc to interrupt");
    assert_eq!(working.state, AgentState::Working);
    assert!(working.visible_working);

    let idle = explain(Agent::Devin, "context: 12%\n❭ Ask Devin to build");
    assert_eq!(idle.state, AgentState::Idle);
    assert!(idle.visible_idle);
}

#[test]
fn omp_manifest_does_not_hold_working_from_stale_maintenance_scrollback() {
    let idle_after_maintenance = explain(
        Agent::OhMyPi,
        "Auto context-full maintenance…\n\n──────────────────────────── old ──\n❯ ",
    );

    assert_eq!(idle_after_maintenance.state, AgentState::Idle);
    assert!(!idle_after_maintenance.visible_working);
}

#[test]
fn omp_manifest_detects_live_working_after_prompt_line() {
    let maintenance_after_prompt = explain(
        Agent::OhMyPi,
        "❯ build the thing\nAuto context-full maintenance…",
    );

    assert_eq!(maintenance_after_prompt.state, AgentState::Working);
    assert!(maintenance_after_prompt.visible_working);
}

#[test]
fn claude_manifest_detects_compacting_and_live_working_chrome() {
    let compacting = explain(
        Agent::Claude,
        "· Compacting conversation…\n  ▰▰▰▱▱▱▱▱▱▱ 7%\n  ⎿  Tip: Use /permissions",
    );
    assert_eq!(compacting.state, AgentState::Working);
    assert!(compacting.visible_working);

    let live_spinner = explain(
        Agent::Claude,
        "● Sourcing e2e build… (42s · ↓ 1.2k tokens)\n\n──────────────────────────── task ──",
    );
    assert_eq!(live_spinner.state, AgentState::Working);
    assert!(live_spinner.visible_working);
}

#[test]
fn claude_manifest_detects_live_working_after_prompt_line() {
    let spinner_after_prompt = explain(
        Agent::Claude,
        "❯ build the thing\n● Sourcing e2e build… (42s · ↓ 1.2k tokens)",
    );

    assert_eq!(spinner_after_prompt.state, AgentState::Working);
    assert!(spinner_after_prompt.visible_working);
}

#[test]
fn claude_manifest_does_not_hold_working_from_stale_spinner_scrollback() {
    let idle_after_spinner = explain(
        Agent::Claude,
        "● Sourcing e2e build… (42s · ↓ 1.2k tokens)\n\n──────────────────────────── task ──\n❯ ",
    );

    assert_eq!(idle_after_spinner.state, AgentState::Idle);
    assert!(!idle_after_spinner.visible_working);
}

#[test]
fn claude_enter_to_confirm_is_blocked() {
    let result = explain(
        Agent::Claude,
        "Apply this change?\nEnter to confirm · Esc to cancel",
    );
    assert_eq!(result.state, AgentState::Blocked);
    assert_eq!(
        result.matched_rule.as_ref().map(|rule| rule.id.as_str()),
        Some("live_blocked_form")
    );
    assert!(result.visible_blocker);
}

#[test]
fn claude_osc_title_half_circle_frames_are_working() {
    for frame in ['◐', '◓', '◑', '◒'] {
        let title = format!("{frame} Initial conversation with Claude");
        let result = explain_with_input(
            Agent::Claude,
            DetectionInput {
                screen: "",
                osc_title: &title,
                osc_progress: "",
            },
        );
        assert_eq!(result.state, AgentState::Working, "frame {frame}");
        assert_eq!(
            result.matched_rule.as_ref().map(|rule| rule.id.as_str()),
            Some("osc_title_working"),
            "frame {frame}"
        );
        assert!(!result.visible_working, "frame {frame}");
    }
}

#[test]
fn kiro_prompt_idle_requires_positive_idle_evidence() {
    let idle = explain(
        Agent::Kiro,
        "Ask a question or describe a task\n/copy to clipboard",
    );
    assert_eq!(idle.state, AgentState::Idle);
    assert_eq!(
        idle.matched_rule.as_ref().map(|rule| rule.id.as_str()),
        Some("prompt_idle")
    );
    assert!(idle.visible_idle);

    let working = explain(
        Agent::Kiro,
        "Ask a question or describe a task\n/copy to clipboard\nKiro is working",
    );
    assert_eq!(working.state, AgentState::Working);
    assert_ne!(
        working.matched_rule.as_ref().map(|rule| rule.id.as_str()),
        Some("prompt_idle")
    );
}

#[test]
fn cursor_run_everything_status_is_not_blocked() {
    let result = explain(Agent::Cursor, "Run Everything\nctrl+c to stop");
    assert_ne!(result.state, AgentState::Blocked);
    assert!(!result.visible_blocker);
}

#[test]
fn osc_only_matches_do_not_claim_visible_screen_evidence() {
    with_manifest_dirs("osc-not-visible", || {
        write_local_codex(&rules_manifest(
            r#"
[[rules]]
id = "osc_working"
state = "working"
priority = 10
region = "osc_title"
visible_working = true
contains = ["working"]
"#,
        ));

        let explain = explain_with_input(
            Agent::Codex,
            DetectionInput {
                screen: "",
                osc_title: "Working on it",
                osc_progress: "",
            },
        );

        assert_eq!(explain.state, AgentState::Working);
        assert_eq!(
            explain.matched_rule.as_ref().map(|rule| rule.id.as_str()),
            Some("osc_working")
        );
        assert!(!explain.visible_working);
    });
}

#[test]
fn rule_semantics_apply_gates_priority_and_line_regex() {
    with_manifest_dirs("rule-semantics", || {
        write_local_codex(&rules_manifest(
            r#"
[[rules]]
id = "low_contains"
state = "idle"
priority = 1
contains = ["match"]

[[rules]]
id = "high_nested_gates"
state = "working"
priority = 10
contains = ["match"]
all = [
  { any = [{ regex = ["w[io]n"] }, { contains = ["fallback"] }] },
]
not = [
  { contains = ["blocked"] },
]

[[rules]]
id = "line_regex"
state = "blocked"
priority = 20
line_regex = ["^exact line$"]
"#,
        ));

        let high = explain(Agent::Codex, "match win");
        assert_eq!(high.state, AgentState::Working);
        assert_eq!(
            high.matched_rule.as_ref().map(|rule| rule.id.as_str()),
            Some("high_nested_gates")
        );

        let not_gate = explain(Agent::Codex, "match win blocked");
        assert_eq!(not_gate.state, AgentState::Idle);
        assert_eq!(
            not_gate.matched_rule.as_ref().map(|rule| rule.id.as_str()),
            Some("low_contains")
        );

        let line = explain(Agent::Codex, "before\nexact line\nafter");
        assert_eq!(line.state, AgentState::Blocked);
        assert_eq!(
            line.matched_rule.as_ref().map(|rule| rule.id.as_str()),
            Some("line_regex")
        );
    });
}

#[test]
fn remote_manifest_loads_between_local_override_and_bundled() {
    with_manifest_dirs("remote-source", || {
        write_remote_codex(&remote_manifest("9999.01.01.1", "blocked", "remote-ready"));

        let explain = explain(Agent::Codex, "remote-ready");

        assert_eq!(explain.state, AgentState::Blocked);
        assert!(matches!(
            explain.source,
            Some(ManifestSource::Remote { .. })
        ));
        assert_eq!(explain.manifest_version.as_deref(), Some("9999.01.01.1"));
        assert_eq!(
            explain.cached_remote_version.as_deref(),
            Some("9999.01.01.1")
        );
    });
}

#[test]
fn fallback_explain_preserves_active_manifest_version() {
    with_manifest_dirs("fallback-version", || {
        write_remote_codex(&remote_manifest("9999.01.01.1", "blocked", "remote-ready"));

        let explain = explain(Agent::Codex, "ordinary prompt text");

        assert_eq!(explain.state, AgentState::Idle);
        assert_eq!(
            explain.fallback_reason.as_deref(),
            Some(DEFAULT_KNOWN_AGENT_IDLE_FALLBACK)
        );
        assert_eq!(explain.manifest_version.as_deref(), Some("9999.01.01.1"));
        assert!(matches!(
            explain.source,
            Some(ManifestSource::Remote { .. })
        ));
    });
}

#[test]
fn older_cached_remote_manifest_does_not_shadow_newer_bundled_manifest() {
    with_manifest_dirs("older-remote-bundled-fallback", || {
        write_remote_codex(&remote_manifest("2026.06.10.0", "blocked", "remote-ready"));

        let explain = explain(Agent::Codex, "remote-ready");

        assert_eq!(explain.state, AgentState::Idle);
        assert!(matches!(explain.source, Some(ManifestSource::Bundled)));
        assert_eq!(
            explain.cached_remote_version.as_deref(),
            Some("2026.06.10.0")
        );
        assert!(explain
            .warning
            .as_deref()
            .is_some_and(|warning| warning.contains("older than bundled")));
    });
}

#[test]
fn local_override_shadows_cached_remote_manifest() {
    with_manifest_dirs("local-shadows-remote", || {
        write_remote_codex(&remote_manifest("9999.01.01.1", "blocked", "remote-ready"));
        write_local_codex(&local_manifest("idle", "local-ready"));

        let explain = explain(Agent::Codex, "local-ready");

        assert_eq!(explain.state, AgentState::Idle);
        assert!(matches!(explain.source, Some(ManifestSource::Override(_))));
        assert!(explain.local_override_shadowing_remote);
        assert_eq!(
            explain.cached_remote_version.as_deref(),
            Some("9999.01.01.1")
        );
    });
}

#[test]
fn invalid_local_override_falls_back_to_cached_remote_manifest() {
    with_manifest_dirs("invalid-local-remote-fallback", || {
        write_remote_codex(&remote_manifest("9999.01.01.1", "blocked", "remote-ready"));
        write_local_codex("id = ");

        let explain = explain(Agent::Codex, "remote-ready");

        assert_eq!(explain.state, AgentState::Blocked);
        assert!(matches!(
            explain.source,
            Some(ManifestSource::Remote { .. })
        ));
        assert!(explain.warning.is_some());
    });
}

#[test]
fn detection_uses_cached_manifest_until_explicit_reload() {
    with_manifest_dirs("cache-boundary", || {
        write_remote_codex(&remote_manifest("9999.01.01.1", "blocked", "cached-ready"));

        let cached = explain(Agent::Codex, "cached-ready");
        assert_eq!(cached.state, AgentState::Blocked);
        assert!(matches!(cached.source, Some(ManifestSource::Remote { .. })));
        assert_eq!(
            cached.matched_rule.as_ref().map(|rule| rule.id.as_str()),
            Some("test")
        );

        write_remote_codex_without_reload(&remote_manifest("9999.01.01.2", "working", "new-ready"));

        let unchanged = explain(Agent::Codex, "new-ready");
        assert_eq!(unchanged.state, AgentState::Idle);
        assert_eq!(
            unchanged.fallback_reason.as_deref(),
            Some(DEFAULT_KNOWN_AGENT_IDLE_FALLBACK)
        );
        assert_eq!(
            unchanged.cached_remote_version.as_deref(),
            Some("9999.01.01.1")
        );

        reload_manifests();

        let reloaded = explain(Agent::Codex, "new-ready");
        assert_eq!(reloaded.state, AgentState::Working);
        assert_eq!(
            reloaded.cached_remote_version.as_deref(),
            Some("9999.01.01.2")
        );
        assert_eq!(
            reloaded.matched_rule.as_ref().map(|rule| rule.id.as_str()),
            Some("test")
        );
    });
}

#[test]
fn bundled_manifest_coverage_matches_agent_contract() {
    for agent in Agent::ALL {
        if agent == Agent::Mastracode {
            assert!(bundled_manifest(agent).is_none());
        } else {
            assert!(
                bundled_manifest(agent).is_some(),
                "missing bundled manifest for {}",
                agent_label(agent)
            );
        }
    }
}

#[test]
fn manifest_validation_rejects_unknown_fields_empty_rules_invalid_regions_and_regexes() {
    assert!(parse_manifest(
        r#"
id = "codex"

[[rules]]
id = "typo"
state = "working"
contain = ["Working"]
"#
    )
    .is_err());

    assert!(parse_manifest(
        r#"
id = "codex"

[[rules]]
id = "empty"
state = "working"
"#
    )
    .is_err());

    assert!(parse_manifest(
        r#"
id = "codex"

[[rules]]
id = "bad_region"
state = "working"
region = "after_last_promt_marker"
contains = ["Working"]
"#
    )
    .is_err());

    assert!(parse_manifest(
        r#"
id = "codex"

[[rules]]
id = "bad_regex"
state = "working"
regex = ["["]
"#
    )
    .is_err());

    assert!(parse_manifest(
        r#"
id = "codex"

[[rules]]
id = "bad_nested_regex"
state = "working"
any = [{ line_regex = ["["] }]
"#
    )
    .is_err());
}

#[test]
fn manifest_validation_keeps_skip_rules_neutral() {
    assert!(parse_manifest(
        r#"
id = "codex"

[[rules]]
id = "bad_skip_state"
state = "idle"
skip_state_update = true
contains = ["menu"]
"#
    )
    .is_err());

    assert!(parse_manifest(
        r#"
id = "codex"

[[rules]]
id = "bad_skip_visible"
state = "unknown"
skip_state_update = true
visible_blocker = true
contains = ["menu"]
"#
    )
    .is_err());
}

#[test]
fn manifest_validation_rejects_excessive_rule_count() {
    let mut manifest = String::from(
        r#"
id = "codex"
"#,
    );
    for index in 0..129 {
        manifest.push_str(&format!(
            r#"
[[rules]]
id = "rule_{index}"
state = "idle"
contains = ["ready"]
"#
        ));
    }

    assert!(parse_manifest(&manifest).is_err());
}

#[test]
fn manifest_validation_rejects_excessive_gate_depth() {
    let manifest = r#"
id = "codex"

[[rules]]
id = "deep"
state = "idle"
contains = ["ready"]
all = [
  { contains = ["1"], all = [
    { contains = ["2"], all = [
      { contains = ["3"], all = [
        { contains = ["4"], all = [
          { contains = ["5"], all = [
            { contains = ["6"], all = [
              { contains = ["7"], all = [
                { contains = ["8"], all = [
                  { contains = ["9"] },
                ] },
              ] },
            ] },
          ] },
        ] },
      ] },
    ] },
  ] },
]
"#;

    assert!(parse_manifest(manifest).is_err());
}

#[test]
fn manifest_validation_rejects_excessive_matchers() {
    let matchers = (0..33)
        .map(|index| format!(r#""m{index}""#))
        .collect::<Vec<_>>()
        .join(", ");
    let manifest = format!(
        r#"
id = "codex"

[[rules]]
id = "many"
state = "idle"
contains = [{matchers}]
"#
    );

    assert!(parse_manifest(&manifest).is_err());
}

#[test]
fn bottom_non_empty_lines_uses_bottom_occurrence_for_repeated_text() {
    let content = "marker\nold\n\nmiddle\nmarker\nnew\n";

    assert_eq!(
        region(
            DetectionInput {
                screen: content,
                osc_title: "",
                osc_progress: "",
            },
            "bottom_non_empty_lines(2)",
        ),
        "marker\nnew\n"
    );
}

#[test]
fn copilot_accept_and_interrupt_chrome_are_classified() {
    let blocked = explain(
        Agent::GithubCopilot,
        "Do you want to proceed?\nesc cancel\nenter accept",
    );
    assert_eq!(blocked.state, AgentState::Blocked);
    assert!(blocked.visible_blocker);
    assert_eq!(
        blocked.matched_rule.as_ref().map(|rule| rule.id.as_str()),
        Some("selection_blocker")
    );

    let working = explain(Agent::GithubCopilot, "esc interrupt");
    assert_eq!(working.state, AgentState::Working);
    assert!(working.visible_working);
}

#[test]
fn antigravity_background_task_count_without_tasks_hint_is_working() {
    let working = explain(Agent::Antigravity, "· 2 tasks");

    assert_eq!(working.state, AgentState::Working);
    assert!(working.visible_working);
    assert_eq!(
        working.matched_rule.as_ref().map(|rule| rule.id.as_str()),
        Some("background_tasks_working")
    );
}
#[test]
fn qwen_manifest_distinguishes_blocked_working_and_idle_evidence() {
    let blocked_title = explain_with_input(
        Agent::Qwen,
        DetectionInput {
            screen: "",
            osc_title: "✳ Confirm tool",
            osc_progress: "",
        },
    );
    assert_eq!(blocked_title.state, AgentState::Blocked);

    for screen in [
        "⠏ Waiting for user confirmation...",
        "allow execution of: rm -rf tmp\nyes, allow once",
        "❯ 1. Continue\n↑/↓: Select  Enter: Confirm",
        "Do you trust this folder?\ntrust folder (y)\ndon't trust (esc)",
    ] {
        let blocked = explain(Agent::Qwen, screen);
        assert_eq!(blocked.state, AgentState::Blocked, "{screen}");
        assert!(blocked.visible_blocker, "{screen}");
    }

    let working_title = explain_with_input(
        Agent::Qwen,
        DetectionInput {
            screen: "",
            osc_title: "◐ Editing files",
            osc_progress: "",
        },
    );
    assert_eq!(working_title.state, AgentState::Working);

    for screen in [
        "⠋ Thinking (12s · esc to cancel)",
        "(2m 4s · esc to cancel)",
    ] {
        let working = explain(Agent::Qwen, screen);
        assert_eq!(working.state, AgentState::Working, "{screen}");
        assert!(working.visible_working, "{screen}");
    }

    let progress = explain_with_input(
        Agent::Qwen,
        DetectionInput {
            screen: "",
            osc_title: "",
            osc_progress: "4;3;20",
        },
    );
    assert_eq!(progress.state, AgentState::Working);

    let idle = explain(Agent::Qwen, "> type your message");
    assert_eq!(idle.state, AgentState::Idle);
    assert!(idle.visible_idle);
}

#[test]
fn grok_current_ui_chrome_distinguishes_blocked_working_and_idle() {
    let permission = explain(
        Agent::Grok,
        "┃  2 (○) Yes, proceed\n1/3:select │ Ctrl+o:yolo │ Ctrl+c:cancel",
    );
    assert_eq!(permission.state, AgentState::Blocked);
    assert!(permission.visible_blocker);

    let question = explain(
        Agent::Grok,
        "┃  z (○) Type your answer here\nEsc:unselect │ Tab:scrollback │ Shift+x:dismiss",
    );
    assert_eq!(question.state, AgentState::Blocked);
    assert!(question.visible_blocker);

    let working = explain(
        Agent::Grok,
        "⠧ Waiting on subagent… 2.8s   13s ⇣29.7k [stop]\nEsc:cancel │ Ctrl+.:shortcuts",
    );
    assert_eq!(working.state, AgentState::Working);
    assert!(working.visible_working);

    let idle = explain(Agent::Grok, "Ctrl+.:shortcuts");
    assert_eq!(idle.state, AgentState::Idle);
    assert!(idle.visible_idle);
}

#[test]
fn grok_osc_signals_precede_screen_fallbacks() {
    let plain_idle = explain_with_input(
        Agent::Grok,
        DetectionInput {
            screen: "⠋ ⠙ ⠹ startup artwork",
            osc_title: "grok",
            osc_progress: "",
        },
    );
    assert_eq!(plain_idle.state, AgentState::Idle);
    assert!(!plain_idle.visible_idle);
    assert_eq!(
        plain_idle
            .matched_rule
            .as_ref()
            .map(|rule| rule.id.as_str()),
        Some("osc_title_idle")
    );

    let named_idle = explain_with_input(
        Agent::Grok,
        DetectionInput {
            screen: "",
            osc_title: "review-session - grok",
            osc_progress: "",
        },
    );
    assert_eq!(named_idle.state, AgentState::Idle);
    assert!(!named_idle.visible_idle);
    assert_eq!(
        named_idle
            .matched_rule
            .as_ref()
            .map(|rule| rule.id.as_str()),
        Some("osc_title_idle")
    );

    let title_working = explain_with_input(
        Agent::Grok,
        DetectionInput {
            screen: "",
            osc_title: "⠧ Waiting on subagent…",
            osc_progress: "",
        },
    );
    assert_eq!(title_working.state, AgentState::Working);
    assert_eq!(
        title_working
            .matched_rule
            .as_ref()
            .map(|rule| rule.id.as_str()),
        Some("osc_title_working")
    );
    assert!(!title_working.visible_working);

    let progress_working = explain_with_input(
        Agent::Grok,
        DetectionInput {
            screen: "",
            osc_title: "custom Grok title",
            osc_progress: "4;1;-1",
        },
    );
    assert_eq!(progress_working.state, AgentState::Working);
    assert_eq!(
        progress_working
            .matched_rule
            .as_ref()
            .map(|rule| rule.id.as_str()),
        Some("osc_progress_working")
    );

    let progress_idle = explain_with_input(
        Agent::Grok,
        DetectionInput {
            screen: "⠧ stale status [stop]",
            osc_title: "",
            osc_progress: "4;0;0",
        },
    );
    assert_eq!(progress_idle.state, AgentState::Idle);
    assert_eq!(
        progress_idle
            .matched_rule
            .as_ref()
            .map(|rule| rule.id.as_str()),
        Some("osc_progress_idle")
    );

    let permission = explain_with_input(
        Agent::Grok,
        DetectionInput {
            screen: "⠧ stale status [stop]",
            osc_title: "⚠ Action Required",
            osc_progress: "4;1;-1",
        },
    );
    assert_eq!(permission.state, AgentState::Blocked);
    assert_eq!(
        permission
            .matched_rule
            .as_ref()
            .map(|rule| rule.id.as_str()),
        Some("osc_title_blocked")
    );

    let screen_only = explain_with_input(
        Agent::Grok,
        DetectionInput {
            screen: "⠧ Waiting on subagent… 2.8s [stop]",
            osc_title: "",
            osc_progress: "",
        },
    );
    assert_eq!(screen_only.state, AgentState::Working);
    assert!(screen_only.visible_working);
    assert_eq!(
        screen_only
            .matched_rule
            .as_ref()
            .map(|rule| rule.id.as_str()),
        Some("spinner_status_working")
    );
}

#[test]
fn amp_osc_and_footer_chrome_tracks_active_turns() {
    let blocked = explain_with_input(
        Agent::Amp,
        DetectionInput {
            screen: "",
            osc_title: "Plugin confirmation needed",
            osc_progress: "",
        },
    );
    assert_eq!(blocked.state, AgentState::Blocked);
    assert!(!blocked.visible_blocker);

    let working = explain_with_input(
        Agent::Amp,
        DetectionInput {
            screen: "╰ running tools ─",
            osc_title: "⠧ amp",
            osc_progress: "",
        },
    );
    assert_eq!(working.state, AgentState::Working);
    assert!(!working.visible_working);

    let footer_working = explain(Agent::Amp, "╰ 1 waiting ─");
    assert_eq!(footer_working.state, AgentState::Working);
    assert!(footer_working.visible_working);

    let idle = explain_with_input(
        Agent::Amp,
        DetectionInput {
            screen: "",
            osc_title: "Session - amp - ",
            osc_progress: "",
        },
    );
    assert_eq!(idle.state, AgentState::Idle);
    assert!(!idle.visible_idle);
}

#[test]
fn codex_trust_directory_requires_live_top_region() {
    let screen = "> You are in C:\\Users\\user\\project\n\n\
        Do you trust the contents of this\n\
        directory? Working with untrusted\n\
        contents comes with higher risk of\n\
        prompt injection. Trusting the\n\
        directory allows project-local config,\n\
        hooks, and exec policies to load.\n\n\
        › 1. Yes, continue\n\
          2. No, quit\n\n\
        Press enter to continue\n";
    let result = explain_with_input(
        Agent::Codex,
        DetectionInput {
            screen,
            osc_title: "project",
            osc_progress: "",
        },
    );

    assert_eq!(result.state, AgentState::Blocked);
    assert_eq!(
        result.matched_rule.as_ref().map(|rule| rule.id.as_str()),
        Some("trust_directory")
    );
    assert!(result.visible_blocker);

    let transcript = "› > You are in C:\\Users\\user\\project\n\n\
        Do you trust the contents of this\n\
        directory? Working with untrusted contents comes with higher risk.\n";
    let result = explain_with_input(
        Agent::Codex,
        DetectionInput {
            screen: transcript,
            osc_title: "project",
            osc_progress: "",
        },
    );

    assert_eq!(result.state, AgentState::Idle);
    assert_ne!(
        result.matched_rule.as_ref().map(|rule| rule.id.as_str()),
        Some("trust_directory")
    );
    assert!(!result.visible_blocker);
}
