import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import scripts.changelog as changelog


class ChangelogTest(unittest.TestCase):
    def test_drain_inserts_sorted_pending_entries_and_removes_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            changes = root / ".changes"
            changes.mkdir()
            (root / "CHANGELOG.md").write_text("# Changelog\n\n## v0.1.0 - 2026-01-01\n\n- Old\n")
            (changes / "b.md").write_text("- Second\n")
            (changes / "a.md").write_text("- First\n")

            with patch.object(changelog, "CHANGELOG_PATH", root / "CHANGELOG.md"), patch.object(
                changelog, "CHANGES_DIR", changes
            ):
                updated = changelog.drain("0.2.0", "2026-07-06")

            self.assertEqual(
                updated,
                "# Changelog\n\n"
                "## v0.2.0 - 2026-07-06\n\n"
                "- First\n\n"
                "- Second\n\n"
                "## v0.1.0 - 2026-01-01\n\n"
                "- Old\n",
            )
            self.assertEqual((root / "CHANGELOG.md").read_text(), updated)
            self.assertFalse((changes / "a.md").exists())
            self.assertFalse((changes / "b.md").exists())

    def test_drain_keeps_changelog_when_no_pending_entries(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            body = "# Changelog\n\n## v0.1.0\n\n- Old"
            (root / "CHANGELOG.md").write_text(body)

            with patch.object(changelog, "CHANGELOG_PATH", root / "CHANGELOG.md"), patch.object(
                changelog, "CHANGES_DIR", root / ".changes"
            ):
                self.assertEqual(changelog.drain("0.2.0", "2026-07-06"), body)
                self.assertEqual((root / "CHANGELOG.md").read_text(), body)


if __name__ == "__main__":
    unittest.main()
