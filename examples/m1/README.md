# M1：整数算术

入口：[src/main.dc](src/main.dc)

本示例验证第一条包含实际计算的完整编译链路：

- `fn main()` 程序入口
- `i32` 十进制整数字面量
- `+`、`-`、`*`、`/`、`%`
- 一元负号和括号
- 运算符优先级
- `return` 返回进程退出码
- 单行与多行注释

构建和运行：

```bash
./target/release/dc build examples/m1
./examples/m1/target/m1
echo $?
```

表达式结果和预期退出码为 `28`。M1 尚不支持变量和输出，所以通过进程退出码观察结果。
