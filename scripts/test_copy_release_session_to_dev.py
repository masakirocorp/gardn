#!/usr/bin/env python3
"""Copy release session files into the gardn-dev namespace."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts.copy_release_session_to_dev import (
    copy_release_session_to_dev,
    iter_copy_relpaths,
)


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


class CopyReleaseSessionToDevTests(unittest.TestCase):
    def test_copies_session_config_and_named_sessions(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            source = Path(raw) / "gardn"
            dest = Path(raw) / "gardn-dev"
            write(source / "session.json", '{"workspaces":[]}')
            write(source / "session-history.json", '{"panes":[]}')
            write(source / "config.toml", 'theme = "night"\n')
            write(source / "plugins.json", "[]")
            write(source / "ssh-profiles.json", "{}")
            write(source / "sessions" / "work" / "session.json", '{"name":"work"}')
            write(
                source / "sessions" / "work" / "session-history.json",
                '{"panes":[]}',
            )
            write(source / "gardn-server.log", "noise")
            write(source / "gardn.sock", "")
            write(source / "installation-id", "release-id")
            write(source / "release-notes.json", "{}")
            write(dest / "session.json", '{"stale":true}')
            write(dest / "gardn-dev-only.log", "keep")

            copied = copy_release_session_to_dev(source, dest)

            self.assertEqual(
                copied,
                [
                    Path("session.json"),
                    Path("session-history.json"),
                    Path("config.toml"),
                    Path("plugins.json"),
                    Path("ssh-profiles.json"),
                    Path("sessions") / "work" / "session.json",
                    Path("sessions") / "work" / "session-history.json",
                ],
            )
            self.assertEqual((dest / "session.json").read_text(), '{"workspaces":[]}')
            self.assertEqual((dest / "config.toml").read_text(), 'theme = "night"\n')
            self.assertEqual(
                (dest / "sessions" / "work" / "session.json").read_text(),
                '{"name":"work"}',
            )
            self.assertFalse((dest / "gardn-server.log").exists())
            self.assertFalse((dest / "gardn.sock").exists())
            self.assertFalse((dest / "installation-id").exists())
            self.assertEqual((dest / "gardn-dev-only.log").read_text(), "keep")

    def test_dry_inventory_skips_runtime_files(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            source = Path(raw) / "gardn"
            write(source / "session.json", "{}")
            write(source / "gardn-client.log", "log")
            write(source / ".session.json.lock", "lock")
            self.assertEqual(iter_copy_relpaths(source), [Path("session.json")])

    def test_missing_source_raises(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            source = Path(raw) / "missing"
            dest = Path(raw) / "gardn-dev"
            with self.assertRaises(FileNotFoundError):
                copy_release_session_to_dev(source, dest)
