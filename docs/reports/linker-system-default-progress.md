# 系统链接器默认化进度报告

本报告记录 M20 之后的链接器发行策略调整（无 H 编号批次）。改动已实现并完成 Linux x86_64
本机验证与文档同步；Windows/macOS 本机复验与远端 CI 未运行，未验证项在下文如实标注。
所有改动尚未 commit/push/tag。

- 状态：**实现完成；Linux x86_64 默认 lane、打包与冒烟通过；Windows/macOS 与远端 CI 未验证**
- 开始 HEAD：`7fc848c62d02a29bc6ce4b2d4f3df58bd836b97b`（`update to v0.4.0`）；改动前工作区干净
- 决策来源：用户确认「三平台全部切换系统链接器」且「保留 `--bundled-linker` 可选回退」
- 历史基线：[M20 进度报告](m20-progress.md)（M20 已完成）、[安装说明](../installation.md)

## 背景与目标

发行包此前携带 `rust-lld` 与 `libLLVM`（约 108 MB），是包体积的主要来源；默认链接参数探测
仍可能调用 `cc`/`xcrun`，并非真正无系统依赖。本次将默认链接器切换为平台系统链接器
（Unix `cc`，Windows `link`），发行包不再携带任何链接器，同时保留原 rust-lld 路径作为
`--bundled-linker` 可选回退。目标：发行包从约 114 MB 降到约 9 MB，同时保留诊断/对比能力。

## 修改范围

| 类别 | 文件 | 内容 |
| --- | --- | --- |
| 链接器选择 | `crates/dolphin-linker/src/linker.rs` | `LinkerChoice` 默认改为 `System`，新增 `Bundled`；`resolve_tool` 仅服务 bundled |
| 平台抽象 | `crates/dolphin-platform/src/platform.rs` | `linker_name` 改指系统链接器、新增 `bundled_linker_name`；`system_link_command` 为共享库补 `-Wl,-rpath,$ORIGIN` / `-Wl,-rpath,@loader_path` |
| 平台构建 | `crates/dolphin-platform/build.rs` | `link_args.rs` 探测改为 `--bundled-linker` 专用回退，注释更新 |
| LLD 定位 | `crates/dolphin-linker/build.rs` | `DOLPHIN_LLD` 注入明确为 bundled 专用 |
| CLI | `src/main.rs` | 新增 `--bundled-linker`；`--system-linker` 保留为默认的显式写法；两者互斥；`dc env` 输出系统/自带链接器 |
| 打包 | `scripts/package.py` | 删除 `find_rust_lld`、`bundle_rust_lld_with_libllvm`、`rustc_sysroot`；只打包 `dc`、`dolphin-compiler`、LICENSE、README |
| CI | `.github/workflows/ci.yml` | 打包/冒烟注释更新；冒烟新增「归档不得含 rust-lld」守卫；macOS rust-lld 步骤说明限定为 cargo test 与 bundled 测试 |
| 测试 | `tests/cli.rs` | 新增 `build_help_lists_linker_options_and_they_conflict`、`build_with_bundled_linker_runs`；更新 `env_shows_host_target_and_linker` |
| 测试 | `crates/dolphin-platform/src/platform.rs` | 新增 `unix_system_link_command_adds_rpath_for_shared_libs`、`linker_names_follow_platform` |
| 文档 | README、`docs/installation.md`、`docs/implemented-features.md`、`docs/roadmap.md`、`docs/compiler-implementation.md`、`docs/plan-m18-plus.md` | 当前行为、包内容、依赖边界、体积基准、H21-00 决策方向同步 |
| 网站 | `docs/website/{index,install}.html`、`assets/js/i18n.js`、`assets/js/content/{zh,en}-tutorial.js` | 去掉「随包 rust-lld / 必须同目录」，明确系统工具链要求与 `--bundled-linker` |

共 20 个文件修改（不含本报告），未触碰运行时 C/C++ 源码、示例 `.do`、`.dlib` 格式与包管理逻辑。

## 公开行为变化

- 默认链接器：Unix `cc`，Windows `link`；不再默认解析 `rust-lld`。
- 新增稳定选项 `--bundled-linker`；`--system-linker` 继续接受，作为默认行为的显式写法（CLI 兼容政策）。
- `dc env` 输出从 `linker: rust-lld` + `system linker: ...` 改为 `linker: cc|link (system; default)` + `bundled linker: rust-lld (--bundled-linker)`。
- 系统链接路径在声明 `shared-libs` 时补 rpath，修复此前只有 rust-lld 路径具备的运行时库发现能力。
- 发行包内容变化：移除 `rust-lld`、`libLLVM.dylib`、`libLLVM.so.*`；包体积约 114 MB → 8.8 MB（Linux x86_64 实测）。
- 用户环境要求变化：构建程序需要本机开发工具链；Windows 需要已激活的 MSVC 开发环境（`link` 与 `LIB`/`PATH`）。
- 未提升版本号，未改变 `.dlib`、锁文件或 CLI 子命令语义。

## 验收映射与证据

| 检查 | 证据 | 结果 |
| --- | --- | --- |
| 默认使用系统链接器 | `tests/cli.rs::env_shows_host_target_and_linker`；`dc env` 实测 | 通过 |
| 系统链接命令含共享库 rpath | `platform::tests::unix_system_link_command_adds_rpath_for_shared_libs` | 通过 |
| 平台链接器名称 | `platform::tests::linker_names_follow_platform` | 通过 |
| `--bundled-linker` 仍可用 | `tests/cli.rs::build_with_bundled_linker_runs`；手动 `dc run main.do --bundled-linker` | 通过（Linux） |
| 选项互斥与帮助 | `tests/cli.rs::build_help_lists_linker_options_and_they_conflict` | 通过 |
| 系统链接路径端到端（含 FFI 共享库） | 全量测试套件（FFI 用例默认走系统链接器） | 通过（Linux） |
| 发行包不含链接器 | `scripts/package.py` 产物清单；CI 冒烟新增 `test ! -e smoke/rust-lld` | 通过（Linux 本机打包；CI 守卫未在远端运行） |
| 发行包体积 | `dist/dolphin-0.4.0-x86_64-unknown-linux-gnu.tar.gz` 9,166,785 字节（约 8.8 MB） | 通过（Linux） |
| 发行包冒烟 | 解压后 `dc build examples/m8 --release` 并运行：exit 64，stderr 为空 | 通过（Linux） |
| 格式与静态检查 | `cargo fmt --all -- --check`、clippy `-D warnings` | 通过 |
| 脚本与 CI 语法 | `python3 -m py_compile scripts/package.py`、YAML 解析 `ci.yml` | 通过 |

## 实际运行命令与测试数量

- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings`：通过。
- `cargo test --workspace --exclude dolphin-codegen-llvm`：**450 passed, 0 failed**（含新增用例）。
- `cargo build --release --bins`：通过。
- `python3 scripts/package.py --target x86_64-unknown-linux-gnu`：产出 tar.gz + `.sha256`。
- 冒烟：解压归档后 `dc env`、`dc build examples/m8 --release`、运行产物 exit 64。
- 手动：单文件 `dc run main.do` 与 `dc run main.do --bundled-linker` 均输出正常。

## 未运行的检查及原因

- Windows/macOS 本机复验：当前开发机为 Linux；三平台默认 lane 依赖远端 CI（本次未 push/tag）。
- 远端 CI 三平台门禁与 tag 发布流程：未运行。
- macOS 打包与冒烟、macOS 上 `--bundled-linker`（需 `DYLD_FALLBACK_LIBRARY_PATH`，CI 已有步骤）：未运行。
- Windows 上 `link`/`rust-lld` 的打包冒烟：未运行；依赖已激活的 MSVC 环境。
- Linux LLVM lane（`--features llvm`）：本次未改 LLVM 代码生成，未运行。

## 剩余问题与后续

1. **M21 干净环境验收**：本改动按[路线图](../roadmap.md) M12 后续调整与 [M21 交接](../plan-m18-plus.md) H21-00 的「要求系统 SDK/开发库并检测」方向落地；隔离镜像/虚拟机与三平台干净环境验收仍属 H21-04。
2. **缺链接器诊断**：Windows 未激活 MSVC 环境时，`link` 缺失只报启动失败；后续可考虑 vswhere 探测或更可操作的诊断（当前文档已说明 Developer Command Prompt 要求）。
3. **版本与发布**：改动尚未 commit/tag，未更新版本号；发布前需按发布流程同步归档校验和与许可证清单。
4. **文档历史口径**：M12 历史记录保留原「随包 LLD」事实，当前行为以 README、[安装说明](../installation.md) 与[已实现功能参考](../implemented-features.md)为准。
