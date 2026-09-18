# M14：手动内存管理与 C 互操作

本示例演示 M14 的显式内存模型：

- `mem.alloc<T>(count)` 分配连续存储，`defer mem.free(buffer)` 在作用域退出时释放；
- `mem.create<T>(value)` / `mem.destroy(ptr)` 管理单个对象；
- 结构体数组按统一布局写入与读取，字段对齐由 `mem.size_of` / `mem.align_of` 决定；
- `s.bytes()` 与 `string.from_bytes(...)` 在只读字节视图与 `string` 之间零分配转换。

## 构建与运行

```text
dc build examples/m14
./examples/m14/target/m14
```

Debug 构建链接检测版运行时：若存在未释放的分配，正常退出时会在 stderr 打印泄漏
报告，并保留程序退出码。Release 构建不追踪分配。

## C 互操作示例

`ffi/` 子目录演示 `extern "C"` 声明与原生链接（需要系统 C 工具链预编译 fixture）：

```text
cd examples/m14/ffi
cc -std=c11 -c -o native/demo.o native/demo.c        # Windows: cl /c /Fonative\demo.obj native\demo.c
dc build .
./target/ffi
```

`ffi/dolphin.toml` 按目标三元组声明 `objects`。`dc` 消费的是预编译 C 文件，
不负责编译 C 源码。
