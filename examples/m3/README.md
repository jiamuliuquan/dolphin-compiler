# M3：分支与循环

入口：[src/main.do](src/main.do)

本示例在 M2 基础上验证：

- `bool`、`true`、`false` 和一元 `!`
- 算术比较与 `==`、`!=`
- `&&`、`||` 短路求值
- `if/else`
- `while` 条件循环
- `loop` 无限循环
- `break` 和 `continue`
- 块级作用域

示例中的 `false && 1 / 0 == 0` 不会执行右侧除零表达式，用于证明逻辑与确实短路。

构建和运行：

```bash
./target/release/dc build examples/m3
./examples/m3/target/m3
echo $?
```

循环计算 `1 + 3 + 5 + 7 + 9 = 25`，再加上 `loop` 的三次计数，预期退出码为 `28`。
