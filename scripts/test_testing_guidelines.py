#!/usr/bin/env python3
"""Mechanical guardrails for Hako test guidelines.

This is intentionally narrow. It catches high-signal regressions that are easy to
spot mechanically; the testing guidelines still require human review for whether
an assertion is behavioral and refactor-resistant.
"""

from __future__ import annotations

import re
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]

RUST_TEST_FILES = [*sorted((ROOT / "tests").glob("*.rs")), *sorted((ROOT / "src").rglob("*.rs"))]


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def rust_test_functions(text: str) -> list[tuple[str, int, str]]:
    functions: list[tuple[str, int, str]] = []
    header = re.compile(r"#\[(?:tokio::)?test\]\s*(?:async\s+)?fn\s+(?P<name>\w+)\s*\([^)]*\)\s*\{")
    for match in header.finditer(text):
        depth = 1
        idx = match.end()
        while idx < len(text) and depth > 0:
            ch = text[idx]
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
            idx += 1
        line = text.count("\n", 0, match.start()) + 1
        functions.append((match.group("name"), line, text[match.end(): idx - 1]))
    return functions



class TestingGuidelineGuardrails(unittest.TestCase):
    def test_integration_tests_use_retrying_unix_socket_connect(self) -> None:
        """Socket path existence is not readiness; tests must connect through a wait helper."""
        offenders: list[str] = []
        direct_connect = re.compile(r"UnixStream::connect\([^\n]+\)\s*\.\s*(expect|unwrap)\(")
        for path in sorted((ROOT / "tests").glob("*.rs")):
            text = read(path)
            for match in direct_connect.finditer(text):
                line = text.count("\n", 0, match.start()) + 1
                offenders.append(f"{path.relative_to(ROOT)}:{line}: use support::connect_unix_socket")

        self.assertEqual([], offenders)

    def test_render_tests_assert_visible_output(self) -> None:
        """A render smoke test that only checks for panic is not a behavioral spec."""
        offenders: list[str] = []
        for path in RUST_TEST_FILES:
            text = read(path)
            for name, line, body in rust_test_functions(text):
                if ".draw(" not in body and "render_" not in body:
                    continue
                if "assert" in body or "expect(" in body and "render" not in name:
                    continue
                offenders.append(
                    f"{path.relative_to(ROOT)}:{line}: {name} renders without visible assertions"
                )

        self.assertEqual([], offenders)

    def test_protocol_error_tests_check_error_details(self) -> None:
        """Protocol boundary tests should pin useful error shape, not just that any error occurred."""
        offenders: list[str] = []
        path = ROOT / "src" / "protocol" / "wire.rs"
        text = read(path)
        for match in re.finditer(r"assert!\([^\n]+\.is_err\(\)[^\n]*\)", text):
            line = text.count("\n", 0, match.start()) + 1
            offenders.append(f"{path.relative_to(ROOT)}:{line}: assert concrete FramingError variant")

        self.assertEqual([], offenders)


if __name__ == "__main__":
    unittest.main()
