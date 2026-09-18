//! H18-01 聚合写入与求值顺序回归（PLACE-01..08）。
//!
//! 每个用例在可用后端（默认 Cranelift；`--features llvm` 时含 LLVM）×
//! Dolphin Debug/Release 上运行，并断言固定 stdout/exit/stderr。

mod support;

use support::{assert_rejected, assert_runs, assert_traps_without_stdout};

const PLACE_01: &str = r#"
struct Pair { x: i32, y: i32 }

fn mutate(p: *Pair): i32 {
    p->y = 9;
    return 7;
}

fn main() {
    var p = Pair(1, 2);
    p.x = mutate(&p);
    println("{} {}", p.x, p.y);
    return 0;
}
"#;

const PLACE_02: &str = r#"
use std.mem;

fn modify_element(values: []i32): i32 {
    values[1] = 20;
    return 7;
}

fn main() {
    var array: [i32; 3] = [1, 2, 3];
    val view = mem.view<i32>(&array[0], 3_usize);
    array[0] = modify_element(view);
    println("{} {} {}", array[0], array[1], array[2]);
    return 0;
}
"#;

const PLACE_03: &str = r#"
struct Counter { value: i32 }

fn next(c: *Counter): i32 {
    c->value = c->value + 1;
    println("index {}", c->value - 1);
    return c->value - 1;
}

fn right(tag: i32): i32 {
    println("rhs {}", tag);
    return tag;
}

fn main() {
    var values: [i32; 3] = [10, 20, 30];
    var counter = Counter(0);
    values[next(&counter)] += right(5);
    values[next(&counter)] = right(99);
    println("{} {} {} counter={}", values[0], values[1], values[2], counter.value);
    return 0;
}
"#;

const PLACE_04: &str = r#"
struct Pair { x: i32, y: i32 }

fn both(p: *Pair): i32 {
    println("rhs set target");
    p->x = 100;
    println("rhs set other");
    p->y = 200;
    return 7;
}

fn main() {
    var a = Pair(1, 2);
    a.x = both(&a);
    println("{} {}", a.x, a.y);

    var b = Pair(10, 20);
    b.x += both(&b);
    println("{} {}", b.x, b.y);
    return 0;
}
"#;

const PLACE_06: &str = r#"
use std.mem;

struct Inner { a: i32, b: i32 }
struct Outer { inner: Inner, arr: [i32; 2] }

fn touch(values: []i32): i32 {
    values[1] = 20;
    return 7;
}

fn main() {
    val heap = mem.alloc<i32>(3);
    heap[0] = 1;
    heap[1] = 2;
    heap[2] = 3;
    heap[0] = touch(heap);

    var o = Outer(Inner(1, 2), [3, 4]);
    o.inner = Inner(5, 6);
    o.arr = [10, 20];

    val p = mem.create<Outer>(o);
    p->inner = Inner(30, 40);
    p->arr = [50, 60];

    println("{} {} {} {} {} {} {}", heap[0], heap[1], heap[2], o.inner.a, o.inner.b, o.arr[0], o.arr[1]);
    println("{} {} {} {}", p->inner.a, p->inner.b, p->arr[0], p->arr[1]);

    mem.destroy(p);
    mem.free(heap);
    return 0;
}
"#;

const PLACE_08_POINTER: &str = r#"
use std.mem;

struct Pair { x: i32, y: i32 }

fn rebind(pp: **Pair, other: *Pair): i32 {
    val view = mem.view<*Pair>(pp, 1_usize);
    view[0] = other;
    return 7;
}

fn main() {
    var a = Pair(1, 2);
    var b = Pair(100, 200);
    var p = &a;
    p->x = rebind(&p, &b);
    println("{} {} {} {}", a.x, a.y, b.x, b.y);

    var c = Pair(10, 20);
    var q = &c;
    q->y += rebind(&q, &b);
    println("{} {} {} {}", c.x, c.y, b.x, b.y);
    return 0;
}
"#;

const PLACE_08_SLICE: &str = r#"
use std.mem;

fn rebind_slice(desc: *[]i32, other: []i32): i32 {
    val cells = mem.view<[]i32>(desc, 1_usize);
    cells[0] = other;
    return 7;
}

fn main() {
    var a: [i32; 2] = [1, 2];
    var b: [i32; 2] = [100, 200];
    var s = mem.view<i32>(&a[0], 2_usize);
    val other = mem.view<i32>(&b[0], 2_usize);
    s[0] = rebind_slice(&s, other);
    println("{} {} {} {}", a[0], a[1], b[0], b[1]);
    return 0;
}
"#;

const PLACE_07_SIMPLE: &str = r#"
fn marker(): i32 {
    println("RHS-RAN");
    return 7;
}

fn main() {
    var values: [i32; 2] = [1, 2];
    var index = 2;
    values[index] = marker();
    return 0;
}
"#;

const PLACE_07_COMPOUND: &str = r#"
fn marker(): i32 {
    println("RHS-RAN");
    return 7;
}

fn main() {
    var values: [i32; 2] = [1, 2];
    var index = 2;
    values[index] += marker();
    return 0;
}
"#;

#[test]
fn place_01_field_alias_keeps_rhs_side_effect() {
    assert_runs(&[("src/main.do", PLACE_01)], "7 9\n", 0);
}

#[test]
fn place_02_array_rhs_element_alias_preserved() {
    assert_runs(&[("src/main.do", PLACE_02)], "7 20 3\n", 0);
}

#[test]
fn place_03_index_target_evaluated_once() {
    assert_runs(
        &[("src/main.do", PLACE_03)],
        "index 0\nrhs 5\nindex 1\nrhs 99\n15 99 30 counter=2\n",
        0,
    );
}

#[test]
fn place_04_rhs_target_and_other_field() {
    assert_runs(
        &[("src/main.do", PLACE_04)],
        "rhs set target\nrhs set other\n7 200\nrhs set target\nrhs set other\n17 200\n",
        0,
    );
}

#[test]
fn place_05_rejects_immutable_and_out_of_bounds_targets() {
    assert_rejected(
        "struct Pair { x: i32, y: i32 } fn main() { val p = Pair(1, 2); p.x = 3; return 0; }",
        "immutable variable",
    );
    assert_rejected(
        "fn main() { val a: [i32; 2] = [1, 2]; a[0] = 3; return 0; }",
        "cannot modify immutable array",
    );
    assert_rejected(
        "struct Pair { x: i32, y: i32 } fn main() { val p = Pair(1, 2); val ptr = &p; ptr->x = 9; return 0; }",
        "*const",
    );
    assert_rejected(
        "fn main() { val text = \"abc\"; text[0] = 65_u8; return 0; }",
        "cannot index value of type `string`",
    );
    assert_rejected(
        "fn main() { var a: [i32; 2] = [1, 2]; a[5] = 7; return 0; }",
        "array index is out of bounds",
    );
}

#[test]
fn place_05_dynamic_out_of_bounds_traps() {
    assert_traps_without_stdout(
        "fn main() { var values: [i32; 2] = [1, 2]; var index = 2; values[index] = 7; return 0; }",
        "7",
    );
}

#[test]
fn place_06_pointer_slice_and_aggregate_fields() {
    assert_runs(
        &[("src/main.do", PLACE_06)],
        "7 20 3 5 6 10 20\n30 40 50 60\n",
        0,
    );
}

#[test]
fn place_07_bounds_check_before_rhs() {
    assert_traps_without_stdout(PLACE_07_SIMPLE, "RHS-RAN");
    assert_traps_without_stdout(PLACE_07_COMPOUND, "RHS-RAN");
}

#[test]
fn place_08_pointer_rebind_uses_pre_rhs_address() {
    assert_runs(
        &[("src/main.do", PLACE_08_POINTER)],
        "7 2 100 200\n10 27 100 200\n",
        0,
    );
}

#[test]
fn place_08_slice_descriptor_rebind_uses_pre_rhs_address() {
    assert_runs(&[("src/main.do", PLACE_08_SLICE)], "7 2 100 200\n", 0);
}
