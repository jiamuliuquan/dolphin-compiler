//! 符号索引与词法作用域解析（M20/H20-02，§6.6）。
//!
//! 定义来自 `dolphin-hir` 的 lowering side table；解析在**已限定** AST 上做
//! 词法作用域遍历（函数参数、`val`/`var`、`for` 变量、match 绑定、内层遮蔽），
//! 模块路径与限定名使用 `modules::resolve_modules` 的产物，禁止纯文本同名匹配。

use std::collections::{BTreeMap, HashMap, HashSet};

use dolphin_hir::lower::{AnalysisData, DefinitionData, DefinitionKind};
use dolphin_hir::monomorphize::GenericKey;
use dolphin_ir::ir;
use dolphin_package::package::{PackageGraph, PackageId};
use dolphin_source::source::{SourceFile, SourceId, Span};
use dolphin_syntax::ast;

/// 定义身份：包 + 全限定名（同一快照内唯一）。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefId {
    pub package: PackageId,
    pub qualified: String,
}

/// 定义种类（M20 §6.6 冻结集合）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefKind {
    Function,
    Struct,
    Enum,
    Trait,
    Method,
    Field,
    Variant,
    TypeParam,
}

/// 符号身份：定义、泛型实例、局部绑定或模块。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolId {
    Def(DefId),
    Instance {
        def: DefId,
        type_args: Vec<ir::Type>,
    },
    Local {
        source: SourceId,
        name_span: Span,
    },
    Module {
        qualified: String,
    },
}

/// 一条定义：身份、种类、名字位置与稳定签名文本。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub id: SymbolId,
    pub kind: DefKind,
    pub source: SourceId,
    pub name_span: Span,
    pub signature: String,
}

/// 一个标识符位置的解析结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Local {
        source: SourceId,
        name_span: Span,
    },
    Def(DefId),
    Instance {
        def: DefId,
        type_args: Vec<ir::Type>,
    },
    Module {
        qualified: String,
    },
    Unresolved,
}

/// 一条解析记录：`(source, span)` 上的标识符解析为 `resolution`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolutionEntry {
    pub source: SourceId,
    pub span: Span,
    pub resolution: Resolution,
}

/// 文档符号（`documentSymbol` 用；impl 无对应 `DefKind`，由 LSP 层另行处理）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSymbol {
    pub name: String,
    pub qualified: String,
    pub kind: DefKind,
    pub name_span: Span,
    pub span: Span,
}

/// 不可变符号索引；由分析单元成功 lowering 后构建，不跨快照缓存。
pub struct SymbolIndex {
    pub definitions: BTreeMap<DefId, Definition>,
    /// 下标是 `TypeId`。
    pub type_names: BTreeMap<ir::TypeId, SymbolId>,
    /// 下标是 `FunctionId`。
    pub function_instances: BTreeMap<ir::FunctionId, SymbolId>,
    /// 按 `(source, span.start, span.end)` 排序。
    pub resolutions: Vec<ResolutionEntry>,
    by_qualified: HashMap<String, DefId>,
    modules: HashSet<String>,
    display_names: HashMap<PackageId, String>,
    documents: BTreeMap<u32, Vec<DocumentSymbol>>,
}

impl SymbolIndex {
    pub fn new(
        analysis: &AnalysisData,
        program: &ast::Program,
        sources: &[SourceFile],
        graph: Option<&PackageGraph>,
    ) -> SymbolIndex {
        let display_names: HashMap<PackageId, String> = graph
            .map(|graph| {
                graph
                    .packages
                    .iter()
                    .map(|package| (package.id, package.manifest.package.name.clone()))
                    .collect()
            })
            .unwrap_or_default();

        let mut definitions = BTreeMap::new();
        let mut by_qualified = HashMap::new();
        for data in &analysis.definitions {
            let def = DefId {
                package: data.package,
                qualified: data.qualified.clone(),
            };
            let kind = map_kind(data.kind);
            let signature = signature_for(program, data, kind, &display_names);
            let definition = Definition {
                id: SymbolId::Def(def.clone()),
                kind,
                source: source_id(data.source_id),
                name_span: data.name_span,
                signature,
            };
            by_qualified
                .entry(data.qualified.clone())
                .or_insert_with(|| def.clone());
            definitions.insert(def, definition);
        }

        let mut modules = HashSet::new();
        for qualified in by_qualified.keys() {
            let mut rest = qualified.as_str();
            while let Some((prefix, _)) = rest.rsplit_once('.') {
                modules.insert(prefix.to_string());
                rest = prefix;
            }
        }
        modules.insert("std.mem".to_string());

        let type_names = analysis
            .type_names
            .iter()
            .enumerate()
            .map(|(index, key)| (ir::TypeId(index), symbol_for_key(key)))
            .collect();
        let function_instances = analysis
            .function_instances
            .iter()
            .enumerate()
            .map(|(index, key)| (ir::FunctionId(index), symbol_for_key(key)))
            .collect();

        let mut index = SymbolIndex {
            definitions,
            type_names,
            function_instances,
            resolutions: Vec::new(),
            by_qualified,
            modules,
            display_names,
            documents: BTreeMap::new(),
        };
        index.collect_documents(program);
        index.collect_resolutions(program, sources);
        index
    }

    /// 光标所在标识符的解析；找不到任何记录时返回 `Unresolved`。
    pub fn resolve(&self, source: SourceId, offset: usize) -> Resolution {
        let mut best: Option<&ResolutionEntry> = None;
        for entry in &self.resolutions {
            if entry.source != source {
                continue;
            }
            if entry.span.start <= offset
                && offset < entry.span.end
                && best.is_none_or(|current| {
                    entry.span.end - entry.span.start < current.span.end - current.span.start
                })
            {
                best = Some(entry);
            }
        }
        best.map(|entry| entry.resolution.clone())
            .unwrap_or(Resolution::Unresolved)
    }

    pub fn definition_of(&self, id: &SymbolId) -> Option<&Definition> {
        match id {
            SymbolId::Def(def) => self.definitions.get(def),
            SymbolId::Instance { def, .. } => self.definitions.get(def),
            SymbolId::Local { .. } | SymbolId::Module { .. } => None,
        }
    }

    /// 用户类型的显示文本；泛型实参递归渲染，不输出 `TypeId(n)`。
    pub fn render_type(&self, ty: &ir::Type) -> String {
        match ty {
            ir::Type::Struct(id) | ir::Type::Enum(id) => match self.type_names.get(id) {
                Some(symbol) => self.render_symbol(symbol),
                None => format!("<type {}>", id.0),
            },
            ir::Type::Ptr { pointee, mutable } => {
                if *mutable {
                    format!("*{}", self.render_type(pointee))
                } else {
                    format!("*const {}", self.render_type(pointee))
                }
            }
            ir::Type::Slice { element, mutable } => {
                if *mutable {
                    format!("[]{}", self.render_type(element))
                } else {
                    format!("[]const {}", self.render_type(element))
                }
            }
            ir::Type::Array { element, length } => format!("[{element}; {length}]"),
            other => other.to_string(),
        }
    }

    pub fn document_symbols(&self, source: SourceId) -> Vec<DocumentSymbol> {
        self.documents.get(&source.0).cloned().unwrap_or_default()
    }

    fn render_symbol(&self, symbol: &SymbolId) -> String {
        match symbol {
            SymbolId::Def(def) => self.display_qualified(&def.qualified),
            SymbolId::Instance { def, type_args } => {
                let args: Vec<String> = type_args.iter().map(|ty| self.render_type(ty)).collect();
                format!(
                    "{}<{}>",
                    self.display_qualified(&def.qualified),
                    args.join(", ")
                )
            }
            SymbolId::Local { .. } => "<local>".to_string(),
            SymbolId::Module { qualified } => self.display_qualified(qualified),
        }
    }

    /// 依赖包前缀 `@<id>.` 替换为包 `name`；根包与 `std` 保持原样。
    fn display_qualified(&self, qualified: &str) -> String {
        display_qualified(qualified, &self.display_names)
    }

    fn collect_documents(&mut self, program: &ast::Program) {
        for function in &program.functions {
            self.push_document(
                function.source_id,
                DocumentSymbol {
                    name: simple_name(&function.name),
                    qualified: function.name.clone(),
                    kind: DefKind::Function,
                    name_span: function.name_span,
                    span: block_span(&function.body, function.name_span),
                },
            );
        }
        for structure in &program.structs {
            self.push_document(
                structure.source_id,
                DocumentSymbol {
                    name: simple_name(&structure.name),
                    qualified: structure.name.clone(),
                    kind: DefKind::Struct,
                    name_span: structure.name_span,
                    span: structure.name_span,
                },
            );
        }
        for enumeration in &program.enums {
            self.push_document(
                enumeration.source_id,
                DocumentSymbol {
                    name: simple_name(&enumeration.name),
                    qualified: enumeration.name.clone(),
                    kind: DefKind::Enum,
                    name_span: enumeration.name_span,
                    span: enumeration.name_span,
                },
            );
        }
        for item in &program.traits {
            self.push_document(
                item.source_id,
                DocumentSymbol {
                    name: simple_name(&item.name),
                    qualified: item.name.clone(),
                    kind: DefKind::Trait,
                    name_span: item.name_span,
                    span: item.name_span,
                },
            );
        }
    }

    fn push_document(&mut self, source_id: usize, symbol: DocumentSymbol) {
        self.documents
            .entry(source_id as u32)
            .or_default()
            .push(symbol);
    }

    fn collect_resolutions(&mut self, program: &ast::Program, sources: &[SourceFile]) {
        for source in sources {
            if source.id == SourceId::ANONYMOUS {
                continue;
            }
            let mut resolver = ScopeResolver {
                source: source.id,
                text: &source.text,
                by_qualified: &self.by_qualified,
                modules: &self.modules,
                scopes: Vec::new(),
                entries: Vec::new(),
            };
            resolver.run(program);
            self.resolutions.extend(resolver.entries);
        }
        self.resolutions.sort_by_key(|entry| {
            (
                entry.source.0,
                entry.span.start,
                entry.span.end,
                resolution_rank(&entry.resolution),
            )
        });
    }
}

/// 作用域解析遍历器。
struct ScopeResolver<'a> {
    source: SourceId,
    text: &'a str,
    by_qualified: &'a HashMap<String, DefId>,
    modules: &'a HashSet<String>,
    scopes: Vec<HashMap<String, Span>>,
    entries: Vec<ResolutionEntry>,
}

impl ScopeResolver<'_> {
    fn run(&mut self, program: &ast::Program) {
        let source_id = self.source.0 as usize;
        for function in &program.functions {
            if function.source_id == source_id {
                self.function(function);
            }
        }
        for item in &program.traits {
            if item.source_id == source_id {
                for method in &item.methods {
                    self.function(method);
                }
            }
        }
        for item in &program.impls {
            if item.source_id == source_id {
                for method in &item.methods {
                    self.function(method);
                }
            }
        }
    }

    fn function(&mut self, function: &ast::Function) {
        self.scopes.push(HashMap::new());
        for parameter in &function.parameters {
            self.declare(&parameter.name, parameter.name_span);
        }
        self.block(&function.body);
        self.scopes.pop();
    }

    fn block(&mut self, statements: &[ast::Statement]) {
        self.scopes.push(HashMap::new());
        for statement in statements {
            self.statement(statement);
        }
        self.scopes.pop();
    }

    fn statement(&mut self, statement: &ast::Statement) {
        match &statement.kind {
            ast::StatementKind::Variable {
                name,
                name_span,
                type_name,
                initializer,
                ..
            } => {
                self.expr(initializer);
                if let Some(ty) = type_name {
                    self.type_ref(ty);
                }
                self.declare(name, *name_span);
            }
            ast::StatementKind::Assignment {
                name,
                name_span,
                value,
                ..
            } => {
                self.expr(value);
                self.name_use(name, *name_span);
            }
            ast::StatementKind::IndexAssignment {
                name,
                name_span,
                index,
                value,
                ..
            } => {
                self.expr(index);
                self.expr(value);
                self.name_use(name, *name_span);
            }
            ast::StatementKind::FieldAssignment {
                name,
                name_span,
                field_span,
                value,
                ..
            } => {
                self.expr(value);
                self.name_use(name, *name_span);
                self.record(*field_span, Resolution::Unresolved);
            }
            ast::StatementKind::Expression(expression) => self.expr(expression),
            ast::StatementKind::If {
                condition,
                then_block,
                else_block,
            } => {
                self.expr(condition);
                self.block(then_block);
                if let Some(block) = else_block {
                    self.block(block);
                }
            }
            ast::StatementKind::Loop(block) => self.block(block),
            ast::StatementKind::While { condition, body } => {
                self.expr(condition);
                self.block(body);
            }
            ast::StatementKind::For {
                name,
                name_span,
                iterable,
                body,
            } => {
                match iterable {
                    ast::ForIterable::Range { start, end, .. } => {
                        self.expr(start);
                        self.expr(end);
                    }
                    ast::ForIterable::Array(array) => self.expr(array),
                }
                self.scopes.push(HashMap::new());
                self.declare(name, *name_span);
                self.block(body);
                self.scopes.pop();
            }
            ast::StatementKind::Break
            | ast::StatementKind::Continue
            | ast::StatementKind::Return(None) => {}
            ast::StatementKind::Defer(expression)
            | ast::StatementKind::Return(Some(expression)) => {
                self.expr(expression);
            }
        }
    }

    fn expr(&mut self, expression: &ast::Expr) {
        match &expression.kind {
            ast::ExprKind::Number(_)
            | ast::ExprKind::Character(_)
            | ast::ExprKind::Boolean(_)
            | ast::ExprKind::String(_) => {}
            ast::ExprKind::Name(name) => self.name_use(name, expression.span),
            ast::ExprKind::Array(values) => {
                for value in values {
                    self.expr(value);
                }
            }
            ast::ExprKind::RepeatArray { value, .. } => self.expr(value),
            ast::ExprKind::Call {
                callee,
                callee_span,
                type_arguments,
                arguments,
            } => {
                for argument in arguments {
                    self.expr(argument);
                }
                for ty in type_arguments {
                    self.type_ref(ty);
                }
                self.callee(callee, *callee_span);
            }
            ast::ExprKind::Index { array, index } => {
                self.expr(array);
                self.expr(index);
            }
            ast::ExprKind::Cast { value, ty } => {
                self.expr(value);
                self.type_ref(ty);
            }
            ast::ExprKind::Unary { operand, .. }
            | ast::ExprKind::AddressOf { operand }
            | ast::ExprKind::Deref { operand } => self.expr(operand),
            ast::ExprKind::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            ast::ExprKind::Field {
                base, field_span, ..
            } => {
                self.expr(base);
                self.record(*field_span, Resolution::Unresolved);
            }
            ast::ExprKind::Match { value, arms } => {
                self.expr(value);
                for arm in arms {
                    self.scopes.push(HashMap::new());
                    self.pattern(&arm.pattern);
                    self.expr(&arm.body);
                    self.scopes.pop();
                }
            }
        }
    }

    fn pattern(&mut self, pattern: &ast::MatchPattern) {
        match pattern {
            ast::MatchPattern::Wildcard => {}
            ast::MatchPattern::Enum {
                name,
                name_span,
                bindings,
                ..
            } => {
                let resolution = match name.rsplit_once('.') {
                    Some((enum_name, _)) => self.def_resolution(enum_name),
                    None => self.def_resolution(name),
                };
                self.record(*name_span, resolution);
                let spans = binding_spans(self.text, *name_span);
                for (binding, span) in bindings.iter().zip(spans) {
                    self.declare(binding, span);
                }
            }
        }
    }

    fn type_ref(&mut self, ty: &ast::TypeRef) {
        match &ty.kind {
            ast::TypeRefKind::Name { name, arguments } => {
                for argument in arguments {
                    self.type_ref(argument);
                }
                let resolution = self.def_resolution(name);
                self.record(ty.span, resolution);
            }
            ast::TypeRefKind::Array { element, .. }
            | ast::TypeRefKind::Ptr {
                pointee: element, ..
            }
            | ast::TypeRefKind::Slice { element, .. } => self.type_ref(element),
        }
    }

    /// 调用目标：局部接收者、限定函数/方法定义、模块路径或未解析。
    fn callee(&mut self, callee: &str, span: Span) {
        let segments = segment_spans(self.text, span);
        if segments.is_empty() {
            return;
        }
        // 方法调用：接收者首段是局部绑定，接收者类型推断不在 M20 范围。
        if segments.len() > 1 && self.lookup(segments[0].0).is_some() {
            self.name_use(segments[0].0, segments[0].1);
            for segment in &segments[1..] {
                self.record(segment.1, Resolution::Unresolved);
            }
            return;
        }
        // 已由 loader 限定的函数/方法/构造调用：整体是定义。
        if let Some(def) = self.by_qualified.get(callee) {
            let last = segments.last().expect("segments is not empty");
            self.record(last.1, Resolution::Def(def.clone()));
            let resolved: Vec<&str> = callee.split('.').collect();
            if resolved.len() == segments.len() {
                for (index, segment) in segments[..segments.len() - 1].iter().enumerate() {
                    let prefix = resolved[..=index].join(".");
                    let resolution = self.def_resolution(&prefix);
                    self.record(segment.1, resolution);
                }
            } else {
                for segment in &segments[..segments.len() - 1] {
                    self.record(segment.1, Resolution::Unresolved);
                }
            }
            return;
        }
        if self.modules.contains(callee) {
            let last = segments.last().expect("segments is not empty");
            self.record(
                last.1,
                Resolution::Module {
                    qualified: callee.to_string(),
                },
            );
            return;
        }
        for segment in segments {
            let resolution = self.def_resolution(segment.0);
            self.record(segment.1, resolution);
        }
    }

    fn def_resolution(&self, qualified: &str) -> Resolution {
        if let Some(def) = self.by_qualified.get(qualified) {
            return Resolution::Def(def.clone());
        }
        if self.modules.contains(qualified) {
            return Resolution::Module {
                qualified: qualified.to_string(),
            };
        }
        Resolution::Unresolved
    }

    fn declare(&mut self, name: &str, span: Span) {
        if name == "_" {
            return;
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), span);
        }
        self.record(
            span,
            Resolution::Local {
                source: self.source,
                name_span: span,
            },
        );
    }

    fn lookup(&self, name: &str) -> Option<Span> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn name_use(&mut self, name: &str, span: Span) {
        let resolution = match self.lookup(name) {
            Some(name_span) => Resolution::Local {
                source: self.source,
                name_span,
            },
            None => Resolution::Unresolved,
        };
        self.record(span, resolution);
    }

    fn record(&mut self, span: Span, resolution: Resolution) {
        if span.end > span.start {
            self.entries.push(ResolutionEntry {
                source: self.source,
                span,
                resolution,
            });
        }
    }
}

fn source_id(index: usize) -> SourceId {
    SourceId(u32::try_from(index).unwrap_or(u32::MAX))
}

fn map_kind(kind: DefinitionKind) -> DefKind {
    match kind {
        DefinitionKind::Function => DefKind::Function,
        DefinitionKind::Struct => DefKind::Struct,
        DefinitionKind::Enum => DefKind::Enum,
        DefinitionKind::Trait => DefKind::Trait,
        DefinitionKind::Method => DefKind::Method,
        DefinitionKind::Field => DefKind::Field,
        DefinitionKind::Variant => DefKind::Variant,
        DefinitionKind::TypeParam => DefKind::TypeParam,
    }
}

fn symbol_for_key(key: &GenericKey) -> SymbolId {
    let def = DefId {
        package: key.package,
        qualified: key.name.clone(),
    };
    if key.args.is_empty() {
        SymbolId::Def(def)
    } else {
        SymbolId::Instance {
            def,
            type_args: key.args.clone(),
        }
    }
}

fn simple_name(qualified: &str) -> String {
    qualified
        .rsplit('.')
        .next()
        .unwrap_or(qualified)
        .to_string()
}

fn block_span(block: &[ast::Statement], fallback: Span) -> Span {
    block
        .iter()
        .map(|statement| statement.span)
        .reduce(Span::merge)
        .unwrap_or(fallback)
}

fn display_qualified(qualified: &str, display: &HashMap<PackageId, String>) -> String {
    if let Some(rest) = qualified.strip_prefix('@')
        && let Some((id, rest)) = rest.split_once('.')
        && let Ok(id) = id.parse::<u32>()
        && let Some(name) = display.get(&PackageId(id))
    {
        return format!("{name}.{rest}");
    }
    qualified.to_string()
}

/// 源文本中 `callee_span` 内按 `.` 切分的段与 span；ASCII `.` 保证字符边界安全。
fn segment_spans(text: &str, span: Span) -> Vec<(&str, Span)> {
    if span.start >= span.end || span.end > text.len() {
        return Vec::new();
    }
    if !text.is_char_boundary(span.start) || !text.is_char_boundary(span.end) {
        return Vec::new();
    }
    let slice = &text[span.start..span.end];
    let mut segments = Vec::new();
    let mut start = 0usize;
    for (index, byte) in slice.bytes().enumerate() {
        if byte == b'.' {
            if index > start {
                segments.push((
                    &slice[start..index],
                    Span::new(span.start + start, span.start + index),
                ));
            }
            start = index + 1;
        }
    }
    if start < slice.len() {
        segments.push((
            &slice[start..],
            Span::new(span.start + start, span.start + slice.len()),
        ));
    }
    segments
}

/// 在模式首标识符之后扫描绑定标识符的 span；扫描受限，找不到时返回空列表。
fn binding_spans(text: &str, pattern_span: Span) -> Vec<Span> {
    let mut spans = Vec::new();
    if pattern_span.end >= text.len() || !text.is_char_boundary(pattern_span.end) {
        return spans;
    }
    let rest = &text[pattern_span.end..];
    let Some(open) = rest.find('(') else {
        return spans;
    };
    let before = &rest[..open];
    if before.contains("=>") || before.contains('{') || before.contains(';') {
        return spans;
    }
    let bytes = rest.as_bytes();
    let mut index = open + 1;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() || byte == b',' {
            index += 1;
            continue;
        }
        if byte == b')' {
            break;
        }
        if byte.is_ascii_alphanumeric() || byte == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            spans.push(Span::new(
                pattern_span.end + start,
                pattern_span.end + index,
            ));
            continue;
        }
        // 其他字符（例如类型实参残留）：跳到下一个分隔符。
        while index < bytes.len() && bytes[index] != b',' && bytes[index] != b')' {
            index += 1;
        }
    }
    spans
}

fn resolution_rank(resolution: &Resolution) -> u8 {
    match resolution {
        Resolution::Local { .. } => 0,
        Resolution::Def(_) => 1,
        Resolution::Instance { .. } => 2,
        Resolution::Module { .. } => 3,
        Resolution::Unresolved => 4,
    }
}

/// 签名冻结格式（M20 §6.6 规则 5）。
fn signature_for(
    program: &ast::Program,
    data: &DefinitionData,
    kind: DefKind,
    display: &HashMap<PackageId, String>,
) -> String {
    let name = display_qualified(&data.qualified, display);
    match kind {
        DefKind::Function => program
            .functions
            .iter()
            .find(|function| function.name == data.qualified)
            .map(|function| function_signature(function, &name, display))
            .unwrap_or_else(|| format!("fn {name}()")),
        DefKind::Method => {
            for item in &program.impls {
                for method in &item.methods {
                    let key = format!("{}::{}", item.type_name, method.name);
                    if key == data.qualified {
                        let owner = display_qualified(&item.type_name, display);
                        return function_signature(
                            method,
                            &format!("{owner}.{}", method.name),
                            display,
                        );
                    }
                }
            }
            for item in &program.traits {
                for method in &item.methods {
                    let key = format!("{}::{}", item.name, method.name);
                    if key == data.qualified {
                        let owner = display_qualified(&item.name, display);
                        return function_signature(
                            method,
                            &format!("{owner}.{}", method.name),
                            display,
                        );
                    }
                }
            }
            format!("fn {name}()")
        }
        DefKind::Struct => format!("struct {name}"),
        DefKind::Enum => format!("enum {name}"),
        DefKind::Trait => format!("trait {name}"),
        DefKind::Field => {
            let field = simple_name(&data.qualified);
            for structure in &program.structs {
                for declaration in &structure.fields {
                    if declaration.name == field && structure.name == owner_of(&data.qualified) {
                        return format!("{field}: {}", render_type_ref(&declaration.ty, display));
                    }
                }
            }
            field
        }
        DefKind::Variant => {
            let variant = simple_name(&data.qualified);
            for enumeration in &program.enums {
                for declaration in &enumeration.variants {
                    if declaration.name == variant && enumeration.name == owner_of(&data.qualified)
                    {
                        let types: Vec<String> = declaration
                            .fields
                            .iter()
                            .map(|ty| render_type_ref(ty, display))
                            .collect();
                        return format!(
                            "{}.{variant}({})",
                            display_qualified(&enumeration.name, display),
                            types.join(", ")
                        );
                    }
                }
            }
            variant
        }
        DefKind::TypeParam => simple_name(&data.qualified),
    }
}

fn owner_of(qualified: &str) -> &str {
    qualified
        .rsplit_once('.')
        .map(|(owner, _)| owner)
        .unwrap_or("")
}

fn function_signature(
    function: &ast::Function,
    display_name: &str,
    display: &HashMap<PackageId, String>,
) -> String {
    let parameters: Vec<String> = function
        .parameters
        .iter()
        .map(|parameter| render_type_ref(&parameter.ty, display))
        .collect();
    let mut signature = format!("fn {display_name}({})", parameters.join(", "));
    if let Some(return_type) = &function.return_type {
        let rendered = render_type_ref(return_type, display);
        if rendered != "Unit" {
            signature.push_str(&format!(" -> {rendered}"));
        }
    }
    signature
}

fn render_type_ref(ty: &ast::TypeRef, display: &HashMap<PackageId, String>) -> String {
    match &ty.kind {
        ast::TypeRefKind::Name { name, arguments } => {
            let mut text = display_qualified(name, display);
            if !arguments.is_empty() {
                let args: Vec<String> = arguments
                    .iter()
                    .map(|argument| render_type_ref(argument, display))
                    .collect();
                text.push('<');
                text.push_str(&args.join(", "));
                text.push('>');
            }
            text
        }
        ast::TypeRefKind::Array { element, length } => {
            format!("[{}; {length}]", render_type_ref(element, display))
        }
        ast::TypeRefKind::Ptr { pointee, mutable } => {
            if *mutable {
                format!("*{}", render_type_ref(pointee, display))
            } else {
                format!("*const {}", render_type_ref(pointee, display))
            }
        }
        ast::TypeRefKind::Slice { element, mutable } => {
            if *mutable {
                format!("[]{}", render_type_ref(element, display))
            } else {
                format!("[]const {}", render_type_ref(element, display))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use dolphin_hir::lower;
    use dolphin_hir::modules::{self, PackageSources};
    use dolphin_package::package::PackageId;

    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "dolphin-analysis-index-{tag}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn build_index(tag: &str, text: &str) -> (SymbolIndex, SourceFile) {
        let root = temp_dir(tag);
        fs::write(root.join("main.do"), text).expect("write source");
        let packages = vec![PackageSources {
            id: PackageId::ROOT,
            prefix: String::new(),
            aliases: BTreeMap::new(),
            source_root: root.clone(),
            exclude: HashSet::new(),
            extra: Vec::new(),
        }];
        let loaded = modules::load_packages_collecting(&packages);
        assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
        let program = loaded.program.as_ref().expect("merged program");
        let lowered = lower::lower_sources_analysis_collecting(
            &loaded.sources,
            program,
            &loaded.packages,
            true,
        )
        .expect("lowering");
        let index = SymbolIndex::new(&lowered.analysis, program, &loaded.sources, None);
        let source = loaded.sources.into_iter().next().expect("user source file");
        fs::remove_dir_all(root).unwrap();
        (index, source)
    }

    fn offset_of(text: &str, needle: &str) -> usize {
        text.find(needle)
            .unwrap_or_else(|| panic!("missing `{needle}`"))
    }

    fn local(source: SourceId, start: usize, length: usize) -> Resolution {
        Resolution::Local {
            source,
            name_span: Span::new(start, start + length),
        }
    }

    #[test]
    fn resolves_parameters_locals_for_vars_and_shadowing() {
        let text = "\
fn helper(value: i32): i32 { return value; }
fn main() {
    val x = 1;
    val y = x;
    if true {
        val x = helper(1);
        val z = x;
    }
    for i in 0..2 {
        val w = i;
    }
    return x;
}
";
        let (index, source) = build_index("scope", text);
        let outer_x = offset_of(text, "val x = 1;") + 4;
        let inner_x = offset_of(text, "val x = helper(1);") + 4;
        let for_i = offset_of(text, "for i in") + 4;
        let y_use = offset_of(text, "val y = x;") + 8;
        let z_use = offset_of(text, "val z = x;") + 8;
        let w_use = offset_of(text, "val w = i;") + 8;
        assert_eq!(
            index.resolve(source.id, y_use),
            local(source.id, outer_x, 1)
        );
        assert_eq!(
            index.resolve(source.id, z_use),
            local(source.id, inner_x, 1)
        );
        assert_eq!(index.resolve(source.id, w_use), local(source.id, for_i, 1));
        // 参数 `value` 在函数体内解析为局部绑定。
        let value_use = offset_of(text, "return value;") + 7;
        let value_param = offset_of(text, "value: i32");
        assert_eq!(
            index.resolve(source.id, value_use),
            local(source.id, value_param, "value".len())
        );
        // 已限定函数调用解析为定义。
        let callee = offset_of(text, "helper(1)");
        assert_eq!(
            index.resolve(source.id, callee),
            Resolution::Def(DefId {
                package: PackageId::ROOT,
                qualified: "helper".to_string(),
            })
        );
    }

    #[test]
    fn match_bindings_resolve_to_declaration_spans() {
        let text = "\
fn main() {
    val maybe: Option<i32> = Option.Some(7);
    val copy = match maybe {
        Option.Some(inner) => inner,
        Option.None => 0,
    };
    return copy;
}
";
        let (index, source) = build_index("match", text);
        let binding = offset_of(text, "Option.Some(inner)") + "Option.Some(".len();
        let use_offset = offset_of(text, "=> inner,") + 3;
        assert_eq!(
            index.resolve(source.id, use_offset),
            local(source.id, binding, "inner".len())
        );
        // 模式名解析到枚举 variant 定义。
        let pattern = offset_of(text, "Option.Some(inner)");
        assert_eq!(
            index.resolve(source.id, pattern),
            Resolution::Def(DefId {
                package: PackageId::STD,
                qualified: "std.Option".to_string(),
            })
        );
    }

    #[test]
    fn definitions_signatures_documents_and_instances() {
        let text = "\
struct Point { x: i32, y: i32 }
struct Pair<T> { first: T, second: T }
fn helper(value: i32): i32 { return value; }
fn main() {
    val p = Point(1, 2);
    val pair = Pair<i32>(p.x, p.y);
    return helper(pair.first);
}
";
        let (index, source) = build_index("definitions", text);

        let point = DefId {
            package: PackageId::ROOT,
            qualified: "Point".to_string(),
        };
        let definition = index.definition_of(&SymbolId::Def(point.clone())).unwrap();
        assert_eq!(definition.kind, DefKind::Struct);
        assert_eq!(definition.signature, "struct Point");
        assert_eq!(definition.source, source.id);
        assert_eq!(
            &source.text[definition.name_span.start..definition.name_span.end],
            "Point"
        );

        let helper = DefId {
            package: PackageId::ROOT,
            qualified: "helper".to_string(),
        };
        assert_eq!(
            index
                .definition_of(&SymbolId::Def(helper.clone()))
                .unwrap()
                .signature,
            "fn helper(i32) -> i32"
        );
        assert_eq!(
            index
                .definition_of(&SymbolId::Def(DefId {
                    package: PackageId::ROOT,
                    qualified: "Point.x".to_string(),
                }))
                .unwrap()
                .signature,
            "x: i32"
        );

        let symbols = index.document_symbols(source.id);
        let names: Vec<&str> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
        assert_eq!(names, vec!["helper", "main", "Point", "Pair"]);

        // 泛型实例：`Pair<i32>` 的 TypeId 渲染为带实参的显示名。
        let (pair_id, symbol) = index
            .type_names
            .iter()
            .find(|(_, symbol)| match symbol {
                SymbolId::Instance { def, .. } => def.qualified == "Pair",
                _ => false,
            })
            .expect("Pair<i32> instance");
        assert_eq!(
            symbol,
            &SymbolId::Instance {
                def: DefId {
                    package: PackageId::ROOT,
                    qualified: "Pair".to_string(),
                },
                type_args: vec![ir::Type::I32],
            }
        );
        assert_eq!(index.render_type(&ir::Type::Struct(*pair_id)), "Pair<i32>");
    }

    #[test]
    fn unresolved_names_stay_unresolved() {
        // 语义错误程序无法 lowering；用合法程序验证未知字段访问为 Unresolved。
        let text = "struct Point { x: i32 }\nfn main() { val p = Point(1); return p.x; }\n";
        let (index, source) = build_index("unresolved", text);
        let field = offset_of(text, "p.x;") + 2;
        assert_eq!(index.resolve(source.id, field), Resolution::Unresolved);
        assert_eq!(
            index.resolve(source.id, offset_of(text, "fn main")),
            Resolution::Unresolved
        );
    }
}
