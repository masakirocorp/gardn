#!/usr/bin/env python3
from __future__ import annotations

import argparse
import fnmatch
import json
import subprocess
import sys
from pathlib import Path


HARD_REQUIRED = {
    "apps/gardn/Cargo.toml": [
        'name = "gardn"',
        'repository = "https://github.com/masakirocorp/gardn"',
    ],
    ".github/workflows/release.yml": [
        "gardn-linux-x86_64",
        "gardn-linux-aarch64",
        "gardn-macos-x86_64",
        "gardn-macos-aarch64",
    ],
}

OPTIONAL_REQUIRED = {
    "apps/gardn/src/update.rs": [
        "https://api.github.com/repos/masakirocorp/gardn/releases/latest",
        "masakirocorp/gardn",
        "gardn update",
    ],
}

GARDN_OWNED = [
    "README.md",
    "AGENTS.md",
    "SKILL.md",
    "apps/gardn/assets/logo.svg",
    "docs/**",
    "website/**",
]

REVIEW_REQUIRED = [
    "apps/gardn/Cargo.toml",
    "Cargo.lock",
    "apps/gardn/build.rs",
    "justfile",
    ".github/workflows/**",
    "apps/gardn/src/update.rs",
    "apps/gardn/src/release_notes.rs",
    "apps/gardn/src/ui/release_notes.rs",
    "apps/gardn/vendor/libghostty-vt/**",
    "scripts/vendor_libghostty_vt.py",
    "scripts/build_vendored_libghostty_vt.sh",
    "scripts/capture_keys.py",
    "scripts/capture_key_matrix.py",
    "scripts/verify_suspicious_keys.py",
]

FORBIDDEN_RESURRECTIONS = [
    ".pi/**",
    ".zed/**",
    ".github/APPROVED_CONTRIBUTORS",
    ".github/ISSUE_TEMPLATE/**",
    ".github/dependabot.yml",
    ".github/workflows/build-artifacts-manual.yml",
    ".github/workflows/label-intends-to-pr.yml",
    ".github/workflows/label-next-release-issues.yml",
    "scripts/changelog.py",
    "scripts/test_changelog.py",
]

IDENTITY_FORBIDDEN = [
    "GARDN_",
    "gardn-dev",
    'name = "gardn"',
    "masakirocorp/gardn",
    "https://gardn.dev",
    "http://gardn.dev",
]

ATTRIBUTION_ALLOWED = {
    "AGENTS.md",
    "README.md",
}

IGNORED_IDENTITY_PATHS = [
    "apps/gardn/vendor/libghostty-vt/**",
]


class GitError(RuntimeError):
    pass


def git(*args: str, check: bool = True) -> str:
    completed = subprocess.run(
        ["git", *args],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if check and completed.returncode != 0:
        raise GitError(completed.stderr.strip() or completed.stdout.strip())
    return completed.stdout


def matches(path: str, patterns: list[str]) -> bool:
    return any(fnmatch.fnmatch(path, pattern) for pattern in patterns)


def changed_name_status(base: str, head: str) -> list[tuple[str, str]]:
    output = git("diff", "--name-status", f"{base}..{head}")
    rows: list[tuple[str, str]] = []
    for line in output.splitlines():
        if not line:
            continue
        parts = line.split("\t")
        status = parts[0]
        path = parts[-1]
        rows.append((status, path))
    return rows


def file_text_at(ref: str, path: str) -> str | None:
    output = git("show", f"{ref}:{path}", check=False)
    if not output:
        return None
    return output


def branch_exists(ref: str) -> bool:
    return subprocess.run(
        ["git", "rev-parse", "--verify", "--quiet", ref],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    ).returncode == 0


def append_unique(items: list[str], item: str) -> None:
    if item not in items:
        items.append(item)


def upstream_port_status(base: str, upstream: str) -> tuple[int, str]:
    ledger = Path("upstream-port-map.json")
    script = Path("scripts/upstream_status.py")
    if not ledger.exists() or not script.exists():
        return 0, "upstream port ledger not configured\n"
    completed = subprocess.run(
        [
            "python3",
            str(script),
            "--base",
            base,
            "--upstream",
            upstream,
            "--ledger",
            str(ledger),
            "--check",
        ],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return completed.returncode, completed.stdout + completed.stderr


def build_markdown_report(report: dict[str, object]) -> str:
    failures = report["failures"]
    review_required = report["review_required"]
    changed = report["changed"]

    lines = [
        "# upstream sync report",
        "",
        f"base: `{report['base']}`",
        f"upstream: `{report['upstream']}`",
        f"head: `{report['head']}`",
        "",
        f"changed paths: {len(changed)}",
        "",
        "## port review discipline",
        "- Treat upstream as signal, not authority: port behavior, not trust.",
        "- For every ported change, identify the invariant, check Gardn context, add or adjust Gardn tests, then merge.",
        "",
        "## failures",
    ]
    if failures:
        lines.extend(f"- {failure}" for failure in failures)  # type: ignore[union-attr]
    else:
        lines.append("- none")

    lines.extend(["", "## human review required"])
    if review_required:
        lines.extend(f"- {item}" for item in review_required)  # type: ignore[union-attr]
    else:
        lines.append("- none")

    lines.extend(["", "## upstream port status"])
    port_status = report.get("upstream_port_status")
    if port_status:
        lines.append(str(port_status).rstrip())
    else:
        lines.append("- not checked")

    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate Gardn upstream-sync policy")
    parser.add_argument("--base", default="origin/master")
    parser.add_argument("--upstream", default="upstream/master")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--report-json", default="sync-report.json")
    parser.add_argument("--report-md", default="sync-report.md")
    args = parser.parse_args()

    failures: list[str] = []
    review_required: list[str] = []

    for ref in [args.base, args.upstream, args.head]:
        if not branch_exists(ref):
            failures.append(f"missing git ref: {ref}")

    unmerged = [line for line in git("diff", "--name-only", "--diff-filter=U").splitlines() if line]
    if unmerged:
        failures.append("unmerged paths remain: " + ", ".join(unmerged))

    rows = changed_name_status(args.base, args.head) if not failures else []
    changed = [path for _status, path in rows]

    for status, path in rows:
        if matches(path, FORBIDDEN_RESURRECTIONS):
            failures.append(f"removed upstream process plumbing resurrected: {status} {path}")

        if matches(path, GARDN_OWNED):
            append_unique(
                review_required,
                f"Gardn-owned path changed; preserve/review manually: {status} {path}",
            )
            if status.startswith("D") and (path.startswith("docs/") or path.startswith("website/")):
                failures.append(f"Gardn docs/site path deleted: {path}")

        if matches(path, REVIEW_REQUIRED):
            append_unique(review_required, f"review-required path changed: {status} {path}")

    for path, tokens in HARD_REQUIRED.items():
        text = file_text_at(args.head, path)
        if text is None:
            failures.append(f"required Gardn identity file missing: {path}")
            continue
        for token in tokens:
            if token not in text:
                failures.append(f"missing required Gardn token in {path}: {token}")

    for path, tokens in OPTIONAL_REQUIRED.items():
        text = file_text_at(args.head, path)
        if text is None:
            append_unique(review_required, f"optional Gardn identity file missing: {path}")
            continue
        for token in tokens:
            if token not in text:
                append_unique(review_required, f"review Gardn updater token in {path}: {token}")

    for path in changed:
        if matches(path, IGNORED_IDENTITY_PATHS):
            continue
        text = file_text_at(args.head, path)
        if text is None:
            continue
        for token in IDENTITY_FORBIDDEN:
            if token in text and path not in ATTRIBUTION_ALLOWED:
                failures.append(f"possible runtime identity regression in {path}: {token}")

    port_status_code = 0
    port_status = ""
    if not failures:
        port_status_code, port_status = upstream_port_status(args.base, args.upstream)
        if port_status_code != 0:
            failures.append("upstream port ledger has unclassified or pending commits")
    report: dict[str, object] = {
        "base": args.base,
        "upstream": args.upstream,
        "head": args.head,
        "changed": changed,
        "review_required": review_required,
        "upstream_port_status": port_status,
        "failures": failures,
    }
    Path(args.report_json).write_text(json.dumps(report, indent=2) + "\n")
    Path(args.report_md).write_text(build_markdown_report(report))

    if failures:
        print(Path(args.report_md).read_text(), file=sys.stderr)
        return 1

    print(Path(args.report_md).read_text())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
