//! H18-05 impl 头与不支持语法回归（IMPL-01..04）。
//!
//! 每个用例在可用后端（默认 Cranelift；`--features llvm` 时含 LLVM）×
//! Dolphin Debug/Release 上运行并断言固定 stdout 或诊断关键字。

mod support;

use support::{assert_rejected_messages, assert_runs};

const IMPL_01_CONSTRUCT_THEN_METHOD: &str = r#"
struct Box<T> { value: T }

impl<U> Box<U> {
    fn get(self): U { return self.value; }
}

fn main() {
    val b = Box<i32>(7);
    println("{}", b.get());
    return 0;
}
"#;

const IMPL_01_ASSOCIATED_FIRST: &str = r#"
struct Box<T> { value: T }

impl<U> Box<U> {
    fn make(value: U): Self { return Box<U>(value); }
}

fn main() {
    val b = Box<i32>::make(7);
    println("{}", b.value);
    return 0;
}
"#;

const IMPL_01_BOUND_POSITIVE: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
struct Box<T: Has> { value: T }

impl<U> Box<U> {
    fn get(self): U { return self.value; }
}

fn main() {
    val b = Box<Good>(Good(3));
    val inner = b.get();
    println("{}", inner.value);
    return 0;
}
"#;

const IMPL_01_BOUND_NEGATIVE: &str = r#"
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
struct Box<T: Has> { value: T }

impl<U> Box<U> {
    fn make(value: U): Self { return Box<U>(value); }
}

fn main() {
    val b = Box<Bad>::make(Bad(1));
    return 0;
}
"#;

const IMPL_02_SPECIALIZATION: &str = r#"
struct Box<T> { value: T }
impl Box<i32> {}
fn main() { return 0; }
"#;

const IMPL_02_MISSING_ARGUMENT: &str = r#"
struct Box<T> { value: T }
impl<T> Box {}
fn main() { return 0; }
"#;

const IMPL_02_REPEATED_ARGUMENT: &str = r#"
struct Box<T> { value: T }
impl<T> Box<T, T> {}
fn main() { return 0; }
"#;

const IMPL_02_REORDERED: &str = r#"
struct Pair<A, B> { first: A, second: B }
impl<A, B> Pair<B, A> {}
fn main() { return 0; }
"#;

const IMPL_02_ARITY_MISMATCH: &str = r#"
struct Pair<A, B> { first: A, second: B }
impl<T> Pair<T, T> {}
fn main() { return 0; }
"#;

const IMPL_02_NESTED: &str = r#"
struct Box<T> { value: T }
impl<T> Box<Box<T>> {}
fn main() { return 0; }
"#;

const IMPL_02_BLANKET: &str = r#"
trait Has { type Item; }
impl<T> Has for T { type Item = i32; }
fn main() { return 0; }
"#;

const IMPL_02_IMPL_BOUND: &str = r#"
trait Has { type Item; }
struct Box<T> { value: T }
impl<T: Has> Box<T> {}
fn main() { return 0; }
"#;

const IMPL_02_METHOD_TYPE_PARAM: &str = r#"
struct Box<T> { value: T }
impl<T> Box<T> {
    fn id<V>(value: V): V { return value; }
}
fn main() { return 0; }
"#;

const IMPL_02_GENERIC_TRAIT_ARGUMENT: &str = r#"
trait Container<T> { }
struct Box<T> { value: T }
impl<U> Container<U> for Box<U> {}
fn main() { return 0; }
"#;

const IMPL_02_UNKNOWN_TARGET: &str = r#"
impl Missing {}
fn main() { return 0; }
"#;

const IMPL_03_DUPLICATE_METHOD: &str = r#"
struct S { x: i32 }
impl S { fn f(self): i32 { return 1; } }
impl S { fn f(self): i32 { return 2; } }
fn main() { return 0; }
"#;

const IMPL_03_TRAIT_METHOD_CONFLICT: &str = r#"
trait T { fn f(self: *const Self): i32; }
struct S { x: i32 }
impl S { fn f(self: *const Self): i32 { return 1; } }
impl T for S { fn f(self: *const Self): i32 { return 2; } }
fn main() { return 0; }
"#;

#[test]
fn impl_01_construct_then_call_renamed_params() {
    assert_runs(&[("src/main.do", IMPL_01_CONSTRUCT_THEN_METHOD)], "7\n", 0);
}

#[test]
fn impl_01_associated_function_first_instantiation() {
    assert_runs(&[("src/main.do", IMPL_01_ASSOCIATED_FIRST)], "7\n", 0);
}

#[test]
fn impl_01_bound_checked_through_method_instantiation() {
    assert_runs(&[("src/main.do", IMPL_01_BOUND_POSITIVE)], "3\n", 0);
    assert_rejected_messages(
        IMPL_01_BOUND_NEGATIVE,
        &["Bad", "Has", "does not implement", "main.do"],
    );
}

#[test]
fn impl_02_specialization_is_rejected() {
    assert_rejected_messages(
        IMPL_02_SPECIALIZATION,
        &["concrete impl target", "not supported"],
    );
}

#[test]
fn impl_02_missing_and_repeated_arguments_are_rejected() {
    assert_rejected_messages(IMPL_02_MISSING_ARGUMENT, &["expects 1 type arguments"]);
    assert_rejected_messages(IMPL_02_REPEATED_ARGUMENT, &["expects 1 type arguments"]);
}

#[test]
fn impl_02_reordered_and_arity_mismatch_are_rejected() {
    assert_rejected_messages(
        IMPL_02_REORDERED,
        &["impl target", "type parameters in order"],
    );
    assert_rejected_messages(IMPL_02_ARITY_MISMATCH, &["impl for `Pair` declares 1"]);
}

#[test]
fn impl_02_nested_argument_is_rejected() {
    assert_rejected_messages(IMPL_02_NESTED, &["impl target", "type parameters in order"]);
}

#[test]
fn impl_02_blanket_impl_is_rejected() {
    assert_rejected_messages(IMPL_02_BLANKET, &["blanket impl"]);
}

#[test]
fn impl_02_impl_parameter_bound_is_rejected() {
    assert_rejected_messages(IMPL_02_IMPL_BOUND, &["bounds on impl type parameters"]);
}

#[test]
fn impl_02_method_type_parameter_is_rejected() {
    assert_rejected_messages(
        IMPL_02_METHOD_TYPE_PARAM,
        &["cannot declare its own type parameters"],
    );
}

#[test]
fn impl_02_generic_trait_argument_is_rejected() {
    assert_rejected_messages(IMPL_02_GENERIC_TRAIT_ARGUMENT, &["generic trait arguments"]);
}

#[test]
fn impl_02_unknown_target_is_rejected() {
    assert_rejected_messages(IMPL_02_UNKNOWN_TARGET, &["unknown type `Missing`"]);
}

#[test]
fn impl_03_same_name_method_conflict_is_rejected() {
    assert_rejected_messages(IMPL_03_DUPLICATE_METHOD, &["already defined"]);
    assert_rejected_messages(IMPL_03_TRAIT_METHOD_CONFLICT, &["already defined"]);
}
