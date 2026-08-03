#!/usr/bin/env python3
"""Transactional, manifest-owned installer for the public SHR system."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import sys
import tempfile
import uuid
from pathlib import Path, PurePosixPath

SCHEMA_VERSION = 1
STATE_RELATIVE = Path("share/shr-daw-install")
MANIFEST_NAME = "manifest.json"
PENDING_NAME = "pending.json"


class InstallError(RuntimeError):
    pass


def _atomic_json(path: Path, document: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            json.dump(document, output, indent=2, sort_keys=True)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def _safe_relative(value: str) -> Path:
    pure = PurePosixPath(value)
    if pure.is_absolute() or not pure.parts or any(part in ("", ".", "..") for part in pure.parts):
        raise InstallError(f"unsafe installed path: {value!r}")
    return Path(*pure.parts)


def _fingerprint(path: Path) -> dict[str, object]:
    metadata = path.lstat()
    if stat.S_ISLNK(metadata.st_mode):
        return {"kind": "symlink", "target": os.readlink(path)}
    if not stat.S_ISREG(metadata.st_mode):
        raise InstallError(f"installed resource is not a regular file or symlink: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return {
        "kind": "file",
        "sha256": digest.hexdigest(),
        "mode": stat.S_IMODE(metadata.st_mode),
    }


def _exists(path: Path) -> bool:
    return os.path.lexists(path)


def _check_ancestors(root: Path, relative: Path) -> None:
    candidate = root
    for part in relative.parts[:-1]:
        candidate /= part
        if not _exists(candidate):
            continue
        metadata = candidate.lstat()
        if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
            raise InstallError(f"installed path has an unsafe parent: {candidate}")


def _scan_payload(payload: Path) -> tuple[dict[str, dict[str, object]], list[str]]:
    if not payload.is_dir() or payload.is_symlink():
        raise InstallError("payload must be a regular directory")
    entries: dict[str, dict[str, object]] = {}
    directories: list[str] = []
    for current, names, files in os.walk(payload, followlinks=False):
        current_path = Path(current)
        names.sort()
        files.sort()
        relative_directory = current_path.relative_to(payload)
        for name in list(names):
            candidate = current_path / name
            if candidate.is_symlink():
                names.remove(name)
                files.append(name)
        if relative_directory != Path("."):
            directories.append(relative_directory.as_posix())
        for name in sorted(files):
            candidate = current_path / name
            relative = candidate.relative_to(payload).as_posix()
            entries[relative] = _fingerprint(candidate)
    if not entries:
        raise InstallError("payload is empty")
    return entries, sorted(directories)


def _load_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise InstallError(f"cannot read {path}: {error}") from error


def _validate_manifest(document: object, prefix: str) -> dict[str, object]:
    if not isinstance(document, dict) or document.get("schema_version") != SCHEMA_VERSION:
        raise InstallError("installed manifest has an unsupported schema")
    if document.get("prefix") != prefix:
        raise InstallError("installed manifest belongs to a different prefix")
    entries = document.get("entries")
    directories = document.get("directories")
    if not isinstance(entries, dict) or not isinstance(directories, list):
        raise InstallError("installed manifest is malformed")
    for name, expected in entries.items():
        _safe_relative(name)
        if not isinstance(expected, dict) or expected.get("kind") not in ("file", "symlink"):
            raise InstallError("installed manifest contains an invalid entry")
    for name in directories:
        if not isinstance(name, str):
            raise InstallError("installed manifest contains an invalid directory")
        _safe_relative(name)
    return document


def _state_paths(root: Path, prefix: str) -> tuple[Path, Path, Path]:
    relative_prefix = _safe_relative(prefix.lstrip("/"))
    _check_ancestors(root, relative_prefix / STATE_RELATIVE / MANIFEST_NAME)
    state = root / relative_prefix / STATE_RELATIVE
    return state, state / MANIFEST_NAME, state / PENDING_NAME


def _copy_resource(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    if _exists(destination):
        if destination.is_dir() and not destination.is_symlink():
            raise InstallError(f"refusing to replace directory {destination}")
        destination.unlink()
    if source.is_symlink():
        destination.symlink_to(os.readlink(source))
        return
    descriptor, temporary = tempfile.mkstemp(prefix=f".{destination.name}.", dir=destination.parent)
    os.close(descriptor)
    try:
        shutil.copy2(source, temporary, follow_symlinks=False)
        os.replace(temporary, destination)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def _remove_resource(path: Path) -> None:
    if not _exists(path):
        return
    if path.is_dir() and not path.is_symlink():
        raise InstallError(f"refusing to remove directory as a file: {path}")
    path.unlink()


def recover(root: Path, prefix: str) -> bool:
    state, manifest_path, pending_path = _state_paths(root, prefix)
    if not pending_path.exists():
        return False
    pending = _load_json(pending_path)
    if not isinstance(pending, dict) or pending.get("schema_version") != SCHEMA_VERSION:
        raise InstallError("pending transaction is malformed; manual recovery is required")
    transaction = state / str(pending.get("transaction"))
    resources = pending.get("resources")
    if not isinstance(resources, list):
        raise InstallError("pending transaction has no resource list")
    for item in reversed(resources):
        if not isinstance(item, dict) or not isinstance(item.get("path"), str):
            raise InstallError("pending transaction has an invalid resource")
        relative = _safe_relative(item["path"])
        target = root / relative
        _remove_resource(target)
        if item.get("existed"):
            backup_name = item.get("backup")
            if not isinstance(backup_name, str):
                raise InstallError("pending transaction has no backup")
            backup = transaction / backup_name
            if not _exists(backup):
                raise InstallError(f"transaction backup is missing: {backup}")
            _copy_resource(backup, target)
    previous = pending.get("previous_manifest")
    if previous is None:
        manifest_path.unlink(missing_ok=True)
    else:
        _atomic_json(manifest_path, previous)
    pending_path.unlink()
    shutil.rmtree(transaction, ignore_errors=True)
    return True


def _read_current(
    root: Path, prefix: str, recover_pending: bool
) -> tuple[dict[str, object] | None, Path, Path, Path]:
    state, manifest_path, pending_path = _state_paths(root, prefix)
    if pending_path.exists():
        if not recover_pending:
            raise InstallError("an interrupted installation requires recovery before planning")
        recover(root, prefix)
    current = None
    if manifest_path.exists():
        current = _validate_manifest(_load_json(manifest_path), prefix)
    return current, state, manifest_path, pending_path


def plan(payload: Path, root: Path, prefix: str, system_version: str) -> dict[str, object]:
    entries, directories = _scan_payload(payload)
    current, _, _, _ = _read_current(root, prefix, recover_pending=False)
    old_entries = {} if current is None else current["entries"]
    assert isinstance(old_entries, dict)
    for relative_name, new_fingerprint in entries.items():
        relative = _safe_relative(relative_name)
        _check_ancestors(root, relative)
        target = root / relative
        if not _exists(target):
            continue
        actual = _fingerprint(target)
        old = old_entries.get(relative_name)
        if old is None and actual != new_fingerprint:
            raise InstallError(f"refusing to overwrite user-managed resource: /{relative_name}")
        if old is not None and actual != old:
            raise InstallError(f"managed resource was modified outside the installer: /{relative_name}")
    for relative_name, old_fingerprint in old_entries.items():
        if relative_name in entries:
            continue
        relative = _safe_relative(relative_name)
        _check_ancestors(root, relative)
        target = root / relative
        if _exists(target) and _fingerprint(target) != old_fingerprint:
            raise InstallError(f"managed resource was modified outside the installer: /{relative_name}")
    return {
        "schema_version": SCHEMA_VERSION,
        "system_version": system_version,
        "prefix": prefix,
        "entries": entries,
        "directories": directories,
    }


def apply(payload: Path, root: Path, prefix: str, system_version: str) -> dict[str, object]:
    recover(root, prefix)
    desired = plan(payload, root, prefix, system_version)
    current, state, manifest_path, pending_path = _read_current(
        root, prefix, recover_pending=False
    )
    old_entries = {} if current is None else current["entries"]
    assert isinstance(old_entries, dict)
    entries = desired["entries"]
    assert isinstance(entries, dict)
    affected = sorted(set(old_entries) | set(entries))
    changed = [
        name
        for name in affected
        if (old_entries.get(name) != entries.get(name)) or not _exists(root / _safe_relative(name))
    ]
    state.mkdir(parents=True, exist_ok=True)
    if state.is_symlink() or not state.is_dir():
        raise InstallError(f"installer state is not a regular directory: {state}")
    transaction_name = f"transaction-{uuid.uuid4().hex}"
    transaction = state / transaction_name
    transaction.mkdir(mode=0o700)
    resources: list[dict[str, object]] = []
    for index, name in enumerate(changed):
        target = root / _safe_relative(name)
        existed = _exists(target)
        item: dict[str, object] = {"path": name, "existed": existed}
        if existed:
            backup_name = f"backup-{index}"
            _copy_resource(target, transaction / backup_name)
            item["backup"] = backup_name
        resources.append(item)
    _atomic_json(
        pending_path,
        {
            "schema_version": SCHEMA_VERSION,
            "transaction": transaction_name,
            "resources": resources,
            "previous_manifest": current,
        },
    )
    fail_after = int(os.environ.get("SHR_INSTALL_FAIL_AFTER", "0"))
    mutations = 0
    for name in changed:
        target = root / _safe_relative(name)
        if name in entries:
            source = payload / _safe_relative(name)
            if _fingerprint(source) != entries[name]:
                raise InstallError(f"payload changed during installation: /{name}")
            _copy_resource(source, target)
            if _fingerprint(target) != entries[name]:
                raise InstallError(f"installed resource verification failed: /{name}")
        else:
            _remove_resource(target)
        mutations += 1
        if fail_after and mutations >= fail_after:
            raise InstallError("injected interrupted installation")
    _atomic_json(manifest_path, desired)
    pending_path.unlink()
    shutil.rmtree(transaction)
    return desired


def uninstall(root: Path, prefix: str) -> list[str]:
    current, state, manifest_path, _ = _read_current(root, prefix, recover_pending=True)
    if current is None:
        return []
    entries = current["entries"]
    assert isinstance(entries, dict)
    modified: list[str] = []
    for name, expected in entries.items():
        target = root / _safe_relative(name)
        if not _exists(target):
            continue
        if _fingerprint(target) != expected:
            modified.append(name)
    if modified:
        raise InstallError(
            "refusing to uninstall modified managed resources: "
            + ", ".join(f"/{name}" for name in modified)
        )
    for name in sorted(entries, reverse=True):
        _remove_resource(root / _safe_relative(name))
    directories = current["directories"]
    assert isinstance(directories, list)
    for name in sorted(directories, key=lambda value: (value.count("/"), value), reverse=True):
        try:
            (root / _safe_relative(name)).rmdir()
        except OSError:
            pass
    manifest_path.unlink(missing_ok=True)
    try:
        state.rmdir()
    except OSError:
        pass
    return sorted(entries)


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("plan", "apply", "recover", "uninstall"))
    parser.add_argument("--payload", type=Path)
    parser.add_argument("--root", type=Path, default=Path("/"))
    parser.add_argument("--prefix", default="/usr/local")
    parser.add_argument("--system-version", default="unknown")
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    try:
        root = arguments.root.resolve(strict=True)
        if not root.is_dir() or root.is_symlink():
            raise InstallError("installation root must be a regular directory")
        if not arguments.prefix.startswith("/") or arguments.prefix == "/":
            raise InstallError("prefix must be a non-root absolute path")
        if arguments.command in ("plan", "apply"):
            if arguments.payload is None:
                raise InstallError("--payload is required")
            result = (
                plan(arguments.payload, root, arguments.prefix, arguments.system_version)
                if arguments.command == "plan"
                else apply(arguments.payload, root, arguments.prefix, arguments.system_version)
            )
            print(json.dumps({"status": arguments.command, "files": len(result["entries"])}))
        elif arguments.command == "recover":
            print(json.dumps({"status": "recovered" if recover(root, arguments.prefix) else "clean"}))
        else:
            removed = uninstall(root, arguments.prefix)
            print(json.dumps({"status": "uninstalled", "files": len(removed)}))
        return 0
    except (InstallError, OSError, ValueError) as error:
        print(f"managed install failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
