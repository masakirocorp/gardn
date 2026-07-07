#!/usr/bin/env python3
from __future__ import annotations

import argparse
import sys
from pathlib import Path


def extract_release_notes(changelog: str, package: str, version: str) -> str:
    normalized_version = version.removeprefix("v")
    heading = f"## {package}@{normalized_version}"
    lines = changelog.splitlines()

    try:
        start = next(i for i, line in enumerate(lines) if line.strip() == heading)
    except StopIteration as exc:
        raise ValueError(f"missing changelog section: {heading}") from exc

    end = len(lines)
    for i in range(start + 1, len(lines)):
        if lines[i].startswith("## "):
            end = i
            break

    body = "\n".join(lines[start + 1 : end]).strip()
    if not body:
        raise ValueError(f"empty changelog section: {heading}")
    return body


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Extract one package/version section from a Tegami changelog")
    parser.add_argument("changelog", type=Path)
    parser.add_argument("package")
    parser.add_argument("version")
    args = parser.parse_args(argv)

    try:
        body = extract_release_notes(args.changelog.read_text(encoding="utf-8"), args.package, args.version)
    except (OSError, ValueError) as exc:
        print(exc, file=sys.stderr)
        return 1

    print(body)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
