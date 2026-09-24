# M20 规格：项目级诊断与开发工具（H20-00 冻结）

> 状态：H20-00 设计冻结产物。**本文冻结的 API、格式与语义尚未实现**；H20-01..H20-05 完成前，
> 任何条目不得写入 [implemented-features](implemented-features.md)、README 或示例说明为“当前可用”。
> 实现顺序与批次验收编号见 [M18-M21 计划](plan-m18-plus.md) 第 6 节与本文第 12 节。
>
> 前置：M19 已完成并通过阶段验收（[m19-progress](reports/m19-progress.md) 末节），
> `examples/m19` 与 `dc test` 是 M20 的验收对象。
>
> 决策请求：D-M20-1..4 已于 2026-09-24 由用户确认（第 13 节）。H20-01 已解阻但**未派发**；
> 收到批次指令前不得开始编码。本批不修改任何源码、示例、清单或持久格式。

## 1. 范围与非目标

M20 交付项目级开发体验：结构化诊断、无构建/网络副作用的共享项目分析、未保存文件 overlay、
lib/多 bin/依赖项目的诊断与跨文件导航、格式化保持性与项目发现、以及一次真实调试器验收。
本文冻结实现这些目标所需的接口、身份、生命周期与协议。

M20 明确不做（不得以“顺手”方式引入）：

- 数据库式增量分析框架、query 系统、后台常驻分析进程；
- 一次拆开全部 lowering/类型检查（仅按第 5 节做收集式前端与 side table）；
- 补全（completion）、语义高亮、重命名、代码操作等新 LSP 能力；
- 表达式级类型推断 API；hover 只提供定义处声明的签名/类型，不提供任意表达式类型；
- 异步/线程 runtime、并发任务、取消令牌之外的调度框架；
- 新语言语法、宏、allocator、闭包、异常；
- 构建缓存、性能优化（属 M21）；
- Cranelift 调试信息、Windows/PDB 调试、局部变量值检查；
- `dolphin.toml`/`dolphin.lock`/`.dlib` 任何字段或格式变化。

## 2. 总则（所有新 API 共同遵守）

1. **只读分析**：项目分析不得访问 HTTP(S)、不得写 `dolphin.lock`、不得在项目内产出任何构建产物
   （对象、可执行、`.dlib`、生成入口）。允许读取/解包已有缓存归档到 `DOLPHIN_HOME`（见 §6.2）。
2. **错误不 panic**：词法/语法/清单/依赖/分析错误一律转成 `Diagnostic` 返回；任何索引/切片前
   先做边界检查。错误程序不得进入 codegen（§5.6）。
3. **快照不可变**：一次分析的结果以 `Arc<AnalysisSnapshot>` 共享；快照内数据全部拥有，
   不借用 `AnalysisHost`。旧快照可以继续被读，但不得被写回。
4. **身份稳定**：`SourceId`/`Span`/`DefId` 的身份规则见 §3；跨快照不承诺稳定，禁止持久化。
5. **不猜测名字**：hover/definition 只能使用分析产物中的 `Resolution`/`DefId`；找不到时返回
   `null`，不得回退为“同文件文本查找同名顶层声明”（当前 LSP 的旧行为在 H20-03 删除）。
6. **兼容优先**：`Diagnostic::plain`/`Diagnostic::at`、`lex`/`parse`、`load_packages`、
   `lower_sources`、`resolve_project` 的现有签名保持；新能力通过新函数/新字段/新 crate 增量提供。
7. **未实现标注**：本文所有 API 在对应批次落地前，代码中不得以 `pub` 形式占位并声称可用；
   文档按批次更新（每批四种证据见 [计划](plan-m18-plus.md) 第 10 节）。

## 3. 决策 A：SourceId/Span 身份与符号身份（冻结）

### 3.1 当前事实（H20-00 开始时）

- `Span { start: usize, end: usize }` 是 `SourceFile.text` 的半开字节区间；渲染时向下吸附到
  UTF-8 字符边界（`crates/dolphin-source/src/diagnostic.rs`）。
- `SourceFile` 只有 `path`/`text`/`line_starts`，没有文件身份；AST 顶层定义有 `source_id: usize`
  （`dolphin-syntax/src/ast.rs`），是 loader 分配的 `sources` 下标。
- `Diagnostic` 只保存 `rendered: String`；LSP 靠解析渲染文本中的 `--> path:line:col` 再换算位置
  （`crates/dolphin-lsp/src/lib.rs:407-443`）。

### 3.2 推荐方案

`SourceId` 成为诊断与导航的唯一文件身份，`Span` 语义不变。

```rust
// crates/dolphin-source/src/source.rs（新增/修改）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(pub u32);

impl SourceId {
    /// 未绑定到任何加载单元的源码（单文件工具、测试临时文件）。
    pub const ANONYMOUS: SourceId = SourceId(u32::MAX);
}

pub struct SourceFile {
    pub id: SourceId,          // 新增
    pub path: PathBuf,
    pub text: String,
    line_starts: Vec<usize>,   // 私有，保持
}

impl SourceFile {
    pub fn new(path: PathBuf, text: String) -> Self;                    // id = ANONYMOUS（现有签名不变）
    pub fn with_id(id: SourceId, path: PathBuf, text: String) -> Self;  // 新增
    pub fn line_column(&self, offset: usize) -> (usize, usize);         // 1-based 字符列，不变
    pub fn line_count(&self) -> usize;                                  // 不变
    pub fn line_start(&self, line: usize) -> usize;                     // 不变
    pub fn line_text(&self, line: usize) -> &str;                       // 不变
}

/// 只读文件表，供渲染与 LSP 位置换算。
pub struct SourceMap<'a> { files: &'a [SourceFile] }

impl<'a> SourceMap<'a> {
    pub fn new(files: &'a [SourceFile]) -> Self;
    pub fn file(&self, id: SourceId) -> Option<&'a SourceFile>;
    pub fn path(&self, id: SourceId) -> Option<&'a Path>;
    pub fn position(&self, id: SourceId, offset: usize) -> Option<(usize, usize)>; // 1-based
    pub fn offset(&self, id: SourceId, line: usize, column: usize) -> Option<usize>; // 1-based 字符列
}
```

冻结规则：

1. **Span 身份**：`Span` 永远是对同一 `SourceFile.text` 的字节半开区间；诊断主 span 必须落在
   字符边界上（构造时吸附，渲染时再吸附一次；两者都不 panic）。
2. **SourceId 身份**：在一个加载单元（`LoadedSources`/`AnalysisUnit`）内，`SourceId.0` 等于该
   文件在该单元 `sources` 向量中的下标，由 loader 按确定性顺序分配（§5.3）。`ANONYMOUS`
   只用于未进入任何加载单元的 `SourceFile`。
3. **跨快照不稳定**：`SourceId` 不持久化、不跨快照比较；诊断跨文件引用一律使用
   `(SourceId, Span)`，渲染/协议转换时必须同时持有对应 `SourceMap`。
4. **符号身份**：定义用 `DefId { package: PackageId, qualified: String }`；`qualified` 是
   loader 限定后的全名（已含 `std.`/`@<id>.` 包前缀），在同一快照内唯一。泛型实例用
   `SymbolId::Instance { def, type_args }`（等价于现有 `GenericKey`，但显式携带定义身份）。
5. **定义位置**：`Definition { id, kind, source: SourceId, name_span: Span, signature: String }`；
   `name_span` 是该定义名字 token 的 span。局部变量用
   `SymbolId::Local { source, name_span }`，只在本文件有效。

### 3.3 拒绝方案

- 把 `SourceId` 做成全局原子计数器：快照之间不可比且测试不稳定；拒绝。
- 用文件路径字符串当身份：大小写/分隔符/符号链接在不同平台不等价；拒绝。
- 把 `Span` 改成行列或同时存两套：与现有 479 处 `Diagnostic::at` 调用和 AST 冲突；拒绝。
- 持久化 `SourceId` 到 `.dlib`/锁文件：跨构建无意义且扩大兼容面；拒绝。

### 3.4 正反例

- 正例：`examples/m19/dtext` 中 `src/app.do` 的函数在 LSP hover 返回
  `fn textstats...` 形式的显示名（§6.6）与正确 `name_span`；跨文件 definition 指向
  `textstats/src/lib.do`。
- 正例：同一文件在 lib 单元与 bin 单元中各有一个 `SourceId`；诊断去重按路径而非 `SourceId`
  （§6.5），因此不会重复发布。
- 反例：把 `SourceId::ANONYMOUS` 的 `Diagnostic` 交给需要 `SourceMap` 的 LSP 转换 → 必须
  返回无位置诊断（`(0,0)-(0,0)`），不得 panic、不得猜文件。

## 4. 决策 B：结构化诊断数据（冻结）

### 4.1 推荐方案

`Diagnostic` 保留现有构造函数与 `Display` 文本，新增结构化字段与访问器。全部 479 处
`Diagnostic::plain`/`Diagnostic::at` 调用点**无需修改**。

```rust
// crates/dolphin-source/src/diagnostic.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity { Error, Warning, Note }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub source: SourceId,
    pub span: Span,
    pub message: String,       // 可为空
}

pub struct Diagnostic { /* 私有：code, severity, message, labels, notes, rendered */ }

impl Diagnostic {
    // 现有签名与语义不变：
    pub fn plain(message: impl Into<String>) -> Self;                        // code = "E0000"，无 label
    pub fn at(source: &SourceFile, span: Span, message: impl AsRef<str>) -> Self; // code = "E0001"

    // 新增构造/修饰：
    pub fn error(code: &str, message: impl Into<String>) -> Self;            // 无位置错误
    pub fn with_label(self, source: &SourceFile, span: Span,
                      message: impl Into<String>) -> Self;                   // 追加 secondary label
    pub fn with_note(self, note: impl Into<String>) -> Self;
    pub fn with_code(self, code: &str) -> Self;

    // 新增访问器：
    pub fn code(&self) -> &str;
    pub fn severity(&self) -> Severity;
    pub fn message(&self) -> &str;
    pub fn labels(&self) -> &[Label];   // labels[0] 是 primary（若有）
    pub fn notes(&self) -> &[String];
}
```

冻结规则：

1. **code 命名空间**：`E0000` = `plain`（保持）；`E0001` = `at`（保持）；M20 新分类使用
   `E1xxx`（项目/清单/依赖）与 `E2xxx`（分析/协议），具体登记见 §4.4。同一 code 的含义
   一旦发布不得改变；新增测试断言 `code`，旧测试继续断言 `Display` 文本，两者都保留。
2. **severity**：M20 只产生 `Severity::Error`；`Warning`/`Note` 变体保留但不得在 CLI 默认
   输出中新增，避免改变 `dc check` 的成功判定。引入任何 warning 需另开决策。
3. **labels 顺序**：`labels[0]` 是 primary；其余为 related locations。`at` 产生的
   `(source.id, span)` 即 primary。
4. **Display 兼容**：`plain` 与 `at` 的输出必须与当前逐字节一致（含
   `error[E0001]: ...\n --> path:line:col\n  |\n...` 格式与 `Diagnostic::at` 的 Windows
   正斜杠路径规则）。secondary label 与 note 追加为独立行：
   ```text
     = note: <message> (<path>:<line>:<column>)
     = note: <path>:<line>:<column>          // message 为空
     = note: <note 文本>
   ```
5. **拥有权**：`Diagnostic` 完全拥有自身数据（`String`/`Vec`），可跨线程移动、可存入快照；
   `Label` 只存 `SourceId`/`Span`/消息，不借用 `SourceFile`。
6. **LSP 映射**（H20-01 实现，H20-03 使用）：
   - `severity` → LSP 1/2/3；`code` → 字符串 code；`message` → **纯消息文本**（不再发渲染块）；
   - primary label → `range`（UTF-16）；无 label → `(0,0)-(0,0)`；
   - `labels[1..]` → `relatedInformation[{location:{uri,range},message}]`；
   - `source` 固定 `"dolphin"`。

### 4.2 拒绝方案

- 保留 `rendered` 让 LSP 继续解析文本：正是 H20-00 要修的基础缺陷；拒绝。
- 让 `Diagnostic` 持有 `&SourceFile`/路径+行列快照：路径快照会随 overlay 失效，且增大
  clone 成本；只存 `SourceId`+`Span`，渲染时用 `SourceMap`；拒绝。
- 给全部 479 个调用点逐个补 code：超出 H20-01 范围；`plain`/`at` 的默认 code 足够区分
  身份，新分类按需增补；拒绝。

### 4.3 正反例

- 正例：`diag_01_cli_and_lsp_same_error_identity` 对同一 fixture 断言 CLI 的
  `error[E0001]` 与 LSP 的 `code == "E0001"`、UTF-16 range 一致。
- 正例：`diag_04_generic_chain_and_user_type_names` 的错误消息含
  `Vec<Result<i32, myerr.MyError>>` 形式的显示名，而不是 `TypeId(3)`。
- 反例：把 `plain("...")` 的 `message()` 返回成 `"error[E0000]: ..."`（含前缀）：LSP 会重复
  渲染；`message()` 必须只返回构造时传入的文本。

### 4.4 code 登记表（M20 冻结；新增需在本表登记）

| code | 含义 | 产生点 |
| --- | --- | --- |
| `E0000` | 无位置的通用错误 | `Diagnostic::plain`（全部现有调用点） |
| `E0001` | 有位置的词法/语法/语义错误 | `Diagnostic::at`（全部现有调用点） |
| `E0002` | 诊断数量达到上限 | `lex_recovering`/`parse_recovering`/声明级收集 |
| `E1001` | 依赖不可本地恢复 | `resolve_readonly` |
| `E1002` | 项目清单/源码根加载失败 | 分析路径的 manifest/发现错误 |
| `E2001` | 分析内部不可恢复错误 | `dolphin-analysis` 兜底转换 |

M20 不改变 `E0000`/`E0001` 的含义；其余 code 在对应批次首次落地时才有意义。

## 5. 决策 C：错误恢复与多错误收集（冻结）

### 5.1 冻结范围

H20-01 收集三类错误，均可跨文件、可同一文件多处：

1. **读取 + 词法 + 语法 + 包路径校验**（§5.2、§5.3）；
2. **互不依赖声明的语义校验**：`build_templates` 覆盖的声明级检查（类型/函数/trait 重名、
   保留名、`main` 形状、类型参数重复、impl 头、trait impl 缺失成员等，§5.4）；
3. **函数体 lowering**：保持“首个错误即停”，每个目标至多一条。

任一阶段有错误即 `partial=true`、不进入后续阶段、不产生 codegen 产物。
“拆开函数体 lowering 收集表达式级多错误”不在 M20 范围。

### 5.2 词法与语法恢复

```rust
// crates/dolphin-source/src/lexer.rs
pub fn lex(source: &SourceFile) -> Result<Vec<Token>, Diagnostic>;           // 不变
pub fn lex_recovering(source: &SourceFile) -> (Vec<Token>, Vec<Diagnostic>); // 新增

// crates/dolphin-syntax/src/parser.rs
pub fn parse(source: &SourceFile, tokens: Vec<Token>) -> Result<ast::Program, Diagnostic>; // 不变
pub fn parse_recovering(source: &SourceFile, tokens: Vec<Token>)
    -> (ast::Program, Vec<Diagnostic>);                                      // 新增
```

冻结的恢复规则：

1. **词法**：未知字符 → 在字符 span 报 `E0001`，跳过该字符继续；未闭合字符串/字符 →
   在起始引号报错，跳到行尾继续；未闭合块注释 → 在 `/*` 报错，跳到 EOF 结束。
   返回值总是以 `Eof` 结尾，可安全交给 parser。
2. **语法同步点**：块内跳过到 `;`（消费）、`}`（不消费，交回块解析）或同层语句起始关键字
   （`if`/`while`/`loop`/`for`/`return`/`break`/`continue`/`defer`/`val`/`var`）；顶层跳过到
   `fn`/`struct`/`enum`/`trait`/`impl`/`use`/`pkg`/EOF。顶层多余的 `}` 报错并跳过。
3. **上限**：每个文件最多收集 100 条诊断；超出后追加一条
   `E0002: too many errors; further diagnostics suppressed` 并停止。
4. **安全**：`parse_recovering` 对任意 `lex_recovering` 输出不得 panic；得到的 AST 允许含
   空名/零 span 的残缺节点，但**只用于诊断与文档符号**，不得传给 `lower*`。
5. `lex`/`parse` 的现有“首错返回”行为不变；driver 单文件路径（`compile_frontend`）在
   H20-01 改用 recovering 版本以收集全部错误。

### 5.3 收集式 loader

```rust
// crates/dolphin-hir/src/modules.rs
pub struct LoadedSources {
    pub sources: Vec<SourceFile>,               // 已分配 SourceId
    pub asts: Vec<Option<ast::Program>>,        // 与 sources 对齐；parse 失败为 None
    pub packages: Vec<PackageId>,               // 与 sources 对齐
    pub diagnostics: Vec<Diagnostic>,           // 读取/词法/语法/pkg 校验错误
    pub program: Option<ast::Program>,          // 全部成功且模块解析成功后的合并 AST
}

pub trait SourceProvider {
    /// 递归发现源码根下的 `.do` 文件（含仅存在于 overlay 的新文件），返回绝对、词法规范化、
    /// 排序后的路径。目录不存在返回空列表，不报错。
    fn discover(&self, source_root: &Path) -> Result<Vec<PathBuf>, String>;
    /// 读取源码文本；overlay 优先于磁盘。返回错误文本而非 Diagnostic。
    fn read(&self, path: &Path) -> Result<String, String>;
}

pub fn load_packages_collecting(packages: &[PackageSources]) -> LoadedSources;
pub fn load_packages_with_provider(
    packages: &[PackageSources],
    provider: &dyn SourceProvider,
) -> LoadedSources;
```

冻结规则：

1. `load_packages` 保持 `Result<LoadedProgram, Diagnostic>`：内部走收集式 loader；有诊断时
   返回第一条；否则返回合并程序。driver 与既有调用方不改也能编译。
2. 文件顺序 = 每个 `PackageSources` 按 `source_root` 发现并排序（含 `extra`，extra 在其后，
   按传入顺序），随后追加 stdlib 单元；`SourceId` 按该顺序分配（与当前 loader 一致）。
3. 诊断顺序 = 文件顺序，再按 span.start；跨文件诊断不得乱序。
4. `pkg` 校验错误按文件收集；全部文件解析且校验通过后才运行 `resolve_modules`（现有
   首错语义）；`resolve_modules` 失败时把该诊断追加进 `diagnostics` 且 `program=None`。
5. `SourceProvider` 路径规则：所有传入/返回路径为绝对路径并做纯词法规范化（去掉 `.`/`..`、
   统一分隔符），不做符号链接解析、不要求文件存在；比较 `exclude` 时用同一规范化。
   `DiskProvider` 用现有 `discover_sources` 语义（递归、仅 `.do`、目录不存在为空）。

### 5.4 声明级语义多错误收集

```rust
// crates/dolphin-hir/src/lower.rs（新增；现有 lower_sources/lower_library 不变）
/// 与 lower_sources_analysis 相同，但声明级错误全部收集；函数体 lowering 仍首错即停。
pub fn lower_sources_analysis_collecting(
    sources: &[SourceFile],
    program: &ast::Program,
    packages: &[PackageId],
    require_main: bool,
) -> Result<LoweredProgram, Vec<Diagnostic>>;
```

冻结规则：

1. `build_templates` 改为逐声明校验：失败声明报一条诊断并从模板表跳过，后续**独立**声明
   继续校验；引用被跳过声明的地方会得到自己的“未知类型/函数”诊断（真实错误，允许）。
2. 每个顶层声明（struct/enum/function/trait/impl）至多贡献一条声明级诊断；诊断总上限与
   §5.2 一致（100 条后追加 `E0002` 并停止）。
3. 声明级诊断非空 → 返回 `Err(全部诊断)`，不做实例化与函数体 lowering；为空 → 进入现有
   lowering 流程，函数体首个错误返回 `Err(vec![该诊断])`。
4. `require_main` 缺 `main` 也按一条声明级/全局诊断处理。
5. `lower_sources`/`lower_library` 的行为、错误顺序与返回类型不变；收集行为只通过新函数
   暴露，供 `check_manifest_collecting` 与 H20-02 的分析路径使用。

### 5.5 CLI 集成（保持退出码与首错文本）

```rust
// crates/dolphin-driver/src/lib.rs（新增）
/// 收集 lib + 全部 bin 的前端诊断；空 Vec 表示通过。
/// 每个目标：先收集式 loader；有诊断则不再 lower；否则调用 lower_sources_analysis_collecting，
/// 得到声明级全部诊断或函数体首个语义错误（每个目标至多一组）。
pub fn check_manifest_collecting(manifest: &Manifest, graph: &PackageGraph) -> Vec<Diagnostic>;
```

- `dc check` 打印全部收集到的诊断到 stderr（`eprintln!`，按列表顺序，不加分隔行），
  非空则退出 1；单文件模式同样走 recovering lex/parse。
- 跨目标去重：同一诊断在 lib 与多个 bin 目标重复出现时，按
  `(primary 路径, span, code, message)` 去重并保留首个目标的顺序（与 §6.5 第 6 条同一规则）。
- `dc build`/`run`/`test` 在编译前先调用 `check_manifest_collecting`：非空则打印全部并退出 1，
  不写对象、不写可执行、不写 `.dlib`、不生成测试入口；空则走现有构建路径（语义不变）。
- 首条诊断的 `Display` 文本与当前单错输出一致，因此现有“`contains` 单错文本”的测试不被破坏。

### 5.6 与 codegen 的边界

任何 `diagnostics` 非空或 `partial=true` 的单元，HIR 不得继续实例化/降低函数体；
`AnalysisSnapshot` 中该单元 `index=None`。错误程序不产生可执行产物（DIAG-05）。

### 5.7 拒绝方案

- 用 panic/`unwrap` 兜底恢复失败：违反总则 2；拒绝。
- 一次重写 parser 为纯 `Result` 组合子：超范围且回归风险高；在现有递归下降里加同步点；拒绝。
- 拆开函数体 lowering 收集表达式级多错误：计划明确不要求，回归风险高；拒绝。
- 收集时对被跳过声明静默继续并伪造模板：会产生级联假错误或假通过；跳过并保留真实级联错误；
  拒绝伪造。

### 5.8 正反例

- 正例：文件 A 缺一个 `}`、文件 B 有非法字符，`diag_02` 断言 CLI 打印多条、LSP 对两个 URI
  各发布对应诊断。
- 正例：同一文件两个独立顶层声明各有一个声明级错误（重复函数名 + 非法 impl 头），
  `diag_02` 同时得到两条；互不依赖的函数体错误仍只报首个。
- 正例：同一文件 120 处非法字符，只得到 100 条 + `E0002`，进程不 panic、不 OOM。
- 反例：`fn main() { var x: = 1; }` 恢复后若把残缺表达式交给 lower 会产生伪造类型进入
  codegen；必须由“有诊断就不 lower”阻断。

## 6. 决策 D：共享项目分析接口与快照生命周期（冻结）

### 6.1 新 crate 与依赖

新增 `crates/dolphin-analysis`（workspace 成员）：

- 依赖：`dolphin-source`、`dolphin-syntax`、`dolphin-hir`、`dolphin-package`；
- **不得**依赖 `dolphin-driver`、任何 codegen 后端、`dolphin-linker`、`dolphin-platform`；
- `dolphin-lsp` 改为依赖 `dolphin-analysis`（移除对 `dolphin-hir` 的直接依赖）。

### 6.2 只读依赖解析（依赖缺失行为）

```rust
// crates/dolphin-package/src/resolver.rs（新增）
/// 只读解析：读现有 dolphin.lock（若有），offline 模式，绝不写锁/索引/归档。
/// 远程依赖只能从已有锁摘要或缓存索引恢复；无法恢复时返回 E1001。
pub fn resolve_readonly(manifest: &Manifest, cache: &Cache) -> Result<PackageGraph, Diagnostic>;
```

冻结行为：

1. 路径依赖照常从磁盘解析；`file://` 仓库允许（本地读，不算网络）；HTTP(S) 一律禁止。
2. 锁文件存在时只读使用（不校验“是否过期”，不写回）；不存在时不报“缺少锁”，继续尝试
   从缓存恢复远程依赖。
3. 远程依赖无法恢复（无锁摘要、无缓存索引、摘要不匹配、解包失败）→ 返回
   `E1001`：`"dependency `{coordinate}` is not available locally: {reason}; run `dc fetch` and retry"`。
4. 允许把已有缓存归档解包到 `DOLPHIN_HOME`（现有 `ensure_unpacked` 行为），不得写项目目录。
5. CLI 的 `dc check/build/run/test/package/fetch/publish` **继续使用现有网络/写锁逻辑**
   （ANALYSIS-06）；只读解析只服务分析路径。

### 6.3 overlay 与 URI

```rust
// crates/dolphin-analysis
pub struct AnalysisHost { /* root, overlays, revision, cached snapshot */ }

impl AnalysisHost {
    pub fn new(root: PathBuf) -> Self;
    pub fn root(&self) -> &Path;
    /// 记录未保存文本；version 来自 LSP textDocument.version。
    /// 同一路径已有 >= version 的 overlay 时忽略并返回 false（stale）。
    /// 成功时 revision 自增并丢弃缓存快照。
    pub fn set_overlay(&mut self, path: PathBuf, version: u64, text: String) -> bool;
    /// didClose：移除 overlay；成功移除时 revision 自增。
    pub fn remove_overlay(&mut self, path: &Path) -> bool;
    pub fn revision(&self) -> u64;
    /// 按当前 overlays 构建（或返回缓存的）快照。
    pub fn snapshot(&mut self) -> Arc<AnalysisSnapshot>;
}

pub fn uri_to_path(uri: &str) -> Option<PathBuf>;  // 仅 file://；百分号解码；Windows 盘符
pub fn path_to_uri(path: &Path) -> String;         // 绝对路径 → file:// URI
```

冻结规则：

1. overlay 键是绝对、词法规范化路径；匹配任何包（根包与 path/缓存依赖）源码根下的文件。
2. `uri_to_path` 支持百分号解码（空格、Unicode、中文路径），Windows `file:///C:/x` →
   `C:\x`，反斜杠 URI 拒绝；非 `file` scheme 返回 `None`（LSP 对打开的该文档只做语法诊断，
   不进入项目分析）。
3. `didClose` 后：若磁盘存在则回到磁盘文本；若 overlay-only 文件在磁盘不存在则从发现集合
   移除，其诊断清空。
4. overlay-only 新文件若位于某包 `source_root` 下，按相对路径推导模块名参与该包分析；
   若位于任何 `source_root` 之外，只做单文件语法诊断。
5. 拥有权：`AnalysisHost` 拥有 overlay 文本；`snapshot()` 复制/派生数据，快照不借用 host。

### 6.4 lib/bin 选择（冻结）

1. 有 `[lib]`：产生一个 `UnitKind::Lib` 单元，加载 `source_root` 下除**全部** bin 入口外的
   文件，不要求 `main`（与 `compile_library` 一致）。
2. 每个 `[[bin]]`：产生一个 `UnitKind::Bin(name)` 单元，加载 `source_root` 下除**其他** bin
   入口外的文件（与 `compile_bin` 一致）；该 bin 入口缺失或缺少 `main` 产生诊断。
3. 共享 helper 文件会出现在多个单元；诊断合并去重（§6.5 第 6 条）。同一文件的符号定义在
   Lib 与 Bin 单元内一致；查询该文件时按 `Lib > Bin(按清单顺序)` 取第一个包含它的单元。
4. 单元顺序：Lib 在前，Bins 按 `dolphin.toml` 声明顺序。
5. `[package].source` 自定义目录、path 依赖、缓存依赖源码均按上述规则参与；缓存依赖只有
   lib（`.dlib` 不含 bin）。

### 6.5 快照与版本/取消规则

```rust
pub struct AnalysisSnapshot {
    pub revision: u64,
    pub root: PathBuf,
    pub mode: AnalysisMode,                       // Project | SingleFile
    pub graph: Option<Arc<PackageGraph>>,         // 只读解析成功时；用于包名显示与依赖源码定位
    pub units: Vec<AnalysisUnit>,
    pub diagnostics: Vec<Diagnostic>,             // 文件级，已合并去重、已排序
    pub project_diagnostics: Vec<Diagnostic>,     // 无 primary label（清单/依赖）
    pub partial: bool,
}

pub enum AnalysisMode { Project, SingleFile }
pub enum UnitKind { Lib, Bin(String) }

pub struct AnalysisUnit {
    pub kind: UnitKind,
    pub sources: Vec<SourceFile>,
    pub packages: Vec<PackageId>,
    pub diagnostics: Vec<Diagnostic>,
    pub partial: bool,
    pub index: Option<Arc<SymbolIndex>>,          // 语义成功时
}
```

1. **无异步**：H20-03 在传输循环内同步处理每条消息；M20 不引入后台线程、任务队列或取消令牌。
   因此“旧任务覆盖新快照”不可能发生；仍以 `revision` 作为契约：任何未来异步实现必须在发布前
   比较 `snapshot.revision == host.revision()`，不等则丢弃结果。
2. **stale overlay**：`set_overlay` 对同路径旧 version 返回 `false`，不修改状态、不提升 revision。
3. **缓存**：host 只在 revision 变化后重建快照；连续查询同一 revision 返回同一 `Arc`。
4. **发布带版本**：LSP `publishDiagnostics` 对打开文档携带其 overlay version（未打开则省略）。
5. **项目解析失败**：`resolve_readonly` 失败时 `units=[]`、`project_diagnostics=[E1001]`、
   `partial=true`；LSP 对每个打开文档仍运行单文件语法分析并发布（§7.4），保证“无法分析”
   不被显示为“没有错误”。
6. **合并去重**：`AnalysisSnapshot.diagnostics` 按
   `(primary label 的规范化路径, span, code, message)` 去重，保留第一个单元的顺序；
   排序键为 `(路径, span.start, code)`。项目级诊断不进 `diagnostics`。

### 6.6 符号索引与解析

```rust
pub struct SymbolIndex {
    pub definitions: BTreeMap<DefId, Definition>,
    pub type_names: BTreeMap<ir::TypeId, SymbolId>,     // Def 或 Instance（含实参）
    pub function_instances: BTreeMap<ir::FunctionId, SymbolId>,
    pub resolutions: Vec<ResolutionEntry>,              // 按 (source, span.start) 排序
}

pub struct DocumentSymbol {
    pub name: String,          // 非限定名
    pub qualified: String,     // 全限定名（含包前缀）
    pub kind: DefKind,
    pub name_span: Span,
    pub span: Span,            // 声明整体范围（结构体/枚举/trait/impl；函数为函数体范围）
}

pub enum SymbolId {
    Def(DefId),
    Instance { def: DefId, type_args: Vec<ir::Type> },
    Local { source: SourceId, name_span: Span },
    Module { qualified: String },
}

pub struct DefId { pub package: PackageId, pub qualified: String }
pub enum DefKind { Function, Struct, Enum, Trait, Method, Field, Variant, TypeParam }
pub struct Definition {
    pub id: SymbolId, pub kind: DefKind,
    pub source: SourceId, pub name_span: Span,
    pub signature: String,        // 稳定文本，见 §7.5
}
pub struct ResolutionEntry { pub source: SourceId, pub span: Span, pub resolution: Resolution }
pub enum Resolution {
    Local { source: SourceId, name_span: Span },
    Def(DefId),
    Instance { def: DefId, type_args: Vec<ir::Type> },
    Module { qualified: String },
    Unresolved,
}

impl SymbolIndex {
    pub fn resolve(&self, source: SourceId, offset: usize) -> Resolution;
    pub fn definition_of(&self, id: &SymbolId) -> Option<&Definition>;
    pub fn render_type(&self, ty: &ir::Type) -> String;
    pub fn document_symbols(&self, source: SourceId) -> Vec<DocumentSymbol>;
}
```

1. `definitions` 来源：`dolphin-hir` 新增的 lowering side table（§6.7），包含
   函数/结构体/枚举/trait/方法/字段/variant/类型参数；`qualified` 已含包前缀。
2. `resolutions` 来源：`dolphin-analysis` 在**已限定** AST 上做词法作用域遍历：函数参数、
   `val`/`var`、`for` 变量、match 绑定形成作用域；内层遮蔽外层；`use` 别名与模块路径按
   `modules::resolve_modules` 的结果解析。禁止纯文本同名匹配。
3. **显示名**：身份始终是 `DefId`/`SymbolId`；展示文本把依赖包前缀 `@<id>.` 替换为包
   `name`（`@1.stats.count` → `textstats.stats.count`），根包与 `std` 保持原样；同名包只影响
   显示、不影响解析。
4. `render_type`：用户类型用 `type_names` 的 `SymbolId`（实例带实参），内建类型用现有名字，
   泛型实参递归渲染（`Vec<i32>`、`Pair<textstats.Point>`），不得输出 `TypeId(n)`。
5. `Definition.signature` 冻结格式：函数/方法
   `fn <显示名>(<参数类型, 逗号分隔>) -> <返回类型>`（Unit 返回省略 `-> ...`）；结构体
   `struct <显示名>`；枚举 `enum <显示名>`；trait `trait <显示名>`；字段
   `<字段名>: <类型>`；variant `<枚举显示名>.<variant>(<类型列表>)`；类型参数 `<名称>`。
6. 拥有权：`SymbolIndex` 不可变、拥有自身数据；`Resolution` 不含引用；快照间不共享可变状态。
7. 表达式级类型推断不在 M20 范围；`resolve` 返回定义身份，不返回表达式类型。

### 6.7 HIR side table（最小改动）

```rust
// crates/dolphin-hir/src/lower.rs（新增；lower_sources 行为不变）
pub struct LoweredProgram {
    pub program: ir::Program,
    pub analysis: AnalysisData,
}

pub struct AnalysisData {
    pub definitions: Vec<DefinitionData>,   // qualified/package/source_id/name_span/kind
    pub type_names: Vec<GenericKey>,        // 下标 = TypeId
    pub function_instances: Vec<GenericKey>,// 下标 = FunctionId
}

pub fn lower_sources_analysis(sources: &[SourceFile], program: &ast::Program,
                              packages: &[PackageId]) -> Result<LoweredProgram, Diagnostic>;
// §5.4 的收集式入口，声明级诊断全部返回：
pub fn lower_sources_analysis_collecting(sources: &[SourceFile], program: &ast::Program,
                                         packages: &[PackageId], require_main: bool)
    -> Result<LoweredProgram, Vec<Diagnostic>>;
```

- `lower_sources`/`lower_library` 改为调用同一实现并只返回 `.program`，行为与错误顺序不变。
- `AnalysisData` 由 `ProgramLowerer` 已有的 `tables`/`state.type_keys`/`instance_ids` 汇总，
  不新增类型推断、不改变布局或 ABI。
- 分析路径用收集式入口：声明级错误全部进 `AnalysisUnit.diagnostics`；函数体错误按首错进入。

### 6.8 拒绝方案

- 把分析放进 `dolphin-driver`：会拖入 codegen feature 与 `compile_error!`；拒绝。
- 每次 didChange 重新 `resolve_project`（会写锁/下载）：违反 ANALYSIS-01；拒绝。
- 为分析单独复制一份包解析逻辑：与构建行为漂移；只读解析复用 `resolver`/`Registry`；
  拒绝。
- 全量增量框架（salsa 类）：计划明确不强制；拒绝。

### 6.9 正反例

- 正例：`analysis_01` 在带缓存远程依赖的项目上取快照，断言 `dolphin.lock` 哈希与项目文件
  列表不变、`target/` 不新增文件、HTTP 不被访问（不可达 base 仍返回 E1001 提示 `dc fetch`）。
- 正例：`analysis_02` 覆盖 path 依赖 `textstats` 的未保存文本，调用方 `dtext` 诊断随 overlay
  变化；`didClose` 后恢复磁盘版本。
- 反例：同一文件同时属于 lib 与两个 bin，若按 `SourceId` 去重会发布两份相同诊断；必须按
  `(路径, span, code, message)` 去重。

## 7. 决策 E：LSP 协议与测试协议（冻结）

### 7.1 启动与能力

- CLI：`dc lsp [PROJECT]`（可选位置参数，默认当前目录；无参数调用保持可用）。项目根由 CLI
  参数决定；`initialize.rootUri`/`workspaceFolders` 只用于 URI↔路径映射，不改变根。
- 根目录向上找不到 `dolphin.toml` 时进入 `AnalysisMode::SingleFile`：每个打开文档按现有
  单文件规则分析（`pkg`/`use` 或缺少 `main` 时跳过语义检查），保证无项目文件的编辑体验不退化。
- `initialize` 结果冻结为：
  ```json
  {"capabilities": {
    "textDocumentSync": 1,
    "hoverProvider": true,
    "definitionProvider": true,
    "documentSymbolProvider": true,
    "positionEncoding": "utf-16"
  }}
  ```
  不新增 pull diagnostics/completion/semantic tokens；同步方式保持全文同步（`didChange` 只取
  `contentChanges` 最后一项的 `text`）。

### 7.2 生命周期与错误

| 情形 | 冻结行为 |
| --- | --- |
| `initialize` 之前的请求（除 `initialize`） | JSON-RPC error `-32002`（Server not initialized） |
| `shutdown` | `result: null` |
| `shutdown` 之后的请求（除 `exit`） | JSON-RPC error `-32600` |
| `exit`（已 `shutdown`） | 进程退出码 0 |
| `exit`（未 `shutdown`） | 进程退出码 1（D-M20-2） |
| EOF（客户端断开） | 正常退出，退出码 0（保持现状） |
| 未知方法请求 | JSON-RPC error `-32601`（Method not found），带原 `id`（D-M20-1） |
| 未知通知 | 忽略，不响应 |
| JSON 体解析失败 | 跳过该消息并继续（保持现状；文档标注为容错而非 LSP 严格行为） |
| 缺少/非法 `Content-Length`、超 16 MiB | 终止服务，stderr 写原因，退出码非 0 |

### 7.3 诊断发布

1. 任何 overlay 变化后重算；对每个**打开的**文档，若诊断集合与上次发布不同才发布。
2. 发布顺序：URI 字典序升序，保证确定性；同文档内诊断按
   `(range.start.line, range.start.character, code)` 排序。
3. 打开文档携带 `version`（该文档 overlay 的 version）；未打开文档省略。
4. `didClose`：立即发布该 URI 空诊断；其余打开文档按第 1 条重算。
5. 项目级诊断（`project_diagnostics`，无 primary label）通过 `window/showMessage`
   （type=1）发送一次，消息为各诊断 `message` 换行连接；不伪造文件级 range。

### 7.4 “无法分析”不等于“没有错误”

- 项目解析失败时：对每个打开文档仍发布单文件语法诊断；有语法错误就显示语法错误。
- 单文件/项目语义分析失败（`partial`）：文档已有的词法/语法诊断照常发布；项目级原因经
  `window/showMessage` 提示，不发布空数组伪装通过。
- `publishDiagnostics` 的 `message` 使用 `Diagnostic.message()`，`code` 使用 `Diagnostic.code()`。

### 7.5 hover / definition / documentSymbol

| 请求 | 冻结结果 |
| --- | --- |
| hover 定义（函数） | markdown ``"fn `<显示名>`"`` |
| hover 定义（结构体/枚举/trait） | ``"struct `<显示名>`"`` / ``"enum …"`` / ``"trait …"`` |
| hover 局部变量 | ``"local `<name>`"``；有类型标注时追加 ``": <render_type>"`` |
| hover 模块 | ``"module `<显示名>`"`` |
| hover 未解析 | `null` |
| definition 定义 | 单个 `Location {uri, range=name_span}`；跨包指向缓存源码 `file://` 路径 |
| definition 局部变量 | 同文件 `Location` 指向绑定 `name_span` |
| definition 未解析 | `null` |
| documentSymbol | 该文件顶层声明（函数/结构体/枚举/trait/impl），`name` 为非限定名，`kind`/`range`/`selectionRange` 保持现有取值 |

- hover 的 `range` 为光标处标识符 token span；definition 不返回 `LocationLink`、不返回数组。
- 局部遮蔽、参数、`for` 变量、match 绑定必须解析到最近绑定（LSP-03）。

### 7.6 LSP 测试协议（冻结）

新增 `tests/support/lsp.rs` 与 `tests/m20_lsp.rs`：

```rust
pub struct LspSession { /* child, stdin, stdout reader */ }
impl LspSession {
    pub fn start(project: &Path) -> Self;          // 启动 `dc lsp <project>`
    pub fn send(&mut self, message: Value);
    pub fn recv_response(&mut self, id: i64) -> Value;      // 10s 超时
    pub fn recv_notification(&mut self, method: &str) -> Value;
    pub fn shutdown(&mut self);                    // shutdown → exit → 断言退出码 0
    pub fn finish(self) -> std::process::ExitStatus;
}
```

1. 必须通过真实子进程 stdio 会话（`Content-Length` 帧、大小写不敏感头部、`\r\n\r\n` 结束），
   不得只调用 `Server::handle` 内部 helper 作为验收；crate 内单测可以保留为补充。
2. 超时（10s）到达即 kill 子进程并在 panic 中带上已读输出与 stderr。
3. 固定握手：`initialize(id=1, rootUri)` → 断言 capabilities（含 `positionEncoding`）→
   `initialized` 通知 → 场景 → `shutdown` → `exit` → 退出码 0。
4. 断言策略：能力与协议错误断言完整 JSON；诊断断言 `code`、`range`、`message` 关键内容与
   排序，不绑定整段渲染文本；每次诊断发布断言 `version`（打开文档）。
5. 三平台默认 lane 必须运行 `m20_lsp`；Windows 增加盘符/反斜杠 URI 用例。

### 7.7 拒绝方案

- 引入异步 LSP runtime/任务队列：计划明确不必；同步实现 + revision 契约；拒绝。
- 用 `tower-lsp` 等新依赖：新增依赖需单独决策且超出最小改动；拒绝。
- 保留未知方法 `result:null`：不满足 LSP-06；拒绝（D-M20-1）。
- 让 hover 回退到文本同名查找：正是要修的行为；拒绝。

## 8. 决策 F：Formatter 项目发现与保持性（冻结）

### 8.1 发现规则（FMT-05，D-M20-4）

1. `dc fmt` 无路径参数：从 cwd 向上发现 `dolphin.toml`；找到则根为
   `[package].source`（默认 `src`），否则根为 `src`（保持现状）。
2. 显式路径参数：文件精确格式化；目录递归格式化其中 `.do`；显式单文件即使位于构建输出
   目录也格式化。
3. 递归排除（无论默认还是显式目录）：项目 `build.output`（默认 `target`）目录、`.git` 目录；
   不跟随目录符号链接（防环）；非 `.do` 忽略。
4. 输出统一 LF；CRLF 输入会被改写为 LF（保持现有 `dolphin-format` 行为并写入文档）。
5. 不读取/不修改 `dolphin.lock`、不写构建产物。

### 8.2 写入原子性（D-M20-4）

1. 先对全部选中文件在内存中格式化；任一文件格式化失败则**本次不写任何文件**，打印错误并
   退出 1（全有或全无；`FMT-03` 的“错误文件不被部分覆盖”由此保证）。
2. `--check` 只报告需要格式化的文件，不写任何文件，需要格式化时退出非 0（保持现状）。
3. 写入前重新读取并确认内容未变？不做（会引入竞态复杂度）；写入失败按文件报错并退出 1。

### 8.3 保持性（FMT-01/02）

1. 幂等：`format(format(x)) == format(x)`，对全部 `examples/**/*.do` 与测试 fixture。
2. 语义 token 等价：对原文与格式化文本分别 `lex`，比较非 trivia（lexer 本就跳过空白与注释）
   的 `TokenKind` 序列（含 `Identifier`/`Number`/`String`/`Character` 的字面值）；注释另行
   断言“注释文本集合不变”（`//` 与 `/* */` 出现次数与内容，忽略行尾空白与缩进）。
3. 若某语法触发 token 不等价，H20-04 必须先修 `dolphin-format` 再冻结通过；不得用跳过该
   示例来通过。

### 8.4 正反例

- 正例：`fmt_02` 对包含泛型、`defer`、`extern "C"`、字符串内 `{}`/`//`、块注释的 fixture
  断言 token 序列不变、注释数量不变。
- 正例：`fmt_05` 断言 `dc fmt` 在带清单项目中不触碰 `target/` 下的 `.do`；显式传该文件时
  仍格式化。
- 反例：目录扫描进入 `target/` 会把生成入口 `*-tests.entry.do` 格式化并污染构建输入；必须排除。

## 9. 决策 G：调试器验收协议（冻结）

### 9.1 范围

只验 Linux gdb 与 macOS lldb 上的 **LLVM Debug** DWARF；Cranelift、Windows/PDB 明确不纳入
本阶段（DBG-04 记录边界）。不宣称局部变量值/类型检查完整支持。

### 9.2 脚本与测试

- `scripts/debug_smoke.sh <exe> <break-file> <break-line>`：用 gdb 批处理运行，输出归一化
  的断点命中、`info line`、`bt` 文本到 stdout；找不到 gdb 时以非 0 退出并打印
  `debugger not available`。
- `scripts/debug_smoke_lldb.sh <exe> <break-file> <break-line>`：macOS lldb 等价实现。
- `tests/m20_debug.rs`：在临时目录生成两文件项目（`src/math.do` 提供 `add`，`src/main.do`
  调用 `add` 后返回），用 `dc build --backend llvm`（Dolphin Debug）构建，再调用脚本并断言：
  - DBG-01：调试器实际加载（脚本退出 0，输出含 `Breakpoint`/`stop reason`）；
  - DBG-02：断点命中正确文件与行（输出含 `<math.do>:<期望行>`）；
  - DBG-03：`bt` 含 `#0`（math 函数）与 `#1`（main），且顺序正确；
  - DBG-04：Release（或 `--backend cranelift`）构建不产生可用行信息，断言脚本以文档化
    方式报告“无行表/未命中”，不算失败。
- 测试仅在 `--features llvm` 且存在 `gdb`（Linux）/`lldb`（macOS）时执行；否则测试
  **失败并提示设置 `DOLPHIN_SKIP_DEBUGGER=1` 显式跳过**，跳过时报告必须列为“未验证”，
  不得写成通过。未启用 `llvm` feature 时该测试文件不编译，报告中同样列为未运行，
  不得以“0 tests”视为通过。CI 的 LLVM lane 安装 gdb 后运行 `--test m20_debug`。
- 行号约定：fixture 源码中用注释 `// DBG-BREAK` 标记期望断点行，测试扫描该标记取行号，
  避免硬编码漂移。

### 9.3 正反例

- 正例：断点命中 `src/math.do` 的 `add` 函数体首行，`bt` 显示 main 调用帧。
- 反例：用“可执行文件里存在 `.debug_line` 段”代替真实断点验收：那是 M17 已有检查，
  不满足 DBG-02/03；拒绝。

## 10. 源码入口、拥有权与错误行为

### 10.1 源码入口

| 工作 | 入口 |
| --- | --- |
| `SourceId`/`SourceMap`/`SourceFile.id` | `crates/dolphin-source/src/source.rs` |
| 结构化 `Diagnostic`/`Severity`/`Label`/渲染 | `crates/dolphin-source/src/diagnostic.rs` |
| `lex_recovering` | `crates/dolphin-source/src/lexer.rs` |
| `parse_recovering` | `crates/dolphin-syntax/src/parser.rs` |
| 收集式 loader / `SourceProvider` / `LoadedSources` | `crates/dolphin-hir/src/modules.rs` |
| lowering side table / `LoweredProgram` | `crates/dolphin-hir/src/lower.rs`、`monomorphize.rs`（只读汇总） |
| 只读依赖解析 | `crates/dolphin-package/src/resolver.rs`（+ 现有 `registry`/`cache`/`lockfile`） |
| 分析宿主/快照/索引/URI/overlay | 新 `crates/dolphin-analysis/src/`（`lib.rs`、`host.rs`、`index.rs`、`uri.rs`） |
| LSP 协议与 stdio | `crates/dolphin-lsp/src/lib.rs` |
| CLI `lsp` 参数、`fmt` 发现、多诊断打印 | `src/main.rs` |
| 收集式检查入口 | `crates/dolphin-driver/src/lib.rs` |
| 调试脚本 | `scripts/debug_smoke.sh`、`scripts/debug_smoke_lldb.sh` |
| 测试 | `tests/m20_diag.rs`、`tests/m20_analysis.rs`、`tests/m20_lsp.rs`、`tests/m20_fmt.rs`、`tests/m20_debug.rs`、`tests/support/lsp.rs` |
| CI | `.github/workflows/ci.yml`（新增 `--test` 列表与 gdb 安装） |

### 10.2 拥有权与生命周期

| 对象 | 拥有者 | 生命周期/失效规则 |
| --- | --- | --- |
| `SourceFile` | 加载单元/快照 | 快照存活期；`SourceId` 仅在该单元内有效 |
| `Diagnostic` | 返回值/快照 | 完全拥有；`SourceId` 需配合产生它的 `SourceMap` 使用 |
| overlay 文本 | `AnalysisHost` | `didClose`/替换前有效；快照持有自己的副本 |
| `AnalysisSnapshot` | 调用方（`Arc`） | 不可变；新 revision 生成新快照，旧快照可读但不得写回 |
| `SymbolIndex` | 快照 | 不可变；不跨快照缓存 |
| LSP JSON | 传输循环 | 写出后丢弃 |

### 10.3 错误行为汇总

| 情形 | 行为 |
| --- | --- |
| 词法/语法/包校验错误 | 收集为 `Diagnostic`，`partial=true`，不 lower，不产生产物 |
| 声明级语义错误 | 逐声明收集（§5.4），`partial=true`，不 lower，不产生产物 |
| 函数体语义错误 | 该目标首个错误返回 `Diagnostic`，不 codegen |
| 依赖缺失/缓存不可用 | `E1001` 项目诊断 + `window/showMessage`；文档仍发语法诊断 |
| overlay 旧版本 | 忽略（返回 false），不提升 revision |
| URI 非 `file`/非法 | 该文档不进入项目分析；仅语法诊断 |
| 分析内部不可恢复错误 | 转 `E2001` 项目诊断；不得 panic、不得返回“无错误” |
| 调试器缺失 | `m20_debug` 失败并提示显式跳过；报告中列为未验证 |

## 11. 兼容性影响

- **持久格式**：`dolphin.toml`、`dolphin.lock`、`.dlib` 均无字段/格式变化；分析不写锁、
  不下载、不产生产物（§6.2、§6.5）。
- **CLI**：
  - `dc lsp [PROJECT]`：新增可选位置参数，无参数调用不变；协议行为变化见 D-M20-1/2。
  - `dc check/build/run/test`：错误程序现在可能打印多条词法/语法/声明级诊断；首条文本、
    退出码 1、成功输出均不变；不产生产物的保证加强。
  - `dc fmt`：默认根与排除集合、部分失败的全有或全无行为变化见 D-M20-4；`--check` 语义不变。
- **Rust 库 API**：`SourceFile` 新增 `pub id` 字段（仓库内无结构体字面量，风险仅限外部）；
  `Diagnostic` 新增字段/访问器但 `plain`/`at` 签名与 `Display` 不变；新增 crate
  `dolphin-analysis`；`dolphin-lsp` 依赖调整。`load_packages`/`lower_sources`/`resolve_project`
  签名保持。
- **资源模型**：不新增语言级资源；分析可解包缓存归档到 `DOLPHIN_HOME`，不触碰项目与用户文件。
- **保留命名**：`dolphin-analysis`、`SourceId`、`AnalysisHost` 等为新增公开名；无冲突。
- **未实现标注**：H20-01..05 完成前，本文所有 API 与行为均不得写入当前能力文档或示例。

## 12. 测试矩阵与批次完成标准

统一要求：每个验收点有真实测试名、固定期望与执行结果；诊断/分析/LSP 与后端无关，默认
后端三平台必须运行；涉及构建/产物的断言显式使用 Dolphin Debug/Release；新测试文件加入
CI 显式 `--test` 列表（LLVM lane）与 README 开发验证命令；未运行项如实标注。

### 12.1 H20-01 结构化诊断与有限恢复

| 验收 | 测试（`tests/m20_diag.rs`） | 完成标准 |
| --- | --- | --- |
| DIAG-01 | `diag_01_cli_and_lsp_same_error_identity` | CLI `E0001` 文本位置与 LSP `code`/UTF-16 range 完全一致 |
| DIAG-02 | `diag_02_two_files_independent_errors` | 两文件各一独立错误（含语法与声明级语义各一）同时出现在 CLI 与 LSP |
| DIAG-03 | `diag_03_non_bmp_and_crlf_positions` | 非 BMP 字符前后位置、CRLF 行列正确；不 panic |
| DIAG-04 | `diag_04_generic_chain_and_user_type_names` | 错误消息含可读泛型/用户类型名，无 `TypeId(n)` |
| DIAG-05 | `diag_05_error_program_no_panic_no_artifact` | 退出 1、无产物、stderr 无 `panicked` |
| 上限 | `diag_06_error_collection_bound` | 词法/语法与声明级各 100 条 + `E0002`，行为确定 |

完成标准：上述测试在 Linux/Windows/macOS 默认 lane 与 Linux LLVM lane 通过；旧测试全绿；
`cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings`；
报告给出每条测试的固定期望与未验证项。

### 12.2 H20-02 共享项目分析接口与 overlay

| 验收 | 测试（`tests/m20_analysis.rs`） | 完成标准 |
| --- | --- | --- |
| ANALYSIS-01 | `analysis_01_no_network_no_lock_write_no_artifacts` | 锁/项目文件不变、无网络、无 `target/` 新文件 |
| ANALYSIS-02 | `analysis_02_unsaved_dependency_affects_caller` | overlay 依赖改变调用方诊断；关闭后恢复 |
| ANALYSIS-03 | `analysis_03_library_without_main` | lib-only 项目无“缺 main”误报 |
| ANALYSIS-04 | `analysis_04_multi_bin_and_custom_source` | 自定义 source + 多 bin 单元选择正确 |
| ANALYSIS-05 | `analysis_05_same_name_different_package_not_confused` | 同坐标/同名不同包定义不混淆 |
| ANALYSIS-06 | `analysis_06_cli_build_behavior_unchanged` | 同项目 CLI 构建行为与 M19 基线一致 |

完成标准：`dolphin-analysis` 单测（overlay stale、URI 编解码、单元选择、作用域解析）与上表
通过；Windows 路径/盘符用例通过；`implemented-features` 只在通过后更新。

### 12.3 H20-03 项目级 LSP

| 验收 | 测试（`tests/m20_lsp.rs`，真实 stdio 会话） | 完成标准 |
| --- | --- | --- |
| LSP-01 | `lsp_01_pkg_use_type_error` | 含 `pkg`/`use` 文档得到项目语义诊断 |
| LSP-02 | `lsp_02_cross_file_and_cross_package_definition` | definition 指向依赖源码正确 `name_span` |
| LSP-03 | `lsp_03_local_shadowing_and_parameters` | 参数/局部/遮蔽解析到最近绑定 |
| LSP-04 | `lsp_04_open_change_close_dependency_change` | 打开/变更/关闭/依赖 overlay 事件序列正确 |
| LSP-05 | `lsp_05_sequential_versions_latest_wins` | 连续版本发布带正确 version，最终为最新文本 |
| LSP-06 | `lsp_06_unknown_request_lifecycle_conformance` | capabilities、-32601/-32002/-32600、退出码按 §7.2 |
| M19 流程 | `lsp_07_m19_development_flow`（H20-05 关闭） | 编辑器流程：错误→修复→跨包定义，并列出仍缺功能 |

完成标准：三平台默认 lane 通过；已有 `dolphin-lsp` 单测更新为 D-M20-1 行为后通过。

### 12.4 H20-04 Formatter 保持性与项目发现

| 验收 | 测试（`tests/m20_fmt.rs`） | 完成标准 |
| --- | --- | --- |
| FMT-01 | `fmt_01_idempotent_on_examples_and_fixtures` | 全部示例/fixture 二次格式化完全一致 |
| FMT-02 | `fmt_02_token_preservation_and_comments` | 非 trivia token 序列与注释不变 |
| FMT-03 | `fmt_03_error_file_not_written_all_or_nothing` | 任一失败时不写任何文件（D-M20-4） |
| FMT-04 | `fmt_04_check_writes_nothing` | `--check` 零写入、非零退出 |
| FMT-05 | `fmt_05_project_discovery_and_crlf` | 清单 source、排除 target/.git、CRLF→LF |
| FMT-06 | `fmt_06_m1_m19_examples_behavior_preserved` | 格式化后的 M1-M19 示例仍可构建/自测 |

完成标准：三平台默认 lane 通过；若发现 token 破坏，先修 formatter 再验收。

### 12.5 H20-05 调试器与整体体验

| 验收 | 证据 | 完成标准 |
| --- | --- | --- |
| DBG-01 | `tests/m20_debug.rs::dbg_01_debugger_loads` | gdb/lldb 实际加载 LLVM Debug 产物 |
| DBG-02 | `dbg_02_breakpoint_correct_file_line` | 多文件断点命中正确文件行 |
| DBG-03 | `dbg_03_cross_function_call_stack` | `bt` 含 `#0`/`#1` 正确调用链 |
| DBG-04 | `dbg_04_release_without_debug_boundary` | 优化/无调试边界有文档化结果 |
| 整体 | `lsp_07_m19_development_flow` | M19 项目完整开发流程走通，报告仍缺功能 |

完成标准：LLVM Debug 上通过；工具缺失时明确未验证；Cranelift/PDB 不在本阶段。

### 12.6 阶段完成标准

M20 阶段完成需要四种证据同时成立：代码实现、自动化验收、真实示例/工具流程（`examples/m19`
+ `tests/m20_lsp.rs` 的 `lsp_07`）、当前文档。**H20-00 只产出规格，不满足上述任何一条**；
阶段完成状态只能根据已发生的验证更新，且需用户确认。

## 13. 需用户确认的最小决策请求

| 编号 | 请求 | 推荐 | 影响面 | 确认结果 |
| --- | --- | --- | --- | --- |
| D-M20-1 | `dc lsp` 未知方法请求从 `result:null` 改为 JSON-RPC `-32601`；`initialize` 前的请求改为 `-32002` | 按 LSP 3.17 冻结（§7.2） | 已有客户端与 `dolphin-lsp` 单测 `unknown_request_returns_null` 必须更新 | 2026-09-24 用户同意 |
| D-M20-2 | `dc lsp` 在未收到 `shutdown` 就收到 `exit` 时以退出码 1 结束（已 `shutdown` 后 `exit` 为 0；EOF 仍为 0） | 按 LSP 规范冻结（§7.2） | 脚本化启动的客户端会观察到退出码变化 | 2026-09-24 用户同意 |
| D-M20-3 | LSP `publishDiagnostics.message` 改为纯消息文本，新增 `code`/`relatedInformation`；CLI 渲染不变 | 按 §4.1/§7.3 冻结 | 编辑器显示文本更干净；协议字段为增量 | 2026-09-24 用户同意 |
| D-M20-4 | `dc fmt`：有清单时默认根为 `[package].source` 并排除 `build.output`/`.git`；显式单文件仍精确生效；任一选中文件格式化失败时本次不写任何文件 | 按 §8.1/§8.2 冻结 | `dc fmt .` 默认写入集合与部分失败行为改变 | 2026-09-24 用户同意 |

D-M20-1..4 已确认；未在表中的变化均为增量（新增可选参数、新增 crate/字段/访问器、错误输出
更完整），不改变既有成功路径与持久格式。

## 14. 未决与阻塞

1. **H20-01 已解阻、等待派发**：D-M20-1..4 已于 2026-09-24 确认；收到 H20-01 批次指令前
   不得开始编码。派发时按第 12.1 节执行。
2. **H20-05 工具可用性**：CI/本机是否可安装 gdb、macOS 是否具备 lldb 需在派发 H20-05 时
   核实；缺失时该项标未验证，不用其他平台结果代替。
3. **函数体内多错误收集**：M20 明确不做（§5.1）；若后续真实项目证明必需，另开决策与批次，
   不在本阶段扩围。
4. **表达式级类型/局部变量值**：不在 M20；hover 与调试器只提供声明信息，若用户要求更强
   能力需新规格。
5. 本规格所有 API 与行为均**未实现**；H20-00 完成不等于 M20 完成，也不得在
   `implemented-features`/README 标记为当前能力。
