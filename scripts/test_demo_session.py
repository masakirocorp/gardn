#!/usr/bin/env python3
"""Behavior of the isolated demo session fixture."""

from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path

from scripts.demo_session import (
    AGENT_DIR,
    AGENTS,
    CAPTURE_WINDOW,
    CAPTURE_WINDOW_TITLE,
    FIXTURE_DIR,
    GROUPS,
    LOGS_SCRIPT,
    NESTED_UNSETS,
    WORKSPACES,
    apply_theme,
    resolve_demo_theme,
    is_capture_window,
    patch_session_snapshot,
    write_attach_wrapper,
    write_ghostty_config,
)


class DemoSessionFixtureTests(unittest.TestCase):
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
        self.assertTrue((FIXTURE_DIR / "ghostty-themes" / "Gardn Day").is_file())
        self.assertTrue((FIXTURE_DIR / "ghostty-themes" / "Gardn Night").is_file())

    def test_config_pins_day_night_and_disables_updates(self) -> None:
        text = (FIXTURE_DIR / "config.toml").read_text()
        self.assertIn("[theme]", text)
        self.assertIn('light = "gardn-day"', text)
        self.assertIn('dark = "gardn-night"', text)
        self.assertIn("[update]", text)
        self.assertIn("version_check = false", text)
        self.assertIn("manifest_check = false", text)
        self.assertIn('open_group_menu = "ctrl+g"', text)
        self.assertIn('delivery = "off"', text)
        self.assertNotIn("terminal_accent", text)

    def test_apply_theme_rewrites_mode_in_both_app_dirs(self) -> None:
        fixture = (FIXTURE_DIR / "config.toml").read_text()
        with tempfile.TemporaryDirectory() as raw:
            home = Path(raw)
            for app_dir in ("gardn", "gardn-dev"):
                dest = home / "config" / app_dir
                dest.mkdir(parents=True)
                (dest / "config.toml").write_text(fixture)
            apply_theme(home, "night")
            for app_dir in ("gardn", "gardn-dev"):
                text = (home / "config" / app_dir / "config.toml").read_text()
                self.assertIn('mode = "dark"', text)
                self.assertIn('light = "gardn-day"', text)
                self.assertIn('dark = "gardn-night"', text)
            apply_theme(home, "day")
            text = (home / "config" / "gardn" / "config.toml").read_text()
            self.assertIn('mode = "light"', text)

    def test_resolve_demo_theme_prefers_explicit_then_env(self) -> None:
        self.assertEqual(resolve_demo_theme("night"), "night")
        self.assertEqual(resolve_demo_theme("day"), "day")
        previous = os.environ.get("GARDN_DEMO_THEME")
        os.environ["GARDN_DEMO_THEME"] = "night"
        try:
            self.assertEqual(resolve_demo_theme(None), "night")
            self.assertEqual(resolve_demo_theme("day"), "day")
        finally:
            if previous is None:
                os.environ.pop("GARDN_DEMO_THEME", None)
            else:
                os.environ["GARDN_DEMO_THEME"] = previous


    def test_agent_screens_do_not_use_a_product_brand(self) -> None:
        for spec in AGENTS:
            text = (AGENT_DIR / spec["script"]).read_text()
            self.assertNotIn("lumen", text.lower(), spec["script"])

    def test_agent_scripts_export_their_hint(self) -> None:
        for spec in AGENTS:
            text = (AGENT_DIR / spec["script"]).read_text()
            self.assertIn(
                f"export GARDN_AGENT={spec['name']}",
                text,
                spec["script"],
            )


    def test_pi_and_logs_use_fixture_screens_without_host_paths(self) -> None:
        blocked = ("exec pi", "http.server", "/Users/", "omp-mk", "PI_CODING_AGENT")
        for name in (next(spec["script"] for spec in AGENTS if spec["name"] == "pi"), LOGS_SCRIPT):
            text = (AGENT_DIR / name).read_text()
            for needle in blocked:
                self.assertNotIn(needle, text, name)
            self.assertIn("\\033[", text, name)

    def test_demo_declares_at_least_one_working_agent(self) -> None:
        working = [spec["name"] for spec in AGENTS if spec["state"] == "working"]
        self.assertGreaterEqual(len(working), 1, working)

    def test_working_agent_screens_do_not_look_idle(self) -> None:
        for spec in AGENTS:
            if spec["state"] != "working":
                continue
            text = (AGENT_DIR / spec["script"]).read_text()
            self.assertNotRegex(text, r"\bDone\b", spec["script"])



    def test_patch_assigns_accents_groups_and_unseen_triage_agent(self) -> None:
        snapshot = {
            "groups": [
                {"id": "default", "name": "product"},
                {"id": "g-ops", "name": "ops"},
                {"id": "g-commerce", "name": "commerce"},
            ],
            "workspaces": [
                {
                    "id": "ws-checkout",
                    "custom_name": "checkout",
                    "group_id": "default",
                    "public_pane_numbers": {"1": 1, "2": 2},
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
                    "id": "ws-billing",
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
                    "id": "ws-deploy",
                    "custom_name": "deploy",
                    "group_id": "default",
                    "public_pane_numbers": {"4": 7},
                    "tabs": [
                        {
                            "custom_name": "staging",
                            "panes": {"4": {"label": "claude"}},
                        }
                    ],
                },
            ],
        }

        patched = patch_session_snapshot(snapshot, now=1_700_000_000)

        accents = {group["name"]: group["accent"] for group in patched["groups"]}
        icons = {group["name"]: group["icon"] for group in patched["groups"]}
        self.assertEqual(
            accents,
            {"product": "yellow", "ops": "red", "commerce": "cyan"},
        )
        self.assertEqual(
            icons,
            {"product": "✎", "ops": "⚙", "commerce": "♥"},
        )
        self.assertEqual(len(set(icons.values())), 3)

        workspaces = {workspace["custom_name"]: workspace for workspace in patched["workspaces"]}
        self.assertEqual(workspaces["checkout"]["group_id"], "default")
        self.assertEqual(workspaces["billing"]["group_id"], "g-commerce")
        self.assertEqual(workspaces["deploy"]["group_id"], "g-ops")
        self.assertFalse(workspaces["checkout"]["tabs"][1]["panes"]["2"]["seen"])
        self.assertTrue(workspaces["checkout"]["tabs"][0]["panes"]["1"]["seen"])
        self.assertFalse(patched["group_filter_enabled"])
        self.assertEqual(
            workspaces["checkout"]["tabs"][0]["panes"]["1"]["terminal_semantics"]["state"],
            "Working",
        )
        self.assertEqual(
            workspaces["checkout"]["tabs"][0]["panes"]["1"]["terminal_semantics"][
                "last_meaningful_agent_activity_unix_secs"
            ],
            1_700_000_000 - 40,
        )
        self.assertEqual(
            patched["agent_follow_up"],
            [
                {
                    "workspace_id": "ws-deploy",
                    "pane_number": 7,
                    "added_at_unix_secs": 1_700_000_000 - 26 * 60,
                }
            ],
        )

    def test_workspace_labels_match_declared_groups(self) -> None:
        group_names = {group["name"] for group in GROUPS}
        unknown = [
            workspace["label"]
            for workspace in WORKSPACES
            if workspace["group"] not in group_names
        ]
        self.assertEqual(unknown, [])

    def test_attach_wrapper_unsets_nested_env_and_execs_session(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            wrapper = write_attach_wrapper(Path("/usr/bin/gardn"), home)
            text = wrapper.read_text()
            self.assertTrue(os.access(wrapper, os.X_OK))
            for key in NESTED_UNSETS:
                self.assertIn(f"-u {key}", text)
            self.assertIn('GARDN_DEMO_HOME="' + str(home) + '"', text)
            self.assertIn("GARDN_SESSION=demo", text)
            self.assertIn('"/usr/bin/gardn" --session demo', text)

    def test_ghostty_capture_config_pins_canvas_font_and_command(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            wrapper = write_attach_wrapper(Path("/usr/bin/gardn"), home)
            config = write_ghostty_config(home, wrapper)
            text = config.read_text()
            self.assertIn(f"command = {wrapper}", text)
            self.assertIn(f"title = {CAPTURE_WINDOW_TITLE}", text)
            self.assertIn("font-size = 15", text)
            self.assertIn('font-family = "Masakiro Mono"', text)
            self.assertIn("window-width = 131", text)
            self.assertIn("window-height = 42", text)
            self.assertIn("window-decoration = none", text)
            self.assertIn("macos-window-shadow = false", text)
            self.assertIn("window-padding-y = 0", text)
            self.assertIn("window-theme = light", text)
            self.assertIn("Gardn Day", text)
            self.assertTrue((home / "ghostty" / "themes" / "Gardn Day").is_file())
            night = write_ghostty_config(home, wrapper, "night").read_text()
            self.assertIn("window-theme = dark", night)
            self.assertIn("Gardn Night", night)

        self.assertEqual(CAPTURE_WINDOW["width"], 1179)
        self.assertEqual(CAPTURE_WINDOW["height"], 798)
        self.assertEqual(CAPTURE_WINDOW["columns"], 131)
        self.assertEqual(CAPTURE_WINDOW["rows"], 42)
        self.assertLessEqual(CAPTURE_WINDOW["x"] + CAPTURE_WINDOW["width"], 1710)
        self.assertLessEqual(CAPTURE_WINDOW["y"] + CAPTURE_WINDOW["height"], 1112)

        self.assertTrue(
            is_capture_window({"name": CAPTURE_WINDOW_TITLE, "x": 0, "y": 0, "width": 1, "height": 1})
        )
        self.assertFalse(
            is_capture_window({"name": "mbair.local: checkout", "x": 0, "y": 0, "width": 1, "height": 1})
        )





if __name__ == "__main__":
    unittest.main()
