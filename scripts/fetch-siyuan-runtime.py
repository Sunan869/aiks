#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import platform as host_platform
import shutil
import stat
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCK_FILE = ROOT / "siyuan.version"
DEFAULT_DEST = ROOT / "apps" / "aiks-desktop" / "src-tauri" / "resources" / "siyuan"
CACHE_ROOT = ROOT / ".build" / "cache" / "aiks-siyuan"
STAGING_ROOT = ROOT / ".build" / "staging" / "aiks-siyuan-runtime"


def read_lock(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip().strip('"').strip("'")
    required = [
        "version",
        "workbench_version",
        "release_repo",
        "release_tag",
        "upstream_commit",
        "fork_commit",
        "profile",
        "bridge_protocol",
    ]
    missing = [key for key in required if not values.get(key)]
    if missing:
        raise RuntimeError("siyuan.version is missing: " + ", ".join(missing))
    return values


def current_platform() -> str:
    if sys.platform == "win32":
        os_name = "windows"
    elif sys.platform == "darwin":
        os_name = "macos"
    elif sys.platform.startswith("linux"):
        os_name = "linux"
    else:
        raise RuntimeError(f"Unsupported operating system: {sys.platform}")

    machine = host_platform.machine().lower()
    if machine in {"x86_64", "amd64"}:
        arch = "x64"
    elif machine in {"arm64", "aarch64"}:
        arch = "arm64"
    else:
        raise RuntimeError(f"Unsupported architecture: {machine}")

    target = f"{os_name}-{arch}"
    supported = {
        "windows-x64",
        "macos-x64",
        "macos-arm64",
        "linux-x64",
        "linux-arm64",
    }
    if target not in supported:
        raise RuntimeError(f"AIKS runtime is not published for {target}")
    return target


def download(url: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "AIKS-Runtime-Setup/4.2"})
    with urllib.request.urlopen(request, timeout=120) as response, destination.open("wb") as output:
        shutil.copyfileobj(response, output)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def nonempty_dir(path: Path) -> bool:
    return path.is_dir() and any(path.iterdir())


def kernel_name() -> str:
    return "SiYuan-Kernel.exe" if sys.platform == "win32" else "SiYuan-Kernel"


def validate_layout(root: Path) -> None:
    kernel = root / "kernel" / kernel_name()
    if not kernel.is_file() or kernel.stat().st_size <= 1024 * 1024:
        raise RuntimeError(f"Invalid SiYuan kernel: {kernel}")
    for name in ("stage", "appearance"):
        if not nonempty_dir(root / name):
            raise RuntimeError(f"Runtime directory missing or empty: {name}")


def validate_manifest(root: Path, lock: dict[str, str], target: str) -> None:
    path = root / "aiks-runtime.json"
    if not path.is_file():
        raise RuntimeError("aiks-runtime.json missing from runtime")
    manifest = json.loads(path.read_text(encoding="utf-8"))
    expected = {
        "workbenchVersion": lock["workbench_version"],
        "siyuanBaseVersion": lock["version"],
        "upstreamCommit": lock["upstream_commit"],
        "forkRepository": lock["release_repo"],
        "forkCommit": lock["fork_commit"],
        "profile": lock["profile"],
        "platform": target,
        "bridgeProtocol": int(lock["bridge_protocol"]),
    }
    for key, value in expected.items():
        if manifest.get(key) != value:
            raise RuntimeError(
                f"Runtime manifest mismatch for {key}: expected {value!r}, got {manifest.get(key)!r}"
            )


def install_runtime(source: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    for name in ("kernel", "stage", "appearance", "guide"):
        source_path = source / name
        target_path = destination / name
        if target_path.exists():
            shutil.rmtree(target_path)
        if source_path.exists():
            shutil.copytree(source_path, target_path)
    shutil.copy2(source / "aiks-runtime.json", destination / "aiks-runtime.json")
    license_file = source / "LICENSE-SIYUAN.txt"
    if license_file.exists():
        shutil.copy2(license_file, destination / "LICENSE-SIYUAN.txt")


def main() -> int:
    parser = argparse.ArgumentParser(description="Install the locked AIKS SiYuan runtime")
    parser.add_argument("--dest", type=Path, default=DEFAULT_DEST)
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--runtime-archive", type=Path)
    args = parser.parse_args()

    lock = read_lock(LOCK_FILE)
    target = current_platform()
    asset = f"aiks-siyuan-runtime-{target}.zip"
    tag = lock["release_tag"]
    repo = lock["release_repo"]

    destination = args.dest.resolve()
    manifest_path = destination / "aiks-runtime.json"
    if not args.force and manifest_path.is_file():
        try:
            validate_layout(destination)
            validate_manifest(destination, lock, target)
            print(f"AIKS SiYuan runtime already ready: {target}")
            return 0
        except Exception:
            pass

    if args.runtime_archive:
        archive = args.runtime_archive.resolve()
        if not archive.is_file():
            raise RuntimeError(f"Runtime archive not found: {archive}")
    else:
        cache_dir = CACHE_ROOT / tag
        archive = cache_dir / asset
        checksum_file = cache_dir / f"{asset}.sha256"
        base = f"https://github.com/{repo}/releases/download/{tag}"
        if not archive.is_file() or not checksum_file.is_file():
            print(f"Downloading {repo} / {tag} / {asset}")
            download(f"{base}/{asset}", archive)
            download(f"{base}/{asset}.sha256", checksum_file)
        expected_hash = checksum_file.read_text(encoding="ascii").split()[0].lower()
        actual_hash = sha256(archive)
        if actual_hash != expected_hash:
            archive.unlink(missing_ok=True)
            raise RuntimeError(
                f"Runtime SHA256 mismatch: expected {expected_hash}, got {actual_hash}"
            )

    if STAGING_ROOT.exists():
        shutil.rmtree(STAGING_ROOT)
    STAGING_ROOT.mkdir(parents=True)
    with zipfile.ZipFile(archive) as zf:
        zf.extractall(STAGING_ROOT)

    runtime_root = STAGING_ROOT
    if not (runtime_root / "aiks-runtime.json").is_file():
        matches = list(STAGING_ROOT.rglob("aiks-runtime.json"))
        if len(matches) != 1:
            raise RuntimeError("Unable to locate aiks-runtime.json after extraction")
        runtime_root = matches[0].parent

    validate_layout(runtime_root)
    validate_manifest(runtime_root, lock, target)
    install_runtime(runtime_root, destination)
    validate_layout(destination)
    validate_manifest(destination, lock, target)

    kernel = destination / "kernel" / kernel_name()
    if sys.platform != "win32":
        kernel.chmod(kernel.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

    marker = destination / "aiks-runtime-version.txt"
    marker.write_text(lock["version"] + "\n", encoding="ascii")

    result = subprocess.run(
        [str(kernel), "serve", "--help"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
        timeout=30,
    )
    if result.returncode != 0:
        raise RuntimeError(f"SiYuan kernel smoke test failed: {result.returncode}")

    print(f"AIKS SiYuan runtime ready: {target} -> {destination}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
