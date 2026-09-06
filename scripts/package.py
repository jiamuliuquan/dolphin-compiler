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
import tempfile
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


def bundle_rust_lld_with_libllvm(
    rust_lld: Path, sysroot: Path, target: str
) -> list[tuple[str, Path]]:
    """为 macOS / Linux 发行包修复 rust-lld 的动态链接依赖。

    Rust 1.96+ 把 rust-lld 改为动态链接 LLVM（macOS 依赖 ``@rpath/libLLVM.dylib``，
    Linux 依赖 ``libLLVM.so.<版本>-rust-<版本>-stable``），但 rustup 分发的工具链
    未把该库放到 rust-lld 可解析的位置（macOS 的 rpath 指向官方构建机的
    ``/Users/runner/work/...`` 路径，上游 rust-lang/rust#151063 的后续仍未修复；
    Linux 的 ``$ORIGIN`` rpath 在发行包平铺布局下同样无法命中），导致发行包里的
    rust-lld 运行时报 ``Library not loaded`` / ``cannot open shared object file``。

    这里把工具链自带的 LLVM 动态库一起打包，并把 rust-lld 复制到临时目录改写其
    rpath，使其从自身所在目录（即发行包根）解析该库：
    - macOS：用 ``install_name_tool`` 追加 ``@loader_path``；
    - Linux：用 ``patchelf`` 把 RUNPATH 设为 ``$ORIGIN``。

    返回追加进发行包的文件列表（改写后的 rust-lld 与 LLVM 动态库）。
    """
    lib_dir = sysroot / "lib"

    if target.endswith("apple-darwin"):
        libllvm = lib_dir / "libLLVM.dylib"
        arcname = "libLLVM.dylib"
        if not libllvm.is_file():
            raise SystemExit(f"libLLVM.dylib not found at {libllvm}")
        staged = Path(tempfile.mkdtemp(prefix="dolphin-lld-")) / "rust-lld"
        shutil.copy2(rust_lld, staged)
        run(["install_name_tool", "-add_rpath", "@loader_path", str(staged)])
    else:
        # Linux：定位 libLLVM.so.*（soname 形如 libLLVM.so.22.1-rust-1.98.1-stable）。
        candidates = sorted(lib_dir.glob("libLLVM.so.*"))
        if not candidates:
            raise SystemExit(f"libLLVM.so.* not found in {lib_dir}")
        libllvm = candidates[0]
        arcname = libllvm.name
        staged = Path(tempfile.mkdtemp(prefix="dolphin-lld-")) / "rust-lld"
        shutil.copy2(rust_lld, staged)
        run(["patchelf", "--set-rpath", "$ORIGIN", str(staged)])

    return [("rust-lld", staged), (arcname, libllvm)]


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
        if args.target.endswith(("apple-darwin", "unknown-linux-gnu")):
            # macOS / Linux 的 rust-lld 动态链接 LLVM，需连同 LLVM 库打包并修复 rpath。
            files.extend(bundle_rust_lld_with_libllvm(rust_lld, rustc_sysroot(), args.target))
        else:
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
