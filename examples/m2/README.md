# M2：局部变量

入口：[src/main.dc](src/main.dc)

本示例在 M1 基础上验证：

- `var` 可变变量
- `val` 不可变变量
- `var value: i32 = ...` 显式类型标注
- 根据初始化表达式进行局部类型推断
- `=` 普通赋值
- `+=`、`-=`、`*=`、`/=`、`%=` 复合赋值
- 同一作用域重复声明检查
- 对 `val` 重新赋值的编译错误

构建和运行：

```bash
./target/release/dc build examples/m2
./examples/m2/target/m2
echo $?
```

预期退出码为 `42`。
