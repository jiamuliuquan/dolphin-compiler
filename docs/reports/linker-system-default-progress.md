# 系统链接器默认化进度报告

本报告记录 M20 之后的链接器发行策略调整（无 H 编号批次）。改动已实现并完成 Linux x86_64
本机验证与文档同步；首次 push 后远端 CI 在 Windows 冒烟暴露 `link` 同名工具缺陷，已修复并在
Windows 本机复验；macOS 与修复后的远端 CI 待复跑，未验证项在下文如实标注。

- 状态：**实现完成；Linux x86_64 默认 lane、打包与冒烟通过；Windows 默认 lane 与冒烟经本机复验通过（含 `link` 同名冲突修复）；macOS 与修复后的远端 CI 未验证**
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
| 链接器定位 | `crates/dolphin-linker/src/linker.rs` | 修复：Windows 上 `link` 解析到 MSVC 工具链（`VCToolsInstallDir` → PATH 过滤 `\vc\`），避免 Git for Windows coreutils `link` 同名冲突 |
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

## 远端 CI 复验与 Windows `link` 同名冲突修复（2026-09-25）

首次 push（`8f94c7f`）后远端 CI（run `36084667388`）Linux/macOS 全绿，Windows 的
`Smoke test the release archive` 在 `smoke/dc build examples/m8 --release` 失败：

```
error[E0000]: linking `examples/m8\target\m8.exe` failed
link: extra operand 'examples/m8\\target\\m8.obj'
Try 'link --help' for more information.
```

原因：`dc` 以 `Command::new("link")` 按名字解析系统链接器，而 GitHub Actions 的
`shell: bash` 让 Git for Windows 的 `usr\bin\link.exe`（GNU coreutils 的硬链接工具）
排在 MSVC `link.exe` 之前，实际调用的是错误的同名程序（`--help` 提示与 `extra operand`
均为 coreutils 特征）。

修复（`crates/dolphin-linker/src/linker.rs`）：Windows 上 `link` 先解析到 MSVC 工具链——
优先 `VCToolsInstallDir`（vcvars 激活时设置）下的 `bin\Host<x>\x64\link.exe`，否则扫描
PATH 并只接受包含 `\vc\` 的工具链路径；仍找不到时保持原名交给 `Command` 报错。Unix `cc`
与 `--bundled-linker` 的 `rust-lld` 行为不变。新增 Windows 单测
`windows_link_resolves_to_msvc_toolchain` 与 `coreutils_link_is_not_mistaken_for_msvc`。

验证（Windows 11 本机，MSVC 14.44 已激活，并在 PATH 最前放置伪造的 coreutils 同名
`link.exe` 以复现 CI 条件）：修复前同一条件可复现 CI 的失败；修复后 `dc build
examples/m8 --release` 成功且产物 exit 64，CI 冒烟的其余步骤（m14/m15/m18/m19、
`.dlib` 打包、`file://` 仓库 fetch/build/offline 重建）全部通过；`cargo test --workspace
--exclude dolphin-codegen-llvm`、fmt、clippy 全绿。macOS 与修复后的远端 CI 待复跑。

## 实际运行命令与测试数量

- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings`：通过。
- `cargo test --workspace --exclude dolphin-codegen-llvm`：**450 passed, 0 failed**（含新增用例）。
- `cargo build --release --bins`：通过。
- `python3 scripts/package.py --target x86_64-unknown-linux-gnu`：产出 tar.gz + `.sha256`。
- 冒烟：解压归档后 `dc env`、`dc build examples/m8 --release`、运行产物 exit 64。
- 手动：单文件 `dc run main.do` 与 `dc run main.do --bundled-linker` 均输出正常。

## 未运行的检查及原因

- macOS 本机复验：当前无 macOS 机器；依赖远端 CI 与后续复验。
- 修复后的远端 CI 三平台门禁与 tag 发布流程：待 push 后复跑；首次运行的 Windows 失败已在本机修复并复验。
- macOS 打包与冒烟、macOS 上 `--bundled-linker`（需 `DYLD_FALLBACK_LIBRARY_PATH`，CI 已有步骤）：未运行。
- Windows 上 `--bundled-linker`（`rust-lld`）的发行包冒烟：未单独运行；`tests/cli.rs::build_with_bundled_linker_runs` 在 Windows 本机全量测试中通过。
- Linux LLVM lane（`--features llvm`）：本次未改 LLVM 代码生成，未运行。

## 剩余问题与后续

1. **M21 干净环境验收**：本改动按[路线图](../roadmap.md) M12 后续调整与 [M21 交接](../plan-m18-plus.md) H21-00 的「要求系统 SDK/开发库并检测」方向落地；隔离镜像/虚拟机与三平台干净环境验收仍属 H21-04。
2. **缺链接器诊断**：Windows 上 `link` 现已能避开 Git for Windows 的 coreutils 同名工具并解析到 MSVC 工具链（见上文修复节）；未激活 MSVC 环境且 PATH 无 MSVC 工具链时仍只报启动失败，后续可考虑 vswhere 探测或更可操作的诊断（当前文档已说明 Developer Command Prompt 要求）。
3. **版本与发布**：改动尚未 commit/tag，未更新版本号；发布前需按发布流程同步归档校验和与许可证清单。
4. **文档历史口径**：M12 历史记录保留原「随包 LLD」事实，当前行为以 README、[安装说明](../installation.md) 与[已实现功能参考](../implemented-features.md)为准。
