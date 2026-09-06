use std::fs;
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
fn env_shows_host_target_and_linker() {
    let output = dc().arg("env").output().expect("dc env should run");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("host:"));
    assert!(stdout.contains("target:"));
    assert!(stdout.contains("abi:"));
    assert!(stdout.contains("linker: rust-lld"));
    assert!(stdout.contains("system linker:"));
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
        path = "src/main.dc"
        "#,
    )
    .unwrap();
    fs::write(project.join("src/main.dc"), "fn main() { return 0; }").unwrap();

    let output = dc()
        .arg("info")
        .arg(&project)
        .output()
        .expect("dc info should run");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("me.foxlab:hello:0.1.0"));
    assert!(stdout.contains("hello -> "));
    assert!(stdout.contains("main.dc"));

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
        path = "src/main.dc"
        "#,
    )
    .unwrap();
    fs::write(project.join("src/main.dc"), "fn main() { return 42; }").unwrap();

    let run = dc()
        .arg("run")
        .arg(&project)
        .output()
        .expect("dc run should run");
    assert_eq!(run.status.code(), Some(42));

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
        path = "src/a.dc"

        [[bin]]
        name = "b"
        path = "src/b.dc"
        "#,
    )
    .unwrap();
    fs::write(project.join("src/a.dc"), "fn main() { return 1; }").unwrap();
    fs::write(project.join("src/b.dc"), "fn main() { return 2; }").unwrap();

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
