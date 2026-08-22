#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

STATUSES = {"ported", "superseded", "skipped", "pending"}
PASSING_STATUSES = {"ported", "superseded", "skipped"}


@dataclass(frozen=True)
class UpstreamCommit:
    sha: str
    subject: str


def run(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [*args],
        check=check,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )


def git(*args: str, check: bool = True) -> str:
    completed = run("git", *args, check=check)
    return completed.stdout.strip()


def upstream_commits(base: str, upstream: str) -> list[UpstreamCommit]:
    output = git("log", "--reverse", "--format=%H%x00%s", f"{base}..{upstream}")
    commits: list[UpstreamCommit] = []
    for line in output.splitlines():
        if not line:
            continue
        sha, subject = line.split("\0", 1)
        commits.append(UpstreamCommit(sha=sha, subject=subject))
    return commits


def cherry_equivalent_commits(base: str, upstream: str) -> set[str]:
    output = git("cherry", "-v", base, upstream)
    equivalent: set[str] = set()
    for line in output.splitlines():
        if line.startswith("- "):
            parts = line.split(maxsplit=2)
            if len(parts) >= 2:
                equivalent.add(parts[1])
    return equivalent


def load_ledger(path: Path) -> dict[str, dict[str, object]]:
    if not path.exists():
        return {}
    data = json.loads(path.read_text())
    if not isinstance(data, dict) or not isinstance(data.get("entries"), list):
        raise ValueError(f"{path} must contain an object with an entries array")

    entries: dict[str, dict[str, object]] = {}
    for idx, raw in enumerate(data["entries"]):
        if not isinstance(raw, dict):
            raise ValueError(f"entries[{idx}] must be an object")
        upstream = raw.get("upstream")
        status = raw.get("status")
        reason = raw.get("reason")
        if not isinstance(upstream, str) or len(upstream) < 7:
            raise ValueError(f"entries[{idx}].upstream must be a git SHA or unique prefix")
        if status not in STATUSES:
            raise ValueError(f"entries[{idx}].status must be one of {sorted(STATUSES)}")
        if status in {"skipped", "superseded"} and not isinstance(reason, str):
            raise ValueError(f"entries[{idx}].reason is required for {status}")
        if upstream in entries:
            raise ValueError(f"duplicate ledger entry for {upstream}")
        entries[upstream] = raw
    return entries


def find_entry(entries: dict[str, dict[str, object]], sha: str) -> dict[str, object] | None:
    if sha in entries:
        return entries[sha]
    matches = [entry for prefix, entry in entries.items() if sha.startswith(prefix)]
    if len(matches) > 1:
        prefixes = ", ".join(str(match["upstream"]) for match in matches)
        raise ValueError(f"ambiguous ledger prefixes for {sha}: {prefixes}")
    return matches[0] if matches else None


def local_list(entry: dict[str, object] | None) -> str:
    if not entry:
        return "-"
    local = entry.get("local")
    if isinstance(local, list) and local:
        return ",".join(str(item)[:12] for item in local)
    if isinstance(local, str) and local:
        return local[:12]
    return "-"


def render_status(
    commits: list[UpstreamCommit],
    entries: dict[str, dict[str, object]],
    equivalent: set[str],
) -> tuple[str, list[UpstreamCommit], list[UpstreamCommit]]:
    lines = [
        "# upstream port status",
        "",
        "| upstream | status | local | subject | reason |",
        "|---|---|---|---|---|",
    ]
    unclassified: list[UpstreamCommit] = []
    pending: list[UpstreamCommit] = []

    for commit in commits:
        entry = find_entry(entries, commit.sha)
        if commit.sha in equivalent and entry is None:
            status = "ported"
            reason = "patch-equivalent in base"
            local = "patch-id"
        elif entry is not None:
            status = str(entry["status"])
            reason = str(entry.get("reason", ""))
            local = local_list(entry)
        else:
            status = "unclassified"
            reason = "missing from upstream-port-map.json"
            local = "-"
            unclassified.append(commit)

        if status == "pending":
            pending.append(commit)

        lines.append(
            f"| `{commit.sha[:12]}` | {status} | {local} | {commit.subject} | {reason} |"
        )

    return "\n".join(lines) + "\n", unclassified, pending


def main() -> int:
    parser = argparse.ArgumentParser(description="Report Gardn's upstream port ledger status")
    parser.add_argument("--base", default="origin/master")
    parser.add_argument("--upstream", default="upstream/master")
    parser.add_argument("--ledger", default="upstream-port-map.json")
    parser.add_argument("--check", action="store_true", help="fail on unclassified or pending upstream commits")
    args = parser.parse_args()

    commits = upstream_commits(args.base, args.upstream)
    entries = load_ledger(Path(args.ledger))
    equivalent = cherry_equivalent_commits(args.base, args.upstream)
    report, unclassified, pending = render_status(commits, entries, equivalent)
    print(report, end="")

    unknown_entries = [prefix for prefix in entries if not any(commit.sha.startswith(prefix) for commit in commits)]
    if unknown_entries:
        print(
            "unknown ledger upstream entries: " + ", ".join(sorted(unknown_entries)),
            file=sys.stderr,
        )
        if args.check:
            return 1

    if args.check and (unclassified or pending):
        if unclassified:
            print(
                "unclassified upstream commits: "
                + ", ".join(commit.sha[:12] for commit in unclassified),
                file=sys.stderr,
            )
        if pending:
            print(
                "pending upstream commits: " + ", ".join(commit.sha[:12] for commit in pending),
                file=sys.stderr,
            )
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
