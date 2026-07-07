---
status: accepted
---

# Isolate reusable platform behavior behind platform APIs

Hako centralizes reusable macOS/Linux process inspection, foreground process-group lookup, process cwd lookup, session traversal, process liveness/signaling, port discovery, clipboard, URL-opening, desktop notification, fd-limit, and host input-source behavior behind `apps/hako/src/platform/`. Product logic should call platform APIs such as `foreground_job`, `foreground_process_group_id`, `foreground_process_group_id_for_tty_fd`, `process_cwd`, `session_processes`, `process_exists`, `signal_processes`, `active_tcp_listeners`, `write_clipboard`, `read_clipboard_image`, `open_url`, `show_desktop_notification`, `raise_server_nofile_limit`, and `PrefixInputSource` instead of duplicating those macOS/Linux implementations at call sites.

This is accepted because Hako needs to run on macOS and Linux while keeping core terminal, detection, workspace, API, and UI code focused on product behavior. The platform module exposes shared data types such as `ForegroundJob`, `ForegroundProcess`, `TcpListenerInfo`, and `Signal`; re-exports macOS or Linux implementations based on target OS; and defines stub fallbacks for unsupported targets for the high-level platform APIs.

## Considered options

- Put OS-specific process and host-integration code directly in the feature modules that need it. Rejected because detection, terminal shutdown, command status, clipboard, notifications, URL opening, and input-source handling would each grow their own operating-system branches.
- Hide every conditional compilation branch in `apps/hako/src/platform/`. Rejected because some `cfg` seams express local API availability or protocol/transport behavior rather than reusable platform APIs, including Unix PTY/fd/handoff support, stdin readiness polling, terminal protocol compatibility, release target metadata, UI modifier labels, file-permission checks, remote/update command orchestration, and the current sound playback module.
- Centralize reusable OS process/cwd/session/liveness/signaling/port/clipboard/URL/notification/input-source behavior behind `apps/hako/src/platform/`, while allowing narrow local `cfg` seams when the conditional belongs to another abstraction. Accepted because it keeps product logic portable without pretending all compilation differences are the same platform API.

## Consequences

New macOS/Linux process inspection, foreground PGID lookup, cwd lookup, process liveness, session traversal/signaling, port discovery, clipboard/image, URL-opening, desktop notification, fd-limit, or host input-source behavior should be added to `apps/hako/src/platform/` first. Core modules may depend on platform-owned types and functions, but should not duplicate platform-owned process inspection (`/proc` foreground job/cwd/session traversal, macOS `proc_pidinfo`/`sysctl`), `lsof` parsing, signal delivery, clipboard command selection, URL opener selection, or desktop-notifier selection. Existing non-platform `cfg` branches should remain narrow, local to the abstraction they protect, and not be treated as precedent for scattering reusable platform behavior.

Historical rationale beyond the current source and repository instructions is `[INFERENCE]`: this boundary likely exists to keep agent detection, pane shutdown, clipboard integration, and notifications maintainable across macOS and Linux without scattering platform expertise through unrelated modules.
