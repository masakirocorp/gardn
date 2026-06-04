#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import subprocess
import sys
from pathlib import Path


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


def gh(*args: str, check: bool = True) -> str:
    completed = run("gh", *args, check=check)
    return completed.stdout.strip()


def remote_url(name: str) -> str:
    return git("remote", "get-url", name)


def ensure_clean_worktree() -> None:
    if git("status", "--porcelain"):
        raise SystemExit("error: working tree is not clean")


def ensure_remotes() -> None:
    origin = remote_url("origin")
    upstream = remote_url("upstream")
    if "masakirocorp/hako" not in origin:
        raise SystemExit(f"error: origin must point to masakirocorp/hako, got {origin}")
    if "ogulcancelik/herdr" not in upstream:
        raise SystemExit(f"error: upstream must point to ogulcancelik/herdr, got {upstream}")


def ref_exists(ref: str) -> bool:
    return run("git", "rev-parse", "--verify", "--quiet", ref, check=False).returncode == 0


def branch_exists(branch: str) -> bool:
    return ref_exists(f"refs/heads/{branch}") or ref_exists(f"refs/remotes/origin/{branch}")


def is_ancestor(ancestor: str, descendant: str) -> bool:
    return run("git", "merge-base", "--is-ancestor", ancestor, descendant, check=False).returncode == 0


def upstream_commits(base_ref: str, upstream_ref: str) -> list[str]:
    output = git("log", "--oneline", f"{base_ref}..{upstream_ref}")
    return [line for line in output.splitlines() if line]


def write_pr_body(path: Path, branch: str, base_ref: str, upstream_ref: str, commits: list[str]) -> None:
    guard = Path("sync-report.md")
    guard_text = guard.read_text() if guard.exists() else "guard report not generated\n"
    status = run(
        "python3",
        "scripts/upstream_status.py",
        "--base",
        base_ref,
        "--upstream",
        upstream_ref,
        check=False,
    )
    status_text = status.stdout if status.stdout else status.stderr
    body = [
        "## Summary",
        f"- Merge `{upstream_ref}` into Hako on `{branch}`.",
        f"- Upstream commits: {len(commits)}.",
        "- Preserve Hako-owned identity, docs, website, release, and repo policy surfaces.",
        "- Treat upstream as signal, not authority: port behavior, not trust.",
        "- For each ported change: identify the invariant, check Hako context, add or adjust Hako tests, then merge.",
        "",
        "## Upstream commits",
    ]
    body.extend(f"- `{line}`" for line in commits)
    body.extend(
        [
            "",
            "## Verification",
            "- `python3 scripts/guard_upstream_sync.py --base " + base_ref + " --upstream " + upstream_ref + " --head HEAD`",
            "- `python3 scripts/upstream_status.py --base " + base_ref + " --upstream " + upstream_ref + " --check`",
            "- `just check` before merge",
            "",
            "## Guard report",
            guard_text,
            "",
            "## Upstream port status",
            status_text,
        ]
    )
    path.write_text("\n".join(body) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description="Create a Hako upstream sync branch and PR")
    parser.add_argument("--base", default="origin/master")
    parser.add_argument("--upstream", default="upstream/master")
    parser.add_argument("--date", default=dt.datetime.now(dt.UTC).strftime("%Y-%m-%d"))
    parser.add_argument("--branch")
    parser.add_argument("--no-pr", action="store_true", help="stop after creating the local sync branch")
    parser.add_argument("--no-push", action="store_true", help="stop before pushing")
    args = parser.parse_args()

    branch = args.branch or f"sync/upstream-{args.date}"

    ensure_clean_worktree()
    ensure_remotes()

    print("fetching origin and upstream")
    run("git", "fetch", "origin", "master", "--prune", check=True)
    run("git", "fetch", "upstream", "--prune", "--no-tags", check=True)

    if is_ancestor(args.upstream, args.base):
        print("no upstream changes to sync")
        return 0

    if branch_exists(branch):
        raise SystemExit(f"error: branch already exists: {branch}")

    commits = upstream_commits(args.base, args.upstream)

    print(f"creating {branch} from {args.base}")
    run("git", "switch", "--create", branch, args.base, check=True)

    print(f"merging {args.upstream}")
    merge = run(
        "git",
        "-c",
        "rerere.enabled=true",
        "-c",
        "rerere.autoupdate=false",
        "merge",
        "--no-ff",
        args.upstream,
        "-m",
        f"sync: merge upstream {args.date}",
        check=False,
    )
    if merge.returncode != 0:
        sys.stderr.write(merge.stdout)
        sys.stderr.write(merge.stderr)
        print("merge stopped for manual conflict resolution")
        print("after resolving, run:")
        print(f"  python3 scripts/guard_upstream_sync.py --base {args.base} --upstream {args.upstream} --head HEAD")
        return merge.returncode

    guard = run(
        "python3",
        "scripts/guard_upstream_sync.py",
        "--base",
        args.base,
        "--upstream",
        args.upstream,
        "--head",
        "HEAD",
        check=False,
    )
    sys.stdout.write(guard.stdout)
    sys.stderr.write(guard.stderr)
    if guard.returncode != 0:
        print("guard failed; fix policy violations before pushing")
        return guard.returncode

    pr_body = Path("sync-report-pr.md")
    write_pr_body(pr_body, branch, args.base, args.upstream, commits)

    if args.no_push:
        print(f"created local sync branch {branch}")
        return 0

    print(f"pushing {branch}")
    run("git", "push", "-u", "origin", branch, check=True)

    if args.no_pr:
        return 0

    print("creating PR")
    url = gh(
        "pr",
        "create",
        "--repo",
        "masakirocorp/hako",
        "--base",
        "master",
        "--head",
        branch,
        "--title",
        f"sync: upstream {args.date}",
        "--body-file",
        str(pr_body),
    )
    print(url)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
