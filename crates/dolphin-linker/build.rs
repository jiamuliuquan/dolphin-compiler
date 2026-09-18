//! 构建脚本：定位 `rust-lld` 完整路径并通过 `DOLPHIN_LLD` 环境变量传给 crate。
//!
//! `rust-lld` 位于 Rust sysroot 的 `lib/rustlib/<target>/bin/`，通常不在 PATH。
//! 这里在编译期确定路径，运行时 `linker` 模块优先用它，找不到时再回退到
//! PATH 或 `rustc --print sysroot` 动态查询。

use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let target = env::var("TARGET").expect("TARGET is set by Cargo");
    let sysroot = Command::new(env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string()))
        .arg("--print")
        .arg("sysroot")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .ok();
    if let Some(sysroot) = sysroot {
        let lld = Path::new(&sysroot)
            .join("lib")
            .join("rustlib")
            .join(&target)
            .join("bin")
            .join("rust-lld");
        if lld.exists() {
            println!("cargo:rustc-env=DOLPHIN_LLD={}", lld.display());
        }
    }
}
