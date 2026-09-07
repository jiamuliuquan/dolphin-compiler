use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_compiler::{
    BuildProfile, BuildSettings, build_manifest, check_manifest, load_manifest,
};

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
