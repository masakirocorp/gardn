use crate::api::schema::{ConnectionSaveParams, ConnectionTarget, EmptyParams, Method, Request};

pub(super) fn run_connection_command(args: &[String]) -> std::io::Result<i32> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        print_connection_help();
        return Ok(2);
    };
    match subcommand {
        "list" => connection_list(&args[1..]),
        "save" => connection_save(&args[1..]),
        "delete" => connection_action(&args[1..], "delete"),
        "test" => connection_action(&args[1..], "test"),
        "connect" => connection_action(&args[1..], "connect"),
        "disconnect" => connection_action(&args[1..], "disconnect"),
        "help" | "--help" | "-h" => {
            print_connection_help();
            Ok(0)
        }
        _ => {
            print_connection_help();
            Ok(2)
        }
    }
}

fn connection_list(args: &[String]) -> std::io::Result<i32> {
    if !args.is_empty() {
        eprintln!("usage: gardn connection list");
        return Ok(2);
    }
    super::print_response(&super::send_request(&Request {
        id: "cli:connection:list".into(),
        method: Method::ConnectionList(EmptyParams::default()),
    })?)
}

fn connection_save(args: &[String]) -> std::io::Result<i32> {
    let Some(profile_id) = args.first() else {
        eprintln!(
            "usage: gardn connection save <profile-id> --name NAME --target SSH_TARGET [--directory PATH]"
        );
        return Ok(2);
    };
    let mut name = None;
    let mut target = None;
    let mut suggested_directory = None;
    let mut index = 1;
    while index < args.len() {
        let option = args[index].as_str();
        let Some(value) = args.get(index + 1) else {
            eprintln!("missing value for {option}");
            return Ok(2);
        };
        match option {
            "--name" => name = Some(value.clone()),
            "--target" => target = Some(value.clone()),
            "--directory" => suggested_directory = Some(value.clone()),
            _ => {
                eprintln!("unknown option: {option}");
                return Ok(2);
            }
        }
        index += 2;
    }
    let Some(name) = name else {
        eprintln!("missing --name");
        return Ok(2);
    };
    let Some(target) = target else {
        eprintln!("missing --target");
        return Ok(2);
    };
    super::print_response(&super::send_request(&Request {
        id: "cli:connection:save".into(),
        method: Method::ConnectionSave(ConnectionSaveParams {
            profile_id: profile_id.clone(),
            name,
            target,
            suggested_directory,
        }),
    })?)
}

fn connection_action(args: &[String], action: &str) -> std::io::Result<i32> {
    let [profile_id] = args else {
        eprintln!("usage: gardn connection {action} <profile-id>");
        return Ok(2);
    };
    let target = ConnectionTarget {
        profile_id: profile_id.clone(),
    };
    let method = match action {
        "delete" => Method::ConnectionDelete(target),
        "test" => Method::ConnectionTest(target),
        "connect" => Method::ConnectionConnect(target),
        "disconnect" => Method::ConnectionDisconnect(target),
        _ => unreachable!("connection action is selected by the command dispatcher"),
    };
    super::print_response(&super::send_request(&Request {
        id: format!("cli:connection:{action}"),
        method,
    })?)
}

fn print_connection_help() {
    eprintln!("gardn connection commands:");
    eprintln!("  gardn connection list");
    eprintln!(
        "  gardn connection save <profile-id> --name NAME --target SSH_TARGET [--directory PATH]"
    );
    eprintln!("  gardn connection delete <profile-id>");
    eprintln!("  gardn connection test <profile-id>");
    eprintln!("  gardn connection connect <profile-id>");
    eprintln!("  gardn connection disconnect <profile-id>");
}
