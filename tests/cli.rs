use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn dc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dc"))
}

#[test]
fn help_lists_commands_and_global_options() {
    let output = dc().arg("--help").output().expect("dc should run");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Usage: dc [OPTIONS] <COMMAND>"));
    assert!(stdout.contains("check"));
    assert!(stdout.contains("build"));
    assert!(stdout.contains("run"));
    assert!(stdout.contains("package"));
    assert!(stdout.contains("fetch"));
    assert!(stdout.contains("publish"));
    assert!(stdout.contains("info"));
    assert!(stdout.contains("env"));
    assert!(stdout.contains("--color"));
}

#[test]
fn version_comes_from_cargo_package_metadata() {
    let output = dc().arg("--version").output().expect("dc should run");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("dc {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn build_help_is_scoped_and_profiles_conflict() {
    let help = dc()
        .args(["build", "--help"])
        .output()
        .expect("dc should run");
    assert!(help.status.success());
    let stdout = String::from_utf8(help.stdout).unwrap();
    assert!(stdout.contains("dc build"));
    assert!(stdout.contains("--release"));
    assert!(stdout.contains("--output"));

    let conflict = dc()
        .args(["build", ".", "--debug", "--release"])
        .output()
        .expect("dc should run");
    assert_eq!(conflict.status.code(), Some(2));
    assert!(
        String::from_utf8(conflict.stderr)
            .unwrap()
            .contains("cannot be used with")
    );
}

#[test]
fn build_help_lists_linker_options_and_they_conflict() {
    let help = dc()
        .args(["build", "--help"])
        .output()
        .expect("dc build --help should run");
    assert!(help.status.success());
    let stdout = String::from_utf8(help.stdout).unwrap();
    assert!(stdout.contains("--system-linker"));
    assert!(stdout.contains("--bundled-linker"));

    let conflict = dc()
        .args(["build", ".", "--system-linker", "--bundled-linker"])
        .output()
        .expect("dc should run");
    assert_eq!(conflict.status.code(), Some(2));
    assert!(
        String::from_utf8(conflict.stderr)
            .unwrap()
            .contains("cannot be used with")
    );
}

#[test]
fn env_shows_host_target_and_linker() {
    let output = dc().arg("env").output().expect("dc env should run");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("host:"));
    assert!(stdout.contains("target:"));
    assert!(stdout.contains("abi:"));
    assert!(stdout.contains(if cfg!(windows) {
        "linker: link (system; default)"
    } else {
        "linker: cc (system; default)"
    }));
    assert!(stdout.contains("bundled linker: rust-lld (--bundled-linker)"));
    // 宿主与目标三元组必须一致（M10 阶段宿主即目标）。
    let host = stdout
        .lines()
        .find(|line| line.starts_with("host: "))
        .expect("host line should be present");
    let target = stdout
        .lines()
        .find(|line| line.starts_with("target: "))
        .expect("target line should be present");
    assert_eq!(
        host.trim_start_matches("host: "),
        target.trim_start_matches("target: ")
    );
}

#[test]
fn info_shows_package_coordinate() {
    let project = std::env::temp_dir().join(format!(
        "dolphin-cli-info-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("dolphin.toml"),
        r#"
        [package]
        group = "me.foxlab"
        name = "hello"
        version = "0.1.0"

        [[bin]]
        name = "hello"
        path = "src/main.do"
        "#,
    )
    .unwrap();
    fs::write(project.join("src/main.do"), "fn main() { return 0; }").unwrap();

    let output = dc()
        .arg("info")
        .arg(&project)
        .output()
        .expect("dc info should run");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("me.foxlab:hello:0.1.0"));
    assert!(stdout.contains("hello -> "));
    assert!(stdout.contains("main.do"));

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn build_with_manifest_and_run_single_bin() {
    let project = std::env::temp_dir().join(format!(
        "dolphin-cli-bin-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("dolphin.toml"),
        r#"
        [package]
        group = "g"
        name = "n"
        version = "0.1.0"

        [[bin]]
        name = "app"
        path = "src/main.do"
        "#,
    )
    .unwrap();
    fs::write(project.join("src/main.do"), "fn main() { return 42; }").unwrap();

    let run = dc()
        .arg("run")
        .arg(&project)
        .output()
        .expect("dc run should run");
    assert_eq!(run.status.code(), Some(42));

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn build_with_bundled_linker_runs() {
    // 默认系统链接器由其余端到端测试覆盖；这里验证 `--bundled-linker`
    // 仍能解析 Rust 工具链的 rust-lld 并产出可运行程序。
    let project = std::env::temp_dir().join(format!(
        "dolphin-cli-bundled-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("main.do"), "fn main() { return 42; }").unwrap();

    let run = dc()
        .arg("run")
        .arg(project.join("main.do"))
        .arg("--bundled-linker")
        .output()
        .expect("dc run should run");
    assert_eq!(
        run.status.code(),
        Some(42),
        "bundled linker run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn run_multiple_bins_requires_selection() {
    let project = std::env::temp_dir().join(format!(
        "dolphin-cli-multi-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("dolphin.toml"),
        r#"
        [package]
        group = "g"
        name = "n"
        version = "0.1.0"

        [[bin]]
        name = "a"
        path = "src/a.do"

        [[bin]]
        name = "b"
        path = "src/b.do"
        "#,
    )
    .unwrap();
    fs::write(project.join("src/a.do"), "fn main() { return 1; }").unwrap();
    fs::write(project.join("src/b.do"), "fn main() { return 2; }").unwrap();

    let no_bin = dc()
        .arg("run")
        .arg(&project)
        .output()
        .expect("dc run should run");
    assert!(!no_bin.status.success());
    assert!(String::from_utf8(no_bin.stderr).unwrap().contains("--bin"));

    let with_bin = dc()
        .arg("run")
        .arg(&project)
        .arg("--bin")
        .arg("b")
        .output()
        .expect("dc run --bin should run");
    assert_eq!(with_bin.status.code(), Some(2));

    fs::remove_dir_all(project).unwrap();
}

/// 在隔离的 `DOLPHIN_HOME` 下运行 dc（避免污染开发者全局缓存）。
fn dc_with_home(home: &std::path::Path) -> Command {
    let mut command = dc();
    command.env("DOLPHIN_HOME", home);
    command
}

#[test]
fn lib_only_project_builds_package_and_rejects_run() {
    let base = std::env::temp_dir().join(format!(
        "dolphin-cli-lib-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let project = base.join("mathlib");
    let home = base.join("home");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(
        project.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "mathlib"
        version = "1.0.0"

        [lib]
        path = "src/lib.do"
        "#,
    )
    .unwrap();
    fs::write(
        project.join("src/lib.do"),
        "pub fn add(a: i32, b: i32): i32 { return a + b; }",
    )
    .unwrap();

    let build = dc_with_home(&home)
        .arg("build")
        .arg(&project)
        .arg("--lib")
        .output()
        .expect("dc build --lib should run");
    assert!(build.status.success());
    assert!(
        String::from_utf8(build.stdout)
            .unwrap()
            .contains("mathlib-1.0.0.dlib")
    );
    assert!(project.join("target/package/mathlib-1.0.0.dlib").is_file());

    // `run --lib` 明确拒绝。
    let run = dc_with_home(&home)
        .arg("run")
        .arg(&project)
        .arg("--lib")
        .output()
        .expect("dc run --lib should run");
    assert!(!run.status.success());
    assert!(String::from_utf8(run.stderr).unwrap().contains("--lib"));

    // `run` 纯库项目拒绝。
    let run_lib = dc_with_home(&home)
        .arg("run")
        .arg(&project)
        .output()
        .expect("dc run should run");
    assert!(!run_lib.status.success());
    assert!(
        String::from_utf8(run_lib.stderr)
            .unwrap()
            .contains("library")
    );

    // `dc package` 等价产出 `.dlib`。
    let package = dc_with_home(&home)
        .arg("package")
        .arg(&project)
        .output()
        .expect("dc package should run");
    assert!(package.status.success());
    assert!(
        project
            .join("target/package/mathlib-1.0.0.dlib.sha256")
            .is_file()
    );

    fs::remove_dir_all(base).unwrap();
}

#[test]
fn fetch_writes_lockfile_for_path_dependency() {
    let base = std::env::temp_dir().join(format!(
        "dolphin-cli-fetch-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let lib = base.join("lib");
    fs::create_dir_all(lib.join("src")).unwrap();
    fs::write(
        lib.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "lib"
        version = "1.0.0"

        [lib]
        path = "src/lib.do"
        "#,
    )
    .unwrap();
    fs::write(lib.join("src/lib.do"), "pub fn value(): i32 { return 1; }").unwrap();

    let app = base.join("app");
    fs::create_dir_all(app.join("src")).unwrap();
    fs::write(
        app.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "app"
        version = "0.1.0"

        [[bin]]
        name = "app"
        path = "src/main.do"

        [dependencies]
        lib = { path = "../lib" }
        "#,
    )
    .unwrap();
    fs::write(app.join("src/main.do"), "fn main() { return 0; }").unwrap();

    let fetch = dc_with_home(&home)
        .arg("fetch")
        .arg(&app)
        .output()
        .expect("dc fetch should run");
    assert!(
        fetch.status.success(),
        "{:?}",
        String::from_utf8(fetch.stderr)
    );
    let lock = fs::read_to_string(app.join("dolphin.lock")).unwrap();
    assert!(lock.contains("source = \"path\""));
    assert!(lock.contains("org.example:lib:1.0.0"));

    fs::remove_dir_all(base).unwrap();
}

// ---------------------------------------------------------------------------
// H18-08 BUILD-01..04：项目目标与 profile 一致性。
// ---------------------------------------------------------------------------

/// 受控泄漏程序：Debug runtime 报告泄漏，Release runtime 不报告。
const H18_08_LEAK: &str = "use std.mem;\n\nfn main() {\n    val bytes = mem.alloc<u8>(8);\n    bytes[0] = 1_u8;\n    return 42;\n}\n";

fn unique_project(tag: &str) -> PathBuf {
    let project = std::env::temp_dir().join(format!(
        "dolphin-cli-h18-08-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(project.join("src")).unwrap();
    project
}

/// 写一个单 bin 项目，`[build] optimization` 由参数决定。
fn write_source_project(tag: &str, optimization: &str, source: &str) -> PathBuf {
    let project = unique_project(tag);
    fs::write(
        project.join("dolphin.toml"),
        format!(
            r#"
            [package]
            group = "org.example"
            name = "h1808"
            version = "0.1.0"

            [[bin]]
            name = "app"
            path = "src/main.do"

            [build]
            optimization = "{optimization}"
            "#
        ),
    )
    .unwrap();
    fs::write(project.join("src/main.do"), source).unwrap();
    project
}

fn run_project(project: &Path, args: &[&str]) -> std::process::Output {
    dc().arg("run")
        .arg(project)
        .args(args)
        .output()
        .expect("dc run should run")
}

fn stderr_text(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// BUILD-02/03：清单 Release 配合显式 `--debug` 必须使用 Debug runtime（有泄漏报告），
/// 而不是被清单覆盖成 Release。
#[test]
fn h18_08_build_03_explicit_debug_on_release_manifest_reports_leak() {
    let project = write_source_project("debug-over-release", "release", H18_08_LEAK);
    let output = run_project(&project, &["--debug"]);
    assert_eq!(
        output.status.code(),
        Some(42),
        "stderr={}",
        stderr_text(&output)
    );
    assert!(
        stderr_text(&output).contains("leaked"),
        "explicit --debug must select the Debug runtime: {}",
        stderr_text(&output)
    );
    fs::remove_dir_all(project).unwrap();
}

/// BUILD-03 反向：清单 Debug 配合显式 `--release` 必须使用 Release runtime（无泄漏报告）。
#[test]
fn h18_08_build_03_explicit_release_on_debug_manifest_is_clean() {
    let project = write_source_project("release-over-debug", "debug", H18_08_LEAK);
    let output = run_project(&project, &["--release"]);
    assert_eq!(
        output.status.code(),
        Some(42),
        "stderr={}",
        stderr_text(&output)
    );
    assert!(
        output.stderr.is_empty(),
        "explicit --release must not report leaks: {}",
        stderr_text(&output)
    );
    fs::remove_dir_all(project).unwrap();
}

/// BUILD-02：没有显式 profile 时以根清单 `build.optimization` 为准。
#[test]
fn h18_08_build_02_manifest_optimization_is_the_default() {
    let release = write_source_project("manifest-release", "release", H18_08_LEAK);
    let output = run_project(&release, &[]);
    assert_eq!(
        output.status.code(),
        Some(42),
        "stderr={}",
        stderr_text(&output)
    );
    assert!(
        output.stderr.is_empty(),
        "release manifest default must not report leaks: {}",
        stderr_text(&output)
    );
    fs::remove_dir_all(release).unwrap();

    let debug = write_source_project("manifest-debug", "debug", H18_08_LEAK);
    let output = run_project(&debug, &[]);
    assert_eq!(
        output.status.code(),
        Some(42),
        "stderr={}",
        stderr_text(&output)
    );
    assert!(
        stderr_text(&output).contains("leaked"),
        "debug manifest default must report leaks: {}",
        stderr_text(&output)
    );
    fs::remove_dir_all(debug).unwrap();
}

/// BUILD-03：Debug runtime 的正常清理不误报；受控非法释放（double free）以固定
/// 退出码与诊断失败，而不是静默继续。
#[test]
fn h18_08_build_03_debug_runtime_reports_leak_and_invalid_free() {
    let clean = write_source_project(
        "clean-cleanup",
        "debug",
        "use std.mem;\n\nfn main() {\n    val bytes = mem.alloc<u8>(8);\n    bytes[0] = 1_u8;\n    mem.free(bytes);\n    return 42;\n}\n",
    );
    let output = run_project(&clean, &["--debug"]);
    assert_eq!(
        output.status.code(),
        Some(42),
        "stderr={}",
        stderr_text(&output)
    );
    assert!(
        output.stderr.is_empty(),
        "clean Debug program must not report leaks: {}",
        stderr_text(&output)
    );
    fs::remove_dir_all(clean).unwrap();

    let invalid = write_source_project(
        "invalid-free",
        "debug",
        "use std.mem;\n\nfn main() {\n    val bytes = mem.alloc<u8>(8);\n    bytes[0] = 1_u8;\n    mem.free(bytes);\n    mem.free(bytes);\n    return 0;\n}\n",
    );
    let output = run_project(&invalid, &["--debug"]);
    assert_eq!(
        output.status.code(),
        Some(103),
        "invalid free must keep its runtime exit code: {}",
        stderr_text(&output)
    );
    assert!(
        stderr_text(&output).contains("invalid free"),
        "invalid free must be diagnosed: {}",
        stderr_text(&output)
    );
    fs::remove_dir_all(invalid).unwrap();
}

/// BUILD-04：`--backend` 显式选择优先于 `DOLPHIN_BACKEND` 环境变量。
#[test]
fn h18_08_build_04_explicit_backend_beats_environment() {
    let project = write_source_project("backend-priority", "debug", "fn main() { return 0; }\n");

    // 无显式选项：非法环境值被告警并回退。
    let env_only = dc()
        .arg("run")
        .arg(&project)
        .env("DOLPHIN_BACKEND", "not-a-backend")
        .output()
        .expect("dc run should run");
    assert!(
        env_only.status.success(),
        "stderr={}",
        stderr_text(&env_only)
    );
    assert!(
        stderr_text(&env_only).contains("unknown codegen backend"),
        "environment must be consulted without --backend: {}",
        stderr_text(&env_only)
    );

    // 显式 --backend：不读取环境变量，因此没有告警。
    let explicit = dc()
        .arg("run")
        .arg(&project)
        .args(["--backend", "cranelift"])
        .env("DOLPHIN_BACKEND", "not-a-backend")
        .output()
        .expect("dc run should run");
    assert!(
        explicit.status.success(),
        "stderr={}",
        stderr_text(&explicit)
    );
    assert!(
        !stderr_text(&explicit).contains("unknown codegen backend"),
        "explicit --backend must not consult DOLPHIN_BACKEND: {}",
        stderr_text(&explicit)
    );

    fs::remove_dir_all(project).unwrap();
}

/// BUILD-02：作为依赖的包不能反过来覆盖根 profile。根清单未显式指定时使用根清单
/// 默认（Debug），即使依赖自己的清单声明 `optimization = "release"`。
#[test]
fn h18_08_build_02_dependency_manifest_does_not_override_root_profile() {
    let base = unique_project("dep-profile");
    let dep = base.join("dep");
    fs::create_dir_all(dep.join("src")).unwrap();
    fs::write(
        dep.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "dep"
        version = "1.0.0"

        [lib]
        path = "src/lib.do"

        [build]
        optimization = "release"
        "#,
    )
    .unwrap();
    fs::write(dep.join("src/lib.do"), "pub fn value(): i32 { return 42; }").unwrap();

    let app = base.join("app");
    fs::create_dir_all(app.join("src")).unwrap();
    fs::write(
        app.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "app"
        version = "0.1.0"

        [[bin]]
        name = "app"
        path = "src/main.do"

        [dependencies]
        dep = { path = "../dep" }
        "#,
    )
    .unwrap();
    fs::write(
        app.join("src/main.do"),
        "use dep;\nuse std.mem;\n\nfn main() {\n    val bytes = mem.alloc<u8>(8);\n    bytes[0] = 1_u8;\n    return dep.value();\n}\n",
    )
    .unwrap();

    let output = run_project(&app, &[]);
    assert_eq!(
        output.status.code(),
        Some(42),
        "stderr={}",
        stderr_text(&output)
    );
    assert!(
        stderr_text(&output).contains("leaked"),
        "root manifest default must decide the profile, not the dependency: {}",
        stderr_text(&output)
    );

    fs::remove_dir_all(base).unwrap();
}

/// BUILD-02：库与 bin 使用同一最终 profile。LLVM Debug 发射 DWARF、Release 不发射，
/// 因此可用 debug 段是否存在观察两个产物，而不只是断言枚举值。
/// ELF/COFF 检查可执行文件与 `.debug_info`；Mach-O 链接器按平台惯例不把 DWARF
/// 复制进可执行文件（保留在对象文件与 debug map 中），macOS 改为检查 lib/bin
/// 的对象文件与 `__debug_info` 段名，见 tests/backend.rs 的同类说明。
#[cfg(feature = "llvm")]
#[test]
fn h18_08_build_02_library_and_bins_share_effective_profile() {
    fn contains(bytes: &[u8], needle: &[u8]) -> bool {
        bytes.windows(needle.len()).any(|window| window == needle)
    }

    let project = unique_project("lib-bin-profile");
    let manifest = |optimization: &str| {
        format!(
            r#"
            [package]
            group = "org.example"
            name = "mixed"
            version = "0.1.0"

            [lib]
            path = "src/lib.do"

            [[bin]]
            name = "app"
            path = "src/main.do"

            [build]
            optimization = "{optimization}"
            "#
        )
    };
    fs::write(
        project.join("src/lib.do"),
        "pub fn value(): i32 { return 42; }\n",
    )
    .unwrap();
    fs::write(
        project.join("src/main.do"),
        "fn main() { return value() - 42; }\n",
    )
    .unwrap();

    let lib_object = project.join("target/lib/mixed.o");
    let bin = if cfg!(windows) {
        project.join("target/app.exe")
    } else {
        project.join("target/app")
    };
    // ELF/COFF 的 Debug 信息在可执行文件里；Mach-O 可执行文件只有 debug map，
    // DWARF 在对象文件中，因此 macOS 检查 `__debug_info` 与 `app.o`。
    let debug_needle: &[u8] = if cfg!(target_os = "macos") {
        b"__debug_info"
    } else {
        b".debug_info"
    };
    let debug_artifacts: [PathBuf; 2] = if cfg!(target_os = "macos") {
        [lib_object.clone(), project.join("target/app.o")]
    } else {
        [lib_object.clone(), bin.clone()]
    };

    // 清单 Release + 显式 --debug：库与 bin 都必须是 Debug（含 DWARF）。
    fs::write(project.join("dolphin.toml"), manifest("release")).unwrap();
    let debug_build = dc()
        .arg("build")
        .arg(&project)
        .args(["--debug", "--backend", "llvm"])
        .output()
        .expect("dc build should run");
    assert!(
        debug_build.status.success(),
        "stderr={}",
        stderr_text(&debug_build)
    );
    for path in &debug_artifacts {
        let bytes = fs::read(path).unwrap_or_else(|error| {
            panic!("read {}: {error}", path.display());
        });
        assert!(
            contains(&bytes, debug_needle),
            "{} must be built with the Debug profile",
            path.display()
        );
    }

    // 清单 Debug + 显式 --release：库与 bin 都必须是 Release（无 DWARF）。
    fs::write(project.join("dolphin.toml"), manifest("debug")).unwrap();
    fs::remove_dir_all(project.join("target")).unwrap();
    let release_build = dc()
        .arg("build")
        .arg(&project)
        .args(["--release", "--backend", "llvm"])
        .output()
        .expect("dc build should run");
    assert!(
        release_build.status.success(),
        "stderr={}",
        stderr_text(&release_build)
    );
    for path in &debug_artifacts {
        let bytes = fs::read(path).unwrap_or_else(|error| {
            panic!("read {}: {error}", path.display());
        });
        assert!(
            !contains(&bytes, debug_needle),
            "{} must be built with the Release profile",
            path.display()
        );
    }

    fs::remove_dir_all(project).unwrap();
}
