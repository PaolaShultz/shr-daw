#!/usr/bin/env python3
"""Resolve pinned public SHR components and build one install payload."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

REVISION = re.compile(r"[0-9a-f]{40}")
VERSION = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+")
SAFE_ENTRY = re.compile(r"[a-z0-9][a-z0-9.-]*")


class PreparationError(RuntimeError):
    pass


def run(arguments: list[str], cwd: Path, environment: dict[str, str] | None = None) -> None:
    subprocess.run(arguments, cwd=cwd, env=environment, check=True)


def contract(root: Path) -> dict[str, object]:
    try:
        document = json.loads((root / "install/compatibility.json").read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise PreparationError(f"cannot read compatibility contract: {error}") from error
    if not isinstance(document, dict) or document.get("schema_version") != 1:
        raise PreparationError("unsupported compatibility contract")
    if not VERSION.fullmatch(str(document.get("system_version", ""))):
        raise PreparationError("compatibility contract has an invalid system version")
    components = document.get("components")
    if not isinstance(components, list):
        raise PreparationError("compatibility contract has no components")
    expected = {"shr-daw", "moj-sint", "shr-sampler", "shr-drums"}
    names = {item.get("name") for item in components if isinstance(item, dict)}
    if names != expected:
        raise PreparationError("compatibility contract component set is incomplete")
    for component in components:
        if not isinstance(component, dict) or not VERSION.fullmatch(str(component.get("version", ""))):
            raise PreparationError("compatibility contract has an invalid component version")
        if component["name"] == "shr-daw":
            if component.get("repository") is not None or component.get("revision") is not None:
                raise PreparationError("local SHR-DAW component must not have a second source")
            continue
        repository = component.get("repository")
        revision = component.get("revision")
        if not isinstance(repository, str) or not repository.startswith(
            "https://github.com/PaolaShultz/"
        ) or not repository.endswith(".git"):
            raise PreparationError(f"{component['name']} has no approved public repository")
        if not isinstance(revision, str) or REVISION.fullmatch(revision) is None:
            raise PreparationError(f"{component['name']} has no exact Git revision")
    return document


def component(document: dict[str, object], name: str) -> dict[str, object]:
    components = document["components"]
    assert isinstance(components, list)
    return next(item for item in components if isinstance(item, dict) and item.get("name") == name)


def package_version(source: Path, workspace: bool = False) -> str:
    with (source / "Cargo.toml").open("rb") as input_file:
        manifest = tomllib.load(input_file)
    table = manifest.get("workspace", {}).get("package", {}) if workspace else manifest.get("package", {})
    version = table.get("version")
    if not isinstance(version, str) or VERSION.fullmatch(version) is None:
        raise PreparationError(f"cannot determine package version in {source}")
    return version


def toolchain(source: Path) -> str:
    with (source / "rust-toolchain.toml").open("rb") as input_file:
        document = tomllib.load(input_file)
    channel = document.get("toolchain", {}).get("channel")
    if not isinstance(channel, str) or VERSION.fullmatch(channel) is None:
        raise PreparationError(f"{source} does not pin an exact Rust toolchain")
    return channel


def checkout(component_record: dict[str, object], destination: Path) -> None:
    repository = str(component_record["repository"])
    revision = str(component_record["revision"])
    environment = os.environ.copy()
    environment["GIT_TERMINAL_PROMPT"] = "0"
    destination.mkdir()
    run(["git", "init", "--quiet"], destination, environment)
    run(["git", "remote", "add", "origin", repository], destination, environment)
    try:
        run(["git", "fetch", "--quiet", "--depth", "1", "origin", revision], destination, environment)
    except subprocess.CalledProcessError as error:
        raise PreparationError(
            f"cannot fetch public pinned source {component_record['name']} at {revision}"
        ) from error
    run(["git", "checkout", "--quiet", "--detach", "FETCH_HEAD"], destination, environment)
    actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=destination, text=True).strip()
    if actual != revision:
        raise PreparationError(f"fetched revision mismatch for {component_record['name']}")


def checked_allowlist(source: Path, relative_manifest: str, suffix: str) -> list[str]:
    manifest = source / relative_manifest
    entries = []
    for line in manifest.read_text(encoding="utf-8").splitlines():
        name = line.strip()
        if not name or name.startswith("#"):
            continue
        if SAFE_ENTRY.fullmatch(name) is None or not name.endswith(suffix):
            raise PreparationError(f"unsafe public allowlist entry {name!r} in {manifest}")
        candidate = manifest.parent / name
        if not candidate.exists() or candidate.is_symlink():
            raise PreparationError(f"missing or linked public resource: {candidate}")
        entries.append(name)
    if not entries or len(entries) != len(set(entries)):
        raise PreparationError(f"public allowlist is empty or duplicated: {manifest}")
    return entries


def copy_tree_without_links(source: Path, destination: Path) -> None:
    for current, names, files in os.walk(source, followlinks=False):
        current_path = Path(current)
        for name in names + files:
            if (current_path / name).is_symlink():
                raise PreparationError(f"public resource contains a symlink: {current_path / name}")
    shutil.copytree(source, destination)


def build(source: Path, profile: str) -> Path:
    pin = toolchain(source)
    run(["rustup", "toolchain", "install", pin, "--profile", "minimal"], source)
    arguments = ["rustup", "run", pin, "cargo", "build", "--locked"]
    if profile == "release":
        arguments.append("--release")
    run(arguments, source)
    return source / "target" / ("release" if profile == "release" else "debug")


def prepare(root: Path, output: Path, profile: str) -> None:
    document = contract(root)
    system_version = str(document["system_version"])
    if package_version(root) != system_version:
        raise PreparationError("SHR-DAW source version does not match its compatibility contract")
    if output.exists() and any(output.iterdir()):
        raise PreparationError("payload output directory must be absent or empty")
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="shr-public-sources-") as temporary:
        source_root = Path(temporary)
        moj_record = component(document, "moj-sint")
        sampler_record = component(document, "shr-sampler")
        moj = source_root / "moj-sint"
        sampler = source_root / "shr-sampler"
        checkout(moj_record, moj)
        checkout(sampler_record, sampler)
        if package_version(moj) != moj_record["version"]:
            raise PreparationError("pinned Moj Sint version does not match compatibility contract")
        if package_version(sampler, workspace=True) != sampler_record["version"]:
            raise PreparationError("pinned SHR Sampler version does not match compatibility contract")
        shr_target = build(root, profile)
        moj_target = build(moj, profile)
        sampler_target = build(sampler, profile)
        run(
            [
                "make",
                "install-files",
                f"DESTDIR={output}",
                f"BUILD_PROFILE={profile}",
            ],
            root,
        )
        binary_dir = output / "usr/local/bin"
        binary_dir.mkdir(parents=True, exist_ok=True)
        shutil.copy2(moj_target / "moj-sint", binary_dir / "moj-sint")
        shutil.copy2(sampler_target / "shr-sampler", binary_dir / "shr-sampler")
        if not (shr_target / "shr").is_file():
            raise PreparationError("SHR-DAW build did not produce its executable")
        moj_names = checked_allowlist(moj, "presets/cleared-presets.txt", ".mojsint")
        moj_output = output / "usr/local/share/moj-sint/presets"
        moj_output.mkdir(parents=True)
        for name in moj_names:
            shutil.copy2(moj / "presets" / name, moj_output / name)
        shutil.copy2(moj / "presets/cleared-presets.txt", moj_output / "cleared-presets.txt")
        sampler_names = checked_allowlist(
            sampler, "instruments/cleared-instruments.txt", ".shrinst"
        )
        sampler_output = output / "usr/local/share/shr-sampler/instruments"
        sampler_output.mkdir(parents=True)
        for name in sampler_names:
            source = sampler / "instruments" / name
            if not source.is_dir():
                raise PreparationError(f"public SHR Sampler instrument is not a package: {source}")
            copy_tree_without_links(source, sampler_output / name)
        shutil.copy2(
            sampler / "instruments/cleared-instruments.txt",
            sampler_output / "cleared-instruments.txt",
        )
        for name, source in (("moj-sint", moj), ("shr-sampler", sampler)):
            doc = output / "usr/local/share/doc" / name
            doc.mkdir(parents=True)
            for filename in ("LICENSE", "README.md", "THIRD_PARTY.md"):
                shutil.copy2(source / filename, doc / filename)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "prepare"))
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--profile", choices=("debug", "release"), default="release")
    arguments = parser.parse_args()
    try:
        root = arguments.root.resolve(strict=True)
        document = contract(root)
        if arguments.command == "check":
            print(json.dumps(document, indent=2, sort_keys=True))
        else:
            if arguments.output is None:
                raise PreparationError("--output is required for prepare")
            prepare(root, arguments.output.resolve(), arguments.profile)
        return 0
    except (PreparationError, OSError, UnicodeError, subprocess.CalledProcessError) as error:
        print(f"install preparation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
