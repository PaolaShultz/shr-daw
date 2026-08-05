#!/usr/bin/env python3
"""Focused URL-policy tests for the deterministic documentation generator."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("generate-docs-site.py")
SPEC = importlib.util.spec_from_file_location("generate_docs_site", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
GENERATOR = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = GENERATOR
SPEC.loader.exec_module(GENERATOR)


class RemoteBadgePolicyTests(unittest.TestCase):
    def document(self, path: str) -> object:
        return GENERATOR.Document(path=Path(path), source="", tokens=[])

    def test_readme_accepts_https_shields_badge_without_local_image(self) -> None:
        source = "https://img.shields.io/github/license/PaolaShultz/shr-daw"
        rewritten, external, image_path = GENERATOR.rewrite_url(
            self.document("README.md"), source, {}, image=True
        )
        self.assertEqual(rewritten, source)
        self.assertTrue(external)
        self.assertIsNone(image_path)

    def test_other_remote_images_remain_rejected(self) -> None:
        cases = (
            ("README.md", "https://example.com/image.svg"),
            ("README.md", "http://img.shields.io/badge/status-alpha-orange"),
            ("docs/README.md", "https://img.shields.io/badge/status-alpha-orange"),
        )
        for document, source in cases:
            with self.subTest(document=document, source=source):
                with self.assertRaisesRegex(SystemExit, "remote image is unsupported"):
                    GENERATOR.rewrite_url(
                        self.document(document), source, {}, image=True
                    )


if __name__ == "__main__":
    unittest.main()
