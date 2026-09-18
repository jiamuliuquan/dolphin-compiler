use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_compiler::{BuildOptions, BuildProfile, BuildSettings, build, build_with_profile};

mod support;

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
            use mathutil.math;
            use text.labels.name;

            fn main() {
                println("{} = {}", name(), math.min(helper(), 7));
                return math.min(helper(), 7);
            }
            "#,
        ),
        ("src/helper.do", "fn helper(): i32 { return 5; }"),
        (
            "src/mathutil/math.do",
            "pkg mathutil; pub fn min(a: i32, b: i32): i32 { if a < b { return a; } return b; } fn private_value(): i32 { return 9; }",
        ),
        (
            "src/text/labels.do",
            "pkg text; pub fn name(): string { return \"minimum\"; }",
        ),
    ])
    .expect("M7 project should build and run");
    assert_eq!(output.status.code(), Some(5));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "minimum = 5\n");
}

#[test]
fn m7_package_prefix_import_exposes_nested_module() {
    let output = run_project(&[
        (
            "src/main.do",
            r#"
            use mathutil;

            fn main() {
                return mathutil.math.min(8, 3);
            }
            "#,
        ),
        (
            "src/mathutil/math.do",
            "pkg mathutil; pub fn min(a: i32, b: i32): i32 { if a < b { return a; } return b; }",
        ),
    ])
    .expect("package prefix import should build and run");
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn m7_rejects_private_access_and_package_mismatch() {
    let private = build_project(&[
        (
            "src/main.do",
            "use mathutil.math; fn main() { return math.secret(); }",
        ),
        (
            "src/mathutil/math.do",
            "pkg mathutil; fn secret(): i32 { return 1; }",
        ),
    ])
    .unwrap_err();
    assert!(private.to_string().contains("private"));
    assert!(private.to_string().contains("src/main.do"));

    let mismatch = build_project(&[
        ("src/main.do", "fn main() {}"),
        (
            "src/mathutil/math.do",
            "pkg wrong.path; pub fn min(a: i32, b: i32): i32 { return a; }",
        ),
    ])
    .unwrap_err();
    assert!(mismatch.to_string().contains("does not match"));
    assert!(mismatch.to_string().contains("src/mathutil/math.do"));
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
            pkg geom;
            pub struct Point {
                pub x: i32,
                pub y: i32,
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
fn m14_aggregate_pass_return_and_field_write() {
    let output = run_program(
        r#"
        struct Point {
            x: i32,
            y: i32,
        }

        fn translate(p: Point, dx: i32, dy: i32): Point {
            var moved = p;
            moved.x = moved.x + dx;
            moved.y = moved.y + dy;
            return moved;
        }

        fn main() {
            var p = Point(10, 20);
            p.x = 1;
            val q = translate(p, 10, 20);
            return p.x + q.x + q.y;
        }
        "#,
    );
    // 注意：进程退出码在 Linux/Unix 上是 8 位（0..=255），必须用 ≤ 255 的值。
    assert_eq!(output.status.code(), Some(52));
}

#[test]
fn m14_enum_pass_return() {
    let output = run_program(
        r#"
        enum Shape {
            Circle(f64),
            Rectangle(f64, f64),
            Empty,
        }

        fn area(s: Shape): f64 {
            return match s {
                Shape.Circle(r) => 3.14 * r * r,
                Shape.Rectangle(w, h) => w * h,
                Shape.Empty => 0.0,
            };
        }

        fn main() {
            val circle = Shape.Circle(2.0);
            val rect = Shape.Rectangle(3.0, 4.0);
            return area(circle) as i32 + area(rect) as i32;
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(24));
}

#[test]
fn m14_usize_and_isize() {
    let output = run_program(
        r#"
        fn main() {
            val a: usize = 42_usize;
            val b: isize = -7_isize;
            return (a as i32) + (b as i32);
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(35));
}

#[test]
fn m14_rejects_recursive_layout() {
    let error = build_project(&[(
        "src/main.do",
        "struct Node { value: i32, next: Node } fn main() { return 0; }",
    )])
    .unwrap_err();
    assert!(error.to_string().contains("recursive layout"));
}

#[test]
fn m14_rejects_field_write_to_immutable() {
    let error = build_project(&[(
        "src/main.do",
        "struct Point { x: i32 } fn main() { val p = Point(1); p.x = 2; return 0; }",
    )])
    .unwrap_err();
    assert!(error.to_string().contains("immutable variable"));
}

#[test]
fn m14_pointer_address_of_and_field_write() {
    let output = run_program(
        r#"
        struct Point {
            x: i32,
            y: i32,
        }

        fn update(p: *Point) {
            p->x = 9;
        }

        fn main() {
            var point = Point(1, 2);
            val p = &point;
            update(p);
            point.y = 20;
            return point.x + point.y;
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(29));
}

#[test]
fn m14_pointer_deref_read() {
    let output = run_program(
        r#"
        struct Point {
            x: i32,
            y: i32,
        }

        fn sum(p: *Point): i32 {
            return (*p).x + (*p).y;
        }

        fn main() {
            var point = Point(10, 20);
            val p = &point;
            return sum(p);
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(30));
}

#[test]
fn m14_null_pointer() {
    let output = run_program(
        r#"
        fn main() {
            val p: *i32 = null;
            if p == null {
                return 0;
            }
            return 1;
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn m14_rejects_write_through_const_pointer() {
    let error = build_project(&[(
        "src/main.do",
        r#"
        struct Point { x: i32 }
        fn main() {
            val p = Point(1);
            val ptr = &p;
            ptr->x = 9;
            return 0;
        }
        "#,
    )])
    .unwrap_err();
    assert!(error.to_string().contains("*const"));
}

#[test]
fn m14_rejects_address_of_temporary() {
    let error = build_project(&[(
        "src/main.do",
        "struct Point { x: i32 } fn make(): Point { return Point(1); } fn main() { val p = &make(); return 0; }",
    )])
    .unwrap_err();
    assert!(error.to_string().contains("addressable"));
}

#[test]
fn m14_stable_address_after_reassignment() {
    let output = run_program(
        r#"
        struct Point {
            x: i32,
            y: i32,
        }

        fn main() {
            var point = Point(1, 2);
            val p = &point;
            point = Point(3, 4);
            return (*p).x + (*p).y;
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(7));
}

#[test]
fn m14_std_mem_namespace_resolves() {
    let output = run_program("use std.mem; fn main() { return 0; }");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn m14_mem_alloc_read_write_and_free() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            val bytes = mem.alloc<u8>(4);
            bytes[0] = 68_u8;
            bytes[1] = 111_u8;
            bytes[2] = 108_u8;
            bytes[3] = 0_u8;
            println("{} {} {}", bytes[0], bytes[1], bytes[2]);
            val last = bytes[3] as i32;
            mem.free(bytes);
            return last;
        }
        "#,
    )])
    .expect("mem project should build");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "68 111 108\n");
    assert!(
        output.stderr.is_empty(),
        "fully freed program must not report leaks"
    );
}

#[test]
fn m14_mem_zero_allocation_is_noop_and_leak_free() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            val empty = mem.alloc<u8>(0);
            println("{}", empty.len);
            mem.free(empty);
            return 0;
        }
        "#,
    )])
    .expect("zero allocation project should build");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn m14_mem_padded_struct_size_align_and_stride() {
    // Mixed 的布局：a@0、b@8、c@16，最大对齐 8，末尾补齐到 24。
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        struct Mixed {
            a: u8,
            b: u64,
            c: u16,
        }

        fn main() {
            val size = mem.size_of<Mixed>() as i32;
            val align = mem.align_of<Mixed>() as i32;

            val items = mem.alloc<Mixed>(3);
            items[0] = Mixed(1_u8, 10_u64, 2_u16);
            items[2] = Mixed(3_u8, 20_u64, 4_u16);
            val stride_sum = (items[0].b as i32) + (items[2].b as i32) + (items[2].c as i32);
            mem.free(items);

            println("size={} align={} stride={}", size, align, stride_sum);
            return stride_sum;
        }
        "#,
    )])
    .expect("padded struct project should build");
    // Mixed 的布局：a@0、b@8、c@16，最大对齐 8，末尾补齐到 24。
    assert_eq!(output.status.code(), Some(34));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "size=24 align=8 stride=34\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn m14_mem_create_destroy_roundtrip() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        struct Point {
            x: i32,
            y: i32,
        }

        fn main() {
            val p = mem.create<Point>(Point(4, 5));
            p->x = 40;
            val result = (*p).x + (*p).y;
            mem.destroy(p);
            return result;
        }
        "#,
    )])
    .expect("create/destroy project should build");
    assert_eq!(output.status.code(), Some(45));
    assert!(output.stderr.is_empty());
}

#[test]
fn m14_mem_view_copy_and_utf8() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            val bytes = mem.alloc<u8>(3);
            bytes[0] = 230_u8;
            bytes[1] = 181_u8;
            bytes[2] = 183_u8;

            val copy = mem.alloc<u8>(3);
            mem.copy(copy, bytes);

            val view = mem.view<u8>(copy.ptr, copy.len);
            val read_only: []const u8 = view;
            println("{} {}", mem.is_valid_utf8(read_only), view[2]);

            mem.free(bytes);
            mem.free(copy);
            return 0;
        }
        "#,
    )])
    .expect("view/copy project should build");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true 183\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn m14_mem_cast_ptr_roundtrip() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            val bytes = mem.alloc<u32>(2);
            bytes[0] = 7_u32;
            bytes[1] = 9_u32;

            val opaque = mem.cast_ptr<Unit>(bytes.ptr);
            val restored = mem.cast_ptr<u32>(opaque);
            val view = mem.view<u32>(restored, 2);
            val result = (view[0] + view[1]) as i32;

            mem.free(bytes);
            return result;
        }
        "#,
    )])
    .expect("cast_ptr project should build");
    assert_eq!(output.status.code(), Some(16));
}

#[test]
fn m14_mem_size_overflow_exits_102_in_both_profiles() {
    let source = r#"
        use std.mem;

        fn main() {
            val bytes = mem.alloc<u64>(18446744073709551615_usize);
            mem.free(bytes);
            return 0;
        }
    "#;
    for profile in [BuildProfile::Debug, BuildProfile::Release] {
        let output = run_project_with_profile(&[("src/main.do", source)], profile, &[])
            .expect("overflow project should build");
        assert_eq!(
            output.status.code(),
            Some(102),
            "size overflow must exit 102 in {profile:?}"
        );
    }
}

#[test]
fn m14_mem_allocation_failure_injection_exits_102_in_both_profiles() {
    let source = r#"
        use std.mem;

        fn main() {
            val bytes = mem.alloc<u8>(4);
            mem.free(bytes);
            return 0;
        }
    "#;
    for profile in [BuildProfile::Debug, BuildProfile::Release] {
        let output = run_project_with_profile(
            &[("src/main.do", source)],
            profile,
            &[("DOLPHIN_TEST_ALLOC_LIMIT", "0")],
        )
        .expect("injected OOM project should build");
        assert_eq!(
            output.status.code(),
            Some(102),
            "injected OOM must exit 102 in {profile:?}"
        );
    }
}

#[test]
fn m14_mem_invalid_free_exits_103_in_debug() {
    let double_free = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            val bytes = mem.alloc<u8>(4);
            mem.free(bytes);
            mem.free(bytes);
            return 0;
        }
        "#,
    )])
    .expect("double free project should build");
    assert_eq!(double_free.status.code(), Some(103));

    let subview = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            val bytes = mem.alloc<u8>(4);
            val sub = bytes.slice(0, 2);
            mem.free(sub);
            return 0;
        }
        "#,
    )])
    .expect("subview free project should build");
    assert_eq!(subview.status.code(), Some(103));

    let stack_view = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            var array: [u8; 2] = [1_u8, 2_u8];
            val view = mem.view<u8>(&array[0], 2);
            mem.free(view);
            return 0;
        }
        "#,
    )])
    .expect("stack view project should build");
    assert_eq!(stack_view.status.code(), Some(103));
}

#[test]
fn m14_mem_leak_is_reported_in_debug_only() {
    let source = r#"
        use std.mem;

        fn main() {
            val bytes = mem.alloc<u8>(8);
            bytes[0] = 1_u8;
            return 42;
        }
    "#;
    let debug = run_project_with_profile(&[("src/main.do", source)], BuildProfile::Debug, &[])
        .expect("leak project should build");
    assert_eq!(
        debug.status.code(),
        Some(42),
        "leak report preserves exit code"
    );
    assert!(
        String::from_utf8(debug.stderr).unwrap().contains("leaked"),
        "debug runtime should report the leak"
    );

    let release = run_project_with_profile(&[("src/main.do", source)], BuildProfile::Release, &[])
        .expect("leak project should build");
    assert_eq!(release.status.code(), Some(42));
    assert!(
        release.stderr.is_empty(),
        "release runtime does not track leaks"
    );
}

#[test]
fn m14_const_coercion_adds_readonly_for_slices_and_pointers() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn sum(p: *const i32, bytes: []const u8): i32 {
            return (*p) + (bytes[0] as i32);
        }

        fn main() {
            var value = 10;
            val bytes = mem.alloc<u8>(1);
            bytes[0] = 5_u8;
            val result = sum(&value, bytes);
            mem.free(bytes);
            return result;
        }
        "#,
    )])
    .expect("const coercion project should build");
    assert_eq!(output.status.code(), Some(15));
}

#[test]
fn m14_rejects_removing_readonly() {
    let error = build_project(&[(
        "src/main.do",
        r#"
        fn write(p: *i32) {}
        fn main() {
            val value = 1;
            write(&value);
            return 0;
        }
        "#,
    )])
    .unwrap_err();
    assert!(error.to_string().contains("*const"));
}

#[test]
fn m14_mem_view_with_null_and_nonzero_length_exits_101() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            val view = mem.view<u8>(null, 4);
            return view.len as i32;
        }
        "#,
    )])
    .expect("null view project should build");
    assert_eq!(output.status.code(), Some(101));
}

#[test]
fn m14_address_of_index_checks_bounds() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            val bytes = mem.alloc<u8>(2);
            var index = 2;
            val out_of_range = &bytes[index as usize];
            mem.free(bytes);
            return 0;
        }
        "#,
    )])
    .expect("address-of index project should build");
    assert_eq!(output.status.code(), Some(101));
}

#[test]
fn m14_defer_runs_in_reverse_on_natural_exit() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        fn note(tag: i32) { println("clean {}", tag); }

        fn main() {
            defer note(1);
            defer note(2);
            println("body");
            return 0;
        }
        "#,
    )])
    .expect("defer project should build");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "body\nclean 2\nclean 1\n"
    );
}

#[test]
fn m14_defer_covers_branches_return_and_fallthrough() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        fn note(tag: i32) { println("clean {}", tag); }

        fn pick(flag: bool): i32 {
            defer note(1);
            if flag {
                defer note(2);
                return 10;
            }
            defer note(3);
            return 20;
        }

        fn main() {
            println("{}", pick(true));
            println("{}", pick(false));
            return 0;
        }
        "#,
    )])
    .expect("defer branch project should build");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "clean 2\nclean 1\n10\nclean 3\nclean 1\n20\n"
    );
}

#[test]
fn m14_defer_loop_cleans_only_exited_scopes() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn note(tag: i32) { println("clean {}", tag); }

        fn main() {
            val outer = mem.alloc<u8>(1);
            defer mem.free(outer);
            for i in 0..3 {
                val inner = mem.alloc<u8>(1);
                defer mem.free(inner);
                defer note(i);
                if i == 1 { continue; }
                if i == 2 { break; }
                println("iter {}", i);
            }
            println("after");
            return 0;
        }
        "#,
    )])
    .expect("defer loop project should build");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "iter 0\nclean 0\nclean 1\nclean 2\nafter\n"
    );
    assert!(
        output.stderr.is_empty(),
        "each iteration must free its own allocation"
    );
}

#[test]
fn m14_defer_evaluates_arguments_at_exit() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        fn make(tag: i32): i32 { println("make {}", tag); return tag; }
        fn note(tag: i32) { println("clean {}", tag); }

        fn main() {
            var tag = 1;
            defer note(tag);
            defer note(make(5));
            tag = 2;
            println("body tag={}", tag);
            return 0;
        }
        "#,
    )])
    .expect("defer argument project should build");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "body tag=2\nmake 5\nclean 5\nclean 2\n"
    );
}

#[test]
fn m14_defer_return_value_snapshotted_before_cleanup() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            val bytes = mem.alloc<u8>(1);
            bytes[0] = 42_u8;
            defer mem.free(bytes);
            return bytes[0] as i32;
        }
        "#,
    )])
    .expect("defer return project should build");
    assert_eq!(output.status.code(), Some(42));
    assert!(output.stderr.is_empty());
}

#[test]
fn m14_defer_rejects_non_call_and_non_unit() {
    let non_call =
        build_project(&[("src/main.do", "fn main() { defer 1; return 0; }")]).unwrap_err();
    assert!(non_call.to_string().contains("call expression"));

    let non_unit = build_project(&[(
        "src/main.do",
        "fn main() { defer length(\"x\"); return 0; }",
    )])
    .unwrap_err();
    assert!(non_unit.to_string().contains("must return `Unit`"));
}

#[test]
fn m14_string_bytes_and_from_bytes_roundtrip() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            val text = "海豚";
            val bytes = text.bytes();
            println("len={} first={}", bytes.len, bytes[0]);
            val again = string.from_bytes(bytes);
            println("{}", again == text);
            return 0;
        }
        "#,
    )])
    .expect("string view project should build");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "len=6 first=230\ntrue\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn m14_string_from_bytes_rejects_invalid_utf8() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;

        fn main() {
            val bytes = mem.alloc<u8>(2);
            bytes[0] = 255_u8;
            bytes[1] = 254_u8;
            val text = string.from_bytes(bytes);
            println("{}", text);
            mem.free(bytes);
            return 0;
        }
        "#,
    )])
    .expect("invalid UTF-8 project should build");
    assert_eq!(output.status.code(), Some(104));
}

#[test]
fn m14_slice_range_and_index_bounds_are_checked() {
    let inverted = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;
        fn main() {
            val bytes = mem.alloc<u8>(4);
            val sub = bytes.slice(3, 1);
            mem.free(bytes);
            return sub.len as i32;
        }
        "#,
    )])
    .expect("inverted range project should build");
    assert_eq!(inverted.status.code(), Some(101));

    let too_long = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;
        fn main() {
            val bytes = mem.alloc<u8>(4);
            val sub = bytes.slice(0, 5);
            mem.free(bytes);
            return sub.len as i32;
        }
        "#,
    )])
    .expect("out of range project should build");
    assert_eq!(too_long.status.code(), Some(101));

    let index = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;
        fn main() {
            val bytes = mem.alloc<u8>(4);
            var position = 4;
            val value = bytes[position as usize];
            mem.free(bytes);
            return value as i32;
        }
        "#,
    )])
    .expect("index bounds project should build");
    assert_eq!(index.status.code(), Some(101));
}

#[test]
fn m14_field_address_shares_storage() {
    let output = run_program(
        r#"
        struct Point {
            x: i32,
            y: i32,
        }

        fn main() {
            var point = Point(1, 2);
            val x_address = &point.x;
            point.x = 5;
            val y_address = &point.y;
            point.y = 7;
            return (*x_address) + (*y_address);
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(12));
}

#[test]
fn m14_rejects_writing_readonly_string_views() {
    let bytes_write = build_project(&[(
        "src/main.do",
        r#"
        fn main() {
            val text = "abc";
            val bytes = text.bytes();
            bytes[0] = 65_u8;
            return 0;
        }
        "#,
    )])
    .unwrap_err();
    assert!(bytes_write.to_string().contains("[]const"));

    let string_write = build_project(&[(
        "src/main.do",
        r#"
        fn main() {
            val text = "abc";
            text[0] = 65_u8;
            return 0;
        }
        "#,
    )])
    .unwrap_err();
    assert!(
        string_write
            .to_string()
            .contains("cannot index value of type `string`")
    );

    let const_view_write = build_project(&[(
        "src/main.do",
        r#"
        use std.mem;
        fn main() {
            val bytes = mem.alloc<u8>(2);
            val read_only = mem.view_const<u8>(bytes.ptr, 2);
            read_only[0] = 1_u8;
            mem.free(bytes);
            return 0;
        }
        "#,
    )])
    .unwrap_err();
    assert!(const_view_write.to_string().contains("[]const"));
}

#[test]
fn m14_mem_rejects_invalid_intrinsic_usage() {
    let missing_type = build_project(&[(
        "src/main.do",
        "use std.mem; fn main() { val bytes = mem.alloc(4); return 0; }",
    )])
    .unwrap_err();
    assert!(missing_type.to_string().contains("explicit type argument"));

    let negative = build_project(&[(
        "src/main.do",
        "use std.mem; fn main() { val bytes = mem.alloc<u8>(-1); return 0; }",
    )])
    .unwrap_err();
    assert!(negative.to_string().contains("must not be negative"));

    let signed_count = build_project(&[(
        "src/main.do",
        "use std.mem; fn main() { val n: i32 = 4; val bytes = mem.alloc<u8>(n); return 0; }",
    )])
    .unwrap_err();
    assert!(signed_count.to_string().contains("must be `usize`"));

    let free_const = build_project(&[(
        "src/main.do",
        r#"
        use std.mem;
        fn main() {
            val bytes = mem.alloc<u8>(4);
            val read_only: []const u8 = bytes;
            mem.free(read_only);
            return 0;
        }
        "#,
    )])
    .unwrap_err();
    assert!(free_const.to_string().contains("writable slice"));

    let unknown = build_project(&[(
        "src/main.do",
        "use std.mem; fn main() { mem.allocate<u8>(4); return 0; }",
    )])
    .unwrap_err();
    assert!(unknown.to_string().contains("unknown function"));
}

#[test]
fn m15_generic_functions_types_and_methods() {
    let output = run_program(
        r#"
        struct Pair<T> {
            first: T,
            second: T,
        }

        impl<T> Pair<T> {
            fn swapped(self): Pair<T> {
                return Pair<T>(self.second, self.first);
            }
        }

        fn identity<T>(value: T): T {
            return value;
        }

        enum Maybe<T> {
            Just(T),
            Nothing,
        }

        impl<T> Maybe<T> {
            fn is_nothing(self): bool {
                return match self {
                    Maybe.Just(value) => false,
                    Maybe.Nothing => true,
                };
            }
        }

        fn main() {
            val p = Pair<i32>(identity<i32>(20), identity(22));
            val q = p.swapped();
            val empty: Maybe<i32> = Maybe.Nothing;
            if q.first + q.second != 42 { return 1; }
            if !empty.is_nothing() { return 2; }
            return 42;
        }
        "#,
    );
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn m15_generics_cross_module_signatures_and_trait() {
    let output = run_project(&[
        (
            "src/main.do",
            r#"
            use util.numbers;

            trait Measured {
                fn measure(self: *const Self): i32;
            }

            fn total(p: numbers.Pair<i32>): i32 {
                return p.first + p.second;
            }

            fn main() {
                val p = numbers.Pair<i32>(20, 22);
                return total(p) + numbers.static_zero();
            }
            "#,
        ),
        (
            "src/util/numbers.do",
            r#"
            pkg util;

            pub struct Pair<T> {
                pub first: T,
                pub second: T,
            }

            pub fn static_zero(): i32 {
                return 0;
            }
            "#,
        ),
    ])
    .expect("M15 cross-module generics should build");
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn m15_field_visibility_requires_pub_across_modules() {
    let private = build_project(&[
        (
            "src/main.do",
            "use geom.shapes; fn main() { val p = shapes.Point(1, 2); return p.x; }",
        ),
        (
            "src/geom/shapes.do",
            "pkg geom; pub struct Point { x: i32, y: i32 }",
        ),
    ])
    .unwrap_err();
    assert!(private.to_string().contains("is private"));

    let public = run_project(&[
        (
            "src/main.do",
            "use geom.shapes; fn main() { val p = shapes.Point(1, 41); return p.x + p.y; }",
        ),
        (
            "src/geom/shapes.do",
            "pkg geom; pub struct Point { pub x: i32, pub y: i32 }",
        ),
    ])
    .expect("public fields should be accessible across modules");
    assert_eq!(public.status.code(), Some(42));
}

#[test]
fn m15_type_parameter_bounds_resolve_associated_types() {
    let output = run_project(&[
        (
            "src/main.do",
            r#"
            use util.holding;
            use util.holding.Container;

            fn unwrap<C: Container>(value: C::Element): C::Element {
                return value;
            }

            fn main() {
                return unwrap<holding.Box>(42);
            }
            "#,
        ),
        (
            "src/util/holding.do",
            r#"
            pkg util;

            pub trait Container {
                type Element;
            }

            pub struct Box {
                pub value: i32,
            }

            impl Container for Box {
                type Element = i32;
            }
            "#,
        ),
    ])
    .expect("bounded associated type should build");
    assert_eq!(output.status.code(), Some(42));

    let mismatch = build_project(&[
        (
            "src/main.do",
            r#"
            use util.holding;
            fn main() {
                val b = holding.Box(1);
                return b.value;
            }
            "#,
        ),
        (
            "src/util/holding.do",
            r#"
            pkg util;
            pub trait Container { type Element; fn element(self: *const Self): Self::Element; }
            pub struct Box { pub value: i32 }
            impl Container for Box {
                type Element = i32;
                fn element(self: *const Self): bool { return true; }
            }
            "#,
        ),
    ])
    .unwrap_err();
    assert!(mismatch.to_string().contains("does not match"));
}

#[test]
fn m15_rejects_unknown_method_and_missing_trait_member() {
    let unknown = build_project(&[(
        "src/main.do",
        "struct S { x: i32 } fn main() { val s = S(1); return s.nope(); }",
    )])
    .unwrap_err();
    assert!(unknown.to_string().contains("has no method"));

    let missing = build_project(&[(
        "src/main.do",
        "trait T { fn f(self: *const Self): i32; } struct S { x: i32 } impl T for S { } fn main() { return 0; }",
    )])
    .unwrap_err();
    assert!(missing.to_string().contains("missing"));
}

#[test]
fn m15b_vec_push_get_set_pop_clone() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.collections.Vec;

        struct Point { x: i32, y: i32 }

        fn main() {
            var points = Vec<Point>::init();
            defer points.deinit();
            points.push(Point(1, 2));
            points.push(Point(3, 4));
            points.push(Point(5, 6));
            var sum = 0;
            for point in points.iter() {
                sum += point.x + point.y;
            }
            points.set(1_usize, Point(10, 20));
            val removed = points.pop();
            if removed.is_none() { return 99; }
            val first = points.get(0_usize);
            if first.is_none() { return 98; }
            val missing = points.get(9_usize);
            if missing.is_some() { return 97; }
            var copy = points.clone();
            defer copy.deinit();
            return sum + points.len() as i32 + copy.len() as i32;
        }
        "#,
    )])
    .expect("M15-B Vec program should build");
    // 元素和 1+2+3+4+5+6=21；pop 后 len=2；clone 后 len=2。
    assert_eq!(output.status.code(), Some(25));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("leaked"));
}

#[test]
fn m15b_vec_reserve_overflow_and_allocation_failure() {
    let overflow = run_project(&[(
        "src/main.do",
        r#"
        use std.collections.Vec;
        fn main() {
            var values = Vec<i32>::init();
            values.push(1);
            values.reserve(18446744073709551615_usize);
            return 0;
        }
        "#,
    )])
    .expect("overflow program should build");
    assert_eq!(overflow.status.code(), Some(102));

    let limited = run_project_with_profile(
        &[(
            "src/main.do",
            r#"
            use std.collections.Vec;
            fn main() {
                var values = Vec<i32>::init();
                defer values.deinit();
                values.push(1);
                return 0;
            }
            "#,
        )],
        BuildProfile::Debug,
        &[("DOLPHIN_TEST_ALLOC_LIMIT", "0")],
    )
    .expect("allocation-limited program should build");
    assert_eq!(limited.status.code(), Some(102));

    let reset = run_project(&[(
        "src/main.do",
        r#"
        use std.collections.Vec;
        fn main() {
            var values = Vec<i32>::with_capacity(0_usize);
            values.push(5);
            values.deinit();
            values.deinit();
            if values.is_empty() { return 0; }
            return 1;
        }
        "#,
    )])
    .expect("reset program should build");
    assert_eq!(reset.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&reset.stderr).contains("leaked"));
}

#[test]
fn m15b_vec_of_strings_manual_deinit() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.collections.Vec;
        use std.text;
        use std.text.String;

        fn main() {
            var texts = Vec<String>::init();
            defer texts.deinit();
            texts.push(text.concat("a", "b"));
            texts.push(text.concat("c", "d"));
            val count = texts.len();
            var index = 0_usize;
            while index < count {
                var item = texts.as_slice()[index];
                item.deinit();
                index += 1_usize;
            }
            return count as i32;
        }
        "#,
    )])
    .expect("Vec<String> program should build");
    assert_eq!(output.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("leaked"));
}

#[test]
fn m15b_vec_receiver_mutability_and_private_fields() {
    let immutable = build_project(&[(
        "src/main.do",
        "use std.collections.Vec; fn main() { val v = Vec<i32>::init(); v.push(1); return 0; }",
    )])
    .unwrap_err();
    assert!(immutable.to_string().contains("immutable receiver"));

    let private = build_project(&[(
        "src/main.do",
        "use std.collections.Vec; fn main() { val v = Vec<i32>::init(); return v.used; }",
    )])
    .unwrap_err();
    assert!(private.to_string().contains("is private"));

    let readonly = run_project(&[(
        "src/main.do",
        r#"
        use std.collections.Vec;
        fn main() {
            val values = Vec<i32>::init();
            return values.len() as i32
                + values.capacity() as i32
                + values.as_slice().len as i32;
        }
        "#,
    )])
    .expect("read-only Vec access should build");
    assert_eq!(readonly.status.code(), Some(0));
}

#[test]
fn m15b_text_and_cstring() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.text;
        use std.text.String;
        use std.ffi.CString;

        fn main() {
            var greeting = text.concat("hello, ", "Dolphin");
            defer greeting.deinit();
            var copy = greeting.clone();
            defer copy.deinit();
            var code = 0;
            if text.trim("  padded  ") == "padded" { code += 1; }
            val view = greeting.view();
            if view == "hello, Dolphin" { code += 1; }
            if text.starts_with(view, "hello") { code += 1; }
            if text.ends_with(view, "Dolphin") { code += 1; }
            if text.contains(view, "lo, Do") { code += 1; }
            val window = text.substring(view, 0_usize, 5_usize);
            if window.is_ok() { code += 1; }
            val boundary = text.substring("\u{4f60}\u{597d}", 0_usize, 1_usize);
            if boundary.is_err() { code += 1; }
            val valid = text.from_utf8(view.bytes());
            if valid.is_ok() { code += 1; }
            val converted = CString::from("with nul");
            if converted.is_ok() { code += 1; }
            val nul = CString::from("a\u{0}b");
            if nul.is_err() { code += 1; }
            var owned = match converted {
                Result.Ok(value) => value,
                Result.Err(error) => CString::empty(),
            };
            defer owned.deinit();
            return code;
        }
        "#,
    )])
    .expect("M15-B text program should build");
    assert_eq!(output.status.code(), Some(10));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("leaked"));
}

#[test]
fn m15b_iterator_protocol_and_rejection() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        struct Counter { current: i32, end: i32 }

        impl Iterator for Counter {
            type Item = i32;
            fn next(self: *Self): Option<i32> {
                if self->current >= self->end { return Option.None; }
                val value = self->current;
                self->current += 1;
                return Option.Some(value);
            }
        }

        fn total<I: Iterator>(items: I): i32 {
            var sum = 0;
            for value in items { sum += value; }
            return sum;
        }

        fn main() {
            return total(Counter(0, 4));
        }
        "#,
    )])
    .expect("user iterator program should build");
    assert_eq!(output.status.code(), Some(6));

    let duck = build_project(&[(
        "src/main.do",
        r#"
        struct Fake { current: i32 }
        impl Fake {
            fn next(self: *Self): Option<i32> { return Option.None; }
        }
        fn main() { var fake = Fake(0); for value in fake { } return 0; }
        "#,
    )])
    .unwrap_err();
    assert!(duck.to_string().contains("implement `Iterator`"));
}

#[test]
fn m15b_for_arrays_ranges_and_cleanup() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        fn make(): [i32; 3] { return [10, 20, 30]; }
        fn main() {
            var code = 0;
            for value in make() { code += value; }
            for index in 1..=3 { code += index; }
            for index in 3..3 { code += 100; }
            for value in [7, 8] {
                if value == 7 { continue; }
                code += value;
            }
            for value in [1, 2, 3] {
                if value == 2 { break; }
                code += value;
            }
            return code;
        }
        "#,
    )])
    .expect("array/range for program should build");
    // 60 + 6 + 0 + 8 + 1
    assert_eq!(output.status.code(), Some(75));

    let cleanup = run_project(&[(
        "src/main.do",
        r#"
        fn note(value: i32) { println("{}", value); }
        fn main() {
            for value in [1, 2, 3] {
                defer note(value);
                if value == 2 { break; }
            }
            return 0;
        }
        "#,
    )])
    .expect("cleanup program should build");
    assert_eq!(cleanup.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&cleanup.stdout), "1\n2\n");
}

#[test]
fn m15b_slice_iter_prelude_and_std_import() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std;

        fn main() {
            var total = 0;
            val text = "ABC";
            for byte in text.bytes() {
                total += byte as i32;
            }
            val some: Option<i32> = Option.Some(5);
            if some.is_some() { total += 1; }
            val result: Result<i32, i32> = Result.Ok(2);
            if result.is_ok() { total += 1; }
            return total;
        }
        "#,
    )])
    .expect("slice iterator + prelude program should build");
    // 65+66+67 + 1 + 1
    assert_eq!(output.status.code(), Some(200));
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

fn run_project_with_profile(
    files: &[(&str, &str)],
    profile: BuildProfile,
    environment: &[(&str, &str)],
) -> Result<std::process::Output, dolphin_compiler::Diagnostic> {
    let (project, artifact) = build_project_with_profile(files, profile)?;
    let mut command = Command::new(&artifact.executable);
    for (key, value) in environment {
        command.env(key, value);
    }
    let output = command.output().expect("generated executable should run");
    fs::remove_dir_all(project).expect("temporary project should be removed");
    Ok(output)
}

fn build_project(
    files: &[(&str, &str)],
) -> Result<dolphin_compiler::BuildArtifact, dolphin_compiler::Diagnostic> {
    build_project_with_path(files).map(|(_, artifact)| artifact)
}

fn build_project_with_profile(
    files: &[(&str, &str)],
    profile: BuildProfile,
) -> Result<(std::path::PathBuf, dolphin_compiler::BuildArtifact), dolphin_compiler::Diagnostic> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let project = std::env::temp_dir().join(format!(
        "dolphin-profile-test-{}-{unique}-{}",
        std::process::id(),
        NEXT_PROJECT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    for (relative, source) in files {
        let path = project.join(relative);
        fs::create_dir_all(path.parent().unwrap()).expect("source directory should be created");
        fs::write(path, source).expect("source file should be written");
    }
    match build_with_profile(
        BuildOptions {
            input: project.clone(),
            output: None,
        },
        profile,
        BuildSettings::default(),
    ) {
        Ok(artifact) => Ok((project, artifact)),
        Err(error) => {
            fs::remove_dir_all(project).expect("temporary project should be removed");
            Err(error)
        }
    }
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

// --- 回归测试：布局/代码生成与诊断修复 ---

#[test]
fn rejects_aggregate_equality() {
    let error = build_project(&[(
        "src/main.do",
        "struct P { x: i32, y: i32 } fn main() { val a = P(1, 2); val b = P(1, 3); if a == b { return 1; } return 0; }",
    )])
    .unwrap_err();
    assert!(error.to_string().contains("cannot be compared"));
}

#[test]
fn rejects_formatting_struct_value() {
    let error = build_project(&[(
        "src/main.do",
        "struct P { x: i32 } fn main() { val p = P(1); println(\"{}\", p); return 0; }",
    )])
    .unwrap_err();
    assert!(error.to_string().contains("cannot format"));
}

#[test]
fn rejects_non_integer_array_index_in_place() {
    let error = build_project(&[(
        "src/main.do",
        "fn main() { val a: [i32; 3] = [1, 2, 3]; val p = &a[true]; return *p; }",
    )])
    .unwrap_err();
    assert!(error.to_string().contains("expected `i32`"));
}

#[test]
fn rejects_oversized_array_type() {
    let error = build_project(&[(
        "src/main.do",
        "fn main() { val a: [i64; 536870913] = [0; 536870913]; return 0; }",
    )])
    .unwrap_err();
    assert!(error.to_string().contains("too large"));
}

#[test]
fn writes_aggregate_field_components_at_real_offsets() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        use std.mem;
        struct S { a: [i32; 2], b: i32 }
        fn main() {
            val p = mem.create<S>(S([0, 0], 0));
            defer mem.destroy(p);
            p->a = [10, 20];
            p->b = 3;
            println("{} {} {}", p->a[0], p->a[1], p->b);
            return 0;
        }
        "#,
    )])
    .expect("aggregate field assignment should build");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "10 20 3");
}

#[test]
fn bool_array_pointer_index_uses_element_stride() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        fn main() {
            val values: [bool; 3] = [true, false, true];
            val p = &values[2];
            println("{}", *p);
            return 0;
        }
        "#,
    )])
    .expect("bool array indexing should build");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "true");
}

#[test]
fn non_scalar_return_with_all_paths_returning() {
    let output = run_project(&[(
        "src/main.do",
        r#"
        struct P { x: i32, y: i32 }
        fn f(c: bool): P { if c { return P(1, 2); } else { return P(3, 4); } }
        fn main() {
            val p = f(true);
            println("{} {}", p.x, p.y);
            return 0;
        }
        "#,
    )])
    .expect("all-path-return struct function should build");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1 2");
}

#[test]
fn rejects_unbounded_generic_type_expansion() {
    let error = build_project(&[(
        "src/main.do",
        "use std.mem;\nstruct Bad<T> { next: Bad<Bad<T>> }\nfn main() { val size = mem.size_of<Bad<i32>>(); return 0; }",
    )])
    .unwrap_err();
    assert!(error.to_string().contains("too deep"));
}

#[test]
fn rejects_trait_and_type_name_collision() {
    let error = build_project(&[(
        "src/main.do",
        "struct Foo { x: i32 } trait Foo { fn f(self: *const Self): i32; } fn main() { return 0; }",
    )])
    .unwrap_err();
    assert!(error.to_string().contains("already defined"));
}

#[test]
fn unknown_multibyte_escape_reports_diagnostic() {
    let error =
        build_project(&[("src/main.do", "fn main() { val s = \"\\é\"; return 0; }")]).unwrap_err();
    assert!(error.to_string().contains("unknown string escape"));
}

#[test]
fn rejects_unescaped_quote_char_literal() {
    let error =
        build_project(&[("src/main.do", "fn main() { val c = '''; return 0; }")]).unwrap_err();
    assert!(error.to_string().contains("character literal"));
}

#[test]
fn submodule_type_shadows_root_type_of_same_name() {
    let output = run_project(&[
        (
            "src/main.do",
            r#"
            use sub.thing;
            struct Thing { a: i32 }
            fn main() {
                val t = thing.make();
                return t.b;
            }
            "#,
        ),
        (
            "src/sub/thing.do",
            "pkg sub;\n\npub struct Thing { pub b: i32 }\npub fn make(): Thing { return Thing(7); }\n",
        ),
    ])
    .expect("submodule type should shadow root type");
    assert_eq!(output.status.code(), Some(7));
}

#[test]
fn method_cannot_read_private_field_of_other_module_type() {
    let error = build_project(&[
        (
            "src/main.do",
            r#"
            use sub.thing;
            use sub.thing.Thing;
            trait Reveal { fn peek(self: *const Self): i32; }
            impl Reveal for Thing {
                fn peek(self: *const Self): i32 { return self->hidden; }
            }
            fn main() {
                val s = thing.make();
                return s.peek();
            }
            "#,
        ),
        (
            "src/sub/thing.do",
            "pkg sub;\n\npub struct Thing { hidden: i32 }\npub fn make(): Thing { return Thing(42); }\n",
        ),
    ])
    .unwrap_err();
    assert!(error.to_string().contains("private"));
}

#[test]
fn function_named_like_enum_variant_is_not_enum_construction() {
    let output = run_project(&[
        (
            "src/main.do",
            r#"
            use other.thing;
            enum MyE { foo(i32), Bar }
            fn main() {
                val x: MyE = MyE.Bar;
                val y = thing.foo();
                return y;
            }
            "#,
        ),
        (
            "src/other/thing.do",
            "pkg other;\npub fn foo(): i32 { return 99; }\n",
        ),
    ])
    .expect("imported function call should not become enum construction");
    assert_eq!(output.status.code(), Some(99));
}

/// H18-10：子模块（`pkg`）内用裸名构造本模块枚举项（`Enum.Variant(...)` 与
/// `Enum.Variant`）必须解析到本模块类型。修复前 `is_constructor_target` 只按
/// 未限定名查找类型表，子模块里的构造被误报为 `unknown function`。
#[test]
fn h18_10_enum_construction_in_submodule() {
    use support::backends;

    let files = [
        (
            "src/main.do",
            "use report.stats;\nfn main() { return stats.answer(); }",
        ),
        (
            "src/report/stats.do",
            r#"
            pkg report;

            pub enum Query { Found(i32), NotFound }

            pub fn make(x: i32): Query {
                if x > 0 {
                    return Query.Found(x);
                }
                return Query.NotFound;
            }

            pub fn answer(): i32 {
                val query = make(7);
                val value = match query {
                    Query.Found(score) => score,
                    Query.NotFound => -1,
                };
                return value;
            }
            "#,
        ),
    ];

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let project = std::env::temp_dir().join(format!(
        "dolphin-constructor-test-{}-{unique}-{}",
        std::process::id(),
        NEXT_PROJECT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    for (relative, source) in files {
        let path = project.join(relative);
        fs::create_dir_all(path.parent().unwrap()).expect("source directory should be created");
        fs::write(path, source).expect("source file should be written");
    }

    for backend in backends() {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let artifact = build_with_profile(
                BuildOptions {
                    input: project.clone(),
                    output: None,
                },
                profile,
                BuildSettings::with_backend(backend),
            )
            .unwrap_or_else(|error| {
                panic!("submodule enum construction must build ({backend:?}/{profile:?}): {error}")
            });
            let output = Command::new(&artifact.executable)
                .output()
                .expect("generated executable should run");
            assert_eq!(
                output.status.code(),
                Some(7),
                "backend={backend:?} profile={profile:?}"
            );
        }
    }
    fs::remove_dir_all(project).expect("temporary project should be removed");
}
