#!/usr/bin/env python3
"""M16 后端基准：对比 Cranelift 与 LLVM 的编译时间、运行时间与产物体积。

同一份源码（默认 `examples/m16`）在两个后端、两种 profile 下分别构建，测量：

- Debug/Release 编译时间（清空 target 后重新构建）
- Release 运行时间（多次取最优）
- 可执行文件体积

用法：
    python3 scripts/bench.py [--project examples/m16] [--runs 3] [--no-build]

要求先构建带 LLVM 后端的编译器：
    cargo build --release --features llvm --bins
"""

import argparse
import os
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

BACKENDS = ["cranelift", "llvm"]
PROFILES = ["debug", "release"]


def dc_binary() -> Path:
    name = "dc.exe" if os.name == "nt" else "dc"
    return ROOT / "target" / "release" / name


def build_compiler() -> None:
    subprocess.run(
        ["cargo", "build", "--release", "--features", "llvm", "--bins"],
        cwd=ROOT,
        check=True,
    )


def executable(project: Path, name: str) -> Path:
    path = project / "target" / name
    return path.with_suffix(".exe") if os.name == "nt" else path


def compile_time(dc: Path, project: Path, profile: str, backend: str) -> float:
    shutil.rmtree(project / "target", ignore_errors=True)
    env = dict(os.environ, DOLPHIN_BACKEND=backend)
    start = time.perf_counter()
    subprocess.run(
        [str(dc), "build", str(project), f"--{profile}"],
        cwd=ROOT,
        env=env,
        check=True,
        capture_output=True,
    )
    return time.perf_counter() - start


def run_time(program: Path, runs: int) -> tuple[float, str]:
    best = None
    last = ""
    for _ in range(runs):
        start = time.perf_counter()
        result = subprocess.run([str(program)], capture_output=True, check=True)
        elapsed = time.perf_counter() - start
        best = elapsed if best is None else min(best, elapsed)
        last = result.stdout.decode().strip()
    return best, last


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project", default="examples/m16")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--no-build", action="store_true")
    args = parser.parse_args()

    if not args.no_build:
        build_compiler()

    dc = dc_binary()
    if not dc.is_file():
        raise SystemExit(f"missing {dc}; run `cargo build --release --features llvm --bins`")

    project = (ROOT / args.project).resolve()
    name = project.name

    rows = []
    for backend in BACKENDS:
        for profile in PROFILES:
            seconds = compile_time(dc, project, profile, backend)
            program = executable(project, name)
            size = program.stat().st_size
            row = {
                "backend": backend,
                "profile": profile,
                "compile_s": seconds,
                "size_kb": size / 1024,
            }
            if profile == "release":
                best, output = run_time(program, args.runs)
                row["run_s"] = best
                row["output"] = output
            rows.append(row)

    print()
    print(f"project: {args.project}")
    print()
    print("| backend | profile | compile (s) | run (s) | size (KiB) |")
    print("| --- | --- | ---: | ---: | ---: |")
    for row in rows:
        run = f"{row['run_s']:.3f}" if "run_s" in row else "-"
        print(
            f"| {row['backend']} | {row['profile']} | {row['compile_s']:.3f} | {run} | {row['size_kb']:.1f} |"
        )

    release = {row["backend"]: row for row in rows if row["profile"] == "release"}
    if "llvm" in release and "cranelift" in release:
        speedup = release["cranelift"]["run_s"] / release["llvm"]["run_s"]
        print()
        print(f"Release speedup (cranelift/llvm): {speedup:.2f}x")
        outputs = {row["output"] for row in release.values()}
        if len(outputs) != 1:
            print(f"WARNING: backend outputs differ: {outputs}", file=sys.stderr)


if __name__ == "__main__":
    main()
