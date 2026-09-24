//! H20-02 共享项目分析接口与 overlay 的集成验收（ANALYSIS-01..06）。
//!
//! 每个验收点断言固定期望：诊断 code/message、单元选择、文件字节不变、
//! 零网络、零写锁、零构建产物；不与其他后端比较一致性代替正确性。

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_analysis::{AnalysisHost, AnalysisMode, DefId, Resolution, SymbolId, UnitKind};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dolphin-m20-analysis-{tag}-{}-{unique}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent");
    }
    fs::write(path, text).expect("write");
}

fn analysis_host(root: &Path, home: &Path) -> AnalysisHost {
    AnalysisHost::with_cache_root(root.to_path_buf(), home.to_path_buf())
}

/// 递归读取目录下所有文件的字节，用于“项目文件不变”断言。
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect_tree(root, root, &mut files);
    files
}

fn collect_tree(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_tree(root, &path, files);
        } else if let Ok(bytes) = fs::read(&path) {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            files.insert(relative, bytes);
        }
    }
}

fn dc() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dc"));
    command
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost");
    command
}

fn run_dc(home: &Path, args: &[&str]) -> Output {
    dc().env("DOLPHIN_HOME", home)
        .args(args)
        .output()
        .expect("dc should run")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// ANALYSIS-01：分析零网络、零写锁、零构建产物；依赖缺失给出 `E1001`。
#[test]
fn analysis_01_no_network_no_lock_write_no_artifacts() {
    let base = temp_dir("a01");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();

    // 场景一：HTTP 远程依赖无锁、无缓存：必须直接返回 E1001，且不发起连接。
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.set_nonblocking(true).unwrap();
    let repo_url = format!("http://{}", listener.local_addr().unwrap());
    let project = base.join("remote");
    write(
        &project.join("dolphin.toml"),
        &format!(
            "[package]\ngroup = \"g\"\nname = \"remote\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"remote\"\npath = \"src/main.do\"\n\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\nmath = \"org.example:mathlib:1.0.0\"\n"
        ),
    );
    write(
        &project.join("src/main.do"),
        "use math;\nfn main() { return math.answer(); }\n",
    );
    let before = tree(&project);
    let mut host = analysis_host(&project, &home);
    let snapshot = host.snapshot();
    assert_eq!(snapshot.mode, AnalysisMode::Project);
    assert!(snapshot.partial, "依赖缺失必须 partial=true");
    assert!(snapshot.units.is_empty(), "{:?}", snapshot.units.len());
    assert_eq!(snapshot.project_diagnostics.len(), 1);
    let diagnostic = &snapshot.project_diagnostics[0];
    assert_eq!(diagnostic.code(), "E1001");
    assert!(
        diagnostic.message().contains("org.example:mathlib:1.0.0")
            && diagnostic.message().contains("run `dc fetch` and retry"),
        "E1001 必须提示依赖坐标与 dc fetch：{}",
        diagnostic.message()
    );
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == ErrorKind::WouldBlock),
        "分析路径不得访问 HTTP(S)"
    );
    assert_eq!(tree(&project), before, "分析不得改写项目文件或写锁");
    assert!(!project.join("target").exists(), "分析不得产出构建产物");
    assert!(!project.join("dolphin.lock").exists());

    // 场景二：路径依赖 + 已有锁：只读解析成功，锁与项目文件逐字节不变。
    let dep = base.join("dep");
    write(
        &dep.join("dolphin.toml"),
        "[package]\ngroup = \"org.example\"\nname = \"dep\"\nversion = \"1.0.0\"\n\n[lib]\npath = \"src/lib.do\"\n",
    );
    write(
        &dep.join("src/lib.do"),
        "pub fn value(): i32 { return 42; }\n",
    );
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        "[package]\ngroup = \"g\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"app\"\npath = \"src/main.do\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n",
    );
    write(
        &app.join("src/main.do"),
        "use dep.value;\nfn main() { return value(); }\n",
    );
    let fetch = run_dc(&home, &["fetch", app.to_str().unwrap()]);
    assert!(fetch.status.success(), "{}", stderr_text(&fetch));
    assert!(app.join("dolphin.lock").is_file());
    let before = tree(&app);
    let mut host = analysis_host(&app, &home);
    let snapshot = host.snapshot();
    assert!(!snapshot.partial, "{:?}", snapshot.project_diagnostics);
    assert!(
        snapshot.diagnostics.is_empty(),
        "{:?}",
        snapshot.diagnostics
    );
    assert_eq!(snapshot.units.len(), 1);
    assert!(snapshot.graph.is_some());
    assert_eq!(tree(&app), before, "分析必须逐字节保留项目文件与锁");
    assert!(!app.join("target").exists());

    fs::remove_dir_all(base).unwrap();
}

/// ANALYSIS-02：未保存依赖文件改变调用方诊断；关闭后恢复磁盘版本。
#[test]
fn analysis_02_unsaved_dependency_affects_caller() {
    let base = temp_dir("a02");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let stats = base.join("stats");
    write(
        &stats.join("dolphin.toml"),
        "[package]\ngroup = \"org.example\"\nname = \"stats\"\nversion = \"1.0.0\"\n\n[lib]\npath = \"src/lib.do\"\n",
    );
    let stats_lib = stats.join("src/lib.do");
    write(&stats_lib, "pub fn count(): i32 { return 1; }\n");
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        "[package]\ngroup = \"g\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"app\"\npath = \"src/main.do\"\n\n[dependencies]\nstats = { path = \"../stats\" }\n",
    );
    let app_main = app.join("src/main.do");
    write(
        &app_main,
        "use stats.count;\nfn main() { return count(); }\n",
    );

    let mut host = analysis_host(&app, &home);
    let baseline = host.snapshot();
    assert!(!baseline.partial, "{:?}", baseline.project_diagnostics);
    assert!(
        baseline.diagnostics.is_empty(),
        "{:?}",
        baseline.diagnostics
    );

    // 未保存文本把依赖函数返回类型改为 bool：调用方 main 出现固定类型诊断。
    assert!(host.set_overlay(
        stats_lib.clone(),
        1,
        "pub fn count(): bool { return true; }\n".to_string()
    ));
    assert!(!host.set_overlay(
        stats_lib.clone(),
        1,
        "pub fn count(): bool { return false; }\n".to_string()
    ));
    assert_eq!(host.overlay_version(&stats_lib), Some(1));
    let changed = host.snapshot();
    assert!(changed.partial, "overlay 后必须 partial=true");
    assert_eq!(changed.diagnostics.len(), 1, "{:?}", changed.diagnostics);
    let diagnostic = &changed.diagnostics[0];
    assert_eq!(diagnostic.code(), "E0001");
    assert_eq!(
        diagnostic.message(),
        "expected `i32`, found `bool`",
        "调用方诊断必须来自 overlay 后的依赖签名"
    );
    let unit = changed.unit_for_path(&app_main).expect("app bin unit");
    let label = diagnostic.labels().first().expect("primary label");
    let primary_path = unit
        .sources
        .iter()
        .find(|source| source.id == label.source)
        .map(|source| source.path.clone())
        .expect("diagnostic source in unit");
    assert_eq!(primary_path, app_main);

    // didClose：恢复磁盘版本，诊断清空。
    assert!(host.remove_overlay(&stats_lib));
    let restored = host.snapshot();
    assert!(!restored.partial, "{:?}", restored.project_diagnostics);
    assert!(
        restored.diagnostics.is_empty(),
        "{:?}",
        restored.diagnostics
    );
    assert_eq!(host.revision(), 2);

    fs::remove_dir_all(base).unwrap();
}

/// ANALYSIS-03：lib-only 项目不误报“缺 main”；bin 缺 main 有明确诊断。
#[test]
fn analysis_03_library_without_main() {
    let base = temp_dir("a03");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();

    let library = base.join("library");
    write(
        &library.join("dolphin.toml"),
        "[package]\ngroup = \"g\"\nname = \"library\"\nversion = \"0.1.0\"\n\n[lib]\npath = \"src/lib.do\"\n",
    );
    write(
        &library.join("src/lib.do"),
        "pub fn value(): i32 { return 7; }\n",
    );
    let mut host = analysis_host(&library, &home);
    let snapshot = host.snapshot();
    assert_eq!(snapshot.mode, AnalysisMode::Project);
    assert_eq!(snapshot.units.len(), 1);
    assert_eq!(snapshot.units[0].kind, UnitKind::Lib);
    assert!(!snapshot.partial, "{:?}", snapshot.project_diagnostics);
    assert!(
        snapshot.diagnostics.is_empty(),
        "{:?}",
        snapshot.diagnostics
    );
    assert!(snapshot.project_diagnostics.is_empty());
    assert!(snapshot.units[0].index.is_some(), "lib 必须完成语义分析");

    // 反例：bin 目标缺少 main 必须报告，而不是静默通过。
    let binary = base.join("binary");
    write(
        &binary.join("dolphin.toml"),
        "[package]\ngroup = \"g\"\nname = \"binary\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"binary\"\npath = \"src/main.do\"\n",
    );
    write(
        &binary.join("src/main.do"),
        "pub fn other(): i32 { return 1; }\n",
    );
    let mut host = analysis_host(&binary, &home);
    let snapshot = host.snapshot();
    assert!(snapshot.partial);
    assert_eq!(snapshot.units.len(), 1);
    assert_eq!(snapshot.units[0].kind, UnitKind::Bin("binary".into()));
    assert!(
        snapshot
            .project_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message() == "program does not define `main`"),
        "{:?}",
        snapshot.project_diagnostics
    );

    fs::remove_dir_all(base).unwrap();
}

/// ANALYSIS-04：自定义 source + 多 bin 的单元选择与排除规则。
#[test]
fn analysis_04_multi_bin_and_custom_source() {
    let base = temp_dir("a04");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let project = base.join("project");
    write(
        &project.join("dolphin.toml"),
        "[package]\ngroup = \"g\"\nname = \"multi\"\nversion = \"0.1.0\"\nsource = \"code\"\n\n[lib]\npath = \"code/lib.do\"\n\n[[bin]]\nname = \"first\"\npath = \"code/first.do\"\n\n[[bin]]\nname = \"second\"\npath = \"code/second.do\"\n",
    );
    write(
        &project.join("code/lib.do"),
        "pub fn helper(): i32 { return 1; }\n",
    );
    write(
        &project.join("code/shared.do"),
        "pub fn shared(): i32 { return 2; }\n",
    );
    write(
        &project.join("code/first.do"),
        "fn main() { return shared(); }\n",
    );
    write(
        &project.join("code/second.do"),
        "fn main() { return shared(); }\n",
    );
    let mut host = analysis_host(&project, &home);
    let snapshot = host.snapshot();
    assert!(!snapshot.partial, "{:?}", snapshot.project_diagnostics);
    let kinds: Vec<&UnitKind> = snapshot.units.iter().map(|unit| &unit.kind).collect();
    assert_eq!(
        kinds,
        vec![
            &UnitKind::Lib,
            &UnitKind::Bin("first".into()),
            &UnitKind::Bin("second".into())
        ],
        "单元顺序必须是 Lib 在前、Bin 按清单顺序"
    );

    let shared = project.join("code/shared.do");
    let first = project.join("code/first.do");
    let second = project.join("code/second.do");
    // 共享文件按 Lib > Bin 选择第一个包含它的单元。
    assert_eq!(
        snapshot.unit_for_path(&shared).map(|unit| &unit.kind),
        Some(&UnitKind::Lib)
    );
    assert_eq!(
        snapshot.unit_for_path(&first).map(|unit| &unit.kind),
        Some(&UnitKind::Bin("first".into()))
    );
    // lib 排除全部 bin 入口；每个 bin 排除其他 bin 入口。
    let lib_unit = &snapshot.units[0];
    assert!(lib_unit.source_id_for_path(&shared).is_some());
    assert!(lib_unit.source_id_for_path(&first).is_none());
    assert!(lib_unit.source_id_for_path(&second).is_none());
    let first_unit = &snapshot.units[1];
    assert!(first_unit.source_id_for_path(&first).is_some());
    assert!(first_unit.source_id_for_path(&second).is_none());
    assert!(first_unit.source_id_for_path(&shared).is_some());
    let second_unit = &snapshot.units[2];
    assert!(second_unit.source_id_for_path(&second).is_some());
    assert!(second_unit.source_id_for_path(&first).is_none());
    for unit in &snapshot.units {
        assert!(unit.index.is_some(), "{:?}", unit.kind);
    }

    fs::remove_dir_all(base).unwrap();
}

/// ANALYSIS-05：同名模块/类型来自不同包时身份不混淆。
#[test]
fn analysis_05_same_name_different_package_not_confused() {
    let base = temp_dir("a05");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    for (name, value) in [("left", 1), ("right", 2)] {
        let package = base.join(name);
        write(
            &package.join("dolphin.toml"),
            &format!(
                "[package]\ngroup = \"org.example\"\nname = \"{name}\"\nversion = \"1.0.0\"\n\n[lib]\npath = \"src/lib.do\"\n"
            ),
        );
        write(
            &package.join("src/lib.do"),
            &format!("pub fn value(): i32 {{ return {value}; }}\n"),
        );
        write(
            &package.join("src/mod/inner.do"),
            &format!(
                "pkg mod;\npub struct Point {{ pub x: i32 }}\npub fn value(): i32 {{ return {value}0; }}\n"
            ),
        );
    }
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        "[package]\ngroup = \"g\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"app\"\npath = \"src/main.do\"\n\n[dependencies]\nleft = { path = \"../left\" }\nright = { path = \"../right\" }\n",
    );
    let text = "\
use left;
use right;
fn main() {
    val a = left.mod.inner.value();
    val b = right.mod.inner.value();
    val p = left.mod.inner.Point(1);
    val q = right.mod.inner.Point(2);
    return a + b + p.x + q.x;
}
";
    write(&app.join("src/main.do"), text);
    let mut host = analysis_host(&app, &home);
    let snapshot = host.snapshot();
    assert!(!snapshot.partial, "{:?}", snapshot.project_diagnostics);
    let app_main = app.join("src/main.do");
    let unit = snapshot.unit_for_path(&app_main).expect("bin unit");
    let index = unit.index.as_ref().expect("symbol index");
    let source = unit.source_id_for_path(&app_main).expect("app source id");

    let left_call = text.find("left.mod.inner.value").unwrap() + "left.mod.inner.".len();
    let right_call = text.find("right.mod.inner.value").unwrap() + "right.mod.inner.".len();
    let left_resolution = index.resolve(source, left_call);
    let right_resolution = index.resolve(source, right_call);
    let Resolution::Def(left_def) = &left_resolution else {
        panic!("left.mod.value must resolve to a definition: {left_resolution:?}");
    };
    let Resolution::Def(right_def) = &right_resolution else {
        panic!("right.mod.value must resolve to a definition: {right_resolution:?}");
    };
    assert_ne!(left_def, right_def, "同名不同包必须是不同 DefId");
    assert_ne!(left_def.package, right_def.package);
    let left_definition = index
        .definition_of(&SymbolId::Def(left_def.clone()))
        .unwrap();
    let right_definition = index
        .definition_of(&SymbolId::Def(right_def.clone()))
        .unwrap();
    assert_eq!(
        left_definition.signature,
        "fn left.mod.inner.value() -> i32"
    );
    assert_eq!(
        right_definition.signature,
        "fn right.mod.inner.value() -> i32"
    );
    assert_ne!(left_definition.source, right_definition.source);

    // 同名类型构造同样不混淆，且类型显示名带包名。
    let left_point = text.find("left.mod.inner.Point").unwrap() + "left.mod.inner.".len();
    let right_point = text.find("right.mod.inner.Point").unwrap() + "right.mod.inner.".len();
    let Resolution::Def(left_point_def) = index.resolve(source, left_point) else {
        panic!("left.mod.Point must resolve to a definition");
    };
    let Resolution::Def(right_point_def) = index.resolve(source, right_point) else {
        panic!("right.mod.Point must resolve to a definition");
    };
    assert_ne!(left_point_def, right_point_def);
    let render = |def: &DefId| {
        let (id, _) = index
            .type_names
            .iter()
            .find(|(_, symbol)| match symbol {
                SymbolId::Def(symbol_def) => symbol_def == def,
                _ => false,
            })
            .unwrap_or_else(|| panic!("type instance for {def:?}"));
        index.render_type(&dolphin_ir::ir::Type::Struct(*id))
    };
    assert_eq!(render(&left_point_def), "left.mod.inner.Point");
    assert_eq!(render(&right_point_def), "right.mod.inner.Point");

    fs::remove_dir_all(base).unwrap();
}

/// ANALYSIS-06：同项目 CLI 构建/运行行为保持 M19 基线（分析不改变构建路径）。
#[test]
fn analysis_06_cli_build_behavior_unchanged() {
    let base = temp_dir("a06");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let dep = base.join("dep");
    write(
        &dep.join("dolphin.toml"),
        "[package]\ngroup = \"org.example\"\nname = \"dep\"\nversion = \"1.0.0\"\n\n[lib]\npath = \"src/lib.do\"\n",
    );
    write(
        &dep.join("src/lib.do"),
        "pub fn value(): i32 { return 42; }\n",
    );
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        "[package]\ngroup = \"g\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"app\"\npath = \"src/main.do\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n",
    );
    write(
        &app.join("src/main.do"),
        "use dep.value;\nfn main() { return value(); }\n",
    );

    // 分析只读：不写锁、不产生产物，也不影响随后 CLI 构建。
    let before = tree(&app);
    let mut host = analysis_host(&app, &home);
    let snapshot = host.snapshot();
    assert!(!snapshot.partial, "{:?}", snapshot.project_diagnostics);
    assert_eq!(tree(&app), before);
    assert!(!app.join("target").exists());

    let check = run_dc(&home, &["check", app.to_str().unwrap()]);
    assert!(check.status.success(), "{}", stderr_text(&check));
    assert_eq!(stdout_text(&check), "Checked g:app:0.1.0\n");

    let build = run_dc(&home, &["build", app.to_str().unwrap()]);
    assert!(build.status.success(), "{}", stderr_text(&build));
    let built = stdout_text(&build);
    assert!(built.starts_with("Built "), "{built}");
    let executable = if cfg!(windows) {
        app.join("target/app.exe")
    } else {
        app.join("target/app")
    };
    assert!(executable.is_file(), "Debug 构建产物必须存在");

    // `dc run` 透传应用退出码 42；stderr 必须为空。
    let run = run_dc(&home, &["run", app.to_str().unwrap()]);
    assert_eq!(run.status.code(), Some(42), "{}", stderr_text(&run));
    assert_eq!(stderr_text(&run), "");
    assert_eq!(stdout_text(&run), "");

    // 显式 Dolphin Release：同一项目构建与运行行为一致。
    let release_build = run_dc(&home, &["build", "--release", app.to_str().unwrap()]);
    assert!(
        release_build.status.success(),
        "{}",
        stderr_text(&release_build)
    );
    assert!(executable.is_file());
    let release_run = run_dc(&home, &["run", "--release", app.to_str().unwrap()]);
    assert_eq!(
        release_run.status.code(),
        Some(42),
        "{}",
        stderr_text(&release_run)
    );

    fs::remove_dir_all(base).unwrap();
}
