# M9：项目清单与构建描述

入口：`dolphin.toml`，声明包坐标与两个可执行目标。

本示例验证：

- `[package]` 中的 `group`、`name`、`version` 和 `source`
- 包坐标 `me.foxlab:greeter:0.1.0`
- `[[bin]]` 声明多个可执行目标及各自入口文件
- `[build]` 中的 `output` 与 `optimization`
- 多个目标共享 `src/` 下的模块（`util.dc`）
- 从任意目录向上查找清单，无需重复传入目录

项目结构：

```text
m9/
├── dolphin.toml        项目清单
└── src/
    ├── main.dc         cli 入口（main）
    ├── server.dc       server 入口（main）
    └── util.dc         共享模块（double、classify）
```

## 查看包信息

```bash
./target/release/dc info examples/m9
```

输出包坐标、源码目录、目标列表与优化级别。

## 检查与构建

```bash
./target/release/dc check examples/m9
./target/release/dc build examples/m9
```

`build` 会为每个 `[[bin]]` 生成一个可执行文件：

```text
target/cli       由 src/main.dc 编译
target/server    由 src/server.dc 编译
```

## 运行单个目标

```bash
./target/release/dc run examples/m9 --bin cli      # 退出码 42
./target/release/dc run examples/m9 --bin server   # 退出码 26
```

由于项目声明了两个目标，`run` 必须用 `--bin` 指定运行哪一个。也可以只构建单个目标：

```bash
./target/release/dc build examples/m9 --bin server
```

## 从子目录运行

清单查找会沿目录向上回溯，因此从项目内任意子目录执行同样有效：

```bash
cd examples/m9/src
../../../target/release/dc check
```

`M1-M8` 的无清单目录和单文件行为保持兼容：没有 `dolphin.toml` 时，`check/build/run` 仍按旧规则处理 `src/` 或单文件。
