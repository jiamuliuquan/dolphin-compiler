// M16 后端基准程序：计算密集循环 + 递归，用于对比 Cranelift 与 LLVM。
//
// 同一份源码在 `DOLPHIN_BACKEND=cranelift` 与 `DOLPHIN_BACKEND=llvm` 下构建，
// 比较 Debug 编译速度、Release 运行性能与产物体积。见 `scripts/bench.py`。

fn fib(n: i64): i64 {
    if n < 2_i64 {
        return n;
    }
    return fib(n - 1_i64) + fib(n - 2_i64);
}

fn main() {
    var total: i64 = 0_i64;
    var i: i64 = 0_i64;
    while i < 100000000_i64 {
        total += (i % 7_i64) * (i % 13_i64);
        i += 1_i64;
    }
    val f = fib(35_i64);
    println("sum = {}, fib = {}", total, f);
    return 0;
}
