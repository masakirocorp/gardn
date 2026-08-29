#!/usr/bin/env python3
"""Copy persisted Gardn release-session files into the gardn-dev namespace."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

GLOBAL_FILES = (
    "session.json",
    "session-history.json",
    "config.toml",
    "plugins.json",
    "ssh-profiles.json",
)
NAMED_SESSION_FILES = (
    "session.json",
    "session-history.json",
)


def config_home() -> Path:
    xdg = os.environ.get("XDG_CONFIG_HOME")
    if xdg:
        return Path(xdg)
    return Path.home() / ".config"


def release_dir(home: Path | None = None) -> Path:
    return (home or config_home()) / "gardn"


def dev_dir(home: Path | None = None) -> Path:
    return (home or config_home()) / "gardn-dev"


def iter_copy_relpaths(source: Path) -> list[Path]:
    paths: list[Path] = []
    for name in GLOBAL_FILES:
        candidate = source / name
        if candidate.is_file():
            paths.append(Path(name))
    sessions = source / "sessions"
    if sessions.is_dir():
        for child in sorted(sessions.iterdir()):
            if not child.is_dir():
                continue
            for name in NAMED_SESSION_FILES:
                candidate = child / name
                if candidate.is_file():
                    paths.append(Path("sessions") / child.name / name)
    return paths


def copy_release_session_to_dev(source: Path, dest: Path) -> list[Path]:
    if not source.is_dir():
        raise FileNotFoundError(f"release session directory not found: {source}")
    copied = iter_copy_relpaths(source)
    if not copied:
        raise FileNotFoundError(f"no session files to copy from {source}")
    dest.mkdir(parents=True, exist_ok=True)
    for rel in copied:
        target = dest / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source / rel, target)
    return copied


def stop_dev_server(dev_binary: Path) -> None:
    if not dev_binary.is_file():
        return
    env = os.environ.copy()
    env.pop("GARDN_SOCKET_PATH", None)
    env.pop("GARDN_CLIENT_SOCKET_PATH", None)
    subprocess.run(
        [str(dev_binary), "server", "stop"],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Copy ~/.config/gardn session state into ~/.config/gardn-dev."
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print files that would be copied and exit",
    )
    parser.add_argument(
        "--skip-stop",
        action="store_true",
        help="do not stop the gardn-dev server before copying",
    )
    args = parser.parse_args(argv)

    source = release_dir()
    dest = dev_dir()
    if not source.is_dir():
        print(f"release session directory not found: {source}", file=sys.stderr)
        return 1
    relpaths = iter_copy_relpaths(source)
    if not relpaths:
        print(f"no session files to copy from {source}", file=sys.stderr)
        return 1

    if args.dry_run:
        for rel in relpaths:
            print(f"{source / rel} -> {dest / rel}")
        return 0

    if not args.skip_stop:
        stop_dev_server(Path.home() / ".local" / "bin" / "gardn-dev")

    try:
        copied = copy_release_session_to_dev(source, dest)
    except FileNotFoundError as error:
        print(error, file=sys.stderr)
        return 1
    for rel in copied:
        print(f"copied {rel}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
