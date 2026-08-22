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
            eprintln!("usage: gardn integration status [--outdated-only]");
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
            crate::integration::IntegrationStatusKind::Outdated
                if status
                    .installed_version
                    .is_some_and(|installed| installed >= status.expected_version) =>
            {
                format!("needs repair ({version})")
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
            "usage: gardn integration {action} <pi|omp|claude|codex|devin|kimi|droid|copilot|opencode|hermes|qodercli|cursor>"
        );
        return Ok(None);
    };
    if args.len() != 1 {
        eprintln!(
            "usage: gardn integration {action} <pi|omp|claude|codex|devin|kimi|droid|copilot|opencode|hermes|qodercli|cursor|grok>"
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
        "grok" => IntegrationTarget::Grok,
        _ => {
            eprintln!("unknown integration target: {target}");
            eprintln!("currently supported: pi, omp, claude, codex, devin, kimi, droid, copilot, opencode, hermes, qodercli, cursor, grok");
            return Ok(None);
        }
    };

    Ok(Some(parsed))
}

fn print_integration_help() {
    eprintln!("gardn integration commands:");
    eprintln!("  gardn integration install pi");
    eprintln!("  gardn integration install omp");
    eprintln!("  gardn integration install claude");
    eprintln!("  gardn integration install codex");
    eprintln!("  gardn integration install devin");
    eprintln!("  gardn integration install kimi");
    eprintln!("  gardn integration install droid");
    eprintln!("  gardn integration install opencode");
    eprintln!("  gardn integration install hermes");
    eprintln!("  gardn integration install qodercli");
    eprintln!("  gardn integration install cursor");
    eprintln!("  gardn integration uninstall pi");
    eprintln!("  gardn integration uninstall omp");
    eprintln!("  gardn integration uninstall claude");
    eprintln!("  gardn integration uninstall codex");
    eprintln!("  gardn integration uninstall devin");
    eprintln!("  gardn integration uninstall kimi");
    eprintln!("  gardn integration uninstall droid");
    eprintln!("  gardn integration uninstall opencode");
    eprintln!("  gardn integration uninstall hermes");
    eprintln!("  gardn integration uninstall qodercli");
    eprintln!("  gardn integration uninstall cursor");
    eprintln!("  gardn integration status [--outdated-only]");
}
