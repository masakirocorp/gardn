from __future__ import annotations

import subprocess
import sys
import unittest
from pathlib import Path

from scripts.macos_app_version import bundle_version


VERSION_SCRIPT = Path(__file__).with_name("macos_app_version.py")


class MacosAppVersionTests(unittest.TestCase):
    def test_stable_version_uses_reserved_build_version_range(self) -> None:
        self.assertEqual(bundle_version("0.10.13"), "10.10.13999")

    def test_encoded_version_is_newer_than_legacy_build_nine(self) -> None:
        encoded = tuple(map(int, bundle_version("0.10.13").split(".")))

        self.assertLess((9, 0, 0), encoded)

    def test_beta_versions_sort_before_their_stable_release(self) -> None:
        beta_zero = tuple(map(int, bundle_version("0.10.13-beta.0").split(".")))
        beta_two = tuple(map(int, bundle_version("0.10.13-beta.2").split(".")))
        stable = tuple(map(int, bundle_version("0.10.13").split(".")))

        self.assertLess(beta_zero, beta_two)
        self.assertLess(beta_two, stable)

    def test_beta_998_is_the_last_beta_stage(self) -> None:
        self.assertEqual(bundle_version("0.10.13-beta.998"), "10.10.13998")
        self.assertLess(bundle_version("0.10.13-beta.998"), bundle_version("0.10.13"))

    def test_next_patch_beta_sorts_after_the_prior_stable_release(self) -> None:
        prior_stable = tuple(map(int, bundle_version("0.10.13").split(".")))
        next_patch_beta = tuple(map(int, bundle_version("0.10.14-beta.0").split(".")))

        self.assertLess(prior_stable, next_patch_beta)

    def test_minor_rollover_preserves_order(self) -> None:
        prior = tuple(map(int, bundle_version("0.9.999").split(".")))
        following = tuple(map(int, bundle_version("0.10.0").split(".")))

        self.assertEqual(prior, (10, 9, 999999))
        self.assertEqual(following, (10, 10, 999))
        self.assertLess(prior, following)

    def test_major_rollover_preserves_order(self) -> None:
        prior = tuple(map(int, bundle_version("0.999.999").split(".")))
        following = tuple(map(int, bundle_version("1.0.0").split(".")))

        self.assertEqual(prior, (10, 999, 999999))
        self.assertEqual(following, (11, 0, 999))
        self.assertLess(prior, following)

    def test_canonical_syntax_is_required(self) -> None:
        for version in (
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "1.2.3-beta.01",
            "1.2",
            "1.2.3 ",
            "1.2.3+build.1",
        ):
            with self.subTest(version=version):
                with self.assertRaisesRegex(ValueError, "canonical"):
                    bundle_version(version)

    def test_beta_stage_above_998_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "0 through 998"):
            bundle_version("0.10.13-beta.999")

    def test_encoded_components_must_fit_signed_64_bit(self) -> None:
        for version, component in (
            ("9223372036854775798.0.0", "major"),
            ("0.9223372036854775808.0", "minor"),
            ("0.0.9223372036854775", "patch/stage"),
        ):
            with self.subTest(version=version):
                with self.assertRaisesRegex(ValueError, component):
                    bundle_version(version)

    def test_cli_reports_invalid_input_without_traceback(self) -> None:
        result = subprocess.run(
            [sys.executable, str(VERSION_SCRIPT), "0.10.13-alpha.1"],
            capture_output=True,
            text=True,
            check=False,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertIn("error:", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_cli_requires_exactly_one_version_argument(self) -> None:
        for arguments in ([], ["0.10.13", "extra"]):
            with self.subTest(arguments=arguments):
                result = subprocess.run(
                    [sys.executable, str(VERSION_SCRIPT), *arguments],
                    capture_output=True,
                    text=True,
                    check=False,
                )

                self.assertNotEqual(result.returncode, 0)
                self.assertIn("usage:", result.stderr)

    def test_unknown_prerelease_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            bundle_version("0.10.13-alpha.1")

if __name__ == "__main__":
    unittest.main()
