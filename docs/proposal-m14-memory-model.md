# M14 实现规格：手动内存管理与 C 互操作（已完成）

> 状态：v2 已按 R00–R08 实现并验收，M14-A–F 全部完成；当前项目已推进到 M17。
>
> 范围：内存、指针、切片、字符串视图、清理控制流、C ABI 与原生链接。
>
> 衔接：[M15 实现规格](proposal-m15-generics-stdlib.md)负责用户泛型、方法、标准库与库包发布，M15-A–F 也已完成。
>
> 进度入口：[路线图](roadmap.md)，当前能力见[已实现功能参考](implemented-features.md)。本文保留已完成的语义规格与历史实施过程；旧批次见[归档实施指南](plan-m14-m15-rework.md)。当前执行见 [M18+ 计划](plan-m18-plus.md)与 [M18 正确性计划](plan-m18-correctness.md)，不得从 M13 重做。

## 1. 给实现模型的说明（历史基线）

**`9e6f4c6` 仅是当年从 M13 开始重实现 M14/M15 的历史基线，不是当前源码基线，也不是回退目标。** 当时旧 M14/M15 实现已清理，M13 仅有标量、数组、静态字符串、模块、结构体、枚举与 match，尚无指针/切片类型、堆分配、defer、用户泛型、方法/trait 或内置 Dolphin 标准库；这些缺口现已由 M14/M15 v2 补齐。后续代理应核对当前源码和测试，只修复可复现的具体问题，不得按这段历史描述重建或删除现有实现。

本文中的“必须”保留为语义与回归验收要求，“延后”不是本规格的实现任务。正文中的“新增”“尚无”“目前”“待实现”及从 M13 起步的阶段顺序均是历史实施措辞，不描述当前缺失能力。Dolphin 示例保留 M14 的目标语法，实际可运行工程与覆盖范围应核对当前 `examples/m14` 和测试；本文不声明本轮重新运行了全部示例。返回类型统一写 `: T`，结构体统一采用位置构造 `Point(1, 2)`。

**历史小上下文执行方式（不再用于派发新实现任务）**：先读第 1、2、9 节，再按阶段加载正文：A→第 3 节；B→第 3、6 节；C→第 3、4、8 节；D→第 5 节；E→第 7 节；F→基线衔接表与第 10 节。当前回归审计仍可按这些引用查阅语义，但任务选择以 M18+ 新计划为准。

下表保留当时的源码入口和新增工作，并非当前文件索引。仓库现已拆为 `crates/` workspace，旧 `src/*.rs` 链接可能不再存在；从实际 [Cargo.toml](../Cargo.toml) 和 [crates/](../crates/) 定位，不要因历史路径失效而新建重复模块：

| 历史文件 | M13 当时已有基础 / 当时新增工作 |
| --- | --- |
| [ast.rs](../src/ast.rs)、[parser.rs](../src/parser.rs)、[token.rs](../src/token.rs)、[lexer.rs](../src/lexer.rs) | TypeRef 只有具名类型/定长数组；新增指针、切片、const/null、defer、extern 与 intrinsic 显式类型实参 |
| [lower.rs](../src/lower.rs) | 已有标量/数组/struct/enum 检查与 CFG；新增聚合值参数/返回、字段写入、place、取址/解引用、分配及作用域清理 |
| [ir.rs](../src/ir.rs)、[codegen.rs](../src/codegen.rs) | 复用类型化 IR、聚合值分量展开和底层栈/load/store 机制；新增语言级指针/切片及相应布局/指令 |
| [unix_runtime.c](../runtime/unix_runtime.c)、[windows_runtime.cpp](../runtime/windows_runtime.cpp) | 仅有输出、字符串比较、trap 处理；新增分配/释放、UTF-8 校验、Debug live 表及退出报告 |
| [build.rs](../build.rs)、[platform.rs](../src/platform.rs)、[linker.rs](../src/linker.rs) | 运行时预编译与内嵌、本机 ABI、LLD 链接；目前不是通用 C FFI |
| [manifest.rs](../src/manifest.rs)、[lib.rs](../src/lib.rs)、[main.rs](../src/main.rs) | 清单、构建入口与 CLI；需要接入原生链接输入 |
| [tests/build.rs](../tests/build.rs)、[tests/cli.rs](../tests/cli.rs) | 复用测试驱动，增加本文测试矩阵 |

### 1.1 设计约束与历史教训

下表只解释设计依据，**不是对当前代码的描述，也不是恢复旧功能的任务清单**。

| 旧设计 / 旧说法 | v2 决策 |
| --- | --- |
| 曾经实现过就必须保留兼容 | 当时重实现仅以 M13 为基线；未进入 M13 的旧 API/语法不提供兼容别名，不得据此删除当前已交付的 v2 能力 |
| 复制指针不会产生共享问题 | 复制地址会产生别名；共享、悬垂和重复释放由调用契约约束 |
| 不写 null 就能保证指针非空 | 原始指针允许 null，C 返回的空指针必须能表达 |
| `string` 与可写 `[]u8` 等价 | 布局相同不代表类型相同；字符串和只读字节视图不能被直接写入 |
| 字符串 `+` 隐式分配，接收者 free | 不引入字符串 `+`，M15 使用返回拥有型结果的 `concat` API；避免中间分配丢失 |
| `try` 的简单语法检查能保证不逃逸 | 不引入资源 try 语法；仅用 defer，不建立借用检查系统 |
| 释放后读取 header 魔数检查 double-free | 不得读取已释放内存；Debug 在独立登记表中验证释放 |
| 悬垂引用由 trap 兜底 | 不承诺检测悬垂引用，操作系统也不保证对错误地址立即报错 |
| 已有 runtime C 函数就是支持调用 C | 必须补齐用户可声明的 `extern "C"`、ABI 类型和链接配置 |

## 2. 最终设计：像 Zig 一样显式，日常代码保持简洁

1. **没有 GC、RC、自动析构、隐式 move 和借用检查器。** 不引入 `'a`、`&mut`、生命周期参数或生命周期推断系统。
2. **值默认复制。** 结构体、枚举、数组复制其值；指针和切片字段只复制描述符，不复制底层分配。
3. **分配与释放可见。** 用 `mem.alloc/free`、`mem.create/destroy`，或标准库拥有型对象的 `init/deinit`、`clone`。
4. **释放责任是 API 契约。** “拥有者”是文档术语，不是类型系统的线性所有权；复制拥有型对象不会自动转移责任。
5. **`defer` 是唯一推荐的作用域清理语法。** 普通调用、跨函数返回、C 资源都走相同机制。
6. **C 互操作是语言地基。** 普通 C 函数可直接声明并调用；调用者负责声明与 C 头文件一致。
7. **首版使用全局分配器。** 不要求每个业务 API 传 allocator；自定义 allocator/arena 延后，不能暗中实现 GC。

```dc
use std.mem;

fn make_bytes(): []u8 {
    val bytes = mem.alloc<u8>(4);
    bytes[0] = 68_u8;
    bytes[1] = 111_u8;
    bytes[2] = 108_u8;
    bytes[3] = 0_u8;
    return bytes;                   // 约定：调用者负责 free
}

fn main() {
    val bytes = make_bytes();
    defer mem.free(bytes);
    println("{}", bytes[0]);
}
```

## 3. 类型、布局和可变性

### 3.1 语法与规则表

| 形式 | 含义 | 复制行为 |
| --- | --- | --- |
| `T`、`[T; N]` | 普通值 | 复制全部分量；不隐式申请内存 |
| `*T` | 可写原始指针，可为 null | 复制地址 |
| `*const T` | 只读原始指针，可为 null | 复制地址 |
| `[]T` | 可写视图 `{ ptr: *T, len: usize }` | 复制地址与元素个数 |
| `[]const T` | 只读视图 `{ ptr: *const T, len: usize }` | 复制地址与元素个数 |
| `string` | 合法 UTF-8 的只读字节视图 | 复制地址与字节数，不拥有内存 |
| `usize` / `isize` | 目标指针宽度的无符号 / 有符号整数 | 当前三种 64 位目标上为 64 位 |

- `const` 只出现在指针/切片元素限定位置；它表示不能通过这个视图写入，不承诺没有其他可写别名，不是 Rust 借用。
- `var`/`val` 控制**绑定和值对象**是否可改；`val buf: []u8` 可以写 `buf[0]`，但不能重新绑定 `buf`。`val array: [u8; 4]` 的元素仍不可写。
- 从 `[]T` 到 `[]const T`、从 `*T` 到 `*const T` 可隐式增加只读限定；反向转换是编译错误。此规则仅限同一层，不能递归把 `**T` 转成 `**const T`。
- `&var_local` 得到 `*T`；`&val_local` 和不可变函数参数取址得到 `*const T`。不能经可写指针绕过 `val`。
- `.ptr`、`.len` 是切片只读字段；`.len` 是元素数，`string` 的 `.len` 是字节数。保留 `length(x)` 作为 `.len` 的兼容入口，并统一返回 `usize`。
- `string` 不支持下标读取/写入；按字节读取用 `s.bytes()` 返回的 `[]const u8`，不引入按字符下标。
- `null` 只能在已知指针类型的上下文使用，例如 `val p: *u8 = null`、`p == null`。`var x = null` 无法推断类型，应报错。
- M15 的 `Option<T>` 是普通枚举；`Option<*T>` 不等于 C 的 nullable pointer，也不做空指针布局优化。

### 3.2 取址与稳定存储

支持 `&x`、`&x.field`、`&slice[index]`，前提是表达式具有稳定存储位置；取址时也执行索引边界检查。禁止 `&make_value()`、`&(a + b)` 等临时值取址。

```dc
struct Point { x: i32, y: i32 }

fn update(p: *Point) {
    p->x = 9;
}

fn main() {
    var point = Point(1, 2);
    val p = &point;
    update(p);
    point = Point(3, 4);  // 原栈槽地址不变；p 仍指向 point，现在读到新值
}
```

必须建立统一的“可寻址位置”表示（local / field / index / dereference）。**取址不是简单地将 SSA 值临时复制到另一个栈槽。** 被取址局部变量须有唯一稳定存储，后续普通读写与指针读写访问同一位置。否则 `update(&point)` 后读 `point.x` 会读到旧值。

- `*p`、`p->field` 分别是解引用与字段访问；`p->x` 等价 `(*p).x`。
- M14 不支持指针加减；连续内存通过切片访问。
- 对有效原始指针的解引用，Debug 插入 null 检查；非 null 的悬垂、越界原始指针不保证检测。
- 结构体自引用必须经过指针。按值递归无论经过结构体、数组还是枚举，都是无限大小，应报告布局循环。

### 3.3 内存布局

集中实现 `layout_of(T) -> { size, align, field_offsets }`，禁止 codegen、FFI、分配器各算一套布局。

- 结构体字段按声明顺序排列，各字段前补齐对齐，结构体末尾补齐到最大字段对齐。
- 数组元素步长为元素完整 `size`，包含尾部 padding。
- Dolphin 枚举采用统一布局：tag 为偏移 0 的 i32；有 payload 分量时 payload 从 `align_up(4, 8) = 8` 开始，每个统一分量占 8 字节，整体 align=8，size 按 align 向上补齐；完全没有 payload 时 size=4、align=4（H18-02）。同一份描述由公共 `layout` 提供，参数、返回、栈槽、字段、切片和复制使用同一字节布局；此布局只用于 Dolphin，不是 C ABI，也不同步到 C extern struct。
- 指针的布局不递归展开指向类型。
- `Unit` 可作为无返回值；允许 `*Unit` / `*const Unit` 表示 C 的 `void*` / `const void*`，禁止解引用。首版堆分配拒绝 `size == 0` 的元素类型，零大小类型分配延后。
- 普通 Dolphin 聚合值的内部调用约定可以继续按分量展开；**这不等于 C 聚合值调用约定**。

历史上 M13 拒绝 struct/enum 作为函数参数和返回值，字段也不能单独写入。M14-A 已新增这两项能力：值接收者复制完整有效值，var 字段写入更新原存储；M14-B 已加入指针操作。当时的验收要求是不能仅凭 `component_layout` 等底层工具就认定这些语言功能完成；如今仍须用对应回归测试核对行为。

## 4. 分配、释放与初始化

### 4.1 对用户公开的 API

以下 `std.mem` 入口可以先以编译器 intrinsic 接入；M15 的容器必须调用它们，不得按容器名特判。

| API | 结果及责任 |
| --- | --- |
| `mem.alloc<T>(count: usize): []T` | 分配未初始化的连续存储；调用者写入后才能读取；原始切片交 `mem.free` |
| `mem.free<T>(buffer: []T)` | 只释放这一块连续存储，不递归释放元素字段 |
| `mem.create<T>(value: T): *T` | 分配一个已初始化对象；复制 value；对应 `mem.destroy` |
| `mem.destroy<T>(ptr: *T)` | 只释放对象本身，不递归释放字段 |
| `mem.size_of<T>(): usize` / `mem.align_of<T>(): usize` | 编译期布局常量 |
| `mem.copy<T>(dst: []T, src: []const T)` | 相同长度、允许重叠，按值复制；不分配，不深拷贝 |
| `mem.is_valid_utf8(bytes: []const u8): bool` | 只校验编码、不分配、不 trap；供高层 Result 型字符串转换复用 |
| `mem.view<T>(ptr: *T, len: usize): []T` | 从 C 指针创建视图，不取得释放权；有效范围由调用者保证 |
| `mem.view_const<T>(ptr: *const T, len: usize): []const T` | 同上，只读 |
| `mem.cast_ptr<T>(ptr): *T` | 在可写对象指针与 `*Unit` 间显式转换，校验目标对齐；不移除 const、不允许整数转指针 |
| `mem.cast_const_ptr<T>(ptr): *const T` | 对应只读转换，允许增加 const |

**M13 没有泛型调用语法。M14-C 必须新增 intrinsic 专用的显式类型实参解析**：例如 `mem.alloc<i32>(n)`、`mem.size_of<Point>()`；将类型实参作为 AST 类型节点保存并解析为具体类型。M14 不支持用户 `fn f<T>` / `struct Box<T>` / 泛型 impl；这些在 M15-A 实现。M15 单态化后把具体 T 交回同一 intrinsic lowering，不再增加第二套分配 API。

intrinsic 推断仅覆盖签名足以确定元素类型的调用，例如 `mem.free(buffer)`、`mem.create(value)`、`mem.copy(dst, src)`；alloc、size_of、align_of、cast 的目标类型须显式给出。M14 的 T 只能是当时已有可布局类型（或 API 允许的指针类型），不能是未声明的用户类型参数。

**内置入口启动顺序**：M14-B 开始在 `modules.rs` 建立固定身份的内建 std 命名空间及 `std.mem` 模块声明，M14-C 接入分配/布局 intrinsic；`use std.mem;` 不要求磁盘上先有 `src/stdlib/std.do`，也不依赖 M15 的 lib/依赖加载。`s.bytes()`、`s.slice(start,end)` 和 `string.from_bytes(...)` 是 M14 的有限内建调用形态，直接降低为相应操作，不要求提前完成用户 impl/方法解析。M15 在同一 std 身份上新增 Dolphin 源码标准库。

std 的保留身份固定为 `dolphin:std:<compiler-version>`。M14 只需为内建符号保存这个身份，不要求提前完成完整 PackageGraph、仓库或泛型实例化；M15 再将它接入统一的包图。

M13 示例中的 `src/std/` 是用户普通模块。首次保留 std 名称时显式迁移仓库内这些示例的模块名与导入，并更新相关回归；其他用户项目发生冲突时给出诊断，不覆盖/合并用户源码。

约束：

1. `count` 常量可按上下文推断为 `usize`；负数字面量和超出 usize 的字面量报错。有符号变量必须先检查非负再显式转换；底层 `as` 的数值转换语义不由分配器改变，分配器无法恢复转换前的信息。
2. 分配前检查 `count * size_of<T>()` 与运行时元数据大小计算溢出。OOM、大小溢出统一退出 `102`。
3. 返回地址满足 `align_of<T>()`。三个平台只支持自然对齐不超过 `max_align_t` 的类型；暂不开放用户自定义超大对齐。
4. `alloc<T>(0)` 返回 `{ null, 0 }`，不计入分配表；`free({null, 0})` 和 `destroy(null)` 是无操作。
5. `free` 只能接收 Dolphin 分配器返回的**完整原始切片**，地址、元素类型对应的字节数必须匹配。子切片、栈数组视图、静态字符串和 C 分配结果不能传入。
6. 未初始化元素只能先写后读。编译器不做完整初始化数据流检查，也不承诺 Debug 捕获读取未初始化内存。标准库不得把未初始化容量暴露为有效元素。
7. `mem.view` 的 null 指针只允许配合零长度；非零范围的有效性、存活期和写权限由调用者负责。
8. 每块分配只释放一次。别名数目不受追踪，“最后一个持有者自动释放”不是本语言规则。
9. `mem.copy` 的 src 覆盖范围必须全部已初始化，dst 可以是待初始化存储；只复制值，不为资源元素登记新的释放责任。
10. `free` 仅对规范空分配 `{null,0}` 无操作；非空 allocation 的零长子视图不能绕过完整范围校验。`mem.view(null, 非零长度)` 在两种 profile 下均以 101 失败。

### 4.2 切片操作

`slice.slice(start, end)` 返回半开区间子视图；保持元素只读性，零分配。始终检查 `0 <= start <= end <= len`，运算采用 `usize`，不在检查前做可能溢出的指针偏移。

索引必须在 `[0, len)`，Debug/Release 均保留边界检查。空切片不解引用 null。`mem.copy` 检查长度相等后按 `memmove` 语义处理，避免重叠复制错误。

### 4.3 跨函数和容器责任

| 操作 | 谁负责释放 |
| --- | --- |
| 返回新分配切片 / 拥有型容器 | 调用者；函数不能同时注册释放返回值的 defer |
| 返回已有对象的切片视图 | 原拥有者；调用者不得 free 视图 |
| 接收 `[]const T` / `*const T` 做查询 | 调用者保持存活；被调用方不释放、不长期保存，除非 API 明确说明 |
| 复制含指针的结构体 | 仍是原释放责任约定；复制品不是新的独立拥有者 |
| C 库分配资源 | 调用该 C 库指定的释放函数，例如 `sqlite3_close`，不能混用 `mem.free` |

## 5. `defer` 的精确控制流语义

语法：`defer call_expression;`。M14 新增返回 `Unit` 的普通函数、intrinsic 或 extern 调用清理；M15 方法实现后同样可作延迟调用。不支持 defer 块、嵌套 defer 或在 defer 中跳转。

1. 绑定最近的词法块。仅运行时已经执行到的 defer 生效；同块按注册顺序逆序执行。
2. **退出时求值。** 调用参数不是注册时的快照；名称在注册处绑定到局部变量身份，退出时读该变量的最新值。内层同名变量不能改变绑定。
3. 自然出块、`return`、`break`、`continue` 均执行实际退出的块的清理。
4. `return expr` 先求值并保存返回值，再从内到外清理，再返回。返回一个即将被释放的视图仍是调用者编程错误。
5. `break` / `continue` 只清理退出到目标循环所经过的块，**不能清理循环之外仍然存活的 defer**。循环体每轮都是新的作用域。
6. 运行时 trap、OOM、进程强制退出不展开 Dolphin 栈，因而不执行 defer；不能写“任何退出路径都执行”。

```dc
use std.mem;

fn main() {
    val outer = mem.alloc<u8>(8);
    defer mem.free(outer);
    for i in 0..3 {
        val inner = mem.alloc<u8>(4);
        defer mem.free(inner);
        if i == 1 { continue; }  // 只释放本轮 inner
        if i == 2 { break; }     // 只释放本轮 inner
    }
    outer[0] = 1_u8;             // outer 此时仍有效
}
```

实现要求：维护词法 scope 与循环出口的 scope 深度。为 CFG 的每条退出边生成清理序列；不能在 lowering 一条分支时 `drain` 共享清理元数据，导致另一个分支漏清理。延迟动作中的类型错误必须传播为源码诊断，不能被忽略。

### 5.1 不实现资源 `try` 兼容层

M13 没有 `try(...)` 资源语法；旧实现已清理，因此 M14/M15 不新增它、不恢复 try_resources，也不做专门的逃逸检查。局部资源使用 `mem.alloc` + `defer mem.free`，C 资源使用其释放函数 + defer。遇到历史样例的资源 try 写法按不支持的语法诊断处理，不能为了兼容历史提案重新引入另一套资源模型。

## 6. 字符串与优雅 API 的边界

- 字面量是静态 `string` 视图，永远不能 `free`。`s.bytes()` 零分配且只读。
- `string.from_bytes(bytes: []const u8): string` 校验 UTF-8 后返回视图，零分配；非法 UTF-8 退出 `104`。拥有者在视图存活期间不得释放或将字节改为非法 UTF-8；没有静态借用证明。
- 保持 M13 对字符串 `+` 的拒绝，不新增其 lowering；诊断可提示 M15 的 `std.text.concat` 和结果 deinit。数值 `+` 不变。
- M15 用拥有型 `String` 表达动态字符串，用 `string` 表达视图；`String.view()` 不分配，`String.deinit()` 释放。
- `trim` / `substring` 等查询默认返回视图；需要独立内存时显式 `clone`。`substring` 校验 UTF-8 边界，不能将任意字节区间包装成 string。
- 不允许 API 表面是普通查询，内部却返回必须释放的新分配而名称和返回类型都不说明。

M14 与 M15 的接口衔接：M14 先实现内存与 UTF-8 原语；`String`、`std.text.concat` 在 M15-B 实现。M14 用例显式分配字节、复制并建立字符串视图，不提前增加另一套拥有型字符串。

## 7. C 互操作：直接声明、直接调用

### 7.1 首版支持边界

“直接调用 C”指 Dolphin 源码声明 C ABI 函数，生成本机调用并链接已有 C 目标文件/库，无需每个函数再写 runtime 包装。**不要求解析 C 头文件。** 宏、内联函数、C++ 重载可以由库作者提供小型 C 包装。

```dc
// src/native/api.do
pkg native;

extern "C" {
    pub fn demo_add(a: i32, b: i32): i32;
    pub fn demo_fill(out: *u8, len: usize): i32;
    pub fn demo_create(): *Unit;
    pub fn demo_destroy(handle: *Unit);
}
```

```dc
use native.api;
use std.mem;

fn main() {
    val bytes = mem.alloc<u8>(16);
    defer mem.free(bytes);
    val status = api.demo_fill(bytes.ptr, bytes.len);
    if status != 0 { return status; }

    val handle = api.demo_create();
    if handle == null { return 1; }
    defer api.demo_destroy(handle);
    return api.demo_add(20, 22);
}
```

对应 C 声明使用 `int32_t`、`uint8_t*`、`size_t`、`void*`，测试中编译真实 C 文件核对。`extern` 内不写函数体；可见性与普通函数一致；没有 `pub` 的声明只在本模块可用。

### 7.2 C ABI 类型表

| Dolphin 类型 | C 类型 / 规则 |
| --- | --- |
| `i8/u8` … `i64/u64` | 对应 `int8_t/uint8_t` … `int64_t/uint64_t` |
| `f32/f64` | `float/double` |
| `usize/isize` | `size_t/ptrdiff_t`，按目标宽度 |
| `c_int/c_uint` | C `int/unsigned int`；当前目标均 32 位 |
| `c_long/c_ulong` | Linux/macOS 64 位；Windows MSVC 32 位，不能固定写成 i64 |
| `c_char` | C plain char；当前三个官方目标均为有符号 8 位，别名到 i8，供 C 字符串签名使用 |
| `*T / *const T` | C `T* / const T*`，原始地址，可能为 null |
| `*Unit / *const Unit` | C `void* / const void*` |
| 返回 `Unit` / 省略返回类型 | C `void`；不能作为按值参数 |

`c_int/c_uint/c_long/c_ulong/c_char` 是平台相关内建类型别名，不引入第二套数值运算。首版拒绝把 `bool`、Dolphin `char`、`string`、切片、普通结构体、枚举、泛型类型或 trait 直接按值写入 extern 签名；需要用 C 标量、指针、显式长度替代。C `_Bool`、可变参数、函数指针、回调、union、位域、packed struct、C 导出函数均延后。

对象指针的 pointee 只允许本表标量、Unit、extern struct 以及这些类型的固定数组/指针组合。普通 Dolphin 类型需先按 C API 契约转换成 `*Unit` opaque handle，不能凭两个结构体布局恰巧相同绕过 ABI 检查。同一原始 C 链接名在多个模块声明时，调用约定、参数与返回 ABI 必须一致；不一致在链接前标出冲突声明。

### 7.3 C 结构体

```dc
extern struct CPoint { x: f64, y: f64 }

extern "C" {
    fn demo_translate(point: *CPoint, dx: f64, dy: f64);
}
```

- `extern struct` 固定按目标 C 布局排列字段，支持 §7.2 的标量/指针、定长数组和嵌套 extern struct。
- 只能通过指针传给 C；**C 结构体按值传参/返回延后**，不能直接复用 Dolphin 的分量展开 ABI。
- 用同一个 C 测试库输出 `sizeof`、`_Alignof`、`offsetof` 与 Dolphin 比较。Windows 测试工具链不支持某项语法时用对应 MSVC 写法。
- 普通 Dolphin 结构体布局即使偶然一致，也不能自动当成 `extern struct`。

### 7.4 C 字符串与释放配对

C 的 `char*` 通常需要 NUL 结尾，Dolphin `string` 不承诺结尾有零字节。M14 测试可以手工创建 `[数据..., 0]` 缓冲；M15 提供 `CString` 封装。禁止直接把字符串 `.ptr` 当作 C 字符串。

测试必须包含：空指针返回、带长度的非 NUL 字节、C 修改缓冲、C 创建并由 C 销毁句柄。禁止将 C `malloc` 返回值传给 Dolphin `free`，也禁止把 Dolphin 分配的数据交给 C `free`。

### 7.5 原生链接清单

在现有 `dolphin.toml` 上增加以下结构；使用完整目标三元组，字段默认空数组，路径相对当前包目录：

```toml
[native.x86_64-pc-windows-msvc]
objects = ["native/windows/demo.obj"]
static-libs = ["native/windows/demo.lib"]
shared-libs = []      # Windows 填导入库 .lib，运行时 DLL 另见 runtime-files
runtime-files = []    # 构建 bin 时复制到可执行文件同目录

[native.x86_64-unknown-linux-gnu]
objects = ["native/linux/demo.o"]
static-libs = ["native/linux/libdemo.a"]
shared-libs = []      # .so 文件路径
runtime-files = []

[native.aarch64-apple-darwin]
objects = ["native/macos/demo.o"]
static-libs = ["native/macos/libdemo.a"]
shared-libs = []      # .dylib 文件路径
runtime-files = []
```

同一个 C 实现只在 `objects` 或 `static-libs` 中选一种，上例展示字段格式，不表示应重复加入同一符号。`shared-libs` 只接受具体文件，不接受任意链接器 flag。

实现规则：

1. M14 消费**预编译 C 文件**。编译 C 源码由库作者使用系统 C 工具链完成；dc 不增加 C 编译器，也不自动执行构建脚本。
2. 编译目标和 C 文件目标必须一致；缺文件、格式/架构不符、未解析符号均给出包含文件和目标的诊断。只构建宿主目标，交叉编译延后。
3. IR 的外部函数携带 `CallingConvention::C`、原始链接名、参数与返回类型。codegen 使用目标 C calling convention，整型扩展规则不能只按寄存器位宽猜测。
4. C 函数按原符号导入，不加 Dolphin 包 mangling。平台前缀只由现有目标符号层处理一次。
5. 链接输入统一为用户 object、native objects、native libraries、runtime 与系统依赖；默认 LLD 和 `--system-linker` 都要支持。静态库依赖顺序由调用方在列表内声明，首版诊断循环静态库依赖，不猜测重排。
6. Linux 共享库使用 `$ORIGIN` 搜索随程序附带的库；macOS 使用 `@loader_path` 并要求库本身有可重定位 install name；Windows 复制 DLL。随程序分发的 `.so/.dylib` 也必须列入 runtime-files，且其传递动态依赖由库作者一并声明；不递归扫描宿主机器的动态库。构建时不能留下发布者机器上的绝对路径作为运行时依赖。
7. 标准 C/系统库符号可以使用已有运行时系统链接输入；第三方库仍需明确清单文件。M15 负责把这些文件装入库包并合并传递链接需求。

共享库打包的可执行约束：`shared-libs` 对应的非系统运行时文件必须列入 `runtime-files`，包括需要随程序部署的传递动态库。ELF 共享库须有不含路径的 SONAME，与复制后的文件名一致；Mach-O 使用 `@rpath/<文件名>` 的 install name，并给 bin 设置 `@loader_path` 的 rpath，拒绝遗留发布者绝对路径；Windows 使用导入库加对应 DLL。首版不修改供应商库的内部依赖记录，无法满足这些约束时明确诊断。正常 `dc check` 只检查声明、清单及路径，C 符号解析和共享库可加载性由 build/run 与集成测试验收。

## 8. 运行时检测和失败行为

### 8.1 退出码

| 情况 | Debug | Release |
| --- | --- | --- |
| 算术 trap、切片越界、无效切片区间 | `101` | `101` |
| 分配失败 / 分配尺寸溢出 | `102` | `102` |
| 无效释放：未知地址、已释放地址、长度不匹配 | `103` | 不保证检测 |
| UTF-8 校验失败 | `104` | `104` |
| null 解引用 / 指针对齐检查失败 | `101` | 显式转换保留对齐检查；原始解引用不保证检测 |
| 正常退出时泄漏 | stderr 报告，保留程序退出码 | 不追踪 |

现有 Unix trap 处理器调用 `_Exit(101)`，不是“仅由信号退出，没有进程退出码”。Windows 也统一到明确进程退出码。运行时失败不执行 Dolphin defer；泄漏报告只保证正常退出路径。

### 8.2 Debug 分配表

首版用独立的**存活分配登记表**，记录地址、字节数和对齐：

1. 分配用户内存并登记。登记表自身申请失败同样按 `102` 结束。
2. `free` 先按地址查登记表，查不到就报 `103`；**查表之前不解引用用户地址，也不向前读 header**。
3. 查到后核对完整分配大小，移除记录，再调用系统 free。
4. 正常退出遍历尚存记录，打印数量、地址、大小到 stderr。
5. 可用链表起步；不能通过永不释放所有用户内存来伪造 double-free 检测。

边界：地址被新分配重用后，旧指针可能恰好命中新记录，普通地址登记表不能区分这种情况。测试和文档不得承诺捕获所有 stale pointer、use-after-free 或 double-free。

### 8.3 Debug/Release 资源选择

`build.rs` 为每个发行目标预编译两份运行时（检测版、普通版），分别内嵌；`BuildProfile` 决定链接哪一份。用户构建程序时仍不需要现场编译 runtime。不得以“目前只内嵌一份”为由声称无法区分两种配置。

## 9. 从 M13 实现的顺序与基线衔接（历史，已完成）

下表各阶段已完成，保留当时的前置关系、交付物与通过条件，不得再逐行派发为“从 M13 新增”的任务。

| 阶段 | 前置 | 修改范围 / 交付物 | 通过条件 |
| --- | --- | --- | --- |
| M14-A 基线与布局 | M13 | 记录 M13 检查结果；统一 layout/place、usize/isize；新增聚合传参/返回及字段写入 | 值复制/字段写入正确；布局循环有诊断，指针验收在 B |
| M14-B 指针与视图 | A | 从 AST 到 codegen 新增指针/切片、const/null、取址/解引用、视图操作及内建 std 身份 | const 反例、null、索引边界、字段地址通过；内建入口可解析 |
| M14-C 分配器 | B | 新增显式类型实参调用、std.mem intrinsic、runtime 分配/检测及双对象资源 | 对齐、零分配、尺寸溢出、无效释放、泄漏报告通过 |
| M14-D 清理控制流 | C | 从 parser 到 CFG 新增 defer；按退出边生成清理序列 | 自然退出/分支/return/break/continue 清理顺序全部通过 |
| M14-E C 互操作 | B、C、D | extern AST/IR、C 类型检查、extern struct、native 清单与链接 | 真正的 C 测试库在三个目标分别构建运行 |
| M14-F 集成与收尾 | E | 补齐 UTF-8 原语、示例、测试及实现参考；核对 M13 长度/模块名迁移 | 完成 §10，路线图按证据更新 |

基线衔接表：

| M13 基线或历史写法 | 实现要求 |
| --- | --- |
| 历史全局 `allocate` / `free` | M13 不存在这些内建；只新增 std.mem API，不提供全局兼容别名 |
| 历史 `try(...)` | 不实现资源 try，不恢复逃逸检查；只实现 defer |
| M13 静态字符串 | 新增只读 bytes/UTF-8 视图，string 仍不可释放；不引入字符串 + |
| M13 struct/enum 参数、返回及字段写入限制 | M14-A 新增支持，对应原“不支持”测试改为有依据的功能正例 |
| M13 `length` 返回值按 i32 使用 | 新版统一 usize；迁移显式类型和循环索引，不保留两套长度 ABI |
| M13 用户 `src/std/` 示例 | 首次引入内建 std 时迁移仓库示例名字；其他项目冲突明确诊断 |

有意改变的旧测试应改为新语义正例或诊断反例，并记录原因；不能单纯删除失败测试。用户已有其他未提交修改须先识别，不得顺手覆盖。

当时的回归基线是 M0-M13 测试，`examples/m14` 与对应测试随后按本规格新建。当前回归必须保留并覆盖已交付的 M14-M17 能力，不能退回仅验证 M0-M13，更不能删除现有 M14/M15 测试、示例或 runtime。

## 10. 验收用例（已交付规格，后续回归依据）

| 编号 | 输入 / 场景 | 可观察结果 |
| --- | --- | --- |
| MEM-01 | `Point` 按值传递后修改副本；再经指针修改原对象 | 副本独立，指针更新原值 |
| MEM-02 | 局部对象取址、字段取址、整体重新赋值、指针读写交错 | 所有路径读到同一存储的新值 |
| MEM-03 | 对 val 取可写地址、写 `[]const T`、取临时值地址 | 编译诊断，含位置 |
| MEM-04 | 分配 0 个、多个标量、含 padding 的结构体；create/destroy | 长度、步长、对齐正确；零长度不泄漏 |
| MEM-05 | 尺寸乘法溢出；用测试专用分配失败注入触发 OOM | 两配置都退出 102；不依赖真耗尽机器内存 |
| MEM-06 | 正常释放、立即重复释放、释放子切片 / 栈视图 | Debug 正常或 103；检测本身不读无效内存 |
| MEM-07 | 一块故意未释放，正常返回指定退出码 | Debug stderr 报告，退出码保持 |
| MEM-08 | if 两分支分别 return，一分支 fall-through，嵌套循环 break/continue | 日志精确证明仅退出作用域逆序清理，无漏执行/多执行 |
| MEM-09 | defer 参数变量被重绑定；参数含有副作用的函数调用；return 先计算再清理 | 仅在退出时求实参，验证求值次数及返回/清理顺序 |
| MEM-10 | 越界、倒置区间、无效 UTF-8、字符串字面量写入 | 运行时 101/104 或编译期只读诊断 |
| FFI-01 | 调用 C 的整数、浮点、多参数函数，含负窄整数、c_long、c_char 指针 | 返回值和参数 ABI 正确，Windows long 为32位，无 runtime 专用包装 |
| FFI-02 | C 写缓冲、返回 null、C 创建/C 销毁句柄 | 字节结果正确，释放配对 |
| FFI-03 | C/Dolphin 比较 extern struct size/align/offset | 三个官方目标一致 |
| FFI-04 | C 签名使用 string、切片、普通 struct、可变参数 | 编译期拒绝，不拖到链接器或运行时 |
| FFI-05 | 原生库路径带空格、缺库、错误架构、缺符号 | 正确构建或包含路径/目标的明确诊断 |
| FFI-06 | 静态库和共享库各一个集成示例 | 默认 LLD / 系统链接回退均可运行 |

以下保留当时的收尾命令，不是本轮文档维护需要执行的操作；当前 workspace 的检查范围与 feature 组合应按新计划及实际 CI 确定：

```text
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release --bins
```

当时已新增 `examples/m14` 的内存样例与 C fixture。后续回归仍需在三个官方目标（Linux x86_64、macOS ARM64、Windows x86_64）分别验证，不能把只在开发者机器上通过写成跨平台完成。

## 11. 明确延后

自定义 allocator/arena、自动资源析构、借用分析、指针算术、整数地址转换、C 头文件导入、C 可变参数/回调/函数导出、C 聚合值按值 ABI、union/位域/packed、交叉编译。M15 的库包发布不以这些能力为前提。
