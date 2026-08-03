#!/usr/bin/env python3
"""Build pinned public sources and simulate a complete install without hardware."""

from __future__ import annotations

import json
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


def run(arguments: list[str], *, capture: bool = False) -> str:
    result = subprocess.run(
        arguments,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )
    return result.stdout.strip() if capture else ""


def main() -> int:
    artifacts = ROOT / "artifacts"
    artifacts.mkdir(exist_ok=True)
    contract = json.loads((ROOT / "install/compatibility.json").read_text(encoding="utf-8"))
    system_version = contract["system_version"]
    with tempfile.TemporaryDirectory(prefix="install-validation-", dir=artifacts) as temporary:
        workspace = Path(temporary)
        payload = workspace / "payload"
        install_root = workspace / "root"
        install_root.mkdir()
        run(
            [
                "python3",
                "scripts/prepare_install.py",
                "prepare",
                "--root",
                str(ROOT),
                "--output",
                str(payload),
                "--profile",
                "debug",
            ]
        )
        manager = [
            "python3",
            "scripts/managed_install.py",
            "--payload",
            str(payload),
            "--root",
            str(install_root),
            "--system-version",
            system_version,
        ]
        run([manager[0], manager[1], "plan", *manager[2:]])
        run([manager[0], manager[1], "apply", *manager[2:]])
        run([manager[0], manager[1], "apply", *manager[2:]])

        prefix = install_root / "usr/local"
        assert run([str(prefix / "bin/shr"), "--version"], capture=True) == (
            f"shr {system_version}"
        )
        assert run([str(prefix / "bin/shr-sampler"), "--version"], capture=True) == (
            "shr-sampler 0.1.2"
        )
        run(
            [
                str(prefix / "bin/shr-sampler"),
                "validate",
                str(prefix / "share/shr-sampler/instruments/shr-clear-tone.shrinst"),
            ]
        )
        moj_presets = list((prefix / "share/moj-sint/presets").glob("*.mojsint"))
        assert len(moj_presets) == 13
        assert not any(path.name == ".git" for path in payload.rglob(".git"))
        assert not any("user" in path.relative_to(payload).parts for path in payload.rglob("*"))
        unrelated = prefix / "share/unrelated.txt"
        unrelated.write_text("keep", encoding="utf-8")
        run(
            [
                "python3",
                "scripts/managed_install.py",
                "uninstall",
                "--root",
                str(install_root),
            ]
        )
        assert unrelated.read_text(encoding="utf-8") == "keep"
        assert not (prefix / "bin/shr").exists()
    print("whole-system disposable installation simulation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
