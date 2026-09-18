# M18：组合语义回归

演示 M18 修复后的组合行为：

- 泛型容器 `Box<T>` 与泛型枚举 `Maybe<T>`，`match` 解构与关联方法；
- 指针别名写：`pair.x = mutate(&pair)` 中 RHS 通过指针修改 `pair.y`，简单字段赋值只写目标分量（H18-01）；
- 跨函数聚合：`Pair` 按值传参与返回（聚合 sret 路径）；
- 源码标准库 `Vec<i32>` 与 `for` 迭代，`defer numbers.deinit()` 清理；
- `mem.alloc` / `mem.free` + `defer`，切片跨函数求和（布局与步长，H18-02）。

## 运行

```bash
./target/release/dc build examples/m18
./examples/m18/target/m18
```

输出：

```text
alias = 7 9
generic = 40
shifted = 8 10
sum = 12
```

退出码 `0`；Cranelift 与 LLVM、Debug 与 Release 输出一致，Debug 下 stderr 为空（无泄漏报告）。
