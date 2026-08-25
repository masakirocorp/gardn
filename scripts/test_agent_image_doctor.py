import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


class AgentImageDoctorTests(unittest.TestCase):
    def test_rejects_pi_aliasing_the_omp_executable(self):
        repo_root = Path(__file__).resolve().parents[1]
        doctor = repo_root / "ci" / "agent-tests" / "doctor.sh"

        with tempfile.TemporaryDirectory() as tmp:
            bin_dir = Path(tmp) / "bin"
            bin_dir.mkdir()
            fake = bin_dir / "fake"
            fake.write_text("#!/bin/sh\nprintf 'test-version\\n'\n")
            fake.chmod(fake.stat().st_mode | stat.S_IXUSR)

            bins = [
                "claude",
                "codex",
                "opencode",
                "copilot",
                "hermes",
                "droid",
                "kimi",
                "maki",
                "qwen",
                "kilo",
                "kilo-code",
                "cursor-agent",
                "qoder",
                "qodercli",
                "jq",
                "python3",
                "node",
                "pnpm",
                "git",
            ]
            for name in bins:
                (bin_dir / name).symlink_to(fake)

            omp = bin_dir / "omp"
            omp.symlink_to(fake)
            (bin_dir / "pi").symlink_to(omp)

            result = subprocess.run(
                [str(doctor)],
                cwd=repo_root,
                env={**os.environ, "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}"},
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            output = result.stdout + result.stderr
            self.assertNotEqual(result.returncode, 0, output)
            self.assertIn("pi and omp resolve to the same executable", output)

    def test_doctor_does_not_require_kiro(self):
        doctor = (
            Path(__file__).resolve().parents[1] / "ci" / "agent-tests" / "doctor.sh"
        ).read_text()
        self.assertNotIn("kiro", doctor)


if __name__ == "__main__":
    unittest.main()
