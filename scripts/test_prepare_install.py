#!/usr/bin/env python3
"""Contract tests for public source and allowlist resolution."""

import importlib.util
import json
import tempfile
import unittest
from unittest.mock import patch
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
        self.assertEqual(document["system_version"], "0.4.9")
        for name in ("shr-synth", "shr-sampler", "shr-drums"):
            record = prepare.component(document, name)
            self.assertTrue(record["repository"].startswith("https://github.com/PaolaShultz/"))
            self.assertRegex(record["revision"], r"^[0-9a-f]{40}$")

    def test_renamed_synth_payload_retains_legacy_command_alias(self):
        with tempfile.TemporaryDirectory(prefix="shr-rename-payload-") as temporary:
            root = Path(temporary) / "shr-daw"
            (root / "install").mkdir(parents=True)
            (root / "Cargo.toml").write_text('[package]\nversion = "0.4.9"\n')
            (root / "install/compatibility.json").write_text(
                (MODULE_PATH.parent.parent / "install/compatibility.json").read_text()
            )

            def checkout(record, destination):
                destination.mkdir()
                synth = record["name"] == "shr-synth"
                table = "package" if synth else "workspace.package"
                (destination / "Cargo.toml").write_text(
                    f'[{table}]\nversion = "{record["version"]}"\n'
                )
                for filename in ("LICENSE", "README.md", "THIRD_PARTY.md"):
                    (destination / filename).write_text("fixture")
                if synth:
                    bank = destination / "presets"
                    bank.mkdir()
                    (bank / "cleared-presets.txt").write_text("sound.mojsint\n")
                    (bank / "sound.mojsint").write_text("fixture")
                    notices = destination / "vendor/open303"
                    notices.mkdir(parents=True)
                    for filename in ("LICENSE-MIT", "LICENSE-OOURA", "README.md", "UPSTREAM.sha256", "LOCAL.sha256"):
                        (notices / filename).write_text("fixture")
                else:
                    bank = destination / "instruments"
                    (bank / "sound.shrinst").mkdir(parents=True)
                    (bank / "cleared-instruments.txt").write_text("sound.shrinst\n")

            def build(source, profile):
                target = source / "target" / profile
                target.mkdir(parents=True)
                name = "shr" if source == root else source.name
                (target / name).write_text("fixture executable")
                return target

            payload = Path(temporary) / "payload"
            with patch.object(prepare, "checkout", side_effect=checkout), patch.object(
                prepare, "build", side_effect=build
            ), patch.object(prepare, "run"):
                prepare.prepare(root, payload, "debug")
            binary = payload / "usr/local/bin/shr-synth"
            alias = payload / "usr/local/bin/moj-sint"
            self.assertTrue(binary.is_file())
            self.assertTrue(alias.is_symlink())
            self.assertEqual(alias.resolve(), binary)
            self.assertTrue((payload / "usr/local/share/shr-synth/presets/sound.mojsint").is_file())
            self.assertTrue((payload / "usr/local/share/doc/shr-synth/open303/LICENSE-MIT").is_file())
            self.assertFalse((payload / "usr/local/share/moj-sint").exists())

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
