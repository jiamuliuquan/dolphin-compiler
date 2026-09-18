//! H18-02 枚举布局与内存步长回归（LAYOUT-01..06）。
//!
//! 每个用例在可用后端（默认 Cranelift；`--features llvm` 时含 LLVM）×
//! Dolphin Debug/Release 上运行，并断言固定 stdout/exit/stderr。

mod support;

use dolphin_compiler::BuildProfile;
use support::{DEFAULT_TIMEOUT, assert_rejected, assert_runs, assert_traps, backends, run_project};

const LAYOUT_01: &str = r#"
use std.mem;

enum Empty { A, B }
enum Payload { Value(i64), Empty }
enum Multi { Pair(i32, i64), Triple(f64, f64, f64), None }

fn main() {
    println("{} {}", mem.size_of<Empty>(), mem.align_of<Empty>());
    println("{} {}", mem.size_of<Payload>(), mem.align_of<Payload>());
    println("{} {}", mem.size_of<Multi>(), mem.align_of<Multi>());
    return 0;
}
"#;

const LAYOUT_02: &str = r#"
use std.mem;

enum Slot { Value(i32), Empty }

fn main() {
    val items = mem.alloc<Slot>(3);
    items[0] = Slot.Value(10);
    items[1] = Slot.Empty;
    items[2] = Slot.Value(30);

    val second = &items[1];
    val initial = match (*second) {
        Slot.Value(v) => v,
        Slot.Empty => -1,
    };

    items[1] = Slot.Value(20);
    val updated = match items[1] {
        Slot.Value(v) => v,
        Slot.Empty => -1,
    };
    val third = match items[2] {
        Slot.Value(v) => v,
        Slot.Empty => -1,
    };

    println("{} {} {}", initial, updated, third);
    mem.free(items);
    return 0;
}
"#;

const LAYOUT_03: &str = r#"
use std.mem;

enum Shape { Circle(f64), Dot }
struct Holder { head: i32, shape: Shape, tail: i32 }
enum Wrapper { H(Holder), Empty }

fn main() {
    val holder = mem.create<Holder>(Holder(7, Shape.Circle(2.5), 9));
    defer mem.destroy(holder);

    val wrapped = Wrapper.H(Holder(1, Shape.Dot, 3));
    val inner_sum = match wrapped {
        Wrapper.H(inner) => inner.head + inner.tail,
        Wrapper.Empty => -1,
    };
    val area = match holder->shape {
        Shape.Circle(r) => r,
        Shape.Dot => 0.0,
    };

    println("{} {} {} {}", mem.size_of<Holder>(), mem.align_of<Holder>(), inner_sum, area);
    return 0;
}
"#;

const LAYOUT_04: &str = r#"
use std;
use std.mem;

struct Point { x: i32, y: i32 }

enum Payloads {
    Text(string),
    Ptr(*Point),
    Struct(Point),
    Opt(Option<i32>),
    Res(Result<i32, i32>),
    Blank,
}

fn score(value: Payloads): i32 {
    return match value {
        Payloads.Text(text) => length(text) as i32,
        Payloads.Ptr(p) => p->x + p->y,
        Payloads.Struct(p) => p.x + p.y,
        Payloads.Opt(maybe) => match maybe {
            Option.Some(v) => v,
            Option.None => 0,
        },
        Payloads.Res(result) => match result {
            Result.Ok(v) => v,
            Result.Err(e) => -e,
        },
        Payloads.Blank => 100,
    };
}

fn main() {
    var point = Point(4, 5);
    val checks = score(Payloads.Text("abc"))
        + score(Payloads.Ptr(&point))
        + score(Payloads.Struct(Point(6, 7)))
        + score(Payloads.Opt(Option.Some(8)))
        + score(Payloads.Res(Result.Err(9)))
        + score(Payloads.Blank);

    val buffer = mem.alloc<Payloads>(2);
    buffer[0] = Payloads.Struct(Point(10, 20));
    buffer[1] = Payloads.Blank;
    val copy = mem.alloc<Payloads>(2);
    mem.copy(copy, buffer);
    val copied = match copy[0] {
        Payloads.Struct(p) => p.x + p.y,
        Payloads.Text(text) => length(text) as i32,
        Payloads.Ptr(p) => p->x + p->y,
        Payloads.Opt(_) => 0,
        Payloads.Res(_) => 0,
        Payloads.Blank => -1,
    };
    val blank_copied = match copy[1] {
        Payloads.Blank => 1,
        Payloads.Text(_) => 0,
        Payloads.Ptr(_) => 0,
        Payloads.Struct(_) => 0,
        Payloads.Opt(_) => 0,
        Payloads.Res(_) => 0,
    };
    mem.free(copy);
    mem.free(buffer);

    println("{} {} {}", checks, copied, blank_copied);
    return 0;
}
"#;

const LAYOUT_05_FREE: &str = r#"
use std.mem;

enum Slot { Value(i64), Empty }

fn main() {
    val items = mem.alloc<Slot>(5);
    var index = 0;
    while index < 5 {
        items[index as usize] = Slot.Value(index as i64);
        index += 1;
    }
    items[2] = Slot.Empty;

    val copy = mem.alloc<Slot>(5);
    mem.copy(copy, items);
    val last = match copy[4] {
        Slot.Value(v) => v as i32,
        Slot.Empty => -1,
    };
    mem.free(copy);
    mem.free(items);

    println("{}", last);
    return 0;
}
"#;

const LAYOUT_05_BOUNDS: &str = r#"
use std.mem;

enum Slot { Value(i64), Empty }

fn main() {
    val items = mem.alloc<Slot>(2);
    var index = 2;
    items[index as usize] = Slot.Empty;
    mem.free(items);
    return 0;
}
"#;

const LAYOUT_06_OVERSIZED: &str = r#"
use std.mem;

struct Chunk { bytes: [i32; 536870911] }
struct DoubleChunk { first: Chunk, second: Chunk }
enum Huge { V(DoubleChunk), Empty }

fn main() {
    val size = mem.size_of<Huge>();
    println("{}", size);
    return 0;
}
"#;

#[test]
fn layout_01_enum_sizes_and_alignments() {
    assert_runs(&[("src/main.do", LAYOUT_01)], "4 4\n16 8\n32 8\n", 0);

    for backend in backends() {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let result = run_project(
                backend,
                profile,
                &[("src/main.do", LAYOUT_01)],
                DEFAULT_TIMEOUT,
            );
            assert!(
                result.build_error.is_none(),
                "build failed: {}",
                result.context()
            );
            for line in result.stdout.lines() {
                let mut parts = line.split_whitespace();
                let size: u32 = parts.next().expect("size").parse().expect("size number");
                let align: u32 = parts.next().expect("align").parse().expect("align number");
                if size != 0 {
                    assert_eq!(
                        size % align,
                        0,
                        "size must be a multiple of align: {}",
                        result.context()
                    );
                }
            }
        }
    }
}

#[test]
fn layout_02_enum_slice_alloc_address_and_write() {
    assert_runs(&[("src/main.do", LAYOUT_02)], "-1 20 30\n", 0);
}

#[test]
fn layout_03_enum_in_struct_and_struct_in_enum() {
    assert_runs(&[("src/main.do", LAYOUT_03)], "32 8 4 2.5\n", 0);
}

#[test]
fn layout_04_payloads_match_params_returns_and_copy() {
    assert_runs(&[("src/main.do", LAYOUT_04)], "124 30 1\n", 0);
}

#[test]
fn layout_05_free_full_slice_without_mismatch() {
    assert_runs(&[("src/main.do", LAYOUT_05_FREE)], "4\n", 0);
}

#[test]
fn layout_05_dynamic_bounds_trap_in_both_profiles() {
    assert_traps(LAYOUT_05_BOUNDS);
}

#[test]
fn layout_06_rejects_oversized_nested_aggregate() {
    assert_rejected(LAYOUT_06_OVERSIZED, "too large");
}
