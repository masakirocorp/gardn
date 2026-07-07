#!/usr/bin/env python3
"""Drain pending changelog entries into CHANGELOG.md for a release."""

from __future__ import annotations

import argparse
import datetime as dt
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHANGELOG_PATH = ROOT / "CHANGELOG.md"
CHANGES_DIR = ROOT / ".changes"


def normalize(body: str) -> str:
    return "\n".join(line.rstrip() for line in body.splitlines()).strip()


def pending_entries(changes_dir: Path | None = None) -> list[tuple[Path, str]]:
    changes_dir = changes_dir or CHANGES_DIR
    if not changes_dir.exists():
        return []
    entries: list[tuple[Path, str]] = []
    for path in sorted(changes_dir.glob("*.md")):
        body = normalize(path.read_text())
        if body:
            entries.append((path, body))
    return entries


def changelog_body(path: Path | None = None) -> str:
    path = path or CHANGELOG_PATH
    if path.exists():
        return normalize(path.read_text())
    return "# Changelog"


def strip_title(body: str) -> str:
    prefix = "# Changelog"
    if body == prefix:
        return ""
    if body.startswith(prefix + "\n\n"):
        return body[len(prefix) + 2 :].strip()
    return body


def release_section(version: str, date: str, entries: list[tuple[Path, str]]) -> str:
    return f"## v{version} - {date}\n\n" + "\n\n".join(body for _, body in entries)


def drain(version: str, date: str, *, dry_run: bool = False) -> str:
    entries = pending_entries()
    base = changelog_body()
    if not entries:
        return base

    section = release_section(version, date, entries)
    rest = strip_title(base)
    updated = normalize("\n\n".join(part for part in ["# Changelog", section, rest] if part)) + "\n"

    if not dry_run:
        CHANGELOG_PATH.write_text(updated)
        for path, _ in entries:
            path.unlink()
    return updated


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("--date", default=dt.date.today().isoformat())
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    print(drain(args.version, args.date, dry_run=args.dry_run), end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
