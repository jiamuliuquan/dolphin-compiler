# M7：多文件项目与模块

入口：[src/main.do](src/main.do)

本示例验证：

- 递归扫描项目 `src/` 下的 `.do` 文件
- `src/main.do` 与 `src/helper.do` 合并到根模块并省略 `pkg`
- `src/std/math.do` 声明与路径一致的 `pkg std.math;`
- `use std.math;` 导入模块并通过 `math.clamp(...)` 调用
- `use std.math.min;` 导入公开成员并通过 `min(...)` 调用
- `pub` 控制跨模块可见性
- 同一模块内可以调用私有函数

构建和运行：

```bash
./target/release/dc build examples/m7
./examples/m7/target/m7
echo $?
```

预期输出：

```text
min = 3, clamp = 10
```

预期退出码为 `13`。

当前不支持 `use std.*`、导入别名和依赖包。`std` 只是本项目 `src/std/` 下的普通源码模块；`print` 和 `println` 是编译器内建函数。
