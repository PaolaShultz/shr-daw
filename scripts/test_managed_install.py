#!/usr/bin/env python3
"""Hardware-free tests for SHR's manifest-owned installer."""

import importlib.util
import os
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("managed_install.py")
SPEC = importlib.util.spec_from_file_location("managed_install", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
managed = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(managed)


class ManagedInstallTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="shr-install-test-")
        self.base = Path(self.temporary.name)
        self.root = self.base / "root"
        self.payload = self.base / "payload"
        self.root.mkdir()
        self._write_payload("v1")

    def tearDown(self):
        os.environ.pop("SHR_INSTALL_FAIL_AFTER", None)
        self.temporary.cleanup()

    def _write_payload(self, content):
        binary = self.payload / "usr/local/bin/shr"
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_text(content, encoding="utf-8")
        binary.chmod(0o755)
        alias = binary.with_name("shs")
        alias.unlink(missing_ok=True)
        alias.symlink_to("shr")

    def test_fresh_install_and_idempotent_reinstall(self):
        first = managed.apply(self.payload, self.root, "/usr/local", "1.0.0")
        self.assertEqual((self.root / "usr/local/bin/shr").read_text(), "v1")
        second = managed.apply(self.payload, self.root, "/usr/local", "1.0.0")
        self.assertEqual(first["entries"], second["entries"])

    def test_update_replaces_only_unchanged_owned_resource(self):
        managed.apply(self.payload, self.root, "/usr/local", "1.0.0")
        self._write_payload("v2")
        managed.apply(self.payload, self.root, "/usr/local", "1.0.1")
        self.assertEqual((self.root / "usr/local/bin/shr").read_text(), "v2")

    def test_foreign_collision_and_modified_owned_resource_are_refused(self):
        foreign = self.root / "usr/local/bin/shr"
        foreign.parent.mkdir(parents=True)
        foreign.write_text("foreign", encoding="utf-8")
        with self.assertRaisesRegex(managed.InstallError, "user-managed"):
            managed.plan(self.payload, self.root, "/usr/local", "1.0.0")
        foreign.unlink()
        managed.apply(self.payload, self.root, "/usr/local", "1.0.0")
        foreign.write_text("locally edited", encoding="utf-8")
        with self.assertRaisesRegex(managed.InstallError, "modified outside"):
            managed.plan(self.payload, self.root, "/usr/local", "1.0.0")

    def test_partial_install_recovers_before_retry(self):
        os.environ["SHR_INSTALL_FAIL_AFTER"] = "1"
        with self.assertRaisesRegex(managed.InstallError, "interrupted"):
            managed.apply(self.payload, self.root, "/usr/local", "1.0.0")
        os.environ.pop("SHR_INSTALL_FAIL_AFTER")
        with self.assertRaisesRegex(managed.InstallError, "requires recovery"):
            managed.plan(self.payload, self.root, "/usr/local", "1.0.0")
        managed.apply(self.payload, self.root, "/usr/local", "1.0.0")
        self.assertEqual((self.root / "usr/local/bin/shr").read_text(), "v1")

    def test_uninstall_removes_owned_files_and_preserves_unrelated_data(self):
        managed.apply(self.payload, self.root, "/usr/local", "1.0.0")
        unrelated = self.root / "usr/local/share/unrelated.txt"
        unrelated.parent.mkdir(parents=True, exist_ok=True)
        unrelated.write_text("keep", encoding="utf-8")
        removed = managed.uninstall(self.root, "/usr/local")
        self.assertIn("usr/local/bin/shr", removed)
        self.assertTrue(unrelated.exists())
        self.assertFalse((self.root / "usr/local/bin/shr").exists())

    def test_uninstall_refuses_modified_managed_file(self):
        managed.apply(self.payload, self.root, "/usr/local", "1.0.0")
        (self.root / "usr/local/bin/shr").write_text("edited", encoding="utf-8")
        with self.assertRaisesRegex(managed.InstallError, "refusing to uninstall"):
            managed.uninstall(self.root, "/usr/local")


if __name__ == "__main__":
    unittest.main()
