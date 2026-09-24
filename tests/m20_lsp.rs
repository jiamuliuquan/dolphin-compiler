//! H20-03 项目级 LSP 的集成验收（LSP-01..06）。
//!
//! 全部通过真实 `dc lsp <项目>` 子进程 stdio 会话（`tests/support/lsp.rs`）；
//! 每个验收点断言固定期望：诊断 code/message/range/version、definition URI 与
//! `name_span`、hover 文本、协议错误码与退出码，不只比较两个后端一致。

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_analysis::path_to_uri;
use serde_json::{Value, json};
use support::lsp::LspSession;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dolphin-m20-lsp-{tag}-{}-{unique}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory");
    }
    fs::write(path, text).expect("source file should be written");
}

/// 固定握手：`initialize(id=1, rootUri)` → 断言 capabilities → `initialized`。
fn initialize(session: &mut LspSession, project: &Path) {
    session.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "rootUri": path_to_uri(project) },
    }));
    let response = session.recv_response(1);
    assert!(
        response.get("error").is_none(),
        "initialize failed: {response}"
    );
    let capabilities = &response["result"]["capabilities"];
    assert_eq!(capabilities["textDocumentSync"], 1);
    assert_eq!(capabilities["hoverProvider"], true);
    assert_eq!(capabilities["definitionProvider"], true);
    assert_eq!(capabilities["documentSymbolProvider"], true);
    assert_eq!(capabilities["positionEncoding"], "utf-16");
    session.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
}

fn open(session: &mut LspSession, path: &Path, text: &str, version: u64) {
    session.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": path_to_uri(path),
                "languageId": "dolphin",
                "version": version,
                "text": text,
            }
        }
    }));
}

fn change(session: &mut LspSession, path: &Path, text: &str, version: u64) {
    session.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": path_to_uri(path), "version": version },
            "contentChanges": [ { "text": text } ],
        }
    }));
}

fn close(session: &mut LspSession, path: &Path) {
    session.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didClose",
        "params": { "textDocument": { "uri": path_to_uri(path) } },
    }));
}

fn position_request(
    session: &mut LspSession,
    id: i64,
    method: &str,
    path: &Path,
    position: (u64, u64),
) -> Value {
    session.send(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": {
            "textDocument": { "uri": path_to_uri(path) },
            "position": { "line": position.0, "character": position.1 },
        }
    }));
    session.recv_response(id)
}

/// `needle` 首字节的 0-based LSP 位置（`needle` 必须完整位于一行内）。
fn position_of(text: &str, needle: &str) -> (u64, u64) {
    let offset = text
        .find(needle)
        .unwrap_or_else(|| panic!("missing `{needle}` in fixture"));
    let line = text[..offset].matches('\n').count() as u64;
    let line_start = text[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    (line, (offset - line_start) as u64)
}

fn position_after(text: &str, needle: &str, extra: usize) -> (u64, u64) {
    let (line, character) = position_of(text, needle);
    (line, character + extra as u64)
}

fn diagnostics_of(notification: &Value) -> &Vec<Value> {
    notification["params"]["diagnostics"]
        .as_array()
        .unwrap_or_else(|| panic!("diagnostics array: {notification}"))
}

/// 写一个 path 依赖 `stats`（`pub fn count(): i32`）与应用 `app` 的最小项目。
fn project_with_dependency(base: &Path) -> (PathBuf, PathBuf) {
    let stats = base.join("stats");
    write(
        &stats.join("dolphin.toml"),
        "[package]\ngroup = \"org.example\"\nname = \"stats\"\nversion = \"1.0.0\"\n\n[lib]\npath = \"src/lib.do\"\n",
    );
    write(
        &stats.join("src/lib.do"),
        "pub fn count(): i32 { return 1; }\n",
    );
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        "[package]\ngroup = \"g\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"app\"\npath = \"src/main.do\"\n\n[dependencies]\nstats = { path = \"../stats\" }\n",
    );
    (app, stats)
}

/// LSP-01：含 `pkg`/`use` 的文档得到项目语义诊断（单文件模式会跳过语义检查）。
#[test]
fn lsp_01_pkg_use_type_error() {
    let base = temp_dir("lsp01");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let (app, _stats) = project_with_dependency(&base);
    let main_text =
        "use stats.count;\nfn main() {\n    val total: bool = count();\n    return;\n}\n";
    let main = app.join("src/main.do");
    write(&main, main_text);

    let mut session = LspSession::start_with_home(&app, &home);
    initialize(&mut session, &app);
    open(&mut session, &main, main_text, 1);
    let notification = session.recv_notification("textDocument/publishDiagnostics");
    let params = &notification["params"];
    assert_eq!(params["uri"], path_to_uri(&main));
    assert_eq!(params["version"], 1);
    let diagnostics = diagnostics_of(&notification);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["code"], "E0001");
    assert_eq!(diagnostics[0]["message"], "expected `bool`, found `i32`");
    assert_eq!(diagnostics[0]["severity"], 1);
    assert_eq!(diagnostics[0]["source"], "dolphin");
    let call = position_of(main_text, "count()");
    assert_eq!(
        diagnostics[0]["range"]["start"],
        json!({ "line": call.0, "character": call.1 })
    );
    assert_eq!(
        diagnostics[0]["range"]["end"],
        json!({ "line": call.0, "character": call.1 + 7 })
    );

    session.shutdown();
    let (status, stderr) = session.finish_with_stderr();
    assert_eq!(status.code(), Some(0));
    assert_eq!(stderr, "");
    fs::remove_dir_all(base).unwrap();
}

/// LSP-02：definition 指向依赖源码（跨包）与根包其他文件（跨文件）的真实 `name_span`。
#[test]
fn lsp_02_cross_file_and_cross_package_definition() {
    let base = temp_dir("lsp02");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let (app, stats) = project_with_dependency(&base);
    let util_text = "pub fn helper(): i32 { return 1; }\n";
    let util = app.join("src/util.do");
    write(&util, util_text);
    let main_text = "\
use stats.count;
fn main() {
    val a = helper();
    val b = count();
    return a + b;
}
";
    let main = app.join("src/main.do");
    write(&main, main_text);

    let mut session = LspSession::start_with_home(&app, &home);
    initialize(&mut session, &app);
    open(&mut session, &main, main_text, 1);
    let notification = session.recv_notification("textDocument/publishDiagnostics");
    assert!(diagnostics_of(&notification).is_empty(), "{notification}");

    // 跨文件：`helper` 定义在根包的另一个文件。
    let response = position_request(
        &mut session,
        2,
        "textDocument/definition",
        &main,
        position_of(main_text, "helper()"),
    );
    let location = &response["result"];
    assert_eq!(location["uri"], path_to_uri(&util));
    assert_eq!(
        location["range"]["start"],
        json!({ "line": 0, "character": 7 })
    );
    assert_eq!(
        location["range"]["end"],
        json!({ "line": 0, "character": 13 })
    );

    // 跨包：`count` 定义在 path 依赖 `stats` 的源码里。
    let response = position_request(
        &mut session,
        3,
        "textDocument/definition",
        &main,
        position_of(main_text, "count()"),
    );
    let location = &response["result"];
    assert_eq!(location["uri"], path_to_uri(&stats.join("src/lib.do")));
    assert_eq!(
        location["range"]["start"],
        json!({ "line": 0, "character": 7 })
    );
    assert_eq!(
        location["range"]["end"],
        json!({ "line": 0, "character": 12 })
    );

    // hover 使用定义身份与显示名：根包函数不带前缀，依赖包函数带包名。
    let response = position_request(
        &mut session,
        4,
        "textDocument/hover",
        &main,
        position_of(main_text, "helper()"),
    );
    assert_eq!(response["result"]["contents"]["value"], "fn `helper`");
    let response = position_request(
        &mut session,
        5,
        "textDocument/hover",
        &main,
        position_of(main_text, "count()"),
    );
    assert_eq!(response["result"]["contents"]["value"], "fn `stats.count`");

    session.shutdown();
    assert_eq!(session.finish().code(), Some(0));
    fs::remove_dir_all(base).unwrap();
}

/// LSP-03：参数、局部变量与遮蔽解析到最近绑定；hover 显示类型标注。
#[test]
fn lsp_03_local_shadowing_and_parameters() {
    let base = temp_dir("lsp03");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        "[package]\ngroup = \"g\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"app\"\npath = \"src/main.do\"\n",
    );
    let text = "\
fn helper(value: i32): i32 { return value; }
fn main() {
    val x = 1;
    if true {
        val x = helper(2);
        val y = x;
    }
    return x;
}
";
    let main = app.join("src/main.do");
    write(&main, text);

    let mut session = LspSession::start_with_home(&app, &home);
    initialize(&mut session, &app);
    open(&mut session, &main, text, 1);
    let notification = session.recv_notification("textDocument/publishDiagnostics");
    assert!(diagnostics_of(&notification).is_empty(), "{notification}");

    // 遮蔽：内层 `x` 的使用解析到内层绑定。
    let inner_decl = position_after(text, "val x = helper(2);", 4);
    let response = position_request(
        &mut session,
        2,
        "textDocument/definition",
        &main,
        position_after(text, "val y = x;", 8),
    );
    let location = &response["result"];
    assert_eq!(location["uri"], path_to_uri(&main));
    assert_eq!(
        location["range"]["start"],
        json!({ "line": inner_decl.0, "character": inner_decl.1 })
    );
    assert_eq!(
        location["range"]["end"],
        json!({ "line": inner_decl.0, "character": inner_decl.1 + 1 })
    );

    // 外层 `x` 的使用不被内层遮蔽影响。
    let outer_decl = position_after(text, "val x = 1;", 4);
    let response = position_request(
        &mut session,
        3,
        "textDocument/definition",
        &main,
        position_after(text, "return x;", 7),
    );
    let location = &response["result"];
    assert_eq!(
        location["range"]["start"],
        json!({ "line": outer_decl.0, "character": outer_decl.1 })
    );

    // 参数：`value` 的使用解析到参数绑定，hover 带类型标注。
    let parameter = position_after(text, "fn helper(value: i32)", 10);
    let value_use = position_after(text, "return value;", 7);
    let response = position_request(&mut session, 4, "textDocument/definition", &main, value_use);
    let location = &response["result"];
    assert_eq!(
        location["range"]["start"],
        json!({ "line": parameter.0, "character": parameter.1 })
    );
    let response = position_request(&mut session, 5, "textDocument/hover", &main, value_use);
    assert_eq!(
        response["result"]["contents"]["value"],
        "local `value`: i32"
    );
    assert_eq!(
        response["result"]["range"]["start"],
        json!({ "line": value_use.0, "character": value_use.1 })
    );

    session.shutdown();
    assert_eq!(session.finish().code(), Some(0));
    fs::remove_dir_all(base).unwrap();
}

/// LSP-04：打开/变更/关闭依赖 overlay 的事件序列正确（调用方诊断随 overlay 变化）。
#[test]
fn lsp_04_open_change_close_dependency_change() {
    let base = temp_dir("lsp04");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let (app, stats) = project_with_dependency(&base);
    let main_text = "\
use stats.count;
fn main() {
    val total: i32 = count();
    return;
}
";
    let main = app.join("src/main.do");
    write(&main, main_text);
    let dep = stats.join("src/lib.do");
    let dep_disk = fs::read_to_string(&dep).unwrap();

    let mut session = LspSession::start_with_home(&app, &home);
    initialize(&mut session, &app);

    open(&mut session, &main, main_text, 1);
    let notification = session.recv_notification("textDocument/publishDiagnostics");
    assert_eq!(notification["params"]["uri"], path_to_uri(&main));
    assert_eq!(notification["params"]["version"], 1);
    assert!(diagnostics_of(&notification).is_empty(), "{notification}");

    open(&mut session, &dep, &dep_disk, 1);
    let notification = session.recv_notification("textDocument/publishDiagnostics");
    assert_eq!(notification["params"]["uri"], path_to_uri(&dep));
    assert_eq!(notification["params"]["version"], 1);
    assert!(diagnostics_of(&notification).is_empty(), "{notification}");

    // 未保存依赖把返回类型改成 bool：调用方出现固定类型诊断，带调用方文档版本。
    change(
        &mut session,
        &dep,
        "pub fn count(): bool { return true; }\n",
        2,
    );
    let notification = session.recv_notification("textDocument/publishDiagnostics");
    assert_eq!(notification["params"]["uri"], path_to_uri(&main));
    assert_eq!(notification["params"]["version"], 1);
    let diagnostics = diagnostics_of(&notification);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["code"], "E0001");
    assert_eq!(diagnostics[0]["message"], "expected `i32`, found `bool`");
    let call = position_of(main_text, "count()");
    assert_eq!(
        diagnostics[0]["range"]["start"],
        json!({ "line": call.0, "character": call.1 })
    );
    assert_eq!(
        diagnostics[0]["range"]["end"],
        json!({ "line": call.0, "character": call.1 + 7 })
    );

    // didClose：立即发布该 URI 空诊断，调用方恢复磁盘版本后诊断清空。
    close(&mut session, &dep);
    let closed = session.recv_notification("textDocument/publishDiagnostics");
    assert_eq!(closed["params"]["uri"], path_to_uri(&dep));
    assert!(closed["params"].get("version").is_none(), "{closed}");
    assert!(diagnostics_of(&closed).is_empty(), "{closed}");
    let restored = session.recv_notification("textDocument/publishDiagnostics");
    assert_eq!(restored["params"]["uri"], path_to_uri(&main));
    assert_eq!(restored["params"]["version"], 1);
    assert!(diagnostics_of(&restored).is_empty(), "{restored}");

    // 一次依赖 overlay 变化同时改变两个打开文档的诊断：按 URI 字典序升序发布（§7.3 规则 2）。
    open(
        &mut session,
        &dep,
        "pub fn count(): bool { return true;\n",
        2,
    );
    let dep_syntax = session.recv_notification("textDocument/publishDiagnostics");
    assert_eq!(dep_syntax["params"]["uri"], path_to_uri(&dep));
    assert_eq!(dep_syntax["params"]["version"], 2);
    assert_eq!(
        diagnostics_of(&dep_syntax)[0]["message"],
        "expected `}` after block"
    );

    change(
        &mut session,
        &dep,
        "pub fn count(): bool { return true; }\n",
        3,
    );
    let first = session.recv_notification("textDocument/publishDiagnostics");
    let second = session.recv_notification("textDocument/publishDiagnostics");
    let first_uri = first["params"]["uri"].as_str().unwrap().to_string();
    let second_uri = second["params"]["uri"].as_str().unwrap().to_string();
    assert!(
        first_uri < second_uri,
        "发布顺序必须是 URI 字典序升序：{first_uri} !< {second_uri}"
    );
    assert_eq!(first_uri, path_to_uri(&main));
    assert_eq!(second_uri, path_to_uri(&dep));
    assert_eq!(
        diagnostics_of(&first)[0]["message"],
        "expected `i32`, found `bool`"
    );
    assert!(diagnostics_of(&second).is_empty(), "{second}");

    session.shutdown();
    assert_eq!(session.finish().code(), Some(0));
    fs::remove_dir_all(base).unwrap();
}

/// LSP-05：连续版本更新按最新文本发布；旧版本 overlay 被忽略。
#[test]
fn lsp_05_sequential_versions_latest_wins() {
    let base = temp_dir("lsp05");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        "[package]\ngroup = \"g\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"app\"\npath = \"src/main.do\"\n",
    );
    let main = app.join("src/main.do");
    let version1 = "fn main() { return missing1; }\n";
    let version2 = "fn main() { return missing2; }\nfn extra() { return; }\n";
    let version3 = "fn main() { return 0; }\nfn extra() { return; }\n";
    write(&main, version1);

    let mut session = LspSession::start_with_home(&app, &home);
    initialize(&mut session, &app);
    open(&mut session, &main, version1, 1);
    let first = session.recv_notification("textDocument/publishDiagnostics");
    assert_eq!(first["params"]["version"], 1);
    assert_eq!(diagnostics_of(&first).len(), 1);
    assert_eq!(
        diagnostics_of(&first)[0]["message"],
        "unknown variable `missing1`"
    );

    // 连续两次变更：每次都发布对应版本与最新文本的诊断。
    change(&mut session, &main, version2, 2);
    change(&mut session, &main, version3, 3);
    let second = session.recv_notification("textDocument/publishDiagnostics");
    assert_eq!(second["params"]["version"], 2);
    assert_eq!(
        diagnostics_of(&second)[0]["message"],
        "unknown variable `missing2`"
    );
    let third = session.recv_notification("textDocument/publishDiagnostics");
    assert_eq!(third["params"]["version"], 3);
    assert!(diagnostics_of(&third).is_empty(), "{third}");

    // 旧版本 overlay：不提升 revision、不改写文本；documentSymbol 仍是版本 3 内容。
    change(&mut session, &main, version2, 2);
    session.send(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "textDocument/documentSymbol",
        "params": { "textDocument": { "uri": path_to_uri(&main) } }
    }));
    let response = session.recv_response(6);
    let names = response["result"]
        .as_array()
        .expect("symbol array")
        .iter()
        .filter_map(|symbol| symbol["name"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["main", "extra"], "{response}");

    session.shutdown();
    assert_eq!(session.finish().code(), Some(0));
    fs::remove_dir_all(base).unwrap();
}

/// LSP-06：capabilities、-32601/-32002/-32600、未知通知忽略与退出码按 §7.2。
#[test]
fn lsp_06_unknown_request_lifecycle_conformance() {
    let base = temp_dir("lsp06");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let project = base.join("project");
    fs::create_dir_all(&project).unwrap();

    // 未 initialize 的请求：-32002，且不返回 result。
    let mut session = LspSession::start_with_home(&project, &home);
    session.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/hover",
        "params": {}
    }));
    let response = session.recv_response(2);
    assert_eq!(response["error"]["code"], -32002);
    assert!(response.get("result").is_none(), "{response}");

    // 固定握手；capabilities 含 positionEncoding。
    initialize(&mut session, &project);

    // 未知通知：忽略且不响应；后续请求仍得到正确响应。
    session.send(json!({
        "jsonrpc": "2.0",
        "method": "workspace/didSomethingUnknown",
        "params": {}
    }));
    session.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "workspace/unknownThing",
        "params": {}
    }));
    let response = session.recv_response(3);
    assert_eq!(response["error"]["code"], -32601);
    assert_eq!(response["error"]["message"], "Method not found");
    assert!(response.get("result").is_none(), "{response}");

    // shutdown → result null；shutdown 之后的请求 → -32600。
    session.send(json!({ "jsonrpc": "2.0", "id": 4, "method": "shutdown" }));
    let response = session.recv_response(4);
    assert!(response["result"].is_null(), "{response}");
    session.send(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "textDocument/documentSymbol",
        "params": {}
    }));
    let response = session.recv_response(5);
    assert_eq!(response["error"]["code"], -32600);
    session.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    assert_eq!(session.finish().code(), Some(0));

    // 未 shutdown 就 exit：退出码 1（D-M20-2）。
    let mut abrupt = LspSession::start_with_home(&project, &home);
    initialize(&mut abrupt, &project);
    abrupt.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    assert_eq!(abrupt.finish().code(), Some(1));

    // EOF（客户端断开）：退出码 0（保持现状）。
    let mut eof = LspSession::start_with_home(&project, &home);
    initialize(&mut eof, &project);
    assert_eq!(eof.finish().code(), Some(0));

    fs::remove_dir_all(base).unwrap();
}

/// 边界：非法 `Content-Length` 终止服务、stderr 写原因、退出码非 0（§7.2）。
#[test]
fn lsp_malformed_frame_terminates_with_error() {
    let mut session = LspSession::start_default();
    session.send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
    session.recv_response(1);
    session.send_raw(b"Content-Length: 999999999\r\n\r\n");
    let (status, stderr) = session.finish_with_stderr();
    assert_eq!(status.code(), Some(1), "stderr: {stderr}");
    assert!(
        stderr.contains("exceeds the 16777216 byte limit"),
        "stderr 必须写原因：{stderr}"
    );
}

/// 边界：不带位置参数的 `dc lsp` 保持可用（默认当前目录，单文件模式）。
#[test]
fn lsp_accepts_no_project_argument() {
    let mut session = LspSession::start_default();
    session.send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
    let response = session.recv_response(1);
    assert_eq!(
        response["result"]["capabilities"]["positionEncoding"],
        "utf-16"
    );
    session.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    session.shutdown();
    assert_eq!(session.finish().code(), Some(0));
}

/// 边界：非 `file` scheme 的文档只做单文件分析，不进入项目 overlay/索引。
#[test]
fn lsp_non_file_uri_stays_single_file() {
    let mut session = LspSession::start_default();
    session.send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
    session.recv_response(1);
    session.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));

    let uri = "untitled:Untitled-1";
    let text = "fn main() { return missing; }\n";
    session.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": { "uri": uri, "languageId": "dolphin", "version": 1, "text": text }
        }
    }));
    let notification = session.recv_notification("textDocument/publishDiagnostics");
    assert_eq!(notification["params"]["uri"], uri);
    let diagnostics = diagnostics_of(&notification);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["code"], "E0001");
    assert_eq!(diagnostics[0]["message"], "unknown variable `missing`");

    // 语义失败（未知变量）时索引不可用：definition 返回 null，而不是文本同名猜测。
    session.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/definition",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 19 }
        }
    }));
    let response = session.recv_response(2);
    assert!(response["result"].is_null(), "{response}");

    session.shutdown();
    assert_eq!(session.finish().code(), Some(0));
}

/// 边界：项目依赖不可本地恢复时，`window/showMessage` 报告原因，文档仍发布语法诊断。
#[test]
fn lsp_project_failure_keeps_syntax_diagnostics() {
    let base = temp_dir("lspfail");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let app = base.join("app");
    write(
        &app.join("dolphin.toml"),
        "[package]\ngroup = \"g\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"app\"\npath = \"src/main.do\"\n\n[repositories]\ndefault = \"http://127.0.0.1:9\"\n\n[dependencies]\nmissing = \"org.example:missing:1.0.0\"\n",
    );
    let main_text = "fn main() { return 0;\n";
    let main = app.join("src/main.do");
    write(&main, main_text);

    let mut session = LspSession::start_with_home(&app, &home);
    initialize(&mut session, &app);
    open(&mut session, &main, main_text, 1);

    let message = session.recv_notification("window/showMessage");
    assert_eq!(message["params"]["type"], 1);
    let text = message["params"]["message"].as_str().unwrap();
    assert!(
        text.contains("org.example:missing:1.0.0") && text.contains("run `dc fetch` and retry"),
        "项目诊断必须说明依赖坐标与恢复方式：{text}"
    );

    // “无法分析”不等于“没有错误”：语法诊断照常发布。
    let notification = session.recv_notification("textDocument/publishDiagnostics");
    assert_eq!(notification["params"]["uri"], path_to_uri(&main));
    let diagnostics = diagnostics_of(&notification);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["code"], "E0001");
    assert_eq!(diagnostics[0]["message"], "expected `}` after block");

    session.shutdown();
    assert_eq!(session.finish().code(), Some(0));
    // 分析路径零副作用：不写锁、不产出构建产物。
    assert!(!app.join("dolphin.lock").exists());
    assert!(!app.join("target").exists());
    fs::remove_dir_all(base).unwrap();
}
