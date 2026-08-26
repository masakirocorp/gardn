#!/usr/bin/env python3
"""Create or attach the isolated Gardn demo session."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
FIXTURE_DIR = REPO_ROOT / "demo" / "session"
AGENT_DIR = FIXTURE_DIR / "agents"
SESSION_NAME = "demo"

GROUPS = (
    {"name": "product", "accent": "yellow", "cwd": "checkout"},
    {"name": "ops", "accent": "red", "cwd": "deploy"},
    {"name": "commerce", "accent": "cyan", "cwd": "billing"},
)

WORKSPACES = (
    {"label": "checkout", "group": "product", "dir": "checkout", "tabs": ("cart", "review")},
    {"label": "catalog", "group": "product", "dir": "catalog", "tabs": ("search", "merch")},
    {"label": "deploy", "group": "ops", "dir": "deploy", "tabs": ("staging", "prod")},
    {"label": "metrics", "group": "ops", "dir": "metrics", "tabs": ("dash",)},
    {"label": "billing", "group": "commerce", "dir": "billing", "tabs": ("invoices", "refunds")},
    {"label": "inventory", "group": "commerce", "dir": "inventory", "tabs": ("stock",)},
)

AGENTS = (
    {"name": "pi", "workspace": "checkout", "tab": "cart", "script": "demo-pi"},
    {"name": "kimi", "workspace": "checkout", "tab": "review", "script": "demo-idle", "seen": False},
    {"name": "omp", "workspace": "catalog", "tab": "search", "script": "demo-omp"},
    {"name": "claude", "workspace": "deploy", "tab": "staging", "script": "demo-claude"},
    {"name": "gemini", "workspace": "metrics", "tab": "dash", "script": "demo-gemini"},
    {"name": "codex", "workspace": "billing", "tab": "invoices", "script": "demo-codex"},
    {"name": "cursor", "workspace": "inventory", "tab": "stock", "script": "demo-cursor"},
)

LOGS_SCRIPT = "demo-logs"
FOCUS_WORKSPACE = "checkout"
FOCUS_TAB = "cart"


def default_home() -> Path:
    override = os.environ.get("GARDN_DEMO_HOME")
    if override:
        return Path(override).expanduser()
    return Path("/tmp/gardn-demo")


def default_bin() -> Path:
    override = os.environ.get("GARDN_BIN")
    if override:
        return Path(override).expanduser()
    which = shutil.which("gardn")
    if which:
        return Path(which)
    return Path.home() / ".local/bin/gardn"


def demo_env(home: Path) -> dict[str, str]:
    env = os.environ.copy()
    env["XDG_CONFIG_HOME"] = str(home / "config")
    env["XDG_STATE_HOME"] = str(home / "state")
    env["GARDN_SESSION"] = SESSION_NAME
    env.pop("GARDN_SOCKET_PATH", None)
    env.pop("GARDN_CLIENT_SOCKET_PATH", None)
    return env


def attach_env(home: Path) -> dict[str, str]:
    env = demo_env(home)
    for key in ("GARDN_ENV", "GARDN_PANE_ID", "GARDN_BIN_PATH", "GARDN_AGENT"):
        env.pop(key, None)
    return env


def run_cli(bin_path: Path, home: Path, *args: str) -> dict[str, Any] | None:
    completed = subprocess.run(
        [str(bin_path), "--session", SESSION_NAME, *args],
        env=demo_env(home),
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"gardn {' '.join(args)} failed ({completed.returncode}): "
            f"{completed.stderr or completed.stdout}"
        )
    text = completed.stdout.strip()
    if not text.startswith("{"):
        return None
    return json.loads(text)


def try_cli(bin_path: Path, home: Path, *args: str) -> dict[str, Any] | None:
    try:
        return run_cli(bin_path, home, *args)
    except RuntimeError:
        return None


def server_running(bin_path: Path, home: Path) -> bool:
    completed = subprocess.run(
        [str(bin_path), "--session", SESSION_NAME, "status", "server"],
        env=demo_env(home),
        capture_output=True,
        text=True,
        check=False,
    )
    return completed.returncode == 0 and "status: running" in completed.stdout


def start_server(bin_path: Path, home: Path) -> None:
    if server_running(bin_path, home):
        return
    log_path = home / "server.log"
    log_path.parent.mkdir(parents=True, exist_ok=True)
    with log_path.open("ab") as log:
        subprocess.Popen(
            [str(bin_path), "--session", SESSION_NAME, "server"],
            env=demo_env(home),
            stdout=log,
            stderr=log,
            start_new_session=True,
        )
    deadline = time.time() + 10
    while time.time() < deadline:
        if server_running(bin_path, home):
            return
        time.sleep(0.1)
    raise RuntimeError(f"demo server did not become ready; see {log_path}")


def stop_server(bin_path: Path, home: Path) -> None:
    if not server_running(bin_path, home):
        return
    subprocess.run(
        [str(bin_path), "--session", SESSION_NAME, "server", "stop"],
        env=demo_env(home),
        check=False,
        capture_output=True,
        text=True,
    )
    deadline = time.time() + 8
    while time.time() < deadline:
        if not server_running(bin_path, home):
            return
        time.sleep(0.1)


def install_fixture(home: Path) -> None:
    for name in ("checkout", "catalog", "deploy", "metrics", "billing", "inventory"):
        (home / "spaces" / name).mkdir(parents=True, exist_ok=True)
    runtime_agents = home / "bin"
    runtime_agents.mkdir(parents=True, exist_ok=True)
    for script in AGENT_DIR.iterdir():
        if script.is_file():
            target = runtime_agents / script.name
            shutil.copy2(script, target)
            target.chmod(0o755)
    config_text = (FIXTURE_DIR / "config.toml").read_text()
    for app_dir in ("gardn", "gardn-dev"):
        dest = home / "config" / app_dir
        dest.mkdir(parents=True, exist_ok=True)
        (dest / "config.toml").write_text(config_text)


def session_path(home: Path) -> Path:
    matches = list((home / "config").glob("*/sessions/demo/session.json"))
    if not matches:
        raise FileNotFoundError(f"no demo session.json under {home / 'config'}")
    return matches[0]


def group_map(payload: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {group["name"]: group for group in payload["result"]["groups"]}


def workspace_map(payload: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {workspace["label"]: workspace for workspace in payload["result"]["workspaces"]}


def ensure_groups(bin_path: Path, home: Path) -> dict[str, dict[str, Any]]:
    groups = group_map(run_cli(bin_path, home, "group", "list"))
    default = next(iter(groups.values()))
    if default["name"] != "product":
        run_cli(bin_path, home, "group", "rename", default["group_id"], "product")
        groups = group_map(run_cli(bin_path, home, "group", "list"))
    for spec in GROUPS:
        if spec["name"] in groups:
            continue
        cwd = home / "spaces" / spec["cwd"]
        run_cli(bin_path, home, "group", "create", spec["name"], "--cwd", str(cwd))
        groups = group_map(run_cli(bin_path, home, "group", "list"))
    return groups


def ensure_workspaces(bin_path: Path, home: Path) -> dict[str, dict[str, Any]]:
    workspaces = workspace_map(run_cli(bin_path, home, "workspace", "list"))
    for spec in WORKSPACES:
        if spec["label"] in workspaces:
            continue
        cwd = home / "spaces" / spec["dir"]
        created = run_cli(
            bin_path,
            home,
            "workspace",
            "create",
            "--cwd",
            str(cwd),
            "--label",
            spec["label"],
            "--no-focus",
        )
        workspaces[spec["label"]] = created["result"]["workspace"]
    return workspace_map(run_cli(bin_path, home, "workspace", "list"))


def ensure_tabs(
    bin_path: Path, home: Path, workspaces: dict[str, dict[str, Any]]
) -> dict[tuple[str, str], dict[str, Any]]:
    tabs = run_cli(bin_path, home, "tab", "list")["result"]["tabs"]
    by_workspace: dict[str, list[dict[str, Any]]] = {}
    for tab in tabs:
        by_workspace.setdefault(tab["workspace_id"], []).append(tab)
    result: dict[tuple[str, str], dict[str, Any]] = {}
    for spec in WORKSPACES:
        workspace = workspaces[spec["label"]]
        existing = {
            tab["label"]: tab for tab in by_workspace.get(workspace["workspace_id"], [])
        }
        cwd = str(home / "spaces" / spec["dir"])
        for label in spec["tabs"]:
            tab = existing.get(label)
            if tab is None:
                created = run_cli(
                    bin_path,
                    home,
                    "tab",
                    "create",
                    "--workspace",
                    workspace["workspace_id"],
                    "--label",
                    label,
                    "--cwd",
                    cwd,
                    "--no-focus",
                )
                tab = created["result"]["tab"]
            result[(spec["label"], label)] = tab
    leftover = run_cli(bin_path, home, "tab", "list")["result"]["tabs"]
    wanted = {(tab["tab_id"]) for tab in result.values()}
    for tab in leftover:
        if tab["label"] in {"1", "scratch"} and tab["tab_id"] not in wanted:
            try_cli(bin_path, home, "tab", "close", tab["tab_id"])
    return result


def ensure_agents(
    bin_path: Path,
    home: Path,
    workspaces: dict[str, dict[str, Any]],
    tabs: dict[tuple[str, str], dict[str, Any]],
) -> None:
    present = {
        agent.get("agent")
        for agent in run_cli(bin_path, home, "agent", "list")["result"]["agents"]
    }
    for spec in AGENTS:
        if spec["name"] in present:
            continue
        workspace = workspaces[spec["workspace"]]
        tab = tabs[(spec["workspace"], spec["tab"])]
        script = home / "bin" / spec["script"]
        run_cli(
            bin_path,
            home,
            "agent",
            "start",
            spec["name"],
            "--workspace",
            workspace["workspace_id"],
            "--tab",
            tab["tab_id"],
            "--no-focus",
            "--",
            str(script),
        )
        present.add(spec["name"])


def ensure_cart_split(bin_path: Path, home: Path, tabs: dict[tuple[str, str], dict[str, Any]]) -> None:
    cart = tabs[(FOCUS_WORKSPACE, FOCUS_TAB)]
    panes = [
        pane
        for pane in run_cli(bin_path, home, "pane", "list")["result"]["panes"]
        if pane["tab_id"] == cart["tab_id"]
    ]
    if len(panes) >= 2:
        return
    split = run_cli(
        bin_path,
        home,
        "pane",
        "split",
        panes[0]["pane_id"],
        "--direction",
        "right",
        "--no-focus",
    )
    log_pane = split["result"]["pane"]["pane_id"]
    run_cli(bin_path, home, "pane", "run", log_pane, str(home / "bin" / LOGS_SCRIPT))


def close_extra_shells(bin_path: Path, home: Path) -> None:
    keep_agents = {spec["name"] for spec in AGENTS}
    cart_tab = None
    for tab in run_cli(bin_path, home, "tab", "list")["result"]["tabs"]:
        if tab["label"] == FOCUS_TAB:
            cart_tab = tab["tab_id"]
    for pane in run_cli(bin_path, home, "pane", "list")["result"]["panes"]:
        if pane.get("agent") in keep_agents:
            continue
        if cart_tab and pane["tab_id"] == cart_tab and pane.get("agent") is None:
            continue
        if pane.get("agent") is None and pane["tab_id"] != cart_tab:
            # Keep named empty tabs (refunds, merch, prod) as single shells.
            tab_panes = [
                other
                for other in run_cli(bin_path, home, "pane", "list")["result"]["panes"]
                if other["tab_id"] == pane["tab_id"]
            ]
            if len(tab_panes) == 1:
                continue
            try_cli(bin_path, home, "pane", "close", pane["pane_id"])


def patch_session_snapshot(snapshot: dict[str, Any]) -> dict[str, Any]:
    accents = {group["name"]: group["accent"] for group in GROUPS}
    workspace_groups = {workspace["label"]: workspace["group"] for workspace in WORKSPACES}
    unseen = {agent["name"] for agent in AGENTS if not agent.get("seen", True)}
    name_by_id = {group["id"]: group["name"] for group in snapshot.get("groups", [])}
    id_by_name = {group["name"]: group["id"] for group in snapshot.get("groups", [])}
    for group in snapshot.get("groups", []):
        accent = accents.get(group.get("name"))
        if accent:
            group["accent"] = accent
    for workspace in snapshot.get("workspaces", []):
        label = workspace.get("custom_name")
        group_name = workspace_groups.get(label)
        if group_name and group_name in id_by_name:
            workspace["group_id"] = id_by_name[group_name]
        for tab in workspace.get("tabs", []):
            for pane in tab.get("panes", {}).values():
                agent_name = pane.get("agent_name") or pane.get("label")
                pane["seen"] = agent_name not in unseen
    snapshot["group_filter_enabled"] = False
    snapshot["active_group"] = 0
    default_view = snapshot.setdefault("default_view", {})
    default_view["active"] = 0
    default_view["selected"] = 0
    default_view["sidebar_collapsed"] = False
    default_view["right_sidebar_collapsed"] = False
    default_view["agent_panel_scope"] = "AllWorkspaces"
    default_view["sidebar_section_split"] = 0.48
    _ = name_by_id
    return snapshot


def apply_snapshot_patch(home: Path) -> None:
    path = session_path(home)
    snapshot = json.loads(path.read_text())
    path.write_text(json.dumps(patch_session_snapshot(snapshot), indent=2) + "\n")


def refresh_agents(bin_path: Path, home: Path) -> None:
    scripts = {spec["name"]: home / "bin" / spec["script"] for spec in AGENTS}
    for agent in run_cli(bin_path, home, "agent", "list")["result"]["agents"]:
        name = agent.get("agent")
        script = scripts.get(name)
        if script is None:
            continue
        try_cli(bin_path, home, "pane", "send-keys", agent["pane_id"], "ctrl+c")
        time.sleep(0.25)
        run_cli(bin_path, home, "pane", "run", agent["pane_id"], str(script))
    cart_tabs = [
        tab
        for tab in run_cli(bin_path, home, "tab", "list")["result"]["tabs"]
        if tab["label"] == FOCUS_TAB
    ]
    if cart_tabs:
        for pane in run_cli(bin_path, home, "pane", "list")["result"]["panes"]:
            if pane["tab_id"] == cart_tabs[0]["tab_id"] and not pane.get("agent"):
                try_cli(
                    bin_path,
                    home,
                    "pane",
                    "send-keys",
                    pane["pane_id"],
                    "ctrl+c",
                )
                time.sleep(0.1)
                run_cli(
                    bin_path,
                    home,
                    "pane",
                    "run",
                    pane["pane_id"],
                    str(home / "bin" / LOGS_SCRIPT),
                )


def focus_showcase(bin_path: Path, home: Path) -> None:
    workspaces = workspace_map(run_cli(bin_path, home, "workspace", "list"))
    checkout = workspaces[FOCUS_WORKSPACE]
    run_cli(bin_path, home, "workspace", "focus", checkout["workspace_id"])
    for tab in run_cli(bin_path, home, "tab", "list")["result"]["tabs"]:
        if tab["label"] == FOCUS_TAB and tab["workspace_id"] == checkout["workspace_id"]:
            run_cli(bin_path, home, "tab", "focus", tab["tab_id"])
            return


def wait_for_statuses(bin_path: Path, home: Path) -> None:
    deadline = time.time() + 12
    last: list[tuple[str | None, str | None]] = []
    while time.time() < deadline:
        agents = run_cli(bin_path, home, "agent", "list")["result"]["agents"]
        last = [(agent.get("agent"), agent.get("agent_status")) for agent in agents]
        known = [status for _, status in last if status not in (None, "unknown")]
        if len(known) >= 3:
            return
        time.sleep(0.4)
    print("warning\tagent statuses still settling: " + ", ".join(f"{n}={s}" for n, s in last))


def seed(bin_path: Path, home: Path, reset: bool) -> None:
    install_fixture(home)
    if reset:
        stop_server(bin_path, home)
        for path in (home / "config").glob("*/sessions/demo/session.json"):
            path.unlink(missing_ok=True)
            history = path.with_name("session-history.json")
            history.unlink(missing_ok=True)
    start_server(bin_path, home)
    ensure_groups(bin_path, home)
    workspaces = ensure_workspaces(bin_path, home)
    tabs = ensure_tabs(bin_path, home, workspaces)
    ensure_agents(bin_path, home, workspaces, tabs)
    ensure_cart_split(bin_path, home, tabs)
    close_extra_shells(bin_path, home)
    focus_showcase(bin_path, home)
    stop_server(bin_path, home)
    apply_snapshot_patch(home)
    start_server(bin_path, home)
    refresh_agents(bin_path, home)
    focus_showcase(bin_path, home)
    wait_for_statuses(bin_path, home)




def print_status(bin_path: Path, home: Path) -> None:
    print(f"home\t{home}")
    print(f"bin\t{bin_path}")
    print(f"running\t{server_running(bin_path, home)}")
    if not server_running(bin_path, home):
        return
    groups = run_cli(bin_path, home, "group", "list")["result"]["groups"]
    print("groups\t" + ", ".join(f"{group['name']}({group['workspace_count']})" for group in groups))
    workspaces = run_cli(bin_path, home, "workspace", "list")["result"]["workspaces"]
    print(
        "spaces\t"
        + ", ".join(f"{workspace['label']}:{workspace['tab_count']}" for workspace in workspaces)
    )
    agents = run_cli(bin_path, home, "agent", "list")["result"]["agents"]
    print(
        "agents\t"
        + ", ".join(f"{agent.get('agent')}={agent.get('agent_status')}" for agent in agents)
    )


def attach_command(bin_path: Path, home: Path) -> list[str]:
    return [
        f"XDG_CONFIG_HOME={home / 'config'}",
        f"XDG_STATE_HOME={home / 'state'}",
        "GARDN_SESSION=demo",
        str(bin_path),
        "--session",
        "demo",
    ]


def attach(bin_path: Path, home: Path) -> int:
    start_server(bin_path, home)
    env = attach_env(home)
    os.execvpe(str(bin_path), [str(bin_path), "--session", SESSION_NAME], env)
    return 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command",
        choices=("seed", "start", "stop", "status", "attach", "print-attach"),
        help="seed rebuilds the isolated demo session",
    )
    parser.add_argument("--reset", action="store_true", help="wipe the isolated session first")
    parser.add_argument("--home", type=Path, default=default_home())
    parser.add_argument("--bin", type=Path, default=default_bin())
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    home = args.home.expanduser()
    bin_path = args.bin.expanduser()
    if not bin_path.exists():
        print(f"error: gardn binary not found: {bin_path}", file=sys.stderr)
        return 1
    if args.command == "seed":
        seed(bin_path, home, reset=args.reset)
        print_status(bin_path, home)
        print("attach\tjust demo-attach")
        print("or\t" + " ".join(attach_command(bin_path, home)))
        return 0
    if args.command == "start":
        install_fixture(home)
        start_server(bin_path, home)
        print_status(bin_path, home)
        return 0
    if args.command == "stop":
        stop_server(bin_path, home)
        print_status(bin_path, home)
        return 0
    if args.command == "status":
        print_status(bin_path, home)
        return 0
    if args.command == "print-attach":
        print(" ".join(attach_command(bin_path, home)))
        return 0
    if args.command == "attach":
        return attach(bin_path, home)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
