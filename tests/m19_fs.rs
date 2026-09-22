//! H19-03 FS-01..06：文件打开、读写、截断/追加与句柄状态机。
//!
//! 每个用例在可用后端 × Dolphin Debug/Release 上构建，直接以 argv 传路径运行并断言
//! 固定 stdout/stderr/exit；Debug 下额外检查未关闭句柄报告（正常清理必须为空）。

mod support;

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const FS_READ_PROGRAM: &str = r#"
use std.fs.open;
use std.fs.OpenMode;
use std.io.stdin;
use std.process.arg;
use std.mem;

fn main(): i32 {
    val p = arg(1_usize);
    if p.is_err() {
        return 9;
    }
    val path = match p {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    val opened = open(path, OpenMode.Read);
    if opened.is_err() {
        return 1;
    }
    var stream = match opened {
        Result.Ok(value) => value,
        Result.Err(error) => stdin(),
    };
    defer stream.close_abort();
    val buffer = mem.alloc<u8>(4096_usize);
    defer mem.free(buffer);
    var total = 0_usize;
    var checksum = 0_usize;
    var running = true;
    while running {
        val result = stream.read(buffer);
        if result.is_err() {
            return 2;
        }
        val count = match result {
            Result.Ok(value) => value,
            Result.Err(error) => 0_usize,
        };
        if count == 0_usize {
            running = false;
        } else {
            var index = 0_usize;
            while index < count {
                checksum += buffer[index] as usize;
                index += 1_usize;
            }
            total += count;
        }
    }
    println("total={} checksum={} open={}", total, checksum, stream.is_open());
    return 0;
}
"#;

const FS_ERROR_PROGRAM: &str = r#"
use std.error.Error;
use std.error.ErrorKind;
use std.fs.open;
use std.fs.OpenMode;
use std.io.stdin;
use std.process.arg;

fn kind_label(error: Error): string {
    return match error.kind() {
        ErrorKind.NotFound => "not-found",
        ErrorKind.PermissionDenied => "permission",
        ErrorKind.IsADirectory => "is-dir",
        ErrorKind.InvalidArgument => "invalid",
        ErrorKind.NotOwned => "not-owned",
        ErrorKind.Closed => "closed",
        ErrorKind.Other => "other",
    };
}

fn attempt(path: string, mode: OpenMode): i32 {
    val opened = open(path, mode);
    if opened.is_err() {
        val error = match opened {
            Result.Ok(value) => Error::new(ErrorKind.Other, 0_i32),
            Result.Err(value) => value,
        };
        println("kind={} code={}", kind_label(error), error.code());
        return 3;
    }
    var stream = match opened {
        Result.Ok(value) => value,
        Result.Err(value) => stdin(),
    };
    val closed = stream.close();
    if closed.is_err() {
        return 4;
    }
    println("kind=ok");
    return 0;
}

fn main(): i32 {
    val p = arg(1_usize);
    val m = arg(2_usize);
    if p.is_err() || m.is_err() {
        return 9;
    }
    val path = match p {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    val mode_text = match m {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    if mode_text == "write" {
        return attempt(path, OpenMode.Write);
    }
    if mode_text == "append" {
        return attempt(path, OpenMode.Append);
    }
    return attempt(path, OpenMode.Read);
}
"#;

const FS_NUL_PROGRAM: &str = r#"
use std.error.Error;
use std.error.ErrorKind;
use std.fs.open;
use std.fs.OpenMode;

fn main(): i32 {
    val path = "bad\u{0}path";
    val opened = open(path, OpenMode.Read);
    if opened.is_err() {
        val error = match opened {
            Result.Ok(value) => Error::new(ErrorKind.Other, 0_i32),
            Result.Err(value) => value,
        };
        val label = match error.kind() {
            ErrorKind.InvalidArgument => "invalid",
            _ => "other",
        };
        println("kind={} code={}", label, error.code());
        return 0;
    }
    return 1;
}
"#;

const FS_WRITE_PROGRAM: &str = r#"
use std.fs.open;
use std.fs.OpenMode;
use std.io.stdin;
use std.process.arg;

fn write_with(path: string, mode: OpenMode, data_text: string): i32 {
    val opened = open(path, mode);
    if opened.is_err() {
        return 1;
    }
    var stream = match opened {
        Result.Ok(value) => value,
        Result.Err(value) => stdin(),
    };
    val payload = data_text.bytes();
    val written = stream.write_all(payload);
    val closed = stream.close();
    if written.is_err() || closed.is_err() {
        return 2;
    }
    println("write-ok");
    return 0;
}

fn main(): i32 {
    val p = arg(1_usize);
    val m = arg(2_usize);
    val d = arg(3_usize);
    if p.is_err() || m.is_err() || d.is_err() {
        return 9;
    }
    val path = match p {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    val mode_text = match m {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    val data_text = match d {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    if mode_text == "append" {
        return write_with(path, OpenMode.Append, data_text);
    }
    return write_with(path, OpenMode.Write, data_text);
}
"#;

const FS_MODE_PROGRAM: &str = r#"
use std.fs.open;
use std.fs.OpenMode;
use std.io.stdin;
use std.io.from_raw;
use std.process.arg;
use std.mem;

fn main(): i32 {
    val p = arg(1_usize);
    if p.is_err() {
        return 9;
    }
    val path = match p {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    val buffer = mem.alloc<u8>(16_usize);
    defer mem.free(buffer);

    val write_opened = open(path, OpenMode.Write);
    if write_opened.is_err() {
        return 1;
    }
    var write_stream = match write_opened {
        Result.Ok(value) => value,
        Result.Err(value) => stdin(),
    };
    val read_on_write = write_stream.read(buffer);
    val close_first = write_stream.close();
    val close_second = write_stream.close();

    val read_opened = open(path, OpenMode.Read);
    if read_opened.is_err() {
        return 2;
    }
    var read_stream = match read_opened {
        Result.Ok(value) => value,
        Result.Err(value) => stdin(),
    };
    val payload_text = "x";
    val write_on_read = read_stream.write(payload_text.bytes());
    read_stream.close();

    var invalid = from_raw(0_usize);
    val invalid_read = invalid.read(buffer);

    println("read-on-write-err={} write-on-read-err={} close1={} close2={} open-after={} invalid-open={} invalid-read-err={}",
        read_on_write.is_err(), write_on_read.is_err(), close_first.is_ok(), close_second.is_ok(),
        write_stream.is_open(), invalid.is_open(), invalid_read.is_err());
    return 0;
}
"#;

const FS_HANDOFF_PROGRAM: &str = r#"
use std.fs.open;
use std.fs.OpenMode;
use std.io.stdin;
use std.io.from_raw;
use std.process.arg;

fn main(): i32 {
    val p = arg(1_usize);
    if p.is_err() {
        return 9;
    }
    val path = match p {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    val opened = open(path, OpenMode.Read);
    if opened.is_err() {
        return 1;
    }
    var first = match opened {
        Result.Ok(value) => value,
        Result.Err(value) => stdin(),
    };
    defer first.close_abort();
    val raw = first.release();
    var second = from_raw(raw);
    defer second.close_abort();
    println("released={} first-open={} second-open={}", raw != 0_usize, first.is_open(), second.is_open());
    return 0;
}
"#;

const FS_LEAK_PROGRAM: &str = r#"
use std.fs.open;
use std.fs.OpenMode;
use std.io.stdin;
use std.process.arg;

fn main(): i32 {
    val p1 = arg(1_usize);
    val p2 = arg(2_usize);
    if p1.is_err() || p2.is_err() {
        return 9;
    }
    val path1 = match p1 {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    val path2 = match p2 {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    val opened1 = open(path1, OpenMode.Read);
    if opened1.is_err() {
        return 1;
    }
    var current = match opened1 {
        Result.Ok(value) => value,
        Result.Err(value) => stdin(),
    };
    defer current.close_abort();
    val opened2 = open(path2, OpenMode.Read);
    if opened2.is_err() {
        return 2;
    }
    current = match opened2 {
        Result.Ok(value) => value,
        Result.Err(value) => stdin(),
    };
    return 0;
}
"#;

struct Probe {
    root: PathBuf,
    home: PathBuf,
}

fn unique_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "dolphin-m19-fs-{tag}-{}-{unique}",
        std::process::id()
    ))
}

fn new_probe(tag: &str, source: &str) -> Probe {
    let root = unique_dir(tag);
    let home = root.join("home");
    fs::create_dir_all(root.join("src")).expect("project src");
    fs::create_dir_all(&home).expect("isolated home");
    fs::write(
        root.join("dolphin.toml"),
        "[package]\ngroup = \"org.example\"\nname = \"m19fs\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"m19fs\"\npath = \"src/main.do\"\n",
    )
    .expect("manifest");
    fs::write(root.join("src/main.do"), source).expect("source");
    Probe { root, home }
}

impl Probe {
    fn executable(&self) -> PathBuf {
        let path = self.root.join("target").join("m19fs");
        if cfg!(windows) {
            path.with_extension("exe")
        } else {
            path
        }
    }

    fn fixture(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).expect("fixture dir");
        path
    }

    fn build(&self, backend: &str, release: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dc"));
        command
            .env("DOLPHIN_HOME", &self.home)
            .arg("build")
            .arg(&self.root)
            .args(["--backend", backend]);
        if release {
            command.arg("--release");
        }
        command.output().expect("dc build should run")
    }

    fn run(&self, args: &[OsString]) -> Output {
        Command::new(self.executable())
            .args(args)
            .output()
            .expect("program should run")
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn pattern_byte(index: usize) -> u8 {
    (index % 251) as u8
}

fn pattern_checksum(length: usize) -> usize {
    (0..length).map(|index| pattern_byte(index) as usize).sum()
}

fn bytes_checksum(bytes: &[u8]) -> usize {
    bytes.iter().map(|byte| *byte as usize).sum()
}

fn assert_built(probe: &Probe, backend: &str, release: bool) {
    let build = probe.build(backend, release);
    assert!(
        build.status.success(),
        "build failed backend={backend} release={release}: {}",
        stderr_text(&build)
    );
}

/// 供各用例复用的组合遍历：断言成功运行和固定 stdout/stderr。
fn run_cases(probe: &Probe, args: &[OsString]) -> Vec<(String, bool, Output)> {
    let mut outputs = Vec::new();
    for backend in support::backends() {
        for release in [false, true] {
            assert_built(probe, backend.name(), release);
            let output = probe.run(args);
            assert_eq!(
                output.status.code(),
                Some(0),
                "backend={} release={release} stderr={}",
                backend.name(),
                stderr_text(&output)
            );
            outputs.push((backend.name().to_string(), release, output));
        }
    }
    outputs
}

/// FS-01：空文件、小文件、多块文件的循环读取与固定字节数/校验和。
#[test]
fn fs_01_empty_small_multi_chunk() {
    let probe = new_probe("fs01", FS_READ_PROGRAM);
    let empty = probe.fixture("data/empty.txt");
    fs::write(&empty, b"").unwrap();
    let small = probe.fixture("data/small.txt");
    fs::write(&small, b"hello\n").unwrap();
    let large = probe.fixture("data/large.bin");
    let length = 100 * 1024 + 37;
    let large_bytes: Vec<u8> = (0..length).map(pattern_byte).collect();
    fs::write(&large, &large_bytes).unwrap();

    for (path, expected) in [
        (empty, "total=0 checksum=0 open=true\n".to_string()),
        (
            small.clone(),
            format!(
                "total=6 checksum={} open=true\n",
                bytes_checksum(b"hello\n")
            ),
        ),
        (
            large,
            format!(
                "total={length} checksum={} open=true\n",
                pattern_checksum(length)
            ),
        ),
    ] {
        let args = vec![path.into_os_string()];
        for (backend, release, output) in run_cases(&probe, &args) {
            assert_eq!(
                stdout_text(&output),
                expected,
                "backend={backend} release={release}"
            );
            assert!(
                output.stderr.is_empty(),
                "backend={backend} release={release} stderr={}",
                stderr_text(&output)
            );
        }
    }
}

/// FS-02：不存在、目录、含 NUL 路径的稳定错误类别。
#[test]
fn fs_02_missing_dir_and_bad_path() {
    let probe = new_probe("fs02", FS_ERROR_PROGRAM);
    let existing = probe.fixture("data/file.txt");
    fs::write(&existing, b"x").unwrap();

    for backend in support::backends() {
        for release in [false, true] {
            assert_built(&probe, backend.name(), release);

            // 不存在的文件：NotFound。
            let missing = probe.root.join("data/no-such-file.txt");
            let output = probe.run(&[missing.into_os_string(), OsString::from("read")]);
            assert_eq!(output.status.code(), Some(3));
            assert!(
                stdout_text(&output).contains("kind=not-found"),
                "backend={} release={release}: {}",
                backend.name(),
                stdout_text(&output)
            );

            // 目录：Unix IsADirectory，Windows InvalidArgument（规格 §5.1）。
            let dir = probe.root.join("data");
            for mode in ["read", "write", "append"] {
                let output = probe.run(&[dir.clone().into_os_string(), mode.into()]);
                assert_eq!(output.status.code(), Some(3));
                let stdout = stdout_text(&output);
                let expected = if cfg!(windows) {
                    "kind=invalid"
                } else {
                    "kind=is-dir"
                };
                assert!(
                    stdout.contains(expected),
                    "mode={mode} backend={} release={release}: {stdout}",
                    backend.name()
                );
            }

            // 合法文件成功路径：显式关闭后退出，Debug 无未关闭句柄报告。
            let output = probe.run(&[existing.clone().into_os_string(), OsString::from("read")]);
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(stdout_text(&output), "kind=ok\n");
            assert!(
                output.stderr.is_empty(),
                "backend={} release={release} stderr={}",
                backend.name(),
                stderr_text(&output)
            );
        }
    }

    // 含内部 NUL 的路径在调用运行时前被拒绝。
    let nul = new_probe("fs02-nul", FS_NUL_PROGRAM);
    for (backend, release, output) in run_cases(&nul, &[]) {
        assert_eq!(
            stdout_text(&output),
            "kind=invalid code=0\n",
            "backend={backend} release={release}"
        );
    }
}

/// FS-03：受控读写失败（模式不匹配、/dev/full）、幂等关闭与 invalid 句柄。
#[test]
fn fs_03_injected_read_write_close_failure() {
    let probe = new_probe("fs03", FS_MODE_PROGRAM);
    let file = probe.fixture("data/mode.txt");
    fs::write(&file, b"0123456789").unwrap();
    let expected = "read-on-write-err=true write-on-read-err=true close1=true close2=true open-after=false invalid-open=false invalid-read-err=true\n";
    for (backend, release, output) in run_cases(&probe, &[file.clone().into_os_string()]) {
        assert_eq!(
            stdout_text(&output),
            expected,
            "backend={backend} release={release}"
        );
        assert!(
            output.stderr.is_empty(),
            "backend={backend} release={release} stderr={}",
            stderr_text(&output)
        );
    }

    // Linux：写入 /dev/full 必须返回 Err（exit 2），而不是 trap。
    #[cfg(target_os = "linux")]
    {
        let full = new_probe("fs03-full", FS_WRITE_PROGRAM);
        for backend in support::backends() {
            for release in [false, true] {
                assert_built(&full, backend.name(), release);
                let output = full.run(&[
                    OsString::from("/dev/full"),
                    OsString::from("write"),
                    OsString::from("0123456789"),
                ]);
                assert_eq!(
                    output.status.code(),
                    Some(2),
                    "backend={} release={release} stderr={}",
                    backend.name(),
                    stderr_text(&output)
                );
            }
        }
    }
}

/// FS-04：含空格与 Unicode 的完整路径可写可读。
#[test]
fn fs_04_space_and_unicode_path() {
    let probe = new_probe("fs04", FS_READ_PROGRAM);
    let unicode = probe.fixture("数据 dir/文件 name.txt");
    let payload = "héllo 世界\n";
    fs::write(&unicode, payload.as_bytes()).unwrap();

    let expected = format!(
        "total={} checksum={} open=true\n",
        payload.len(),
        bytes_checksum(payload.as_bytes())
    );
    for (backend, release, output) in run_cases(&probe, &[unicode.into_os_string()]) {
        assert_eq!(
            stdout_text(&output),
            expected,
            "backend={backend} release={release}"
        );
    }
}

/// FS-05：`Write` 截断既有文件，`Append` 追加；`Append` 可创建新文件。
#[test]
fn fs_05_truncate_and_append() {
    let probe = new_probe("fs05", FS_WRITE_PROGRAM);
    let target = probe.fixture("data/target.txt");
    fs::write(&target, b"AAAAAAAAAA").unwrap();
    let fresh = probe.root.join("data/fresh.txt");
    let _ = fs::remove_file(&fresh);

    for backend in support::backends() {
        for release in [false, true] {
            assert_built(&probe, backend.name(), release);

            let output = probe.run(&[
                target.clone().into_os_string(),
                OsString::from("write"),
                OsString::from("xy"),
            ]);
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(stdout_text(&output), "write-ok\n");
            assert_eq!(fs::read(&target).unwrap(), b"xy");

            let output = probe.run(&[
                target.clone().into_os_string(),
                OsString::from("append"),
                OsString::from("z"),
            ]);
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(fs::read(&target).unwrap(), b"xyz");

            let _ = fs::remove_file(&fresh);
            let output = probe.run(&[
                fresh.clone().into_os_string(),
                OsString::from("append"),
                OsString::from("new"),
            ]);
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(fs::read(&fresh).unwrap(), b"new");
        }
    }
}

/// FS-06：显式关闭、重绑定与显式转交遵循状态机；Debug 报告未关闭句柄。
#[test]
fn fs_06_state_machine_close_rebind_handoff() {
    // 显式转交：release + from_raw，Debug 无未关闭句柄。
    let handoff = new_probe("fs06-handoff", FS_HANDOFF_PROGRAM);
    let file = handoff.fixture("data/handoff.txt");
    fs::write(&file, b"x").unwrap();
    for (backend, release, output) in run_cases(&handoff, &[file.clone().into_os_string()]) {
        assert_eq!(
            stdout_text(&output),
            "released=true first-open=false second-open=true\n",
            "backend={backend} release={release}"
        );
        assert!(
            output.stderr.is_empty(),
            "backend={backend} release={release} stderr={}",
            stderr_text(&output)
        );
    }

    // 未关闭就重绑定：Debug 报告 1 个未关闭句柄，Release 不报告；退出码不变。
    let leak = new_probe("fs06-leak", FS_LEAK_PROGRAM);
    let first = leak.fixture("data/first.txt");
    let second = leak.fixture("data/second.txt");
    fs::write(&first, b"a").unwrap();
    fs::write(&second, b"b").unwrap();
    for backend in support::backends() {
        for release in [false, true] {
            assert_built(&leak, backend.name(), release);
            let output = leak.run(&[
                first.clone().into_os_string(),
                second.clone().into_os_string(),
            ]);
            assert_eq!(
                output.status.code(),
                Some(0),
                "backend={} release={release}",
                backend.name()
            );
            if release {
                assert!(
                    output.stderr.is_empty(),
                    "Release must not report handles (backend={}): {}",
                    backend.name(),
                    stderr_text(&output)
                );
            } else {
                assert!(
                    stderr_text(&output).contains("open handle(s) not closed at exit"),
                    "Debug must report the leaked handle (backend={}): {}",
                    backend.name(),
                    stderr_text(&output)
                );
            }
        }
    }
}
