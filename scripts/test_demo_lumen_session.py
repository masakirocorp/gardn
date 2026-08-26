#!/usr/bin/env python3
"""Behavior of the fictional lumen demo fixture."""

from __future__ import annotations

import unittest

from scripts.demo_lumen_session import (
    AGENT_DIR,
    AGENTS,
    FIXTURE_DIR,
    GROUPS,
    WORKSPACES,
    patch_session_snapshot,
)


class DemoLumenFixtureTests(unittest.TestCase):
    def test_fixture_files_exist_for_every_agent_script(self) -> None:
        missing = [
            spec["script"]
            for spec in AGENTS
            if not (AGENT_DIR / spec["script"]).is_file()
        ]
        self.assertEqual(missing, [], f"missing agent scripts: {missing}")
        self.assertTrue((AGENT_DIR / "demo-logs").is_file())
        self.assertTrue((AGENT_DIR / "hold.py").is_file())
        self.assertTrue((FIXTURE_DIR / "config.toml").is_file())

    def test_agent_scripts_export_their_hint(self) -> None:
        for spec in AGENTS:
            text = (AGENT_DIR / spec["script"]).read_text()
            self.assertIn(
                f"export GARDN_AGENT={spec['name']}",
                text,
                spec["script"],
            )

    def test_patch_assigns_accents_groups_and_unseen_triage_agent(self) -> None:
        snapshot = {
            "groups": [
                {"id": "default", "name": "product"},
                {"id": "g-ops", "name": "ops"},
                {"id": "g-commerce", "name": "commerce"},
            ],
            "workspaces": [
                {
                    "custom_name": "checkout",
                    "group_id": "default",
                    "tabs": [
                        {
                            "custom_name": "cart",
                            "panes": {"1": {"agent_name": "pi", "seen": True}},
                        },
                        {
                            "custom_name": "review",
                            "panes": {"2": {"agent_name": "kimi", "seen": True}},
                        },
                    ],
                },
                {
                    "custom_name": "billing",
                    "group_id": "default",
                    "tabs": [
                        {
                            "custom_name": "invoices",
                            "panes": {"3": {"agent_name": "codex"}},
                        }
                    ],
                },
                {
                    "custom_name": "deploy",
                    "group_id": "default",
                    "tabs": [
                        {
                            "custom_name": "staging",
                            "panes": {"4": {"label": "claude"}},
                        }
                    ],
                },
            ],
        }

        patched = patch_session_snapshot(snapshot)

        accents = {group["name"]: group["accent"] for group in patched["groups"]}
        self.assertEqual(
            accents,
            {"product": "yellow", "ops": "red", "commerce": "cyan"},
        )
        workspaces = {workspace["custom_name"]: workspace for workspace in patched["workspaces"]}
        self.assertEqual(workspaces["checkout"]["group_id"], "default")
        self.assertEqual(workspaces["billing"]["group_id"], "g-commerce")
        self.assertEqual(workspaces["deploy"]["group_id"], "g-ops")
        self.assertFalse(workspaces["checkout"]["tabs"][1]["panes"]["2"]["seen"])
        self.assertTrue(workspaces["checkout"]["tabs"][0]["panes"]["1"]["seen"])
        self.assertFalse(patched["group_filter_enabled"])

    def test_workspace_labels_match_declared_groups(self) -> None:
        group_names = {group["name"] for group in GROUPS}
        unknown = [
            workspace["label"]
            for workspace in WORKSPACES
            if workspace["group"] not in group_names
        ]
        self.assertEqual(unknown, [])


if __name__ == "__main__":
    unittest.main()
