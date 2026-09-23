# M19 规格：真实 CLI、标准库与用户测试（H19-00 冻结）

> 状态：H19-00 设计冻结产物；**H19-01..H19-07 已完成实现并通过三平台（Linux/Windows/macOS）
> 默认后端与 Linux LLVM 验收**（证据见 [M19 进度报告](reports/m19-progress.md)）。本文冻结的
> API/语义/退出码/资源规则均已落地，未新增语言语法（D3）。未实现条目不得写入“已实现功能”。
> 实现顺序与验收编号见 [M18-M21 计划](plan-m18-plus.md) 第 5 节与本文“测试矩阵”。
>
> 前置：M18 已完成并通过阶段验收（[m18-progress](reports/m18-progress.md)）。
>
> 决策请求：D1–D3 已由用户于 2026-09-22 确认（见第 12 节）。

## 1. 范围与非目标

M19 交付一个 `dtext` 文本统计/过滤工具（`examples/m19`），为此补齐最小标准库与 `dc test`。
本文冻结 API、语义、拥有权、错误行为、兼容影响、源码入口与测试矩阵。

不做（M19 明确排除）：allocator 参数、隐式析构、异常、`?`、闭包、线程/异步、宏、
HashMap/HashSet、正则、JSON、网络、完整 Unicode 算法、稳定二进制 ABI、跨平台交叉编译、
一般 block expression / if expression、任意新语法。

## 2. 总则（所有 API 共同遵守）

1. **无自动析构**：拥有型句柄必须显式 `close`/`deinit`；`defer` 是唯一作用域清理语法。
2. **浅复制**：结构体赋值/传参/返回按值浅复制；拥有型句柄的内部状态在堆上共享，
   复制句柄不产生第二份关闭权（第 6 节状态机）。
3. **分配失败**：`std.mem` 与拥有型容器的分配失败沿用 M14 契约——运行时以退出码 `102`
   终止，不返回 `Result`；I/O 失败才走 `Result`。
4. **trap 不展开**：越界/溢出/除零/非法 UTF-8 断言（`dolphin_check_utf8`）继续以
   101/104 终止且不执行 `defer`；I/O 层不新增 trap。
5. **进程退出码**：`main` 的返回码语义不变；Debug 泄漏报告与清理失败报告只写 stderr，
   不改写退出码（“不覆盖原错误”）。
6. **无字符串格式化依赖**：错误对象只携带稳定类别与 native code，诊断文本由调用方
   自行拼接或 `println`；测试断言不依赖格式化数字。
7. **平台错误显式**：不支持的路径/参数编码必须返回 `InvalidArgument` 或 `NotUtf8`，
   禁止静默替换字符后继续。

## 3. 决策 1：参数与环境 API

### 3.1 推荐方案

新增源码标准库单元 `std.process`（`crates/dolphin-std/src/process.do`），只提供**借用视图**，
不引入进程上下文拥有型容器：

```dolphin
pkg std.process;

pub enum ArgError { OutOfRange, NotUtf8 }
pub enum EnvLookup { Found(string), Missing, NotUtf8 }

pub fn arg_count(): usize
pub fn arg(index: usize): Result<string, ArgError>
pub fn program_name(): Result<string, ArgError>   // = arg(0)
pub fn env(name: string): EnvLookup
```

- `arg(index)` 返回的 `string` 是**借用视图**，有效到进程结束；不得 `deinit`、不得写入。
- `arg_count` 包含 `arg(0)`（程序名/路径）。`dc run` 的用户参数从 `arg(1)` 开始。
- `env(name)`：`Found(view)` 表示存在且值为合法 UTF-8；`Missing` 表示不存在；
  `NotUtf8` 表示存在但值不是合法 UTF-8。三种结果互斥，与“缺项”区分开。
- `name` 是合法 `string`（编译期已保证 UTF-8）；不会因 name 出错。

### 3.2 平台语义

- **Unix**：`arg` 直接指向 `argv[i]`；`env` 指向 `environ` 条目值。非 UTF-8 的
  argv/env 值由包装层校验后返回 `NotUtf8`，绝不替换字符。
- **Windows**：首次访问时用 `GetCommandLineW` + `CommandLineToArgvW` 取宽字符向量，
  逐个用 `WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, ...)` 转成 UTF-8 并缓存；
  含未配对代理项的条目在 `arg` 返回 `NotUtf8`（`arg_count` 仍计数）。缓存用运行时
  `malloc`（不走 `dolphin_alloc`），属进程生命周期资源，不计入 Debug 泄漏报告。
- 环境在 Windows 用 `GetEnvironmentStringsW` 建立同样的缓存表；大小写按平台原生规则
  （Windows 大小写不敏感，Unix 敏感），规格不承诺跨平台统一大小写行为。
- 视图构造与校验复用既有能力：运行时返回 `*Unit` 数据指针与长度，Dolphin 包装层用
  `mem.view<u8>(ptr, len)` 建立 `[]const u8`，再用既有 `std.text.from_utf8(bytes): Result<string, TextError>`
  校验并得到 `string`；`InvalidUtf8` 映射为 `ArgError.NotUtf8`/`EnvLookup.NotUtf8`。
  **不得使用会以 104 终止的 `string.from_bytes` 做“可恢复”校验**（该内建保持既有 trap 语义）。

### 3.3 `dc run` 参数转发（H19-01）

CLI 语法冻结为：

```text
dc run <项目或文件> [编译选项] -- <应用参数...>
dc run <项目或文件> [编译选项]                 # 无应用参数
```

- `--` 之后的所有参数（包括空字符串、含空格、Unicode、以 `-` 开头、`--help`）
  **原样**传给可执行文件，不经过 shell、不做引号/通配符展开。
- 没有裸 `--` 时，原有编译选项解析不变；`dc build` 不接受 `--` 之后的额外参数（用法错误 2）。
- 目录/单文件两种输入都支持；`dc run` 直接以子进程执行构建产物并透传退出码。
- 直接运行产物（不经 `dc run`）读取同一 OS argv；ARGS-01 断言两者一致。

### 3.4 拒绝方案

| 方案 | 拒绝理由 |
| --- | --- |
| `args(): []string` 拥有型数组 | 需要拥有型容器与逐项释放，且 Windows 需一次性分配；M19 不需要 |
| `args(): []const []const u8` 嵌套切片 | 语言不支持嵌套数组/嵌套切片 |
| `arg(i): string` 遇非 UTF-8 时替换/跳过 | 静默改变用户输入，违反显式错误要求 |
| 环境 API 返回 `Option<string>` | 无法区分“缺项”和“存在但非 UTF-8” |
| 给 `main` 增加 `args: []string` 参数 | 破坏无参数 `main` 的源码兼容，且与 defer/exit 语义纠缠 |

### 3.5 正反例

- 正例：`arg_count()==3`，`arg(1)` 为 `--filter`，`arg(2)` 为含空格 Unicode 文本。
- 正例：`env("PATH")` 为 `Found`，`env("DOLPHIN_NO_SUCH_ENV")` 为 `Missing`。
- 反例：`arg(99)` 返回 `ArgError.OutOfRange`，不 panic、不 trap。
- 反例：Unix 上传入非法 UTF-8 argv 时 `arg(i)` 返回 `ArgError.NotUtf8`，不替换字符。

## 4. 决策 2：字节 I/O API

### 4.1 推荐方案

新增 `std.io`（`io.do`），所有读写基于显式缓冲、返回 `Result`，不隐藏部分读写：

```dolphin
pkg std.io;

pub struct Stream { handle: usize }   // 私有字段；0 = 已关闭

pub fn stdin(): Stream
pub fn stdout(): Stream
pub fn stderr(): Stream

pub fn read(self: *const Stream, buffer: []u8): Result<usize, Error>
pub fn write(self: *const Stream, bytes: []const u8): Result<usize, Error>
pub fn write_all(self: *const Stream, bytes: []const u8): Result<bool, Error>
pub fn flush(self: *const Stream): Result<bool, Error>
pub fn close(self: *Self): Result<bool, Error>
pub fn close_abort(self: *Self)
pub fn is_open(self: *const Stream): bool
pub fn release(self: *Self): usize
pub fn from_raw(handle: usize): Stream
pub fn eprint(bytes: []const u8): Result<bool, Error>
```

成功类返回值用 `Result<bool, Error>`（`true` = 成功）：当前语言不能表达 `Result<(), E>`（`()` 不是类型；
`Result<Unit, E>` 构造会在 codegen 触发内部错误，H19-02 已改为实例化诊断），经用户 2026-09-22 确认
采用 `bool`；不新增 Unit 值语法。实现批次：H19-02 已落地标准流子集（`std.error` + `std.io` 的
`stdin/stdout/stderr/read/write/write_all/flush/close/close_abort/is_open/eprint`）；
`release`/`from_raw` 与 `std.fs.open` 一起在 H19-03 落地。

语义：

- `read` 请求至多 `buffer.len` 字节，返回实际读取数；`Ok(0)` 表示 EOF，不区分
  “文件结束”和“被关闭”。短读是正常结果，调用方必须循环。
- `write` 返回实际写入数；`write_all` 循环直到写完全部字节，写入 0 或出错时返回
  `Err`，此时可能已写入前缀（文档明确，不静默丢弃）。
- 运行时的 `read`/`write` 包装内部重试可重试中断（`EINTR`）；`Interrupted` 不作为
  `ErrorKind` 暴露。
- 最大单次读取由调用方缓冲大小决定，无内置上限；不提供“一次读完全部”的 M19 API。
- 标准流是**借用句柄**：`close` 返回 `Err(NotOwned)` 且不触碰 OS 句柄；
  `close_abort` 对借用句柄是无操作；`release` 对借用句柄返回 0 并保持打开。
  `print`/`println`（既有格式化输出）继续直接写 fd 1，不经过 `std.io`。
- `std.io.stdout()` 每次调用返回同一借用句柄；`is_open()` 对借用标准流恒为 true。
- `flush` 对无缓冲包装是透传（Unix `fsync` 不属于 M19；Windows 用 `FlushFileBuffers`）。

### 4.2 拒绝方案

| 方案 | 拒绝理由 |
| --- | --- |
| 提供 `read_all(): []u8` 拥有型缓冲 | 需要定义所有权/释放与无限大小上限，M19 用不上 |
| `read` 自动重试到填满 buffer | 会吞掉 EOF 与短读语义，阻塞型交互不可控 |
| 用 `usize` 负数/`0` 表示错误 | 与计数混淆；错误必须走 `Result` |
| 标准流允许 `close` | 会破坏运行时收尾与 `print`；且用户无权重置进程标准流 |
| 把 I/O 全部实现为 Rust 内建 intrinsic | 违反“平台逻辑放 runtime、业务逻辑放 Dolphin 源码库”的分工 |
| 暴露 `Interrupted` | 平台细节泄漏给应用；重试是包装层的职责 |

### 4.3 正反例

- 正例：IO-02 空输入 `read` 立即 `Ok(0)`；大输入多块读取拼接结果与输入字节完全一致。
- 正例：IO-03 受控 fixture 让 `write` 返回短写，`write_all` 仍写完全部字节。
- 反例：IO-04 非法 UTF-8 字节可经 `read` 读入 `[]u8`；`std.text.from_utf8` 返回
  `Err(TextError.InvalidUtf8)`（可恢复），而 `string.from_bytes` 保持既有语义以 `104` 终止；
  应用必须使用前者，不得把任意字节当合法 `string`。
- 反例：对 `stdout()` 调用 `close` 返回 `Err(NotOwned)`，之后 `print` 仍可用。

## 5. 决策 3：文件 API

### 5.1 推荐方案

新增 `std.fs`（`fs.do`），与 `std.io.Stream` 共用句柄类型：

```dolphin
pkg std.fs;

pub enum OpenMode { Read, Write, Append }

pub fn open(path: string, mode: OpenMode): Result<std.io.Stream, Error>
```

- `Read`：只读打开，不存在 → `NotFound`；目录 → Unix 为 `IsADirectory`，Windows 为
  `InvalidArgument`（`ERROR_ACCESS_DENIED`/`ERROR_INVALID_HANDLE`）；测试按平台断言对应类别。
- `Write`：创建或截断，只写；Unix 默认权限 `0644`（受 umask 影响），Windows 默认属性。
- `Append`：创建或追加，只写，写入位置在每次写入前置于末尾。
- 不提供 `ReadWrite`；目标工具不需要，避免额外状态。
- 路径是 `string`（合法 UTF-8，编译期保证）。含内部 NUL 的路径在调用运行时前返回
  `InvalidArgument`。**Unix 非 UTF-8 路径在 M19 无法表达**：来自 argv 的非法 UTF-8
  已在 `std.process.arg` 层返回 `NotUtf8`，不会被静默替换后打开错误文件。
- Windows 路径转 UTF-16 失败（含未配对代理项）→ `InvalidArgument`。
- 关闭/读写与 `std.io` 同一组方法（`read`/`write`/`write_all`/`flush`/`close`/
  `close_abort`/`is_open`/`release`/`from_raw`）。

实现状态（H19-03）：以上 `open` 与 `release`/`from_raw` 已落地；`Read` 在 Unix 用 `fstat` 识别目录并
返回 `IsADirectory`，Windows 用文件属性识别并返回 `InvalidArgument`。Debug 运行时在退出收尾报告
未关闭的自有流（`Dolphin: N open handle(s) not closed at exit`，退出码不变）；借用标准流不计入。

### 5.2 拒绝方案

| 方案 | 拒绝理由 |
| --- | --- |
| `File`/`Stream` 两套类型 | 方法重复、所有权规则要写两遍；目标程序只用一种句柄语义 |
| 提供 `ReadWrite` 模式 | 目标工具不需要；追加“位置读写”会引入 seek 状态机 |
| `val` 句柄也能 `close` | 可写接收者规则已冻结（`close` 需 `*Self`）；API 不破坏该规则 |
| 关闭失败后自动重试或 trap | 关闭失败不可靠重试；trap 会覆盖原错误，违反总则 5 |
| 用路径字符串的 NUL 截断打开 | 静默打开错误文件，违反总则 7 |
| M19 支持任意字节路径（`[]const u8` 重载） | 需要第二套路径类型与 Windows 语义，超出最小范围 |

### 5.3 正反例

- 正例：FS-01 空文件 `read` → `Ok(0)`；小文件一次读完；多块文件循环读到 EOF。
- 正例：FS-04 路径含空格与中文，三平台可打开（Windows 走 UTF-16 转换）。
- 反例：FS-02 不存在的文件 → `NotFound`；目录 → `IsADirectory`/`InvalidArgument`；
  路径含 `\0` → `InvalidArgument`，不调用系统 open。
- 反例：FS-05 `Write` 截断既有文件；`Append` 追加不截断。

## 6. 决策 4：资源状态与 defer

### 6.1 冻结的 defer 语义（M14 不变）

`defer call;` 在**注册处 lowering**（绑定当时的局部变量身份），在作用域退出时逆序发出，
因此**实参读取的是退出时的最新值**。M19 不改变这一点，也不允许把 `defer` 改成注册时
捕获值来回避重绑定问题（计划明确禁止）。

### 6.2 句柄状态机（新增 API 契约）

运行时为每个打开的非标准流维护一个堆上状态：`{ open: bool, owned: bool, native: ... }`。
`Stream.handle` 指向该状态（`usize` 不透明 id）；**句柄浅复制共享同一状态**。

| 操作 | 前置 | 结果 |
| --- | --- | --- |
| `close()` | open 且 owned | 调系统 close；无论成功失败都标记 `open=false`；失败返回 `Err(native)` |
| `close()` | 已关闭 | `Ok(())`，无操作（幂等，`defer` 重复关闭安全） |
| `close()` | 借用标准流 | `Err(NotOwned)`，状态不变 |
| `close_abort()` | open 且 owned | 调 close；失败写 stderr 诊断，不改退出码；随后 `open=false` |
| `close_abort()` | 已关闭/借用 | 无操作 |
| `read/write/flush` | 已关闭 | `Err(Closed)` |
| `read/write/flush` | open | 正常 |
| `release()` | open 且 owned | 返回 id，`self.open=false`（本句柄不再关闭），流保持打开交给接收者 |
| `release()` | 已关闭/借用 | 返回 0，无操作 |
| `from_raw(id)` | id 为 open 的非标准流 | 构造负责关闭的句柄；非法 id 返回 `Stream{handle:0}`，后续操作 `Err(Closed)` |

**重绑定规则（FS-06）**：`var s = open(a); defer s.close_abort();` 之后若要重绑定
`s = open(b)`，必须先 `s.close()` 或 `let id = s.release()`（显式转交）。若直接重绑定，
旧句柄状态失去唯一引用且永不关闭——Debug 运行时在退出收尾时把未关闭的自有流
（借用标准流除外）作为 `open handle(s)` 报告到 stderr（退出码不变），使其可被测试观测。
这是 API 契约，不是新增隐式 move 或自动析构。

### 6.3 清理失败报告

- `close_abort` 失败：向 stderr 写固定前缀 `Dolphin cleanup error: close failed` 与
  native code，然后继续；程序退出码保持 `main` 的返回值（不覆盖原错误）。
- 显式 `close()` 的失败由调用方决定如何报告与映射退出码。
- 现有 Debug 泄漏报告行为不变（stderr、退出码不变）。

### 6.4 拒绝方案

| 方案 | 拒绝理由 |
| --- | --- |
| 关闭失败时 trap/exit 105 | 覆盖原错误与退出码，违反“不覆盖原错误” |
| 关闭失败后保持 `open=true` 供重试 | 多数平台关闭失败后句柄状态不可信，重试可能二次关闭或泄漏 |
| 值语义 `bool closed` 副本各一份 | 会产生双重关闭权，违反“复制不产生第二份关闭权” |
| 把 `defer` 改成注册时捕获值 | 计划明确禁止；且改变已冻结的 M14 语义 |
| 引入自动析构/RAII | 超出 M19 非目标，破坏手动内存契约 |

## 7. 决策 5：错误类型

### 7.1 推荐方案

新增 `std.error`（`error.do`）：

```dolphin
pkg std.error;

pub enum ErrorKind {
    Other,             // 0
    NotFound,          // 1
    PermissionDenied,  // 2
    IsADirectory,      // 3
    InvalidArgument,   // 4
    NotOwned,          // 5
    Closed,            // 6
}

pub struct Error { kind: ErrorKind, code: i32 }  // 字段私有

impl Error {
    pub fn new(kind: ErrorKind, code: i32): Error
    pub fn kind(self: *const Self): ErrorKind
    pub fn code(self: *const Self): i32
}
```

- `ErrorKind` 的判别值 `0..6` 是运行时的稳定 ABI；`std.io`/`std.fs`/`std.process`
  在错误路径上读取运行时 kind 编号，用 if 链映射为枚举（语言不支持整数 match）。
- `code` 保留平台原生错误码（Unix `errno`，Windows `GetLastError`），同一 kind 下
  可用于排查；不承诺跨平台数值一致。`NotOwned`/`Closed`/路径 NUL 等纯 API 错误
  `code == 0`。
- 不提供 `Display`/格式化；应用自己决定输出文本（目标工具用固定前缀 + 已实现的
  `println("{}", ...)` 打印 code）。

### 7.2 映射表（冻结类别，不逐条冻结 errno）

| 场景 | kind |
| --- | --- |
| `ENOENT` / `ERROR_FILE_NOT_FOUND` / `ERROR_PATH_NOT_FOUND` | `NotFound` |
| `EACCES` / `EPERM` / `ERROR_ACCESS_DENIED` | `PermissionDenied` |
| `EISDIR` | `IsADirectory` |
| `EINVAL` / `ENAMETOOLONG` / Windows `ERROR_INVALID_NAME` | `InvalidArgument` |
| 关闭借用标准流 | `NotOwned` |
| 已关闭句柄上的读写 | `Closed` |
| 其余（含 `EMFILE`/`ENOSPC` 等） | `Other` + native code |

### 7.3 拒绝方案

| 方案 | 拒绝理由 |
| --- | --- |
| 字符串错误消息内建 | 依赖未实现的格式化/拥有型字符串和分配；且不稳定 |
| 每平台一套错误枚举 | 目标程序要写三份分支 |
| 只用 native code，无稳定类别 | 应用与测试无法跨平台断言 |
| `Error` 携带拥有型消息、需要 deinit | 错误路径资源管理复杂化，M19 不需要 |

### 7.4 验收例

- FS-02：用 `match error.kind()` 断言 `ErrorKind.NotFound` / `ErrorKind.IsADirectory`
  （枚举比较用 `match`，不依赖未承诺的 `==`）。
- 权限失败：断言 `PermissionDenied` 且 `error.code() != 0`（保留 native code）。
- 纯 API 错误（`NotOwned`/`Closed`/路径 NUL）：断言对应 kind 且 `error.code() == 0`。

## 8. 决策 6：本地库构建（需用户确认，D1）

### 8.1 现状（事实）

- `dc build --lib` **同时产出验证目标文件与 `.dlib`**，是已文档化行为。
- `.dlib` 打包拒绝 path 依赖（`dc package`/`publish` 同样限制）。
- lib+bin 的应用通过 `dc build`/`run` 消费 path 依赖库是可行的（H18-08 起一致）。

### 8.2 推荐方案（D1）

**M19 不改变 `dc build --lib` 的公开行为，也不放宽 `package`/`publish` 对 path 依赖的限制。**
开发/测试闭环改由新的、纯增量的 `dc test` 提供：

- `dc test <项目>` 编译“库源码 + tests 目录 + 生成入口”为测试二进制，**不产出 `.dlib`、不要求
  可发布性**，因此可以正常解析 path 依赖并复用模块/包可见性。
- 目标工具 `dtext` 是 bin 应用 + path 依赖库，`dc build/run` 已足够；库自身用 `dc test` 开发。
- 文档同步：`build --lib` 仍描述为“构建库并打包”；在 installation/implemented-features 中新增
  `dc test` 的说明，并明确“开发测试不经过打包”。

### 8.3 拒绝方案

| 方案 | 拒绝理由 |
| --- | --- |
| 让 `build --lib` 默认不再打包 | 破坏已文档化行为与脚本/CI 兼容，须用户另行批准 |
| 允许 `.dlib` 携带 path 依赖 | 破坏可发布归档的身份与可重现性，计划明确禁止 |
| 只做内部 Rust 测试入口、不提供 CLI | 用户测试目标无法达成；H19-05 验收要求 `dc test` |

### 8.4 决策请求

> **D1**：确认“`build --lib` 行为不变，M19 以新增 `dc test` 提供库开发闭环；不放宽 path 依赖
> 打包限制”。若要求改变 `build --lib` 默认打包行为，请单独批准并给出期望的默认输出与迁移说明。

### 8.5 验收例

- H19-05a TEST-05：lib-only 与含 path 依赖的 lib 包在**不产出 `.dlib`** 的情况下由 `dc test`
  编译并运行测试。
- 回归：`dc build --lib` 仍产出验证目标文件与 `.dlib`（H18-11 既有示例/测试继续通过），
  `package`/`publish` 对 path 依赖仍拒绝。

## 9. 决策 7：用户测试（需用户确认，D2）

### 9.1 推荐方案

**发现规则（无 `[[test]]` 清单变更）**：

- 测试文件位置：包根 `tests/` 的**直接子文件** `*.do`（不递归）。空/不存在则视为 0 测试。
- 测试文件**不得声明 `pkg`**，按“包根模块文件”编译（与 `src/*.do` 同模块），因此可以访问
  根模块私有项与子模块 `pub` 项；子模块私有项不可见（不把私有项改 `pub`）。
- 入口/签名：函数名以 `test_` 开头、无参数、无类型参数、返回 `Unit`。测试文件中的
  `main` 被拒绝，避免与生成入口冲突。
- 其他非 `test_` 函数在 `tests/*.do` 中允许作为 helper；测试之间不保证共享状态隔离，
  但每个测试在独立进程中执行（见下）。
- 生成入口：`dc test` 生成临时包根模块（不写入用户 `src/`），对每个测试声明调用；
  该入口只由 `dc test` 编译，`dc build`/`run`/`package` 永不读取 `tests/`。

**断言 API**：新增 `std.test`（`test.do`）：

```dolphin
pkg std.test;
pub fn expect(condition: bool)
pub fn fail()
```

- `expect(false)` / `fail()` 写 stderr 固定文本 `Dolphin test assertion failed` 并以 `106` 退出。
- 不提供带消息的断言（无格式化依赖）；需要上下文时先 `println`。
- `106` 是测试专用退出码，`101`–`104` 的运行时错误语义不变。

**执行与汇总**：

- `dc test [编译选项] [--filter <子串>] [项目目录]`；`--release`/`--debug`、`--backend`、
  `--system-linker`、`--locked`/`--offline` 与既有编译选项一致；默认 Debug。
- 一个测试二进制 `target/test/<包名>-tests[.exe]`（内部入口参数 `--dolphin-test <名称>`，
  不对外承诺）。每个测试由 `dc test` 在独立子进程中运行，30 秒固定超时；超时 kill 并回收，
  继续后续测试。
- 确定性顺序：按测试全限定名排序。输出固定格式：

  ```text
  test <name> ... ok
  test <name> ... FAILED (assertion)
  test <name> ... FAILED (trap exit 101)
  test <name> ... FAILED (timeout after 30s)
  N passed; M failed; K filtered out
  ```

- 退出码：全部通过 0；任一失败 1；编译失败 1（附编译器诊断）；用法错误 2。
- 0 测试：`no tests found`，退出 1；`--filter` 无匹配：`no tests matched filter`，退出 1。
- 子进程 stdout/stderr 直接继承（测试输出可见）；runner 不吞输出。
- 编译器回归仍由 Rust 测试负责；`dc test` 只服务用户项目，不替代 `cargo test`。

### 9.2 H19-05 拆分（按计划要求）

- `H19-05a`：库开发/测试目标入口（`dc test` 命令与 `target/test/` 产物、无打包路径）。
  **已实现（2026-09-22）**：`dc test <项目>` 需要 `[lib]` 目标，编译“库源码 + 生成的根模块
  占位入口”到 `target/test/<包名>-tests[.exe]`（同时写 `<包名>-tests.entry.do` 与 `.o`），
  不产出 `.dlib`、不要求可发布性，path 依赖可解析；生成入口与 `src/*.do` 同属根模块，
  可访问根模块私有项。本批不读取 `tests/`、不执行子进程，因此一律按冻结的 0 测试规则
  输出 `no tests found` 并以 1 退出（发现与执行见 b/c）。
- `H19-05b`：发现与 harness 生成（`tests/`、`test_*`、生成入口、可见性）。
  **已实现（2026-09-22）**：发现包根 `tests/` 直接子文件（不递归）的 `test_*`；冻结校验
  （测试文件不得声明 `pkg`/定义 `main`；`test_*` 无参数、无类型参数、返回 Unit、非 extern；
  同名测试在发现阶段报错）；按全限定名排序生成入口，入口按内部参数 `--dolphin-test <名称>`
  分发（未知名称 3、参数缺失/错误 2，不对外承诺）；全部 `tests/*.do`（含只定义 helper 的文件）
  与生成入口一起作为根模块源码注入，可访问根模块私有项与子模块 `pub` 项。新增 `std.test`
  （`expect`/`fail` + 运行时 `dolphin_test_fail`，失败写 `Dolphin test assertion failed` 并退出
  106）。**子进程执行/汇总未实现**：`dc test` 发现 0 个测试时输出 `no tests found`，发现 N>0 时
  输出临时信息 `built N tests (execution lands in H19-05c)`，两者都以 1 退出（不伪报通过）。
- `H19-05c`：子进程执行与汇总（超时、退出码分类、过滤、汇总）。
  **已实现（2026-09-22）**：每个测试用 `<二进制> --dolphin-test <名称>` 在独立子进程中运行，
  子进程 stdout/stderr 直接继承；固定 30 秒超时，超时 kill 并回收后继续后续测试。退出码分类为
  0→`ok`、106→`FAILED (assertion)`、其余→`FAILED (trap exit N)`（无正常退出码统一按 1）；
  固定输出 `test <name> ...` 行与 `N passed; M failed; K filtered out`。`--filter <子串>` 按名称
  子串选择子集（在完整 harness 上运行，K 为未选中数）。0 测试 `no tests found`、过滤无匹配
  `no tests matched filter`，均以 1 退出；全部通过 0、任一失败 1、编译失败 1、用法错误 2。
  H19-05 三个子批次完成。
每个子批次独立正反例，全部通过才关闭 H19-05。

### 9.3 拒绝方案

| 方案 | 拒绝理由 |
| --- | --- |
| `[[test]]` 清单段 | 需要清单格式扩展与模板/路径配置，M19 用不上；自动发现已足够 |
| `#[test]` 属性/宏 | 语言无属性与宏系统 |
| 同进程顺序跑测试 | trap/`_Exit` 会杀掉整个 runner，无法继续 |
| 把私有项全改 `pub` 以便测试 | 改变库的公开面，计划明确禁止 |
| `--filter` 无匹配返回 0 | 会让拼写错误静默“通过” |

### 9.4 决策请求

> **D2**：确认测试发现约定（`tests/*.do` + `test_*` 自动发现，不新增 `[[test]]` 清单字段）、
> `target/test/` 输出路径、断言失败退出码 `106`、0 测试/无匹配退出 1。

### 9.5 验收例

- TEST-01：全部通过时汇总固定为 `N passed; 0 failed; 0 filtered out`，exit 0。
- TEST-02：至少一个断言失败时输出 `FAILED (assertion)`，exit 1，其余测试仍执行。
- TEST-03：`--filter` 选中子集且顺序确定；0 测试或无匹配均为 exit 1 并给出固定消息。
- TEST-04：死循环测试 30 秒被 kill 并回收，后续测试继续，汇总标记 `timeout after 30s`。
- TEST-05：lib-only、lib+bin、path 依赖三种项目均可用；TEST-06：测试确实实例化 stdlib 公开泛型
  （如 `Vec<i32>`/`Option<T>`），而不是只做语法检查。
- 可见性：测试能访问根模块私有项；不通过把所有私有项改 `pub` 实现。

## 10. 决策 8：错误处理语法（需用户确认，D3）

### 10.1 推荐方案

**M19 不新增任何语法**（不加 match 语句块、不加 `?`、不加 if/match 表达式、不加 block
expression）。目标程序用 `Result` + `match` 表达式 + helper 函数 + `defer` 完成：

- 可失败步骤封装为返回 `Result<T, Error>` 的 helper；循环内用 `match` 把结果存入
  `var`，`if result.is_err()` 时 `break` 并在循环外统一处理。
- 早返回路径的资源由 `defer` 清理（M14 语义）；错误对象是值类型（枚举+int），无悬垂。
- `main` 用 `return match run() { Result.Ok(code) => code, Result.Err(e) => report(e) }`
  映射退出码。

### 10.2 若 H19-06 证明组合写法阻塞目标程序

H19-06 必须先写完所有失败路径，若确有阻塞，**停止并把最小语法提案交回用户重新冻结**，
提案至少包含：branch scope、`return`/`break`/`continue` 行为、defer 清理点、分支结果类型、
发散分支处理、AST/lower/IR 改动面与两后端测试。不得自行实现，也不得顺带实现一般
block/if 表达式、`?` 或异常。

### 10.3 拒绝方案

| 方案 | 拒绝理由 |
| --- | --- |
| 直接实现 match 语句块 | 语法/AST/lower/IR 面大，M19 目标程序未证明必需 |
| 实现 `?` 传播 | 需要新的发散语义与错误转换，超出范围 |
| 异常/`try` 资源语法 | 与手动内存/defer 契约冲突，M14 已明确拒绝 |

### 10.4 决策请求

> **D3**：确认“M19 不新增语法；若确需，H19-06 回到用户重新冻结”。同时确认继续保留
> M14 defer 语义（退出时读取最新值），并用句柄状态机+显式 `close`/`release` 处理重绑定。

### 10.5 验收例

- ERR-01..04 全部用既有 `Result`/`match`/辅助函数/`defer` 实现并断言固定 stdout/stderr/exit。
- 若任一验收在未加语法时无法通过，H19-06 必须按 §10.2 停止并提交最小语法提案，不得自行实现；
  该情形下 H19-06 状态为阻塞。

## 11. 目标程序 `dtext`（H19-07 冻结）

- 位置与拆分：`examples/m19/textstats`（`[lib]`，统计与过滤核心）与 `examples/m19/dtext`
  （`[[bin]]`，path 依赖 `textstats`，参数/IO/退出码/帮助）。两包各自有 `tests/*.do`。
- 用法（应用参数，经 `dc run ... --` 或直接运行）：

  ```text
  dtext [--help] [--filter <text>] [<path>]
  ```

  - 无 `path` 或 `path == "-"`：读 stdin（不关闭标准输入）。
  - `--help`：打印用法，退出 0。
  - 未知选项、缺少 `--filter` 参数、多余位置参数：stderr 诊断 + 退出 2。
- 统计与过滤规则（精确冻结）：
  - 输入必须是合法 UTF-8；遇到非法字节序列 → stderr `dtext: invalid UTF-8` + 退出 1。
    字节读取允许，转成文本前必须校验（IO-04）。
  - 行定义：以 `\n` 分隔；行内容为两个 `\n` 之间的字节，并去掉紧邻 `\n` 前的一个 `\r`
    （CRLF）；孤立 `\r` 保留。最后一行无 `\n` 时仍是独立一行（非空残留）。
  - `lines`：行数。空输入 0；`"a"` 为 1；`"a\n"` 为 1；`"a\n\n"` 为 2。
  - `--filter <text>`：保留“行内容包含该 UTF-8 字节子串”的行（合法 UTF-8 下等价于码点
    子串）；无 `--filter` 时所有行都算匹配；空子串匹配所有行。
  - `matched`：匹配行数。
  - `bytes`：从输入读取的总字节数。
  - 输出固定为三行（顺序固定，无本地化、无额外空格）：

    ```text
    lines=<n>
    matched=<n>
    bytes=<n>
    ```

- 退出码：成功 0；I/O 或编码错误 1；用法错误 2。
- 资源规则：文件句柄在成功与错误路径都关闭；stdin/stdout/stderr 不关闭；重复运行不累积
  资源（Debug 无泄漏报告、无 open-handle 报告）。
- 跨平台：三平台默认后端（Cranelift）与 Linux LLVM 均验证；路径含空格与 Unicode 的用例
  覆盖三平台。

实现状态（H19-07）：`examples/m19/textstats` 与 `examples/m19/dtext` 已实现；用法、
行规则、诊断、退出码与资源规则按本节冻结值落地（诊断文本见 [M19 报告](reports/m19-progress.md)
H19-07 节）。`dtext` 声明 `[lib]`（应用逻辑 `src/app.do`）+ `[[bin]]`（入口 `src/main.do`），
以满足 `dc test` 的 `[lib]` 要求；D1 冻结了 `dc build --lib` 的打包行为，声明 path 依赖的
lib+bin 包因此用 `dc build --bin dtext` / `dc run --bin dtext` 构建运行（plain `dc build`
仍按 D1 在打包库时拒绝 path 依赖）。两包各自有 `tests/*.do`，由 `dc test` 运行。

### 11.1 dtext 依赖的文本/数值 API（H19-04 实现，规格补充）

H19-04 按上述目标程序补齐 `std.text` 的最小文本/数值能力；签名与拥有权在此冻结：

```dolphin
pub enum NumberError { Empty, InvalidDigit, Overflow }

pub struct Lines { /* 私有字段 */ }
impl Lines { pub fn init(storage: []const u8): Lines }
impl Iterator for Lines { type Item = []const u8; }
pub fn lines(bytes: []const u8): Lines

pub struct Builder { /* 私有字段 */ }
impl Builder {
    pub fn init(): Builder
    pub fn with_capacity(capacity: usize): Builder
    pub fn len(self: *const Self): usize
    pub fn is_empty(self: *const Self): bool
    pub fn append(self: *Self, s: string)
    pub fn append_bytes(self: *Self, bytes: []const u8)
    pub fn view(self: *const Self): []const u8
    pub fn consume(self: *Self, count: usize)
    pub fn clear(self: *Self)
    pub fn deinit(self: *Self)
}

pub fn parse_i64(s: string): Result<i64, NumberError>
pub fn parse_u64(s: string): Result<u64, NumberError>
```

- `lines` 的规则与第 11 节 `dtext` 行定义逐字一致（`\n` 分隔、去掉紧邻 `\n` 前一个 `\r`、
  孤立 `\r` 保留、无 `\n` 的非空尾段算一行、空输入 0 行）；产出**借用视图**、零分配、
  不校验 UTF-8（调用方需要文本时用 `from_utf8`）。
- `Builder` 是**拥有型**缓冲，`view()` 返回**借用视图**，在下一次
  `append`/`append_bytes`/`consume`/`clear`/`deinit` 之后失效（扩容替换底层存储）；
  构造中允许暂不完整的 UTF-8，因此 `view()` 是原始字节。长度和与扩容倍增溢出走 102
  分配失败通道，不做算术 trap。
- `parse_i64` 接受一个可选 `-`/`+`，`parse_u64` 只接受可选 `+`；随后必须至少有一位 ASCII
  数字，无空白/下划线/进制前缀。溢出在乘加前检查并返回 `NumberError.Overflow`，不触发 101。
- 测试矩阵 TEXT-01..04 见第 14 节；实现证据见 [M19 报告](reports/m19-progress.md) H19-04 节。

## 12. 需用户确认的最小决策请求

| 编号 | 请求 | 推荐 | 确认结果 |
| --- | --- | --- | --- |
| D1 | `dc build --lib` 是否保持“产出目标文件 + 打包 `.dlib`”不变，M19 仅新增 `dc test` 作为开发/测试闭环；不放宽 `.dlib` 的 path 依赖限制 | 保持不变（第 8 节） | 2026-09-22 用户同意 |
| D2 | 测试发现与运行约定：`tests/*.do` + `test_*` 自动发现（不新增 `[[test]]` 清单字段）、`target/test/` 输出、断言失败退出码 `106`、0 测试/过滤无匹配退出 1 | 按第 9 节冻结 | 2026-09-22 用户同意 |
| D3 | M19 不新增语法（含 match 语句块）；保留 M14 defer 语义，用句柄状态机与显式 `close`/`release` 处理重绑定 | 按第 10 节冻结 | 2026-09-22 用户同意 |

D1–D3 已确认；H19-01..H19-04 已实现（见 [M19 报告](reports/m19-progress.md)），
H19-05 及之后批次等待人工派发后实施。未派发前不开始编码。

## 13. 运行时平台封装（源码入口与 ABI）

同一份 C 运行时新增函数（`runtime/unix_runtime.c`、`runtime/windows_runtime.cpp`），
Dolphin 侧通过 `extern "C"` 声明；`std.error`/`std.io`/`std.fs`/`std.process` 的拥有权/
错误映射写在 Dolphin 源码库，不写进 Rust 编译器：

| 函数（C 侧签名） | Dolphin extern 绑定要点 | 语义 |
| --- | --- | --- |
| `uintptr_t dolphin_stream_stdin/stdout/stderr(void)` | 返回 `usize` | 借用流状态 id（进程生命周期） |
| `int dolphin_stream_open(const uint8_t *path, uintptr_t len, int mode, uintptr_t *out)` | `path: *const u8`，`out: *usize` | 0 成功；非 0 失败并设置 last-error；mode 0/1/2 = Read/Write/Append |
| `int dolphin_stream_read(uintptr_t s, uint8_t *buf, uintptr_t len, uintptr_t *out_read)` | `buf: *u8`（取切片 `.ptr`） | 0 成功（`out_read` 可为 0 = EOF）；重试 `EINTR` |
| `int dolphin_stream_write(uintptr_t s, const uint8_t *bytes, uintptr_t len, uintptr_t *out_written)` | `bytes: *const u8` | 0 成功；短写由 `write_all` 处理；重试 `EINTR` |
| `int dolphin_stream_flush(uintptr_t s)` | | 0 成功 |
| `int dolphin_stream_close(uintptr_t s)` | | 0 成功；失败后状态仍标记关闭 |
| `uint8_t dolphin_stream_is_open(uintptr_t s)` | 返回 `u8`/`bool` | 1/0 |
| `usize dolphin_arg_count(void)` | 返回 `usize` | argv 条目数（含 argv[0]） |
| `void *dolphin_arg(uintptr_t i, uintptr_t *len)` | 返回 `*Unit`，`len: *usize` | 成功返回数据指针并写长度；NULL 时读 last-error（`NotFound`=越界、`InvalidArgument`=非 UTF-8） |
| `void *dolphin_env(const uint8_t *name, uintptr_t len, uintptr_t *out_len)` | `name: *const u8`，返回 `*Unit` | 非 NULL=找到；NULL 时 last-error 区分 `NotFound`/`InvalidArgument` |
| `int dolphin_last_error_kind(void)` / `int dolphin_last_error_code(void)` | 返回 `i32` | 稳定 kind 0..6 与 native code |
| `void dolphin_test_fail(void)` | | 写固定文本并以 106 退出 |

- ABI 表使用的类型均为既有 FFI 支持范围（`*Unit`、`*const u8`、`*u8`、`*usize`、`usize`、`i32`），
  不使用未验证的指针的指针；数据指针由 Dolphin 侧用 `mem.view<u8>(ptr, len)` 转成视图。
- Debug 运行时在 `dolphin_runtime_finish` 追加“未关闭自有流”报告（借用标准流除外），
  不改变退出码；Release 不追踪。
- 新增运行时函数必须同时实现 Unix 与 Windows；不得用 `#ifdef` 在 Dolphin 层分支。

## 14. 测试矩阵与批次完成标准

统一要求：每个验收点在**可用后端 × Dolphin Debug/Release**（涉及 runtime/I/O 的用
Debug 检查泄漏与句柄报告）上运行，断言固定 stdout/stderr/exit；错误断言错误类别与
native code 是否存在，不绑定整段渲染文本。新测试文件必须加入 CI 显式 `--test` 列表与
`docs/implementation` 命令说明。

| 批次 | 验收编号 → 测试文件/测试名（计划） | 完成标准 |
| --- | --- | --- |
| H19-01 | ARGS-01..04 → `tests/m19_args.rs`：`args_01_direct_and_dc_run_match`、`args_02_spaces_unicode_and_dash`、`args_03_missing_env_and_not_utf8`、`args_04_main_exit_unchanged` | `dc run --` 原样转发（含空参数/前导 `-`）、非 UTF-8/缺项区分、`main` 兼容 |
| H19-02 | IO-01..04 → `tests/m19_io.rs`：`io_01_redirect_stdio`、`io_02_empty_and_chunked_read`、`io_03_partial_write_fixture`、`io_04_invalid_utf8_bytes_not_string`；失败路径 `stdout` close 返回 `NotOwned` | 短读/短写/EOF 语义、标准流不可关闭、无泄漏 |
| H19-03 | FS-01..06 → `tests/m19_fs.rs`：`fs_01_empty_small_multi_chunk`、`fs_02_missing_dir_and_bad_path`、`fs_03_injected_read_write_close_failure`、`fs_04_space_and_unicode_path`、`fs_05_truncate_and_append`、`fs_06_state_machine_close_rebind_handoff` | 状态机幂等关闭、重绑定/转交不双关不漏关、Debug 无 open-handle 报告 |
| H19-04 | TEXT-01..04 → `tests/m19_text.rs`：`text_01_lines_crlf_no_final_newline`、`text_02_utf8_boundaries`、`text_03_parse_signed_extremes_overflow`、`text_04_builder_grow_and_deinit` | 行/CRLF/边界冻结、解析失败返回 `Result` 不 trap、builder 失效规则 |
| H19-05a/b/c | TEST-01..06 → `tests/m19_test_cmd.rs`：`test_01_all_pass`、`test_02_failure_nonzero`、`test_03_filter_and_zero`、`test_04_timeout_kills_and_continues`、`test_05_lib_only_bin_and_path_dep`、`test_06_stdlib_generic_instantiation` | 发现/可见性/子进程隔离/超时/汇总冻结；三个子批次各自正反例 |
| H19-06 | ERR-01..04 → `tests/m19_errors.rs`：`err_01_early_return_cleans_resources`、`err_02_error_views_not_dangling`、`err_03_return_snapshot_and_defer_order`、`err_04_io_error_not_trap` | 不新增语法即可完成；确认需要新语法时按第 10.2 节回到用户 |
| H19-07 | `tests/m19_app.rs` + `examples/m19` 自测：空输入、正常 UTF-8、无末尾换行、无匹配、非法参数、缺失文件、受控读写失败、重复运行无累积 | 三平台默认后端 + Linux LLVM；stdout/stderr/exit 固定；发行包可构建并运行该示例 |

## 15. 兼容性影响

- **CLI**：`dc test` 为新增子命令（H19-05 已完成构建/发现/执行与汇总）；`dc run` 增加 `--` 之后
  的应用参数（增量语法，原有调用不变）。`dc build --lib` 行为不变（D1）。用法错误仍为退出码 2。
- **清单/持久格式**：`dolphin.toml` 不新增字段（D2 自动发现）；`dolphin.lock` 与 `.dlib`
  格式、`compiler-version` 精确匹配策略不变；`dc test` 不写锁以外的持久文件。
- **资源模型**：`defer` 语义不变；新增的只是 `std.io`/`std.fs` 句柄状态机契约与
  Debug open-handle 报告。既有 `mem.alloc/free`、`Vec`/`String`/`CString` 行为不变；
  `std.text` 的 `Lines`/`Builder`/`parse_i64`/`parse_u64` 为纯增量（第 11.1 节）。
- **退出码**：`101`–`104` 不变；新增 `106` 仅由 `std.test` 断言失败使用。清理失败不新增退出码。
- **保留命名**：`std.process`/`std.io`/`std.fs`/`std.error`/`std.test` 成为保留的 std 模块名；
  用户模块本就不能占用 `std` 命名空间，无额外破坏。
- **未实现标注**：本文所有 API 在对应批次完成前均不得写入“已实现功能”或示例。

## 16. 未决与阻塞

- D1、D2、D3 已于 2026-09-22 由用户确认；H19-01、H19-02、H19-03、H19-04 已完成对应实现
  （见 [M19 报告](reports/m19-progress.md)）。H19-05a/b/c 已完成（构建、发现/harness/`std.test`、
  子进程执行/超时/分类/过滤/汇总），TEST-01..06 均有真实测试。H19-06 以组合回归完成：
  ERR-01..04（`tests/m19_errors.rs`）证明既有 `Result`/`match`/helper/`defer` 足以写完失败路径，
  **未新增任何语法**；两处组合限制（字段后直接方法调用、`return match` 的 Err 构造臂推断）
  用局部绑定绕过，详见 [M19 报告](reports/m19-progress.md) H19-06 节。H19-07 已完成：
  `examples/m19/textstats`（lib）与 `examples/m19/dtext`（lib+bin，path 依赖）落地本节冻结的
  用法/行规则/诊断/退出码/资源规则；Linux/Windows/macOS 默认后端与 Linux LLVM 通过，远端 CI 通过。
  M19 阶段完成，下一阶段 M20 先做 H20-00 设计冻结。
- H19-02 经用户确认把 unit-like 返回值从 `Result<(), Error>` 改为 `Result<bool, Error>`（当前语言
  无 Unit 值；`Result<Unit, E>` 会触发诊断），并修复了暴露的两个编译器缺陷（Unit payload 诊断、
  LLVM 重复 extern 符号）。规格第 4.1 节已同步。
- H19-03 的受控关闭失败没有可移植注入方式（`close(2)` 对普通文件不报错）；用模式不匹配读写、
  Linux `/dev/full` 写入失败、幂等重复关闭与 invalid 句柄覆盖，未伪造真实关闭失败。
- `dc test` 支持 `--release`/`--debug`（默认 Debug），已在 §9.1 冻结；不作为阻塞项。
- 若 H19-06 证明必须新增语法，按第 10.2 节停止并重新冻结（不预先批准）。
