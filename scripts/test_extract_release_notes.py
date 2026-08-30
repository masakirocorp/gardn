from __future__ import annotations

import textwrap
import unittest

from scripts.extract_release_notes import extract_release_notes


class ExtractReleaseNotesTests(unittest.TestCase):
    def test_extracts_exact_package_version_section_and_stops_before_next_package(self) -> None:
        changelog = textwrap.dedent(
            """\
            # Changelog

            ## gardn-docs@1.2.3

            Docs package notes must not be selected.

            ## gardn@1.2.30

            Wrong version must not be selected.

            ## gardn@1.2.3

            ### Added

            - Release the terminal workspace.
            - Preserve user-visible release prose.

            ## gardn-nix@1.2.3

            Nix package notes must not leak into the GitHub release body.
            """
        )

        notes = extract_release_notes(changelog, "gardn", "1.2.3")

        self.assertEqual(
            notes,
            textwrap.dedent(
                """\
                ### Added

                - Release the terminal workspace.
                - Preserve user-visible release prose.
                """
            ).strip(),
        )

    def test_accepts_v_prefixed_versions_from_release_tags(self) -> None:
        changelog = textwrap.dedent(
            """\
            ## gardn@2.0.0

            GitHub release notes for the tag.
            """
        )

        notes = extract_release_notes(changelog, "gardn", "v2.0.0")

        self.assertEqual(notes, "GitHub release notes for the tag.")


    def test_extracts_beta_prerelease_version_section(self) -> None:
        changelog = textwrap.dedent(
            """\
            ## gardn@0.9.5-beta.1

            Beta notes for the prerelease tag.

            ## gardn@0.9.4

            Stable notes must not be selected.
            """
        )

        notes = extract_release_notes(changelog, "gardn", "v0.9.5-beta.1")

        self.assertEqual(notes, "Beta notes for the prerelease tag.")

    def test_missing_section_fails_with_requested_package_and_version(self) -> None:
        changelog = textwrap.dedent(
            """\
            ## gardn-docs@1.2.3

            Docs-only notes.
            """
        )

        with self.assertRaisesRegex(ValueError, r"missing changelog section: ## gardn@1\.2\.3"):
            extract_release_notes(changelog, "gardn", "1.2.3")

    def test_empty_section_fails_instead_of_emitting_blank_release_notes(self) -> None:
        changelog = textwrap.dedent(
            """\
            ## gardn@1.2.3


            ## gardn-docs@1.2.3

            Docs notes.
            """
        )

        with self.assertRaisesRegex(ValueError, r"empty changelog section: ## gardn@1\.2\.3"):
            extract_release_notes(changelog, "gardn", "1.2.3")


if __name__ == "__main__":
    unittest.main()
