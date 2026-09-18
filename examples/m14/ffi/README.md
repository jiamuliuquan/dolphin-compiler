# M14-E：C 互操作与原生链接

演示 `extern "C"` 声明、`extern struct`、`*Unit` opaque handle 与按目标三元组声明的
原生链接输入。

## 步骤

1. 用系统 C 工具链把 fixture 编译为目标文件（`dc` 不编译 C）：

   ```text
   # Linux / macOS
   cc -std=c11 -c -o native/demo.o native/demo.c

   # Windows（已激活 MSVC 环境）
   cl /nologo /c /Fonative\demo.obj native\demo.c
   ```

2. 构建并运行：

   ```text
   dc build .
   ./target/ffi
   ```

预期 stdout：

```text
bytes=1 2 3 4 moved=11 size=true
```

进程退出码为 `demo_add(20, 22) = 42`。

## 说明

- `dolphin.toml` 的 `[native.<triple>]` 路径相对包目录；缺少文件、架构不符或未解析
  符号都会得到包含文件与目标的诊断。
- `demo_create` / `demo_destroy` 配对使用 C 的 `malloc` / `free`，不能用 Dolphin
  `mem.free` 释放。
- `extern struct` 只按指针传给 C；C 结构体按值传参/返回延后。
