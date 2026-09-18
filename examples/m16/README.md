# M16：优化后端基准

本示例用于对比 **Cranelift**（快速编译后端）与 **LLVM**（优化后端）的：

- Debug 编译时间
- Release 运行时间
- 产物体积

同一份 `src/main.do`，后端通过环境变量 `DOLPHIN_BACKEND` 选择。

## 运行基准

需要先构建带 LLVM 后端的 `dc`（本机需安装 LLVM 开发库，`inkwell` 通过
`llvm-config` 定位）：

```bash
cargo build --release --features llvm --bins
python3 scripts/bench.py
```

## 手动对比

```bash
# Cranelift
DOLPHIN_BACKEND=cranelift ./target/release/dc build examples/m16 --release
./examples/m16/target/m16

# LLVM
DOLPHIN_BACKEND=llvm ./target/release/dc build examples/m16 --release
./examples/m16/target/m16
```

预期输出：

```text
sum = 1799999937, fib = 9227465
```
