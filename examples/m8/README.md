# M8：MVP 标量类型与命令体验

入口：[src/main.dc](src/main.dc)

本示例验证：

- `i8`、`i16`、`i32`、`i64`
- `u8`、`u16`、`u32`、`u64`
- `f32`、`f64` 和 `char`
- `_i8`、`_u64`、`_f32` 等数值后缀
- `as` 显式数值转换
- 新标量类型作为数组元素并参与格式化
- 字符串内容相等比较
- `length(string)` 返回 UTF-8 字节长度
- `check`、`build`、`run` 和 Debug/Release 配置

构建和运行：

```bash
./target/release/dc check examples/m8
./target/release/dc build examples/m8 --release
./target/release/dc run examples/m8 --release
```

使用 `dc --help`、`dc --version` 或 `dc build --help` 查看命令帮助。

最后一个命令输出各类值，并以退出码 `64` 结束。

当前字符串不支持按字节下标或切片。`length` 返回字节数，因此 `length("海豚")` 是 `6`，不是字符数。
