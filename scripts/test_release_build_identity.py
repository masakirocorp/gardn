from __future__ import annotations

import json
import subprocess
import unittest
from pathlib import Path


class ReleaseBuildIdentityTests(unittest.TestCase):
    def test_turbo_forwards_official_release_identity_to_cargo(self) -> None:
        repo_root = Path(__file__).resolve().parents[1]
        result = subprocess.run(
            ["pnpm", "turbo", "run", "build", "--filter=gardn", "--dry=json"],
            cwd=repo_root,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=30,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        dry_run = json.loads(result.stdout)
        build_task = next(task for task in dry_run["tasks"] if task["taskId"] == "gardn#build")
        forwarded = set(build_task["resolvedTaskDefinition"]["env"])
        self.assertTrue(
            {"GARDN_BUILD_CHANNEL", "GARDN_BUILD_COHORT", "GARDN_RELEASE_TAG"} <= forwarded,
            f"official release identity is not forwarded to Cargo: {sorted(forwarded)}",
        )


if __name__ == "__main__":
    unittest.main()
