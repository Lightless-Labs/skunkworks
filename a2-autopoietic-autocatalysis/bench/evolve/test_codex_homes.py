"""Tests for bench/evolve/codex_homes.py. Run: python3 -m unittest bench/evolve/test_codex_homes.py"""

from __future__ import annotations

import re
import unittest

from bench.evolve import codex_homes

OFF = ("apps", "plugins", "remote_plugin", "in_app_browser", "skill_mcp_dependency_install", "skill_search",
       "memories", "browser_use", "browser_use_external", "browser_use_full_cdp_access", "computer_use")


class ConfigTemplateTests(unittest.TestCase):
    def test_ambient_and_bypass_features_are_off(self) -> None:
        text = codex_homes.CONFIG_TEMPLATE.read_text()
        self.assertIn('web_search = "disabled"', text)
        features = text.split("[features]", 1)[1]
        for name in OFF:
            self.assertRegex(features, rf"(?m)^{re.escape(name)} = false$")
        self.assertNotIn("[projects", text)

    def test_unknown_role_rejected(self) -> None:
        with self.assertRaises(ValueError):
            codex_homes.ensure_role_home("operator")


if __name__ == "__main__":
    unittest.main()
