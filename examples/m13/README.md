# M13：结构体、枚举与 match 模式匹配

入口：[src/main.do](src/main.do)

本示例验证：

- 结构体：位置构造、字段访问
- 枚举：携带数据的枚举项、无参枚举项
- `match` 表达式：解构绑定、通配符 `_`、穷尽匹配
- 跨模块类型引用：`pub struct`/`pub enum` 通过 `use` 导入
- 非泛型错误处理：用枚举表示解析结果，`match` 区分成功与失败

构建和运行：

```bash
./target/release/dc check examples/m13
./target/release/dc build examples/m13 --release
./target/release/dc run examples/m13 --release
```

最后一个命令输出各类值，并以退出码 `49` 结束。

当前结构体和枚举不能作为函数参数或返回值（值传递语义延后到 M14）；`match` 需对枚举穷尽匹配，缺失分支会在编译期报错。
