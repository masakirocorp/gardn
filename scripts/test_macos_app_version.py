from __future__ import annotations

import unittest

from scripts.macos_app_version import bundle_version


class MacosAppVersionTests(unittest.TestCase):
    def test_stable_version_uses_reserved_build_version_range(self) -> None:
        self.assertEqual(bundle_version("0.10.13"), "10.10.13999")

    def test_beta_versions_sort_before_their_stable_release(self) -> None:
        beta_zero = bundle_version("0.10.13-beta.0")
        beta_two = bundle_version("0.10.13-beta.2")
        stable = bundle_version("0.10.13")

        self.assertLess(beta_zero, beta_two)
        self.assertLess(beta_two, stable)

    def test_next_patch_beta_sorts_after_the_prior_stable_release(self) -> None:
        prior_stable = bundle_version("0.10.13")
        next_patch_beta = bundle_version("0.10.14-beta.0")

        self.assertLess(prior_stable, next_patch_beta)

    def test_unknown_prerelease_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            bundle_version("0.10.13-alpha.1")

    def test_beta_999_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            bundle_version("0.10.13-beta.999")


if __name__ == "__main__":
    unittest.main()
