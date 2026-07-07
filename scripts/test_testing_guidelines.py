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

RUST_TEST_FILES = [*sorted((ROOT / "apps" / "hako" / "tests").glob("*.rs")), *sorted((ROOT / "apps" / "hako" / "src").rglob("*.rs"))]


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



def body_without_draw_expectations(body: str) -> str:
    """Keep assertion detection from treating render draw plumbing as behavior."""
    return "\n".join(
        line
        for line in body.splitlines()
        if not (".draw(" in line and ".expect(" in line)
    )



class TestingGuidelineGuardrails(unittest.TestCase):
    def test_integration_tests_use_retrying_unix_socket_connect(self) -> None:
        """Socket path existence is not readiness; tests must connect through a wait helper."""
        offenders: list[str] = []
        direct_connect = re.compile(r"UnixStream::connect\([^\n]+\)\s*\.\s*(expect|unwrap)\(")
        for path in sorted((ROOT / "apps" / "hako" / "tests").glob("*.rs")):
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
                behavioral_body = body_without_draw_expectations(body)
                if "assert" in behavioral_body or (
                    "expect(" in behavioral_body and "render" not in name
                ):
                    continue
                offenders.append(
                    f"{path.relative_to(ROOT)}:{line}: {name} renders without visible assertions"
                )

        self.assertEqual([], offenders)

    def test_env_mutation_uses_raii_guards(self) -> None:
        """Process-wide env changes in tests must restore through Drop, not after assertions."""
        offenders: list[str] = []
        env_mutation = re.compile(r"(?:std::)?env::(?:set_var|remove_var)\(")
        for path in RUST_TEST_FILES:
            text = read(path)
            for name, line, body in rust_test_functions(text):
                if env_mutation.search(body):
                    offenders.append(
                        f"{path.relative_to(ROOT)}:{line}: {name} mutates env directly; use TestEnvVar"
                    )

        self.assertEqual([], offenders)

    def test_tests_do_not_assert_only_no_panic(self) -> None:
        """No-panic smoke tests do not specify behavior."""
        offenders: list[str] = []
        smoke_text = re.compile(r"no assertions|no panic|just no panic", re.IGNORECASE)
        for path in RUST_TEST_FILES:
            text = read(path)
            for name, line, body in rust_test_functions(text):
                if smoke_text.search(body):
                    offenders.append(
                        f"{path.relative_to(ROOT)}:{line}: {name} documents no behavioral assertion"
                    )

        self.assertEqual([], offenders)


    def test_protocol_error_tests_check_error_details(self) -> None:
        """Protocol boundary tests should pin useful error shape, not just that any error occurred."""
        offenders: list[str] = []
        path = ROOT / "apps" / "hako" / "src" / "protocol" / "wire.rs"
        text = read(path)
        for match in re.finditer(r"assert!\([^\n]+\.is_err\(\)[^\n]*\)", text):
            line = text.count("\n", 0, match.start()) + 1
            offenders.append(f"{path.relative_to(ROOT)}:{line}: assert concrete FramingError variant")

        self.assertEqual([], offenders)

    def test_protocol_framing_keeps_golden_byte_fixtures(self) -> None:
        """Wire compatibility needs explicit bytes, not only encode/decode round trips."""
        path = ROOT / "apps" / "hako" / "src" / "protocol" / "wire.rs"
        text = read(path)
        bodies = {name: body for name, _line, body in rust_test_functions(text)}
        required = [
            "framing_small_message_roundtrip",
            "client_hello_framing_matches_golden_fixture",
            "server_welcome_framing_matches_golden_fixture",
        ]
        missing = [name for name in required if name not in bodies]
        self.assertEqual([], missing)
        for name in required:
            body = bodies[name]
            self.assertIn("write_message", body, name)
            self.assertIn("read_message", body, name)
            self.assertIn("assert_eq!", body, name)
            self.assertTrue("expected" in body or "&[" in body, name)


if __name__ == "__main__":
    unittest.main()
