# M18 执行合同：正确性收敛与可信基线

> 状态：待实施。本文只新增任务与验收合同，没有修复其中记录的编译器问题。
>
> 入口：[M18+ 总交接计划](plan-m18-plus.md)。一次只执行一个 `H18-NN` 批次，不把整篇作为一次编码任务。
>
> 审计源码基线：`a72db41f4c254a67d0be62be8adcc47598f54b34`，2026-09-17，Linux x86_64，LLVM 22.1.8。实际执行必须重新核对 HEAD、工作区与前置报告，禁止 checkout/reset 到审计提交。

## 1. 目标与非目标

M18 不增加一组新语言特性，而是兑现 M1-M17 已承诺的语义、修复组合行为，并让验收持续执行。

必须交付：聚合赋值不丢失别名修改；枚举布局自洽；饱和转换在两个后端一致；泛型约束与 impl 头不再静默丢失；IR 校验；包来源和项目构建回归；LLVM CI 与发布门禁；准确文档。

明确不做：GC/借用检查器、闭包、异步、宏、完整 trait solver、一般化定长数组、第三后端、稳定 Dolphin 二进制 ABI、全量 HIR/SSA 重写、公共包仓库网站。修复必须允许小范围重构，但不能把这些大工程变成前置条件。

已有的手动内存契约保持不变：值默认浅复制，指针/切片/字符串视图不延长存储寿命；没有自动析构；trap/OOM 不执行栈展开和 defer。不能为了通过测试悄悄引入 RC 或深复制。

## 2. 证据与工作纪律

### 2.1 已执行的审计基线

以下是上述提交的一次本机记录，不是未来批次的验收结果，也不是三平台证明：

| 检查 | 结果 |
| --- | --- |
| `cargo test --workspace --exclude dolphin-codegen-llvm` | 229 项通过；`tests/backend.rs` 为 0 项，因为 feature 门控 |
| `cargo test --workspace --features llvm` | 233 项通过；包括 4 项 backend 测试 |
| `DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test manifest --test packages` | 118 项通过 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo clippy --workspace --all-targets --features llvm -- -D warnings` | 通过 |
| 本文第 3 节最小用例 | 在现有测试全绿的情况下，仍复现 4 类缺口 |

不固定未来测试总数；新增测试后总数应变化。不能通过删测试、改错期望、`#[ignore]`、扩大 `cfg` 排除范围使检查变绿。

### 2.2 每批通用步骤

1. 阅读总计划的执行规则、本批与前置报告，检查 `git status --short`、相关 diff 和实际源码。查找实际存在的 AGENTS.md；不存在不能自行编造规则。
2. 先说明本批现状：已复现、静态发现、已经被前批修复，三者分开。证据过期时更新记录，不照旧重复改代码。
3. 正确性修复先加入失败回归，实际确认失败原因；再实施最小修复。最终提交状态不得遗留意图通过的红测试。
4. 修改公共 IR/layout 时检查两后端、函数 ABI、内存 intrinsic、标准库和 FFI 的使用点，不只改当前失败函数。
5. 运行本批定向测试与第 5 节相应门禁；报告未运行项及原因。只执行了 `dc check` 不能声称构建/运行通过。
6. 更新本批报告和进度行；不标记后续批次完成，不自动 commit/push/release，不触碰无关用户改动。

诊断测试优先断言错误类别、关键语义、源码范围，而不是绑定整段渲染文本或 TypeId 数值。编译器内部错误须返回诊断，不得把输入程序错误变成 panic。

## 3. 最小复现与正确期望

这些是独立程序，不可拼接成多个 main 的项目。持久化回归必须放在仓库测试中；不要依赖审计者的 `/tmp/opencode` 文件。

### 3.1 字段赋值丢失别名修改：已运行复现

```dolphin
struct Pair { x: i32, y: i32 }

fn mutate(p: *Pair): i32 {
    p->y = 9;
    return 7;
}

fn main() {
    var p = Pair(1, 2);
    p.x = mutate(&p);
    println("{} {}", p.x, p.y);
    return 0;
}
```

正确 stdout 为 `7 9\n`，exit 为 0，stderr 为空。审计时两个后端 Debug 均保留了旧的 `y=2`。原因是先加载整个结构体、执行 RHS、再把旧结构体快照整体写回。

入口：[Cranelift codegen](../crates/dolphin-codegen-cranelift/src/codegen.rs) 与 [LLVM codegen](../crates/dolphin-codegen-llvm/src/codegen.rs) 的 `Instruction::SetField`、`SetIndex`；[lower.rs](../crates/dolphin-hir/src/lower.rs) 的 `lower_field_assignment`、`lower_index_assignment`。

### 3.2 枚举布局：已运行复现

此用例需要项目目录，例如测试临时目录的 `src/main.do`。单文件模式拒绝 `use`，不能把该诊断误认为布局修复失败。

```dolphin
use std.mem;

enum Payload { Value(i64), Empty }

fn main() {
    println("{} {}", mem.size_of<Payload>(), mem.align_of<Payload>());
    return 0;
}
```

审计输出 `12 8\n`。当前连续存储按 size 递增，12 的步长不能保持 8 字节对齐；payload 从 offset=4 开始也与 i64 对齐不符。H18-02 的目标结果为 `16 8\n`，不是让测试接受 12。

入口：[layout.rs](../crates/dolphin-ir/src/layout.rs) 的 `layout_of`、`enum_payload_components`，两个后端的枚举组件偏移/加载/存储与 ABI 展开。

### 3.3 浮点转整数：已运行复现

```dolphin
fn convert(x: f64): i32 { return x as i32; }

fn main() {
    println("{}", convert(100000000000000000000.0));
    return 0;
}
```

正确 stdout 为 `2147483647\n`。审计时 Cranelift Debug 符合，LLVM Debug 为 `-2147483648\n`；LLVM 的该输出不是稳定承诺，普通越界转换可能产生 poison。

[语言设计](language-design.md) 和[功能参考](implemented-features.md) 已规定饱和转换。因此不得把错误结果写进规范，不得改成 trap 或未定义行为来迁就 LLVM。

### 3.4 类型声明 bound 被忽略：已运行复现

```dolphin
trait Mark { fn mark(self: *const Self): i32; }
struct Box<T: Mark> { value: T }

fn main() {
    val b = Box<i32>(42);
    println("{}", b.value);
    return 0;
}
```

应拒绝：`i32` 没有 `Mark` 实现。审计时程序被接受并输出 `42\n`。普通泛型函数的 bound 已有检查，不能据此把所有泛型约束判断为未实现。

入口：[lower.rs](../crates/dolphin-hir/src/lower.rs) 的 `build_templates`，[monomorphize.rs](../crates/dolphin-hir/src/monomorphize.rs) 的 `StructTemplate`、`EnumTemplate`、`instantiate_named`/`instantiate_named_at`、`bind_param_bounds`。

### 3.5 静态发现、仍需各批运行确认

| 风险 | 定位 | 验证方向 |
| --- | --- | --- |
| impl 目标实参被丢弃 | [parser.rs](../crates/dolphin-syntax/src/parser.rs) 的 `parse_impl`、`discard_type_arguments` | 空 impl 也必须校验目标参数，不等到方法调用 |
| Path/Remote 来源冲突不对称 | [resolver.rs](../crates/dolphin-package/src/resolver.rs) 的 `acquire_dependency`、`acquire_path` | 两种解析顺序均拒绝冲突 |
| 路径依赖带 bin 时可能加载入口 | [driver](../crates/dolphin-driver/src/lib.rs) 的 `load_graph_sources` | lib+两个 bin 作为依赖不引入 main |
| 清单 Release 覆盖显式 Debug | driver 的 `build_manifest_with_graph`，CLI 的 `BuildArgs::profile` | 显式选项优先于清单，bin/lib 一致 |
| 循环内大返回值临时 alloca | LLVM codegen 的 sret 调用路径 | 验证是否循环累积栈；确认后才纳入修复 |
| macOS C 动态库 fixture 使用 ELF 选项 | [tests/ffi.rs](../tests/ffi.rs) 的 shared-library fixture | 区分 Linux soname 与 Darwin install_name |

## 4. 批次合同

下面的 H18 是新任务编号，不是历史 R00-R21。状态为待实施、进行中、阻塞、待平台验收、完成；完成须有第 6 节格式的报告。“待平台验收”不等于完成，也不能关闭里程碑。

| 编号 | 批次 | 前置 | 状态 |
| --- | --- | --- | --- |
| H18-00 | 基线复核与测试矩阵准备 | 无 | 完成（Linux 本机基线，见 [m18-progress](reports/m18-progress.md)） |
| H18-01 | 聚合写入与求值顺序 | H18-00 | 完成（Linux 本机两后端×两 profile，见 [m18-progress](reports/m18-progress.md)） |
| H18-02 | 枚举布局与内存步长 | H18-01 | 完成（Linux 本机两后端×两 profile，见 [m18-progress](reports/m18-progress.md)） |
| H18-03 | 饱和数值转换 | H18-02 | 完成（Linux 本机两后端×两 profile，见 [m18-progress](reports/m18-progress.md)） |
| H18-04 | 类型声明泛型约束 | H18-03 | 完成（Linux 本机两后端×两 profile，见 [m18-progress](reports/m18-progress.md)） |
| H18-05 | impl 头与不支持语法的拒绝 | H18-04 | 完成（Linux 本机两后端×两 profile，见 [m18-progress](reports/m18-progress.md)） |
| H18-06 | IR 与 LLVM 校验 | H18-05 | 完成（Linux 本机两后端×两 profile，见 [m18-progress](reports/m18-progress.md)） |
| H18-07 | 包来源身份与顺序无关性 | H18-06 | 完成（Linux 本机两后端，见 [m18-progress](reports/m18-progress.md)） |
| H18-08 | 项目目标与 profile 一致性 | H18-07 | 完成（Linux 本机两后端；profile 用 Debug runtime 行为验证，见 [m18-progress](reports/m18-progress.md)） |
| H18-09 | LLVM CI 与发布质量门禁 | H18-08 | 完成（配置与 Linux 本机步骤已验证；Windows 复验：默认 lane 与打包冒烟本机通过，CRLF 格式门禁已按用户决定以 `.gitattributes` 修复并复验；macOS 复验：默认 lane、Darwin ffi、打包冒烟与 LLVM 22 lane 本机通过，rust-lld/libLLVM 环境缺口与两个 ELF 专用 DWARF 断言已按用户批准最小修复；远端 CI/tag 门禁待平台验收，见 [m18-progress](reports/m18-progress.md)） |
| H18-10 | 当前文档与可运行示例核正 | H18-09 | 完成（文档示例抽取测试与示例修复，见 [m18-progress](reports/m18-progress.md)） |
| H18-11 | 全量集成与阶段验收 | H18-10 | 完成（Linux 本机全量门禁 + `examples/m18` + 发行包冒烟；三平台默认 lane 与 LLVM lane 由用户确认远端 CI 通过，见 [m18-progress](reports/m18-progress.md)） |

上述顺序是默认交接顺序，不建议多个代理同时修改 lower/layout/codegen。H18-06 若超过一个上下文，可先做 ID/CFG/location 校验，再做类型/表达式/Place 和 LLVM 校验；两个子批次都完成后才能关闭 H18-06。

### H18-00：基线复核与测试矩阵准备

输入：本文第 1-3 节、[tests/build.rs](../tests/build.rs)、[tests/backend.rs](../tests/backend.rs)、[Cargo.toml](../Cargo.toml)、当前 CI。

操作：

1. 记录 HEAD、操作系统/架构、rustc/cargo/llvm-config 版本、`DOLPHIN_BACKEND` 是否设置及本地改动。
2. 重跑默认测试；有 LLVM 开发环境时重跑 LLVM。缺环境可以完成基线记录，但后端修复批次不能最终验收。
3. 在临时目录重现第 3 节四例；将程序、命令、stdout/stderr/exit 和结论写进报告。若现已修复，定位修复与现有覆盖。
4. 优先复用已有测试驱动。需要共享运行函数时只抽取最小的“构建、限时运行、捕获三路结果”能力，不新建测试框架。
5. 新用例必须可以显式选择 `BackendChoice` 和 Dolphin `BuildProfile`；不要在并行 Rust 测试中修改全局环境变量。

验收：基线报告完整；新建测试基础设施自身有成功、失败、超时用例。超时要终止并回收进程，持续排空 stdout/stderr，避免管道写满死锁；不要求本批改造所有历史测试。

禁止：把第 3 节当前错误结果作为正确期望固化，或留四个未修复红测试后宣称阶段验收通过。失败回归随 H18-01/02/03/04 分别落地。

### H18-01：聚合写入与求值顺序

范围：lower 的赋值路径、IR Place/写指令、两个后端对应发射代码、定向回归。

需要明确并写入语言设计的顺序：

- 简单赋值先求目标位置（含指针/索引及必要边界检查），每个有副作用的目标表达式只执行一次；再求 RHS，最后只写目标子对象。
- 复合赋值还需要在 RHS 前读取一次目标旧值，计算后只写目标；不得重复索引调用或重新选择地址。
- 对象的其他字段/元素若被 RHS 修改必须保留。整个对象赋值仍是按值复制；不能把整值复制也改成引用绑定。

实现建议：普通 `p.field`/数组写入复用已有 `Place` 与地址存储路径。先阅读 `SetFieldAt`/`SetIndexAt`；需要统一 Store 指令时保持最小改动，不强制某个新命名。不接受“仅把整个聚合读取移到 RHS 后”的修补，因为它仍保留全量写回和两套左值规则。

必须测试：

| 编号 | 场景 | 期望 |
| --- | --- | --- |
| PLACE-01 | 第 3.1 节字段别名例 | `7 9\n`，exit 0 |
| PLACE-02 | 数组 RHS 通过元素指针修改另一个元素 | 非目标元素保留修改 |
| PLACE-03 | 索引函数递增计数，简单/复合赋值 | 每次目标求值一次 |
| PLACE-04 | RHS 同时修改目标与非目标字段 | 简单赋值最终目标等于 RHS；复合赋值使用规定的旧值；非目标修改保留 |
| PLACE-05 | const/val 写入、索引越界 | 继续拒绝或按原契约 trap，不绕过权限/检查 |
| PLACE-06 | 指针字段、切片元素、已有复合字段类型 | 与普通局部对象一致；不新增原来不支持的左值语法 |
| PLACE-07 | 动态越界目标配合会打印标记的 RHS | exit=101，stdout 不出现 RHS 标记；检查先于 RHS |
| PLACE-08 | RHS 通过别名重绑定目标指针/切片描述符 | 写入求 RHS 前选定的原地址，复合赋值读取的是该地址的旧值 |

PLACE-03/04/08 还要断言固定事件序列，不仅计数和最终值。使用当前可表达的局部变量与指针构造 fixture；若某种重绑定形式确实无法用现有语法表达，补内部 IR 回归并说明，不为此新增左值语法。

每例显式跑两后端与两种 Dolphin profile。源码不支持的表达式接收者可用局部变量等价表达，不能为了测试顺手扩语言。

完成条件：所有新例有固定期望；两后端不再通过旧聚合快照覆盖目标外数据；历史聚合传参/返回/稳定地址回归通过。

### H18-02：枚举布局与内存步长

前置：H18-01。范围：公共 layout、两个后端的枚举布局消费点及类型布局测试。

本批采用保守的统一布局，不做压缩表示：当前三个 64 位目标中 tag 为 i32；有 payload 分量时 payload 起始偏移为 `align_up(4, 8)=8`，每分量沿用已有 8 字节表示，整体 align=8，size 按整体 align 向上补齐。完全没有 payload 的枚举 size=4、align=4。统一描述至少能查询 tag/payload offset、size、align，禁止两后端各自保留 `4 + ...` 常量。

这只是修复内部内存布局，不承诺稳定二进制 ABI，不改变 C extern struct 布局。原枚举值组件/sret 规则能保留则保留，但必须核查参数、返回、栈槽、字段、切片和复制都使用同一字节布局。

必须测试：

- LAYOUT-01：无 payload、一分量、多分量枚举；每个非零大小类型 `size % align == 0`。第 3.2 节变为 16/8。
- LAYOUT-02：`mem.alloc<Enum>(n)` 后逐元素初始化、取址、读写；不读未初始化存储，不使用尚未支持的定长枚举数组。
- LAYOUT-03：枚举嵌在 struct 中，struct 再作为枚举 payload；字段偏移与尾部填充正确。
- LAYOUT-04：payload 为 string、指针、结构体，及 `Option<T>`/`Result<T,E>`；match、函数传参/返回、mem.copy 均保留值。
- LAYOUT-05：Debug 释放完整切片不误报长度不匹配、正常退出无泄漏；两种 profile 的边界检测仍有效。
- LAYOUT-06：真实 C extern struct 测试继续通过。布局尺寸计算不得通过整数饱和静默伪造可分配大小；新增计算须有溢出防护。构造超过布局表示/既有聚合预算的嵌套类型负例，要求尺寸诊断而非 wrap、饱和到最大数或生成巨量 IR；内部边界单测不实际分配超大内存。

禁止：把 align 降到 1 来逃避 payload 对齐，或仅改 `size_of` 而不改物理偏移。不要测试 padding 的未初始化字节值。

### H18-03：饱和数值转换

范围：两个后端 cast 发射、数值语义参考、边界回归。

保持既有饱和政策，并在规范中补齐特殊值规则：

| 输入 | 整数结果 |
| --- | --- |
| 可表示有限数 | 向零截断 |
| 超过上界、正无穷 | 目标最大值 |
| 低于下界、负无穷 | 有符号最小值；无符号为 0 |
| NaN | 0（本批明确化） |
| 正零、负零 | 0 |

LLVM 优先使用 `llvm.fptosi.sat`/`llvm.fptoui.sat`；按 LLVM 22/inkwell 实际接口实现。不能在未证明边界正确时手写浮点 MAX 比较，i64/u64 最大值在浮点中不一定精确可表示。窄整数目标也必须按最终位宽饱和，而不是先转 i64 再截断。

矩阵：`f32/f64` 到 `i8/i16/i32/i64/isize/u8/u16/u32/u64/usize`，正负小数、±0、NaN、±Inf、上下界附近、负数到无符号。NaN/Inf 可由运行时参数参与的 IEEE 运算构造，避免只验证前端常量路径。至少一组通过函数参数传入转换。

CAST-01 为第 3.3 节固定输出；CAST-02 为上述矩阵。期望以整数文本/位宽规则生成，不能调用待测后端产生 oracle。检查 stdout 中的完整 i64/u64，不用进程退出码承载大整数。

完成条件：两后端、两 profile 均对固定期望通过；原整数窄化/符号扩展、浮点算术和 trap 规则不变。

### H18-04：类型声明的泛型约束

范围：AST 信息传递、模块解析、模板表、类型实例化、约束查询及诊断。

1. struct/enum 模板保留每个类型参数的单 trait bound，保留定义位置/作用域；复用已有泛型函数约束查询，不新建完整 solver。
2. 在具名类型实例化的公共入口检查 bound，而不是仅在构造表达式检查。字段、签名、嵌套类型、无 payload variant 也必须受约束。
3. 查询使用定义包/模块中的 trait 身份；同名跨模块 trait 不能误匹配。保留实例去重、递归占位和实例膨胀限制。
4. 本批支持 bound 绑定的关联类型用于 struct 字段和 enum payload，例如 `struct ItemBox<T: Has> { value: T::Item }`。检查成功后复用现有 `bind_param_bounds` 向类型声明环境绑定 `T::Item`；不能仅检查 impl 存在。不要顺手增加多个 bound 或泛型 trait。

最小正反例公共声明：

```dolphin
trait Has { type Item; }
struct Good { value: i32 }
struct Bad { value: i32 }
impl Has for Good { type Item = i32; }
struct Hold<T: Has> { value: T }
enum Maybe<T: Has> { Some(T), None }
```

分别编译：`Hold<Good>(Good(1))` 接受；`Hold<Bad>(Bad(1))` 拒绝；`val x: Maybe<Good> = Maybe.None;` 接受；`val x: Maybe<Bad> = Maybe.None;` 拒绝。这些是新增验收建议，不是审计已运行结果。

GEN-01：第 3.4 节拒绝；GEN-02：上述正反例；GEN-03：签名/嵌套/跨模块/跨包；GEN-04：已有函数 bound、关联类型、递归泛型与 stdlib 全回归；GEN-05：`ItemBox<Good>(1)` 接受、传 bool 拒绝、`ItemBox<Bad>` 拒绝，对 `enum ItemMaybe<T: Has> { Some(T::Item), None }` 同测。错误要说明类型、缺失的 trait 与实例化位置，不能只打印 `struct@N`。

未实例化泛型函数体是否完整检查是另一项前端设计，不在本批承诺；但不得因为“懒实例化”跳过已被请求的具名类型约束。

### H18-05：impl 头与不支持语法

范围：`parse_impl`、AST impl 目标、模块解析、方法/trait impl 表。不要继续解析后丢弃实参。

冻结本批可用子集：

- 非泛型命名 struct/enum 的固有或静态 trait impl。
- `impl<T> Box<T>`、`impl<A,B> Pair<A,B>` 这种目标参数与 impl 参数按位置一一对应的完整形式。
- 上述泛型目标的 `impl<T> Trait for Box<T>`，包括既有 `Iterator for SliceIter<T>`。
- 类型声明参数与 impl 参数可改名，按位置建立绑定，不能要求文字名称相同。

暂不支持的形式必须在声明处诊断，即便 impl 为空或方法从未调用：具体类型特化、实参重排/重复/嵌套、漏参/多参、未被目标约束的参数、blanket impl、泛型 trait 实参、方法独立泛型参数。impl 参数上的 bound 本批不新增支持，必须明确拒绝而不是忽略；保留 H18-04 类型声明自身的 bound 检查。

IMPL-01 正例：现有 Vec/SliceIter/Iterator 全部通过；`struct Box<T>` 对应 `impl<U> Box<U>` 能正确返回 U 类型字段。声明类型的 T 环境与 impl 的 U 环境必须分别按位置绑定；不能用 impl 参数名称去解析类型声明字段。

用两个独立程序分别测试“先构造对象再调方法”和“关联函数第一次请求类型实例”。后者可用 `impl<U> Box<U> { fn make(value: U): Self { return Box<U>(value); } }`，main 先执行 `val b = Box<i32>::make(7);` 再读字段；若实际解析器的关联调用写法不同，按现有合法写法改 fixture，不能跳过首次实例化路径。再组合 `Box<T: Has>` 和 Good/Bad 检查约束，防止已有实例缓存掩盖参数环境错误。

IMPL-02 反例头（分别配好类型声明，单独编译）：

```dolphin
impl Box<i32> {}
impl<T> Box {}
impl<T> Box<T, T> {}
impl<A, B> Pair<B, A> {}
impl<T> Pair<T, T> {}
impl<T> Box<Box<T>> {}
impl<T> Has for T { type Item = i32; }
```

IMPL-03：同名 trait 方法继续按当前规则拒绝冲突；不引入新的消歧语法。IMPL-04：跨包参数化 impl、签名匹配和私有 helper 解析保持正确。

若发现仓库已有合法用法超出上述子集，先报告冲突并请求范围决策，不直接收窄已发布行为。文档同步必须说明新增的是正确拒绝/约束，而不是宣称支持完整特化。

### H18-06：IR 与 LLVM 校验

第一层放在 `dolphin-ir`，返回后端无关错误，由 HIR/driver 转成 Diagnostic。不要为了报错引入 LLVM/Cranelift 依赖；当前 IR 是可变局部变量 + 表达式 CFG，不是 SSA。

必须检查：

- IR-01：Function/Type/Local/Block ID 与索引合法；先验证再解引用。Type::Struct/Enum 必须指向对应种类的 TypeDef。区分合法指针/切片间接递归与非法按值布局自环/互环；不能只用 visited 集放过所有环，也不能先调用递归 layout 再验证。畸形 IR 负例覆盖按值自环、互环及错误 TypeDef 种类。
- IR-02：普通函数 entry/跳转目标、branch 的 bool 条件、main 引用；库 `main=None` 合法，extern 无函数体合法。
- IR-03：赋值、参数/调用、返回、字段/variant/payload、表达式和 Place 的类型一致；保留合法 const coercion，不能用简单类型相等误拒绝现有程序。
- IR-04：每块 instruction/location 数量一致；source/location 引用有效。明确合成位置的既有约定，不能凭空要求所有位置都非零。
- IR-05：畸形 IR 单测返回错误，不 panic；正常 lib、extern、defer、泛型和 match 程序全部通过。

建议在 `ProgramLowerer::lower` 完成 Program 后统一校验，使 check/build/lib 都覆盖；核对所有公共入口，不要只接 CLI build 分支。verify 是内部契约检查，不代替用户程序的类型检查与内存安全检查。

第二层在 LLVM 发射函数后、DebugInfoBuilder finalize 后、优化前调用 `module.verify()`；优化后再次校验，再输出目标文件。错误注明优化前/后并返回原始 LLVM 信息，不 unwrap。Debug 和 Release 都执行。

LLVM verifier 能接受语法合法但语义错误的普通 fptosi，因此 H18-03 回归仍不可省略。若发现循环 sret alloca 问题，先增加限时运行/生成 IR 证据，确认后用入口块临时存储等最小方案修复并记录，不展开成新优化器。

### H18-07：包来源身份与顺序无关性

输入：[resolver.rs](../crates/dolphin-package/src/resolver.rs)、package/registry/lockfile、tests/manifest 与 tests/packages 的本地 HTTP/file repository fixture。

本批契约：同一 `(group,name)` 只允许一个精确版本和一个来源；Path 与 Remote 永远不是同一来源，即使坐标和内容相同。相同 canonical path 经不同别名引用仍可复用；不新增隐式 override/path patch。

1. 所有命中已有节点的分支都校验来源，不让坐标分支遇到 Path 时直接成功。
2. 规范化来源由统一规则处理；远程来源继续校验根仓库配置。不要只比版本，也不要靠遍历排序掩盖冲突。不要求本批新增“不同仓库 ID 同 URL 自动合并”。
3. 冲突诊断包含两条请求链、版本和来源，不打印 token/URL 凭据。

PKGSRC-01：Path 先加载再 Remote；PKGSRC-02：Remote 先加载再 Path；两者都失败。构造两个根 fixture 调整别名/依赖拓扑，实际确认遍历顺序，不能假设 TOML 文本顺序有效。

PKGSRC-03：同仓库同坐标菱形复用；PKGSRC-04：不同仓库/版本冲突；PKGSRC-05：locked/offline 不绕过来源检查，失败不写出新的有效锁状态；PKGSRC-06：相同 path 的两个别名和循环检测保持正确。

使用隔离缓存与测试仓库，不访问真实用户仓库、不修改全局 `~/.dolphin`。已有有效锁文件/归档属于持久数据，不能无版本地改格式或删兼容检查。

### H18-08：项目目标与 profile 一致性

范围：driver/CLI、tests/manifest、tests/cli。先复现再修复第 3.5 节两个项目问题。

BUILD-01：根应用依赖一个同时声明 lib+两个 bin 的 path 包；只加载依赖的库源码，不能把其 bin main 带入应用。库与 bin 共享 helper 仍可见，path 开发与发布包消费行为一致。

BUILD-02：冻结优先级 `显式 CLI --debug/--release > 根清单 build.optimization > Debug`。bin 与 lib 使用同一最终 profile；作为依赖的包不能反过来覆盖根 profile。CLI 必须能区分“没传 profile”与“显式 Debug”；只把一个 if 反转仍可能错误。

BUILD-03：显式 Debug 配合清单 Release 的程序使用 Debug runtime；用受控泄漏/非法释放例检查 stderr/exit，不能只断言 BuildProfile 枚举值。正常清理例必须 stderr 无泄漏报告。反向 Release 覆盖 Debug 同测。

BUILD-04：现有单文件、多 bin、纯库、native inputs、backend 显式选项优先级回归。

边界：当前 `build --lib` 同时打包 `.dlib` 是已文档化行为，归档又禁止 path dependency。该开发体验问题留到 H19-00 设计决策，M18 不擅自删除打包行为、不把 path 依赖偷偷塞进可发布归档。输出目录的 profile/backend 隔离同样涉及 CLI/脚本兼容，先保持当前公开路径，测试使用不同临时输出避免冲突。

### H18-09：LLVM CI 与发布质量门禁

范围：[ci.yml](../.github/workflows/ci.yml)、必要的测试/打包脚本、安装说明。不新增公共服务或更换 CI 平台。

1. 保留 Linux/macOS/Windows 默认 Cranelift lane；增加固定 LLVM 22 开发环境的 Linux lane，执行第 5 节 LLVM 命令。明确安装路径/版本检查，不能依赖 runner 偶然预装的 LLVM。
2. `tests/backend.rs` 现有对照程序除一致性外还断言 exit=25、stdout=`7 10 12 25\n`、stderr 为空。
3. 在 tag 上也运行质量门禁。发布必须依赖同提交的测试和解压冒烟成功；检查 `if` 与 `needs` 组合，避免依赖 job 被 skip 后发布也意外 skip，或用 `always()` 绕过失败。
4. 优先测试随后上传的同一份归档；若另一个 job 重新构建，必须重新对实际上传归档冒烟，不能把之前不同产物的测试当证明。
5. M14/M15 的“无泄漏”冒烟显式捕获并检查 stderr；仅 exit=0 不足以证明没有泄漏。
6. 核查 Darwin shared-library fixture，不向 macOS linker 传 ELF soname；三平台使用各自后缀与选项。

CI-01：PR/分支 push/tag 的 job 依赖表写进报告。CI-02：LLVM lane 非零测试数、确实执行 LLVM。CI-03：任一门禁失败均不可上传 release。CI-04：实际发行包解压、构建、运行和包消费冒烟。CI-05：格式检查与原三平台回归没有被移除。

本地不能伪造远端 CI 成功。未触发 tag/无远端权限时注明“配置与本机步骤已验证，远端门禁待验证”；不得为了测试自动打 tag 或创建发布。无 C/Rust/SDK 的干净环境可用性仍由 H21 验收，不把当前构建 runner 称为干净机器。

### H18-10：当前文档与示例核正

本次制定计划时已同步一部分入口和明显旧状态，但本批仍待实施；不能据页首更新就关闭此项。

1. 检查 README、implemented-features、language-design、installation、网站中英文内容，区分当前行为、历史规格、未来提案。
2. 修正自建 `std`、拥有对象 `val` 却调用可写 deinit、把语句 if 当表达式、strlen 用 c_ulong 等已知可疑示例；逐个编译验证再改，不能仅按文本替换推断正确。
3. 所有完整可运行示例应有明确类型：成功程序、预期诊断程序、片段、历史草案或未来语法。仅对完整程序自动化编译；不宣称所有 Markdown 代码块都能执行。
4. 建立轻量的文档 fixture/提取机制并进入测试。复用现有 Rust/Python 工具，不为此引入 Node 构建生态。网站与 Markdown 共享实例或显式映射，避免两套同名示例漂移。
5. 定向核查饱和规则、泛型支持子集、LSP 单文件限制、DWARF 范围、发行 SDK 依赖和 `.dlib` 版本政策。

DOC-01：文档索引可达、当前入口无已失效源码链接；DOC-02：完整正反例在各自模式按期望通过；DOC-03：修改过的网站中英文段落同步；DOC-04：已修复缺陷从“当前缺陷”移到历史修复记录并链接测试，不删除证据。

历史 M14/M15 的原始路径表可保留为已标注的历史资料，不要求恢复这些文件。不要把剩余未实现的工具功能写成已完成。

### H18-11：全量集成与阶段验收

1. 核对 H18-00 至 H18-10 报告及所有验收编号，缺项不能以“整体通过”覆盖。
2. 运行第 5 节完整本机门禁；关键语义矩阵为两后端乘两种 Dolphin profile，三平台默认 lane 通过。
3. 增加 `examples/m18` 作为组合回归示例：泛型容器/枚举、跨函数聚合与指针别名写、defer 清理。示例不依赖网络、未初始化读或未实现语法，写明 stdout/exit/stderr。
4. 核查安装/打包说明、旧里程碑回归、发行包 smoke；编译器/包版本是否变更单独请求发布决策，不自动 bump/tag。
5. 给出仍未承诺的边界：内存安全、完整调试、一般数组、增量编译和包兼容策略等。确认无 P0/P1 已知正确性问题被无理由推迟。

只有实现、测试、文档和所承诺平台的证据齐全，才把 roadmap 的 M18 改为已完成。远端 CI 或必要平台不可验证时保留“待平台验收”，不要把本机结果复制成三平台结果。

## 5. 验证命令与解释

以下命令在仓库根目录执行，POSIX shell 语法。Windows PowerShell 使用 `$env:DOLPHIN_BACKEND = "llvm"` 等价设置，并在验证后恢复原值。CI 环境建议使用 job/step 的 env 配置。

### 5.1 每批默认门禁

```bash
cargo fmt --all -- --check
cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings
DOLPHIN_BACKEND=cranelift cargo test --workspace --exclude dolphin-codegen-llvm
git diff --check
```

### 5.2 触及 IR、lower、后端、runtime 的批次

先确认 LLVM 22 开发库可用；仅有 llvm-config 版本输出不代表能链接。

```bash
llvm-config --version
cargo clippy --workspace --all-targets --features llvm -- -D warnings
DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm --test build --test ffi --test cli --test manifest --test packages
cargo test -p dolphin-compiler --features llvm --test backend
```

新增独立测试文件后，必须把它补进显式 `--test` 列表，或增加覆盖它的命令。只跑旧列表不能验收新测试。

`--features llvm` 只是编入后端，未设置后端时大部分集成测试仍走 Cranelift。`cargo test --release` 优化的是 Rust 测试程序，不会把 fixture 的 `BuildProfile::Debug` 自动改成 Dolphin Release。新回归须在 Rust 中显式迭代 profile/backend，并在失败信息中打印二者。

### 5.3 集成与示例

```bash
cargo build --release --bins
./target/release/dc fmt --check examples crates/dolphin-std/src
./target/release/dc check examples/m14
./target/release/dc run examples/m14
./target/release/dc run examples/m15
```

M14/M15 正常 exit=0；M15 stdout 为 `42 22 true\n`。M8 等旧示例有非零的预期退出码，不能用 shell 的 `set -e` 直接把所有非零退出当失败，也不能用 `|| true` 吞掉真正错误；在测试驱动里捕获后断言具体值。

H18-11 增加 m18 后再将其命令加入，不能现在执行不存在的示例。发行打包命令按 installation 和 scripts/package.py 实际参数使用；不要求每个小批次生成发行包。

## 6. 报告与验收映射

执行者在完成首批时创建 `docs/reports/m18-progress.md`，之后每批追加一节。该路径是计划产物，不是当前已有文件。新目录/文件不得覆盖用户已有报告。

每节按下列字段填写：

```text
批次：H18-NN
状态：完成 / 阻塞 / 待平台验收
开始 HEAD 与已有本地改动：
前置批次及报告：
修改文件与关键实现：
验收映射：PLACE-01 -> tests/xxx.rs::实际测试名 -> 后端/profile/平台 -> 结果
修复前复现结果：
修复后结果：stdout / stderr / exit 或诊断
实际运行命令与测试数量：
未运行的检查及原因：
行为/兼容变化：
剩余问题和下一批输入：
```

一个参数化测试可对应多个验收编号，不必机械地每编号建一个 helper。报告引用真实测试名；不得在未执行时填“通过”，不得只写“测试全绿”。若测试失败，保留必要错误输出、最小重现和下一步，状态为阻塞。
