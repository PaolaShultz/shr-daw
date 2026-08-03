#!/usr/bin/env python3
"""Contract tests for public source and allowlist resolution."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("prepare_install.py")
SPEC = importlib.util.spec_from_file_location("prepare_install", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
prepare = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(prepare)


class PrepareInstallTests(unittest.TestCase):
    def test_repository_contract_selects_only_exact_public_revisions(self):
        root = MODULE_PATH.parent.parent
        document = prepare.contract(root)
        self.assertEqual(document["system_version"], "0.4.8")
        for name in ("moj-sint", "shr-sampler", "shr-drums"):
            record = prepare.component(document, name)
            self.assertTrue(record["repository"].startswith("https://github.com/PaolaShultz/"))
            self.assertRegex(record["revision"], r"^[0-9a-f]{40}$")

    def test_allowlist_rejects_escape_links_and_duplicates(self):
        with tempfile.TemporaryDirectory(prefix="shr-allowlist-test-") as temporary:
            root = Path(temporary)
            presets = root / "presets"
            presets.mkdir()
            (presets / "good.mojsint").write_text("public", encoding="utf-8")
            manifest = presets / "cleared-presets.txt"
            manifest.write_text("../private.mojsint\n", encoding="utf-8")
            with self.assertRaises(prepare.PreparationError):
                prepare.checked_allowlist(root, "presets/cleared-presets.txt", ".mojsint")
            manifest.write_text("good.mojsint\ngood.mojsint\n", encoding="utf-8")
            with self.assertRaises(prepare.PreparationError):
                prepare.checked_allowlist(root, "presets/cleared-presets.txt", ".mojsint")

    def test_contract_refuses_moving_or_non_github_sources(self):
        root = MODULE_PATH.parent.parent
        document = json.loads((root / "install/compatibility.json").read_text())
        sampler = next(item for item in document["components"] if item["name"] == "shr-sampler")
        sampler["revision"] = "main"
        with tempfile.TemporaryDirectory(prefix="shr-contract-test-") as temporary:
            fake = Path(temporary)
            (fake / "install").mkdir()
            (fake / "install/compatibility.json").write_text(json.dumps(document))
            with self.assertRaises(prepare.PreparationError):
                prepare.contract(fake)


if __name__ == "__main__":
    unittest.main()
