#!/usr/bin/env python3
"""Create or attach the isolated Gardn demo session."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
FIXTURE_DIR = REPO_ROOT / "demo" / "session"
AGENT_DIR = FIXTURE_DIR / "agents"
SESSION_NAME = "demo"
NESTED_UNSETS = (
    "GARDN_PANE_ID",
    "GARDN_ENV",
    "GARDN_AGENT",
    "GARDN_BIN_PATH",
    "GARDN_SOCKET_PATH",
    "GARDN_CLIENT_SOCKET_PATH",
)
CAPTURE_WINDOW_TITLE = "Gardn Demo Capture"
CAPTURE_WINDOW = {
    "x": 40,
    "y": 36,
    "width": 1179,
    "height": 798,
    "columns": 131,
    "rows": 42,
    "font_family": "Masakiro Mono",
    "font_size": 15,
    "pad_top": 32,
}
GHOSTTY_THEME_NAMES = {
    "day": "Gardn Day",
    "night": "Gardn Night",
}





HOST_TERMINAL = {
    "day": {
        "window_theme": "light",
        "background": "#FFFFFC",
        "foreground": "#1F1F1F",
    },
    "night": {
        "window_theme": "dark",
        "background": "#071A13",
        "foreground": "#F7F3EA",
    },
}
GHOSTTY_APP = Path("/Applications/Ghostty.app")





GROUPS = (
    {"name": "product", "accent": "yellow", "icon": "✎", "cwd": "checkout"},
    {"name": "ops", "accent": "red", "icon": "⚙", "cwd": "deploy"},
    {"name": "commerce", "accent": "cyan", "icon": "♥", "cwd": "billing"},
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
    {"name": "pi", "workspace": "checkout", "tab": "cart", "script": "demo-pi", "state": "working", "age_secs": 40},
    {"name": "kimi", "workspace": "checkout", "tab": "review", "script": "demo-idle", "seen": False, "state": "idle", "age_secs": 8 * 60},
    {"name": "omp", "workspace": "catalog", "tab": "search", "script": "demo-omp", "state": "blocked", "age_secs": 3 * 60},
    {"name": "claude", "workspace": "deploy", "tab": "staging", "script": "demo-claude", "state": "blocked", "age_secs": 26 * 60},
    {"name": "gemini", "workspace": "metrics", "tab": "dash", "script": "demo-gemini", "state": "working", "age_secs": 90},
    {"name": "codex", "workspace": "billing", "tab": "invoices", "script": "demo-codex", "state": "idle", "age_secs": 2 * 3600},
    {"name": "cursor", "workspace": "inventory", "tab": "stock", "script": "demo-cursor", "state": "idle", "age_secs": 5 * 3600},
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
    debug = REPO_ROOT / "target" / "debug" / "gardn"
    if debug.is_file():
        return debug
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
    checkout = home / "spaces" / "checkout"
    (checkout / "package.json").write_text(
        '{\n  "name": "checkout",\n  "private": true,\n  "scripts": {\n    "dev": "node -e \\"setInterval(() => {}, 1e9)\\"",\n    "lint": "node -e \\"process.exit(0)\\""\n  }\n}\n'
    )
    (checkout / "justfile").write_text("test:\n    echo checkout-ok\n")
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
    apply_theme(home, os.environ.get("GARDN_DEMO_THEME", "day"))


def apply_theme(home: Path, theme: str) -> None:
    if theme not in ("day", "night"):
        raise ValueError(f"unknown demo theme: {theme!r}")
    mode = "dark" if theme == "night" else "light"
    pattern = re.compile(r'(?m)^(mode\s*=\s*")[^"]*(")')
    for app_dir in ("gardn", "gardn-dev"):
        path = home / "config" / app_dir / "config.toml"
        if not path.is_file():
            continue
        text = path.read_text()
        start = text.find("[theme]")
        if start < 0:
            continue
        nxt = text.find("\n[", start + 1)
        section = text[start:] if nxt < 0 else text[start:nxt]
        updated, count = pattern.subn(rf"\g<1>{mode}\2", section, count=1)
        if count == 0:
            continue
        path.write_text(text[:start] + updated + ("" if nxt < 0 else text[nxt:]))


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


def _public_pane_number(workspace: dict[str, Any], raw_id: Any) -> int | None:
    numbers = workspace.get("public_pane_numbers") or {}
    candidates: list[Any] = [raw_id, str(raw_id)]
    try:
        as_int = int(raw_id)
    except (TypeError, ValueError):
        as_int = None
    if as_int is not None:
        candidates.extend((as_int, str(as_int)))
    for key in candidates:
        if key in numbers:
            return int(numbers[key])
    return None


DETECTED_AGENT = {
    "pi": "Pi",
    "kimi": "Kimi",
    "omp": "OhMyPi",
    "claude": "Claude",
    "gemini": "Gemini",
    "codex": "Codex",
    "cursor": "Cursor",
}

AGENT_STATE_NAME = {
    "working": "Working",
    "blocked": "Blocked",
    "idle": "Idle",
}


def follow_up_from_snapshot(
    snapshot: dict[str, Any], now: float | None = None
) -> list[dict[str, Any]]:
    now_unix = int(time.time() if now is None else now)
    claude = next(spec for spec in AGENTS if spec["name"] == "claude")
    added_at = now_unix - int(claude.get("age_secs", 26 * 60))
    for workspace in snapshot.get("workspaces", []):
        if workspace.get("custom_name") != "deploy":
            continue
        workspace_id = workspace.get("id")
        if not workspace_id:
            return []
        for tab in workspace.get("tabs", []):
            for raw_id, pane in (tab.get("panes") or {}).items():
                agent_name = pane.get("agent_name") or pane.get("label")
                if agent_name != "claude":
                    continue
                pane_number = _public_pane_number(workspace, raw_id)
                if pane_number is None:
                    return []
                return [
                    {
                        "workspace_id": workspace_id,
                        "pane_number": pane_number,
                        "added_at_unix_secs": added_at,
                    }
                ]
    return []


def semantic_snapshot(spec: dict[str, Any], now_unix: int) -> dict[str, Any]:
    state = spec.get("state", "idle")
    rust_state = AGENT_STATE_NAME[state]
    return {
        "detected_agent": DETECTED_AGENT[spec["name"]],
        "fallback_state": rust_state,
        "fallback_visible_blocker": state == "blocked",
        "fallback_visible_idle": state == "idle",
        "fallback_visible_working": state == "working",
        "state": rust_state,
        "revision": 1,
        "last_meaningful_agent_activity_unix_secs": now_unix - int(spec.get("age_secs", 0)),
    }


def patch_session_snapshot(
    snapshot: dict[str, Any],
    now: float | None = None,
    *,
    sidebar_collapsed: bool = False,
) -> dict[str, Any]:
    accents = {group["name"]: group["accent"] for group in GROUPS}
    icons = {group["name"]: group["icon"] for group in GROUPS}
    workspace_groups = {workspace["label"]: workspace["group"] for workspace in WORKSPACES}
    unseen = {agent["name"] for agent in AGENTS if not agent.get("seen", True)}
    specs = {spec["name"]: spec for spec in AGENTS}
    now_unix = int(time.time() if now is None else now)
    id_by_name = {group["name"]: group["id"] for group in snapshot.get("groups", [])}
    for group in snapshot.get("groups", []):
        accent = accents.get(group.get("name"))
        if accent:
            group["accent"] = accent
        icon = icons.get(group.get("name"))
        if icon:
            group["icon"] = icon
    for workspace in snapshot.get("workspaces", []):
        label = workspace.get("custom_name")
        group_name = workspace_groups.get(label)
        if group_name and group_name in id_by_name:
            workspace["group_id"] = id_by_name[group_name]
        for tab in workspace.get("tabs", []):
            for pane in tab.get("panes", {}).values():
                agent_name = pane.get("agent_name") or pane.get("label")
                pane["seen"] = agent_name not in unseen
                spec = specs.get(agent_name)
                if spec:
                    pane["terminal_semantics"] = semantic_snapshot(spec, now_unix)
    snapshot["group_filter_enabled"] = False
    snapshot["active_group"] = 0
    snapshot["sidebar_collapsed"] = sidebar_collapsed
    snapshot["right_sidebar_collapsed"] = sidebar_collapsed

    default_view = snapshot.setdefault("default_view", {})
    default_view["active"] = 0
    default_view["selected"] = 0
    default_view["sidebar_collapsed"] = sidebar_collapsed
    default_view["right_sidebar_collapsed"] = sidebar_collapsed
    default_view["agent_panel_scope"] = "AllWorkspaces"
    default_view["sidebar_section_split"] = 0.48
    snapshot["agent_follow_up"] = follow_up_from_snapshot(snapshot, now_unix)
    return snapshot



def apply_snapshot_patch(home: Path, *, sidebar_collapsed: bool = False) -> None:
    path = session_path(home)
    snapshot = json.loads(path.read_text())
    snapshot = patch_session_snapshot(snapshot, sidebar_collapsed=sidebar_collapsed)
    bindir = home / "bin"
    specs = {spec["name"]: spec for spec in AGENTS}

    for workspace in snapshot.get("workspaces", []):
        for tab in workspace.get("tabs", []):
            cart = tab.get("custom_name") == FOCUS_TAB
            for pane in (tab.get("panes") or {}).values():
                name = pane.get("agent_name") or pane.get("label")
                spec = specs.get(name)
                if spec:
                    pane["launch_argv"] = [str(bindir / spec["script"])]
                elif cart and not name:
                    pane["launch_argv"] = [str(bindir / LOGS_SCRIPT)]
    path.write_text(json.dumps(snapshot, indent=2) + "\n")


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

def focus_named_workspace_tab(bin_path: Path, home: Path, workspace: str, tab: str) -> None:
    workspaces = workspace_map(run_cli(bin_path, home, "workspace", "list"))
    target = workspaces[workspace]
    run_cli(bin_path, home, "workspace", "focus", target["workspace_id"])
    for row in run_cli(bin_path, home, "tab", "list")["result"]["tabs"]:
        if row["label"] == tab and row["workspace_id"] == target["workspace_id"]:
            run_cli(bin_path, home, "tab", "focus", row["tab_id"])
            return
    raise RuntimeError(f"demo tab not found: {workspace}/{tab}")



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


def report_demo_states(bin_path: Path, home: Path) -> None:
    agents = run_cli(bin_path, home, "agent", "list")["result"]["agents"]
    by_name = {agent.get("agent"): agent for agent in agents}
    now_unix = int(time.time())
    seq = int(time.time() * 1_000_000_000)
    for spec in AGENTS:
        row = by_name.get(spec["name"])
        if not row or not row.get("pane_id"):
            continue
        age = int(spec.get("age_secs", 0))
        try_cli(
            bin_path,
            home,
            "pane",
            "report-agent",
            str(row["pane_id"]),
            "--source",
            f"gardn:{spec['name']}",
            "--agent",
            spec["name"],
            "--state",
            spec["state"],
            "--seq",
            str(seq),
            "--activity-unix-secs",
            str(now_unix - age),
        )
        seq += 1


def prepare_demo_runtime(
    bin_path: Path,
    home: Path,
    *,
    sidebar_collapsed: bool = False,
) -> None:
    refresh_agents(bin_path, home)
    focus_showcase(bin_path, home)
    stop_server(bin_path, home)
    apply_snapshot_patch(home, sidebar_collapsed=sidebar_collapsed)
    start_server(bin_path, home)
    refresh_agents(bin_path, home)
    report_demo_states(bin_path, home)
    focus_showcase(bin_path, home)




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
    prepare_demo_runtime(bin_path, home)
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
        f"GARDN_DEMO_HOME={home}",
        f"XDG_CONFIG_HOME={home / 'config'}",
        f"XDG_STATE_HOME={home / 'state'}",
        "GARDN_SESSION=demo",
        str(bin_path),
        "--session",
        "demo",
    ]


def attach_wrapper_path(home: Path) -> Path:
    return home / "bin" / "demo-client"


def write_attach_wrapper(bin_path: Path, home: Path) -> Path:
    path = attach_wrapper_path(home)
    path.parent.mkdir(parents=True, exist_ok=True)
    unsets = " ".join(f"-u {key}" for key in NESTED_UNSETS)
    path.write_text(
        "#!/bin/sh\n"
        "set -eu\n"
        f"exec env {unsets} "
        f'GARDN_DEMO_HOME="{home}" '
        f'XDG_CONFIG_HOME="{home / "config"}" '
        f'XDG_STATE_HOME="{home / "state"}" '
        f"GARDN_SESSION={SESSION_NAME} "
        f'"{bin_path}" --session {SESSION_NAME}\n'
    )
    path.chmod(0o755)
    return path


def ghostty_theme_file(theme: str) -> Path:
    name = GHOSTTY_THEME_NAMES.get(theme)
    if name is None:
        raise ValueError(f"unknown demo theme: {theme!r}")
    path = FIXTURE_DIR / "ghostty-themes" / name
    if not path.is_file():
        raise FileNotFoundError(f"ghostty theme fixture missing: {path}")
    return path


def install_ghostty_themes(home: Path) -> Path:
    dest = home / "ghostty" / "themes"
    dest.mkdir(parents=True, exist_ok=True)
    for name in GHOSTTY_THEME_NAMES.values():
        shutil.copy2(FIXTURE_DIR / "ghostty-themes" / name, dest / name)
    return dest


def ghostty_config_path(home: Path) -> Path:
    return home / "ghostty-capture.config"




def write_ghostty_config(home: Path, wrapper: Path, theme: str = "day") -> Path:
    if theme not in HOST_TERMINAL:
        raise ValueError(f"unknown demo theme: {theme!r}")
    colors = HOST_TERMINAL[theme]
    install_ghostty_themes(home)
    path = ghostty_config_path(home)
    path.write_text(
        "\n".join(
            [
                "config-default-files = false",
                f"command = {wrapper}",
                f"title = {CAPTURE_WINDOW_TITLE}",
                'font-family = "Masakiro Mono"',
                f"font-size = {CAPTURE_WINDOW['font_size']}",
                f"window-width = {CAPTURE_WINDOW['columns']}",
                f"window-height = {CAPTURE_WINDOW['rows']}",
                "window-padding-x = 0",
                "window-padding-y = 0",
                "window-save-state = never",
                "window-decoration = none",
                "macos-window-shadow = false",
                "confirm-close-surface = false",
                "quit-after-last-window-closed = true",
                f"window-theme = {colors['window_theme']}",
                f"theme = {ghostty_theme_file(theme).resolve()}",
                "",
            ]
        )
    )
    return path



def list_ghostty_windows() -> list[dict[str, Any]]:
    script = r"""
import json
import Quartz

opts = Quartz.kCGWindowListOptionOnScreenOnly | Quartz.kCGWindowListExcludeDesktopElements
wins = Quartz.CGWindowListCopyWindowInfo(opts, Quartz.kCGNullWindowID) or []
rows = []
for window in wins:
    owner = str(window.get("kCGWindowOwnerName") or "")
    if "ghostty" not in owner.lower():
        continue
    bounds = window.get("kCGWindowBounds") or {}
    rows.append(
        {
            "name": str(window.get("kCGWindowName") or ""),
            "pid": int(window.get("kCGWindowOwnerPID") or 0),
            "x": int(bounds.get("X") or 0),
            "y": int(bounds.get("Y") or 0),
            "width": int(bounds.get("Width") or 0),
            "height": int(bounds.get("Height") or 0),
        }
    )
print(json.dumps(rows))
"""
    raw = subprocess.check_output(["/usr/bin/python3", "-c", script], text=True)
    return json.loads(raw)


def is_capture_window(window: dict[str, Any]) -> bool:
    return str(window.get("name") or "") == CAPTURE_WINDOW_TITLE


def demo_client_pids() -> set[int]:
    completed = subprocess.run(
        ["ps", "ax", "-o", "pid=,command="],
        check=True,
        capture_output=True,
        text=True,
    )
    pids: set[int] = set()
    for line in completed.stdout.splitlines():
        if "gardn" not in line or f"--session {SESSION_NAME}" not in line:
            continue
        pid_text, _, command = line.strip().partition(" ")
        if "server" in command.split():
            continue
        pids.add(int(pid_text))
    return pids


def descendant_pids(root: int) -> set[int]:
    completed = subprocess.run(
        ["ps", "ax", "-o", "pid=,ppid="],
        check=True,
        capture_output=True,
        text=True,
    )
    children: dict[int, list[int]] = {}
    for line in completed.stdout.splitlines():
        parts = line.split()
        if len(parts) != 2:
            continue
        pid, ppid = int(parts[0]), int(parts[1])
        children.setdefault(ppid, []).append(pid)
    found: set[int] = set()
    stack = [root]
    while stack:
        current = stack.pop()
        for child in children.get(current, ()):
            if child in found:
                continue
            found.add(child)
            stack.append(child)
    return found


def close_extra_demo_clients(keep: set[int]) -> None:
    for pid in sorted(demo_client_pids() - keep):
        subprocess.run(["kill", str(pid)], check=False)


def wait_for_capture_window(timeout: float = 8.0) -> dict[str, Any]:
    deadline = time.time() + timeout
    last: list[dict[str, Any]] = []
    while time.time() < deadline:
        last = list_ghostty_windows()
        for window in last:
            if is_capture_window(window):
                return window
        time.sleep(0.2)
    raise TimeoutError(f"demo capture window did not appear: {last}")


def wait_for_capture_window_closed(timeout: float = 4.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if not any(is_capture_window(window) for window in list_ghostty_windows()):
            return
        time.sleep(0.15)
    raise TimeoutError("demo capture window did not close")



def launch_capture_ghostty(config: Path) -> None:
    binary = GHOSTTY_APP / "Contents" / "MacOS" / "ghostty"
    env = os.environ.copy()
    env["XDG_CONFIG_HOME"] = str(config.parent)
    subprocess.Popen(
        [
            str(binary),
            "--config-default-files=false",
            f"--config-file={config}",
        ],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )


def _ns_running_app(pid: int, action: str) -> None:
    script = f"""
from AppKit import NSRunningApplication, NSApplicationActivateIgnoringOtherApps
app = NSRunningApplication.runningApplicationWithProcessIdentifier_({pid})
if app is None:
    raise SystemExit(1)
if {action!r} == "activate":
    app.activateWithOptions_(NSApplicationActivateIgnoringOtherApps)
elif {action!r} == "hide":
    app.hide()
elif {action!r} == "unhide":
    app.unhide()
"""
    subprocess.run(["/usr/bin/python3", "-c", script], check=False, capture_output=True, text=True)


def focus_capture_process(pid: int) -> None:
    _ns_running_app(pid, "activate")


def set_process_visible(pid: int, visible: bool) -> None:
    _ns_running_app(pid, "unhide" if visible else "hide")


def other_ghostty_windows(capture_pid: int) -> list[dict[str, Any]]:
    return [
        window
        for window in list_ghostty_windows()
        if int(window.get("pid") or 0) != capture_pid
    ]


def hide_other_ghostty(capture_pid: int) -> list[dict[str, Any]]:
    hidden = other_ghostty_windows(capture_pid)
    for pid in {int(window["pid"]) for window in hidden}:
        set_process_visible(pid, False)
    return hidden


def restore_other_ghostty(hidden: list[dict[str, Any]]) -> None:
    for pid in {int(window["pid"]) for window in hidden}:
        set_process_visible(pid, True)


def resize_capture_window(pid: int) -> None:
    script = f"""
tell application "System Events"
  set p to first process whose unix id is {pid}
  tell p
    set frontmost to true
    repeat with w in windows
      set position of w to {{{CAPTURE_WINDOW["x"]}, {CAPTURE_WINDOW["y"]}}}
      set size of w to {{{CAPTURE_WINDOW["width"]}, {CAPTURE_WINDOW["height"]}}}
    end repeat
  end tell
end tell
"""
    completed = subprocess.run(["osascript", "-e", script], check=False, capture_output=True, text=True)
    if completed.returncode != 0:
        raise RuntimeError(
            completed.stderr.strip() or completed.stdout.strip() or "cannot resize capture window"
        )

def raise_capture_window(window: dict[str, Any]) -> None:
    cliclick = shutil.which("cliclick") or "/opt/homebrew/bin/cliclick"
    x = int(window["x"]) + max(int(window["width"]), 1) // 2
    y = int(window["y"]) + max(int(window["height"]), 1) // 2
    subprocess.run([str(cliclick), f"c:{x},{y}"], check=False, capture_output=True, text=True)


def open_capture_window(bin_path: Path, home: Path, theme: str = "day") -> dict[str, Any]:
    start_server(bin_path, home)
    wrapper = write_attach_wrapper(bin_path, home)
    config = write_ghostty_config(home, wrapper, theme)
    close_stray_demo_windows()
    close_capture_window()
    wait_for_capture_window_closed()
    launch_capture_ghostty(config)
    existing = wait_for_capture_window()
    hidden = hide_other_ghostty(int(existing["pid"]))
    deadline = time.time() + 4
    keep: set[int] = set()
    while time.time() < deadline:
        keep = demo_client_pids() & descendant_pids(existing["pid"])
        if keep:
            break
        time.sleep(0.15)
    if keep:
        close_extra_demo_clients(keep)
    focus_capture_process(int(existing["pid"]))
    sized = wait_for_capture_window()
    for _ in range(6):
        if int(sized.get("width") or 0) >= 800:
            break
        try:
            resize_capture_window(int(sized["pid"]))
        except RuntimeError:
            pass
        time.sleep(0.3)
        sized = wait_for_capture_window()
    if int(sized.get("width") or 0) < 800:
        restore_other_ghostty(hidden)
        raise RuntimeError(f"capture window stayed too small: {sized}")
    return {**sized, "hidden_ghostty": hidden}








def close_capture_window() -> None:
    for window in list_ghostty_windows():
        if not is_capture_window(window):
            continue
        pid = int(window.get("pid") or 0)
        if pid <= 0:
            continue
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            continue


def close_stray_demo_windows() -> None:
    script = """
tell application "System Events"
  if not (exists process "Ghostty") then return
  tell process "Ghostty"
    repeat with w in windows
      if name of w is "~/projects" then
        try
          click button 1 of w
        end try
      end if
    end repeat
  end tell
end tell
"""
    subprocess.run(["osascript", "-e", script], check=False, capture_output=True, text=True)





def attach(bin_path: Path, home: Path) -> int:
    start_server(bin_path, home)
    env = attach_env(home)
    os.execvpe(str(bin_path), [str(bin_path), "--session", SESSION_NAME], env)
    return 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command",
        choices=("seed", "start", "stop", "status", "attach", "open-window", "print-attach"),
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
        print("attach\tjust demo-window")
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
    if args.command == "open-window":
        window = open_capture_window(bin_path, home)
        print(f"window\t{window['name']}\t{window['width']}x{window['height']}")
        print_status(bin_path, home)
        return 0
    if args.command == "attach":
        return attach(bin_path, home)
    return 2



if __name__ == "__main__":
    raise SystemExit(main())
