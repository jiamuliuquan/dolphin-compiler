# 安装与发行

Dolphin 编译器以携带预编译 Dolphin 运行时与 LLD 的发行包发布，使用发行包不需要编译器源码或 Rust。这里的“自包含”不等于携带完整系统 CRT/SDK；能否在目标机器构建并链接程序仍受[系统依赖边界](#6-系统依赖边界)限制，尚不承诺任意无开发工具链、无 SDK 的干净机器解压即可构建。

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
libLLVM.so.*           仅 Linux：rust-lld 的动态依赖（发行包自带）
LICENSE                GPL-3.0 许可证
README.md              说明文档
```

`dc` 构建 Dolphin 程序时会优先在**自身所在目录**查找 `rust-lld`，因此上述文件应保持在同一目录。macOS / Linux 发行包额外携带 LLVM 动态库（`libLLVM.dylib` / `libLLVM.so.*`），它与 `rust-lld` 同目录，`rust-lld` 通过 `@loader_path` / `$ORIGIN` rpath 定位到它，因此这些文件必须放在同一目录，不可拆分。

官方默认 feature 只启用 Cranelift codegen，不启用 LLVM codegen；这与 **Unix 发行包的 LLD 链接器仍依赖并携带 `libLLVM`** 是两回事。打包行为见 [`scripts/package.py`](../scripts/package.py) 的 `bundle_rust_lld_with_libllvm`，不能因未启用 `llvm` feature 就删除这些动态库。

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

在所需本地系统链接依赖齐备时，发行包可离线使用：`dc check/build/run` 在不声明远程依赖时不访问网络。Dolphin 运行时目标文件内嵌于 `dc`，链接器为同目录的 `rust-lld`；离线不代表缺少 CRT/SDK 时也能完成链接。

使用 Maven 风格依赖时，`dc` 通过 HTTPS 下载 `.dlib` 到内容寻址缓存（`DOLPHIN_HOME`，默认 `~/.dolphin`），并写入根目录 `dolphin.lock`。构建后可用 `dc build --locked --offline` 在断网时复现；`file://` 仓库与本地 path 依赖始终可用。`dc env` 会显示当前缓存根。

### 代理

远程仓库访问自动遵循标准代理环境变量，无需额外配置：按顺序检查 `HTTPS_PROXY`、`HTTP_PROXY`、`ALL_PROXY`（大小写变体均可），并用 `NO_PROXY`/`no_proxy` 指定绕过代理的主机（支持精确主机、通配与 CIDR）。

```bash
# 通过公司代理访问 HTTPS 仓库，回环/内网直连
export HTTPS_PROXY=http://proxy.example:8080
export NO_PROXY=127.0.0.1,localhost,.internal.example
```

`dc` 没有独立的 `--proxy` 参数或清单配置；代理只通过环境变量生效。`--offline` 始终不访问网络，与代理无关。把回环地址（`127.0.0.1`、`localhost`）加入 `NO_PROXY` 可避免本地仓库请求被误转发到代理。

## 6. 系统依赖边界

应区分三层依赖：运行 `dc`/`rust-lld`、链接 Dolphin 程序、运行最终程序。预编译 Dolphin 运行时免除了用户现场编译 runtime，默认 LLD 免除了直接调用系统链接器，但两者都不自动提供链接所需的系统 CRT 对象、导入库或 SDK。

| 平台 | 最终程序的主要系统运行时依赖 |
| --- | --- |
| Linux | glibc（libc、动态链接器） |
| macOS | libSystem |
| Windows | UCRT（Universal C Runtime） |

上表不是完整的发行包动态依赖清单，也不证明系统已具备开发期链接文件。当前探测与回退逻辑见 [`crates/dolphin-platform/src/platform.rs`](../crates/dolphin-platform/src/platform.rs) 和 [`crates/dolphin-platform/build.rs`](../crates/dolphin-platform/build.rs)：

| 平台 | 当前链接依赖与路径限制 |
| --- | --- |
| Linux x86_64 | 运行时通过 `CC` 或 `cc -print-file-name=...` 探测 CRT（如 `Scrt1.o`、`crtbeginS.o`）、libc 搜索目录与动态链接器。探测失败回退编译期 `link_args.rs` 常量；这些路径可能包含构建机 GCC 版本目录，目标机缺少对应文件时仍会失败。只有 glibc 运行库不等于具备 CRT/开发链接文件。 |
| macOS ARM64 | 运行时通过 `xcrun --show-sdk-path` / `--show-sdk-version` 探测 SDK，失败后回退编译期 `-syslibroot` / 平台版本。发行包未携带完整 macOS SDK；构建机 Xcode/CLT 路径在目标机可能不存在，仅有 libSystem 不足以证明无 SDK 可链接。 |
| Windows x86_64 | 默认调用 `rust-lld -flavor link`，仍需解析预编译 runtime 的 CRT/系统库依赖。当前平台封装未自行提供完整 MSVC/Windows SDK 库发现与打包，应确保所需库及搜索环境（如 `LIB`）可用；系统提供 UCRT DLL 不等于提供全部链接用库。CI 激活了 MSVC 开发环境，不能据此声称无 Visual Studio/SDK 的干净机器已通过。 |

编译期常量回退只提供参数，不会复制缺失的 CRT/SDK，也不保证跨机器路径有效。`--system-linker` 是需要本机工具链的显式回退，不是无 SDK 问题的通用解决办法。第三方 C 源码仍由库作者使用 C 工具链预编译；消费程序需要清单声明的原生链接文件与运行时附件。

构建**编译器自身**（从源码 `cargo build`）需要 Rust 和本机 C/C++ 编译工具链及对应平台链接依赖。可选 LLVM 后端（`cargo build --features llvm`）还要求本机安装 LLVM 开发库（`llvm-config` 可被 `llvm-sys` 定位，当前对接 LLVM 22）。默认 Cranelift codegen 不需要 LLVM 开发库，但 Unix 发行包的 LLD 仍携带 `libLLVM`。以 `--features llvm` 自行构建的 `dc` 若动态链接 LLVM，运行环境需具备匹配的库；以实际产物的依赖检查为准，不能假定发行包中供 Rust LLD 使用的那份 `libLLVM` 可替代 codegen 所需版本。

#### LLVM 22 开发环境（`--features llvm`）

CI 的 LLVM lane 固定使用 `/usr/lib/llvm-22` 并显式校验版本，不依赖 runner 偶然预装的 LLVM。Ubuntu 上可用 apt.llvm.org 复现同一环境：

```bash
sudo apt-get update
sudo apt-get install -y wget ca-certificates gnupg
wget -qO- https://apt.llvm.org/llvm-snapshot.gpg.key \
  | sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc > /dev/null
. /etc/os-release
echo "deb http://apt.llvm.org/${VERSION_CODENAME}/ llvm-toolchain-${VERSION_CODENAME}-22 main" \
  | sudo tee /etc/apt/sources.list.d/llvm-22.list > /dev/null
sudo apt-get update
sudo apt-get install -y llvm-22-dev
export PATH="/usr/lib/llvm-22/bin:$PATH"
export LLVM_SYS_221_PREFIX=/usr/lib/llvm-22

# 版本检查：必须是 22.x，且能真正链接（`cargo build --features llvm`）
llvm-config --version
cargo clippy --workspace --all-targets --features llvm -- -D warnings
DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm --test build --test ffi --test cli --test manifest --test packages --test doc_examples
cargo test -p dolphin-compiler --features llvm --test backend
```

`llvm-config --version` 输出正确不等于 dev 库可链接；以上命令以实际编译/链接为准。若本机 LLVM 不在 `/usr/lib/llvm-22`，设置 `LLVM_SYS_221_PREFIX` 指向其前缀即可。


### 后续干净环境验收

现有 [CI](../.github/workflows/ci.yml) 在装有开发工具的 runner 上构建并冒烟测试发行包，未隔离构建机 SDK、CRT 与环境变量。M18+ 交接见 [后续计划](plan-m18-plus.md)与 [正确性计划](plan-m18-correctness.md)；在完成以下验证前，不扩大“无需工具链”的承诺：

1. 为三个官方目标分别使用独立干净 VM/容器（适用平台），记录操作系统版本、系统运行库与额外安装项；仅转入发行包和测试源码，不复用构建目录、Rust sysroot 或开发环境。
2. 审计 `dc`、`rust-lld` 与随附动态库的实际加载依赖及路径，验证搬移发行目录后仍可启动；从环境和文件系统两方面隔离开发工具/SDK，不能仅删除 `PATH` 条目。
3. 分别验证 `dc --version`、`dc env`、`check`、Debug/Release `build/run`，再验证源码标准库、预编译 C 附件和已缓存依赖的 `--locked --offline` 构建；记录 stdout/stderr、退出码和实际链接命令/依赖路径。
4. 对缺 CRT/SDK、路径失效、动态库缺失分别记录可复现失败与诊断；逐项确定需随包携带或由用户安装的依赖及许可证边界。只有附上各平台证据后，才可更新最低系统要求和无工具链安装承诺。

## 7. 命令行兼容政策

- `dc` 是正式命令名，`dolphin-compiler` 是等价的兼容名称，二者行为完全一致。
- 子命令（`check`/`build`/`run`/`package`/`fetch`/`publish`/`info`/`env`/`fmt`/`lsp`）及稳定选项在 1.0 之前保持兼容，不做破坏性移除。
- `--color`、`--debug`/`--release`、`--bin`、`--lib`、`--output`、`--system-linker`、`--backend`、`--locked`、`--offline`、`--repository` 为稳定选项。
- `.dlib` 格式版本固定为 `format-version = 1`，消费端要求 `compiler-version` 与当前 `dc` 完全一致；放宽兼容范围时会提升格式/版本策略。
- 诊断错误类别（`E0000`/`E0001`）一经公开保持含义稳定。

## 8. 目标支持等级

| 平台 | 等级 | 说明 |
| --- | --- | --- |
| Linux x86_64 | 一级（完整支持） | CI 测试、构建、端到端运行 |
| macOS ARM64 | 一级（完整支持） | CI 测试、构建、端到端运行 |
| Windows x86_64 | 一级（完整支持） | CI 测试、构建、端到端运行 |

其他目标（如 Windows ARM64、Linux ARM64）当前不支持，编译器会给出诊断而非静默失败。交叉编译不在当前范围内。

“一级”表示当前开发/CI 环境的目标支持范围，不等于已完成无 CRT/SDK 开发文件的干净机器安装验收。

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

### 后端对比（M16）

同一份 `examples/m16` 在两个后端下的实测（Arch Linux x86_64，Release `dc`）：

| 后端 | Debug 编译 | Release 运行 | Release 体积 |
| --- | ---: | ---: | ---: |
| Cranelift（默认） | 0.027 s | 0.230 s | 16.2 KiB |
| LLVM | 0.027 s | 0.162 s | 14.2 KiB |

用 `python3 scripts/bench.py` 复现；脚本默认先执行 `cargo build --release --features llvm --bins`，需要第 6 节所述 LLVM 开发环境。使用 `--no-build` 时，必须事先构建带 `llvm` feature 的 Release 编译器。数值随机器、程序与负载变化，不保证所有场景 LLVM 都更快。

默认使用 Cranelift；只有编译器已启用 `llvm` feature，才可通过 `--backend llvm` 或 `DOLPHIN_BACKEND=llvm` 切换。官方默认发行包未启用该 feature，参数或环境变量不能动态增加后端，选择未编译的后端会报诊断。

## 11. 许可证与第三方组件

Dolphin 编译器以 **GNU General Public License v3.0** 发布（见仓库 `LICENSE`）。

编译器静态链接 Rust 生态依赖（见 `Cargo.lock`），包括 Cranelift（Apache-2.0 WITH LLVM-exception）、Clap（MIT OR Apache-2.0）、serde（MIT OR Apache-2.0）、target-lexicon（Apache-2.0 WITH LLVM-exception）、toml（MIT OR Apache-2.0），以及包管理所需的 zip（MIT）、ureq/rustls/ring（MIT OR Apache-2.0 / ISC）、sha2（MIT OR Apache-2.0）、semver（MIT OR Apache-2.0）和 url（MIT OR Apache-2.0）。启用可选 LLVM 后端（`--features llvm`）时还会链接 inkwell（Apache-2.0）与 LLVM/llvm-sys（Apache-2.0 WITH LLVM-exception）。发行包中的 `rust-lld` 来自 LLVM 项目（Apache-2.0 WITH LLVM-exception）。发布新版本时同步更新本清单。
