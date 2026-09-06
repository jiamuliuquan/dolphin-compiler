# 安装与发行

Dolphin 编译器以**自包含发行包**发布：解压后即可使用，无需安装 C/C++ 工具链，也无需 Rust。

## 1. 获取发行包

每个版本发布三个平台的发行包，命名格式为 `dolphin-<版本>-<目标三元组>`：

| 平台 | 目标三元组 | 归档格式 |
| --- | --- | --- |
| Linux x86_64 | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| macOS ARM64 | `aarch64-apple-darwin` | `.tar.gz` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `.zip` |

每个归档旁附同名 `.sha256` 文件，记录 SHA-256 校验和。

### 校验完整性

```bash
# Linux / macOS
sha256sum -c dolphin-0.1.0-x86_64-unknown-linux-gnu.sha256

# Windows（PowerShell）
Get-FileHash dolphin-0.1.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

## 2. 发行包内容

```text
dc                     编译器主命令
dolphin-compiler       兼容名称（与 dc 等价）
rust-lld               可再分发链接器（发行包自带，LLD）
libLLVM.dylib          仅 macOS：rust-lld 的动态依赖（发行包自带）
LICENSE                GPL-3.0 许可证
README.md              说明文档
```

`dc` 构建 Dolphin 程序时会优先在**自身所在目录**查找 `rust-lld`，因此上述文件应保持在同一目录。macOS 发行包额外携带 `libLLVM.dylib`，它与 `rust-lld` 同目录，`rust-lld` 通过 `@loader_path` rpath 定位到它，因此三者（`dc`、`rust-lld`、`libLLVM.dylib`）必须放在同一目录，不可拆分。

## 3. 安装

发行包无需"安装"：解压到任意目录，把该目录加入 `PATH` 即可。

```bash
# Linux / macOS
mkdir -p ~/.local/dolphin
tar xzf dolphin-0.1.0-x86_64-unknown-linux-gnu.tar.gz -C ~/.local/dolphin
export PATH="$HOME/.local/dolphin:$PATH"   # 建议写入 ~/.bashrc 或 ~/.zshrc
```

```powershell
# Windows（PowerShell）
Expand-Archive dolphin-0.1.0-x86_64-pc-windows-msvc.zip -DestinationPath "$env:USERPROFILE\dolphin"
$env:Path = "$env:USERPROFILE\dolphin;$env:Path"   # 建议用系统环境变量持久化
```

验证安装：

```bash
dc --version
dc env
```

## 4. 升级与卸载

升级：用新版本发行包覆盖旧目录（或解压到新目录后替换 `PATH` 指向）即可。

卸载：删除发行目录，并从 `PATH` 中移除对应条目。Dolphin 不写注册表、不创建系统服务，也无须运行卸载程序。

## 5. 离线使用

发行包完全离线可用：`dc check/build/run` 不访问网络。构建 Dolphin 程序所需的运行时与链接器均已内嵌（运行时目标文件内嵌于 `dc`，链接器为同目录的 `rust-lld`）。

## 6. 系统依赖边界

发行包构建 Dolphin 程序**不再需要 C/C++ 开发工具链**（无需 `cc`/`cl`/`link`），但仍动态链接操作系统自带组件：

| 平台 | 运行时动态依赖 |
| --- | --- |
| Linux | glibc（libc、动态链接器） |
| macOS | libSystem |
| Windows | UCRT（Universal C Runtime） |

这些是操作系统自带组件，不属于 C/C++ 开发工具链，无需额外安装。构建**编译器自身**（从源码 `cargo build`）才需要 Rust 和本机 C 编译器。

## 7. 命令行兼容政策

- `dc` 是正式命令名，`dolphin-compiler` 是等价的兼容名称，二者行为完全一致。
- 子命令（`check`/`build`/`run`/`info`/`env`）及稳定选项在 1.0 之前保持兼容，不做破坏性移除。
- `--color`、`--debug`/`--release`、`--bin`、`--output`、`--system-linker` 为稳定选项。
- 诊断错误类别（`E0000`/`E0001`）一经公开保持含义稳定。

## 8. 目标支持等级

| 平台 | 等级 | 说明 |
| --- | --- | --- |
| Linux x86_64 | 一级（完整支持） | CI 测试、构建、端到端运行 |
| macOS ARM64 | 一级（完整支持） | CI 测试、构建、端到端运行 |
| Windows x86_64 | 一级（完整支持） | CI 测试、构建、端到端运行 |

其他目标（如 Windows ARM64、Linux ARM64）当前不支持，编译器会给出诊断而非静默失败。交叉编译不在当前范围内。

## 9. 版本规则

采用语义化版本（SemVer）`MAJOR.MINOR.PATCH`：

- `MAJOR`：不兼容的语言语法或命令行变更。
- `MINOR`：向后兼容的新功能（新里程碑通常递增 MINOR）。
- `PATCH`：向后兼容的缺陷修复。

当前为 0.x 预发布阶段：语言语法与命令行在 1.0 前可能调整，但会通过 MINOR 版本体现变更。

## 10. 基准

以下为参考数值（Windows x86_64，Release 模式，机器配置不同会有差异）：

| 指标 | 数值 |
| --- | --- |
| 编译器 `dc` 体积 | 约 6.3 MB |
| 发行包内 `rust-lld` 体积 | 约 108 MB |
| 编译 `examples/m8` 耗时 | 约 0.3 秒 |
| `examples/m8` 产物体积 | 约 142 KB |

编译时间与产物体积会随程序规模增长；Release 模式开启 `speed` 优化，Debug 模式编译更快但产物更大、运行更慢。

## 11. 许可证与第三方组件

Dolphin 编译器以 **GNU General Public License v3.0** 发布（见仓库 `LICENSE`）。

编译器静态链接 Rust 生态依赖（约 68 个 crate，见 `Cargo.lock`），包括 Cranelift（Apache-2.0 WITH LLVM-exception）、Clap（MIT OR Apache-2.0）、serde（MIT OR Apache-2.0）、target-lexicon（Apache-2.0 WITH LLVM-exception）、toml（MIT OR Apache-2.0）等。发行包中的 `rust-lld` 来自 LLVM 项目（Apache-2.0 WITH LLVM-exception）。
