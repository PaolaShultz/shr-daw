#!/usr/bin/env python3
"""Hardware-free tests for SHR's manifest-owned installer."""

import importlib.util
import os
import json
import subprocess
import sys
from unittest import mock
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

    def _interrupt_upgrade(self):
        managed.apply(self.payload, self.root, "/usr/local", "1")
        self._write_payload("v2")
        os.environ["SHR_INSTALL_FAIL_AFTER"] = "1"
        with self.assertRaises(managed.InstallError):
            managed.apply(self.payload, self.root, "/usr/local", "2")
        os.environ.pop("SHR_INSTALL_FAIL_AFTER")
        return self.root / "usr/local/share/shr-daw-install"

    def test_recovery_preserves_intervening_repair(self):
        self._interrupt_upgrade()
        target = self.root / "usr/local/bin/shr"
        target.write_text("administrator repair")
        with self.assertRaisesRegex(managed.InstallError, "recovery conflict"):
            managed.recover(self.root, "/usr/local")
        self.assertEqual(target.read_text(), "administrator repair")

    def test_recovery_preflights_missing_backup_before_removing_anything(self):
        state = self._interrupt_upgrade()
        pending = json.loads((state / "pending.json").read_text())
        (state / pending["transaction"] / pending["resources"][0]["backup"]).unlink()
        target = self.root / "usr/local/bin/shr"
        before = target.read_bytes()
        with self.assertRaisesRegex(managed.InstallError, "backup missing"):
            managed.recover(self.root, "/usr/local")
        self.assertEqual(target.read_bytes(), before)

    def test_uninstall_and_recovery_refuse_redirected_parents(self):
        self._interrupt_upgrade()
        parent = self.root / "usr/local/bin"
        saved = parent.with_name("original-bin")
        parent.rename(saved)
        outside = self.base / "outside"
        outside.mkdir()
        (outside / "shr").write_text("v2")
        (outside / "shr").chmod(0o755)
        parent.symlink_to(outside, target_is_directory=True)
        for operation in (managed.recover, managed.uninstall):
            with self.assertRaises(managed.InstallError):
                operation(self.root, "/usr/local")
        self.assertEqual((outside / "shr").read_text(), "v2")

    def test_parent_swap_after_preflight_never_redirects_removal(self):
        managed.apply(self.payload, self.root, "/usr/local", "1")
        parent = self.root / "usr/local/bin"
        outside = self.base / "outside"
        outside.mkdir()
        (outside / "shr").write_text("v1")
        remove = managed._remove_resource
        swapped = False
        def swap_then_remove(path):
            nonlocal swapped
            if not swapped:
                swapped = True
                parent.rename(parent.with_name("original-bin"))
                parent.symlink_to(outside, target_is_directory=True)
            remove(path)
        with mock.patch.object(managed, "_remove_resource", side_effect=swap_then_remove):
            managed.uninstall(self.root, "/usr/local")
        self.assertEqual((outside / "shr").read_text(), "v1")

    def test_second_process_refuses_active_owner_before_mutation(self):
        with managed._transaction_lock(self.root, "/usr/local"):
            for command in ("apply", "uninstall", "recover"):
                result = subprocess.run([sys.executable, str(MODULE_PATH), command,
                    "--root", str(self.root), "--payload", str(self.payload)],
                    capture_output=True, text=True, timeout=10)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("another installer", result.stderr)
                self.assertFalse((self.root / "usr").exists())
        managed.apply(self.payload, self.root, "/usr/local", "1")

    def test_lock_is_released_when_owner_process_dies(self):
        owner = subprocess.Popen([sys.executable, "-c",
            "import fcntl,os,sys; fd=os.open(sys.argv[1],os.O_RDONLY); "
            "fcntl.flock(fd,fcntl.LOCK_EX); print('locked',flush=True); sys.stdin.read()",
            str(self.root)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        try:
            self.assertEqual(owner.stdout.readline().strip(), "locked")
            owner.kill()
            owner.wait(timeout=10)
            managed.apply(self.payload, self.root, "/usr/local", "1")
        finally:
            if owner.poll() is None:
                owner.kill()
                owner.wait(timeout=10)
            owner.stdin.close()
            owner.stdout.close()

    def test_durable_manifest_survives_pending_retirement_interruption(self):
        original = managed._atomic_json
        def publish_then_interrupt(path, document):
            original(path, document)
            if path.name == managed.MANIFEST_NAME:
                raise OSError("interrupted after durable commit")
        with mock.patch.object(managed, "_atomic_json", side_effect=publish_then_interrupt):
            with self.assertRaises(OSError):
                managed.apply(self.payload, self.root, "/usr/local", "1")
        self.assertTrue(managed.recover(self.root, "/usr/local"))
        self.assertEqual((self.root / "usr/local/bin/shr").read_text(), "v1")
        self.assertFalse(managed.recover(self.root, "/usr/local"))

    def test_sync_failure_before_journal_never_mutates_live_payload(self):
        managed.apply(self.payload, self.root, "/usr/local", "1")
        self._write_payload("v2")
        with mock.patch.object(managed.os, "fsync", side_effect=OSError("injected fsync failure")):
            with self.assertRaises(OSError):
                managed.apply(self.payload, self.root, "/usr/local", "2")
        self.assertEqual((self.root / "usr/local/bin/shr").read_text(), "v1")

    def test_plan_does_not_create_installer_state(self):
        managed.plan(self.payload, self.root, "/usr/local", "1")
        self.assertEqual(list(self.root.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
