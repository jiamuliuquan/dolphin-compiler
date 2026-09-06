#!/usr/bin/env python3
"""打包 Dolphin 编译器发行包。

从 `target/release` 收集 `dc` 与 `dolphin-compiler`，从 Rust sysroot 定位
`rust-lld`，连同 LICENSE 与 README 组装成一个自包含发行包（Linux/macOS 为
.tar.gz，Windows 为 .zip），并生成 SHA-256 校验和。

用法：
    python3 scripts/package.py [--version VERSION] [--target TARGET] [--out-dir DIR]

默认从环境变量 Cargo 提供的信息推导版本与目标。
"""

import argparse
import hashlib
import os
import shutil
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path

EXE_SUFFIX = ".exe" if os.name == "nt" else ""


def run(cmd: list[str]) -> str:
    """运行命令并返回 stdout（去除首尾空白）。"""
    return subprocess.run(cmd, check=True, capture_output=True, text=True).stdout.strip()


def rustc_sysroot() -> Path:
    rustc = os.environ.get("RUSTC", "rustc")
    return Path(run([rustc, "--print", "sysroot"]))


def find_rust_lld(target: str) -> Path:
    """定位 rust-lld（Rust 工具链自带，位于 sysroot 的 lib/rustlib/<target>/bin）。"""
    sysroot = rustc_sysroot()
    name = "rust-lld.exe" if target.endswith("msvc") else "rust-lld"
    candidate = sysroot / "lib" / "rustlib" / target / "bin" / name
    if not candidate.is_file():
        raise SystemExit(f"rust-lld not found at {candidate}")
    return candidate


def checksum(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def package_version(root: Path) -> str:
    """从 Cargo.toml 读取 `[package]` 的 `version` 字段。"""
    text = (root / "Cargo.toml").read_text()
    for line in text.splitlines():
        if line.strip().startswith("version"):
            return line.split("=", 1)[1].strip().strip('"')
    raise SystemExit("version not found in Cargo.toml")


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", default=package_version(root))
    parser.add_argument("--target", default=os.environ.get("TARGET", ""))
    parser.add_argument("--out-dir", default="dist")
    args = parser.parse_args()

    release = root / "target" / "release"
    dc = release / f"dc{EXE_SUFFIX}"
    compat = release / f"dolphin-compiler{EXE_SUFFIX}"
    for binary in (dc, compat):
        if not binary.is_file():
            raise SystemExit(f"missing release binary: {binary}")

    rust_lld = find_rust_lld(args.target) if args.target else None

    # 组装发行目录内容。
    name = f"dolphin-{args.version}-{args.target}"
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    files: list[tuple[str, Path]] = [
        (f"dc{EXE_SUFFIX}", dc),
        (f"dolphin-compiler{EXE_SUFFIX}", compat),
        ("LICENSE", root / "LICENSE"),
        ("README.md", root / "README.md"),
    ]
    if rust_lld is not None:
        files.append((f"rust-lld{EXE_SUFFIX}", rust_lld))

    # 打包。
    if args.target.endswith("msvc") or os.name == "nt":
        archive = out_dir / f"{name}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zf:
            for arcname, src in files:
                zf.write(src, arcname)
    else:
        archive = out_dir / f"{name}.tar.gz"
        with tarfile.open(archive, "w:gz") as tf:
            for arcname, src in files:
                tf.add(src, arcname=arcname)

    # 校验和。
    digest = checksum(archive)
    checksum_file = out_dir / f"{name}.sha256"
    checksum_file.write_text(f"{digest}  {archive.name}\n")

    print(f"packaged {archive}")
    print(f"checksum {checksum_file} ({digest})")


if __name__ == "__main__":
    main()
