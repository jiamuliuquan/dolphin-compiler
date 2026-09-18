//! M14-E C 互操作与原生链接的端到端测试（FFI-01–06）。
//!
//! 测试用系统 C 工具链编译真实 C fixture（Unix 用 `cc`，Windows 用 MSVC `cl`），
//! 再通过 `dolphin.toml` 的 `[native.<triple>]` 声明链接输入，验证 C ABI 调用、
//! extern struct 布局、错误诊断与静态/共享库集成。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_compiler::{BuildProfile, BuildSettings, build_manifest, host_platform, load_manifest};

static NEXT_PROJECT_ID: AtomicU64 = AtomicU64::new(0);

struct Project {
    root: PathBuf,
    triple: String,
    bin: String,
}

impl Project {
    fn create(files: &[(&str, &str)]) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolphin-ffi-test-{}-{unique}-{}",
            std::process::id(),
            NEXT_PROJECT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("project directory should be created");
        for (relative, contents) in files {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).expect("parent directory should exist");
            fs::write(path, contents).expect("fixture file should be written");
        }
        let triple = host_platform()
            .expect("host platform should be supported")
            .triple()
            .to_string();
        Self {
            root,
            triple,
            bin: "app".to_string(),
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// 写入 `dolphin.toml`，其中 `[native.<triple>]` 使用给定字段。
    fn write_manifest(&self, native: &str) {
        let contents = format!(
            r#"[package]
group = "me.foxlab"
name = "ffiapp"
version = "0.1.0"

[[bin]]
name = "{}"
path = "src/main.do"

[native.{}]
{native}
"#,
            self.bin, self.triple
        );
        fs::write(self.path("dolphin.toml"), contents).expect("manifest should be written");
    }

    fn build(&self) -> Result<PathBuf, dolphin_compiler::Diagnostic> {
        let manifest = load_manifest(&self.root)?;
        let artifacts = build_manifest(
            &manifest,
            Some(&self.bin),
            BuildProfile::Debug,
            BuildSettings::default(),
        )?;
        let artifact = artifacts.into_iter().next().expect("one artifact");
        Ok(artifact.executable)
    }

    fn run(&self) -> std::process::Output {
        let executable = self.build().expect("FFI project should build");
        Command::new(&executable)
            .output()
            .expect("generated executable should run")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const DEMO_C: &str = r#"
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>

#if defined(_MSC_VER)
#define DOLPHIN_ALIGNOF(T) __alignof(T)
#else
#define DOLPHIN_ALIGNOF(T) _Alignof(T)
#endif

typedef struct { double x; double y; } CPoint;

int32_t ffi_narrow(int8_t a, int16_t b) { return (int32_t)a * 1000 + (int32_t)b; }
double ffi_mix(float a, double b) { return (double)a + b; }
int32_t ffi_long(long value) { return (int32_t)value; }
size_t ffi_sizeof_long(void) { return sizeof(long); }
int32_t ffi_first_char(const char *text) { return (int32_t)(unsigned char)text[0]; }

int32_t ffi_fill(uint8_t *out, size_t len) {
    for (size_t i = 0; i < len; i++) out[i] = (uint8_t)(i + 1);
    return 0;
}

void *ffi_create(void) { return malloc(16); }
void ffi_destroy(void *handle) { free(handle); }
void *ffi_create_null(void) { return NULL; }

double ffi_translate_x(CPoint *point, double dx) { point->x += dx; return point->x; }
size_t ffi_sizeof_point(void) { return sizeof(CPoint); }
size_t ffi_alignof_point(void) { return DOLPHIN_ALIGNOF(CPoint); }
"#;

/// 编译 C 源码为目标文件，返回目标文件路径。
fn compile_c_object(project: &Project, stem: &str, source: &str) -> PathBuf {
    let source_path = project.path(&format!("native/{stem}.c"));
    fs::create_dir_all(source_path.parent().unwrap()).expect("native dir");
    fs::write(&source_path, source).expect("C source should be written");
    if cfg!(windows) {
        let object = project.path(&format!("native/{stem}.obj"));
        let vcvars = locate_vcvars64();
        let script = project.path(&format!("native/build_{stem}.bat"));
        let contents = format!(
            "@echo off\r\ncall \"{}\" >nul\r\ncl /nologo /c /Fo\"{}\" \"{}\"\r\n",
            vcvars.display(),
            object.display(),
            source_path.display()
        );
        fs::write(&script, contents).expect("build script should be written");
        let status = Command::new(&script)
            .status()
            .expect("MSVC build script should run");
        assert!(status.success(), "cl failed to compile the C fixture");
        object
    } else {
        let object = project.path(&format!("native/{stem}.o"));
        let status = Command::new("cc")
            .args(["-std=c11", "-c", "-o"])
            .arg(&object)
            .arg(&source_path)
            .status()
            .expect("cc should run");
        assert!(status.success(), "cc failed to compile the C fixture");
        object
    }
}

/// 把目标文件归档为静态库（Unix 用 `ar`，Windows 用 `llvm-ar`）。
fn archive_static_library(project: &Project, object: &Path, stem: &str) -> PathBuf {
    let archive = project.path(&format!(
        "native/{}",
        if cfg!(windows) {
            format!("{stem}.lib")
        } else {
            format!("lib{stem}.a")
        }
    ));
    let tool = if cfg!(windows) { "llvm-ar" } else { "ar" };
    let status = Command::new(tool)
        .arg("rcs")
        .arg(&archive)
        .arg(object)
        .status()
        .unwrap_or_else(|error| panic!("failed to run {tool}: {error}"));
    assert!(status.success(), "{tool} failed to archive the C fixture");
    archive
}

#[cfg(unix)]
fn build_shared_library(project: &Project, stem: &str, source: &str) -> PathBuf {
    let source_path = project.path(&format!("native/{stem}.c"));
    fs::create_dir_all(source_path.parent().unwrap()).expect("native dir");
    fs::write(&source_path, source).expect("C source should be written");
    // Darwin 使用 `.dylib` 与 `-install_name`；ELF 平台使用 `.so` 与 `-soname`。
    // 不能把 `-Wl,-soname` 传给 macOS linker（ld64 不识别该 ELF 选项）。
    let darwin = cfg!(target_os = "macos");
    let (name, extra) = if darwin {
        (
            format!("lib{stem}.dylib"),
            vec![
                "-install_name".to_string(),
                format!("@rpath/lib{stem}.dylib"),
            ],
        )
    } else {
        (
            format!("lib{stem}.so"),
            vec![format!("-Wl,-soname,lib{stem}.so")],
        )
    };
    let library = project.path(&format!("native/{name}"));
    let mut command = Command::new("cc");
    command.args(["-std=c11", "-fPIC"]);
    if darwin {
        command.arg("-dynamiclib");
    } else {
        command.arg("-shared");
    }
    let status = command
        .args(&extra)
        .arg("-o")
        .arg(&library)
        .arg(&source_path)
        .status()
        .expect("cc should run");
    assert!(status.success(), "cc failed to build the shared library");
    library
}

#[cfg(not(windows))]
fn locate_vcvars64() -> PathBuf {
    unreachable!("vcvars64.bat is only used when compiling the C fixture on Windows")
}

#[cfg(windows)]
fn locate_vcvars64() -> PathBuf {
    if let Ok(path) = std::env::var("DOLPHIN_VCVARS") {
        return PathBuf::from(path);
    }
    for vswhere in [
        r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe",
        r"C:\Program Files\Microsoft Visual Studio\Installer\vswhere.exe",
    ] {
        if let Ok(output) = Command::new(vswhere)
            .args([
                "-latest",
                "-products",
                "*",
                "-requires",
                "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                "-property",
                "installationPath",
            ])
            .output()
        {
            let installation = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !installation.is_empty() {
                let candidate = Path::new(&installation).join("VC/Auxiliary/Build/vcvars64.bat");
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    for root in ["ProgramFiles", "ProgramFiles(x86)"] {
        let Some(root) = std::env::var(root).ok() else {
            continue;
        };
        for year in ["2022", "2019"] {
            for edition in ["Community", "Professional", "Enterprise", "BuildTools"] {
                let candidate = PathBuf::from(&root)
                    .join("Microsoft Visual Studio")
                    .join(year)
                    .join(edition)
                    .join("VC/Auxiliary/Build/vcvars64.bat");
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    panic!("could not locate vcvars64.bat for the FFI tests");
}

/// 把路径渲染为 TOML 基本字符串内容：Windows 反斜杠需转义为 `/`。
fn toml_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn assert_output(output: &std::process::Output, code: i32, stdout: &str) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "unexpected exit code; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), stdout);
}

#[test]
fn ffi01_scalar_and_pointer_arguments() {
    let project = Project::create(&[(
        "src/main.do",
        r#"
        use std.mem;

        extern "C" {
            pub fn ffi_narrow(a: i8, b: i16): i32;
            pub fn ffi_mix(a: f32, b: f64): f64;
            pub fn ffi_long(value: c_long): i32;
            pub fn ffi_sizeof_long(): usize;
            pub fn ffi_first_char(text: *const c_char): i32;
        }

        fn main() {
            val narrow = ffi_narrow(-8_i8, 16_i16);
            val mixed = ffi_mix(1.5_f32, 2.25_f64);
            val long_matches = mem.size_of<c_long>() == ffi_sizeof_long();
            val text = "海豚";
            val bytes = text.bytes();
            val first = ffi_first_char(mem.cast_const_ptr<c_char>(bytes.ptr));
            println("{} {} {} {}", narrow, mixed, long_matches, first);
            return (narrow + 8000) as i32;
        }
        "#,
    )]);
    let object = compile_c_object(&project, "demo", DEMO_C);
    project.write_manifest(&format!("objects = [\"{}\"]", toml_path(&object)));
    let output = project.run();
    assert_output(&output, 16, "-7984 3.75 true 230\n");
}

#[test]
fn ffi02_c_writes_buffer_and_handles_opaque_pointer() {
    let project = Project::create(&[(
        "src/main.do",
        r#"
        use std.mem;

        extern "C" {
            pub fn ffi_fill(out: *u8, len: usize): i32;
            pub fn ffi_create(): *Unit;
            pub fn ffi_destroy(handle: *Unit);
            pub fn ffi_create_null(): *Unit;
        }

        fn main() {
            val handle = ffi_create();
            if handle == null { return 1; }
            defer ffi_destroy(handle);

            val bytes = mem.alloc<u8>(4);
            defer mem.free(bytes);
            val status = ffi_fill(bytes.ptr, bytes.len);
            if status != 0 { return status; }

            val empty = ffi_create_null();
            val empty_ok = empty == null;
            println("{} {} {} {}", bytes[0], bytes[1], bytes[2], empty_ok);
            return 0;
        }
        "#,
    )]);
    let object = compile_c_object(&project, "demo", DEMO_C);
    project.write_manifest(&format!("objects = [\"{}\"]", toml_path(&object)));
    let output = project.run();
    assert_output(&output, 0, "1 2 3 true\n");
}

#[test]
fn ffi03_extern_struct_layout_matches_c() {
    let project = Project::create(&[(
        "src/main.do",
        r#"
        use std.mem;

        extern struct CPoint {
            x: f64,
            y: f64,
        }

        extern "C" {
            pub fn ffi_translate_x(point: *CPoint, dx: f64): f64;
            pub fn ffi_sizeof_point(): usize;
            pub fn ffi_alignof_point(): usize;
        }

        fn main() {
            val point = mem.create<CPoint>(CPoint(1.0, 2.0));
            defer mem.destroy(point);

            val moved = ffi_translate_x(point, 10.0);
            val y = (*point).y;
            val size_ok = mem.size_of<CPoint>() == ffi_sizeof_point();
            val align_ok = mem.align_of<CPoint>() == ffi_alignof_point();
            println("{} {} {} {}", moved, y, size_ok, align_ok);
            return 0;
        }
        "#,
    )]);
    let object = compile_c_object(&project, "demo", DEMO_C);
    project.write_manifest(&format!("objects = [\"{}\"]", toml_path(&object)));
    let output = project.run();
    assert_output(&output, 0, "11 2 true true\n");
}

#[test]
fn ffi04_rejects_non_c_signatures() {
    let cases = [
        (
            "extern \"C\" { pub fn bad(s: string); }",
            "cannot be passed by value",
        ),
        (
            "struct Point { x: i32 } extern \"C\" { pub fn bad(p: Point); }",
            "cannot be passed by value",
        ),
        (
            "extern \"C\" { pub fn bad(flag: bool); }",
            "cannot be passed by value",
        ),
        (
            "extern \"C\" { pub fn bad(s: []u8); }",
            "cannot be passed by value",
        ),
    ];
    for (declaration, expected) in cases {
        let project = Project::create(&[(
            "src/main.do",
            &format!("{declaration} fn main() {{ return 0; }}"),
        )]);
        project.write_manifest("objects = []");
        let error = project.build().unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "expected `{expected}` for `{declaration}`, got `{error}`"
        );
    }
}

#[test]
fn ffi05_reports_missing_native_input_and_symbol() {
    // 缺失的原生目标文件：诊断包含路径与目标三元组。
    let missing = Project::create(&[("src/main.do", "fn main() { return 0; }")]);
    missing.write_manifest("objects = [\"native/does-not-exist.o\"]");
    let error = missing.build().unwrap_err();
    assert!(error.to_string().contains("does-not-exist.o"));
    assert!(error.to_string().contains(&missing.triple));

    // 未解析符号：链接器诊断必须包含缺失符号。
    let unresolved = Project::create(&[(
        "src/main.do",
        r#"
        extern "C" {
            pub fn ffi_missing_symbol(): i32;
        }
        fn main() { return ffi_missing_symbol(); }
        "#,
    )]);
    let object = compile_c_object(&unresolved, "demo", DEMO_C);
    unresolved.write_manifest(&format!("objects = [\"{}\"]", toml_path(&object)));
    let error = unresolved.build().unwrap_err();
    assert!(
        error.to_string().contains("ffi_missing_symbol"),
        "linker error should name the missing symbol, got `{error}`"
    );
}

#[test]
fn ffi05_handles_paths_with_spaces() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("dolphin ffi space {}-{unique}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let project = Project {
        root: root.clone(),
        triple: host_platform().unwrap().triple().to_string(),
        bin: "app".to_string(),
    };
    fs::create_dir_all(project.path("src")).expect("src dir");
    fs::write(
        project.path("src/main.do"),
        r#"
        extern "C" {
            pub fn ffi_narrow(a: i8, b: i16): i32;
        }
        fn main() { return ffi_narrow(0_i8, 7_i16) as i32; }
        "#,
    )
    .expect("main.do");
    let object = compile_c_object(&project, "demo", DEMO_C);
    project.write_manifest(&format!("objects = [\"{}\"]", toml_path(&object)));
    let output = project.run();
    assert_eq!(output.status.code(), Some(7));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ffi06_static_library_integration() {
    let project = Project::create(&[(
        "src/main.do",
        r#"
        extern "C" {
            pub fn ffi_narrow(a: i8, b: i16): i32;
        }
        fn main() { return ffi_narrow(0_i8, 3_i16) as i32; }
        "#,
    )]);
    let object = compile_c_object(&project, "demo", DEMO_C);
    let archive = archive_static_library(&project, &object, "demo");
    project.write_manifest(&format!("static-libs = [\"{}\"]", toml_path(&archive)));
    let output = project.run();
    assert_eq!(output.status.code(), Some(3));
}

#[cfg(unix)]
#[test]
fn ffi06_shared_library_integration() {
    let project = Project::create(&[(
        "src/main.do",
        r#"
        extern "C" {
            pub fn ffi_narrow(a: i8, b: i16): i32;
        }
        fn main() { return ffi_narrow(0_i8, 5_i16) as i32; }
        "#,
    )]);
    let library = build_shared_library(&project, "demo", DEMO_C);
    project.write_manifest(&format!(
        "shared-libs = [\"{library}\"]\nruntime-files = [\"{library}\"]",
        library = toml_path(&library)
    ));
    let executable = project
        .build()
        .expect("shared library project should build");
    let copied = executable
        .parent()
        .unwrap()
        .join(library.file_name().unwrap());
    assert!(
        copied.is_file(),
        "runtime file should be copied next to the executable"
    );
    let output = Command::new(&executable)
        .output()
        .expect("generated executable should run");
    assert_eq!(output.status.code(), Some(5));
}
