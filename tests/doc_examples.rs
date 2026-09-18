//! H18-10：当前文档与网站示例的抽取、分类和编译核正。
//!
//! 机制（轻量，不引入 Node 或额外构建生态）：
//! - 从覆盖的当前文档抽取代码块：Markdown 的 ` ```dc ` 块和网站内容里的
//!   `<pre><code>` 块；候选是含 `fn main` 的完整程序或以 `// src/` 标注的多文件块。
//! - 每个候选必须在 `CLASSIFICATION` 中有明确类型（成功程序 / 只构建 / 预期诊断 /
//!   片段 / 多文件工程 / 历史草案 / 未来语法），否则测试失败——完整示例不允许无类型。
//! - `Run`/`Build`/`Reject` 与 `ProjectRun` 用真实 `dc` 编译/运行并断言固定结果；
//!   英文网站内容为准，中文内容只做代码一致性检查（注释除外），避免重复编译。
//! - 历史提案（proposal-m14/m15、plan-m14-m15-rework）保留原始路径表，不在此列。
//!
//! 维护约定：编辑被覆盖的示例后，`doc_examples_are_classified` 会因首行提示不符而
//! 失败，提醒更新分类与期望，而不是让文档悄悄漂移。

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const MARKDOWN_FILES: &[&str] = &["README.md", "docs/implemented-features.md"];

const WEBSITE_FILES: &[&str] = &[
    "docs/website/assets/js/content/en-tutorial.js",
    "docs/website/assets/js/content/zh-tutorial.js",
    "docs/website/assets/js/content/en-std.js",
    "docs/website/assets/js/content/zh-std.js",
];

/// 当前文档：相对链接必须可达（历史提案的旧路径表已明确标注，不在此列）。
const LINK_CHECK_FILES: &[&str] = &[
    "README.md",
    "docs/implemented-features.md",
    "docs/language-design.md",
    "docs/installation.md",
    "docs/roadmap.md",
    "docs/compiler-implementation.md",
    "docs/plan-m18-plus.md",
    "docs/plan-m18-correctness.md",
    "examples/README.md",
];

#[derive(Clone, Copy)]
#[allow(dead_code)] // 分类是机制的一部分；当前覆盖文件还没有用到全部分类。
enum Class {
    /// 成功程序：构建并运行，断言 stdout、exit，且 stderr 为空（Debug 无泄漏）。
    Run { stdout: &'static str, exit: i32 },
    /// 成功程序：只构建，不运行。
    Build,
    /// 预期诊断程序：构建失败且诊断包含关键字。
    Reject { contains: &'static str },
    /// 片段：不是可独立编译的完整程序，附原因。
    Fragment(&'static str),
    /// 多文件工程示例：由 `examples/` 与集成测试显式映射覆盖，附原因。
    Project(&'static str),
    /// 历史草案：保留展示，不代表当前行为。
    Historical(&'static str),
    /// 未来语法/未实现能力：正文已标注。
    Future(&'static str),
    /// 多文件工程的首块：组装同文件连续的 `// src/` 块后构建并运行。
    ProjectRun { stdout: &'static str, exit: i32 },
    /// 多文件工程的后续块：与首块一起编译，不单独执行。
    ProjectPart,
}

/// `(文件, 候选序号, 首行提示, 分类)`；候选序号按文件内出现顺序从 0 开始。
const CLASSIFICATION: &[(&str, usize, &str, Class)] = &[
    (
        "README.md",
        0,
        "// src/main.do",
        Class::ProjectRun {
            stdout: "min = 3\n",
            exit: 10,
        },
    ),
    (
        "README.md",
        1,
        "// src/mathutil/math.do",
        Class::ProjectPart,
    ),
    (
        "README.md",
        2,
        "fn sum(values: [i32; 4]): i32 {",
        Class::Run {
            stdout: "Dolphin, total = 18, value = 2.25, symbol = 海\n",
            exit: 0,
        },
    ),
    (
        "docs/implemented-features.md",
        0,
        "fn add(a: i32, b: i32): i32 {",
        Class::Run {
            stdout: "",
            exit: 3,
        },
    ),
    (
        "docs/implemented-features.md",
        1,
        "use mathutil.math;",
        Class::Project(
            "展示 `use` 形态；mathutil 模块由 examples/m7 与 tests/build.rs::builds_and_runs_m7_modules 覆盖",
        ),
    ),
    (
        "docs/implemented-features.md",
        2,
        "use mathutil.math.min;",
        Class::Project(
            "展示 `use` 形态；mathutil 模块由 examples/m7 与 tests/build.rs::builds_and_runs_m7_modules 覆盖",
        ),
    ),
    (
        "docs/implemented-features.md",
        3,
        "use mathutil;",
        Class::Project(
            "展示 `use` 形态；mathutil 模块由 examples/m7 与 tests/build.rs::builds_and_runs_m7_modules 覆盖",
        ),
    ),
    (
        "docs/implemented-features.md",
        4,
        "struct Point {",
        Class::Run {
            stdout: "point = (3, 4)\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        0,
        "fn main() {",
        Class::Run {
            stdout: "Hello, Dolphin!\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        1,
        "fn main(): i32 {",
        Class::Run {
            stdout: "",
            exit: 42,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        2,
        "struct Point {",
        Class::Run {
            stdout: "distance^2 = 25\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        3,
        "// src/mathutil/math.do",
        Class::ProjectRun {
            stdout: "min = 3\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        4,
        "// src/main.do",
        Class::ProjectPart,
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        5,
        "fn main() {",
        Class::Run {
            stdout: "Hello\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        6,
        "fn main(): i32 {",
        Class::Run {
            stdout: "",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        7,
        "fn is_prime(n: i32): bool {",
        Class::Run {
            stdout: "2 3 5 7 11 13 17 19 23 29 31 37 41 43 47 \nfound 15 primes\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        8,
        "struct Point {",
        Class::Run {
            stdout: "point = (3, 4)\nx = 10\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        9,
        "// src/geom/shapes.do",
        Class::ProjectRun {
            stdout: "p = (3, 4)\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        10,
        "// src/main.do",
        Class::ProjectPart,
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        11,
        "enum Token {",
        Class::Run {
            stdout: "result = 13\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        12,
        "// src/mathutil/math.do",
        Class::ProjectRun {
            stdout: "min = 3\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        13,
        "// src/main.do",
        Class::ProjectPart,
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        14,
        "fn identity<T>(value: T): T {",
        Class::Run {
            stdout: "20 22\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        15,
        "struct Pair<T> {",
        Class::Run {
            stdout: "3\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        16,
        "struct Counter {",
        Class::Run {
            stdout: "count = 2\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        17,
        "use std.collections.Vec;",
        Class::Run {
            stdout: "total = 6\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        18,
        "use std.text;",
        Class::Run {
            stdout: "Dolphin!\nhe\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        19,
        "use std.mem;",
        Class::Run {
            stdout: "count = 3\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        20,
        "// src/report/stats.do",
        Class::ProjectRun {
            stdout: "total = 255, average = 85\nbest = 95\n",
            exit: 85,
        },
    ),
    (
        "docs/website/assets/js/content/en-tutorial.js",
        21,
        "// src/main.do",
        Class::ProjectPart,
    ),
    // 中文内容与英文逐块对应；代码一致性由 website_translations_are_in_sync 校验，
    // 这里只登记分类，不重复执行。
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        0,
        "fn main() {",
        Class::Run {
            stdout: "Hello, Dolphin!\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        1,
        "fn main(): i32 {",
        Class::Run {
            stdout: "",
            exit: 42,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        2,
        "struct Point {",
        Class::Run {
            stdout: "distance^2 = 25\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        3,
        "// src/mathutil/math.do",
        Class::ProjectRun {
            stdout: "min = 3\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        4,
        "// src/main.do",
        Class::ProjectPart,
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        5,
        "fn main() {",
        Class::Run {
            stdout: "Hello\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        6,
        "fn main(): i32 {",
        Class::Run {
            stdout: "",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        7,
        "fn is_prime(n: i32): bool {",
        Class::Run {
            stdout: "2 3 5 7 11 13 17 19 23 29 31 37 41 43 47 \nfound 15 primes\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        8,
        "struct Point {",
        Class::Run {
            stdout: "point = (3, 4)\nx = 10\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        9,
        "// src/geom/shapes.do",
        Class::ProjectRun {
            stdout: "p = (3, 4)\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        10,
        "// src/main.do",
        Class::ProjectPart,
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        11,
        "enum Token {",
        Class::Run {
            stdout: "result = 13\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        12,
        "// src/mathutil/math.do",
        Class::ProjectRun {
            stdout: "min = 3\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        13,
        "// src/main.do",
        Class::ProjectPart,
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        14,
        "fn identity<T>(value: T): T {",
        Class::Run {
            stdout: "20 22\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        15,
        "struct Pair<T> {",
        Class::Run {
            stdout: "3\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        16,
        "struct Counter {",
        Class::Run {
            stdout: "count = 2\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        17,
        "use std.collections.Vec;",
        Class::Run {
            stdout: "total = 6\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        18,
        "use std.text;",
        Class::Run {
            stdout: "Dolphin!\nhe\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        19,
        "use std.mem;",
        Class::Run {
            stdout: "count = 3\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        20,
        "// src/report/stats.do",
        Class::ProjectRun {
            stdout: "total = 255, average = 85\nbest = 95\n",
            exit: 85,
        },
    ),
    (
        "docs/website/assets/js/content/zh-tutorial.js",
        21,
        "// src/main.do",
        Class::ProjectPart,
    ),
    (
        "docs/website/assets/js/content/en-std.js",
        0,
        "use std.collections.Vec;",
        Class::Run {
            stdout: "len = 1\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-std.js",
        1,
        "struct Countdown {",
        Class::Run {
            stdout: "3\n2\n1\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-std.js",
        2,
        "use std.collections.Vec;",
        Class::Run {
            stdout: "has first\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-std.js",
        3,
        "use std.mem;",
        Class::Run {
            stdout: "len = 4\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-std.js",
        4,
        "use std.collections.Vec;",
        Class::Run {
            stdout: "grew as expected\npopped = 30\ntotal = 30\ncopy len = 2\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-std.js",
        5,
        "use std.text;",
        Class::Run {
            stdout: "Dolphin\nlen = 7\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-std.js",
        6,
        "use std.text;",
        Class::Run {
            stdout: "Dolphin!\ntrue\ntrue\ntrue\ndolp\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/en-std.js",
        7,
        "use std.ffi.CString;",
        Class::Run {
            stdout: "7\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-std.js",
        0,
        "use std.collections.Vec;",
        Class::Run {
            stdout: "len = 1\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-std.js",
        1,
        "struct Countdown {",
        Class::Run {
            stdout: "3\n2\n1\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-std.js",
        2,
        "use std.collections.Vec;",
        Class::Run {
            stdout: "has first\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-std.js",
        3,
        "use std.mem;",
        Class::Run {
            stdout: "len = 4\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-std.js",
        4,
        "use std.collections.Vec;",
        Class::Run {
            stdout: "grew as expected\npopped = 30\ntotal = 30\ncopy len = 2\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-std.js",
        5,
        "use std.text;",
        Class::Run {
            stdout: "Dolphin\nlen = 7\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-std.js",
        6,
        "use std.text;",
        Class::Run {
            stdout: "Dolphin!\ntrue\ntrue\ntrue\ndolp\n",
            exit: 0,
        },
    ),
    (
        "docs/website/assets/js/content/zh-std.js",
        7,
        "use std.ffi.CString;",
        Class::Run {
            stdout: "7\n",
            exit: 0,
        },
    ),
];

struct Block {
    text: String,
    first_line: String,
    is_project_file: bool,
}

fn extract_markdown(path: &str) -> Vec<Block> {
    let text = fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
    let mut blocks = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() != "```dc" {
            continue;
        }
        let mut body = Vec::new();
        for body_line in lines.by_ref() {
            if body_line.trim() == "```" {
                break;
            }
            body.push(body_line);
        }
        blocks.push(body.join("\n"));
    }
    candidates(&blocks)
}

fn extract_website(path: &str) -> Vec<Block> {
    let text = fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
    let mut blocks = Vec::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("<pre><code>") {
        let after = &rest[start + "<pre><code>".len()..];
        let Some(end) = after.find("</code></pre>") else {
            break;
        };
        blocks.push(unescape_html(&after[..end]));
        rest = &after[end + "</code></pre>".len()..];
    }
    candidates(&blocks)
}

fn unescape_html(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn candidates(blocks: &[String]) -> Vec<Block> {
    let mut out = Vec::new();
    for text in blocks {
        let first_line = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("")
            .to_string();
        let is_project_file = first_line.starts_with("// src/");
        if !text.contains("fn main") && !is_project_file {
            continue;
        }
        out.push(Block {
            text: text.clone(),
            first_line,
            is_project_file,
        });
    }
    out
}

fn classification(file: &str, index: usize) -> Option<Class> {
    CLASSIFICATION
        .iter()
        .find(|(entry_file, entry_index, _, _)| *entry_file == file && *entry_index == index)
        .map(|(_, _, _, class)| *class)
}

fn hint(file: &str, index: usize) -> Option<&'static str> {
    CLASSIFICATION
        .iter()
        .find(|(entry_file, entry_index, _, _)| *entry_file == file && *entry_index == index)
        .map(|(_, _, hint, _)| *hint)
}

fn covered_files() -> Vec<&'static str> {
    MARKDOWN_FILES
        .iter()
        .chain(WEBSITE_FILES.iter())
        .copied()
        .collect()
}

fn extract(file: &str) -> Vec<Block> {
    if file.ends_with(".js") {
        extract_website(file)
    } else {
        extract_markdown(file)
    }
}

/// 完整示例必须全部有明确类型；分类提示与文档首行不一致立即失败。
#[test]
fn doc_examples_are_classified() {
    let mut problems = Vec::new();
    for file in covered_files() {
        for (index, block) in extract(file).iter().enumerate() {
            match (classification(file, index), hint(file, index)) {
                (None, _) => problems.push(format!(
                    "unclassified complete example: {file} [{index}] `{}`",
                    block.first_line
                )),
                (Some(_), Some(expected)) if expected != block.first_line => problems.push(format!(
                    "classification is stale: {file} [{index}] expected first line `{expected}`, found `{}`",
                    block.first_line
                )),
                _ => {}
            }
        }
    }
    assert!(
        problems.is_empty(),
        "documentation examples changed; update CLASSIFICATION and expectations:\n{}",
        problems.join("\n")
    );
}

fn temp_project(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("dolphin-doc-{tag}-{}-{unique}", std::process::id()));
    fs::create_dir_all(dir.join("src")).expect("project directory");
    dir
}

fn write_manifest(project: &Path, name: &str) {
    fs::write(
        project.join("dolphin.toml"),
        format!(
            "[package]\ngroup = \"org.example\"\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"{name}\"\npath = \"src/main.do\"\n"
        ),
    )
    .expect("manifest");
}

fn write_project(project: &Path, files: &[(String, String)]) {
    for (relative, source) in files {
        let path = project.join(relative);
        fs::create_dir_all(path.parent().unwrap()).expect("source directory");
        fs::write(path, source).expect("source file");
    }
}

/// 把 `// src/...` 块转成 `(相对路径, 源码)`。
fn project_files(blocks: &[&Block]) -> Vec<(String, String)> {
    blocks
        .iter()
        .map(|block| {
            let mut lines = block.text.lines();
            let header = lines.next().unwrap_or("").trim();
            let relative = header
                .strip_prefix("// ")
                .unwrap_or(header)
                .trim()
                .to_string();
            (relative, lines.collect::<Vec<_>>().join("\n") + "\n")
        })
        .collect()
}

fn run_dc(project: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_dc"))
        .args(arguments)
        .arg(project)
        .output()
        .expect("dc should run")
}

fn assert_run(file: &str, index: usize, project: &Path, expected_stdout: &str, expected_exit: i32) {
    for backend in support::backends() {
        let output = run_dc(project, &["run", "--backend", backend.name()]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "{file} [{index}] backend={} exit; stderr={stderr}",
            backend.name()
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected_stdout,
            "{file} [{index}] backend={} stdout",
            backend.name()
        );
        assert!(
            output.stderr.is_empty(),
            "{file} [{index}] backend={} stderr must be empty: {stderr}",
            backend.name()
        );
    }
}

#[test]
fn doc_examples_build_and_run() {
    for file in covered_files() {
        // 中文网站内容与英文逐块一致；以英文为执行基准，避免重复编译。
        if file.contains("/zh-") {
            continue;
        }
        let blocks = extract(file);
        for (index, block) in blocks.iter().enumerate() {
            match classification(file, index) {
                Some(Class::Run { stdout, exit }) => {
                    let project = temp_project(&format!("run-{index}"));
                    write_manifest(&project, "docapp");
                    write_project(&project, &[("src/main.do".to_string(), block.text.clone())]);
                    assert_run(file, index, &project, stdout, exit);
                    fs::remove_dir_all(&project).ok();
                }
                Some(Class::Build) => {
                    let project = temp_project(&format!("build-{index}"));
                    write_manifest(&project, "docapp");
                    write_project(&project, &[("src/main.do".to_string(), block.text.clone())]);
                    let output = run_dc(&project, &["build"]);
                    assert!(
                        output.status.success(),
                        "{file} [{index}] must build: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    fs::remove_dir_all(&project).ok();
                }
                Some(Class::Reject { contains }) => {
                    let project = temp_project(&format!("reject-{index}"));
                    write_manifest(&project, "docapp");
                    write_project(&project, &[("src/main.do".to_string(), block.text.clone())]);
                    let output = run_dc(&project, &["build"]);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    assert!(
                        !output.status.success(),
                        "{file} [{index}] must be rejected"
                    );
                    assert!(
                        stderr.contains(contains),
                        "{file} [{index}] diagnostic must contain `{contains}`: {stderr}"
                    );
                    fs::remove_dir_all(&project).ok();
                }
                Some(Class::ProjectRun { stdout, exit }) => {
                    // 该文件内连续的 `// src/` 块组成一个工程。
                    let group: Vec<&Block> = blocks[index..]
                        .iter()
                        .take_while(|candidate| candidate.is_project_file)
                        .collect();
                    let project = temp_project(&format!("project-{index}"));
                    write_manifest(&project, "docapp");
                    write_project(&project, &project_files(&group));
                    assert_run(file, index, &project, stdout, exit);
                    fs::remove_dir_all(&project).ok();
                }
                Some(Class::ProjectPart) => {}
                Some(
                    Class::Fragment(_)
                    | Class::Project(_)
                    | Class::Historical(_)
                    | Class::Future(_),
                ) => {}
                None => panic!("{file} [{index}] must be classified"),
            }
        }
    }
}

/// 去掉行注释与空白后比较，检查中文/英文网站代码块逐对一致（DOC-03）。
#[test]
fn website_translations_are_in_sync() {
    let pairs = [
        (
            "docs/website/assets/js/content/en-tutorial.js",
            "docs/website/assets/js/content/zh-tutorial.js",
        ),
        (
            "docs/website/assets/js/content/en-std.js",
            "docs/website/assets/js/content/zh-std.js",
        ),
    ];
    for (english, chinese) in pairs {
        let en = extract(english);
        let zh = extract(chinese);
        assert_eq!(
            en.len(),
            zh.len(),
            "{english} and {chinese} must contain the same number of examples"
        );
        for (index, (left, right)) in en.iter().zip(zh.iter()).enumerate() {
            assert_eq!(
                strip_comments(&left.text),
                strip_comments(&right.text),
                "{english} and {chinese} example [{index}] code differs (comments ignored)"
            );
        }
    }
}

fn strip_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| match line.split_once("//") {
            Some((code, _)) => code.trim_end().to_string(),
            None => line.trim_end().to_string(),
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 当前文档的相对文件链接必须存在（DOC-01）；历史提案的旧路径表另行标注。
#[test]
fn current_doc_links_exist() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut dead = Vec::new();
    for file in LINK_CHECK_FILES {
        let path = root.join(file);
        let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {file}: {error}"));
        let mut rest = text.as_str();
        while let Some(start) = rest.find("](") {
            let after = &rest[start + 2..];
            let Some(end) = after.find(')') else { break };
            let target = after[..end].trim();
            rest = &after[end + 1..];
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
                || target.starts_with('#')
                || target.starts_with('<')
            {
                continue;
            }
            let file_part = target.split('#').next().unwrap_or("");
            if file_part.is_empty() {
                continue;
            }
            let resolved = path.parent().unwrap_or(root).join(file_part);
            if !resolved.exists() {
                let line = text[..start].lines().count();
                dead.push(format!("{file}:{line} -> {target}"));
            }
        }
    }
    assert!(dead.is_empty(), "dead relative links:\n{}", dead.join("\n"));
}

/// DOC-04：已修复缺陷不再停留在“当前已知问题”，且修复表链接了回归测试。
#[test]
fn fixed_defects_moved_to_history() {
    let text = fs::read_to_string("docs/implemented-features.md").expect("read features doc");
    let current = text
        .split("### 1.2 已修复缺陷")
        .next()
        .expect("document has a known-issue section");
    let known_issues = current
        .split("### 1.1 当前已知问题与验证边界")
        .nth(1)
        .expect("document has a known-issue section");
    assert!(
        !known_issues.contains("| 后续批次 |"),
        "known-issue table must not still enumerate fixed defects:\n{known_issues}"
    );
    let history = text
        .split("### 1.2 已修复缺陷")
        .nth(1)
        .expect("document has a fixed-defect section");
    for batch in [
        "H18-01", "H18-02", "H18-03", "H18-04", "H18-05", "H18-07", "H18-08", "H18-09",
    ] {
        assert!(
            history.contains(batch),
            "fixed-defect table must record {batch}"
        );
    }
    assert!(
        history.contains("tests/"),
        "fixed-defect table must link regression tests"
    );
}
