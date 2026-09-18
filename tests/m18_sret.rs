//! H18-06 回归：循环内聚合返回值调用不得随迭代累积栈（sret 槽提升到入口块）。
//!
//! 1e6 次迭代在 Debug LLVM 下若每次 alloca 24 字节会超过 8 MiB 默认栈并崩溃；
//! Debug/Release 与 Cranelift/LLVM 都必须输出固定结果。

mod support;

use support::assert_runs;

const SRET_LOOP: &str = r#"
struct Big { a: i64, b: i64, c: i64 }

fn make(value: i64): Big {
    return Big(value, value + 1_i64, value + 2_i64);
}

fn main() {
    var total = 0_i64;
    var i = 0;
    while i < 1000000 {
        val item = make(i as i64);
        total += item.a;
        i += 1;
    }
    println("{}", total);
    return 0;
}
"#;

#[test]
fn loop_sret_alloca_does_not_accumulate_stack() {
    assert_runs(&[("src/main.do", SRET_LOOP)], "499999500000\n", 0);
}
