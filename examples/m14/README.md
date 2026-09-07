# M14：内存模型与动态数据

入口：[src/main.do](src/main.do)

本示例验证：

- 结构体按值传递（回溯 M13，浅拷贝 + 显式管理）
- 显式指针：取址 `&`、解引用 `*`、字段访问 `->`
- 动态切片：`allocate`/`free` + 索引读写 + `length`
- `try(...)` 语法糖：局部资源出块自动释放（禁止逃逸）
- `defer`：确定性释放
- 字符串拼接 `+`：隐式分配，释放责任交接收者
- 跨模块容器：手写动态数组，动态内存跨函数、跨模块使用

构建和运行：

```bash
./target/release/dc check examples/m14
./target/release/dc build examples/m14 --release
./target/release/dc run examples/m14 --release
```

最后一个命令输出各类值，并以退出码 `21` 结束（`moved.x = 15`，`vector.sum = 6`）。

内存模型要点：堆内存由用户显式 `allocate` 分配、`free` 释放；`defer`/`try` 提供确定性释放；结构体/切片/指针均为值语义（浅拷贝），不引入所有权、引用计数或垃圾回收。Debug 构建启用双重释放检测（退出码 103）与泄漏报告。
