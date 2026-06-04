import json
import tempfile
import unittest
from pathlib import Path

import scripts.preview as preview


class PreviewNotesTests(unittest.TestCase):
    def test_humanize_groups_conventional_subjects(self):
        self.assertEqual(
            preview.humanize_subject("feat(update): add preview channel"),
            ("Added", "Add preview channel"),
        )
        self.assertEqual(
            preview.humanize_subject("fix: handle preview manifest"),
            ("Fixed", "Handle preview manifest"),
        )
        self.assertEqual(
            preview.humanize_subject("not conventional"),
            ("Other", "Not conventional"),
        )

    def test_build_manifest_archives_current_assets(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "preview.json"
            content = preview.build_manifest(
                output=output,
                repo="masakirocorp/hako",
                tag="preview-2026-06-02-abcdef123456",
                build_id="2026-06-02-abcdef123456",
                commit="abcdef1234567890",
                built_at="2026-06-02T03:00:00Z",
                base_version="0.6.6",
                protocol=12,
                notes="Preview notes\n",
                shas={"linux-x86_64": "deadbeef"},
                retain=30,
            )
            data = json.loads(content)
            self.assertEqual(data["channel"], "preview")
            self.assertEqual(data["build_id"], "2026-06-02-abcdef123456")
            self.assertEqual(data["assets"]["linux-x86_64"]["sha256"], "deadbeef")
            self.assertEqual(
                data["assets"]["linux-x86_64"]["url"],
                "https://github.com/masakirocorp/hako/releases/download/preview-2026-06-02-abcdef123456/hako-linux-x86_64",
            )
            self.assertIn("2026-06-02-abcdef123456", data["builds"])

    def test_hidden_subjects_include_preview_manifest_commits(self):
        self.assertTrue(preview.hidden_subject("docs: update preview manifest"))
        self.assertFalse(preview.hidden_subject("fix: repair preview manifest"))


if __name__ == "__main__":
    unittest.main()
