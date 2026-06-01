from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts import upstream_status


class UpstreamStatusTests(unittest.TestCase):
    def test_check_reports_unclassified_commits(self) -> None:
        commits = [
            upstream_status.UpstreamCommit("a" * 40, "fix: one"),
            upstream_status.UpstreamCommit("b" * 40, "fix: two"),
        ]
        entries = {
            "a" * 40: {
                "upstream": "a" * 40,
                "status": "skipped",
                "reason": "docs-only",
            }
        }

        report, unclassified, pending = upstream_status.render_status(commits, entries, set())

        self.assertIn("fix: one", report)
        self.assertIn("fix: two", report)
        self.assertEqual([commit.sha for commit in unclassified], ["b" * 40])
        self.assertEqual(pending, [])

    def test_patch_equivalent_commit_is_ported_without_ledger_entry(self) -> None:
        commit = upstream_status.UpstreamCommit("c" * 40, "fix: equivalent")

        report, unclassified, pending = upstream_status.render_status([commit], {}, {commit.sha})

        self.assertIn("patch-equivalent in base", report)
        self.assertEqual(unclassified, [])
        self.assertEqual(pending, [])

    def test_skipped_and_superseded_entries_require_reasons(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            ledger = Path(temp_dir) / "ledger.json"
            ledger.write_text(
                '{"entries":[{"upstream":"abcdef1","status":"skipped"}]}\n'
            )

            with self.assertRaisesRegex(ValueError, "reason is required"):
                upstream_status.load_ledger(ledger)

    def test_short_prefix_entry_matches_full_sha(self) -> None:
        commit = upstream_status.UpstreamCommit("abcdef1234567890", "fix: adapted")
        entries = {
            "abcdef1": {
                "upstream": "abcdef1",
                "status": "ported",
                "local": ["1234567890abcdef"],
            }
        }

        report, unclassified, pending = upstream_status.render_status([commit], entries, set())

        self.assertIn("1234567890ab", report)
        self.assertEqual(unclassified, [])
        self.assertEqual(pending, [])


if __name__ == "__main__":
    unittest.main()
