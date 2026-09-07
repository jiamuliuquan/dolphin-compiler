use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_compiler::{BuildOptions, build};

static NEXT_PROJECT_ID: AtomicU64 = AtomicU64::new(0);

#[test]
fn builds_and_runs_m1_arithmetic() {
    assert_program_exit("fn main() { return 1 + 2 * 3; }", 7);
}

#[test]
fn builds_and_runs_m2_variables() {
    assert_program_exit(
        "fn main() { var value: i32 = 2; val step = 3; value += step; value *= 2; return value; }",
        10,
    );
}

#[test]
fn builds_and_runs_m3_control_flow() {
    assert_program_exit(
        r#"
        fn main() {
            var total = 0;
            var i = 0;
            while i < 10 {
                i += 1;
                if i % 2 == 0 {
                    continue;
                }
                total += i;
            }

            var loops = 0;
            loop {
                loops += 1;
                if loops >= 3 {
                    break;
                }
            }

            if total == 25 && loops == 3 && !false {
                return total + loops;
            } else {
                return 1;
            }
        }
        "#,
        28,
    );
}

#[test]
fn logical_operators_short_circuit() {
    assert_program_exit(
        r#"
        fn main() {
            if false && 1 / 0 == 0 {
                return 1;
            }
            if true || 1 / 0 == 0 {
                return 12;
            }
            return 2;
        }
        "#,
        12,
    );
}

#[test]
fn builds_and_runs_m4_functions_and_recursion() {
    assert_program_exit(
        r#"
        fn main() {
            return factorial(5);
        }

        fn factorial(value: i32): i32 {
            if value <= 1 {
                return 1;
            }
            return value * factorial(value - 1);
        }
        "#,
        120,
    );
}

#[test]
fn builds_and_runs_m5_print_runtime() {
    let output = run_program(
        r#"
        fn state(enabled: bool): string {
            if enabled {
                return "ready";
            }
            return "stopped";
        }

        fn announce(name: string, count: i32) {
            println("Hello, {}!", name);
            println("count = {}, enabled = {}", count, true);
        }

        fn main() {
            val name = "海豚";
            announce(name, 3);
            println("state = {}", state(true));
            println("escaped braces: {{}}");
            println("limits = {}, {}, '{}'", -2147483648, false, "");
        }
        "#,
    );
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Hello, 海豚!\ncount = 3, enabled = true\nstate = ready\nescaped braces: {}\nlimits = -2147483648, false, ''\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn builds_and_runs_m6_arrays_ranges_and_for() {
    assert_program_exit(
        r#"
        fn make_numbers(): [i32; 4] {
            return [1, 2, 3, 4];
        }

        fn sum(values: [i32; 4]): i32 {
            var total = 0;
            for value in values {
                total += value;
            }
            return total;
        }

        fn main() {
            var numbers = make_numbers();
            numbers[1] = 10;
            numbers[2] += 5;
            var total = sum(numbers);

            for i in 0..5 {
                if i == 2 {
                    continue;
                }
                total += i;
            }
            for i in 1..=3 {
                total += i;
            }
            val repeated: [i32; 3] = [2; 3];
            for value in repeated {
                total += value;
            }
            return total;
        }
        "#,
        43,
    );
}

#[test]
fn m6_string_arrays_cross_function_boundaries() {
    let output = run_program(
        r#"
        fn names(): [string; 2] {
            return ["Ada", "Lin"];
        }

        fn show(values: [string; 2]) {
            for value in values {
                println("name = {}", value);
            }
        }

        fn main() {
            var values = names();
            values[1] = "Dolphin";
            show(values);
        }
        "#,
    );
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "name = Ada\nname = Dolphin\n"
    );
}

#[test]
fn m6_array_bounds_are_checked_at_runtime() {
    let output =
        run_program("fn main() { val values = [1, 2]; var index = 2; return values[index]; }");
    assert!(!output.status.success());
}

#[test]
fn builds_and_runs_m7_modules() {
    let output = run_project(&[
        (
            "src/main.do",
            r#"
            use std.math;
            use text.labels.name;

            fn main() {
                println("{} = {}", name(), math.min(helper(), 7));
                return math.min(helper(), 7);
            }
            "#,
        ),
        ("src/helper.do", "fn helper(): i32 { return 5; }"),
        (
            "src/std/math.do",
            "pkg std.math; pub fn min(a: i32, b: i32): i32 { if a < b { return a; } return b; } fn private_value(): i32 { return 9; }",
        ),
        (
            "src/text/labels.do",
            "pkg text.labels; pub fn name(): string { return \"minimum\"; }",
        ),
    ])
    .expect("M7 project should build and run");
    assert_eq!(output.status.code(), Some(5));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "minimum = 5\n");
}

#[test]
fn m7_rejects_private_access_and_package_mismatch() {
    let private = build_project(&[
        (
            "src/main.do",
            "use std.math; fn main() { return math.secret(); }",
        ),
        (
            "src/std/math.do",
            "pkg std.math; fn secret(): i32 { return 1; }",
        ),
    ])
    .unwrap_err();
    assert!(private.to_string().contains("private"));
    assert!(private.to_string().contains("src/main.do"));

    let mismatch = build_project(&[
        ("src/main.do", "fn main() {}"),
        (
            "src/std/math.do",
            "pkg wrong.path; pub fn min(a: i32, b: i32): i32 { return a; }",
        ),
    ])
    .unwrap_err();
    assert!(mismatch.to_string().contains("does not match"));
    assert!(mismatch.to_string().contains("src/std/math.do"));
}

#[test]
fn builds_and_runs_m8_scalar_types_casts_and_strings() {
    let output = run_program(
        r#"
        fn main() {
            val a: i8 = -8_i8;
            val b: i16 = 16_i16;
            val c: i64 = 64_i64;
            val d: u8 = 8_u8;
            val e: u16 = 16_u16;
            val f: u32 = 32_u32;
            val g: u64 = 64_u64;
            val x: f32 = 1.5_f32;
            val y: f64 = 2.25_f64;
            val letter: char = '海';
            val values: [u16; 2] = [1_u16, 2_u16];
            val converted: i32 = c as i32;
            println("signed = {}, {}, {}", a, b, c);
            println("unsigned = {}, {}, {}, {}", d, e, f, g);
            println("float = {}, {}", x, y);
            println("char = {}, array = {}, {}", letter, values[0], values[1]);
            println("string = {}, length = {}", "海豚" == "海豚", length("海豚"));
            return converted;
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(64));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "signed = -8, 16, 64\nunsigned = 8, 16, 32, 64\nfloat = 1.5, 2.25\nchar = 海, array = 1, 2\nstring = true, length = 6\n"
    );
}

#[test]
fn m8_narrow_integer_overflow_has_runtime_message() {
    let output = run_program(
        "fn main() { val value: u8 = 255_u8; val next = value + 1_u8; return next as i32; }",
    );
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("Dolphin runtime error")
    );
}

#[test]
fn m13_structs_enums_and_match() {
    let output = run_program(
        r#"
        struct Point {
            x: i32,
            y: i32,
        }

        enum Shape {
            Circle(f64),
            Rectangle(f64, f64),
            Empty,
        }

        fn main() {
            var p = Point(3, 4);
            val sum = p.x + p.y;

            val circle = Shape.Circle(2.0);
            val circle_area = match circle {
                Shape.Circle(r) => 3.14 * r * r,
                Shape.Rectangle(w, h) => w * h,
                Shape.Empty => 0.0,
            };

            val rect = Shape.Rectangle(3.0, 4.0);
            val rect_area = match rect {
                Shape.Circle(r) => 3.14 * r * r,
                Shape.Rectangle(w, h) => w * h,
                Shape.Empty => 0.0,
            };

            println("sum = {}, circle = {}, rect = {}", sum, circle_area, rect_area);
            return sum + (circle_area as i32) + (rect_area as i32);
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(31));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "sum = 7, circle = 12.56, rect = 12\n"
    );
}

#[test]
fn m13_enum_error_handling_with_wildcard() {
    let output = run_program(
        r#"
        enum ParseResult {
            Ok(i32),
            Error(i32),
        }

        fn main() {
            val ok = ParseResult.Ok(42);
            val ok_value = match ok {
                ParseResult.Ok(v) => v,
                ParseResult.Error(_) => -1,
            };

            val err = ParseResult.Error(1);
            val err_value = match err {
                ParseResult.Ok(v) => v,
                ParseResult.Error(_) => -1,
            };

            return ok_value + err_value;
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(41));
}

#[test]
fn m13_cross_module_types() {
    let output = run_project(&[
        (
            "src/main.do",
            r#"
            use geom.shapes;

            fn main() {
                var p = shapes.Point(3, 4);
                val circle = shapes.Shape.Circle(2.0);
                val area = match circle {
                    shapes.Shape.Circle(r) => 3.14 * r * r,
                    shapes.Shape.Rectangle(w, h) => w * h,
                    shapes.Shape.Empty => 0.0,
                };
                return p.x + p.y;
            }
            "#,
        ),
        (
            "src/geom/shapes.do",
            r#"
            pkg geom.shapes;
            pub struct Point {
                x: i32,
                y: i32,
            }
            pub enum Shape {
                Circle(f64),
                Rectangle(f64, f64),
                Empty,
            }
            "#,
        ),
    ])
    .expect("M13 cross-module project should build and run");
    assert_eq!(output.status.code(), Some(7));
}

#[test]
fn m13_rejects_missing_match_arms_and_duplicate_fields() {
    let missing = build_project(&[(
        "src/main.do",
        r#"
        enum Shape {
            Circle(f64),
            Rectangle(f64, f64),
        }
        fn main() {
            val s = Shape.Circle(2.0);
            return match s {
                Shape.Circle(r) => 1,
            };
        }
        "#,
    )])
    .unwrap_err();
    assert!(missing.to_string().contains("missing variant"));

    let duplicate = build_project(&[(
        "src/main.do",
        "struct Point { x: i32, x: i32 } fn main() {}",
    )])
    .unwrap_err();
    assert!(duplicate.to_string().contains("already defined"));
}

#[test]
fn m14_structs_passed_by_value() {
    // 结构体作为函数参数与返回值（按值传递，浅拷贝）。
    assert_program_exit(
        r#"
        struct Point { x: i32, y: i32 }
        fn add(a: Point, b: Point): Point {
            return Point(a.x + b.x, a.y + b.y);
        }
        fn main() {
            var p = Point(3, 4);
            var q = Point(10, 20);
            var r = add(p, q);
            return r.x + r.y;
        }
        "#,
        37,
    );
}

#[test]
fn m14_explicit_pointers() {
    // 显式指针：取址 `&`、解引用 `*`、字段访问 `->`。
    assert_program_exit(
        r#"
        struct Point { x: i32, y: i32 }
        fn main() {
            var p = Point(1, 2);
            var q: *Point = &p;
            q->x = 100;
            var deref = *q;
            return p.x + deref.y;
        }
        "#,
        102,
    );
}

#[test]
fn m14_allocate_free_and_slice_indexing() {
    // 动态切片：allocate + 索引读写 + length + free。
    assert_program_exit(
        r#"
        fn fill(): []u8 {
            var buf = allocate(3);
            buf[0] = 5_u8;
            buf[1] = 6_u8;
            buf[2] = 7_u8;
            return buf;
        }
        fn main() {
            var buf = fill();
            var total = buf[0] as i32 + buf[1] as i32 + buf[2] as i32;
            total += length(buf);
            free(buf);
            return total;
        }
        "#,
        21,
    );
}

#[test]
fn m14_try_and_defer() {
    assert_program_exit(
        r#"
        fn main() {
            var sum = 0;
            try (var a = allocate(2), var b = allocate(2)) {
                a[0] = 1_u8;
                a[1] = 2_u8;
                b[0] = 3_u8;
                b[1] = 4_u8;
                sum = a[0] as i32 + a[1] as i32 + b[0] as i32 + b[1] as i32;
            }
            var d = allocate(1);
            defer free(d);
            d[0] = 9_u8;
            return sum + d[0] as i32;
        }
        "#,
        19,
    );
}

#[test]
fn m14_string_concatenation() {
    assert_program_exit(
        r#"
        fn main() {
            var s = "hello" + " " + "world";
            var result = 0;
            if s == "hello world" {
                result = 42;
            }
            free(s);
            return result;
        }
        "#,
        42,
    );
}

#[test]
fn m14_double_free_is_detected() {
    // 双重释放：运行时退出码 103。
    let output = run_program(
        r#"
        fn main() {
            var buf = allocate(4);
            free(buf);
            free(buf);
            return 0;
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(103));
}

#[test]
fn m14_try_resource_cannot_escape() {
    // try 资源禁止逃逸（return / 赋值 / 重新绑定）。
    let returned = build_project(&[(
        "src/main.do",
        r#"
        fn build(): []u8 {
            try (var buf = allocate(4)) {
                return buf;
            }
            return allocate(1);
        }
        fn main() { return 0; }
        "#,
    )])
    .unwrap_err();
    assert!(returned.to_string().contains("cannot escape"));

    let rebound = build_project(&[(
        "src/main.do",
        r#"
        fn main() {
            try (var buf = allocate(4)) {
                var x = buf;
            }
            return 0;
        }
        "#,
    )])
    .unwrap_err();
    assert!(rebound.to_string().contains("cannot escape"));
}

fn assert_program_exit(source: &str, expected: i32) {
    let output = run_program(source);
    assert_eq!(output.status.code(), Some(expected));
}

fn run_program(source: &str) -> std::process::Output {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let project = std::env::temp_dir().join(format!(
        "dolphin-compiler-test-{}-{unique}-{}",
        std::process::id(),
        NEXT_PROJECT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let source_dir = project.join("src");
    fs::create_dir_all(&source_dir).expect("temporary source directory should be created");
    fs::write(source_dir.join("main.do"), source).expect("temporary source should be written");

    let artifact = build(BuildOptions {
        input: project.clone(),
        output: None,
    })
    .expect("test project should build");
    assert!(artifact.object.is_file());
    assert!(artifact.executable.is_file());

    let output = Command::new(&artifact.executable)
        .output()
        .expect("generated executable should run");

    fs::remove_dir_all(project).expect("temporary project should be removed");
    output
}

fn run_project(
    files: &[(&str, &str)],
) -> Result<std::process::Output, dolphin_compiler::Diagnostic> {
    let (project, artifact) = build_project_with_path(files)?;
    let output = Command::new(&artifact.executable)
        .output()
        .expect("generated executable should run");
    fs::remove_dir_all(project).expect("temporary project should be removed");
    Ok(output)
}

fn build_project(
    files: &[(&str, &str)],
) -> Result<dolphin_compiler::BuildArtifact, dolphin_compiler::Diagnostic> {
    build_project_with_path(files).map(|(_, artifact)| artifact)
}

fn build_project_with_path(
    files: &[(&str, &str)],
) -> Result<(std::path::PathBuf, dolphin_compiler::BuildArtifact), dolphin_compiler::Diagnostic> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let project = std::env::temp_dir().join(format!(
        "dolphin-modules-test-{}-{unique}-{}",
        std::process::id(),
        NEXT_PROJECT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    for (relative, source) in files {
        let path = project.join(relative);
        fs::create_dir_all(path.parent().unwrap()).expect("source directory should be created");
        fs::write(path, source).expect("source file should be written");
    }
    match build(BuildOptions {
        input: project.clone(),
        output: None,
    }) {
        Ok(artifact) => Ok((project, artifact)),
        Err(error) => {
            fs::remove_dir_all(project).expect("temporary project should be removed");
            Err(error)
        }
    }
}
