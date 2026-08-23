from __future__ import annotations

import re
import subprocess
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]

# Build retired names from fragments so this guard can inspect itself.
RETIRED_UPSTREAM_NAME = "her" + "dr"
RETIRED_DISPLAY_NAME = "Oh My " + RETIRED_UPSTREAM_NAME.title()
RETIRED_SLUG = "oh-my-" + RETIRED_UPSTREAM_NAME
RETIRED_DOMAIN = "ohmy" + RETIRED_UPSTREAM_NAME + ".com"
RETIRED_CODENAME = "ha" + "ko"
RETIRED_CLI = "o" + "mh"

ALLOWED_PROVENANCE = (
    "https://github.com/ogulcancelik/" + RETIRED_UPSTREAM_NAME,
    "ogulcancelik/" + RETIRED_UPSTREAM_NAME,
    "herdrdev/" + RETIRED_UPSTREAM_NAME,
)

ALLOWED_GUARD_LITERALS = (
    "HER" + "DR_",
    RETIRED_UPSTREAM_NAME + "-dev",
    'name = "' + RETIRED_UPSTREAM_NAME + '"',
    "masakirocorp/" + RETIRED_UPSTREAM_NAME,
    "https://" + RETIRED_UPSTREAM_NAME + ".dev",
    "http://" + RETIRED_UPSTREAM_NAME + ".dev",
)

FORBIDDEN_CONTENT = (
    ("retired display name", re.compile(re.escape(RETIRED_DISPLAY_NAME))),
    ("retired repository slug", re.compile(re.escape(RETIRED_SLUG))),
    ("retired website domain", re.compile(re.escape(RETIRED_DOMAIN))),
    ("retired CLI name", re.compile(rf"(?<![A-Za-z0-9_]){RETIRED_CLI}(?![A-Za-z0-9_])", re.IGNORECASE)),
    ("retired environment prefix", re.compile("O" + "MH_")),
    ("retired upstream environment prefix", re.compile("HER" + "DR_")),
    ("retired standalone upstream name", re.compile(rf"(?<![A-Za-z0-9_]){RETIRED_UPSTREAM_NAME}(?![A-Za-z0-9_])", re.IGNORECASE)),
    ("retired codename", re.compile(rf"(?<![A-Za-z0-9_]){RETIRED_CODENAME}(?![A-Za-z0-9_])", re.IGNORECASE)),
)


def tracked_paths() -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=REPO_ROOT,
        check=True,
        stdout=subprocess.PIPE,
    )
    return [REPO_ROOT / path.decode() for path in result.stdout.split(b"\0") if path]


def scrub_allowed_provenance(path: Path, text: str) -> str:
    for allowed in ALLOWED_PROVENANCE:
        text = text.replace(allowed, "")
    if path == REPO_ROOT / "scripts/guard_upstream_sync.py":
        for allowed in ALLOWED_GUARD_LITERALS:
            text = text.replace(allowed, "")
    return text


class BrandIdentityTests(unittest.TestCase):
    def test_tracked_paths_use_gardn_identity(self) -> None:
        forbidden_path_parts = (
            "apps/" + RETIRED_CLI,
            "crates/" + RETIRED_CLI,
            RETIRED_SLUG,
            RETIRED_DOMAIN,
        )
        violations = [
            path.relative_to(REPO_ROOT).as_posix()
            for path in tracked_paths()
            if any(part in path.relative_to(REPO_ROOT).as_posix().lower() for part in forbidden_path_parts)
        ]
        self.assertEqual(violations, [], "tracked paths retain retired product identity")

    def test_tracked_text_uses_gardn_identity(self) -> None:
        violations: list[str] = []
        for path in tracked_paths():
            if not path.is_file():
                continue
            try:
                text = path.read_text(encoding="utf-8")
            except (UnicodeDecodeError, IsADirectoryError):
                continue

            text = scrub_allowed_provenance(path, text)
            for label, pattern in FORBIDDEN_CONTENT:
                for match in pattern.finditer(text):
                    line = text.count("\n", 0, match.start()) + 1
                    relative_path = path.relative_to(REPO_ROOT).as_posix()
                    violations.append(f"{relative_path}:{line}: {label}")

        self.assertEqual(violations, [], "\n" + "\n".join(violations))


if __name__ == "__main__":
    unittest.main()
