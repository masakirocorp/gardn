#!/usr/bin/env python3

from pathlib import Path
import unittest


class NixSkillFilesetTest(unittest.TestCase):
    def test_nix_source_includes_root_skill_md(self) -> None:
        project_root = Path(__file__).resolve().parent.parent
        package_nix = (project_root / "nix" / "package.nix").read_text()
        self.assertIn("../SKILL.md", package_nix)
        self.assertTrue(
            (project_root / "SKILL.md").is_file(),
            "repo-root SKILL.md must exist for gardn --skill and the Nix fileset",
        )


if __name__ == "__main__":
    unittest.main()
