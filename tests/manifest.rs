use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_compiler::{
    BuildProfile, BuildSettings, build_library, build_manifest, check_manifest, extract,
    load_manifest, read_metadata,
};

mod support;

static NEXT_PROJECT_ID: AtomicU64 = AtomicU64::new(0);

fn temp_project() -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let project = std::env::temp_dir().join(format!(
        "dolphin-manifest-test-{}-{unique}-{}",
        std::process::id(),
        NEXT_PROJECT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&project).expect("temporary project directory should be created");
    project
}

fn write(path: &std::path::Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).expect("parent directory should be created");
    fs::write(path, contents).expect("file should be written");
}

/// 按平台规则拼出可执行文件路径（Windows 追加 `.exe`）。
fn executable_path(project: &std::path::Path, relative: &str) -> std::path::PathBuf {
    let path = project.join(relative);
    if cfg!(windows) {
        path.with_extension("exe")
    } else {
        path
    }
}

#[test]
fn parses_manifest_and_builds_single_bin() {
    let project = temp_project();
    write(
        &project.join("dolphin.toml"),
        r#"
        [package]
        group = "me.foxlab"
        name = "hello"
        version = "0.1.0"

        [[bin]]
        name = "hello"
        path = "src/main.do"
        "#,
    );
    write(&project.join("src/main.do"), "fn main() { return 42; }");

    let manifest = load_manifest(&project).expect("manifest should load");
    assert_eq!(manifest.package.coordinate(), "me.foxlab:hello:0.1.0");

    check_manifest(&manifest).expect("manifest should check");
    let artifacts = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .expect("manifest should build");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(
        artifacts[0].executable,
        executable_path(&project, "target/hello")
    );

    let output = Command::new(&artifacts[0].executable)
        .output()
        .expect("executable should run");
    assert_eq!(output.status.code(), Some(42));

    fs::remove_dir_all(project).expect("temporary project should be removed");
}

#[test]
fn builds_all_and_selects_single_bin_from_multiple() {
    let project = temp_project();
    write(
        &project.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "app"
        version = "1.2.3"
        source = "src"

        [[bin]]
        name = "cli"
        path = "src/cli.do"

        [[bin]]
        name = "server"
        path = "src/server.do"

        [build]
        optimization = "debug"
        output = "dist"
        "#,
    );
    write(&project.join("src/cli.do"), "fn main() { return 1; }");
    write(&project.join("src/server.do"), "fn main() { return 2; }");

    let manifest = load_manifest(&project).expect("manifest should load");

    let all = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .expect("all bins should build");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].executable, executable_path(&project, "dist/cli"));
    assert_eq!(all[1].executable, executable_path(&project, "dist/server"));

    let server = build_manifest(
        &manifest,
        Some("server"),
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .expect("server should build");
    assert_eq!(server.len(), 1);
    let output = Command::new(&server[0].executable)
        .output()
        .expect("server should run");
    assert_eq!(output.status.code(), Some(2));

    fs::remove_dir_all(project).expect("temporary project should be removed");
}

#[test]
fn bins_share_modules_but_have_independent_entry_points() {
    let project = temp_project();
    write(
        &project.join("dolphin.toml"),
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
    );
    write(
        &project.join("src/util.do"),
        "fn double(value: i32): i32 { return value * 2; }",
    );
    write(
        &project.join("src/a.do"),
        "fn main() { return double(21); }",
    );
    write(&project.join("src/b.do"), "fn main() { return double(3); }");

    let manifest = load_manifest(&project).expect("manifest should load");
    let artifacts = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .expect("bins should build");

    let a = Command::new(&artifacts[0].executable)
        .output()
        .expect("a should run");
    let b = Command::new(&artifacts[1].executable)
        .output()
        .expect("b should run");
    assert_eq!(a.status.code(), Some(42));
    assert_eq!(b.status.code(), Some(6));

    fs::remove_dir_all(project).expect("temporary project should be removed");
}

#[test]
fn rejects_unknown_bin_selection() {
    let project = temp_project();
    write(
        &project.join("dolphin.toml"),
        r#"
        [package]
        group = "g"
        name = "n"
        version = "0.1.0"

        [[bin]]
        name = "a"
        path = "src/a.do"
        "#,
    );
    write(&project.join("src/a.do"), "fn main() { return 0; }");

    let manifest = load_manifest(&project).expect("manifest should load");
    let error = build_manifest(
        &manifest,
        Some("missing"),
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unknown binary target `missing`")
    );
    assert!(error.to_string().contains("a"));

    fs::remove_dir_all(project).expect("temporary project should be removed");
}

#[test]
fn rejects_missing_entry_file() {
    let project = temp_project();
    write(
        &project.join("dolphin.toml"),
        r#"
        [package]
        group = "g"
        name = "n"
        version = "0.1.0"

        [[bin]]
        name = "a"
        path = "src/does_not_exist.do"
        "#,
    );

    let manifest = load_manifest(&project).expect("manifest should load");
    let error = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("does not exist"));

    fs::remove_dir_all(project).expect("temporary project should be removed");
}

#[test]
fn parse_error_reports_location() {
    let project = temp_project();
    write(&project.join("dolphin.toml"), "[package\nname = \"n\"\n");

    let error = load_manifest(&project).unwrap_err();
    assert!(error.to_string().contains("dolphin.toml"));

    fs::remove_dir_all(project).expect("temporary project should be removed");
}

#[test]
fn discover_manifest_walks_up_directories() {
    let project = temp_project();
    write(
        &project.join("dolphin.toml"),
        r#"
        [package]
        group = "g"
        name = "n"
        version = "0.1.0"

        [[bin]]
        name = "n"
        path = "src/main.do"
        "#,
    );
    write(&project.join("src/main.do"), "fn main() { return 0; }");

    let nested = project.join("a").join("b").join("c");
    fs::create_dir_all(&nested).expect("nested directory should be created");
    let manifest = dolphin_compiler::discover_manifest(&nested).expect("discovery should succeed");
    assert!(manifest.is_some());
    assert_eq!(manifest.unwrap().package.coordinate(), "g:n:0.1.0");

    fs::remove_dir_all(project).expect("temporary project should be removed");
}

#[test]
fn discover_manifest_returns_none_without_manifest() {
    let project = temp_project();
    let found = dolphin_compiler::discover_manifest(&project).expect("discovery should not error");
    assert!(found.is_none());
    fs::remove_dir_all(project).expect("temporary project should be removed");
}

/// 写一个最小库项目，返回其根目录。
fn library_project(root: &std::path::Path, group: &str, name: &str, version: &str, source: &str) {
    write(
        &root.join("dolphin.toml"),
        &format!(
            r#"
            [package]
            group = "{group}"
            name = "{name}"
            version = "{version}"

            [lib]
            path = "src/lib.do"
            "#
        ),
    );
    write(&root.join("src/lib.do"), source);
}

#[test]
fn pkg01_lib_only_builds_validation_object() {
    let project = temp_project();
    library_project(
        &project,
        "org.example",
        "mathlib",
        "1.0.0",
        "pub fn add(a: i32, b: i32): i32 { return a + b; }",
    );

    let manifest = load_manifest(&project).expect("manifest should load");
    assert!(manifest.lib.is_some());
    assert!(manifest.bins.is_empty());
    check_manifest(&manifest).expect("library should check");

    let artifact = build_library(&manifest, BuildProfile::Debug, BuildSettings::default())
        .expect("library should build");
    assert!(artifact.object.is_file());
    let package = artifact
        .package
        .expect("build --lib should produce a .dlib");
    assert!(package.is_file());
    assert_eq!(
        package.file_name().and_then(|name| name.to_str()),
        Some("mathlib-1.0.0.dlib")
    );
    let checksum = package.with_file_name("mathlib-1.0.0.dlib.sha256");
    assert!(checksum.is_file());

    // 纯库没有可执行目标。
    let bins = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .expect("manifest build should succeed");
    assert!(bins.is_empty());

    fs::remove_dir_all(project).expect("temporary project should be removed");
}

#[test]
fn pkg01_lib_plus_bins_builds_both() {
    let project = temp_project();
    write(
        &project.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "multi"
        version = "0.1.0"

        [lib]
        path = "src/lib.do"

        [[bin]]
        name = "first"
        path = "src/first.do"

        [[bin]]
        name = "second"
        path = "src/second.do"
        "#,
    );
    write(
        &project.join("src/lib.do"),
        "pub fn value(): i32 { return 40; }",
    );
    write(
        &project.join("src/shared.do"),
        "pub fn base(): i32 { return 2; }",
    );
    write(
        &project.join("src/first.do"),
        "fn main() { return base(); }",
    );
    write(
        &project.join("src/second.do"),
        "fn main() { return value() + 1; }",
    );

    let manifest = load_manifest(&project).expect("manifest should load");
    check_manifest(&manifest).expect("lib and bins should check");
    let artifact = build_library(&manifest, BuildProfile::Debug, BuildSettings::default())
        .expect("library should build");
    assert!(artifact.object.is_file());

    let artifacts = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .expect("bins should build");
    assert_eq!(artifacts.len(), 2);
    let first = Command::new(&artifacts[0].executable)
        .output()
        .expect("first should run");
    let second = Command::new(&artifacts[1].executable)
        .output()
        .expect("second should run");
    assert_eq!(first.status.code(), Some(2));
    assert_eq!(second.status.code(), Some(41));

    fs::remove_dir_all(project).expect("temporary project should be removed");
}

/// 建立 app + lib 两个项目；`lib_source` 是库源码，`app_source` 是应用入口。
fn path_dependency_pair(lib_source: &str, app_source: &str) -> std::path::PathBuf {
    let base = temp_project();
    let lib = base.join("mathlib");
    let app = base.join("app");
    library_project(&lib, "org.example", "mathlib", "1.0.0", lib_source);
    write(
        &app.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "app"
        version = "0.1.0"

        [[bin]]
        name = "app"
        path = "src/main.do"

        [dependencies]
        math = { path = "../mathlib" }
        "#,
    );
    write(&app.join("src/main.do"), app_source);
    app
}

#[test]
fn pkg02_path_dependency_builds_and_runs() {
    let app = path_dependency_pair(
        "pub fn add(a: i32, b: i32): i32 { return a + b; }\npub fn twice<T>(value: T): T { return value + value; }",
        "use math.add;\nuse math.twice;\nfn main() { return add(twice<i32>(20), 2); }",
    );
    let manifest = load_manifest(&app).expect("manifest should load");
    let artifacts = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .expect("app should build");
    let output = Command::new(&artifacts[0].executable)
        .output()
        .expect("app should run");
    assert_eq!(output.status.code(), Some(42));
    fs::remove_dir_all(app.parent().unwrap()).expect("temporary project should be removed");
}

#[test]
fn gen02_gen05_cross_package_names_and_private_helpers() {
    let app = path_dependency_pair(
        r#"
        fn secret(value: i32): i32 { return value + 1; }
        pub fn bump(value: i32): i32 { return secret(value); }
        pub fn add(a: i32, b: i32): i32 { return a + b; }
        pub struct Pair<T> { pub first: T, pub second: T }
        "#,
        r#"
        use math;

        fn add(a: i32, b: i32): i32 { return a * 1000 + b; }

        fn main() {
            val pair = math.Pair<i32>(math.bump(20), 21);
            if add(1, 2) != 1002 { return 7; }
            return math.add(pair.first, pair.second);
        }
        "#,
    );
    let manifest = load_manifest(&app).expect("manifest should load");
    let artifacts = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .expect("same-named cross-package definitions should not collide");
    let output = Command::new(&artifacts[0].executable)
        .output()
        .expect("app should run");
    assert_eq!(output.status.code(), Some(42));

    // 私有 helper 不能跨包访问。
    let private = path_dependency_pair(
        "fn secret(value: i32): i32 { return value; }\npub fn ok(): i32 { return 0; }",
        "use math;\nfn main() { return math.secret(1); }",
    );
    let manifest = load_manifest(&private).expect("manifest should load");
    let error = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("private"));

    fs::remove_dir_all(app.parent().unwrap()).expect("temporary project should be removed");
    fs::remove_dir_all(private.parent().unwrap()).expect("temporary project should be removed");
}

#[test]
fn pkg02_dependency_must_declare_lib() {
    let base = temp_project();
    let dep = base.join("binonly");
    write(
        &dep.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "binonly"
        version = "1.0.0"

        [[bin]]
        name = "tool"
        path = "src/main.do"
        "#,
    );
    write(&dep.join("src/main.do"), "fn main() { return 0; }");
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "app"
        version = "0.1.0"

        [[bin]]
        name = "app"
        path = "src/main.do"

        [dependencies]
        tool = { path = "../binonly" }
        "#,
    );
    write(&app.join("src/main.do"), "fn main() { return 0; }");

    let manifest = load_manifest(&app).expect("manifest should load");
    let error = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("[lib]"));
    fs::remove_dir_all(base).expect("temporary project should be removed");
}

#[test]
fn pkg06_dependency_cycle_and_version_conflict_report_chains() {
    // A -> B -> A 环。
    let base = temp_project();
    let a = base.join("a");
    let b = base.join("b");
    write(
        &a.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "a"
        version = "1.0.0"

        [lib]
        path = "src/lib.do"

        [dependencies]
        b = { path = "../b" }
        "#,
    );
    write(&a.join("src/lib.do"), "pub fn a(): i32 { return 1; }");
    write(
        &b.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "b"
        version = "1.0.0"

        [lib]
        path = "src/lib.do"

        [dependencies]
        a = { path = "../a" }
        "#,
    );
    write(&b.join("src/lib.do"), "pub fn b(): i32 { return 2; }");

    let root = base.join("root");
    write(
        &root.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "root"
        version = "0.1.0"

        [[bin]]
        name = "root"
        path = "src/main.do"

        [dependencies]
        a = { path = "../a" }
        "#,
    );
    write(&root.join("src/main.do"), "fn main() { return 0; }");
    let manifest = load_manifest(&root).expect("manifest should load");
    let error = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cycle"), "got: {error}");

    // 同一 (group, name) 两个版本冲突。
    let left = base.join("left");
    library_project(
        &left,
        "org.example",
        "left",
        "1.0.0",
        "pub fn l(): i32 { return 0; }",
    );
    let right = base.join("right");
    library_project(
        &right,
        "org.example",
        "right",
        "1.0.0",
        "pub fn r(): i32 { return 0; }",
    );
    let shared_v1 = base.join("shared-v1");
    library_project(
        &shared_v1,
        "org.example",
        "shared",
        "1.0.0",
        "pub fn s(): i32 { return 1; }",
    );
    let shared_v2 = base.join("shared-v2");
    library_project(
        &shared_v2,
        "org.example",
        "shared",
        "2.0.0",
        "pub fn s(): i32 { return 2; }",
    );
    // left/right 各自依赖不同版本的 shared。
    fs::write(
        left.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "left"
        version = "1.0.0"

        [lib]
        path = "src/lib.do"

        [dependencies]
        shared = { path = "../shared-v1" }
        "#,
    )
    .unwrap();
    fs::write(
        right.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "right"
        version = "1.0.0"

        [lib]
        path = "src/lib.do"

        [dependencies]
        shared = { path = "../shared-v2" }
        "#,
    )
    .unwrap();
    let diamond = base.join("diamond");
    write(
        &diamond.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "diamond"
        version = "0.1.0"

        [[bin]]
        name = "diamond"
        path = "src/main.do"

        [dependencies]
        left = { path = "../left" }
        right = { path = "../right" }
        "#,
    );
    write(&diamond.join("src/main.do"), "fn main() { return 0; }");
    let manifest = load_manifest(&diamond).expect("manifest should load");
    let error = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("conflict"), "got: {error}");

    fs::remove_dir_all(base).expect("temporary project should be removed");
}

/// 读取 `.dlib` 字节。
fn package_bytes(manifest: &dolphin_compiler::Manifest) -> Vec<u8> {
    let artifact = build_library(manifest, BuildProfile::Debug, BuildSettings::default())
        .expect("library should build");
    let path = artifact.package.expect("package should be produced");
    fs::read(path).expect("package should be readable")
}

#[test]
fn pkg03_package_is_deterministic_and_tracks_source_changes() {
    let project = temp_project();
    library_project(
        &project,
        "org.example",
        "mathlib",
        "1.0.0",
        "fn helper(value: i32): i32 { return value + 1; }\npub fn add(a: i32, b: i32): i32 { return helper(a) + b; }",
    );
    let manifest = load_manifest(&project).expect("manifest should load");
    let first = package_bytes(&manifest);
    let second = package_bytes(&manifest);
    assert_eq!(first, second, "same input must produce identical bytes");

    write(
        &project.join("src/lib.do"),
        "fn helper(value: i32): i32 { return value + 2; }\npub fn add(a: i32, b: i32): i32 { return helper(a) + b; }",
    );
    let manifest = load_manifest(&project).expect("manifest should reload");
    let changed = package_bytes(&manifest);
    assert_ne!(
        first, changed,
        "source change must change the package digest"
    );

    fs::remove_dir_all(project).expect("temporary project should be removed");
}

#[test]
fn pkg04_package_excludes_bins_and_rejects_path_dependencies() {
    let project = temp_project();
    write(
        &project.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "mixed"
        version = "0.1.0"

        [lib]
        path = "src/lib.do"

        [[bin]]
        name = "tool"
        path = "src/tool.do"
        "#,
    );
    write(
        &project.join("src/lib.do"),
        "pub fn value(): i32 { return 1; }",
    );
    write(&project.join("src/tool.do"), "fn main() { return 0; }");
    let manifest = load_manifest(&project).expect("manifest should load");
    let bytes = package_bytes(&manifest);
    let unpacked = project.join("unpacked");
    extract(&bytes, &unpacked).expect("package should extract");
    assert!(unpacked.join("src/lib.do").is_file());
    assert!(
        !unpacked.join("src/tool.do").exists(),
        "bin entry must be excluded"
    );
    assert!(unpacked.join("dolphin.toml").is_file());
    assert!(unpacked.join("META-INF/dolphin-package.toml").is_file());

    let with_path = path_dependency_pair(
        "pub fn add(a: i32, b: i32): i32 { return a + b; }",
        "fn main() { return 0; }",
    );
    // 把 app 改成依赖 mathlib 的库，验证 path 依赖不能被发布。
    let wrapper = with_path.parent().unwrap().join("wrapper");
    write(
        &wrapper.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "wrapper"
        version = "0.1.0"

        [lib]
        path = "src/lib.do"

        [dependencies]
        math = { path = "../mathlib" }
        "#,
    );
    write(
        &wrapper.join("src/lib.do"),
        "use math;\npub fn wrap(): i32 { return math.add(1, 2); }",
    );
    let dep_manifest = load_manifest(&wrapper).expect("manifest should load");
    let error =
        build_library(&dep_manifest, BuildProfile::Debug, BuildSettings::default()).unwrap_err();
    assert!(
        error.to_string().contains("path dependency"),
        "got: {error}"
    );

    fs::remove_dir_all(project).expect("temporary project should be removed");
    fs::remove_dir_all(with_path.parent().unwrap()).expect("temporary project should be removed");
}

#[test]
fn pkg11_native_files_are_packaged_with_digests() {
    let project = temp_project();
    write(
        &project.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "nativelib"
        version = "0.1.0"

        [lib]
        path = "src/lib.do"

        [native.x86_64-unknown-linux-gnu]
        static-libs = ["native/libdemo.a"]
        "#,
    );
    write(
        &project.join("src/lib.do"),
        "pub fn value(): i32 { return 1; }",
    );
    write(&project.join("native/libdemo.a"), "fake archive contents");
    let manifest = load_manifest(&project).expect("manifest should load");
    let bytes = package_bytes(&manifest);
    let metadata = read_metadata(&bytes).expect("metadata should parse");
    assert_eq!(
        metadata.targets,
        vec!["x86_64-unknown-linux-gnu".to_string()]
    );
    assert_eq!(metadata.kind, "source");
    assert_eq!(metadata.coordinate, "org.example:nativelib:0.1.0");
    assert_eq!(metadata.compiler_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(metadata.native_files.len(), 1);
    assert!(metadata.native_files[0].path.ends_with("libdemo.a"));

    let unpacked = project.join("unpacked");
    extract(&bytes, &unpacked).expect("package should extract");
    assert!(
        unpacked
            .join("native/x86_64-unknown-linux-gnu/libdemo.a")
            .is_file()
    );
    let normalized = fs::read_to_string(unpacked.join("dolphin.toml")).unwrap();
    assert!(normalized.contains("[native.x86_64-unknown-linux-gnu]"));
    assert!(!normalized.contains("[[bin]]"));

    fs::remove_dir_all(project).expect("temporary project should be removed");
}

#[test]
fn gen02_same_library_generic_used_by_two_packages() {
    let base = temp_project();
    let shared = base.join("shared");
    library_project(
        &shared,
        "org.example",
        "shared",
        "1.0.0",
        "pub fn twice<T>(value: T): T { return value + value; }",
    );
    for name in ["left", "right"] {
        let dir = base.join(name);
        write(
            &dir.join("dolphin.toml"),
            &format!(
                r#"
                [package]
                group = "org.example"
                name = "{name}"
                version = "1.0.0"

                [lib]
                path = "src/lib.do"

                [dependencies]
                shared = {{ path = "../shared" }}
                "#
            ),
        );
        write(
            &dir.join("src/lib.do"),
            &format!("use shared;\npub fn {name}(): i32 {{ return shared.twice<i32>(1); }}"),
        );
    }
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "app"
        version = "0.1.0"

        [[bin]]
        name = "app"
        path = "src/main.do"

        [dependencies]
        left = { path = "../left" }
        right = { path = "../right" }
        "#,
    );
    write(
        &app.join("src/main.do"),
        "use left;\nuse right;\nfn main() { return left.left() + right.right(); }",
    );
    let manifest = load_manifest(&app).expect("manifest should load");
    let artifacts = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .expect("the same library generic used by two packages must dedup");
    let output = Command::new(&artifacts[0].executable)
        .output()
        .expect("app should run");
    // 2 + 2
    assert_eq!(output.status.code(), Some(4));
    fs::remove_dir_all(base).expect("temporary project should be removed");
}

#[test]
fn pkg10_pkg11_native_target_and_runtime_conflicts() {
    use dolphin_compiler::host_platform;

    // PKG-10：包只提供其他目标的原生文件时，在链接前报目标不兼容。
    let base = temp_project();
    let native_lib = base.join("native-lib");
    write(
        &native_lib.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "nativelib"
        version = "1.0.0"

        [lib]
        path = "src/lib.do"

        [native.wasm32-unknown-unknown]
        static-libs = ["native/libdemo.a"]
        "#,
    );
    write(
        &native_lib.join("src/lib.do"),
        "pub fn value(): i32 { return 1; }",
    );
    write(&native_lib.join("native/libdemo.a"), "demo");
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "app"
        version = "0.1.0"

        [[bin]]
        name = "app"
        path = "src/main.do"

        [dependencies]
        native = { path = "../native-lib" }
        "#,
    );
    write(
        &app.join("src/main.do"),
        "use native;\nfn main() { return native.value(); }",
    );
    let manifest = load_manifest(&app).expect("manifest should load");
    let error = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("native"), "got: {error}");

    // PKG-11：两个依赖提供同名不同内容的 runtime 文件时报链接冲突。
    let triple = host_platform().expect("host platform").triple().to_string();
    let base2 = temp_project();
    for (name, contents) in [("alpha", "AAA"), ("beta", "BBB")] {
        let dir = base2.join(name);
        write(
            &dir.join("dolphin.toml"),
            &format!(
                r#"
                [package]
                group = "org.example"
                name = "{name}"
                version = "1.0.0"

                [lib]
                path = "src/lib.do"

                [native.{triple}]
                runtime-files = ["native/shared.dat"]
                "#
            ),
        );
        write(
            &dir.join("src/lib.do"),
            &format!("pub fn {name}(): i32 {{ return 1; }}"),
        );
        write(&dir.join("native/shared.dat"), contents);
    }
    let app2 = base2.join("app");
    write(
        &app2.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "app"
        version = "0.1.0"

        [[bin]]
        name = "app"
        path = "src/main.do"

        [dependencies]
        alpha = { path = "../alpha" }
        beta = { path = "../beta" }
        "#,
    );
    write(
        &app2.join("src/main.do"),
        "use alpha;\nuse beta;\nfn main() { return alpha.alpha() + beta.beta(); }",
    );
    let manifest2 = load_manifest(&app2).expect("manifest should load");
    let conflict = build_manifest(
        &manifest2,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .unwrap_err();
    assert!(conflict.to_string().contains("conflict"), "got: {conflict}");

    fs::remove_dir_all(base).expect("temporary project should be removed");
    fs::remove_dir_all(base2).expect("temporary project should be removed");
}

#[test]
fn pkg02_diamond_shared_dependency_is_loaded_once() {
    let base = temp_project();
    let shared = base.join("shared");
    library_project(
        &shared,
        "org.example",
        "shared",
        "1.0.0",
        "pub fn value(): i32 { return 40; }",
    );
    for name in ["left", "right"] {
        let dir = base.join(name);
        write(
            &dir.join("dolphin.toml"),
            &format!(
                r#"
                [package]
                group = "org.example"
                name = "{name}"
                version = "1.0.0"

                [lib]
                path = "src/lib.do"

                [dependencies]
                shared = {{ path = "../shared" }}
                "#
            ),
        );
        write(
            &dir.join("src/lib.do"),
            &format!("use shared;\npub fn {name}(): i32 {{ return shared.value(); }}"),
        );
    }
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "app"
        version = "0.1.0"

        [[bin]]
        name = "app"
        path = "src/main.do"

        [dependencies]
        left = { path = "../left" }
        right = { path = "../right" }
        "#,
    );
    write(
        &app.join("src/main.do"),
        "use left;\nuse right;\nfn main() { return left.left() + right.right(); }",
    );
    let manifest = load_manifest(&app).expect("manifest should load");
    let artifacts = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::default(),
    )
    .expect("diamond dependency should build");
    let output = Command::new(&artifacts[0].executable)
        .output()
        .expect("app should run");
    assert_eq!(output.status.code(), Some(80));
    fs::remove_dir_all(base).expect("temporary project should be removed");
}

/// H18-04 GEN-03：类型声明 bound 与关联类型跨 path 依赖包检查。
#[test]
fn h18_04_type_bounds_across_path_dependency() {
    use dolphin_compiler::BuildSettings;
    use support::backends;

    let lib = r#"
    pub trait Has { type Item; }
    pub struct Good { pub value: i32 }
    pub struct Bad { pub value: i32 }
    impl Has for Good { type Item = i32; }
    pub struct Holder<T: Has> { pub value: T }
    pub struct ItemBox<T: Has> { pub value: T::Item }
    "#;

    let app = path_dependency_pair(
        lib,
        r#"
        use math;

        fn main() {
            val hold = math.Holder<math.Good>(math.Good(41));
            val good = hold.value;
            val box = math.ItemBox<math.Good>(1);
            return good.value + box.value;
        }
        "#,
    );
    let manifest = load_manifest(&app).expect("manifest should load");
    for backend in backends() {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let artifacts = build_manifest(
                &manifest,
                None,
                profile,
                BuildSettings::with_backend(backend),
            )
            .unwrap_or_else(|error| {
                panic!("cross-package bounds should build ({backend:?}/{profile:?}): {error}")
            });
            let output = Command::new(&artifacts[0].executable)
                .output()
                .expect("app should run");
            assert_eq!(
                output.status.code(),
                Some(42),
                "backend={:?} profile={profile:?}",
                backend
            );
        }
    }
    fs::remove_dir_all(app.parent().unwrap()).expect("temporary project should be removed");

    let rejected = path_dependency_pair(
        lib,
        r#"
        use math;

        fn main() {
            val hold = math.Holder<math.Bad>(math.Bad(1));
            return 0;
        }
        "#,
    );
    let manifest = load_manifest(&rejected).expect("manifest should load");
    let error = build_manifest(
        &manifest,
        None,
        BuildProfile::Debug,
        BuildSettings::with_backend(dolphin_compiler::BackendChoice::Cranelift),
    )
    .expect_err("missing impl across packages must be rejected");
    for needle in ["Bad", "Has", "does not implement"] {
        assert!(
            error.to_string().contains(needle),
            "expected `{needle}` in diagnostic: {error}"
        );
    }
    fs::remove_dir_all(rejected.parent().unwrap()).expect("temporary project should be removed");
}

/// H18-05 IMPL-04：跨包参数化 impl（固有 + trait）与关联类型保持正确。
#[test]
fn h18_05_parameterized_impl_across_path_dependency() {
    use dolphin_compiler::BuildSettings;
    use support::backends;

    let app = path_dependency_pair(
        r#"
        pub trait Wrap { type Output; fn unwrap(self): Self::Output; }
        pub struct Holder<T> { pub value: T }

        impl<U> Holder<U> {
            pub fn get(self): U { return self.value; }
        }

        impl<U> Wrap for Holder<U> {
            type Output = U;
            fn unwrap(self): U { return self.value; }
        }
        "#,
        r#"
        use math;
        use math.Holder;
        use math.Wrap;

        fn main() {
            val h = math.Holder<i32>(40);
            val t = math.Holder<i32>(1);
            return h.unwrap() + t.get();
        }
        "#,
    );
    let manifest = load_manifest(&app).expect("manifest should load");
    for backend in backends() {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let artifacts = build_manifest(
                &manifest,
                None,
                profile,
                BuildSettings::with_backend(backend),
            )
            .unwrap_or_else(|error| {
                panic!("cross-package impl should build ({backend:?}/{profile:?}): {error}")
            });
            let output = Command::new(&artifacts[0].executable)
                .output()
                .expect("app should run");
            assert_eq!(
                output.status.code(),
                Some(41),
                "backend={:?} profile={profile:?}",
                backend
            );
        }
    }
    fs::remove_dir_all(app.parent().unwrap()).expect("temporary project should be removed");
}

/// H18-08 BUILD-01：同时声明 `[lib]` 与两个 `[[bin]]` 的 path 依赖只贡献库源码；
/// 依赖的 bin `main` 不得进入应用，库与 bin 共享的 helper 仍可用。
#[test]
fn h18_08_build_01_path_dependency_with_bins_loads_only_library() {
    use dolphin_compiler::BuildSettings;
    use support::backends;

    let base = temp_project();
    let dep = base.join("dep");
    write(
        &dep.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "dep"
        version = "1.0.0"

        [lib]
        path = "src/lib.do"

        [[bin]]
        name = "tool"
        path = "src/tool.do"

        [[bin]]
        name = "tool2"
        path = "src/tool2.do"
        "#,
    );
    write(
        &dep.join("src/lib.do"),
        "pub fn value(): i32 { return helper() + 40; }",
    );
    write(
        &dep.join("src/shared.do"),
        "pub fn helper(): i32 { return 2; }",
    );
    write(&dep.join("src/tool.do"), "fn main() { return 1; }");
    write(&dep.join("src/tool2.do"), "fn main() { return 2; }");

    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
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
    );
    write(
        &app.join("src/main.do"),
        "use dep;\nfn main() { return dep.value(); }",
    );

    let manifest = load_manifest(&app).expect("manifest should load");
    for backend in backends() {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let artifacts = build_manifest(
                &manifest,
                None,
                profile,
                BuildSettings::with_backend(backend),
            )
            .unwrap_or_else(|error| {
                panic!("path dependency with bins must build ({backend:?}/{profile:?}): {error}")
            });
            let output = Command::new(&artifacts[0].executable)
                .output()
                .expect("app should run");
            assert_eq!(
                output.status.code(),
                Some(42),
                "backend={backend:?} profile={profile:?} stderr={}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                output.stderr.is_empty(),
                "backend={backend:?} profile={profile:?}"
            );
        }
    }

    fs::remove_dir_all(base).expect("temporary project should be removed");
}

/// H18-08 BUILD-02/03：driver 层显式 profile 优先于根清单 `build.optimization`，
/// 并且以运行时行为（Debug 泄漏报告）而不是枚举值验收。
#[test]
fn h18_08_build_03_explicit_profile_overrides_manifest_optimization() {
    use dolphin_compiler::BuildSettings;
    use support::backends;

    let leak_program = "use std.mem;\n\nfn main() {\n    val bytes = mem.alloc<u8>(8);\n    bytes[0] = 1_u8;\n    return 42;\n}\n";
    let project = |optimization: &str| {
        let root = temp_project();
        write(
            &root.join("dolphin.toml"),
            &format!(
                r#"
                [package]
                group = "org.example"
                name = "leaky"
                version = "0.1.0"

                [[bin]]
                name = "leaky"
                path = "src/main.do"

                [build]
                optimization = "{optimization}"
                "#
            ),
        );
        write(&root.join("src/main.do"), leak_program);
        root
    };

    // 清单 Release + 显式 Debug → Debug runtime，必须报告泄漏。
    let release = project("release");
    let manifest = load_manifest(&release).expect("manifest should load");
    for backend in backends() {
        let artifacts = build_manifest(
            &manifest,
            None,
            BuildProfile::Debug,
            BuildSettings::with_backend(backend),
        )
        .expect("explicit Debug build should succeed");
        let output = Command::new(&artifacts[0].executable)
            .output()
            .expect("program should run");
        assert_eq!(output.status.code(), Some(42), "backend={backend:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("leaked"),
            "explicit Debug must win over a release manifest ({backend:?})"
        );
    }
    fs::remove_dir_all(release).expect("temporary project should be removed");

    // 清单 Debug + 显式 Release → Release runtime，不得报告泄漏。
    let debug = project("debug");
    let manifest = load_manifest(&debug).expect("manifest should load");
    for backend in backends() {
        let artifacts = build_manifest(
            &manifest,
            None,
            BuildProfile::Release,
            BuildSettings::with_backend(backend),
        )
        .expect("explicit Release build should succeed");
        let output = Command::new(&artifacts[0].executable)
            .output()
            .expect("program should run");
        assert_eq!(output.status.code(), Some(42), "backend={backend:?}");
        assert!(
            output.stderr.is_empty(),
            "explicit Release must not report leaks ({backend:?}): {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_dir_all(debug).expect("temporary project should be removed");
}

/// H18-09 冒烟发现的既有缺陷回归：trait/impl 方法的 `source_id` 曾被漏设，停留
/// 在解析器默认值 0，导致方法内诊断与 Location 被误归属到第一个源文件
/// （`main.do`），在多字节字符处还会 panic。这里断言方法内错误指向定义它的文件。
#[test]
fn h18_09_method_diagnostics_use_defining_file() {
    let project = temp_project();
    write(
        &project.join("dolphin.toml"),
        r#"
        [package]
        group = "org.example"
        name = "sourceid"
        version = "0.1.0"

        [[bin]]
        name = "sourceid"
        path = "src/main.do"
        "#,
    );
    write(
        &project.join("src/main.do"),
        "fn main() { val w = Widget(1); return w.broken(); }",
    );
    write(
        &project.join("src/util.do"),
        "struct Widget { value: i32 }\n\nimpl Widget {\n    fn broken(self): i32 { return missing_name; }\n}\n",
    );

    let manifest = load_manifest(&project).expect("manifest should load");
    let error = check_manifest(&manifest).expect_err("unknown variable must be rejected");
    let text = error.to_string();
    assert!(
        text.contains("missing_name"),
        "diagnostic must name the unknown variable: {text}"
    );
    assert!(
        text.contains("util.do"),
        "method diagnostic must point at the defining file `util.do`: {text}"
    );
    assert!(
        !text.contains("main.do"),
        "method diagnostic must not be attributed to the first source file: {text}"
    );

    fs::remove_dir_all(project).expect("temporary project should be removed");
}
