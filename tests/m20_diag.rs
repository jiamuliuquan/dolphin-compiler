//! H20-01 结构化诊断与有限恢复的集成验收（DIAG-01..06）。
//!
//! CLI 走真实 `dc check`/`dc build` 子进程并断言三路结果；LSP 断言
//! `dolphin_lsp::Server` 产出的结构化 `publishDiagnostics` 字段
//! （真实 stdio JSON-RPC 会话属 H20-03 的 `tests/m20_lsp.rs`）。
//!
//! 每个验收点的固定期望写在断言里：`code`/`message`/UTF-16 range/退出码/产物。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_lsp::Server;
use serde_json::{Value, json};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dolphin-m20-diag-{tag}-{}-{unique}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn dc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dc"))
}

/// 在隔离的 `DOLPHIN_HOME` 下运行 dc，避免污染开发者全局缓存。
fn run_dc(home: &Path, args: &[&str]) -> Output {
    dc().env("DOLPHIN_HOME", home)
        .args(args)
        .output()
        .expect("dc should run")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// 打开一个文档并返回 `textDocument/publishDiagnostics` 的 diagnostics 数组。
///
/// H20-03 起通知在 `initialize` 之前被忽略，因此这里先完成握手（真实 stdio 会话
/// 的协议验收见 `tests/m20_lsp.rs`）。
fn lsp_diagnostics(uri: &str, text: &str) -> Vec<Value> {
    let mut server = Server::new();
    server.handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
    server.handle(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    let outputs = server.handle(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "dolphin",
                "version": 1,
                "text": text,
            }
        }
    }));
    outputs
        .iter()
        .find(|message| message["method"] == "textDocument/publishDiagnostics")
        .and_then(|message| message["params"]["diagnostics"].as_array().cloned())
        .expect("publishDiagnostics notification")
}

fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.starts_with('/') {
        format!("file://{text}")
    } else {
        format!("file:///{text}")
    }
}

/// DIAG-01：CLI `E0001` 文本位置与 LSP `code`/UTF-16 range 完全一致。
#[test]
fn diag_01_cli_and_lsp_same_error_identity() {
    let base = temp_dir("diag01");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let file = base.join("main.do");
    let text = "fn main() { return missing; }\n";
    fs::write(&file, text).unwrap();

    let output = run_dc(&home, &["check", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr_text(&output));
    let stderr = stderr_text(&output);
    assert!(
        stderr.starts_with("error[E0001]: unknown variable `missing`\n"),
        "CLI 首行必须是 E0001 纯消息：{stderr}"
    );
    assert!(
        stderr.contains(&format!(" --> {}:1:20\n", file.display())),
        "CLI 位置必须是 1:20：{stderr}"
    );

    let diagnostics = lsp_diagnostics(&file_uri(&file), text);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic["code"], "E0001");
    assert_eq!(diagnostic["message"], "unknown variable `missing`");
    assert_eq!(diagnostic["severity"], 1);
    assert_eq!(diagnostic["source"], "dolphin");
    // CLI 1-based 字符列 20 <-> LSP 0-based UTF-16 列 19；行同为第 1 行。
    assert_eq!(diagnostic["range"]["start"]["line"], 0);
    assert_eq!(diagnostic["range"]["start"]["character"], 19);
    assert_eq!(diagnostic["range"]["end"]["character"], 26);

    fs::remove_dir_all(base).unwrap();
}

/// DIAG-02：两文件各一独立错误（语法 + 声明级语义）同时出现在 CLI 与 LSP。
///
/// 两个 `[[bin]]` 目标各自排除另一个入口，保证 a.do 只做语法检查、b.do 能进入
/// 声明级 lowering，从而两个错误互不依赖地同时收集。
#[test]
fn diag_02_two_files_independent_errors() {
    let base = temp_dir("diag02");
    let home = base.join("home");
    let project = base.join("project");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(
        project.join("dolphin.toml"),
        r#"
        [package]
        group = "g"
        name = "diag02"
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
    let a_text = "fn main() { return 0;\n";
    let b_text = "impl Missing { }\nfn main() { return 0; }\n";
    let a = project.join("src/a.do");
    let b = project.join("src/b.do");
    fs::write(&a, a_text).unwrap();
    fs::write(&b, b_text).unwrap();

    let output = run_dc(&home, &["check", project.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr_text(&output));
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains("error[E0001]: expected `}` after block")
            && stderr.contains(&a.display().to_string()),
        "CLI 缺少 a.do 的语法错误：{stderr}"
    );
    assert!(
        stderr.contains("error[E0001]: unknown type `Missing`")
            && stderr.contains(&b.display().to_string()),
        "CLI 缺少 b.do 的声明级错误：{stderr}"
    );

    let a_diagnostics = lsp_diagnostics(&file_uri(&a), a_text);
    assert_eq!(a_diagnostics.len(), 1, "{a_diagnostics:?}");
    assert_eq!(a_diagnostics[0]["code"], "E0001");
    assert_eq!(a_diagnostics[0]["message"], "expected `}` after block");

    let b_diagnostics = lsp_diagnostics(&file_uri(&b), b_text);
    assert_eq!(b_diagnostics.len(), 1, "{b_diagnostics:?}");
    assert_eq!(b_diagnostics[0]["code"], "E0001");
    assert_eq!(b_diagnostics[0]["message"], "unknown type `Missing`");

    fs::remove_dir_all(base).unwrap();
}

/// DIAG-03：非 BMP 字符前后位置、CRLF 行列正确；不 panic。
#[test]
fn diag_03_non_bmp_and_crlf_positions() {
    let base = temp_dir("diag03");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let file = base.join("main.do");
    // 第 2 行错误在非 BMP 字符之前，第 3 行在含非 BMP 的行之后；行尾为 CRLF。
    let text = "fn main() {\r\n    val a: = \"𝄞\";\r\n    val b: = 1;\r\n}\r\n";
    fs::write(&file, text).unwrap();

    let output = run_dc(&home, &["check", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr_text(&output));
    let stderr = stderr_text(&output);
    assert!(!stderr.contains("panicked"), "诊断不得 panic：{stderr}");
    assert!(
        stderr.contains(&format!(" --> {}:2:12\n", file.display())),
        "第 2 行（非 BMP 前）位置必须是 2:12：{stderr}"
    );
    assert!(
        stderr.contains(&format!(" --> {}:3:12\n", file.display())),
        "第 3 行（非 BMP 后、CRLF）位置必须是 3:12：{stderr}"
    );

    let diagnostics = lsp_diagnostics(&file_uri(&file), text);
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["message"], "expected type name after `:`");
    assert_eq!(diagnostics[0]["range"]["start"]["line"], 1);
    assert_eq!(diagnostics[0]["range"]["start"]["character"], 11);
    assert_eq!(diagnostics[1]["range"]["start"]["line"], 2);
    assert_eq!(diagnostics[1]["range"]["start"]["character"], 11);

    fs::remove_dir_all(base).unwrap();
}

/// DIAG-04：错误消息含可读泛型/用户类型名，无 `TypeId(n)`/`struct@`/`enum@`。
#[test]
fn diag_04_generic_chain_and_user_type_names() {
    let base = temp_dir("diag04");
    let home = base.join("home");
    let project = base.join("project");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(
        project.join("dolphin.toml"),
        r#"
        [package]
        group = "g"
        name = "diag04"
        version = "0.1.0"

        [[bin]]
        name = "diag04"
        path = "src/main.do"
        "#,
    )
    .unwrap();
    fs::create_dir_all(project.join("src/myerr")).unwrap();
    fs::write(
        project.join("src/myerr/error.do"),
        "pkg myerr;\npub struct MyError { code: i32 }\n",
    )
    .unwrap();
    let main_text = "fn main() {\n    val x: std.collections.Vec<Result<i32, myerr.error.MyError>> = 1;\n    return;\n}\n";
    fs::write(project.join("src/main.do"), main_text).unwrap();

    // CLI：项目模式，模块限定的用户类型必须显示为 `myerr.error.MyError`。
    let output = run_dc(&home, &["check", project.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr_text(&output));
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains(
            "expected `std.collections.Vec<std.Result<i32, myerr.error.MyError>>`, found `i32`"
        ),
        "错误消息必须含可读泛型链与限定用户类型：{stderr}"
    );
    assert!(
        !stderr.contains("TypeId(") && !stderr.contains("struct@") && !stderr.contains("enum@"),
        "错误消息不得输出内部类型编号：{stderr}"
    );

    // LSP：单文件模式同样输出可读显示名（根级用户类型）。
    let single = base.join("single.do");
    let single_text = "struct MyError { code: i32 }\nfn main() {\n    val x: std.collections.Vec<Result<i32, MyError>> = 1;\n    return;\n}\n";
    fs::write(&single, single_text).unwrap();
    let diagnostics = lsp_diagnostics(&file_uri(&single), single_text);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0]["message"],
        "expected `std.collections.Vec<std.Result<i32, MyError>>`, found `i32`"
    );

    fs::remove_dir_all(base).unwrap();
}

/// DIAG-05：错误程序退出 1、不产生可执行产物、stderr 无 `panicked`。
#[test]
fn diag_05_error_program_no_panic_no_artifact() {
    let base = temp_dir("diag05");
    let home = base.join("home");
    let project = base.join("project");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(
        project.join("dolphin.toml"),
        r#"
        [package]
        group = "g"
        name = "diag05"
        version = "0.1.0"

        [[bin]]
        name = "diag05"
        path = "src/main.do"
        "#,
    )
    .unwrap();
    fs::write(
        project.join("src/main.do"),
        "fn main() { return missing; }\n",
    )
    .unwrap();

    let output = run_dc(&home, &["build", project.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr_text(&output));
    let stderr = stderr_text(&output);
    assert!(stderr.contains("error[E0001]: unknown variable `missing`"));
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert!(
        !project.join("target").exists(),
        "错误程序不得产生任何构建产物"
    );

    // `dc run` 同样在编译前被收集式检查拦截。
    let run = run_dc(&home, &["run", project.to_str().unwrap()]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr_text(&run));
    assert!(!project.join("target").exists());

    fs::remove_dir_all(base).unwrap();
}

/// diag_06：词法/语法/声明级各 100 条 + `E0002`，行为确定。
#[test]
fn diag_06_error_collection_bound() {
    let base = temp_dir("diag06");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();

    let lexical = base.join("lexical.do");
    fs::write(
        &lexical,
        format!("fn main() {{ {} return 0; }}\n", "§".repeat(120)),
    )
    .unwrap();
    let output = run_dc(&home, &["check", lexical.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr_text(&output));
    let stderr = stderr_text(&output);
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert_eq!(
        stderr.matches("error[E0001]").count(),
        100,
        "词法诊断必须恰好 100 条"
    );
    assert_eq!(
        stderr.matches("error[E0002]").count(),
        1,
        "上限后必须追加一条 E0002"
    );
    assert!(stderr.contains("too many errors; further diagnostics suppressed"));

    let syntax = base.join("syntax.do");
    let mut text = String::from("fn main() {\n");
    for index in 0..120 {
        text.push_str(&format!("    val x{index}: = 1;\n"));
    }
    text.push_str("}\n");
    fs::write(&syntax, text).unwrap();
    let output = run_dc(&home, &["check", syntax.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr_text(&output));
    let stderr = stderr_text(&output);
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert_eq!(
        stderr.matches("error[E0001]").count(),
        100,
        "语法诊断必须恰好 100 条"
    );
    assert_eq!(stderr.matches("error[E0002]").count(), 1);

    let project = base.join("project");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("dolphin.toml"),
        r#"
        [package]
        group = "g"
        name = "diag06"
        version = "0.1.0"

        [[bin]]
        name = "diag06"
        path = "src/main.do"
        "#,
    )
    .unwrap();
    let mut declarations = String::new();
    for index in 0..120 {
        declarations.push_str(&format!("impl Missing{index} {{ }}\n"));
    }
    declarations.push_str("fn main() { return 0; }\n");
    fs::write(project.join("src/main.do"), declarations).unwrap();
    let output = run_dc(&home, &["check", project.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr_text(&output));
    let stderr = stderr_text(&output);
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert_eq!(
        stderr.matches("error[E0001]").count(),
        100,
        "声明级诊断必须恰好 100 条"
    );
    assert_eq!(stderr.matches("error[E0002]").count(), 1);

    fs::remove_dir_all(base).unwrap();
}
