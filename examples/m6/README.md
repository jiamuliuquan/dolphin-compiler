# M6：定长数组、范围与 `for`

入口：[src/main.dc](src/main.dc)

本示例在 M5 基础上验证：

- 定长数组类型 `[T; N]`
- 数组字面量 `[1, 2, 3]` 与重复初始化 `[value; N]`
- 下标读取、普通赋值和 `i32` 复合赋值
- 数组作为局部变量、函数参数和返回值
- `for value in array` 数组遍历
- 半开范围 `start..end` 与闭合范围 `start..=end`
- `for` 中的 `continue`
- `val` 数组只读语义

数组当前按值传递，只支持一维非空数组；M8 起元素可以是任意基础标量类型。下标越界会在运行时终止程序。

构建和运行：

```bash
./target/release/dc build examples/m6
./examples/m6/target/m6
echo $?
```

预期输出：

```text
numbers = 1, 10, 8, 4
total = 43
```

预期退出码为 `43`。
