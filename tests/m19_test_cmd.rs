//! H19-05a：`dc test` 测试目标入口（`target/test/<包名>-tests`、无打包路径）。
//!
//! 本批只交付构建侧：命令把库源码 + 生成的根模块入口编译并链接到 `target/test/`，
//! 不产出 `.dlib`、不要求可发布性（path 依赖可解析）。测试发现（H19-05b）与
//! 子进程执行/汇总（H19-05c）尚未实现，因此当前一律按冻结的 0 测试规则输出
//! `no tests found` 并以 1 退出。每个用例在可用后端 × Dolphin Debug/Release 上运行。

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_compiler::{
    BuildProfile, BuildSettings, build_test_target, build_tests, load_manifest,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dolphin-m19-test-{tag}-{}-{unique}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).expect("parent dir");
    fs::write(path, contents).expect("write file");
}

/// 一个隔离的临时根目录：多个子项目共享它以支持相对 path 依赖。
struct Project {
    root: PathBuf,
    home: PathBuf,
}

impl Project {
    fn new(tag: &str) -> Project {
        let root = temp_dir(tag);
        let home = root.join("home");
        fs::create_dir_all(&home).expect("isolated home");
        Project { root, home }
    }

    fn write(&self, relative: &str, contents: &str) {
        write(&self.root, relative, contents);
    }

    fn dir(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// 运行 `dc test <项目> [额外参数]`，使用隔离的 `DOLPHIN_HOME`。
    fn dc_test_in(&self, directory: &Path, extra: &[&str]) -> Output {
        self.dc_in(directory, "test", extra)
    }

    fn dc_in(&self, directory: &Path, command_name: &str, extra: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dc"));
        command
            .env("DOLPHIN_HOME", &self.home)
            .arg(command_name)
            .arg(directory)
            .args(extra);
        command.output().expect("dc should run")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn artifact_at(directory: &Path, name: &str) -> PathBuf {
    let base = directory.join("target").join("test").join(name);
    if cfg!(windows) {
        base.with_extension("exe")
    } else {
        base
    }
}

/// 递归收集目录下所有 `.dlib` 文件（`dc test` 必须不产出任何打包产物）。
fn dlibs_at(directory: &Path) -> Vec<PathBuf> {
    fn walk(directory: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "dlib")
            {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(directory, &mut found);
    found.sort();
    found
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// 直接运行测试二进制（内部 `--dolphin-test` 接口，H19-05c 的 runner 也用它）。
fn run_artifact(artifact: &Path, args: &[&str]) -> Output {
    Command::new(artifact)
        .args(args)
        .output()
        .expect("test binary should run")
}

/// 断言一次成功的 `dc test` 构建：0 测试冻结输出、无打包产物、测试二进制与目标文件存在。
fn assert_test_target_built(
    project: &Project,
    directory: &Path,
    artifact_name: &str,
    extra: &[&str],
    context: &str,
) {
    let output = project.dc_test_in(directory, extra);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{context} stdout={} stderr={}",
        stdout_text(&output),
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "no tests found\n", "{context}");
    assert!(
        output.stderr.is_empty(),
        "{context} stderr={}",
        stderr_text(&output)
    );

    let artifact = artifact_at(directory, artifact_name);
    assert!(
        artifact.is_file(),
        "{context}: missing test binary `{}`",
        artifact.display()
    );
    let object = directory
        .join("target")
        .join("test")
        .join(format!("{artifact_name}.o"));
    assert!(
        object.is_file(),
        "{context}: missing test object `{}`",
        object.display()
    );
    assert!(
        !directory.join("target").join("package").exists(),
        "{context}: `dc test` must not run the packaging path"
    );
    assert!(
        dlibs_at(directory).is_empty(),
        "{context}: `dc test` must not produce `.dlib`"
    );

    // 占位入口（H19-05b 前）本身是可执行且成功退出的；Debug 运行 stderr 必须为空
    // （exit=0 不证明无泄漏报告）。
    let run = Command::new(&artifact)
        .output()
        .expect("test binary should run");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{context}: placeholder entry stderr={}",
        stderr_text(&run)
    );
    assert!(
        run.stderr.is_empty(),
        "{context}: test binary stderr={}",
        stderr_text(&run)
    );
}

fn lib_manifest(name: &str, extra: &str) -> String {
    format!(
        "[package]\ngroup = \"org.example\"\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[lib]\npath = \"src/lib.do\"\n{extra}"
    )
}

/// TEST-05a：lib-only、lib+bin、path 依赖三种项目都能构建测试目标，且不走打包路径。
#[test]
fn test_05a_lib_only_bin_and_path_dep_targets() {
    let base = Project::new("a-targets");
    base.write("lib-only/dolphin.toml", &lib_manifest("alpha", ""));
    base.write(
        "lib-only/src/lib.do",
        "pub fn answer(): i32 { return 42; }\nfn hidden(): i32 { return 1; }\n",
    );
    base.write(
        "lib-bin/dolphin.toml",
        "[package]\ngroup = \"org.example\"\nname = \"beta\"\nversion = \"0.1.0\"\n\n[lib]\npath = \"src/lib.do\"\n\n[[bin]]\nname = \"beta\"\npath = \"src/main.do\"\n",
    );
    base.write("lib-bin/src/lib.do", "pub fn value(): i32 { return 7; }\n");
    base.write("lib-bin/src/main.do", "fn main(): i32 { return 0; }\n");
    base.write("dep/dolphin.toml", &lib_manifest("dep", ""));
    base.write("dep/src/lib.do", "pub fn base(): i32 { return 5; }\n");
    base.write(
        "consumer/dolphin.toml",
        &lib_manifest("gamma", "\n[dependencies]\ndep = { path = \"../dep\" }\n"),
    );
    base.write(
        "consumer/src/lib.do",
        "use dep;\npub fn total(): i32 { return dep.base() + 1; }\n",
    );

    for (relative, artifact) in [
        ("lib-only", "alpha-tests"),
        ("lib-bin", "beta-tests"),
        ("consumer", "gamma-tests"),
    ] {
        let directory = base.dir(relative);
        for backend in support::backends() {
            for release in [false, true] {
                let profile = if release { "--release" } else { "--debug" };
                let context = format!(
                    "dir={relative} artifact={artifact} backend={} release={release}",
                    backend.name()
                );
                assert_test_target_built(
                    &base,
                    &directory,
                    artifact,
                    &["--backend", backend.name(), profile],
                    &context,
                );
            }
        }
    }
}

/// H19-05a 根模块契约：生成入口与 `src/*.do` 同属根模块，可访问私有项与 `pub` 项。
///
/// 用驱动 API 注入调用私有/公开函数的入口，运行产物断言退出码；若入口被当作
/// 独立子模块，私有调用会被拒绝。
#[test]
fn test_05a_generated_entry_is_root_module() {
    let project = Project::new("a-entry");
    project.write("dolphin.toml", &lib_manifest("delta", ""));
    project.write(
        "src/lib.do",
        "pub fn value(): i32 { return 7; }\nfn secret(): i32 { return 1; }\n",
    );
    let manifest = load_manifest(&project.root).expect("manifest should load");
    let entry = "fn main(): i32 {\n    return secret() + value();\n}\n";

    for backend in support::backends() {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let artifact = build_tests(
                &manifest,
                profile,
                BuildSettings::with_backend(backend),
                entry,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "build_tests failed backend={} profile={profile:?}: {error}",
                    backend.name()
                )
            });
            let status = Command::new(&artifact.executable)
                .status()
                .expect("test binary should run");
            assert_eq!(
                status.code(),
                Some(8),
                "backend={} profile={profile:?}: generated entry must read private+public root items",
                backend.name()
            );
        }
    }
}

/// H19-05a 反例：编译失败、缺清单、bin-only、用法错误与 D1 打包限制不变。
#[test]
fn test_05a_errors_usage_and_no_packaging() {
    // 库源码错误：exit 1、诊断在 stderr、不产出测试二进制。
    let broken = Project::new("a-broken");
    broken.write("dolphin.toml", &lib_manifest("broken", ""));
    broken.write("src/lib.do", "pub fn broken(): i32 { return missing; }\n");
    let output = broken.dc_test_in(&broken.root, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_text(&output).contains("unknown variable `missing`"),
        "stderr={}",
        stderr_text(&output)
    );
    assert!(stdout_text(&output).is_empty());
    assert!(!artifact_at(&broken.root, "broken-tests").exists());

    // 没有清单：exit 1，不进入单文件模式。
    let empty = Project::new("a-empty");
    let output = empty.dc_test_in(&empty.root, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_text(&output).contains("no `dolphin.toml` found"),
        "stderr={}",
        stderr_text(&output)
    );

    // 仅 bin：明确拒绝，不静默构建无意义目标。
    let bin_only = Project::new("a-binonly");
    bin_only.write(
        "dolphin.toml",
        "[package]\ngroup = \"org.example\"\nname = \"epsilon\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"epsilon\"\npath = \"src/main.do\"\n",
    );
    bin_only.write("src/main.do", "fn main(): i32 { return 0; }\n");
    let output = bin_only.dc_test_in(&bin_only.root, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_text(&output).contains("requires a library target"),
        "stderr={}",
        stderr_text(&output)
    );
    assert!(!artifact_at(&bin_only.root, "epsilon-tests").exists());

    // `--locked` 无锁文件：exit 1。
    let locked = Project::new("a-locked");
    locked.write("dolphin.toml", &lib_manifest("zeta", ""));
    locked.write("src/lib.do", "pub fn value(): i32 { return 1; }\n");
    let output = locked.dc_test_in(&locked.root, &["--locked"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_text(&output).contains("--locked"),
        "stderr={}",
        stderr_text(&output)
    );

    // 用法错误：clap 拒绝未知参数并退出 2。
    let output = locked.dc_test_in(&locked.root, &["--bogus"]);
    assert_eq!(output.status.code(), Some(2));

    // `dc build`/`dc check` 永不读取 `tests/`（H19-05 的长期契约）。
    let ignored = Project::new("a-ignored-tests");
    ignored.write("dolphin.toml", &lib_manifest("theta", ""));
    ignored.write("src/lib.do", "pub fn value(): i32 { return 1; }\n");
    ignored.write("tests/not_valid.do", "this is not valid dolphin\n");
    let checked = ignored.dc_in(&ignored.root, "check", &[]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "dc check must ignore tests/: stderr={}",
        stderr_text(&checked)
    );
    let built = ignored.dc_in(&ignored.root, "build", &["--lib"]);
    assert_eq!(
        built.status.code(),
        Some(0),
        "dc build must ignore tests/: stderr={}",
        stderr_text(&built)
    );

    // D1 回归：`dc test` 不要求可发布性，但 `dc package` 仍拒绝 path 依赖。
    let base = Project::new("a-packaging");
    base.write("dep2/dolphin.toml", &lib_manifest("dep2", ""));
    base.write("dep2/src/lib.do", "pub fn base(): i32 { return 5; }\n");
    base.write(
        "consumer2/dolphin.toml",
        &lib_manifest("eta", "\n[dependencies]\ndep2 = { path = \"../dep2\" }\n"),
    );
    base.write(
        "consumer2/src/lib.do",
        "use dep2;\npub fn total(): i32 { return dep2.base(); }\n",
    );
    let consumer = base.dir("consumer2");

    let built = base.dc_test_in(&consumer, &[]);
    assert_eq!(
        built.status.code(),
        Some(1),
        "stderr={}",
        stderr_text(&built)
    );
    assert_eq!(stdout_text(&built), "no tests found\n");
    assert!(dlibs_at(&consumer).is_empty());

    let mut command = Command::new(env!("CARGO_BIN_EXE_dc"));
    let packaged = command
        .env("DOLPHIN_HOME", &base.home)
        .arg("package")
        .arg(&consumer)
        .output()
        .expect("dc package should run");
    assert_eq!(packaged.status.code(), Some(1));
    assert!(
        stderr_text(&packaged).contains("path dependency"),
        "stderr={}",
        stderr_text(&packaged)
    );
}

/// TEST-05b：`tests/` 直接子文件发现（不递归）、`test_*` 排序与 helper 排除。
#[test]
fn test_05b_discovery_direct_children_order_and_helpers() {
    let base = Project::new("b-discovery");
    base.write("proj/dolphin.toml", &lib_manifest("discover", ""));
    base.write(
        "proj/src/lib.do",
        "pub fn root_value(): i32 { return 1; }\n",
    );
    base.write(
        "proj/tests/b_file.do",
        "fn helper_from_b(): i32 { return 40; }\nfn test_zeta() { }\nfn test_alpha() { }\n",
    );
    base.write("proj/tests/a_file.do", "fn test_beta() { }\n");
    base.write("proj/tests/nested/ignored.do", "fn test_nested() { }\n");
    base.write("proj/tests/notes.txt", "not a test\n");
    let directory = base.dir("proj");

    let manifest = load_manifest(&directory).expect("manifest should load");
    for backend in support::backends() {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let target =
                build_test_target(&manifest, profile, BuildSettings::with_backend(backend))
                    .unwrap_or_else(|error| {
                        panic!(
                            "build_test_target failed backend={} profile={profile:?}: {error}",
                            backend.name()
                        )
                    });
            let names: Vec<&str> = target.tests.iter().map(|test| test.name.as_str()).collect();
            assert_eq!(
                names,
                ["test_alpha", "test_beta", "test_zeta"],
                "backend={} profile={profile:?}",
                backend.name()
            );
            assert!(target.tests[0].path.ends_with("b_file.do"));
            assert!(target.tests[1].path.ends_with("a_file.do"));
            assert!(target.tests[2].path.ends_with("b_file.do"));
            assert!(target.artifact.executable.is_file());
        }
    }

    // CLI：发现 3 个测试 -> 固定临时输出，未执行前以 1 退出。
    for backend in support::backends() {
        for release in [false, true] {
            let profile = if release { "--release" } else { "--debug" };
            let context = format!("backend={} release={release}", backend.name());
            let output = base.dc_test_in(&directory, &["--backend", backend.name(), profile]);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{context} stdout={} stderr={}",
                stdout_text(&output),
                stderr_text(&output)
            );
            assert_eq!(
                stdout_text(&output),
                "built 3 tests (execution lands in H19-05c)\n",
                "{context}"
            );
            assert!(
                output.stderr.is_empty(),
                "{context} stderr={}",
                stderr_text(&output)
            );
            assert!(
                artifact_at(&directory, "discover-tests").is_file(),
                "{context}: missing test binary"
            );
        }
    }
}

/// TEST-05b：harness 按内部名称分发；断言失败固定 106 文本；私有项与跨文件 helper 可见。
#[test]
fn test_05b_harness_dispatch_and_assertions() {
    let base = Project::new("b-harness");
    base.write("proj/dolphin.toml", &lib_manifest("harness", ""));
    base.write(
        "proj/src/lib.do",
        "pub fn base(): i32 { return 1; }\nfn secret(): i32 { return 41; }\n",
    );
    base.write(
        "proj/tests/checks.do",
        "use std.test.expect;\nuse std.test.fail;\n\nfn test_pass() {\n    expect(secret() + base() == 42);\n}\n\nfn test_expect_false() {\n    expect(false);\n}\n\nfn test_fail() {\n    fail();\n}\n\nfn test_helper() {\n    expect(helper_with(40) == 40);\n    helper();\n}\n",
    );
    base.write(
        "proj/tests/helpers.do",
        "fn helper() { }\nfn helper_with(value: i32): i32 { return value; }\n",
    );
    let directory = base.dir("proj");

    for backend in support::backends() {
        for release in [false, true] {
            let profile = if release { "--release" } else { "--debug" };
            let context = format!("backend={} release={release}", backend.name());
            let output = base.dc_test_in(&directory, &["--backend", backend.name(), profile]);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{context} stdout={} stderr={}",
                stdout_text(&output),
                stderr_text(&output)
            );
            assert_eq!(
                stdout_text(&output),
                "built 4 tests (execution lands in H19-05c)\n",
                "{context}"
            );
            let artifact = artifact_at(&directory, "harness-tests");

            let pass = run_artifact(&artifact, &["--dolphin-test", "test_pass"]);
            assert_eq!(
                pass.status.code(),
                Some(0),
                "{context} stderr={}",
                stderr_text(&pass)
            );
            assert!(
                pass.stderr.is_empty(),
                "{context} stderr={}",
                stderr_text(&pass)
            );

            let helper = run_artifact(&artifact, &["--dolphin-test", "test_helper"]);
            assert_eq!(
                helper.status.code(),
                Some(0),
                "{context} stderr={}",
                stderr_text(&helper)
            );

            for name in ["test_expect_false", "test_fail"] {
                let failed = run_artifact(&artifact, &["--dolphin-test", name]);
                assert_eq!(
                    failed.status.code(),
                    Some(106),
                    "{context} name={name} stderr={}",
                    stderr_text(&failed)
                );
                assert_eq!(
                    stderr_text(&failed),
                    "Dolphin test assertion failed\n",
                    "{context} name={name}"
                );
                assert!(stdout_text(&failed).is_empty(), "{context} name={name}");
            }

            let unknown = run_artifact(&artifact, &["--dolphin-test", "test_missing"]);
            assert_eq!(unknown.status.code(), Some(3), "{context}: unknown test");
            let no_args = run_artifact(&artifact, &[]);
            assert_eq!(no_args.status.code(), Some(2), "{context}: no arguments");
            let missing_name = run_artifact(&artifact, &["--dolphin-test"]);
            assert_eq!(
                missing_name.status.code(),
                Some(2),
                "{context}: missing test name"
            );
            let wrong_flag = run_artifact(&artifact, &["--other", "test_pass"]);
            assert_eq!(wrong_flag.status.code(), Some(2), "{context}: wrong flag");
        }
    }
}

/// 写一个只有测试文件差异的项目并断言 `dc test` 以固定诊断拒绝。
fn assert_test_rejected(tag: &str, package: &str, test_source: &str, needle: &str) {
    let project = Project::new(tag);
    project.write("dolphin.toml", &lib_manifest(package, ""));
    project.write("src/lib.do", "pub fn value(): i32 { return 1; }\n");
    project.write("tests/case.do", test_source);
    let output = project.dc_test_in(&project.root, &[]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{tag}: stdout={} stderr={}",
        stdout_text(&output),
        stderr_text(&output)
    );
    assert!(
        stderr_text(&output).contains(needle),
        "{tag}: expected `{needle}` in stderr={}",
        stderr_text(&output)
    );
    assert!(
        !artifact_at(&project.root, &format!("{package}-tests")).exists(),
        "{tag}: rejected project must not produce a test binary"
    );
}

/// TEST-05b 可见性与反例：子模块 `pub` 可见、私有不可见；`pkg`/`main`/签名/重复/语法错误被拒。
#[test]
fn test_05b_visibility_and_rejections() {
    // 正例：测试可访问子模块 `pub` 项（根模块私有项见 harness 用例的 `secret()`）。
    let base = Project::new("b-visibility");
    base.write("proj/dolphin.toml", &lib_manifest("visibility", ""));
    base.write("proj/src/lib.do", "pub fn value(): i32 { return 1; }\n");
    base.write(
        "proj/src/util/helpers.do",
        "pkg util;\npub fn shown(): i32 { return 2; }\nfn hidden(): i32 { return 1; }\n",
    );
    base.write(
        "proj/tests/vis.do",
        "use std.test.expect;\nuse util.helpers;\n\nfn test_ok() {\n    expect(helpers.shown() == 2);\n}\n",
    );
    let directory = base.dir("proj");
    for backend in support::backends() {
        for release in [false, true] {
            let profile = if release { "--release" } else { "--debug" };
            let context = format!("backend={} release={release}", backend.name());
            let output = base.dc_test_in(&directory, &["--backend", backend.name(), profile]);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{context} stdout={} stderr={}",
                stdout_text(&output),
                stderr_text(&output)
            );
            let artifact = artifact_at(&directory, "visibility-tests");
            let ok = run_artifact(&artifact, &["--dolphin-test", "test_ok"]);
            assert_eq!(
                ok.status.code(),
                Some(0),
                "{context} stderr={}",
                stderr_text(&ok)
            );
        }
    }

    // 子模块私有项：可见性拒绝。
    let private = Project::new("b-private");
    private.write("proj/dolphin.toml", &lib_manifest("privacy", ""));
    private.write("proj/src/lib.do", "pub fn value(): i32 { return 1; }\n");
    private.write(
        "proj/src/util/helpers.do",
        "pkg util;\npub fn shown(): i32 { return 2; }\nfn hidden(): i32 { return 1; }\n",
    );
    private.write(
        "proj/tests/vis.do",
        "use std.test.expect;\nuse util.helpers;\n\nfn test_private() {\n    expect(helpers.hidden() == 1);\n}\n",
    );
    let output = private.dc_test_in(&private.dir("proj"), &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_text(&output).contains("is private"),
        "stderr={}",
        stderr_text(&output)
    );

    assert_test_rejected(
        "b-pkg",
        "pkgtest",
        "pkg tests;\nfn test_x() { }\n",
        "must omit `pkg`",
    );
    assert_test_rejected(
        "b-main",
        "maintest",
        "fn main() { }\nfn test_x() { }\n",
        "must not define `main`",
    );
    assert_test_rejected(
        "b-params",
        "paramtest",
        "fn test_x(value: i32) { }\n",
        "must not take parameters",
    );
    assert_test_rejected(
        "b-typeparams",
        "typeparamtest",
        "fn test_x<T>() { }\n",
        "must not declare type parameters",
    );
    assert_test_rejected(
        "b-return",
        "returntest",
        "fn test_x(): i32 { return 0; }\n",
        "must return Unit",
    );
    assert_test_rejected(
        "b-extern",
        "externtest",
        "extern \"C\" { fn test_x(); }\n",
        "must be a function with a body",
    );
    assert_test_rejected("b-syntax", "syntaxtest", "fn test_x( {\n", "error[");

    // 同名测试：同属根模块，发现阶段给出明确诊断。
    let duplicate = Project::new("b-duplicate");
    duplicate.write("proj/dolphin.toml", &lib_manifest("duptest", ""));
    duplicate.write("proj/src/lib.do", "pub fn value(): i32 { return 1; }\n");
    duplicate.write("proj/tests/one.do", "fn test_dup() { }\n");
    duplicate.write("proj/tests/two.do", "fn test_dup() { }\n");
    let output = duplicate.dc_test_in(&duplicate.dir("proj"), &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_text(&output).contains("duplicate test `test_dup`"),
        "stderr={}",
        stderr_text(&output)
    );
}
