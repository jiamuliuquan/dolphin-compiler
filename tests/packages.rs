//! M15-C–F 端到端：lib 包、本地 path 依赖、`.dlib` 发布与远程消费（PKG-05…14）。
//!
//! 所有测试使用独立的临时 `DOLPHIN_HOME`、临时仓库与 loopback HTTP 服务，
//! 不依赖真实网站或全局缓存。

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_compiler::sha256_hex;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn dc() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dc"));
    // 本地 HTTP fixture 使用回环地址；显式绕过开发机上可能设置的代理，避免
    // 请求被转发到代理而报 `Peer disconnected`（见 docs/installation.md 代理一节）。
    command
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost");
    command
}

fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dolphin-packages-{tag}-{}-{unique}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).expect("parent");
    fs::write(path, contents).expect("write");
}

fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.starts_with('/') {
        format!("file://{text}")
    } else {
        format!("file:///{text}")
    }
}

/// 写一个库项目；`extra` 追加到清单末尾（例如依赖或原生文件）。
fn lib_project(root: &Path, group: &str, name: &str, version: &str, lib_source: &str, extra: &str) {
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
{extra}
"#
        ),
    );
    write(&root.join("src/lib.do"), lib_source);
}

fn bin_project(root: &Path, group: &str, name: &str, main_source: &str, extra: &str) {
    write(
        &root.join("dolphin.toml"),
        &format!(
            r#"
[package]
group = "{group}"
name = "{name}"
version = "0.1.0"

[[bin]]
name = "{name}"
path = "src/main.do"
{extra}
"#
        ),
    );
    write(&root.join("src/main.do"), main_source);
}

/// 在隔离的 `DOLPHIN_HOME` 下运行 dc。
fn run_dc(project: &Path, home: &Path, args: &[&str]) -> Output {
    dc().arg(args[0])
        .args(&args[1..])
        .arg(project)
        .env("DOLPHIN_HOME", home)
        .output()
        .expect("dc should run")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn executable(project: &Path, name: &str) -> PathBuf {
    let path = project.join("target").join(name);
    if cfg!(windows) {
        path.with_extension("exe")
    } else {
        path
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success, got {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        stdout(output),
        stderr(output)
    );
}

fn assert_failure_contains(output: &Output, needle: &str) {
    assert!(
        !output.status.success(),
        "expected failure but succeeded\nstdout: {}",
        stdout(output)
    );
    assert!(
        stderr(output).contains(needle),
        "expected stderr to contain `{needle}`, got: {}",
        stderr(output)
    );
}

/// 发布一个库到仓库并返回其项目目录。
fn publish_library(base: &Path, home: &Path, name: &str, lib_source: &str, extra: &str) -> PathBuf {
    let project = base.join(name);
    let repo_url = file_url(&base.join("repo"));
    let repositories = format!("\n[repositories]\ndefault = \"{repo_url}\"\n");
    lib_project(
        &project,
        "org.example",
        name,
        "1.0.0",
        lib_source,
        &format!("{repositories}{extra}"),
    );
    let output = run_dc(&project, home, &["publish", "--repository", "default"]);
    assert_success(&output);
    project
}

#[test]
fn pkg05_publish_then_consume_with_coordinates_only() {
    let base = temp_dir("pkg05");
    let home = base.join("home");
    let repo = base.join("repo");
    fs::create_dir_all(&home).unwrap();
    let repo_url = file_url(&repo);

    publish_library(
        &base,
        &home,
        "mathlib",
        "pub fn add(a: i32, b: i32): i32 { return a + b; }\npub fn twice<T>(value: T): T { return value + value; }",
        "",
    );
    assert!(
        repo.join("org/example/mathlib/1.0.0/mathlib-1.0.0.dlib")
            .is_file()
    );
    assert!(
        repo.join("org/example/mathlib/1.0.0/mathlib-1.0.0.dlib.sha256")
            .is_file()
    );

    // 消费端只写仓库与坐标，不引用本地源码路径。
    let app = base.join("app");
    bin_project(
        &app,
        "org.example",
        "app",
        "use math.add;\nuse math.twice;\nfn main() { return add(twice<i32>(20), 2); }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\nmath = \"org.example:mathlib:1.0.0\"\n"
        ),
    );
    let output = run_dc(&app, &home, &["build"]);
    assert_success(&output);
    let status = Command::new(executable(&app, "app"))
        .status()
        .expect("app runs");
    assert_eq!(status.code(), Some(42));
    let lock = fs::read_to_string(app.join("dolphin.lock")).unwrap();
    assert!(lock.contains("org.example:mathlib:1.0.0"));

    fs::remove_dir_all(base).ok();
}

#[test]
fn pkg06_transitive_and_diamond_dependencies() {
    let base = temp_dir("pkg06");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let repo_url = file_url(&base.join("repo"));

    // leaf
    publish_library(
        &base,
        &home,
        "leaf",
        "pub fn leaf(): i32 { return 30; }",
        "",
    );
    // middle 依赖 leaf（坐标）。
    publish_library(
        &base,
        &home,
        "middle",
        "use leaf;\npub fn middle(): i32 { return leaf.leaf() + 10; }",
        "\n[dependencies]\nleaf = \"org.example:leaf:1.0.0\"\n",
    );
    // 两个中间包依赖同一个 leaf，构成菱形。
    publish_library(
        &base,
        &home,
        "left",
        "use leaf;\npub fn left(): i32 { return leaf.leaf() + 1; }",
        "\n[dependencies]\nleaf = \"org.example:leaf:1.0.0\"\n",
    );

    let app = base.join("app");
    bin_project(
        &app,
        "org.example",
        "app",
        "use middle;\nuse left;\nfn main() { return middle.middle() + left.left(); }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\nmiddle = \"org.example:middle:1.0.0\"\nleft = \"org.example:left:1.0.0\"\n"
        ),
    );
    let output = run_dc(&app, &home, &["build"]);
    assert_success(&output);
    let status = Command::new(executable(&app, "app"))
        .status()
        .expect("app runs");
    // middle 40 + left 31 = 71
    assert_eq!(status.code(), Some(71));

    let lock = fs::read_to_string(app.join("dolphin.lock")).unwrap();
    assert!(lock.contains("org.example:leaf:1.0.0"));
    assert!(lock.contains("org.example:middle:1.0.0"));
    assert!(lock.contains("org.example:left:1.0.0"));
    // 三个包各一条 `[[package]]` 记录（共享节点去重）。
    assert_eq!(lock.matches("[[package]]").count(), 3);

    fs::remove_dir_all(base).ok();
}

#[test]
fn pkg07_locked_and_offline_rebuild() {
    let base = temp_dir("pkg07");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let repo_url = file_url(&base.join("repo"));
    publish_library(
        &base,
        &home,
        "mathlib",
        "pub fn answer(): i32 { return 42; }",
        "",
    );

    let app = base.join("app");
    bin_project(
        &app,
        "org.example",
        "app",
        "use math;\nfn main() { return math.answer(); }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\nmath = \"org.example:mathlib:1.0.0\"\n"
        ),
    );
    assert_success(&run_dc(&app, &home, &["build"]));
    assert_success(&run_dc(&app, &home, &["build", "--locked", "--offline"]));

    // `file://` 仓库在离线模式下允许直接读取，因此空缓存 + file 仓库可重建。
    // 「缺缓存必须报错」针对 HTTP 源，见 HTTP fixture 测试。
    let empty_home = base.join("empty-home");
    fs::create_dir_all(&empty_home).unwrap();
    assert_success(&run_dc(
        &app,
        &empty_home,
        &["build", "--locked", "--offline"],
    ));

    // 修改依赖别名后 --locked 报锁过期。
    let manifest = fs::read_to_string(app.join("dolphin.toml")).unwrap();
    write(
        &app.join("dolphin.toml"),
        &manifest.replace(
            "math = \"org.example:mathlib:1.0.0\"",
            "maths = \"org.example:mathlib:1.0.0\"",
        ),
    );
    let stale = run_dc(&app, &home, &["build", "--locked"]);
    assert_failure_contains(&stale, "out of date");

    fs::remove_dir_all(base).ok();
}

#[test]
fn pkg08_pkg13_bad_checksum_and_content_change_are_rejected() {
    let base = temp_dir("pkg08");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let repo = base.join("repo");
    let repo_url = file_url(&repo);
    publish_library(
        &base,
        &home,
        "mathlib",
        "pub fn answer(): i32 { return 42; }",
        "",
    );
    let dlib = repo.join("org/example/mathlib/1.0.0/mathlib-1.0.0.dlib");
    let checksum = repo.join("org/example/mathlib/1.0.0/mathlib-1.0.0.dlib.sha256");

    // 坏摘要：篡改 .sha256。
    let original_checksum = fs::read_to_string(&checksum).unwrap();
    fs::write(&checksum, format!("{}\n", "0".repeat(64))).unwrap();
    let app = base.join("app");
    bin_project(
        &app,
        "org.example",
        "app",
        "use math;\nfn main() { return math.answer(); }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\nmath = \"org.example:mathlib:1.0.0\"\n"
        ),
    );
    let bad = run_dc(&app, &home, &["build"]);
    assert_failure_contains(&bad, "checksum mismatch");
    fs::write(&checksum, &original_checksum).unwrap();

    // 正常构建生成锁。
    assert_success(&run_dc(&app, &home, &["build"]));

    // 同一坐标远程内容被替换（PKG-13）：即使没有 --locked 也不接受新摘要。
    // 重新发布同版本不同内容会改变远程 `.sha256`，与锁记录的摘要冲突。
    {
        let mut bytes = fs::read(&dlib).unwrap();
        bytes.push(0);
        fs::write(&dlib, &bytes).unwrap();
        fs::write(&checksum, format!("{}\n", sha256_hex(&bytes))).unwrap();
    }
    let fresh_home = base.join("changed-home");
    fs::create_dir_all(&fresh_home).unwrap();
    let changed = run_dc(&app, &fresh_home, &["build"]);
    assert_failure_contains(&changed, "changed content");

    // 404：清空缓存并移除包文件。
    let gone_home = base.join("gone-home");
    fs::create_dir_all(&gone_home).unwrap();
    fs::remove_file(&dlib).unwrap();
    fs::remove_file(&checksum).unwrap();
    let missing = run_dc(&app, &gone_home, &["build"]);
    assert_failure_contains(&missing, "not found");

    fs::remove_dir_all(base).ok();
}

#[test]
fn pkg12_publish_is_idempotent_and_blocks_overwrite() {
    let base = temp_dir("pkg12");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let project = publish_library(
        &base,
        &home,
        "mathlib",
        "pub fn answer(): i32 { return 42; }",
        "",
    );

    // 重复发布同一字节：幂等成功。
    let again = run_dc(&project, &home, &["publish", "--repository", "default"]);
    assert_success(&again);
    assert!(stdout(&again).contains("already present"));

    // 改变内容后重复发布：拒绝覆盖。
    write(
        &project.join("src/lib.do"),
        "pub fn answer(): i32 { return 7; }",
    );
    let overwrite = run_dc(&project, &home, &["publish", "--repository", "default"]);
    assert_failure_contains(&overwrite, "refusing to overwrite");

    // 半上传恢复：包在、摘要缺失时应补传摘要。
    let checksum = base.join("repo/org/example/mathlib/1.0.0/mathlib-1.0.0.dlib.sha256");
    fs::remove_file(&checksum).unwrap();
    // 恢复原始源码，使包字节与仓库中的包一致。
    write(
        &project.join("src/lib.do"),
        "pub fn answer(): i32 { return 42; }",
    );
    let resumed = run_dc(&project, &home, &["publish", "--repository", "default"]);
    assert_success(&resumed);
    assert!(checksum.is_file());

    fs::remove_dir_all(base).ok();
}

#[test]
fn pkg14_file_repository_with_spaces_and_chinese_path() {
    let base = temp_dir("pkg14");
    let home = base.join("home");
    let repo = base.join("仓库 with spaces");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&repo).unwrap();
    let repo_url = file_url(&repo);

    let project = base.join("mathlib");
    lib_project(
        &project,
        "org.example",
        "mathlib",
        "1.0.0",
        "pub fn answer(): i32 { return 42; }",
        &format!("\n[repositories]\ndefault = \"{repo_url}\"\n"),
    );
    assert_success(&run_dc(
        &project,
        &home,
        &["publish", "--repository", "default"],
    ));

    let app = base.join("app");
    bin_project(
        &app,
        "org.example",
        "app",
        "use math;\nfn main() { return math.answer(); }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\nmath = \"org.example:mathlib:1.0.0\"\n"
        ),
    );
    assert_success(&run_dc(&app, &home, &["build"]));
    let status = Command::new(executable(&app, "app"))
        .status()
        .expect("app runs");
    assert_eq!(status.code(), Some(42));

    fs::remove_dir_all(base).ok();
}

/// 极简 HTTP fixture：GET 读取文件，PUT 条件创建（`If-None-Match: *`）。
struct HttpFixture {
    base: String,
    requests: Arc<AtomicU64>,
    root: PathBuf,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl HttpFixture {
    fn start(root: PathBuf) -> HttpFixture {
        fs::create_dir_all(&root).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(AtomicU64::new(0));
        let files: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));
        let counter = requests.clone();
        let store = files.clone();
        let root_clone = root.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let counter = counter.clone();
                let store = store.clone();
                let root = root_clone.clone();
                std::thread::spawn(move || {
                    let _ = handle_request(stream, counter, store, root);
                });
            }
        });
        HttpFixture {
            base: format!("http://{address}"),
            requests,
            root,
            files,
        }
    }

    fn request_count(&self) -> u64 {
        self.requests.load(Ordering::SeqCst)
    }
}

fn handle_request(
    mut stream: TcpStream,
    counter: Arc<AtomicU64>,
    store: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    root: PathBuf,
) -> std::io::Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = find_subsequence(&buffer, b"\r\n\r\n") {
            header_end = position + 4;
            break;
        }
    }
    let headers_text = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
    let mut lines = headers_text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");
    let content_length: usize = lines
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            if key.eq_ignore_ascii_case("content-length") {
                value.trim().parse().ok()
            } else {
                None
            }
        })
        .next()
        .unwrap_or(0);
    let has_if_none_match = headers_text
        .to_ascii_lowercase()
        .contains("if-none-match: *");
    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);

    counter.fetch_add(1, Ordering::SeqCst);
    let key = path.to_string();
    match method {
        "GET" => {
            let existing = store.lock().unwrap().get(&key).cloned();
            let bytes = existing.or_else(|| {
                let relative = path.trim_start_matches('/').replace("%20", " ");
                fs::read(root.join(relative)).ok()
            });
            match bytes {
                Some(bytes) => {
                    write_response(&mut stream, 200, "OK", &bytes)?;
                }
                None => {
                    write_response(&mut stream, 404, "Not Found", b"")?;
                }
            }
        }
        "PUT" => {
            let mut guard = store.lock().unwrap();
            if has_if_none_match && guard.contains_key(&key) {
                drop(guard);
                write_response(&mut stream, 412, "Precondition Failed", b"")?;
            } else {
                guard.insert(key, body);
                drop(guard);
                write_response(&mut stream, 201, "Created", b"")?;
            }
        }
        _ => {
            write_response(&mut stream, 405, "Method Not Allowed", b"")?;
        }
    }
    Ok(())
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[test]
fn pkg05_pkg14_http_repository_publish_and_consume() {
    let base = temp_dir("http");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let fixture = HttpFixture::start(base.join("repo-root"));
    let repo_url = fixture.base.clone();

    let project = base.join("mathlib");
    lib_project(
        &project,
        "org.example",
        "mathlib",
        "1.0.0",
        "pub fn answer(): i32 { return 42; }",
        &format!("\n[repositories]\ndefault = \"{repo_url}\"\n"),
    );
    assert_success(&run_dc(
        &project,
        &home,
        &["publish", "--repository", "default"],
    ));
    // 重复发布：条件 PUT 触发幂等成功。
    let again = run_dc(&project, &home, &["publish", "--repository", "default"]);
    assert_success(&again);
    assert!(stdout(&again).contains("already present"));

    let app = base.join("app");
    bin_project(
        &app,
        "org.example",
        "app",
        "use math;\nfn main() { return math.answer(); }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\nmath = \"org.example:mathlib:1.0.0\"\n"
        ),
    );
    assert_success(&run_dc(&app, &home, &["build"]));
    let status = Command::new(executable(&app, "app"))
        .status()
        .expect("app runs");
    assert_eq!(status.code(), Some(42));

    // 冷缓存 + --locked --offline：零网络请求。
    let before = fixture.request_count();
    let offline_home = base.join("offline-home");
    fs::create_dir_all(&offline_home).unwrap();
    let missing = run_dc(&app, &offline_home, &["build", "--locked", "--offline"]);
    assert_failure_contains(&missing, "cache");
    assert_eq!(
        fixture.request_count(),
        before,
        "offline mode must not issue HTTP requests"
    );

    let _ = &fixture.files;
    let _ = fixture.root;
    fs::remove_dir_all(base).ok();
}

// ---------------------------------------------------------------------------
// H18-07 PKGSRC-01..06：包来源身份与顺序无关性。
//
// 契约：同一 `(group, name)` 只允许一个精确版本和一个来源；Path 与 Remote 永远
// 不是同一来源，即使坐标和内容相同。这里用 file:// 仓库与隔离 `DOLPHIN_HOME`，
// 不访问真实用户仓库、不修改全局 `~/.dolphin`。依赖按别名排序遍历，因此别名
// 顺序决定先加载 Path 还是 Remote，测试据此实际确认遍历顺序。
// ---------------------------------------------------------------------------

fn count_occurrences(text: &str, needle: &str) -> usize {
    text.match_indices(needle).count()
}

/// 建立 H18-07 的菱形 fixture：`dup` 同时存在于 file 仓库和本地路径；`left`
/// 通过 Path 依赖 `dup`，`right` 通过 Remote 坐标依赖 `dup`。两个分支的请求链
/// 不同，可验证冲突诊断确实列出两条链。
fn write_h18_07_branches(base: &Path, home: &Path) {
    publish_library(base, home, "dup", "pub fn value(): i32 { return 42; }", "");
    lib_project(
        &base.join("left"),
        "org.example",
        "left",
        "1.0.0",
        "pub fn left(): i32 { return 0; }",
        "\n[dependencies]\ndup = { path = \"../dup\" }\n",
    );
    lib_project(
        &base.join("right"),
        "org.example",
        "right",
        "1.0.0",
        "pub fn right(): i32 { return 0; }",
        "\n[dependencies]\ndup = \"org.example:dup:1.0.0\"\n",
    );
}

/// PKGSRC-01：Path 分支先加载，随后 Remote 分支请求同坐标；必须拒绝来源冲突，
/// 且诊断列出 `left` 与 `right` 两条不同的请求链。
#[test]
fn h18_07_pkgsrc_01_path_before_remote_is_rejected() {
    let base = temp_dir("pkgsrc01");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let repo_url = file_url(&base.join("repo"));
    write_h18_07_branches(&base, &home);

    let app = base.join("app");
    bin_project(
        &app,
        "org.example",
        "app",
        "fn main() { return 0; }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\naaa = {{ path = \"../left\" }}\nzzz = {{ path = \"../right\" }}\n"
        ),
    );
    let output = run_dc(&app, &home, &["build"]);
    assert_failure_contains(&output, "source conflict");
    let diagnose = stderr(&output);
    assert!(
        diagnose.contains("already selected from path"),
        "PKGSRC-01 must load the path branch first, got: {diagnose}"
    );
    assert!(
        diagnose.contains("repository `default`"),
        "PKGSRC-01 must name the conflicting remote source, got: {diagnose}"
    );
    assert!(
        diagnose.contains("org.example:left:1.0.0 -> org.example:dup:1.0.0"),
        "PKGSRC-01 must show the path request chain, got: {diagnose}"
    );
    assert!(
        diagnose.contains("org.example:right:1.0.0"),
        "PKGSRC-01 must show the remote request chain, got: {diagnose}"
    );
    assert!(
        !app.join("dolphin.lock").is_file(),
        "a failed resolve must not write a lockfile"
    );

    fs::remove_dir_all(base).ok();
}

/// PKGSRC-02：Remote 分支先加载，随后 Path 分支请求同坐标；必须拒绝来源冲突，
/// 且诊断列出 `right` 与 `left` 两条不同的请求链。
#[test]
fn h18_07_pkgsrc_02_remote_before_path_is_rejected() {
    let base = temp_dir("pkgsrc02");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let repo_url = file_url(&base.join("repo"));
    write_h18_07_branches(&base, &home);

    // 交换别名指向：`aaa` 先命中 Remote 分支 `right`，`zzz` 后命中 Path 分支 `left`。
    let app = base.join("app");
    bin_project(
        &app,
        "org.example",
        "app",
        "fn main() { return 0; }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\naaa = {{ path = \"../right\" }}\nzzz = {{ path = \"../left\" }}\n"
        ),
    );
    let output = run_dc(&app, &home, &["build"]);
    assert_failure_contains(&output, "source conflict");
    let diagnose = stderr(&output);
    assert!(
        diagnose.contains("already selected from repository `default`"),
        "PKGSRC-02 must load the remote branch first, got: {diagnose}"
    );
    assert!(
        diagnose.contains("path `"),
        "PKGSRC-02 must name the conflicting path source, got: {diagnose}"
    );
    assert!(
        diagnose.contains("org.example:right:1.0.0 -> org.example:dup:1.0.0"),
        "PKGSRC-02 must show the remote request chain, got: {diagnose}"
    );
    assert!(
        diagnose.contains("org.example:left:1.0.0"),
        "PKGSRC-02 must show the path request chain, got: {diagnose}"
    );
    assert!(
        !app.join("dolphin.lock").is_file(),
        "a failed resolve must not write a lockfile"
    );

    fs::remove_dir_all(base).ok();
}

/// PKGSRC-03：同仓库同坐标经两个别名引用只解析一次（菱形复用）。
#[test]
fn h18_07_pkgsrc_03_same_repository_same_coordinate_is_reused() {
    let base = temp_dir("pkgsrc03");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let repo_url = file_url(&base.join("repo"));
    publish_library(
        &base,
        &home,
        "dup",
        "pub fn value(): i32 { return 42; }",
        "",
    );

    let app = base.join("app");
    bin_project(
        &app,
        "org.example",
        "app",
        "use aaa;\nfn main() { return aaa.value(); }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\naaa = \"org.example:dup:1.0.0\"\nzzz = \"org.example:dup:1.0.0\"\n"
        ),
    );
    assert_success(&run_dc(&app, &home, &["build"]));
    let status = Command::new(executable(&app, "app"))
        .status()
        .expect("app runs");
    assert_eq!(status.code(), Some(42));
    let lock = fs::read_to_string(app.join("dolphin.lock")).unwrap();
    assert_eq!(
        count_occurrences(&lock, "[[package]]"),
        1,
        "same repository/coordinate must resolve to one node: {lock}"
    );

    fs::remove_dir_all(base).ok();
}

/// PKGSRC-04：不同仓库或不同版本的同名包必须拒绝，并给出冲突诊断。
#[test]
fn h18_07_pkgsrc_04_different_repository_or_version_is_rejected() {
    let base = temp_dir("pkgsrc04");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let repo_url = file_url(&base.join("repo"));
    publish_library(
        &base,
        &home,
        "dup",
        "pub fn value(): i32 { return 42; }",
        "",
    );

    // 同坐标不同仓库 ID：即使两个 ID 指向同一基地址也不自动合并。
    let different_repo = base.join("different-repo");
    bin_project(
        &different_repo,
        "org.example",
        "otherrepo",
        "fn main() { return 0; }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\nother = \"{repo_url}\"\n\n[dependencies]\naaa = \"org.example:dup:1.0.0\"\nzzz = {{ coordinate = \"org.example:dup:1.0.0\", repository = \"other\" }}\n"
        ),
    );
    let output = run_dc(&different_repo, &home, &["build"]);
    assert_failure_contains(&output, "source conflict");
    assert!(
        stderr(&output).contains("repository `other`")
            && stderr(&output).contains("repository `default`"),
        "PKGSRC-04 must name both repositories, got: {}",
        stderr(&output)
    );

    // 同仓库不同精确版本：坐标不同，按版本冲突拒绝。
    let different_version = base.join("different-version");
    bin_project(
        &different_version,
        "org.example",
        "otherver",
        "fn main() { return 0; }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\naaa = \"org.example:dup:1.0.0\"\nzzz = \"org.example:dup:2.0.0\"\n"
        ),
    );
    let output = run_dc(&different_version, &home, &["build"]);
    assert_failure_contains(&output, "conflict");

    fs::remove_dir_all(base).ok();
}

/// PKGSRC-05：`--locked`/`--offline` 不绕过来源检查，失败不写出新的有效锁状态。
#[test]
fn h18_07_pkgsrc_05_locked_and_offline_do_not_bypass_source_check() {
    let base = temp_dir("pkgsrc05");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let repo_url = file_url(&base.join("repo"));
    publish_library(
        &base,
        &home,
        "dup",
        "pub fn value(): i32 { return 42; }",
        "",
    );

    let app = base.join("app");
    bin_project(
        &app,
        "org.example",
        "app",
        "use math;\nfn main() { return math.value(); }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\nmath = \"org.example:dup:1.0.0\"\n"
        ),
    );
    assert_success(&run_dc(&app, &home, &["build"]));
    let lock_path = app.join("dolphin.lock");
    let original_lock = fs::read(&lock_path).unwrap();

    // 加入同坐标的 path 别名：`local` 在别名序中先于 `math`，因此 Path 先加载。
    let manifest = fs::read_to_string(app.join("dolphin.toml")).unwrap();
    write(
        &app.join("dolphin.toml"),
        &manifest.replace(
            "math = \"org.example:dup:1.0.0\"",
            "math = \"org.example:dup:1.0.0\"\nlocal = { path = \"../dup\" }",
        ),
    );

    let locked = run_dc(&app, &home, &["build", "--locked"]);
    assert_failure_contains(&locked, "source conflict");
    assert_eq!(
        fs::read(&lock_path).unwrap(),
        original_lock,
        "`--locked` failure must not rewrite the lockfile"
    );

    let offline = run_dc(&app, &home, &["build", "--offline"]);
    assert_failure_contains(&offline, "source conflict");
    assert_eq!(
        fs::read(&lock_path).unwrap(),
        original_lock,
        "`--offline` failure must not rewrite the lockfile"
    );

    fs::remove_dir_all(base).ok();
}

/// PKGSRC-06：同一 canonical path 经不同别名引用仍复用为单个包。包依赖环的拒绝
/// 由既有 `pkg06_dependency_cycle_and_version_conflict_report_chains` 覆盖。
#[test]
fn h18_07_pkgsrc_06_same_canonical_path_aliases_are_reused() {
    let base = temp_dir("pkgsrc06");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let dup = base.join("dup");
    lib_project(
        &dup,
        "org.example",
        "dup",
        "1.0.0",
        "pub fn value(): i32 { return 42; }",
        "",
    );

    let app = base.join("app");
    bin_project(
        &app,
        "org.example",
        "app",
        "use aaa;\nfn main() { return aaa.value(); }",
        "\n[dependencies]\naaa = { path = \"../dup\" }\nzzz = { path = \"../dup/./\" }\n",
    );
    assert_success(&run_dc(&app, &home, &["build"]));
    let status = Command::new(executable(&app, "app"))
        .status()
        .expect("app runs");
    assert_eq!(status.code(), Some(42));
    let lock = fs::read_to_string(app.join("dolphin.lock")).unwrap();
    assert_eq!(
        count_occurrences(&lock, "[[package]]"),
        1,
        "aliases of the same canonical path must resolve to one node: {lock}"
    );

    fs::remove_dir_all(base).ok();
}

/// H18-08 BUILD-01：同时声明 `[lib]` 与两个 `[[bin]]` 的依赖，无论作为 path 依赖
/// 开发还是发布后的 `.dlib` 消费，都只加载库源码（bin `main` 不进入应用），且库与
/// bin 共享的 helper 仍可见。
#[test]
fn h18_08_build_01_path_and_published_dependency_agree() {
    let base = temp_dir("h18-08-build01");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let repo_url = file_url(&base.join("repo"));

    let dep = base.join("dep");
    lib_project(
        &dep,
        "org.example",
        "dep",
        "1.0.0",
        "pub fn value(): i32 { return helper() + 40; }",
        &format!(
            "\n[[bin]]\nname = \"tool\"\npath = \"src/tool.do\"\n\n[[bin]]\nname = \"tool2\"\npath = \"src/tool2.do\"\n\n[repositories]\ndefault = \"{repo_url}\"\n"
        ),
    );
    write(
        &dep.join("src/shared.do"),
        "pub fn helper(): i32 { return 2; }",
    );
    write(&dep.join("src/tool.do"), "fn main() { return 1; }");
    write(&dep.join("src/tool2.do"), "fn main() { return 2; }");
    assert_success(&run_dc(
        &dep,
        &home,
        &["publish", "--repository", "default"],
    ));

    // path 开发消费。
    let path_app = base.join("path-app");
    bin_project(
        &path_app,
        "org.example",
        "pathapp",
        "use dep;\nfn main() { return dep.value(); }",
        "\n[dependencies]\ndep = { path = \"../dep\" }\n",
    );
    assert_success(&run_dc(&path_app, &home, &["build"]));
    let path_status = Command::new(executable(&path_app, "pathapp"))
        .status()
        .expect("path app runs");
    assert_eq!(path_status.code(), Some(42));

    // 发布包消费（归档已排除 bin，只有 lib 与 helper 源码）。
    let coord_app = base.join("coord-app");
    bin_project(
        &coord_app,
        "org.example",
        "coordapp",
        "use dep;\nfn main() { return dep.value(); }",
        &format!(
            "\n[repositories]\ndefault = \"{repo_url}\"\n\n[dependencies]\ndep = \"org.example:dep:1.0.0\"\n"
        ),
    );
    assert_success(&run_dc(&coord_app, &home, &["build"]));
    let coord_status = Command::new(executable(&coord_app, "coordapp"))
        .status()
        .expect("coord app runs");
    assert_eq!(coord_status.code(), Some(42));

    fs::remove_dir_all(base).ok();
}
