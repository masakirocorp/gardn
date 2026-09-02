from __future__ import annotations

import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
EMBED_SCRIPT = REPO_ROOT / "apps/gardn-macos/scripts/embed_bundled_gardn.sh"


class MacosAppBundleLayoutTests(unittest.TestCase):
    def test_embed_keeps_extra_and_cli_as_distinct_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            products = Path(tmp)
            app = products / "Gardn.app"
            macos_dir = app / "Contents" / "MacOS"
            macos_dir.mkdir(parents=True)
            extra = macos_dir / "Gardn"
            extra.write_bytes(b"extra-binary")
            extra.chmod(extra.stat().st_mode | stat.S_IEXEC)

            cli_src = products / "cli-src"
            cli_src.write_bytes(b"cli-binary")
            cli_src.chmod(cli_src.stat().st_mode | stat.S_IEXEC)

            env = os.environ.copy()
            env.update(
                {
                    "BUILT_PRODUCTS_DIR": str(products),
                    "FULL_PRODUCT_NAME": "Gardn.app",
                    "SRCROOT": str(REPO_ROOT / "apps/gardn-macos"),
                    "GARDN_BUNDLE_BIN": str(cli_src),
                }
            )
            result = subprocess.run(
                [str(EMBED_SCRIPT)],
                env=env,
                cwd=REPO_ROOT,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)

            cli = macos_dir / "gardn-cli"
            self.assertTrue(extra.is_file(), "menu extra Contents/MacOS/Gardn is missing")
            self.assertTrue(cli.is_file(), "bundled CLI Contents/MacOS/gardn-cli is missing")
            self.assertFalse(
                extra.samefile(cli),
                "extra and CLI collapsed to one file on a case-insensitive volume",
            )
            self.assertEqual(extra.read_bytes(), b"extra-binary")
            self.assertEqual(cli.read_bytes(), b"cli-binary")


if __name__ == "__main__":
    unittest.main()
