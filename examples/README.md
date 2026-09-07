# 可运行示例

该目录只保存当前编译器已经支持、能够真实编译和运行的示例。示例按实现里程碑划分，每个目录都是独立 Dolphin 项目，入口均为 `src/main.do`。

## 准备编译器

在仓库根目录执行：

```bash
cargo build --release
```

正式短命令位于 `target/release/dc`，兼容名称位于 `target/release/dolphin-compiler`。

## 示例列表

| 示例 | 已实现能力 | 验证方式 |
| --- | --- | --- |
| [M1](m1/README.md) | `i32` 字面量、算术、括号、返回值 | 退出码 `28` |
| [M2](m2/README.md) | `var`、`val`、类型标注、类型推断、赋值 | 退出码 `42` |
| [M3](m3/README.md) | 布尔、比较、短路逻辑、分支、循环 | 退出码 `28` |
| [M4](m4/README.md) | 多函数、参数、返回类型、前向调用、递归 | 退出码 `120` |
| [M5](m5/README.md) | UTF-8 字符串、`print`、`println`、格式化 | 检查标准输出 |
| [M6](m6/README.md) | 定长数组、下标、范围、`for`、数组函数 ABI | 输出并退出码 `43` |
| [M7](m7/README.md) | 多文件项目、`pkg`、`use`、`pub` 和模块可见性 | 输出并退出码 `13` |
| [M8](m8/README.md) | 完整基础标量、后缀、转换、字符串比较和 `dc` CLI | 输出并退出码 `64` |
| [M9](m9/README.md) | `dolphin.toml` 清单、包坐标、多可执行目标、清单查找 | 多 bin 输出并退出码 `42`/`26` |
| [M13](m13/README.md) | 结构体、枚举、`match` 模式匹配、跨模块类型引用 | 输出并退出码 `49` |

## 通用构建方式

从仓库根目录执行：

```bash
./target/release/dc build examples/m3
./examples/m3/target/m3
echo $?
```

编译器会在示例自身的 `target/` 中生成：

```text
target/m3             本机可执行文件
target/m3.o           Dolphin 程序目标文件
target/m3.runtime.o   最小运行时目标文件
```

`target/` 已被 Git 忽略。

## 当前边界

这些示例刻意不使用以下尚未实现的语法：

- 嵌套数组、空数组、切片和动态数组
- 数组整体比较和直接格式化
- 通配符导入、导入别名和外部依赖
- 结构体/枚举作为函数参数或返回值（值传递语义，M14）
- 泛型和包管理

后续候选能力见[路线图](../docs/roadmap.md)。
