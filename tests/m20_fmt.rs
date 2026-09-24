//! H20-04 Formatter 保持性与项目发现验收（FMT-01..06）。
//!
//! FMT-01/02 直接在仓库示例与源码标准库上验证 `dolphin-format` 的幂等与
//! token/注释保持性；FMT-06 把 M1-M19 示例复制到临时目录，先制造可确定的非规范
//! 输入（行尾空白）再要求 `dc fmt` 恢复原文，然后实际构建/运行/自测，断言固定
//! stdout/stderr/exit。FMT-03/04/05 通过真实 `dc fmt` 子进程断言 D-M20-4 冻结的
//! 清单发现、递归排除、全有或全无与 `--check` 零写入。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_source::lexer::lex;
use dolphin_source::source::SourceFile;
use dolphin_source::token::TokenKind;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dolphin-m20-fmt-{tag}-{}-{unique}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn run_dc(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dc"))
        .env("DOLPHIN_HOME", home)
        .args(args)
        .output()
        .expect("dc should run")
}

fn run_dc_in(cwd: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dc"))
        .env("DOLPHIN_HOME", home)
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("dc should run")
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn write_project(project: &Path, files: &[(&str, &str)]) {
    for (relative, source) in files {
        let path = project.join(relative);
        fs::create_dir_all(path.parent().expect("project file has a parent")).expect("dirs");
        fs::write(&path, source).expect("source file");
    }
}

/// 递归收集 `.do` 文件，跳过构建输出与 `.git`（测试自身的文件遍历，不经过 dc）。
fn collect_do_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let name = entry.file_name();
            if name == ".git" || name == "target" {
                continue;
            }
            collect_do_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "do") {
            out.push(path);
        }
    }
}

fn repository_sources() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = Vec::new();
    collect_do_files(&root.join("examples"), &mut files);
    collect_do_files(&root.join("crates/dolphin-std/src"), &mut files);
    files.sort();
    files
}

fn formatted(text: &str) -> String {
    dolphin_format::format_source(text).expect("source must format")
}

fn token_kinds(path: &Path, text: &str) -> Vec<TokenKind> {
    let source = SourceFile::new(path.to_path_buf(), text.to_string());
    lex(&source)
        .unwrap_or_else(|error| panic!("{} must lex: {error}", path.display()))
        .into_iter()
        .map(|token| token.kind)
        .collect()
}

/// 提取注释文本：`//` 到行尾、`/* */` 内容；逐行去掉缩进与行尾空白。
fn comment_texts(source: &str) -> Vec<(bool, String)> {
    fn normalize(text: &str) -> String {
        text.lines().map(str::trim).collect::<Vec<_>>().join("\n")
    }

    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' {
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                    continue;
                }
                if chars[i] == c {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            i += 2;
            let start = i;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            out.push((true, normalize(&chars[start..i].iter().collect::<String>())));
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            let start = i;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            let end = i.min(chars.len());
            out.push((
                false,
                normalize(&chars[start..end].iter().collect::<String>()),
            ));
            i = (i + 2).min(chars.len());
            continue;
        }
        i += 1;
    }
    out
}

fn assert_token_preservation(path: &Path, text: &str) {
    let formatted_text = formatted(text);
    assert_eq!(
        token_kinds(path, text),
        token_kinds(path, &formatted_text),
        "{}: non-trivia token sequence changed",
        path.display()
    );
    assert_eq!(
        comment_texts(text),
        comment_texts(&formatted_text),
        "{}: comment texts changed",
        path.display()
    );
}

/// 覆盖泛型、`impl`、`defer`、`extern "C"`、字符串内 `{}`/`//`、字符字面量与块注释。
const FIXTURES: &[(&str, &str)] = &[
    (
        "generics_and_methods.do",
        "struct Pair<T> {\nfirst: T,\nsecond: T,\n}\n\nimpl<T> Pair<T> {\nfn swapped(self): Pair<T> {\nreturn Pair<T> {\nfirst: self.second,\nsecond: self.first,\n};\n}\n}\n\nfn identity<T>(value: T): T {\nreturn value;\n}\n\nfn main() {\nval pair = Pair<i32>(1, 2);\nval other = pair.swapped();\nreturn identity(other.first);\n}\n",
    ),
    (
        "ffi_strings_comments.do",
        "use std.mem;\n\nextern \"C\" {\n    pub fn demo_add(a: i32, b: i32): i32;\n}\n\nfn main() {\n    // 行注释里的 } 与 { 不改变缩进\n    val url = \"http://x/{}\";\n    val open = '{';\n    val close = '}';\n    val quote = '\\'';\n    val bytes = mem.alloc<u8>(4);\n    defer mem.free(bytes);\n    /* 块注释\n       } 不配对 */\n    println(\"{} {} {} {} {}\", url, open, close, quote, demo_add(20, 22));\n    return 0;\n}\n",
    ),
    (
        "trait_and_match.do",
        "use std.collections.Vec;\n\ntrait Head {\n    type Item;\n    fn head(self): Self::Item;\n}\n\nenum Maybe<T> {\n    Just(T),\n    Nothing,\n}\n\nfn main() {\n    var numbers = Vec<i32>::init();\n    defer numbers.deinit();\n    numbers.push(1);\n    val maybe = Maybe.Just(7);\n    val value = match maybe {\n        Maybe.Just(inner) => inner,\n        Maybe.Nothing => 0,\n    };\n    return value;\n}\n",
    ),
    (
        "crlf_and_blank_lines.do",
        "fn main() {\r\n    val x = 1;\r\n\r\n\r\n    return x;\r\n}\r\n",
    ),
];

/// FMT-01：全部示例/fixture 二次格式化完全一致。
#[test]
fn fmt_01_idempotent_on_examples_and_fixtures() {
    let files = repository_sources();
    assert!(
        files.len() >= 30,
        "expected repository sources under examples/ and dolphin-std, found {}",
        files.len()
    );
    for file in &files {
        let text = fs::read_to_string(file).expect("read source");
        let once = formatted(&text);
        let twice = formatted(&once);
        assert_eq!(once, twice, "{} is not idempotent", file.display());
    }
    for (name, text) in FIXTURES {
        let once = formatted(text);
        let twice = formatted(&once);
        assert_eq!(once, twice, "fixture {name} is not idempotent");
    }
}

/// FMT-02：非 trivia token 序列与注释不变。
#[test]
fn fmt_02_token_preservation_and_comments() {
    for file in repository_sources() {
        let text = fs::read_to_string(&file).expect("read source");
        assert_token_preservation(&file, &text);
    }
    for (name, text) in FIXTURES {
        assert_token_preservation(Path::new(name), text);
    }
}

/// FMT-03：任一文件格式化失败时本次不写任何文件（全有或全无）。
#[test]
fn fmt_03_error_file_not_written_all_or_nothing() {
    let base = temp_dir("fmt03");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let project = base.join("project");
    write_project(
        &project,
        &[
            (
                "dolphin.toml",
                "[package]\ngroup = \"org.example\"\nname = \"fmtapp\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"fmtapp\"\npath = \"src/main.do\"\n",
            ),
            // 按路径排序 a/m 在 z 之前：修复前会先写前两个再在坏文件上报错。
            ("src/a_good.do", "fn main() {\nval x = 1;\n}\n"),
            ("src/m_good.do", "fn helper() {\nval y = 2;\n}\n"),
            ("src/z_bad.do", "fn broken() {\nval s = \"oops\n}\n"),
        ],
    );
    let before = snapshot_tree(&project);

    let output = run_dc(&home, &["fmt", project.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout={} stderr={}",
        stdout_text(&output),
        stderr_text(&output)
    );
    assert!(
        stderr_text(&output).contains("unterminated string literal"),
        "stderr must name the formatting failure: {}",
        stderr_text(&output)
    );
    assert!(
        !stdout_text(&output).contains("formatted"),
        "no file may be written: {}",
        stdout_text(&output)
    );
    assert_eq!(
        snapshot_tree(&project),
        before,
        "all-or-nothing: every file must stay untouched"
    );

    let check = run_dc(&home, &["fmt", "--check", project.to_str().unwrap()]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr_text(&check));
    assert_eq!(
        snapshot_tree(&project),
        before,
        "--check must not write on failure either"
    );

    fs::remove_dir_all(base).ok();
}

/// FMT-04：`--check` 零写入、需要格式化时非零退出。
#[test]
fn fmt_04_check_writes_nothing() {
    let base = temp_dir("fmt04");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let project = base.join("project");
    write_project(
        &project,
        &[
            (
                "dolphin.toml",
                "[package]\ngroup = \"org.example\"\nname = \"fmtapp\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"fmtapp\"\npath = \"src/main.do\"\n",
            ),
            ("src/main.do", "fn main() {\nval x = 1;\n}\n"),
        ],
    );
    let file = project.join("src/main.do");
    let original = fs::read_to_string(&file).unwrap();

    let check = run_dc(&home, &["fmt", "--check", project.to_str().unwrap()]);
    assert_eq!(
        check.status.code(),
        Some(1),
        "stdout={} stderr={}",
        stdout_text(&check),
        stderr_text(&check)
    );
    assert_eq!(
        stdout_text(&check),
        format!("would reformat {}\n", file.display())
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), original);

    let write = run_dc(&home, &["fmt", project.to_str().unwrap()]);
    assert_eq!(write.status.code(), Some(0), "{}", stderr_text(&write));
    assert_eq!(
        stdout_text(&write),
        format!("formatted {}\n", file.display())
    );
    let canonical = fs::read_to_string(&file).unwrap();
    assert_eq!(canonical, "fn main() {\n    val x = 1;\n}\n");

    let clean = run_dc(&home, &["fmt", "--check", project.to_str().unwrap()]);
    assert_eq!(clean.status.code(), Some(0), "{}", stderr_text(&clean));
    assert_eq!(stdout_text(&clean), "");
    assert_eq!(fs::read_to_string(&file).unwrap(), canonical);

    let again = run_dc(&home, &["fmt", project.to_str().unwrap()]);
    assert_eq!(again.status.code(), Some(0), "{}", stderr_text(&again));
    assert_eq!(stdout_text(&again), "");
    assert_eq!(fs::read_to_string(&file).unwrap(), canonical);

    fs::remove_dir_all(base).ok();
}

/// FMT-05：清单 `[package].source` 发现、排除 `build.output`/`.git`、CRLF→LF。
#[test]
fn fmt_05_project_discovery_and_crlf() {
    let base = temp_dir("fmt05");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let project = base.join("project");
    write_project(
        &project,
        &[
            (
                "dolphin.toml",
                "[package]\ngroup = \"org.example\"\nname = \"fmtapp\"\nversion = \"0.1.0\"\nsource = \"code\"\n\n[[bin]]\nname = \"fmtapp\"\npath = \"code/main.do\"\n\n[build]\noutput = \"out\"\n",
            ),
            ("code/main.do", "fn main() {\r\nval x = 1;\r\n}\r\n"),
            ("code/nested/helper.do", "fn helper() {\nreturn 1;\n}\n"),
            ("out/generated.do", "fn generated() {\nval y = 2;\n}\n"),
            (".git/hook.do", "fn hook() {\nval z = 3;\n}\n"),
        ],
    );
    let generated = project.join("out/generated.do");
    let hook = project.join(".git/hook.do");
    let generated_before = fs::read_to_string(&generated).unwrap();
    let hook_before = fs::read_to_string(&hook).unwrap();

    // 无路径参数：从 cwd 发现清单，根为 `[package].source`（code），不是 src。
    let output = run_dc_in(&project, &home, &["fmt"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        stdout_text(&output),
        stderr_text(&output)
    );
    let main = fs::read_to_string(project.join("code/main.do")).unwrap();
    assert_eq!(main, "fn main() {\n    val x = 1;\n}\n");
    assert!(!main.contains('\r'), "CRLF must be rewritten to LF");
    assert_eq!(
        fs::read_to_string(project.join("code/nested/helper.do")).unwrap(),
        "fn helper() {\n    return 1;\n}\n"
    );
    assert_eq!(fs::read_to_string(&generated).unwrap(), generated_before);
    assert_eq!(fs::read_to_string(&hook).unwrap(), hook_before);
    assert!(
        !project.join("dolphin.lock").exists(),
        "dc fmt must not write dolphin.lock"
    );

    // 显式目录：同样递归排除 build.output 与 .git。
    let output = run_dc(&home, &["fmt", project.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        stdout_text(&output),
        stderr_text(&output)
    );
    assert_eq!(fs::read_to_string(&generated).unwrap(), generated_before);
    assert_eq!(fs::read_to_string(&hook).unwrap(), hook_before);

    // 显式单文件位于构建输出目录时仍精确格式化。
    let output = run_dc(&home, &["fmt", generated.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr_text(&output));
    assert_eq!(
        fs::read_to_string(&generated).unwrap(),
        "fn generated() {\n    val y = 2;\n}\n"
    );

    // 目录符号链接不被跟随（防环）；若跟随会无限递归。
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&project, project.join("code/loop")).unwrap();
        let output = run_dc(&home, &["fmt", project.to_str().unwrap()]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "directory symlink must be skipped: {}",
            stderr_text(&output)
        );
        assert_eq!(fs::read_to_string(&hook).unwrap(), hook_before);
    }

    fs::remove_dir_all(base).ok();
}

struct Example {
    name: &'static str,
    /// 显式 Dolphin profile：`--debug` 或 `--release`。
    profile: &'static str,
    run_args: &'static [&'static str],
    stdout: &'static str,
    exit: i32,
}

/// M1-M19 示例的固定运行结果（来自各自 README 与既有集成测试）。
const EXAMPLES: &[Example] = &[
    Example {
        name: "m1",
        profile: "--debug",
        run_args: &[],
        stdout: "",
        exit: 28,
    },
    Example {
        name: "m2",
        profile: "--debug",
        run_args: &[],
        stdout: "",
        exit: 42,
    },
    Example {
        name: "m3",
        profile: "--debug",
        run_args: &[],
        stdout: "",
        exit: 28,
    },
    Example {
        name: "m4",
        profile: "--debug",
        run_args: &[],
        stdout: "",
        exit: 120,
    },
    Example {
        name: "m5",
        profile: "--debug",
        run_args: &[],
        stdout: "Hello, 海豚!\ncount = 3, enabled = true\nstate = ready\nescaped braces: {}\nminimum i32 = -2147483648\n",
        exit: 0,
    },
    Example {
        name: "m6",
        profile: "--debug",
        run_args: &[],
        stdout: "numbers = 1, 10, 8, 4\ntotal = 43\n",
        exit: 43,
    },
    Example {
        name: "m7",
        profile: "--debug",
        run_args: &[],
        stdout: "min = 3, clamp = 10\n",
        exit: 13,
    },
    Example {
        name: "m8",
        profile: "--debug",
        run_args: &[],
        stdout: "signed = -8, 1600, 64000\nunsigned = 8, 1600, 32000, 64000\nfloat = 1.5, 2.25\nchar = 海, samples = 10, 20, 30\nstring equal = true, bytes = 6\ncast result = 64000\n",
        exit: 64,
    },
    Example {
        name: "m9",
        profile: "--debug",
        run_args: &["--bin", "cli"],
        stdout: "21 doubled is 42\n-5 is negative\n",
        exit: 42,
    },
    Example {
        name: "m9",
        profile: "--debug",
        run_args: &["--bin", "server"],
        stdout: "server total = 26\n",
        exit: 26,
    },
    Example {
        name: "m13",
        profile: "--debug",
        run_args: &[],
        stdout: "point = (3, 4)\ncircle area = 12.56\nparsed value = 42\nfallback = -1\n",
        exit: 49,
    },
    Example {
        name: "m14",
        profile: "--debug",
        run_args: &[],
        stdout: "records=3 total=4.5 text=true\n",
        exit: 0,
    },
    Example {
        name: "m15",
        profile: "--debug",
        run_args: &[],
        stdout: "42 22 true\n",
        exit: 0,
    },
    Example {
        // README 使用 Release；同时覆盖显式 Release 构建/运行。
        name: "m16",
        profile: "--release",
        run_args: &[],
        stdout: "sum = 1799999937, fib = 9227465\n",
        exit: 0,
    },
    Example {
        name: "m18",
        profile: "--debug",
        run_args: &[],
        stdout: "alias = 7 9\ngeneric = 40\nshifted = 8 10\nsum = 12\n",
        exit: 0,
    },
];

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("destination dir");
    for entry in fs::read_dir(source).expect("read source dir") {
        let entry = entry.expect("source entry");
        let name = entry.file_name();
        if name == "target" || name == ".git" {
            continue;
        }
        let target = destination.join(&name);
        let file_type = entry.file_type().expect("file type");
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target);
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// 给每个非空行追加行尾空白，作为可确定、token 等价的非规范输入。
fn add_trailing_whitespace(text: &str) -> String {
    let mut out = String::new();
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(line);
        if !line.is_empty() {
            out.push_str("  ");
        }
    }
    out
}

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if entry.file_type().expect("file type").is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).expect("under root").to_path_buf(),
                    fs::read(&path).expect("read file"),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// FMT-06：格式化后的 M1-M19 示例仍可构建/自测，固定结果不变。
#[test]
fn fmt_06_m1_m19_examples_behavior_preserved() {
    let base = temp_dir("fmt06");
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let examples_root = repo_root().join("examples");

    for example in EXAMPLES {
        let copy = base.join(format!("{}-copy", example.name));
        copy_tree(&examples_root.join(example.name), &copy);

        // 先写入行尾空白，再要求 dc fmt 恢复仓库原文（同时验证格式化确实发生）。
        let mut canonical = Vec::new();
        for file in {
            let mut files = Vec::new();
            collect_do_files(&copy, &mut files);
            files.sort();
            files
        } {
            let original = fs::read_to_string(&file).unwrap();
            fs::write(&file, add_trailing_whitespace(&original)).unwrap();
            canonical.push((file, original));
        }
        assert!(!canonical.is_empty(), "{} has no sources", example.name);

        let format = run_dc(&home, &["fmt", copy.to_str().unwrap()]);
        assert_eq!(
            format.status.code(),
            Some(0),
            "{} fmt: stdout={} stderr={}",
            example.name,
            stdout_text(&format),
            stderr_text(&format)
        );
        for (file, original) in &canonical {
            assert_eq!(
                &fs::read_to_string(file).unwrap(),
                original,
                "{}: formatter must restore canonical text for {}",
                example.name,
                file.display()
            );
        }

        let mut args = vec!["run", copy.to_str().unwrap(), example.profile];
        args.extend_from_slice(example.run_args);
        let run = run_dc(&home, &args);
        assert_eq!(
            run.status.code(),
            Some(example.exit),
            "{} run: stdout={} stderr={}",
            example.name,
            stdout_text(&run),
            stderr_text(&run)
        );
        assert_eq!(stdout_text(&run), example.stdout, "{} stdout", example.name);
        assert_eq!(
            stderr_text(&run),
            "",
            "{} stderr must be empty",
            example.name
        );
    }

    // M19：格式化后两个包的自测仍全过。
    let m19 = base.join("m19-copy");
    copy_tree(&examples_root.join("m19"), &m19);
    let format = run_dc(&home, &["fmt", m19.to_str().unwrap()]);
    assert_eq!(format.status.code(), Some(0), "{}", stderr_text(&format));
    for (package, expected) in [
        (
            "textstats",
            concat!(
                "test test_byte_count ... ok\n",
                "test test_filter_rules ... ok\n",
                "test test_invalid_utf8 ... ok\n",
                "test test_line_rules ... ok\n",
                "test test_unicode_filter ... ok\n",
                "5 passed; 0 failed; 0 filtered out\n",
            ),
        ),
        (
            "dtext",
            concat!(
                "test test_dependency_analyze ... ok\n",
                "test test_number_formatting ... ok\n",
                "test test_parse_filter_and_path ... ok\n",
                "test test_parse_help ... ok\n",
                "test test_parse_usage_errors ... ok\n",
                "5 passed; 0 failed; 0 filtered out\n",
            ),
        ),
    ] {
        let package_dir = m19.join(package);
        let test = run_dc(&home, &["test", package_dir.to_str().unwrap(), "--debug"]);
        assert_eq!(
            test.status.code(),
            Some(0),
            "m19/{package}: stdout={} stderr={}",
            stdout_text(&test),
            stderr_text(&test)
        );
        assert_eq!(stdout_text(&test), expected, "m19/{package} summary");
        assert_eq!(stderr_text(&test), "", "m19/{package} stderr");
    }

    fs::remove_dir_all(base).ok();
}
