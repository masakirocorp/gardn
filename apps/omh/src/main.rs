use std::io;

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;

pub(crate) const OMH_ENV_VAR: &str = "OMH_ENV";
pub(crate) const OMH_ENV_VALUE: &str = "1";
const NESTED_OMH_MESSAGES: [&str; 6] = [
    "inception detected. we need to go deeper... said no one ever.",
    "recursion is a pathway to many abilities some consider to be... unnatural.",
    "you were so preoccupied with whether you could, you didn't stop to think if you should. — dr. malcolm",
    "recursive omhing is disabled. somewhere, a call stack breathes a sigh of relief.",
    "recursive descent denied. there is, in fact, such a thing as too much omh.",
    "recursion detected. base case not found. aborting.",
];

mod agent_detection_policy;
mod agent_profiles;

mod agent_resume;
mod api;
mod app;
mod build_info;
mod checksum;
mod cli;
mod client;
mod commands;
mod config;
mod detect;
mod events;
mod fresh_theme;
mod ghostty;
mod handoff_runtime;
mod hunk_theme;
mod input;
mod integration;
mod ipc;
mod kitty_graphics;
mod layout;
mod lazygit_theme;
mod logging;
mod metadata_tokens;
mod noninteractive_process;
mod pane;
mod persist;
mod platform;
mod plugin_paths;
mod popup_size;
mod ports;
mod product_announcements;
mod protocol;
mod pty;
mod raw_input;
mod release_notes;
mod remote;
mod selection;
mod server;
mod session;
mod settings_rows;
mod sound;
mod terminal;
mod terminal_modes;
mod terminal_notify;
mod terminal_theme;
mod ui;
mod update;
mod workspace;

fn init_logging() {
    crate::logging::init_file_logging("omh.log");
}

const DEFAULT_CONFIG: &str = r##"# Oh My Herdr configuration
# place this file at ~/.config/omh/config.toml

# show first-run notification setup on startup.
# missing also shows onboarding; set false after you've chosen.
# onboarding = true

[theme]
# built-in themes: system, terminal, catppuccin-latte, flexoki-light,
#                 gruvbox-light, kanagawa-lotus, monokai-pro-light,
#                 monokai-pro-light-sun, one-light, rose-pine-dawn,
#                 solarized-light, tokyo-night-day, white, catppuccin,
#                 catppuccin-frappe, catppuccin-macchiato, dracula,
#                 ethereal, everforest, flexoki, gruvbox, hackerman,
#                 kanagawa, last-horizon, lumon, matte-black, miasma,
#                 monokai-classic, monokai-pro, monokai-pro-machine,
#                 monokai-pro-octagon, monokai-pro-ristretto,
#                 monokai-pro-spectrum, nord, one-dark, osaka-jade,
#                 retro-82, rose-pine, solarized, solitude, tokyo-night,
#                 vantablack, vesper
# name = "catppuccin"
# mode = "system"
# light = "system"
# dark = "system"
# terminal_accent = "blue"       # fallback: blue, magenta, cyan, green, yellow, red
# terminal_light_accent = "blue"
# terminal_dark_accent = "blue"

# override individual color tokens on top of the base theme.
# accepts: hex (#rrggbb), named colors, rgb(r,g,b), or panel_bg = "reset"
# [theme.custom]
# panel_bg = "reset"
# accent = "#f5c2e7"
# red = "#ff6188"
# green = "#a6e3a1"

[terminal]
# Executable used for new interactive panes.
# Empty means $SHELL, then /bin/sh.
# default_shell = ""

# Startup mode for new interactive pane shells: "auto", "login", or "non_login".
# "auto" uses login shells on macOS and keeps the current behavior elsewhere.
# shell_mode = "auto"

# CWD policy for new panes, tabs, and workspaces when no explicit --cwd is provided.
# Use "follow" to inherit the source pane/workspace, "home" for $HOME,
# "current" for Oh My Herdr's process directory, or a fixed path such as "~/Projects".
# new_cwd = "follow"

[update]
# Check GitHub for new Oh My Herdr versions in the background.
# version_check = true

# Check for remote agent-detection manifest updates in the background.
# manifest_check = true

[agent_profiles]
# Optional global order across system and custom agent profiles.
# System ids match integration targets, e.g. system:codex, system:omp.
# order = ["system:codex", "system:omp"]

# Custom profiles model wrappers or profile-specific commands.
# Commands are parsed into argv; shell pipes, redirects, globbing, and $VAR
# expansion are not applied. Use env rows for profile/config directories.
# [[agent_profiles.custom]]
# id = "omp-mk"
# name = "omp mk"
# kind = "omp"
# command = "omp-mk"
# enabled = true
#
# [agent_profiles.custom.env]
# PI_CONFIG_DIR = "/Users/me/.omp-mk"

[keys]
# Prefix key to enter prefix mode (default: "ctrl+b")
# Examples: "ctrl+b", "f12", "esc", "-"
# Action bindings use explicit syntax: "prefix+n" requires the prefix;
# "ctrl+alt+n" is a direct terminal-mode shortcut.
# Accepted key syntax: plain keys, ctrl/shift/alt/cmd/super modifiers, and special keys like enter/tab/esc/left/right/up/down.
# Named punctuation such as minus, comma, ampersand, plus, and backtick is also accepted.
# Most reliable direct bindings are ctrl+letter, function keys, and explicit modified chords.
# alt+..., cmd/super, and punctuation-with-modifiers may depend on your terminal/tmux setup.
# prefix = "ctrl+b"

# Prefix-mode actions
# help = "prefix+?"
# settings = "prefix+s"
# detach = "prefix+q"
# reload_config = "prefix+shift+r"
# open_notification_target = "prefix+o"
# workspace_picker = "prefix+w"
# new_workspace = "prefix+shift+n"
# rename_workspace = "prefix+shift+w"
# close_workspace = "prefix+shift+d"
# previous_workspace = "" # optional, unset by default
# next_workspace = ""     # optional, unset by default
# open_group_menu = ""    # optional, unset by default
# new_group = ""          # optional, unset by default
# rename_group = ""       # optional, unset by default
# delete_group = ""       # optional, unset by default
# toggle_group_filter = "" # optional, unset by default
# previous_group = ""     # optional, unset by default
# next_group = ""         # optional, unset by default
# switch_group = "prefix+alt+1..0"
# previous_agent = ""     # optional, unset by default
# next_agent = ""         # optional, unset by default
# open_agent_menu = ""    # optional, unset by default
# command_palette = "prefix+space"
# focus_agent = ""        # optional indexed binding, e.g. "prefix+alt+1..9"
# remote_image_paste = "ctrl+v" # only active in omh --remote; empty disables raw-key image paste
# new_tab = "prefix+c"
# rename_tab = "prefix+shift+t"
# previous_tab = "prefix+p"
# next_tab = "prefix+n"
# switch_tab = "prefix+1..0"
# switch_workspace = "prefix+shift+1..0"
# close_tab = "prefix+shift+x"
# rename_pane = "prefix+shift+p"
# edit_scrollback = "prefix+e"
# focus_pane_left = "prefix+h"
# focus_pane_down = "prefix+j"
# focus_pane_up = "prefix+k"
# focus_pane_right = "prefix+l"
# cycle_pane_next = "prefix+tab"
# cycle_pane_previous = "prefix+shift+tab"
# split_vertical = "prefix+v"
# split_horizontal = "prefix+minus"
# close_pane = "prefix+x"
# zoom = "prefix+z"       # legacy alias: fullscreen
# resize_mode = "prefix+r"
# toggle_sidebar = "prefix+b"
# toggle_context_bar = "prefix+down"
# zen_mode = "prefix+shift+z"
# toggle_right_sidebar = "" # optional, unset by default

# Navigate-mode movement. These local shortcuts win while navigate mode is open.
# They are independent from focus_pane_*. Do not include prefix+, esc, enter, tab, or unmodified 1..0.
# navigate_workspace_up = "up"
# navigate_workspace_down = "down"
# navigate_pane_left = "h"      # left arrow always focuses the pane to the left
# navigate_pane_down = "j"
# navigate_pane_up = "k"
# navigate_pane_right = "l"     # right arrow always focuses the pane to the right

# Custom commands use the same binding syntax.
# type = "shell" runs detached in the background.
# type = "pane" opens a temporary pane and closes it when the command exits.
# On Windows, command strings run through cmd.exe /d /c.
# [[keys.command]]
# key = "prefix+g"
# type = "pane"
# command = "lazygit"
# description = "open lazygit"

#[commands]
# Commands run in the selected project context.
git = "lazygit"
diff = "hunk diff --watch"
ide = "fresh ."

# Legacy indexed shortcut config is still parsed for compatibility.
# Prefer switch_tab, switch_workspace, switch_group, and focus_agent for new configs.
# [keys.indexed]
# tabs = ""       # e.g. "ctrl" makes ctrl+1..9 switch tabs directly
# workspaces = "" # e.g. "ctrl+shift" makes ctrl+shift+1..9 switch workspaces directly
# agents = ""     # e.g. "alt" makes alt+1..9 focus agent rows directly

[ui]
# sidebar width (auto-scaled based on workspace names, this sets the default)
# sidebar_width = 26

# Minimum sidebar width when expanded (columns)
# sidebar_min_width = 18

# Maximum sidebar width when expanded (columns)
# sidebar_max_width = 36

# Terminal width at or below which Oh My Herdr uses the mobile single-column layout.
# Increase this for foldables, tablets, or wide phone terminals.
# mobile_width_threshold = 64
# Bottom context bar visibility: "always" or "never".
# context_bar = "always"



# Capture mouse input for Oh My Herdr's mouse UI.
# Set false to let the terminal handle normal clicks, such as Cmd-clicking URLs.
# Pane apps like lazygit and btop can still receive mouse when they request it.
# mouse_capture = true

# Automatically copy text selected by mouse drag.
# Set false to keep drag selection visible without copying; double-click still copies a word.
# copy_on_select = true

# Host cursor policy: "auto", "native", or "drawn".
# "auto" draws Oh My Herdr's own cursor on native Windows builds and WSL to avoid ConPTY cursor flicker.
# "native" always uses the outer terminal cursor; "drawn" always draws Oh My Herdr's cursor as cell content.
# host_cursor = "auto"

# Optional modifier that forwards right-click hold/drag gestures to pane apps instead of opening Oh My Herdr's pane menu.
# Empty/off disables this. Shift is intentionally unsupported because terminals commonly reserve Shift+mouse.
# Supported values include "ctrl", "alt", "cmd", "super", "meta", "hyper", and + combinations such as "cmd+alt".
# right_click_passthrough_modifier = ""

# Force a full redraw when the outer terminal regains focus.
# Set false to reduce visible flashing when switching back to Oh My Herdr.
# Trade-off: rare host terminal surface corruption may persist until the next full redraw.
# redraw_on_focus_gained = true

# Pane scrollback lines to scroll per mouse wheel notch.
# mouse_scroll_lines = 3

# ask for confirmation before closing a workspace
# confirm_close = true

# ask for a tab name before creating a new tab.
# set false to create tabs immediately with generated names.
# prompt_new_tab_name = true

# ask for a workspace name before interactive creation.
# prompt_new_workspace_name = false

# agent metadata shown in split pane borders when no title or manual name is set.
# pane_border_agent_info = "hidden" # "hidden", "name", or "name_and_status"

# draw borders around split panes.
# pane_borders = true

# keep split panes visually separated instead of sharing divider borders.
# pane_gaps = true

# hide the tab row when a workspace has exactly one tab.
# hide_tab_bar_when_single_tab = false

# show right-aligned topology and section counters.
# show_counters = false

# collapsed sidebar mode: "compact" (narrow rail) or "hidden" (zero-width).
# sidebar_collapsed_mode = "compact"


# accent color for highlights, borders, and navigation ui.
# accepts: hex (#89b4fa), named colors (cyan, blue, magenta), or rgb(r,g,b)
# accent = "cyan"
[ui.sidebar]
# Initial view for each newly connected client. Runtime changes remain client-local.
# "expanded" or "collapsed"
initial_state = "expanded"
# "all", "group", or "current" (space)
initial_agent_scope = "all"


# background notification popup delivery
[ui.toast]
# off = disable pop-up notifications
# omh = show in-app toasts
# terminal = ask the outer terminal to show a desktop notification
# system = ask the OS notification service directly
# delivery = "off"
# delay_seconds = 1

[ui.toast.omh]
# position = "bottom-right"

[ui.toast.clipboard]
# enabled = true
# position = "bottom-center"

# play sounds when agents change state in background workspaces
[ui.sound]
# enabled = true
# optional custom mp3 sound files. relative paths are resolved from this config file's directory.
# path = "sounds/notification.mp3"   # one mp3 file for all sound notifications
# done_path = "sounds/done.mp3"      # overrides only finished notifications
# request_path = "sounds/request.mp3" # overrides only needs-attention notifications

# per-agent overrides: default | on | off
# by default, droid is muted.
# [ui.sound.agents]
# droid = "off"

[session]
# Resume supported AI-agent panes into their native conversation sessions after
# an Oh My Herdr server restart. Requires official integrations that report session refs.
# resume_agents_on_restore = true

[remote]
# Whether Oh My Herdr manages the ssh config used for the `omh --remote` bridge.
# When true (default), Oh My Herdr runs the bridge ssh through a generated config that
# includes your ~/.ssh/config first and adds ServerAliveInterval/
# ServerAliveCountMax as a fallback, so any keepalive you set yourself still
# wins and idle network/NAT timeouts are less likely to drop the bridge.
# Set false to run plain ssh against your ssh config unchanged.
# manage_ssh_config = true

[experimental]
# Allow launching Oh My Herdr from inside an Oh My Herdr-managed pane.
# allow_nested = false
# Experimental local Kitty graphics rendering for attached clients.
# Requires a Kitty graphics-compatible outer terminal.
# kitty_graphics = false
# Save recent pane screen history across full server restarts.
# pane_history = true
# While prefix mode is active, temporarily switch the macOS host input
# source to an ASCII-capable keyboard layout so prefix commands register
# even when a CJK IME is active, then restore the previous input source
# when prefix mode exits. macOS only; best-effort. Default: false.
# switch_ascii_input_source_in_prefix = false
# Expose the focused pane's cursor to the outer terminal so macOS input
# methods keep tracking the candidate window when TUIs paint their own
# cursor (Claude Code, pi, codex). Trade-off: extra cursor visible for
# apps that hide it without painting a replacement (vim normal mode, etc.).
# reveal_hidden_cursor_for_cjk_ime = false
# Optional allow-list: only reveal for focused panes whose detected agent
# matches one of these names. Empty means apply to any focused pane.
# If the list contains no valid names, the reveal does not apply.
# Accepted: pi, claude, codex, gemini, cursor, cline, opencode, copilot,
# kimi, kiro, droid, amp, grok, hermes, kilo, qodercli, qoder, maki.
# cjk_ime_agents = []
# Cursor shape rendered when reveal_hidden_cursor_for_cjk_ime is true.
# Values: block, steady_block (default), underline, steady_underline, bar, steady_bar.
# cjk_ime_cursor_shape = "steady_block"

[advanced]
# Maximum scrollback buffer size in bytes retained per pane terminal.
# Matches Ghostty's default scrollback-limit behavior.
# scrollback_limit_bytes = 10000000
"##;

fn should_block_nested(config: &config::Config) -> bool {
    should_block_nested_for_env(config, std::env::var(OMH_ENV_VAR).ok().as_deref())
}

fn should_block_nested_for_env(config: &config::Config, omh_env: Option<&str>) -> bool {
    !config.experimental.allow_nested && omh_env == Some(OMH_ENV_VALUE)
}

fn random_nested_message() -> &'static str {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as usize)
        .unwrap_or(0);
    let index = (nanos ^ (std::process::id() as usize)) % NESTED_OMH_MESSAGES.len();
    NESTED_OMH_MESSAGES[index]
}

fn exit_if_nested_disabled(config: &config::Config) {
    if should_block_nested(config) {
        eprintln!("\x1b[1merror:\x1b[0m nested Oh My Herdr is disabled by default.");
        eprintln!("see configuration if you want to enable it.");
        eprintln!();
        eprintln!("\x1b[2m\"{}\"\x1b[0m", random_nested_message());
        std::process::exit(1);
    }
}

fn main() -> io::Result<()> {
    let raw_args: Vec<String> = std::env::args().collect();
    let args = match session::configure_from_args(&raw_args) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!("run 'omh --help' for usage");
            std::process::exit(2);
        }
    };
    let (args, remote_launch) = match remote::extract_remote_args(&args) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!("run 'omh --help' for usage");
            std::process::exit(2);
        }
    };

    if remote_launch.is_some()
        && args.get(1).is_some()
        && !args.iter().any(|a| {
            matches!(
                a.as_str(),
                "--help" | "-h" | "--version" | "-V" | "--default-config"
            )
        })
    {
        eprintln!("error: --remote can only be used with the default launch command");
        eprintln!("run 'omh --help' for usage");
        std::process::exit(2);
    }

    match cli::maybe_run(&args) {
        Ok(cli::CommandOutcome::Handled(code)) => std::process::exit(code),
        Ok(cli::CommandOutcome::NotCli) => {}
        Err(error) if cli::protocol_mismatch_was_reported(&error) => std::process::exit(1),
        Err(error) => return Err(error),
    }

    // subcommands and flags (no tui, no logging needed)
    if args.get(1).map(|s| s.as_str()) == Some("remote-client-bridge") {
        return remote::run_remote_client_bridge();
    }

    if args.get(1).map(|s| s.as_str()) == Some("server") {
        return server::headless::run_server();
    }

    // Hidden client mode: connect to an existing server's client socket.
    if args.get(1).map(|s| s.as_str()) == Some("client") {
        let loaded_config = config::Config::load();
        exit_if_nested_disabled(&loaded_config.config);
        return client::run_client();
    }

    if args.get(1).map(|s| s.as_str()) == Some("update") {
        let options = match update::parse_self_update_args(&args[2..]) {
            Ok(options) => options,
            Err(err) if err.starts_with("usage:") => {
                eprintln!("{err}");
                std::process::exit(0);
            }
            Err(err) => {
                eprintln!("{err}");
                eprintln!("usage: omh update [--handoff]");
                std::process::exit(2);
            }
        };
        match update::self_update(options) {
            Ok(_) => return Ok(()),
            Err(e) => {
                if e.starts_with("self-update is disabled") {
                    eprintln!("{e}");
                } else {
                    eprintln!("update failed: {e}");
                }
                std::process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("omh — terminal workspace manager for ai coding agents");
        println!();
        println!("usage: omh [options]");
        println!("       omh --session <name> [options]");
        println!("       omh --remote <ssh-target> [--session <name>]");
        println!("       omh session attach <name>");
        println!("       omh update [--handoff]");
        println!("       omh server stop");
        println!("       omh server reload-config");
        println!("       omh api <subcommand> ...");
        println!("       omh config <subcommand> ...");
        println!("       omh workspace <subcommand> ...");
        println!("       omh tab <subcommand> ...");
        println!("       omh agent <subcommand> ...");
        println!("       omh pane <subcommand> ...");
        println!("       omh wait <subcommand> ...");
        println!("       omh session <subcommand> ...");
        println!("       omh integration <subcommand> ...");
        println!();
        println!("common commands:");
        for (command, description) in [
            ("omh", "launch or attach to the persistent session"),
            (
                "omh status [server|client]",
                "show local client and running server status",
            ),
            ("omh update", "download and install the latest version"),
            (
                "omh server stop",
                "stop the running server via the api socket",
            ),
            (
                "omh server reload-config",
                "reload config.toml in the running server",
            ),
            (
                "omh config reset-keys",
                "Back up config.toml and remove custom keybindings",
            ),
            (
                "omh workspace <subcommand>",
                "workspace helpers over the socket api",
            ),
            ("omh tab <subcommand>", "tab helpers over the socket api"),
            (
                "omh agent <subcommand>",
                "Agent/terminal helpers over the socket API",
            ),
            (
                "omh pane <subcommand>",
                "pane control helpers over the socket api",
            ),
            (
                "omh wait <subcommand>",
                "blocking wait helpers over the socket api",
            ),
            (
                "omh session <subcommand>",
                "manage named persistent sessions",
            ),
            (
                "omh integration <subcommand>",
                "manage built-in agent integrations",
            ),
            (
                "omh api <subcommand>",
                "inspect socket API metadata and live runtime state",
            ),
        ] {
            println!("  {command:<32} {description}");
        }
        println!();
        println!("advanced commands:");
        println!("  {:<32} run as headless server", "omh server");
        println!();
        println!("options:");
        println!("  --no-session        run monolithically (no server/client, escape hatch)");
        println!("  --session <name>    use or create a named persistent session");
        println!("  --remote <target>   attach through ssh to a remote Oh My Herdr server");
        println!("  --remote-keybindings <local|server>");
        println!("                      keybindings for --remote app attach (default: local)");
        println!("  --handoff           opt into live handoff for update or remote attach");
        println!("  --default-config    print default configuration and exit");
        println!("  --version, -V       print version and exit");
        println!("  --help, -h          show this help");
        println!();
        println!("config: {}", config::config_path().display());
        println!("logs:   {}", logging::help_log_paths_summary());
        println!("env:    OMH_CONFIG_PATH overrides config file path");
        println!("home:   https://github.com/masakirocorp/oh-my-herdr");
        return Ok(());
    }

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("omh {}", crate::build_info::version());
        return Ok(());
    }

    if args.iter().any(|a| a == "--default-config") {
        print!("{DEFAULT_CONFIG}");
        return Ok(());
    }

    // Reject unknown flags
    let known_flags = [
        "--no-session",
        "--session",
        "--remote",
        "--remote-keybindings",
        "--version",
        "-V",
        "--default-config",
        "--help",
        "-h",
    ];
    for arg in &args[1..] {
        let arg_name = arg.split_once('=').map(|(name, _)| name).unwrap_or(arg);
        if arg.starts_with('-') && !known_flags.contains(&arg_name) {
            eprintln!("unknown option: {arg}");
            eprintln!("run 'omh --help' for usage");
            std::process::exit(1);
        }
        if !arg.starts_with('-')
            && ![
                "server",
                "client",
                "remote-client-bridge",
                "update",
                "status",
                "config",
                "workspace",
                "pane",
                "wait",
                "session",
                "integration",
            ]
            .contains(&arg.as_str())
        {
            eprintln!("unknown command: {arg}");
            eprintln!("run 'omh --help' for usage");
            std::process::exit(1);
        }
    }

    if let Some(remote_launch) = remote_launch {
        let remote_target = remote_launch.target.clone();
        if let Err(err) = remote::run_remote(remote_launch) {
            eprintln!("error: {err}");
            remote::print_remote_error_hint(&err, &remote_target);
            std::process::exit(1);
        }
        return Ok(());
    }

    let loaded_config = config::Config::load();
    exit_if_nested_disabled(&loaded_config.config);

    let no_session = args.iter().any(|a| a == "--no-session");

    // Auto-detect launch: when --no-session is NOT set, use server/client mode.
    // Check if a server is running, spawn one if needed, then attach as client.
    if !no_session {
        if let Err(err) = server::autodetect::auto_detect_launch() {
            eprintln!("omh: {err}");
            std::process::exit(1);
        }
        return Ok(());
    }

    // --- Monolithic mode (--no-session escape hatch) ---
    // This is the pre-mission single-process behavior.

    init_logging();

    let (api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
    let event_hub = api::EventHub::default();
    let _api_server = match api::start_server_with_capabilities(
        api_tx,
        event_hub.clone(),
        api::default_server_capabilities(),
    ) {
        Ok(server) => server,
        Err(err) if err.kind() == io::ErrorKind::AddrInUse => {
            eprintln!("error: Oh My Herdr is already running");
            eprintln!("socket: {}", api::socket_path().display());
            std::process::exit(1);
        }
        Err(err) => return Err(err),
    };

    let modify_other_keys_mode = crate::input::host_modify_other_keys_mode();

    let original_hook = std::panic::take_hook();
    let panic_resets_modify_other_keys = modify_other_keys_mode.is_some();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("PANIC: {info}");
        if panic_resets_modify_other_keys {
            let _ = std::io::Write::write_all(&mut io::stdout(), b"\x1b[>4;0m");
        }
        if crate::kitty_graphics::is_enabled() {
            let _ = crate::kitty_graphics::clear_all_host_graphics();
        }
        let _ = execute!(
            io::stdout(),
            PopKeyboardEnhancementFlags,
            DisableFocusChange,
            DisableBracketedPaste,
            DisableMouseCapture
        );
        let _ = crate::terminal_modes::clear_host_mouse_reporting(&mut io::stdout());
        ratatui::restore();
        original_hook(info);
    }));

    let config = &loaded_config.config;
    let config_diagnostics =
        (!loaded_config.diagnostics.is_empty()).then(|| loaded_config.diagnostics.clone());
    logging::startup("app");

    // Background update check (non-blocking, best-effort)
    // only checks for newer versions and notifies the tui.
    // Skipped in --no-session mode (testing).

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let result = rt.block_on(async {
        let mut terminal = ratatui::init();
        crate::terminal_modes::clear_host_mouse_reporting(&mut io::stdout())?;
        if config.ui.mouse_capture {
            execute!(io::stdout(), EnableMouseCapture)?;
        } else {
            execute!(io::stdout(), DisableMouseCapture)?;
        }
        execute!(
            io::stdout(),
            EnableBracketedPaste,
            EnableFocusChange,
            PushKeyboardEnhancementFlags(crate::input::ime_compatible_keyboard_enhancement_flags())
        )?;

        // Some hosts do not honor Kitty keyboard enhancement pushes for
        // Shift+Enter. Enable xterm modifyOtherKeys only on hosts where we
        // know it is needed and parseable, so modified Enter stays distinct.
        if let Some(mode) = modify_other_keys_mode {
            use std::io::Write;
            std::io::stdout().write_all(mode.set_sequence())?;
            std::io::stdout().flush()?;
        }

        let mut app = app::App::new(
            config,
            true, // no_session — monolithic mode never saves/restores sessions
            config_diagnostics,
            api_rx,
            event_hub,
        );
        let result = app.run(&mut terminal).await;

        // Reset modifyOtherKeys if we enabled it.
        if modify_other_keys_mode.is_some() {
            use std::io::Write;
            std::io::stdout().write_all(b"\x1b[>4;0m")?;
            std::io::stdout().flush()?;
        }

        if crate::kitty_graphics::is_enabled() {
            crate::kitty_graphics::clear_all_host_graphics()?;
        }
        execute!(
            io::stdout(),
            PopKeyboardEnhancementFlags,
            DisableFocusChange,
            DisableBracketedPaste,
            DisableMouseCapture
        )?;
        crate::terminal_modes::clear_host_mouse_reporting(&mut io::stdout())?;
        ratatui::restore();

        // Drop app (and all workspaces/panes) before runtime shuts down
        drop(app);

        result
    });

    // Shut down runtime immediately — kills lingering PTY reader/writer tasks
    rt.shutdown_timeout(std::time::Duration::from_millis(100));

    logging::shutdown("app");
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses_configured_commands() {
        let config: config::Config = toml::from_str(DEFAULT_CONFIG).unwrap();

        assert_eq!(config.commands, config::CommandsConfig::default());
    }

    #[test]
    fn nested_omh_blocks_when_env_is_set() {
        let config = config::Config::default();
        assert!(should_block_nested_for_env(&config, Some(OMH_ENV_VALUE)));
    }

    #[test]
    fn nested_omh_does_not_block_when_allowed() {
        let config: config::Config =
            toml::from_str("[experimental]\nallow_nested = true\n").unwrap();
        assert!(!should_block_nested_for_env(&config, Some(OMH_ENV_VALUE)));
    }

    #[test]
    fn nested_omh_does_not_block_without_env() {
        let config = config::Config::default();
        assert!(!should_block_nested_for_env(&config, None));
    }

    #[test]
    fn random_nested_message_comes_from_known_set() {
        let message = random_nested_message();
        assert!(NESTED_OMH_MESSAGES.contains(&message));
    }

    #[test]
    fn nested_message_strings_no_longer_repeat_omh_prefix() {
        assert!(NESTED_OMH_MESSAGES
            .iter()
            .all(|message| !message.starts_with("omh:")));
    }
}
