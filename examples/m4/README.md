# M4：函数与调用

入口：[src/main.do](src/main.do)

本示例在 M3 基础上验证：

- 同一文件定义多个函数
- 必须标注类型的函数参数
- `: i32`、`: bool` 等返回类型
- 无返回类型的 `Unit` 函数
- 参数数量和类型检查
- 函数定义顺序不影响调用
- 递归调用
- 非 `main` 函数的完整返回路径检查

`main` 在 `factorial` 之前调用它，证明编译器会先收集全部函数签名。`factorial` 递归计算 `5!`。

构建和运行：

```bash
./target/release/dc build examples/m4
./examples/m4/target/m4
echo $?
```

预期退出码为 `120`。
