# M5：字符串与格式化输出

入口：[src/main.do](src/main.do)

本示例在 M4 基础上验证：

- 不可变 UTF-8 `string`
- 字符串局部变量、参数和返回值
- `print` 不换行输出
- `println` 换行输出
- 使用 `{}` 格式化 `i32`、`bool` 和 `string`
- 使用 `{{`、`}}` 输出字面量花括号
- 占位符数量的编译期检查
- 字符串转义和 Unicode 内容
- 随程序链接的最小输出运行时

格式化函数的第一个参数当前必须是字符串字面量。

构建和运行：

```bash
./target/release/dc build examples/m5
./examples/m5/target/m5
```

预期输出：

```text
Hello, 海豚!
count = 3, enabled = true
state = ready
escaped braces: {}
minimum i32 = -2147483648
```
