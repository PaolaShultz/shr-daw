#!/usr/bin/env python3
"""Transactional, manifest-owned installer for the public SHR system."""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
import shutil
import stat
import sys
import tempfile
import uuid
from pathlib import Path, PurePosixPath
from contextlib import contextmanager, ExitStack
from contextvars import ContextVar

SCHEMA_VERSION = 1
STATE_RELATIVE = Path("share/shr-daw-install")
MANIFEST_NAME = "manifest.json"
PENDING_NAME = "pending.json"
_LOCKED_STATE = ContextVar("installer_state", default=None)


class InstallError(RuntimeError):
    pass


def _sync_directory(path: Path) -> None:
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


@contextmanager
def _resource(root: Path, relative: Path, create: bool = False):
    """Pin each parent with openat/O_NOFOLLOW; never mutate through a swapped path."""
    relative = _safe_relative(relative.as_posix())
    with ExitStack() as stack:
        descriptor = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        stack.callback(os.close, descriptor)
        for part in relative.parts[:-1]:
            if create:
                try:
                    os.mkdir(part, dir_fd=descriptor)
                    os.fsync(descriptor)
                except FileExistsError:
                    pass
            try:
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                                dir_fd=descriptor)
            except OSError as error:
                raise InstallError(f"unsafe or missing resource parent: {relative}") from error
            stack.callback(os.close, child)
            descriptor = child
        yield Path(f"/proc/self/fd/{descriptor}") / relative.name


@contextmanager
def _transaction_lock(root: Path, prefix: str, create: bool = False):
    # flock the existing root directory itself: planning stays read-only and
    # uninstall cannot unlink a lock inode while a second process waits on it.
    # This also serializes different prefixes whose payload paths overlap.
    descriptor = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise InstallError("another installer owns this root; retry when it finishes") from error
        relative = _safe_relative(prefix.lstrip("/")) / STATE_RELATIVE / MANIFEST_NAME
        _check_ancestors(root, relative)
        if create or (root / relative.parent).is_dir():
            with _resource(root, relative, create=create) as manifest:
                token = _LOCKED_STATE.set((root, prefix, manifest.parent))
                try:
                    yield
                finally:
                    _LOCKED_STATE.reset(token)
        else:
            yield
    finally:
        os.close(descriptor)


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
        _sync_directory(path.parent)
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
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as source:
        metadata = os.fstat(source.fileno())
        if not stat.S_ISREG(metadata.st_mode):
            raise InstallError(f"resource changed type: {path}")
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
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(descriptor, "r", encoding="utf-8") as source:
            if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
                raise InstallError(f"JSON state is not a regular file: {path}")
            return json.load(source)
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
    pinned = _LOCKED_STATE.get()
    if pinned is not None and pinned[:2] == (root, prefix):
        state = pinned[2]
        return state, state / MANIFEST_NAME, state / PENDING_NAME
    relative_prefix = _safe_relative(prefix.lstrip("/"))
    _check_ancestors(root, relative_prefix / STATE_RELATIVE / MANIFEST_NAME)
    state = root / relative_prefix / STATE_RELATIVE
    return state, state / MANIFEST_NAME, state / PENDING_NAME


def _copy_resource(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    if _exists(destination) and destination.is_dir() and not destination.is_symlink():
        raise InstallError(f"refusing to replace directory {destination}")
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{destination.name}.", dir=destination.parent)
    os.close(descriptor)
    temporary = Path(temporary_name)
    try:
        if source.is_symlink():
            temporary.unlink()
            temporary.symlink_to(os.readlink(source))
        else:
            shutil.copy2(source, temporary, follow_symlinks=False)
            with temporary.open("rb") as copied:
                os.fsync(copied.fileno())
        os.replace(temporary, destination)
        _sync_directory(destination.parent)
    finally:
        temporary.unlink(missing_ok=True)


def _remove_resource(path: Path) -> None:
    if not _exists(path):
        return
    if path.is_dir() and not path.is_symlink():
        raise InstallError(f"refusing to remove directory as a file: {path}")
    path.unlink()
    _sync_directory(path.parent)


def _recover(root: Path, prefix: str) -> bool:
    state, manifest_path, pending_path = _state_paths(root, prefix)
    if not pending_path.exists():
        return False
    pending = _load_json(pending_path)
    if not isinstance(pending, dict) or pending.get("schema_version") != SCHEMA_VERSION:
        raise InstallError("pending transaction is malformed; manual recovery is required")
    transaction_name = pending.get("transaction")
    if not isinstance(transaction_name, str) or not transaction_name.startswith("transaction-") or len(_safe_relative(transaction_name).parts) != 1:
        raise InstallError("invalid transaction directory")
    transaction = state / transaction_name
    if transaction.is_symlink() or not transaction.is_dir():
        raise InstallError("transaction backup directory is missing or unsafe")
    resources = pending.get("resources")
    if not isinstance(resources, list):
        raise InstallError("pending transaction has no resource list")
    previous = pending.get("previous_manifest")
    desired = pending.get("desired_manifest")
    actual_manifest = _load_json(manifest_path) if _exists(manifest_path) else None
    if actual_manifest not in (previous, desired):
        raise InstallError("recovery conflict: installed manifest was edited")
    committed = desired is not None and actual_manifest == desired
    with ExitStack() as stack:
        prepared = []
        for item in resources:
            if not isinstance(item, dict) or not isinstance(item.get("path"), str) or "before" not in item or "after" not in item:
                raise InstallError("legacy or malformed journal needs manual recovery; files preserved")
            relative = _safe_relative(item["path"])
            target = stack.enter_context(_resource(root, relative, create=True))
            before, after = item["before"], item["after"]
            actual = _fingerprint(target) if _exists(target) else None
            if actual not in ((after,) if committed else (before, after)):
                raise InstallError(f"recovery conflict: /{relative} was edited; preserve it and review the journal")
            backup = None
            if before is not None:
                backup_name = item.get("backup")
                if not isinstance(backup_name, str) or len(_safe_relative(backup_name).parts) != 1:
                    raise InstallError("transaction has no safe backup name")
                backup = transaction / backup_name
                if not _exists(backup) or _fingerprint(backup) != before:
                    raise InstallError(f"transaction backup missing or changed: {backup}")
            prepared.append((target, actual, before, backup))
        # All backups and live resources pass before the first destructive step.
        if not committed:
            for target, actual, before, backup in reversed(prepared):
                if (_fingerprint(target) if _exists(target) else None) != actual:
                    raise InstallError("recovery conflict: resource changed after preflight")
                if actual == before:
                    continue
                if backup is None:
                    _remove_resource(target)
                else:
                    _copy_resource(backup, target)
            if previous is None:
                _remove_resource(manifest_path)
            else:
                _atomic_json(manifest_path, previous)
    pending_path.unlink()
    _sync_directory(state)
    shutil.rmtree(transaction)
    _sync_directory(state)
    return True


def _read_current(
    root: Path, prefix: str, recover_pending: bool
) -> tuple[dict[str, object] | None, Path, Path, Path]:
    state, manifest_path, pending_path = _state_paths(root, prefix)
    if pending_path.exists():
        if not recover_pending:
            raise InstallError("an interrupted installation requires recovery before planning")
        _recover(root, prefix)
    current = None
    if manifest_path.exists():
        current = _validate_manifest(_load_json(manifest_path), prefix)
    return current, state, manifest_path, pending_path


def _plan(payload: Path, root: Path, prefix: str, system_version: str) -> dict[str, object]:
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


def _apply(payload: Path, root: Path, prefix: str, system_version: str) -> dict[str, object]:
    _recover(root, prefix)
    desired = _plan(payload, root, prefix, system_version)
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
    if not state.is_dir():
        raise InstallError(f"installer state is not a regular directory: {state}")
    transaction_name = f"transaction-{uuid.uuid4().hex}"
    transaction = state / transaction_name
    transaction.mkdir(mode=0o700)
    _sync_directory(state)
    resources: list[dict[str, object]] = []
    for index, name in enumerate(changed):
        with _resource(root, _safe_relative(name), create=True) as target:
            before = _fingerprint(target) if _exists(target) else None
            item: dict[str, object] = {"path": name, "before": before, "after": entries.get(name)}
            if before is not None:
                backup_name = f"backup-{index}"
                _copy_resource(target, transaction / backup_name)
                if _fingerprint(transaction / backup_name) != before:
                    raise InstallError("backup changed during copy")
                item["backup"] = backup_name
            resources.append(item)
    _atomic_json(
        pending_path,
        {
            "schema_version": SCHEMA_VERSION,
            "transaction": transaction_name,
            "resources": resources,
            "previous_manifest": current,
            "desired_manifest": desired,
        },
    )
    fail_after = int(os.environ.get("SHR_INSTALL_FAIL_AFTER", "0"))
    mutations = 0
    for item in resources:
        name = item["path"]
        with _resource(root, _safe_relative(name)) as target:
            if (_fingerprint(target) if _exists(target) else None) != item["before"]:
                raise InstallError(f"resource changed after preflight: /{name}")
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
    _sync_directory(state)
    shutil.rmtree(transaction)
    _sync_directory(state)
    return desired


def _uninstall(root: Path, prefix: str) -> list[str]:
    current, state, manifest_path, _ = _read_current(root, prefix, recover_pending=True)
    if current is None:
        return []
    entries = current["entries"]
    assert isinstance(entries, dict)
    with ExitStack() as stack:
        targets = {}
        modified = []
        for name, expected in entries.items():
            # Existing parents must remain directories, including on retries.
            _check_ancestors(root, _safe_relative(name))
            if not _exists(root / _safe_relative(name)):
                continue
            target = stack.enter_context(_resource(root, _safe_relative(name)))
            if _fingerprint(target) != expected:
                modified.append(name)
            targets[name] = target
        if modified:
            raise InstallError("refusing to uninstall modified managed resources: " + ", ".join(modified))
        for name, target in targets.items():
            if _fingerprint(target) != entries[name]:
                raise InstallError(f"resource changed after preflight: /{name}")
            _remove_resource(target)
    directories = current["directories"]
    assert isinstance(directories, list)
    for name in sorted(directories, key=lambda value: (value.count("/"), value), reverse=True):
        try:
            with _resource(root, _safe_relative(name)) as directory:
                directory.rmdir()
                _sync_directory(directory.parent)
        except (OSError, InstallError):
            pass
    _remove_resource(manifest_path)
    try:
        state.rmdir()
    except OSError:
        pass
    return sorted(entries)


def plan(payload: Path, root: Path, prefix: str, system_version: str) -> dict[str, object]:
    with _transaction_lock(root, prefix):
        return _plan(payload, root, prefix, system_version)


def apply(payload: Path, root: Path, prefix: str, system_version: str) -> dict[str, object]:
    with _transaction_lock(root, prefix, create=True):
        return _apply(payload, root, prefix, system_version)


def recover(root: Path, prefix: str) -> bool:
    with _transaction_lock(root, prefix):
        return _recover(root, prefix)


def uninstall(root: Path, prefix: str) -> list[str]:
    with _transaction_lock(root, prefix):
        return _uninstall(root, prefix)


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
