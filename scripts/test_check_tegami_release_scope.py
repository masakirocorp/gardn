from __future__ import annotations

import os
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


class CheckTegamiReleaseScopeTests(unittest.TestCase):
    def test_accepts_pending_changefiles_that_include_required_package(self) -> None:
        repo_root = Path(__file__).resolve().parents[1]
        script = repo_root / "scripts" / "check-tegami-release-scope.mts"

        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["apps/gardn"]\nresolver = "2"\n',
                encoding="utf-8",
            )
            app_dir = root / "apps" / "gardn"
            app_dir.mkdir(parents=True)
            (app_dir / "Cargo.toml").write_text(
                '[package]\nname = "gardn"\nversion = "0.1.0"\nedition = "2021"\n',
                encoding="utf-8",
            )
            changelog_dir = root / ".tegami"
            changelog_dir.mkdir()
            (changelog_dir / "feature.md").write_text(
                textwrap.dedent(
                    """\
                    ---
                    packages:
                      gardn: patch
                    ---

                    ### Add release notes menu

                    Users can open release notes from the app menu.
                    """
                ),
                encoding="utf-8",
            )

            result = subprocess.run(
                ["node", str(script), "gardn"],
                cwd=root,
                env={**os.environ, "NO_COLOR": "1"},
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=20,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "tegami release scope ok: all changefiles include gardn")
        self.assertEqual(result.stderr, "")

    def test_rejects_release_worthy_changefiles_missing_required_package(self) -> None:
        repo_root = Path(__file__).resolve().parents[1]
        script = repo_root / "scripts" / "check-tegami-release-scope.mts"

        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["apps/gardn"]\nresolver = "2"\n',
                encoding="utf-8",
            )
            app_dir = root / "apps" / "gardn"
            app_dir.mkdir(parents=True)
            (app_dir / "Cargo.toml").write_text(
                '[package]\nname = "gardn"\nversion = "0.1.0"\nedition = "2021"\n',
                encoding="utf-8",
            )
            changelog_dir = root / ".tegami"
            changelog_dir.mkdir()
            (changelog_dir / "docs-only.md").write_text(
                textwrap.dedent(
                    """\
                    ---
                    packages:
                      gardn-docs: patch
                    ---

                    ### Clarify setup docs

                    The docs now explain release setup more clearly.
                    """
                ),
                encoding="utf-8",
            )
            (changelog_dir / "app.md").write_text(
                textwrap.dedent(
                    """\
                    ---
                    packages:
                      gardn: patch
                    ---

                    ### Keep app releaseable

                    The app has a valid release note.
                    """
                ),
                encoding="utf-8",
            )

            result = subprocess.run(
                ["node", str(script), "gardn"],
                cwd=root,
                env={**os.environ, "NO_COLOR": "1"},
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=20,
            )

        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertEqual(result.stdout, "")
        self.assertIn("Every release-worthy Tegami changefile must include gardn", result.stderr)
        self.assertIn("- .tegami/docs-only.md", result.stderr)
        self.assertNotIn("app.md", result.stderr)


    def test_rejects_changefiles_without_package_frontmatter(self) -> None:
        repo_root = Path(__file__).resolve().parents[1]
        script = repo_root / "scripts" / "check-tegami-release-scope.mts"

        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            changelog_dir = root / ".tegami"
            changelog_dir.mkdir()
            (changelog_dir / "unversioned.md").write_text(
                "### Missing release metadata\n\nThis change must not be silently ignored.\n",
                encoding="utf-8",
            )

            result = subprocess.run(
                ["node", str(script), "gardn"],
                cwd=root,
                env={**os.environ, "NO_COLOR": "1"},
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=20,
            )

        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("Every release-worthy Tegami changefile must include gardn", result.stderr)
        self.assertIn("- .tegami/unversioned.md", result.stderr)


class TegamiPrereleaseEnvTests(unittest.TestCase):
    def _run_prerelease(self, value: str | None) -> subprocess.CompletedProcess[str]:
        repo_root = Path(__file__).resolve().parents[1]
        env = os.environ.copy()
        if value is None:
            env.pop("GARDN_TEGAMI_PRERELEASE", None)
        else:
            env["GARDN_TEGAMI_PRERELEASE"] = value
        return subprocess.run(
            [
                "node",
                "--input-type=module",
                "-e",
                "import { tegamiPrerelease } from './scripts/tegami.mts'; console.log(tegamiPrerelease() ?? '')",
            ],
            cwd=repo_root,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=20,
        )

    def test_unset_prerelease_is_stable(self) -> None:
        result = self._run_prerelease(None)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "")

    def test_beta_prerelease_is_accepted(self) -> None:
        result = self._run_prerelease("beta")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "beta")

    def test_unknown_prerelease_is_rejected(self) -> None:
        result = self._run_prerelease("alpha")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("GARDN_TEGAMI_PRERELEASE must be \"beta\" when set", result.stderr)


if __name__ == "__main__":
    unittest.main()
