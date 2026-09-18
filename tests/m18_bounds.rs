//! H18-04 类型声明泛型约束回归（GEN-01..05）。
//!
//! 每个用例在可用后端（默认 Cranelift；`--features llvm` 时含 LLVM）×
//! Dolphin Debug/Release 上运行并断言固定 stdout 或诊断关键字。

mod support;

use support::{assert_rejected_messages, assert_runs};

const GEN_01: &str = r#"
trait Mark { fn mark(self: *const Self): i32; }
struct Box<T: Mark> { value: T }

fn main() {
    val b = Box<i32>(42);
    println("{}", b.value);
    return 0;
}
"#;

const GEN_02_POSITIVE: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
struct Hold<T: Has> { value: T }
enum Maybe<T: Has> { Some(T), None }

fn main() {
    val h = Hold<Good>(Good(1));
    val inner = h.value;
    val m: Maybe<Good> = Maybe.None;
    val v = match m {
        Maybe.Some(payload) => payload.value,
        Maybe.None => -1,
    };
    println("{} {}", inner.value, v);
    return 0;
}
"#;

const GEN_02_NEGATIVE_HOLD: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
struct Hold<T: Has> { value: T }

fn main() {
    val h = Hold<Bad>(Bad(1));
    return 0;
}
"#;

const GEN_02_NEGATIVE_MAYBE: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
enum Maybe<T: Has> { Some(T), None }

fn main() {
    val m: Maybe<Bad> = Maybe.None;
    return 0;
}
"#;

const GEN_03_POSITIVE: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
struct Hold<T: Has> { value: T }
struct Wrap<U: Has> { inner: Hold<U> }

fn sum(h: Hold<Good>): i32 {
    val inner = h.value;
    return inner.value;
}

fn main() {
    val w = Wrap<Good>(Hold<Good>(Good(5)));
    val hold = w.inner;
    val inner = hold.value;
    println("{} {}", sum(hold), inner.value);
    return 0;
}
"#;

const GEN_03_NEGATIVE_NESTED: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
struct Hold<T: Has> { value: T }
struct Wrap<U: Has> { inner: Hold<U> }

fn main() {
    val w = Wrap<Bad>(Hold<Bad>(Bad(1)));
    return 0;
}
"#;

const GEN_03_NEGATIVE_SIGNATURE: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
struct Hold<T: Has> { value: T }

fn bad(h: Hold<Bad>): i32 {
    return h.value.value;
}

fn main() {
    return 0;
}
"#;

const GEN_03_CROSS_MODULE_MAIN: &str = r#"
use util.holding;
use other.traits;

fn main() {
    val h = holding.Hold<holding.Good>(holding.Good(9));
    val inner = h.value;
    println("{}", inner.value);
    return 0;
}
"#;

const GEN_03_CROSS_MODULE_NEGATIVE: &str = r#"
use util.holding;

fn main() {
    val h = holding.Hold<holding.Bad>(holding.Bad(1));
    return 0;
}
"#;

const GEN_03_CROSS_MODULE_UTIL: &str = r#"
pkg util;

pub trait Has { type Item; }
pub struct Good { pub value: i32 }
pub struct Bad { pub value: i32 }
impl Has for Good { type Item = i32; }
pub struct Hold<T: Has> { pub value: T }
pub enum Maybe<T: Has> { Some(T), None }
"#;

const GEN_03_CROSS_MODULE_OTHER: &str = r#"
pkg other;

pub trait Has { type Item; }
"#;

const GEN_05_POSITIVE: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
struct ItemBox<T: Has> { value: T::Item }
enum ItemMaybe<T: Has> { Some(T::Item), None }

fn main() {
    val b = ItemBox<Good>(1);
    val m: ItemMaybe<Good> = ItemMaybe.None;
    val some = ItemMaybe<Good>.Some(2);
    val none_value = match m {
        ItemMaybe.Some(item) => item,
        ItemMaybe.None => -1,
    };
    val some_value = match some {
        ItemMaybe.Some(item) => item,
        ItemMaybe.None => -1,
    };
    println("{} {} {}", b.value, none_value, some_value);
    return 0;
}
"#;

const GEN_05_NEGATIVE_BAD_TYPE: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
struct ItemBox<T: Has> { value: T::Item }

fn main() {
    val b = ItemBox<Good>(true);
    return 0;
}
"#;

const GEN_05_NEGATIVE_BAD_IMPL_BOX: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
struct ItemBox<T: Has> { value: T::Item }

fn main() {
    val b = ItemBox<Bad>(1);
    return 0;
}
"#;

const GEN_05_NEGATIVE_BAD_IMPL_ENUM: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
enum ItemMaybe<T: Has> { Some(T::Item), None }

fn main() {
    val m: ItemMaybe<Bad> = ItemMaybe.None;
    return 0;
}
"#;

#[test]
fn gen_01_ignored_type_bound_is_rejected() {
    assert_rejected_messages(GEN_01, &["i32", "Mark", "cannot implement", "main.do"]);
}

#[test]
fn gen_02_positive_hold_and_maybe() {
    assert_runs(&[("src/main.do", GEN_02_POSITIVE)], "1 -1\n", 0);
}

#[test]
fn gen_02_negative_hold_and_maybe() {
    assert_rejected_messages(
        GEN_02_NEGATIVE_HOLD,
        &["Bad", "Has", "does not implement", "main.do"],
    );
    assert_rejected_messages(
        GEN_02_NEGATIVE_MAYBE,
        &["Bad", "Has", "does not implement", "main.do"],
    );
}

#[test]
fn gen_03_positive_signature_and_nested() {
    assert_runs(&[("src/main.do", GEN_03_POSITIVE)], "5 5\n", 0);
}

#[test]
fn gen_03_negative_nested_and_signature() {
    assert_rejected_messages(
        GEN_03_NEGATIVE_NESTED,
        &["Bad", "Has", "does not implement", "main.do"],
    );
    assert_rejected_messages(
        GEN_03_NEGATIVE_SIGNATURE,
        &["Bad", "Has", "does not implement", "main.do"],
    );
}

#[test]
fn gen_03_cross_module_trait_identity() {
    let files = [
        ("src/main.do", GEN_03_CROSS_MODULE_MAIN),
        ("src/util/holding.do", GEN_03_CROSS_MODULE_UTIL),
        ("src/other/traits.do", GEN_03_CROSS_MODULE_OTHER),
    ];
    assert_runs(&files, "9\n", 0);
}

#[test]
fn gen_03_cross_module_negative_uses_qualified_trait() {
    let files = [
        ("src/main.do", GEN_03_CROSS_MODULE_NEGATIVE),
        ("src/util/holding.do", GEN_03_CROSS_MODULE_UTIL),
        ("src/other/traits.do", GEN_03_CROSS_MODULE_OTHER),
    ];
    for backend in support::backends() {
        for profile in [
            dolphin_compiler::BuildProfile::Debug,
            dolphin_compiler::BuildProfile::Release,
        ] {
            let result = support::run_project(backend, profile, &files, support::DEFAULT_TIMEOUT);
            let error = result
                .build_error
                .clone()
                .unwrap_or_else(|| panic!("expected rejection: {}", result.context()));
            for needle in ["util.holding.Bad", "util.holding.Has", "does not implement"] {
                assert!(
                    error.contains(needle),
                    "expected `{needle}` in diagnostic: {error}"
                );
            }
        }
    }
}

#[test]
fn gen_05_associated_type_fields_and_payloads() {
    assert_runs(&[("src/main.do", GEN_05_POSITIVE)], "1 -1 2\n", 0);
}

#[test]
fn gen_05_negative_type_and_missing_impl() {
    assert_rejected_messages(GEN_05_NEGATIVE_BAD_TYPE, &["expected `i32`", "bool"]);
    assert_rejected_messages(GEN_05_NEGATIVE_BAD_IMPL_BOX, &["Bad", "Has"]);
    assert_rejected_messages(GEN_05_NEGATIVE_BAD_IMPL_ENUM, &["Bad", "Has"]);
}
