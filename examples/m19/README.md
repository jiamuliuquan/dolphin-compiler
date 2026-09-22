# M19：`dtext` 文本统计/过滤工具

M19 目标程序：用 M19 标准库（`std.process`/`std.io`/`std.fs`/`std.text`/`std.test`）
实现的纯 Dolphin 命令行工具，并演示 `dc test` 用户测试闭环。

## 结构

- `textstats`（`[lib]`）：行/CRLF/无末尾换行规则、过滤与计数核心；纯函数、零分配。
- `dtext`（`[lib]` + `[[bin]]`）：应用逻辑在 `src/app.do`，`src/main.do` 只是入口；
  path 依赖 `textstats`。两包各自有 `tests/*.do`。

> 布局说明：`dc test` 需要 `[lib]` 目标，因此 `dtext` 声明 lib+bin。D1 冻结了
> `dc build --lib` 的打包行为（`.dlib` 拒绝 path 依赖），所以 lib+bin 且声明 path
> 依赖的包用 `dc build --bin dtext` / `dc run --bin dtext` 构建运行。

## 用法

```text
dtext [--help] [--filter <text>] [<path>]
```

- 无 `path` 或 `path == "-"`：读 stdin（不关闭标准输入）。
- `--help`：打印用法，退出 0。
- 未知选项、缺少 `--filter` 值、多余位置参数：stderr 诊断，退出 2。
- 输出固定三行 `lines=`/`matched=`/`bytes=`；I/O 或非法 UTF-8 错误退出 1。

行规则：以 `\n` 分隔，去掉紧邻 `\n` 前的一个 `\r`；孤立 `\r` 保留；无 `\n` 的非空尾行仍计数。
`--filter` 按 UTF-8 字节子串匹配行内容；无过滤或空子串匹配所有行。

## 构建、运行与自测

```bash
./target/release/dc build examples/m19/dtext --bin dtext
printf 'alpha\nbeta\n' | ./examples/m19/dtext/target/dtext --filter beta
# lines=2
# matched=1
# bytes=11

./target/release/dc test examples/m19/textstats
./target/release/dc test examples/m19/dtext
```

`tests/m19_app.rs` 会把本目录复制到临时目录，在可用后端 × Dolphin Debug/Release 上
构建并实际调用命令行断言三路结果（含空输入、正常 UTF-8、无末尾换行、无匹配、CRLF、
Unicode/空格路径、用法错误、缺失文件、非法 UTF-8、Linux 受控写失败与重复运行无泄漏）。
