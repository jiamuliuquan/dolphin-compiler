//! H19-02 IO-01..04：标准流与字节 I/O。
//!
//! 每个用例在可用后端 × Dolphin Debug/Release 上构建可执行文件，然后以受控的
//! stdin/stdout/stderr 直接运行并断言固定 stdout/stderr/exit；Debug 下额外要求
//! stderr 无泄漏报告（断言包含预期输出的方式由各用例说明）。

mod support;

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const IO_STDIO_PROGRAM: &str = r#"
use std.io.stdin;
use std.io.stdout;
use std.io.stderr;
use std.mem;

fn main(): i32 {
    val input = stdin();
    var output = stdout();
    val errors = stderr();
    val buffer = mem.alloc<u8>(64_usize);
    defer mem.free(buffer);

    val read_result = input.read(buffer);
    val count = match read_result {
        Result.Ok(value) => value,
        Result.Err(error) => 999_usize,
    };

    val out_text = "stdout-marker\n";
    val out_bytes = out_text.bytes();
    val wrote = match output.write_all(out_bytes) {
        Result.Ok(value) => value,
        Result.Err(error) => false,
    };
    val err_text = "stderr-marker\n";
    val err_bytes = err_text.bytes();
    val wrote_err = match errors.write_all(err_bytes) {
        Result.Ok(value) => value,
        Result.Err(error) => false,
    };
    val flushed = match output.flush() {
        Result.Ok(value) => value,
        Result.Err(error) => false,
    };
    // 借用标准流不可关闭：close 必须失败且不影响后续写入。
    val closed = match output.close() {
        Result.Ok(value) => value,
        Result.Err(error) => false,
    };
    val after_text = "after-close\n";
    val after_bytes = after_text.bytes();
    val wrote_after = match output.write_all(after_bytes) {
        Result.Ok(value) => value,
        Result.Err(error) => false,
    };

    println(
        "count={} wrote={} err={} flush={} closed={} after={}",
        count,
        wrote,
        wrote_err,
        flushed,
        closed,
        wrote_after
    );
    return 0;
}
"#;

const IO_READ_PROGRAM: &str = r#"
use std.io.stdin;
use std.mem;

fn main(): i32 {
    val input = stdin();
    val buffer = mem.alloc<u8>(4096_usize);
    defer mem.free(buffer);

    var total = 0_usize;
    var checksum = 0_usize;
    var running = true;
    while running {
        val result = input.read(buffer);
        if result.is_err() {
            return 1;
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
    println("total={} checksum={}", total, checksum);
    return 0;
}
"#;

const IO_WRITE_PROGRAM: &str = r#"
use std.io.stdout;
use std.mem;

fn main(): i32 {
    val output = stdout();
    val size = 262144_usize;
    val buffer = mem.alloc<u8>(size);
    defer mem.free(buffer);

    var index = 0_usize;
    while index < size {
        buffer[index] = (index % 251_usize) as u8;
        index += 1_usize;
    }
    val view = buffer.slice(0_usize, size);
    val result = output.write_all(view);
    if result.is_err() {
        return 1;
    }
    return 0;
}
"#;

const IO_WRITE_FAILURE_PROGRAM: &str = r#"
use std.io.stdout;
use std.io.eprint;
use std.mem;

fn main(): i32 {
    val output = stdout();
    val size = 8192_usize;
    val buffer = mem.alloc<u8>(size);
    defer mem.free(buffer);

    var index = 0_usize;
    while index < size {
        buffer[index] = (index % 251_usize) as u8;
        index += 1_usize;
    }
    val view = buffer.slice(0_usize, size);
    val result = output.write_all(view);
    if result.is_err() {
        val message = "write-error\n";
        eprint(message.bytes());
        return 3;
    }
    return 0;
}
"#;

const IO_UTF8_PROGRAM: &str = r#"
use std.io.stdin;
use std.text.from_utf8;
use std.mem;

fn main(): i32 {
    val input = stdin();
    val buffer = mem.alloc<u8>(64_usize);
    defer mem.free(buffer);
    val result = input.read(buffer);
    val count = match result {
        Result.Ok(value) => value,
        Result.Err(error) => 0_usize,
    };
    val view = buffer.slice(0_usize, count);
    val parsed = from_utf8(view);
    if parsed.is_err() {
        println("utf8=err");
    } else {
        println("utf8=ok");
    }
    return 0;
}
"#;

const IO_TRAP_PROGRAM: &str = r#"
use std.io.stdin;
use std.mem;

fn main(): i32 {
    val input = stdin();
    val buffer = mem.alloc<u8>(64_usize);
    val result = input.read(buffer);
    val count = match result {
        Result.Ok(value) => value,
        Result.Err(error) => 0_usize,
    };
    val view = buffer.slice(0_usize, count);
    val parsed = string.from_bytes(view);
    println("len={}", length(parsed));
    mem.free(buffer);
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
        "dolphin-m19-io-{tag}-{}-{unique}",
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
        "[package]\ngroup = \"org.example\"\nname = \"m19io\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"m19io\"\npath = \"src/main.do\"\n",
    )
    .expect("manifest");
    fs::write(root.join("src/main.do"), source).expect("source");
    Probe { root, home }
}

impl Probe {
    fn executable(&self) -> PathBuf {
        let path = self.root.join("target").join("m19io");
        if cfg!(windows) {
            path.with_extension("exe")
        } else {
            path
        }
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

    /// 以给定 stdin 直接运行程序并捕获三路结果。
    fn run_with_stdin(&self, input: &[u8]) -> Output {
        let mut child = Command::new(self.executable())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("program should start");
        let mut stdin = child.stdin.take().expect("stdin should be piped");
        stdin.write_all(input).expect("stdin should accept input");
        drop(stdin);
        child.wait_with_output().expect("program should finish")
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

/// 构建并运行一次；断言 exit/stdout/stderr 完全等于给定期望。
fn assert_full_run(
    probe: &Probe,
    backend: &str,
    release: bool,
    input: &[u8],
    expected_stdout: &str,
    expected_stderr: &str,
    expected_exit: i32,
) {
    let build = probe.build(backend, release);
    assert!(
        build.status.success(),
        "build failed backend={backend} release={release}: {}",
        stderr_text(&build)
    );
    let output = probe.run_with_stdin(input);
    let context = format!("backend={backend} release={release}");
    assert_eq!(
        output.status.code(),
        Some(expected_exit),
        "{context} exit; stderr={}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), expected_stdout, "{context} stdout");
    assert_eq!(stderr_text(&output), expected_stderr, "{context} stderr");
}

/// IO-01：重定向 stdin/stdout/stderr；借用标准流不可关闭且不影响后续写入。
#[test]
fn io_01_redirect_stdio() {
    let probe = new_probe("io01", IO_STDIO_PROGRAM);
    for backend in support::backends() {
        for release in [false, true] {
            let build = probe.build(backend.name(), release);
            assert!(
                build.status.success(),
                "build failed backend={} release={release}: {}",
                backend.name(),
                stderr_text(&build)
            );
            let output = probe.run_with_stdin(b"hello");
            assert_eq!(
                output.status.code(),
                Some(0),
                "backend={} release={release} stderr={}",
                backend.name(),
                stderr_text(&output)
            );
            assert_eq!(
                stdout_text(&output),
                "stdout-marker\nafter-close\ncount=5 wrote=true err=true flush=true closed=false after=true\n",
                "backend={} release={release}",
                backend.name()
            );
            assert_eq!(
                stderr_text(&output),
                "stderr-marker\n",
                "backend={} release={release}",
                backend.name()
            );
        }
    }
}

/// IO-02：空输入立即 EOF；多块输入循环读到 EOF，字节数与校验和固定。
#[test]
fn io_02_empty_and_chunked_read() {
    let probe = new_probe("io02", IO_READ_PROGRAM);
    for backend in support::backends() {
        for release in [false, true] {
            assert_full_run(
                &probe,
                backend.name(),
                release,
                b"",
                "total=0 checksum=0\n",
                "",
                0,
            );

            let length = 100 * 1024 + 37;
            let input: Vec<u8> = (0..length).map(pattern_byte).collect();
            let expected = format!("total={length} checksum={}\n", pattern_checksum(length));
            assert_full_run(&probe, backend.name(), release, &input, &expected, "", 0);
        }
    }
}

/// IO-03：`write_all` 写完整段大输出；Linux 上用 `/dev/full` 验证失败可恢复。
#[test]
fn io_03_partial_write_and_failure() {
    let probe = new_probe("io03", IO_WRITE_PROGRAM);
    let length = 262144;
    for backend in support::backends() {
        for release in [false, true] {
            let build = probe.build(backend.name(), release);
            assert!(
                build.status.success(),
                "build failed backend={} release={release}: {}",
                backend.name(),
                stderr_text(&build)
            );
            let output = probe.run_with_stdin(b"");
            assert_eq!(
                output.status.code(),
                Some(0),
                "backend={} release={release} stderr={}",
                backend.name(),
                stderr_text(&output)
            );
            assert!(
                output.stderr.is_empty(),
                "backend={} release={release}",
                backend.name()
            );
            assert_eq!(
                output.stdout.len(),
                length,
                "backend={} release={release}",
                backend.name()
            );
            let checksum: usize = output.stdout.iter().map(|byte| *byte as usize).sum();
            assert_eq!(
                checksum,
                pattern_checksum(length),
                "backend={} release={release}",
                backend.name()
            );
        }
    }

    // 失败 fixture：stdout 指向 /dev/full，write 失败必须返回 Err 而不是 trap。
    #[cfg(target_os = "linux")]
    {
        let failure = new_probe("io03-failure", IO_WRITE_FAILURE_PROGRAM);
        for backend in support::backends() {
            for release in [false, true] {
                let build = failure.build(backend.name(), release);
                assert!(
                    build.status.success(),
                    "build failed backend={} release={release}: {}",
                    backend.name(),
                    stderr_text(&build)
                );
                let full = fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .expect("/dev/full should open");
                let output = Command::new(failure.executable())
                    .stdin(Stdio::null())
                    .stdout(Stdio::from(full))
                    .stderr(Stdio::piped())
                    .output()
                    .expect("failed write fixture should run");
                assert_eq!(
                    output.status.code(),
                    Some(3),
                    "backend={} release={release} stderr={}",
                    backend.name(),
                    stderr_text(&output)
                );
                assert!(
                    stderr_text(&output).contains("write-error"),
                    "backend={} release={release} stderr={}",
                    backend.name(),
                    stderr_text(&output)
                );
            }
        }
    }
}

/// IO-04：非法 UTF-8 字节可读为 bytes，但 `from_utf8` 返回错误；
/// 无校验的 `string.from_bytes` 保持 104 trap。
#[test]
fn io_04_invalid_utf8_bytes_not_string() {
    let probe = new_probe("io04", IO_UTF8_PROGRAM);
    for backend in support::backends() {
        for release in [false, true] {
            assert_full_run(
                &probe,
                backend.name(),
                release,
                "héllo".as_bytes(),
                "utf8=ok\n",
                "",
                0,
            );
            assert_full_run(
                &probe,
                backend.name(),
                release,
                &[0x61, 0xff, 0xfe, 0x62],
                "utf8=err\n",
                "",
                0,
            );
        }
    }

    let trap_probe = new_probe("io04-trap", IO_TRAP_PROGRAM);
    for backend in support::backends() {
        for release in [false, true] {
            let build = trap_probe.build(backend.name(), release);
            assert!(
                build.status.success(),
                "build failed backend={} release={release}: {}",
                backend.name(),
                stderr_text(&build)
            );
            let output = trap_probe.run_with_stdin(&[0x61, 0xff, 0x62]);
            assert_eq!(
                output.status.code(),
                Some(104),
                "backend={} release={release} stderr={}",
                backend.name(),
                stderr_text(&output)
            );
            assert!(
                stderr_text(&output).contains("invalid UTF-8"),
                "backend={} release={release} stderr={}",
                backend.name(),
                stderr_text(&output)
            );
        }
    }
}
