#!/usr/bin/env python3
"""Cap and cliclick capture workflow specs."""

from __future__ import annotations

import unittest

from scripts.demo_capture import (
    SHOTS,
    cliclick_commands,
    key_to_cliclick,
    resolve_cap_window_id,
    resolve_shots,
    theme_wants_dark_os,
)
from scripts.demo_session import CAPTURE_WINDOW_TITLE


class DemoCaptureWorkflowTests(unittest.TestCase):
    def test_workspace_shot_focuses_capture_window_without_menu_keys(self) -> None:
        shot = next(row for row in SHOTS if row["name"] == "workspace" and row["theme"] == "day")
        window = {"x": 384, "y": 40, "width": 2160, "height": 1200}
        commands = cliclick_commands(tuple(shot["keys"]), window)
        self.assertEqual(commands, [])
        self.assertNotIn("t:g", commands)
        self.assertNotIn("kd:ctrl", commands)

    def test_groups_shot_sends_ctrl_g(self) -> None:
        shot = next(row for row in SHOTS if row["name"] == "groups" and row["theme"] == "night")
        self.assertEqual(
            key_to_cliclick("Ctrl+G", {"x": 0, "y": 0, "width": 10, "height": 10}),
            ["kd:ctrl", "t:g", "ku:ctrl"],
        )
        commands = cliclick_commands(tuple(shot["keys"]), {"x": 0, "y": 0, "width": 10, "height": 10})
        self.assertIn("kd:ctrl", commands)
        self.assertIn("t:g", commands)

    def test_follow_up_shot_opens_agent_row_context_menu(self) -> None:
        shot = next(row for row in SHOTS if row["name"] == "follow-up" and row["theme"] == "day")
        window = {"x": 10, "y": 20, "width": 2160, "height": 1200}
        commands = cliclick_commands(tuple(shot["keys"]), window)
        self.assertIn("rc:130,810", commands)
        self.assertNotIn("c:290,34", commands)





    def test_cap_window_id_matches_title_not_size(self) -> None:
        windows = [
            {"id": "1", "name": "~/projects", "bounds": {"width": 2160, "height": 1200}},
            {"id": "103277", "name": CAPTURE_WINDOW_TITLE, "bounds": {"width": 2160, "height": 1200}},
        ]
        self.assertEqual(resolve_cap_window_id(windows, CAPTURE_WINDOW_TITLE), "103277")

    def test_all_resolves_first_cut_shots(self) -> None:
        shots = resolve_shots("all", "all")
        names = [(shot["name"], shot["theme"]) for shot in shots]
        self.assertEqual(
            names,
            [
                ("workspace", "day"),
                ("workspace", "night"),
                ("groups", "day"),
                ("groups", "night"),
                ("follow-up", "day"),
                ("follow-up", "night"),
                ("collapsed-status", "night"),
                ("commands", "night"),
                ("triage", "night"),
            ],
        )


    def test_night_shots_request_os_dark_mode(self) -> None:
        self.assertFalse(theme_wants_dark_os("day"))
        self.assertTrue(theme_wants_dark_os("night"))


if __name__ == "__main__":
    unittest.main()
