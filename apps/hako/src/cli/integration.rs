use crate::api::schema::IntegrationTarget;

pub(super) fn run_integration_command(args: &[String]) -> std::io::Result<i32> {
    let Some(subcommand) = args.first().map(|arg| arg.as_str()) else {
        print_integration_help();
        return Ok(2);
    };

    match subcommand {
        "install" => integration_install(&args[1..]),
        "uninstall" => integration_uninstall(&args[1..]),
        "status" => integration_status(&args[1..]),
        "help" | "--help" | "-h" => {
            print_integration_help();
            Ok(0)
        }
        _ => {
            print_integration_help();
            Ok(2)
        }
    }
}

fn integration_status(args: &[String]) -> std::io::Result<i32> {
    let outdated_only = match args {
        [] => false,
        [flag] if flag == "--outdated-only" => true,
        _ => {
            eprintln!("usage: hako integration status [--outdated-only]");
            return Ok(2);
        }
    };

    if outdated_only {
        crate::integration::print_outdated_update_notice();
        return Ok(0);
    }

    for status in crate::integration::installed_integration_statuses() {
        let target = crate::integration::integration_target_label(status.target);
        let version = match status.installed_version {
            Some(version) => format!("v{version}"),
            None => "legacy".to_string(),
        };
        let state = match status.state {
            crate::integration::IntegrationStatusKind::NotInstalled => "not installed".to_string(),
            crate::integration::IntegrationStatusKind::Current => {
                format!("current ({version})")
            }
            crate::integration::IntegrationStatusKind::MissingProfileHooks => {
                "missing profile hooks".to_string()
            }
            crate::integration::IntegrationStatusKind::Outdated => {
                format!("outdated ({version}; expected v{})", status.expected_version)
            }
        };
        println!("{target}: {state} ({})", status.path.display());
    }

    Ok(0)
}

fn integration_install(args: &[String]) -> std::io::Result<i32> {
    let Some(target) = parse_integration_target(args, "install")? else {
        return Ok(2);
    };

    match crate::integration::install_target(target) {
        Ok(messages) => {
            print_integration_messages(messages);
            Ok(0)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(1)
        }
    }
}

fn integration_uninstall(args: &[String]) -> std::io::Result<i32> {
    let Some(target) = parse_integration_target(args, "uninstall")? else {
        return Ok(2);
    };

    match crate::integration::uninstall_target(target) {
        Ok(messages) => {
            print_integration_messages(messages);
            Ok(0)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(1)
        }
    }
}

fn print_integration_messages(messages: Vec<String>) {
    for message in messages {
        println!("{message}");
    }
}

fn parse_integration_target(
    args: &[String],
    action: &str,
) -> std::io::Result<Option<IntegrationTarget>> {
    let Some(target) = args.first().map(|arg| arg.as_str()) else {
        eprintln!(
            "usage: hako integration {action} <pi|omp|claude|codex|devin|kimi|droid|copilot|opencode|hermes|qodercli|cursor>"
        );
        return Ok(None);
    };
    if args.len() != 1 {
        eprintln!(
            "usage: hako integration {action} <pi|omp|claude|codex|devin|kimi|droid|copilot|opencode|hermes|qodercli|cursor>"
        );
        return Ok(None);
    }

    let parsed = match target {
        "pi" => IntegrationTarget::Pi,
        "omp" => IntegrationTarget::Omp,
        "claude" => IntegrationTarget::Claude,
        "codex" => IntegrationTarget::Codex,
        "copilot" => IntegrationTarget::Copilot,
        "devin" => IntegrationTarget::Devin,
        "kimi" => IntegrationTarget::Kimi,
        "droid" => IntegrationTarget::Droid,
        "opencode" => IntegrationTarget::Opencode,
        "hermes" => IntegrationTarget::Hermes,
        "qodercli" => IntegrationTarget::Qodercli,
        "cursor" => IntegrationTarget::Cursor,
        _ => {
            eprintln!("unknown integration target: {target}");
            eprintln!("currently supported: pi, omp, claude, codex, devin, kimi, droid, copilot, opencode, hermes, qodercli, cursor");
            return Ok(None);
        }
    };

    Ok(Some(parsed))
}

fn print_integration_help() {
    eprintln!("hako integration commands:");
    eprintln!("  hako integration install pi");
    eprintln!("  hako integration install omp");
    eprintln!("  hako integration install claude");
    eprintln!("  hako integration install codex");
    eprintln!("  hako integration install devin");
    eprintln!("  hako integration install kimi");
    eprintln!("  hako integration install droid");
    eprintln!("  hako integration install opencode");
    eprintln!("  hako integration install hermes");
    eprintln!("  hako integration install qodercli");
    eprintln!("  hako integration install cursor");
    eprintln!("  hako integration uninstall pi");
    eprintln!("  hako integration uninstall omp");
    eprintln!("  hako integration uninstall claude");
    eprintln!("  hako integration uninstall codex");
    eprintln!("  hako integration uninstall devin");
    eprintln!("  hako integration uninstall kimi");
    eprintln!("  hako integration uninstall droid");
    eprintln!("  hako integration uninstall opencode");
    eprintln!("  hako integration uninstall hermes");
    eprintln!("  hako integration uninstall qodercli");
    eprintln!("  hako integration uninstall cursor");
    eprintln!("  hako integration status [--outdated-only]");
}
