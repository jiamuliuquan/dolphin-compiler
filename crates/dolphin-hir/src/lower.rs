use std::cell::RefCell;
use std::collections::HashMap;

use crate::monomorphize::{
    EnumTemplate, FunctionTemplate, GenericKey, InstanceStatus, MethodTemplate, MonoContext,
    MonoState, Signature, StructTemplate, TemplateTables, TraitImplTemplate, TraitTemplate,
    TypeEnv,
};
use dolphin_ir::ir::{
    self, BasicBlock, BlockId, EnumVariant, Expr, FunctionId, Instruction, LocalId, Location,
    PrintPart, StructField, Terminator, Type, TypeDef, TypeId,
};
use dolphin_ir::layout;
use dolphin_package::package::{PackageId, module_of};
use dolphin_source::diagnostic::{DIAGNOSTIC_LIMIT, Diagnostic, push_capped};
use dolphin_source::source::{SourceFile, Span};
use dolphin_syntax::ast::{
    self, AssignmentOperator, BinaryOperator, ExprKind, ForIterable, Statement, StatementKind,
    TypeRefKind, UnaryOperator,
};

pub fn lower(source: &SourceFile, program: &ast::Program) -> Result<ir::Program, Diagnostic> {
    let loaded = crate::modules::inject_stdlib(source, program)?;
    lower_sources(&loaded.sources, &loaded.program, &loaded.packages)
}

pub fn lower_sources(
    sources: &[SourceFile],
    program: &ast::Program,
    packages: &[PackageId],
) -> Result<ir::Program, Diagnostic> {
    Ok(ProgramLowerer::new(sources, program, packages)?
        .lower(true)?
        .program)
}

/// 库构建：与 `lower_sources` 相同，但不要求 `main`，也不生成入口。
pub fn lower_library(
    sources: &[SourceFile],
    program: &ast::Program,
    packages: &[PackageId],
) -> Result<ir::Program, Diagnostic> {
    Ok(ProgramLowerer::new(sources, program, packages)?
        .lower(false)?
        .program)
}

/// 分析路径 lowering（M20/H20-02）：与 `lower_sources` 相同，但附带 side table。
pub fn lower_sources_analysis(
    sources: &[SourceFile],
    program: &ast::Program,
    packages: &[PackageId],
) -> Result<LoweredProgram, Diagnostic> {
    ProgramLowerer::new(sources, program, packages)?.lower(true)
}

/// 收集式 lowering（M20/H20-01，H20-02 升级返回 side table）：与
/// `lower_sources_analysis` 相同，但声明级错误全部收集；函数体 lowering 仍首错即停。
pub fn lower_sources_analysis_collecting(
    sources: &[SourceFile],
    program: &ast::Program,
    packages: &[PackageId],
    require_main: bool,
) -> Result<LoweredProgram, Vec<Diagnostic>> {
    ProgramLowerer::new_collecting(sources, program, packages)?
        .lower(require_main)
        .map_err(|diagnostic| vec![diagnostic])
}

/// 单文件收集式 lowering：注入内建标准库后走收集式声明校验。
pub fn lower_collecting(
    source: &SourceFile,
    program: &ast::Program,
) -> Result<ir::Program, Vec<Diagnostic>> {
    let loaded =
        crate::modules::inject_stdlib(source, program).map_err(|diagnostic| vec![diagnostic])?;
    lower_sources_analysis_collecting(&loaded.sources, &loaded.program, &loaded.packages, true)
        .map(|lowered| lowered.program)
}

/// 定义种类（M20/H20-02 side table）；`dolphin-analysis` 映射为公开的 `DefKind`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefinitionKind {
    Function,
    Struct,
    Enum,
    Trait,
    Method,
    Field,
    Variant,
    TypeParam,
}

/// lowering side table 中的一条定义（M20/H20-02）。
#[derive(Clone, Debug)]
pub struct DefinitionData {
    pub package: PackageId,
    pub qualified: String,
    pub kind: DefinitionKind,
    pub source_id: usize,
    pub name_span: Span,
}

/// lowering side table（M20/H20-02）：定义、类型实例名与函数实例名。
///
/// `type_names` 的下标是 `TypeId`，`function_instances` 的下标是 `FunctionId`。
#[derive(Debug, Default)]
pub struct AnalysisData {
    pub definitions: Vec<DefinitionData>,
    pub type_names: Vec<GenericKey>,
    pub function_instances: Vec<GenericKey>,
}

/// 带 side table 的 lowering 结果（M20/H20-02）。
#[derive(Debug)]
pub struct LoweredProgram {
    pub program: ir::Program,
    pub analysis: AnalysisData,
}

struct ProgramLowerer<'a> {
    sources: &'a [SourceFile],
    ast: &'a ast::Program,
    tables: TemplateTables,
    definitions: Vec<DefinitionData>,
    state: RefCell<MonoState>,
}

impl<'a> ProgramLowerer<'a> {
    fn new(
        sources: &'a [SourceFile],
        ast: &'a ast::Program,
        packages: &'a [PackageId],
    ) -> Result<Self, Diagnostic> {
        let (tables, definitions, mut diagnostics) =
            build_templates_collecting(sources, ast, packages);
        if !diagnostics.is_empty() {
            return Err(diagnostics.remove(0));
        }
        Ok(Self {
            sources,
            ast,
            tables,
            definitions,
            state: RefCell::new(MonoState::default()),
        })
    }

    /// 收集式构造：声明级诊断非空时直接返回，不进入实例化/函数体 lowering。
    fn new_collecting(
        sources: &'a [SourceFile],
        ast: &'a ast::Program,
        packages: &'a [PackageId],
    ) -> Result<Self, Vec<Diagnostic>> {
        let (tables, definitions, diagnostics) = build_templates_collecting(sources, ast, packages);
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        Ok(Self {
            sources,
            ast,
            tables,
            definitions,
            state: RefCell::new(MonoState::default()),
        })
    }

    fn context(&self) -> MonoContext<'_> {
        MonoContext {
            sources: self.sources,
            tables: &self.tables,
        }
    }

    fn lower(mut self, require_main: bool) -> Result<LoweredProgram, Diagnostic> {
        // 1. 先实例化非泛型类型，保持 M13/M14 的既有布局与 ABI。
        {
            let ctx = self.context();
            let mut state = self.state.borrow_mut();
            for structure in &self.ast.structs {
                if !structure.type_params.is_empty() {
                    continue;
                }
                state.instantiate_named(
                    &ctx,
                    &self.sources[structure.source_id],
                    &structure.name,
                    &[],
                    Vec::new(),
                    structure.name_span,
                )?;
            }
            for enumeration in &self.ast.enums {
                if !enumeration.type_params.is_empty() {
                    continue;
                }
                state.instantiate_named(
                    &ctx,
                    &self.sources[enumeration.source_id],
                    &enumeration.name,
                    &[],
                    Vec::new(),
                    enumeration.name_span,
                )?;
            }
        }
        self.state.borrow().check_layout_cycles()?;

        // 2. 登记所有具体（非泛型）函数实例；泛型实例由调用点按需登记。
        {
            let ctx = self.context();
            let mut state = self.state.borrow_mut();
            for function in &self.ast.functions {
                if !function.type_params.is_empty() {
                    continue;
                }
                let id = state.instantiate_function(
                    &ctx,
                    &self.sources[function.source_id],
                    &function.name,
                    Vec::new(),
                    function.name_span,
                    0,
                )?;
                if require_main && function.name == "main" {
                    if state.instances[id.0].return_type != Type::I32 {
                        return Err(Diagnostic::at(
                            &self.sources[function.source_id],
                            function.name_span,
                            "`main` must return `i32` or omit its return type",
                        ));
                    }
                    state.main = Some(id);
                }
            }
        }

        // 3. 工作队列：展开实例、发现新请求，直到不动点。
        while let Some(id) = self.next_pending() {
            self.lower_instance(id)?;
        }

        let state = self.state.into_inner();
        if require_main && state.main.is_none() {
            return Err(Diagnostic::plain("program does not define `main`"));
        }
        let definitions = std::mem::take(&mut self.definitions);
        let analysis = AnalysisData {
            definitions,
            type_names: state.type_keys.clone(),
            function_instances: state.instance_keys.clone(),
        };
        let main = state.main;
        let mut functions = Vec::with_capacity(state.lowered.len());
        for (index, slot) in state.lowered.into_iter().enumerate() {
            functions.push(slot.ok_or_else(|| {
                Diagnostic::plain(format!(
                    "internal error: function instance {index} was not lowered"
                ))
            })?);
        }
        let program = ir::Program {
            functions,
            main,
            types: state.types,
            sources: self
                .sources
                .iter()
                .map(|source| source.path.clone())
                .collect(),
        };
        dolphin_ir::verify::verify_program(&program).map_err(|error| {
            Diagnostic::plain(format!("internal IR verification failed: {error}"))
        })?;
        Ok(LoweredProgram { program, analysis })
    }

    fn next_pending(&self) -> Option<FunctionId> {
        let mut state = self.state.borrow_mut();
        let id = state.pending.pop()?;
        state.instances[id.0].status = InstanceStatus::InProgress;
        Some(id)
    }

    fn lower_instance(&mut self, id: FunctionId) -> Result<(), Diagnostic> {
        let (name, module, source_id, signature, env, depth, extern_c, link_name) = {
            let mut state = self.state.borrow_mut();
            state.instances[id.0].status = InstanceStatus::InProgress;
            let info = &state.instances[id.0];
            (
                info.name.clone(),
                info.module.clone(),
                info.source_id,
                state.instance_signature(id),
                info.env.clone(),
                info.depth,
                info.extern_c,
                info.link_name.clone(),
            )
        };
        let function_ast = self
            .tables
            .function_ast(&name)
            .expect("registered instance has a template");
        let source = &self.sources[source_id];
        let (line, column) = source.line_column(function_ast.span.start);
        let location = Location {
            file: source_id as u32,
            line: line as u32,
            column: column as u32,
        };
        if extern_c {
            let function = ir::Function {
                id,
                name: name.clone(),
                parameters: (0..signature.parameters.len()).map(LocalId).collect(),
                return_type: signature.return_type.clone(),
                locals: signature.parameters.clone(),
                blocks: Vec::new(),
                entry: BlockId(0),
                external_link_name: link_name,
                source: source_id as u32,
                location,
            };
            let mut state = self.state.borrow_mut();
            state.lowered[id.0] = Some(function);
            state.instances[id.0].status = InstanceStatus::Done;
            return Ok(());
        }
        let function = FunctionLowerer::new(
            self.sources,
            source,
            function_ast,
            signature,
            &self.tables,
            &self.state,
            env,
            depth,
            module,
        )
        .lower()?;
        let mut state = self.state.borrow_mut();
        state.lowered[id.0] = Some(function);
        state.instances[id.0].status = InstanceStatus::Done;
        Ok(())
    }
}

/// 记录一条定义到分析 side table（M20/H20-02）。
fn push_definition(
    definitions: &mut Vec<DefinitionData>,
    package: PackageId,
    qualified: String,
    kind: DefinitionKind,
    source_id: usize,
    name_span: Span,
) {
    definitions.push(DefinitionData {
        package,
        qualified,
        kind,
        source_id,
        name_span,
    });
}

/// 收集所有声明为模板：泛型/普通结构体、枚举与函数。
///
/// M20/H20-01 收集式版本：失败声明报一条诊断并从模板表跳过，后续独立声明继续
/// 校验；每个顶层声明至多贡献一条声明级诊断，达到 100 条后追加 `E0002` 并停止。
/// M20/H20-02 起同时为成功声明收集分析 side table 的定义。
fn build_templates_collecting(
    sources: &[SourceFile],
    ast: &ast::Program,
    packages: &[PackageId],
) -> (TemplateTables, Vec<DefinitionData>, Vec<Diagnostic>) {
    let package_of_source = |source_id: usize| -> PackageId {
        packages.get(source_id).copied().unwrap_or(PackageId::ROOT)
    };
    let mut tables = TemplateTables::default();
    let mut definitions: Vec<DefinitionData> = Vec::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    for structure in &ast.structs {
        if diagnostics.len() >= DIAGNOSTIC_LIMIT {
            return (tables, definitions, diagnostics);
        }
        let source = &sources[structure.source_id];
        let type_params = match type_param_names(&structure.type_params, source) {
            Ok(names) => names,
            Err(diagnostic) => {
                push_capped(&mut diagnostics, diagnostic);
                continue;
            }
        };
        if tables.structs.contains_key(&structure.name)
            || tables.enums.contains_key(&structure.name)
        {
            push_capped(
                &mut diagnostics,
                Diagnostic::at(
                    source,
                    structure.name_span,
                    format!("type `{}` is already defined", structure.name),
                ),
            );
            continue;
        }
        let package = package_of_source(structure.source_id);
        tables.structs.insert(
            structure.name.clone(),
            StructTemplate {
                package,
                source_id: structure.source_id,
                name_span: structure.name_span,
                type_params,
                bounds: type_param_bounds(&structure.type_params),
                fields: structure.fields.clone(),
                extern_c: structure.extern_c,
            },
        );
        for param in &structure.type_params {
            push_definition(
                &mut definitions,
                package,
                format!("{}.{}", structure.name, param.name),
                DefinitionKind::TypeParam,
                structure.source_id,
                param.name_span,
            );
        }
        push_definition(
            &mut definitions,
            package,
            structure.name.clone(),
            DefinitionKind::Struct,
            structure.source_id,
            structure.name_span,
        );
        for field in &structure.fields {
            push_definition(
                &mut definitions,
                package,
                format!("{}.{}", structure.name, field.name),
                DefinitionKind::Field,
                structure.source_id,
                field.name_span,
            );
        }
    }
    for enumeration in &ast.enums {
        if diagnostics.len() >= DIAGNOSTIC_LIMIT {
            return (tables, definitions, diagnostics);
        }
        let source = &sources[enumeration.source_id];
        let type_params = match type_param_names(&enumeration.type_params, source) {
            Ok(names) => names,
            Err(diagnostic) => {
                push_capped(&mut diagnostics, diagnostic);
                continue;
            }
        };
        if tables.structs.contains_key(&enumeration.name)
            || tables.enums.contains_key(&enumeration.name)
        {
            push_capped(
                &mut diagnostics,
                Diagnostic::at(
                    source,
                    enumeration.name_span,
                    format!("type `{}` is already defined", enumeration.name),
                ),
            );
            continue;
        }
        let package = package_of_source(enumeration.source_id);
        tables.enums.insert(
            enumeration.name.clone(),
            EnumTemplate {
                package,
                source_id: enumeration.source_id,
                name_span: enumeration.name_span,
                type_params,
                bounds: type_param_bounds(&enumeration.type_params),
                variants: enumeration.variants.clone(),
            },
        );
        for param in &enumeration.type_params {
            push_definition(
                &mut definitions,
                package,
                format!("{}.{}", enumeration.name, param.name),
                DefinitionKind::TypeParam,
                enumeration.source_id,
                param.name_span,
            );
        }
        push_definition(
            &mut definitions,
            package,
            enumeration.name.clone(),
            DefinitionKind::Enum,
            enumeration.source_id,
            enumeration.name_span,
        );
        for variant in &enumeration.variants {
            push_definition(
                &mut definitions,
                package,
                format!("{}.{}", enumeration.name, variant.name),
                DefinitionKind::Variant,
                enumeration.source_id,
                variant.name_span,
            );
        }
    }
    for function in &ast.functions {
        if diagnostics.len() >= DIAGNOSTIC_LIMIT {
            return (tables, definitions, diagnostics);
        }
        let source = &sources[function.source_id];
        if matches!(function.name.as_str(), "print" | "println" | "length") {
            push_capped(
                &mut diagnostics,
                Diagnostic::at(
                    source,
                    function.name_span,
                    format!("`{}` is a reserved built-in function", function.name),
                ),
            );
            continue;
        }
        if function.name == "main" {
            if !function.type_params.is_empty() {
                push_capped(
                    &mut diagnostics,
                    Diagnostic::at(
                        source,
                        function.name_span,
                        "`main` cannot have type parameters",
                    ),
                );
                continue;
            }
            if !function.parameters.is_empty() {
                push_capped(
                    &mut diagnostics,
                    Diagnostic::at(
                        source,
                        function.name_span,
                        "`main` cannot have parameters yet",
                    ),
                );
                continue;
            }
        }
        if let Err(diagnostic) = type_param_names(&function.type_params, source) {
            push_capped(&mut diagnostics, diagnostic);
            continue;
        }
        if tables.functions.contains_key(&function.name) {
            push_capped(
                &mut diagnostics,
                Diagnostic::at(
                    source,
                    function.name_span,
                    format!("function `{}` is already defined", function.name),
                ),
            );
            continue;
        }
        let bounds = type_param_bounds(&function.type_params);
        let package = package_of_source(function.source_id);
        tables.functions.insert(
            function.name.clone(),
            FunctionTemplate {
                package,
                module: module_of(&function.name).to_string(),
                source_id: function.source_id,
                function: function.clone(),
                bounds,
            },
        );
        for param in &function.type_params {
            push_definition(
                &mut definitions,
                package,
                format!("{}.{}", function.name, param.name),
                DefinitionKind::TypeParam,
                function.source_id,
                param.name_span,
            );
        }
        push_definition(
            &mut definitions,
            package,
            function.name.clone(),
            DefinitionKind::Function,
            function.source_id,
            function.name_span,
        );
    }

    // trait 声明。
    for item in &ast.traits {
        if diagnostics.len() >= DIAGNOSTIC_LIMIT {
            return (tables, definitions, diagnostics);
        }
        let source = &sources[item.source_id];
        if let Err(diagnostic) = type_param_names(&item.type_params, source) {
            push_capped(&mut diagnostics, diagnostic);
            continue;
        }
        if tables.traits.contains_key(&item.name)
            || tables.structs.contains_key(&item.name)
            || tables.enums.contains_key(&item.name)
        {
            push_capped(
                &mut diagnostics,
                Diagnostic::at(
                    source,
                    item.name_span,
                    format!("trait `{}` is already defined", item.name),
                ),
            );
            continue;
        }
        let mut associated_types = Vec::new();
        let mut duplicate_associated = None;
        for associated in &item.associated_types {
            if associated_types.contains(&associated.name) {
                duplicate_associated = Some(Diagnostic::at(
                    source,
                    associated.name_span,
                    format!("associated type `{}` is already declared", associated.name),
                ));
                break;
            }
            associated_types.push(associated.name.clone());
        }
        if let Some(diagnostic) = duplicate_associated {
            push_capped(&mut diagnostics, diagnostic);
            continue;
        }
        let package = package_of_source(item.source_id);
        tables.traits.insert(
            item.name.clone(),
            TraitTemplate {
                associated_types,
                methods: item.methods.clone(),
            },
        );
        for param in &item.type_params {
            push_definition(
                &mut definitions,
                package,
                format!("{}.{}", item.name, param.name),
                DefinitionKind::TypeParam,
                item.source_id,
                param.name_span,
            );
        }
        push_definition(
            &mut definitions,
            package,
            item.name.clone(),
            DefinitionKind::Trait,
            item.source_id,
            item.name_span,
        );
        for method in &item.methods {
            let method_key = format!("{}::{}", item.name, method.name);
            for param in &method.type_params {
                push_definition(
                    &mut definitions,
                    package,
                    format!("{method_key}.{}", param.name),
                    DefinitionKind::TypeParam,
                    method.source_id,
                    param.name_span,
                );
            }
            push_definition(
                &mut definitions,
                package,
                method_key,
                DefinitionKind::Method,
                method.source_id,
                method.name_span,
            );
        }
    }

    // impl 块。
    'impls: for item in &ast.impls {
        if diagnostics.len() >= DIAGNOSTIC_LIMIT {
            return (tables, definitions, diagnostics);
        }
        let source = &sources[item.source_id];
        if let Some(diagnostic) = validate_impl_header(&tables, source, item) {
            push_capped(&mut diagnostics, diagnostic);
            continue;
        }
        let type_params = match type_param_names(&item.type_params, source) {
            Ok(names) => names,
            Err(diagnostic) => {
                push_capped(&mut diagnostics, diagnostic);
                continue;
            }
        };
        if let Some(trait_name) = &item.trait_name {
            let Some(trait_template) = tables
                .traits
                .get(trait_name)
                .map(|template| (template.associated_types.clone(), template.methods.clone()))
            else {
                push_capped(
                    &mut diagnostics,
                    Diagnostic::at(
                        source,
                        item.trait_span,
                        format!("unknown trait `{trait_name}`"),
                    ),
                );
                continue;
            };
            let (trait_associated, trait_methods) = trait_template;
            if tables
                .trait_impls
                .contains_key(&(trait_name.clone(), item.type_name.clone()))
            {
                push_capped(
                    &mut diagnostics,
                    Diagnostic::at(
                        source,
                        item.trait_span,
                        format!(
                            "`{}` already implements trait `{trait_name}`",
                            item.type_name
                        ),
                    ),
                );
                continue;
            }
            for name in &trait_associated {
                if !item.associated_types.iter().any(|b| b.name == *name) {
                    push_capped(
                        &mut diagnostics,
                        Diagnostic::at(
                            source,
                            item.type_span,
                            format!("impl of `{trait_name}` is missing associated type `{name}`"),
                        ),
                    );
                    continue 'impls;
                }
            }
            for trait_method in &trait_methods {
                let Some(implementation) = item
                    .methods
                    .iter()
                    .find(|method| method.name == trait_method.name)
                else {
                    push_capped(
                        &mut diagnostics,
                        Diagnostic::at(
                            source,
                            item.type_span,
                            format!(
                                "impl of `{trait_name}` is missing method `{}`",
                                trait_method.name
                            ),
                        ),
                    );
                    continue 'impls;
                };
                let bindings: HashMap<String, ast::TypeRef> = item
                    .associated_types
                    .iter()
                    .map(|binding| (binding.name.clone(), binding.ty.clone()))
                    .collect();
                if !trait_signature_matches(
                    trait_method,
                    implementation,
                    &item.type_name,
                    &bindings,
                ) {
                    push_capped(
                        &mut diagnostics,
                        Diagnostic::at(
                            source,
                            implementation.name_span,
                            format!(
                                "method `{}` does not match the `{trait_name}` declaration",
                                implementation.name
                            ),
                        ),
                    );
                    continue 'impls;
                }
            }
            for binding in &item.associated_types {
                if !trait_associated.contains(&binding.name) {
                    push_capped(
                        &mut diagnostics,
                        Diagnostic::at(
                            source,
                            binding.name_span,
                            format!(
                                "`{}` is not an associated type of `{trait_name}`",
                                binding.name
                            ),
                        ),
                    );
                    continue 'impls;
                }
            }
            for method in &item.methods {
                if !trait_methods.iter().any(|m| m.name == method.name) {
                    push_capped(
                        &mut diagnostics,
                        Diagnostic::at(
                            source,
                            method.name_span,
                            format!("`{}` is not a method of `{trait_name}`", method.name),
                        ),
                    );
                    continue 'impls;
                }
            }
            tables.trait_impls.insert(
                (trait_name.clone(), item.type_name.clone()),
                TraitImplTemplate {
                    type_params: type_params.clone(),
                    associated_types: item
                        .associated_types
                        .iter()
                        .map(|binding| (binding.name.clone(), binding.ty.clone()))
                        .collect(),
                },
            );
        }
        let mut seen_associated = std::collections::HashSet::new();
        let mut duplicate_binding = None;
        for binding in &item.associated_types {
            if !seen_associated.insert(binding.name.clone()) {
                duplicate_binding = Some(Diagnostic::at(
                    source,
                    binding.name_span,
                    format!("associated type `{}` is bound more than once", binding.name),
                ));
                break;
            }
        }
        if let Some(diagnostic) = duplicate_binding {
            push_capped(&mut diagnostics, diagnostic);
            continue;
        }
        let package = package_of_source(item.source_id);
        for param in &item.type_params {
            push_definition(
                &mut definitions,
                package,
                format!("{}::{}", item.type_name, param.name),
                DefinitionKind::TypeParam,
                item.source_id,
                param.name_span,
            );
        }
        for method in &item.methods {
            let key = format!("{}::{}", item.type_name, method.name);
            if let Some(existing) = tables.methods.get(&key) {
                let owner = match &existing.trait_name {
                    Some(trait_name) => format!("trait `{trait_name}`"),
                    None => "an inherent impl".to_string(),
                };
                push_capped(
                    &mut diagnostics,
                    Diagnostic::at(
                        source,
                        method.name_span,
                        format!("method `{key}` is already defined by {owner}"),
                    ),
                );
                continue 'impls;
            }
            for param in &method.type_params {
                push_definition(
                    &mut definitions,
                    package,
                    format!("{key}.{}", param.name),
                    DefinitionKind::TypeParam,
                    method.source_id,
                    param.name_span,
                );
            }
            push_definition(
                &mut definitions,
                package,
                key.clone(),
                DefinitionKind::Method,
                method.source_id,
                method.name_span,
            );
            tables.methods.insert(
                key,
                MethodTemplate {
                    package,
                    module: item.module.clone(),
                    type_name: item.type_name.clone(),
                    trait_name: item.trait_name.clone(),
                    type_params: type_params.clone(),
                    associated_types: item
                        .associated_types
                        .iter()
                        .map(|binding| (binding.name.clone(), binding.ty.clone()))
                        .collect(),
                    function: method.clone(),
                },
            );
        }
    }
    (tables, definitions, diagnostics)
}

/// H18-05：在声明处校验 impl 头。
///
/// 当前支持子集：具名 struct/enum 的固有或静态 trait impl；泛型目标时目标实参
/// 必须是 impl 参数按位置一一对应（允许改名）。其余形式（特化、重排、重复、
/// 嵌套、漏参/多参、blanket impl、泛型 trait 实参、impl 参数 bound、方法独立
/// 泛型参数）即便方法从未调用也在声明处拒绝。
fn validate_impl_header(
    tables: &TemplateTables,
    source: &SourceFile,
    item: &ast::ImplBlock,
) -> Option<Diagnostic> {
    for param in &item.type_params {
        if param.bound.is_some() {
            return Some(Diagnostic::at(
                source,
                param.name_span,
                "bounds on impl type parameters are not supported yet; declare the bound on the type instead",
            ));
        }
    }
    for method in &item.methods {
        if let Some(param) = method.type_params.first() {
            return Some(Diagnostic::at(
                source,
                param.name_span,
                format!(
                    "method `{}` cannot declare its own type parameters yet",
                    method.name
                ),
            ));
        }
    }
    if let Some(argument) = item.trait_arguments.first() {
        return Some(Diagnostic::at(
            source,
            argument.span,
            "generic trait arguments are not supported yet",
        ));
    }
    if item
        .type_params
        .iter()
        .any(|param| param.name == item.type_name)
    {
        return Some(Diagnostic::at(
            source,
            item.type_span,
            "blanket impl is not supported; the impl target must be a named struct or enum",
        ));
    }
    let declaration_params = tables
        .structs
        .get(&item.type_name)
        .map(|template| template.type_params.clone())
        .or_else(|| {
            tables
                .enums
                .get(&item.type_name)
                .map(|template| template.type_params.clone())
        });
    let Some(declaration_params) = declaration_params else {
        return Some(Diagnostic::at(
            source,
            item.type_span,
            format!("unknown type `{}`", item.type_name),
        ));
    };
    if item.type_params.is_empty() && !item.type_arguments.is_empty() {
        return Some(Diagnostic::at(
            source,
            item.type_span,
            "concrete impl targets for generic types are not supported; declare impl type parameters and use them as target arguments",
        ));
    }
    if item.type_params.len() != declaration_params.len() {
        return Some(Diagnostic::at(
            source,
            item.type_span,
            format!(
                "impl for `{}` declares {} type parameters but `{}` expects {}",
                item.type_name,
                item.type_params.len(),
                item.type_name,
                declaration_params.len()
            ),
        ));
    }
    if item.type_arguments.len() != declaration_params.len() {
        return Some(Diagnostic::at(
            source,
            item.type_span,
            format!(
                "`{}` expects {} type arguments but {} were provided",
                item.type_name,
                declaration_params.len(),
                item.type_arguments.len()
            ),
        ));
    }
    for (index, argument) in item.type_arguments.iter().enumerate() {
        let matches = matches!(
            &argument.kind,
            ast::TypeRefKind::Name { name, arguments }
                if arguments.is_empty() && name == &item.type_params[index].name
        );
        if !matches {
            return Some(Diagnostic::at(
                source,
                argument.span,
                "impl target must use the impl type parameters in order; specialization, reordering, repeated and nested arguments are not supported",
            ));
        }
    }
    None
}

fn type_param_names(
    params: &[ast::TypeParamDecl],
    source: &SourceFile,
) -> Result<Vec<String>, Diagnostic> {
    let mut names = Vec::new();
    for param in params {
        if names.contains(&param.name) {
            return Err(Diagnostic::at(
                source,
                param.name_span,
                format!("type parameter `{}` is already declared", param.name),
            ));
        }
        names.push(param.name.clone());
    }
    Ok(names)
}

/// 收集类型参数的单 trait 约束（限定名）；验证器保证每个参数最多一个。
fn type_param_bounds(params: &[ast::TypeParamDecl]) -> Vec<(String, String)> {
    params
        .iter()
        .filter_map(|param| {
            param
                .bound
                .as_ref()
                .map(|bound| (param.name.clone(), bound.clone()))
        })
        .collect()
}

/// 比较 trait 方法声明与 impl 方法实现：接收者、参数数量和规范化后的类型须一致。
///
/// 规范化会把 `Self` 替换为实现类型、把 `Self::Item` 替换为关联类型绑定，
/// 因此 `fn next(self: *Self): Option<Self::Item>` 与 `fn next(self: *Self): Option<i32>`
/// 在 `type Item = i32;` 下视为一致。
fn trait_signature_matches(
    trait_method: &ast::Function,
    impl_method: &ast::Function,
    type_name: &str,
    bindings: &HashMap<String, ast::TypeRef>,
) -> bool {
    if trait_method.parameters.len() != impl_method.parameters.len() {
        return false;
    }
    for (expected, actual) in trait_method.parameters.iter().zip(&impl_method.parameters) {
        if expected.receiver != actual.receiver {
            return false;
        }
        if canonical_type_ref(&expected.ty, type_name, bindings)
            != canonical_type_ref(&actual.ty, type_name, bindings)
        {
            return false;
        }
    }
    match (&trait_method.return_type, &impl_method.return_type) {
        (None, None) => true,
        (Some(expected), Some(actual)) => {
            canonical_type_ref(expected, type_name, bindings)
                == canonical_type_ref(actual, type_name, bindings)
        }
        _ => false,
    }
}

fn canonical_type_ref(
    ty: &ast::TypeRef,
    type_name: &str,
    bindings: &HashMap<String, ast::TypeRef>,
) -> String {
    match &ty.kind {
        TypeRefKind::Name { name, arguments } => {
            if name == "Self" && arguments.is_empty() {
                return type_name.to_string();
            }
            if let Some(associated) = name.strip_prefix("Self::") {
                if let Some(bound) = bindings.get(associated) {
                    return canonical_type_ref(bound, type_name, bindings);
                }
                return format!("?Self::{associated}");
            }
            if arguments.is_empty() {
                return name.clone();
            }
            let arguments: Vec<String> = arguments
                .iter()
                .map(|argument| canonical_type_ref(argument, type_name, bindings))
                .collect();
            format!("{name}<{}>", arguments.join(","))
        }
        TypeRefKind::Ptr { pointee, mutable } => format!(
            "*{}{}",
            if *mutable { "" } else { "const " },
            canonical_type_ref(pointee, type_name, bindings)
        ),
        TypeRefKind::Slice { element, mutable } => format!(
            "[]{}{}",
            if *mutable { "" } else { "const " },
            canonical_type_ref(element, type_name, bindings)
        ),
        TypeRefKind::Array { element, length } => format!(
            "[{};{}]",
            canonical_type_ref(element, type_name, bindings),
            length
        ),
    }
}

/// 方法接收者形态。
#[derive(Clone, Copy)]
enum ReceiverKind {
    Value,
    Ptr { mutable: bool },
}

#[derive(Clone)]
struct Binding {
    local: LocalId,
    ty: Type,
    mutable: bool,
}

/// 一个词法作用域：绑定与按注册顺序记录的 `defer` 清理调用。
///
/// `defer` 调用在注册时 lowering（绑定到当时的局部变量身份），在作用域退出时
/// 逆序发出，因此实参读取的是退出时的最新值（M14-D）。
#[derive(Default)]
struct Scope {
    bindings: HashMap<String, Binding>,
    defers: Vec<Expr>,
}

#[derive(Clone, Copy)]
struct LoopTargets {
    break_block: BlockId,
    continue_block: BlockId,
    /// `break`/`continue` 需要清理的作用域起点（循环体作用域下标）。
    cleanup_scope: usize,
}

struct WorkingBlock {
    instructions: Vec<Instruction>,
    terminator: Option<Terminator>,
    reachable: bool,
    location: Location,
    locations: Vec<Location>,
    terminator_location: Location,
}

struct FunctionLowerer<'a> {
    sources: &'a [SourceFile],
    source: &'a SourceFile,
    function: &'a ast::Function,
    signature: Signature,
    /// 当前实例的类型参数环境（模板类型参数名 -> 具体类型）。
    env: TypeEnv,
    /// 当前实例的泛型链深度（新实例 +1）。
    depth: usize,
    /// 当前实例所属模块（字段可见性判定）。
    module: String,
    tables: &'a TemplateTables,
    state: &'a RefCell<MonoState>,
    parameters: Vec<LocalId>,
    locals: Vec<Type>,
    scopes: Vec<Scope>,
    blocks: Vec<WorkingBlock>,
    current: BlockId,
    loops: Vec<LoopTargets>,
    current_location: Location,
}

impl<'a> FunctionLowerer<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        sources: &'a [SourceFile],
        source: &'a SourceFile,
        function: &'a ast::Function,
        signature: Signature,
        tables: &'a TemplateTables,
        state: &'a RefCell<MonoState>,
        env: TypeEnv,
        depth: usize,
        module: String,
    ) -> Self {
        let (line, column) = source.line_column(function.span.start);
        let location = Location {
            file: function.source_id as u32,
            line: line as u32,
            column: column as u32,
        };
        Self {
            sources,
            source,
            function,
            signature,
            env,
            depth,
            module,
            tables,
            state,
            parameters: Vec::new(),
            locals: Vec::new(),
            scopes: vec![Scope::default()],
            blocks: vec![WorkingBlock {
                instructions: Vec::new(),
                terminator: None,
                reachable: true,
                location,
                locations: Vec::new(),
                terminator_location: location,
            }],
            current: BlockId(0),
            loops: Vec::new(),
            current_location: location,
        }
    }

    /// 由 span 计算 1 基行列位置。
    fn location_of(&self, span: Span) -> Location {
        let (line, column) = self.source.line_column(span.start);
        Location {
            file: self.function.source_id as u32,
            line: line as u32,
            column: column as u32,
        }
    }

    fn context(&self) -> MonoContext<'_> {
        MonoContext {
            sources: self.sources,
            tables: self.tables,
        }
    }

    /// 解析类型引用（可能触发泛型类型实例化）。
    fn resolve(&self, typeref: &ast::TypeRef) -> Result<Type, Diagnostic> {
        let ctx = self.context();
        self.state
            .borrow_mut()
            .resolve_type(&ctx, self.source, typeref, &self.env)
    }

    fn struct_fields(&self, id: TypeId) -> Vec<StructField> {
        let state = self.state.borrow();
        state
            .struct_fields(id)
            .iter()
            .map(|field| StructField {
                name: field.name.clone(),
                ty: field.ty.clone(),
                public: field.public,
            })
            .collect()
    }

    /// 字段可见性：默认模块私有，跨模块访问（读/写/位置构造）须 `pub`。
    fn check_field_access(
        &self,
        id: TypeId,
        field_index: usize,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let state = self.state.borrow();
        let owner = module_of(&state.type_keys[id.0].name).to_string();
        let field = &state.struct_fields(id)[field_index];
        if owner != self.module && !field.public {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!("field `{}` is private", field.name),
            ));
        }
        Ok(())
    }

    fn enum_variants(&self, id: TypeId) -> Vec<EnumVariant> {
        let state = self.state.borrow();
        state
            .enum_variants(id)
            .iter()
            .map(|variant| EnumVariant {
                name: variant.name.clone(),
                fields: variant.fields.clone(),
            })
            .collect()
    }

    fn type_is_enum(&self, id: TypeId) -> bool {
        self.state.borrow().is_enum(id)
    }

    fn concrete_type_id(&self, name: &str) -> Option<TypeId> {
        self.state.borrow().concrete_type_id(self.tables, name)
    }

    /// 错误消息中的类型显示：用户类型用可读限定名与泛型实参（M20/H20-01）。
    fn display_type(&self, ty: &Type) -> String {
        self.state.borrow().display_type(ty)
    }

    fn signature_of(&self, id: FunctionId) -> Signature {
        self.state.borrow().instance_signature(id)
    }

    /// 登记（或复用）一个函数实例。
    fn instantiate(
        &self,
        name: &str,
        args: Vec<Type>,
        span: Span,
        depth: usize,
    ) -> Result<FunctionId, Diagnostic> {
        let ctx = self.context();
        self.state
            .borrow_mut()
            .instantiate_function(&ctx, self.source, name, args, span, depth)
    }

    fn lower(mut self) -> Result<ir::Function, Diagnostic> {
        for (parameter, ty) in self
            .function
            .parameters
            .iter()
            .zip(self.signature.parameters.iter().cloned())
        {
            if self.scopes[0].bindings.contains_key(&parameter.name) {
                return Err(Diagnostic::at(
                    self.source,
                    parameter.name_span,
                    format!("parameter `{}` is already declared", parameter.name),
                ));
            }
            let local = LocalId(self.locals.len());
            self.locals.push(ty.clone());
            self.parameters.push(local);
            self.scopes[0].bindings.insert(
                parameter.name.clone(),
                Binding {
                    local,
                    ty,
                    // 参数槽可寻址：`self: *Self` 接收者的方法调用与 `&param` 需要它。
                    // 参数在语言中没有 `val`/`var` 限定，首版按可写局部处理（M15-B/R12）。
                    mutable: true,
                },
            );
        }

        self.lower_statements(&self.function.body)?;
        if !self.is_terminated(self.current) {
            // 函数体自然结束：先执行仍在作用域内的 defer，再隐式返回。
            self.emit_cleanups_from(0);
            let reachable = self.blocks[self.current.0].reachable;
            match self.signature.return_type.clone() {
                Type::Unit => self.terminate(Terminator::Return(None)),
                Type::I32 if self.function.name == "main" => {
                    self.terminate(Terminator::Return(Some(Expr::i32(0))))
                }
                _ if reachable => {
                    return Err(Diagnostic::at(
                        self.source,
                        self.function.span,
                        format!(
                            "function `{}` may exit without returning `{}`",
                            self.function.name, self.signature.return_type
                        ),
                    ));
                }
                ty => self.terminate(Terminator::Return(Some(self.default_expr(ty)))),
            }
        }

        let location = self.location_of(self.function.span);
        let blocks = self
            .blocks
            .into_iter()
            .map(|block| BasicBlock {
                instructions: block.instructions,
                terminator: block
                    .terminator
                    .expect("all IR blocks must have a terminator"),
                location: block.location,
                locations: block.locations,
                terminator_location: block.terminator_location,
            })
            .collect();
        Ok(ir::Function {
            id: self.signature.id,
            name: self.function.name.clone(),
            parameters: self.parameters,
            return_type: self.signature.return_type,
            locals: self.locals,
            blocks,
            entry: BlockId(0),
            external_link_name: None,
            source: self.function.source_id as u32,
            location,
        })
    }

    fn lower_statements(&mut self, statements: &[Statement]) -> Result<(), Diagnostic> {
        for statement in statements {
            if self.is_terminated(self.current) || !self.blocks[self.current.0].reachable {
                return Err(Diagnostic::at(
                    self.source,
                    statement.span,
                    "unreachable statement",
                ));
            }
            self.lower_statement(statement)?;
        }
        Ok(())
    }

    fn lower_statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        self.current_location = self.location_of(statement.span);
        match &statement.kind {
            StatementKind::Variable {
                mutable,
                name,
                name_span,
                type_name,
                initializer,
            } => {
                let declared = match type_name {
                    Some(ty) => Some(self.resolve(ty)?),
                    None => None,
                };
                let mut value = self.lower_expr_with_expected(initializer, declared.as_ref())?;
                if value.ty == Type::Unit {
                    return Err(Diagnostic::at(
                        self.source,
                        initializer.span,
                        "cannot store a value of type `Unit`",
                    ));
                }
                let ty = match declared {
                    Some(declared) => {
                        self.require_type(&value.ty, &declared, initializer.span)?;
                        // null 字面量与只读限定转换：以声明类型为准。
                        value.ty = declared.clone();
                        declared
                    }
                    None => {
                        if is_null_ptr(&value.ty) {
                            return Err(Diagnostic::at(
                                self.source,
                                initializer.span,
                                "cannot infer the type of `null`; annotate a pointer type",
                            ));
                        }
                        value.ty.clone()
                    }
                };
                if self.scopes.last().unwrap().bindings.contains_key(name) {
                    return Err(Diagnostic::at(
                        self.source,
                        *name_span,
                        format!("`{name}` is already declared in this scope"),
                    ));
                }
                let local = LocalId(self.locals.len());
                self.locals.push(ty.clone());
                self.scopes.last_mut().unwrap().bindings.insert(
                    name.clone(),
                    Binding {
                        local,
                        ty,
                        mutable: *mutable,
                    },
                );
                self.emit(Instruction::SetLocal { local, value });
            }
            StatementKind::Assignment {
                name,
                name_span,
                operator,
                value,
            } => self.lower_assignment(name, *name_span, *operator, value)?,
            StatementKind::IndexAssignment {
                name,
                name_span,
                index,
                operator,
                value,
            } => self.lower_index_assignment(name, *name_span, index, *operator, value)?,
            StatementKind::FieldAssignment {
                name,
                name_span,
                field,
                field_span,
                deref,
                operator,
                value,
            } => self.lower_field_assignment(
                name,
                *name_span,
                field,
                *field_span,
                *deref,
                *operator,
                value,
            )?,
            StatementKind::Expression(expression) => self.lower_expression_statement(expression)?,
            StatementKind::If {
                condition,
                then_block,
                else_block,
            } => self.lower_if(condition, then_block, else_block.as_deref())?,
            StatementKind::Loop(body) => self.lower_loop(body)?,
            StatementKind::While { condition, body } => self.lower_while(condition, body)?,
            StatementKind::For {
                name,
                name_span,
                iterable,
                body,
            } => self.lower_for(name, *name_span, iterable, body)?,
            StatementKind::Break => {
                let targets = self.loops.last().copied().ok_or_else(|| {
                    Diagnostic::at(self.source, statement.span, "`break` used outside a loop")
                })?;
                // 只清理退出到目标循环经过的作用域（循环体及其内层），
                // 不清理循环之外仍然存活的 defer。
                self.emit_cleanups_from(targets.cleanup_scope);
                self.terminate(Terminator::Jump(targets.break_block));
            }
            StatementKind::Continue => {
                let targets = self.loops.last().copied().ok_or_else(|| {
                    Diagnostic::at(
                        self.source,
                        statement.span,
                        "`continue` used outside a loop",
                    )
                })?;
                self.emit_cleanups_from(targets.cleanup_scope);
                self.terminate(Terminator::Jump(targets.continue_block));
            }
            StatementKind::Defer(call) => self.lower_defer(call, statement.span)?,
            StatementKind::Return(value) => self.lower_return(value.as_ref(), statement.span)?,
        }
        Ok(())
    }

    /// 登记一个 `defer` 清理调用（M14-D）。
    ///
    /// 只接受返回 `Unit` 的普通函数 / intrinsic 调用；在注册处 lowering，
    /// 绑定当时的局部变量身份，退出时读取最新值。
    fn lower_defer(&mut self, call: &ast::Expr, span: Span) -> Result<(), Diagnostic> {
        if !matches!(&call.kind, ExprKind::Call { .. }) {
            return Err(Diagnostic::at(
                self.source,
                span,
                "`defer` expects a call expression",
            ));
        }
        let lowered = self.lower_expr(call)?;
        if lowered.ty != Type::Unit {
            return Err(Diagnostic::at(
                self.source,
                span,
                "`defer` call must return `Unit`",
            ));
        }
        self.scopes.last_mut().unwrap().defers.push(lowered);
        Ok(())
    }

    fn lower_assignment(
        &mut self,
        name: &str,
        name_span: Span,
        operator: AssignmentOperator,
        value: &ast::Expr,
    ) -> Result<(), Diagnostic> {
        let binding = self.lookup(name, name_span)?.clone();
        if !binding.mutable {
            return Err(Diagnostic::at(
                self.source,
                name_span,
                format!("cannot assign to immutable variable `{name}`"),
            ));
        }
        let right = self.lower_expr(value)?;
        let assigned = match operator {
            AssignmentOperator::Assign => {
                self.require_type(&right.ty, &binding.ty, value.span)?;
                // 只读限定转换：以绑定类型为准，保证存储类型一致。
                let mut right = right;
                right.ty = binding.ty.clone();
                right
            }
            operator => {
                if !(binding.ty.is_integer() || binding.ty.is_float()) {
                    return Err(Diagnostic::at(
                        self.source,
                        name_span,
                        "compound assignment requires a numeric value",
                    ));
                }
                self.require_type(&right.ty, &binding.ty, value.span)?;
                Expr {
                    kind: ir::ExprKind::Binary {
                        operator: assignment_binary(operator),
                        left: Box::new(Expr {
                            kind: ir::ExprKind::Local(binding.local),
                            ty: binding.ty.clone(),
                        }),
                        right: Box::new(right),
                    },
                    ty: binding.ty,
                }
            }
        };
        self.emit(Instruction::SetLocal {
            local: binding.local,
            value: assigned,
        });
        Ok(())
    }

    fn lower_index_assignment(
        &mut self,
        name: &str,
        name_span: Span,
        index: &ast::Expr,
        operator: AssignmentOperator,
        value: &ast::Expr,
    ) -> Result<(), Diagnostic> {
        let binding = self.lookup(name, name_span)?.clone();
        match binding.ty.clone() {
            Type::Array { element, length } => {
                if !binding.mutable {
                    return Err(Diagnostic::at(
                        self.source,
                        name_span,
                        format!("cannot modify immutable array `{name}`"),
                    ));
                }
                check_constant_index(self.source, index, length)?;
                let index = self.lower_expr(index)?;
                self.require_type(&index.ty, &Type::I32, name_span)?;
                let index_local = self.store_temporary(index);
                let right = self.lower_expr(value)?;
                let element_type = element.as_type();
                let assigned = match operator {
                    AssignmentOperator::Assign => {
                        self.require_type(&right.ty, &element_type, value.span)?;
                        right
                    }
                    operator => {
                        if !(element_type.is_integer() || element_type.is_float()) {
                            return Err(Diagnostic::at(
                                self.source,
                                name_span,
                                "compound assignment requires a numeric element",
                            ));
                        }
                        self.require_type(&right.ty, &element_type, value.span)?;
                        Expr {
                            kind: ir::ExprKind::Binary {
                                operator: assignment_binary(operator),
                                left: Box::new(Expr {
                                    kind: ir::ExprKind::Index {
                                        array: Box::new(Expr {
                                            kind: ir::ExprKind::Local(binding.local),
                                            ty: binding.ty.clone(),
                                        }),
                                        index: Box::new(Expr {
                                            kind: ir::ExprKind::Local(index_local),
                                            ty: Type::I32,
                                        }),
                                    },
                                    ty: element_type.clone(),
                                }),
                                right: Box::new(right),
                            },
                            ty: element_type.clone(),
                        }
                    }
                };
                let base = ir::Place {
                    kind: ir::PlaceKind::Local(binding.local),
                    ty: binding.ty.clone(),
                    mutable: true,
                };
                let place = ir::Place {
                    kind: ir::PlaceKind::Index {
                        base: Box::new(base),
                        index: Box::new(Expr {
                            kind: ir::ExprKind::Local(index_local),
                            ty: Type::I32,
                        }),
                    },
                    ty: element_type,
                    mutable: true,
                };
                self.emit(Instruction::SetIndexAt {
                    place: Box::new(place),
                    value: assigned,
                });
            }
            Type::Slice { element, mutable } => {
                if !mutable {
                    return Err(Diagnostic::at(
                        self.source,
                        name_span,
                        format!("cannot write through `[]const` slice `{name}`"),
                    ));
                }
                let index_expr = self.lower_usize_value(index, "slice index")?;
                let index_local = self.store_temporary(index_expr);
                let right = self.lower_expr(value)?;
                let element_type = *element;
                let assigned = match operator {
                    AssignmentOperator::Assign => {
                        self.require_type(&right.ty, &element_type, value.span)?;
                        right
                    }
                    operator => {
                        if !(element_type.is_integer() || element_type.is_float()) {
                            return Err(Diagnostic::at(
                                self.source,
                                name_span,
                                "compound assignment requires a numeric element",
                            ));
                        }
                        self.require_type(&right.ty, &element_type, value.span)?;
                        Expr {
                            kind: ir::ExprKind::Binary {
                                operator: assignment_binary(operator),
                                left: Box::new(Expr {
                                    kind: ir::ExprKind::Index {
                                        array: Box::new(local_expr(
                                            binding.local,
                                            binding.ty.clone(),
                                        )),
                                        index: Box::new(local_expr(index_local, Type::Usize)),
                                    },
                                    ty: element_type.clone(),
                                }),
                                right: Box::new(right),
                            },
                            ty: element_type.clone(),
                        }
                    }
                };
                let base = ir::Place {
                    kind: ir::PlaceKind::Local(binding.local),
                    ty: binding.ty.clone(),
                    mutable: true,
                };
                let place = ir::Place {
                    kind: ir::PlaceKind::Index {
                        base: Box::new(base),
                        index: Box::new(local_expr(index_local, Type::Usize)),
                    },
                    ty: element_type,
                    mutable: true,
                };
                self.emit(Instruction::SetIndexAt {
                    place: Box::new(place),
                    value: assigned,
                });
            }
            other => {
                return Err(Diagnostic::at(
                    self.source,
                    name_span,
                    format!("cannot index value of type `{}`", self.display_type(&other)),
                ));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_field_assignment(
        &mut self,
        name: &str,
        name_span: Span,
        field: &str,
        field_span: Span,
        deref: bool,
        operator: AssignmentOperator,
        value: &ast::Expr,
    ) -> Result<(), Diagnostic> {
        let binding = self.lookup(name, name_span)?.clone();
        // 指针字段写 `p->f = ...`：p 必须是可写指针，通过解引用写入。
        let struct_id = if deref {
            let Type::Ptr { pointee, mutable } = &binding.ty else {
                return Err(Diagnostic::at(
                    self.source,
                    name_span,
                    format!("cannot use `->` on value of type `{}`", binding.ty),
                ));
            };
            if !*mutable {
                return Err(Diagnostic::at(
                    self.source,
                    name_span,
                    format!("cannot write through `*const` pointer `{name}`"),
                ));
            }
            let Type::Struct(id) = **pointee else {
                return Err(Diagnostic::at(
                    self.source,
                    name_span,
                    format!(
                        "cannot access field on pointee type `{}`",
                        self.display_type(pointee)
                    ),
                ));
            };
            id
        } else {
            if !binding.mutable {
                return Err(Diagnostic::at(
                    self.source,
                    name_span,
                    format!("cannot assign to immutable variable `{name}`"),
                ));
            }
            let Type::Struct(id) = binding.ty.clone() else {
                return Err(Diagnostic::at(
                    self.source,
                    name_span,
                    format!("cannot access field on value of type `{}`", binding.ty),
                ));
            };
            id
        };
        let fields = self.struct_fields(struct_id);
        let field_index = fields
            .iter()
            .position(|field_def| field_def.name == field)
            .ok_or_else(|| {
                Diagnostic::at(
                    self.source,
                    field_span,
                    format!("struct has no field named `{field}`"),
                )
            })?;
        self.check_field_access(struct_id, field_index, field_span)?;
        let field_type = fields[field_index].ty.clone();
        let right = self.lower_expr(value)?;
        let assigned = match operator {
            AssignmentOperator::Assign => {
                self.require_type(&right.ty, &field_type, value.span)?;
                right
            }
            operator => {
                if !(field_type.is_integer() || field_type.is_float()) {
                    return Err(Diagnostic::at(
                        self.source,
                        field_span,
                        "compound assignment requires a numeric field",
                    ));
                }
                self.require_type(&right.ty, &field_type, value.span)?;
                // `p->field op= value`：左值需先解引用，再取结构体字段。
                let base = if deref {
                    Expr {
                        kind: ir::ExprKind::Deref {
                            pointer: Box::new(local_expr(binding.local, binding.ty.clone())),
                        },
                        ty: Type::Struct(struct_id),
                    }
                } else {
                    local_expr(binding.local, binding.ty.clone())
                };
                Expr {
                    kind: ir::ExprKind::Binary {
                        operator: assignment_binary(operator),
                        left: Box::new(Expr {
                            kind: ir::ExprKind::Field {
                                base: Box::new(base),
                                field: field_index,
                            },
                            ty: field_type.clone(),
                        }),
                        right: Box::new(right),
                    },
                    ty: field_type,
                }
            }
        };
        // 无论目标是局部对象还是指针解引用，都先按 Place 求出字段地址，
        // 再求 RHS，最后只写入字段本身（不整值写回，保留 RHS 对别名的修改）。
        let place = if deref {
            ir::Place {
                kind: ir::PlaceKind::Deref {
                    pointer: Box::new(local_expr(binding.local, binding.ty.clone())),
                },
                ty: Type::Struct(struct_id),
                mutable: true,
            }
        } else {
            ir::Place {
                kind: ir::PlaceKind::Local(binding.local),
                ty: Type::Struct(struct_id),
                mutable: true,
            }
        };
        self.emit(Instruction::SetFieldAt {
            place: Box::new(place),
            field: field_index,
            value: assigned,
        });
        Ok(())
    }

    fn lower_expression_statement(&mut self, expression: &ast::Expr) -> Result<(), Diagnostic> {
        let ExprKind::Call {
            callee, arguments, ..
        } = &expression.kind
        else {
            return Err(Diagnostic::at(
                self.source,
                expression.span,
                "only function calls can be used as expression statements",
            ));
        };
        if matches!(callee.as_str(), "print" | "println") {
            let parts = self.lower_print(arguments, callee == "println", expression.span)?;
            self.emit(Instruction::Print(parts));
        } else {
            let value = self.lower_expr(expression)?;
            self.emit(Instruction::Evaluate(value));
        }
        Ok(())
    }

    fn lower_print(
        &mut self,
        arguments: &[ast::Expr],
        newline: bool,
        span: Span,
    ) -> Result<Vec<PrintPart>, Diagnostic> {
        if arguments.is_empty() {
            return if newline {
                Ok(vec![PrintPart::Text("\n".to_string())])
            } else {
                Ok(Vec::new())
            };
        }
        let ExprKind::String(format) = &arguments[0].kind else {
            return Err(Diagnostic::at(
                self.source,
                arguments[0].span,
                "the first print argument must be a string literal",
            ));
        };
        let format_parts = parse_format(self.source, format, span)?;
        let placeholder_count = format_parts
            .iter()
            .filter(|part| matches!(part, FormatPart::Placeholder))
            .count();
        if placeholder_count != arguments.len() - 1 {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "format string has {placeholder_count} placeholders but {} values were provided",
                    arguments.len() - 1
                ),
            ));
        }

        let mut values = arguments[1..].iter();
        let mut parts = Vec::new();
        for part in format_parts {
            match part {
                FormatPart::Text(text) if !text.is_empty() => parts.push(PrintPart::Text(text)),
                FormatPart::Text(_) => {}
                FormatPart::Placeholder => {
                    let value = self.lower_expr(values.next().unwrap())?;
                    if !is_printable(&value.ty) {
                        return Err(Diagnostic::at(
                            self.source,
                            span,
                            format!("cannot format `{}`", value.ty),
                        ));
                    }
                    parts.push(PrintPart::Value(value));
                }
            }
        }
        if newline {
            parts.push(PrintPart::Text("\n".to_string()));
        }
        Ok(parts)
    }

    fn lower_return(&mut self, value: Option<&ast::Expr>, span: Span) -> Result<(), Diagnostic> {
        let value = match (self.signature.return_type.clone(), value) {
            (Type::I32, None) if self.function.name == "main" => Some(Expr::i32(0)),
            (Type::Unit, None) => None,
            (Type::Unit, Some(_)) => {
                return Err(Diagnostic::at(
                    self.source,
                    span,
                    "a function without a return type cannot return a value",
                ));
            }
            (expected, Some(value)) => {
                let value = self.lower_expr_with_expected(value, Some(&expected))?;
                self.require_type(&value.ty, &expected, span)?;
                Some(value)
            }
            (expected, None) => {
                return Err(Diagnostic::at(
                    self.source,
                    span,
                    format!(
                        "expected a return value of type `{}`",
                        self.display_type(&expected)
                    ),
                ));
            }
        };
        // `return expr` 先求值并保存返回值，再从内到外清理，最后返回。
        // 快照到临时局部，避免清理释放了返回值引用的存储。
        let value = match value {
            Some(expr) if self.has_defers() => {
                let ty = expr.ty.clone();
                let local = self.store_temporary(expr);
                Some(local_expr(local, ty))
            }
            other => other,
        };
        self.emit_cleanups_from(0);
        self.terminate(Terminator::Return(value));
        Ok(())
    }

    fn lower_if(
        &mut self,
        condition: &ast::Expr,
        then_statements: &[Statement],
        else_statements: Option<&[Statement]>,
    ) -> Result<(), Diagnostic> {
        let condition_value = self.lower_expr(condition)?;
        self.require_type(&condition_value.ty, &Type::Bool, condition.span)?;
        let then_block = self.new_block();
        let else_block = self.new_block();
        let merge_block = self.new_block();
        self.terminate(Terminator::Branch {
            condition: condition_value,
            then_block,
            else_block,
        });

        self.current = then_block;
        self.with_scope(|lowerer| lowerer.lower_statements(then_statements))?;
        if !self.is_terminated(self.current) {
            self.terminate(Terminator::Jump(merge_block));
        }

        self.current = else_block;
        if let Some(statements) = else_statements {
            self.with_scope(|lowerer| lowerer.lower_statements(statements))?;
        }
        if !self.is_terminated(self.current) {
            self.terminate(Terminator::Jump(merge_block));
        }
        self.current = merge_block;
        Ok(())
    }

    fn lower_loop(&mut self, statements: &[Statement]) -> Result<(), Diagnostic> {
        let body = self.new_block();
        let exit = self.new_block();
        self.terminate(Terminator::Jump(body));
        self.current = body;
        self.loops.push(LoopTargets {
            break_block: exit,
            continue_block: body,
            cleanup_scope: self.scopes.len(),
        });
        let result = self.with_scope(|lowerer| lowerer.lower_statements(statements));
        self.loops.pop();
        result?;
        if !self.is_terminated(self.current) {
            self.terminate(Terminator::Jump(body));
        }
        self.current = exit;
        Ok(())
    }

    fn lower_while(
        &mut self,
        condition: &ast::Expr,
        statements: &[Statement],
    ) -> Result<(), Diagnostic> {
        let condition_block = self.new_block();
        let body = self.new_block();
        let exit = self.new_block();
        self.terminate(Terminator::Jump(condition_block));

        self.current = condition_block;
        let value = self.lower_expr(condition)?;
        self.require_type(&value.ty, &Type::Bool, condition.span)?;
        self.terminate(Terminator::Branch {
            condition: value,
            then_block: body,
            else_block: exit,
        });

        self.current = body;
        self.loops.push(LoopTargets {
            break_block: exit,
            continue_block: condition_block,
            cleanup_scope: self.scopes.len(),
        });
        let result = self.with_scope(|lowerer| lowerer.lower_statements(statements));
        self.loops.pop();
        result?;
        if !self.is_terminated(self.current) {
            self.terminate(Terminator::Jump(condition_block));
        }
        self.current = exit;
        Ok(())
    }

    fn lower_for(
        &mut self,
        name: &str,
        name_span: Span,
        iterable: &ForIterable,
        statements: &[Statement],
    ) -> Result<(), Diagnostic> {
        match iterable {
            ForIterable::Range {
                start,
                end,
                inclusive,
            } => self.lower_range_for(name, name_span, start, end, *inclusive, statements),
            ForIterable::Array(expression) => {
                self.lower_iterable_for(name, name_span, expression, statements)
            }
        }
    }

    /// `for i in start..end`：求值端点一次，降低为 `std.collections.Range` 迭代器。
    fn lower_range_for(
        &mut self,
        name: &str,
        _name_span: Span,
        start: &ast::Expr,
        end: &ast::Expr,
        inclusive: bool,
        statements: &[Statement],
    ) -> Result<(), Diagnostic> {
        let start_value = self.lower_expr(start)?;
        let end_value = self.lower_expr(end)?;
        self.require_type(&start_value.ty, &Type::I32, start.span)?;
        self.require_type(&end_value.ty, &Type::I32, end.span)?;
        let key = if inclusive {
            "std.collections.Range::inclusive"
        } else {
            "std.collections.Range::exclusive"
        };
        let id = self.instantiate_method(key, Vec::new(), start.span)?;
        let signature = self.signature_of(id);
        if signature.parameters.len() != 2 {
            return Err(Diagnostic::plain(
                "internal error: `Range` constructor expects two endpoints",
            ));
        }
        let mut begin = start_value;
        begin.ty = signature.parameters[0].clone();
        let mut finish = end_value;
        finish.ty = signature.parameters[1].clone();
        let iterator = Expr {
            kind: ir::ExprKind::Call {
                function: id,
                arguments: vec![begin, finish],
            },
            ty: signature.return_type,
        };
        self.lower_iterator_loop(name, start.span, iterator, statements)
    }

    /// `for x in <array | slice | iterator>`：表达式只求值一次。
    ///
    /// 数组与切片通过只读切片适配到 `SliceIter<T>`；其余值必须实现 `Iterator`。
    /// 隐藏的可写迭代器局部覆盖整个循环，`next` 的业务实现来自同一协议。
    fn lower_iterable_for(
        &mut self,
        name: &str,
        _name_span: Span,
        expression: &ast::Expr,
        statements: &[Statement],
    ) -> Result<(), Diagnostic> {
        let value = self.lower_expr(expression)?;
        match value.ty.clone() {
            Type::Array { element, length } => {
                let array_ty = value.ty.clone();
                let element_ty = element.as_type();
                let array_local = self.store_temporary(value);
                let index = Expr {
                    kind: ir::ExprKind::Integer(0),
                    ty: Type::I32,
                };
                let place = ir::Place {
                    kind: ir::PlaceKind::Index {
                        base: Box::new(ir::Place {
                            kind: ir::PlaceKind::Local(array_local),
                            ty: array_ty,
                            mutable: true,
                        }),
                        index: Box::new(index),
                    },
                    ty: element_ty.clone(),
                    mutable: true,
                };
                let pointer = Expr {
                    kind: ir::ExprKind::AddressOf {
                        place: Box::new(place),
                    },
                    ty: Type::Ptr {
                        pointee: Box::new(element_ty.clone()),
                        mutable: true,
                    },
                };
                let len = Expr {
                    kind: ir::ExprKind::Integer(length as u64),
                    ty: Type::Usize,
                };
                let view = Expr {
                    kind: ir::ExprKind::MemView {
                        pointer: Box::new(pointer),
                        len: Box::new(len),
                    },
                    ty: Type::Slice {
                        element: Box::new(element_ty.clone()),
                        mutable: false,
                    },
                };
                let iterator = self.lower_slice_iter(view, element_ty, expression.span)?;
                self.lower_iterator_loop(name, expression.span, iterator, statements)
            }
            Type::Slice { element, .. } => {
                let element_ty = (*element).clone();
                let iterator = self.lower_slice_iter(value, element_ty, expression.span)?;
                self.lower_iterator_loop(name, expression.span, iterator, statements)
            }
            Type::Struct(_) | Type::Enum(_) => {
                self.lower_iterator_loop(name, expression.span, value, statements)
            }
            other => Err(Diagnostic::at(
                self.source,
                expression.span,
                format!(
                    "`for` expects an array, slice, range, or iterator, found `{}`",
                    self.display_type(&other)
                ),
            )),
        }
    }

    /// 构造标准库 `SliceIter<T>`（`s.iter()` 与数组/切片 for 适配共用）。
    fn lower_slice_iter(
        &mut self,
        slice: Expr,
        element: Type,
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let key = "std.collections.SliceIter::init";
        if !self.tables.methods.contains_key(key) {
            return Err(Diagnostic::plain(
                "internal error: source standard library is missing `SliceIter::init`",
            ));
        }
        let id = self.instantiate_method(key, vec![element], span)?;
        let signature = self.signature_of(id);
        let mut argument = slice;
        if let Some(expected) = signature.parameters.first() {
            argument.ty = expected.clone();
        }
        Ok(Expr {
            kind: ir::ExprKind::Call {
                function: id,
                arguments: vec![argument],
            },
            ty: signature.return_type,
        })
    }

    /// 把 `iterator_value` 降低为 `loop { match it.next() { Some(x) => body, None => break } }`。
    fn lower_iterator_loop(
        &mut self,
        name: &str,
        span: Span,
        iterator: Expr,
        statements: &[Statement],
    ) -> Result<(), Diagnostic> {
        let iterator_ty = iterator.ty.clone();
        let (type_name, type_args) = self.type_key_parts(&iterator_ty, span)?;
        if !self
            .tables
            .trait_impls
            .contains_key(&("std.Iterator".to_string(), type_name.clone()))
        {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!("`for` requires `{type_name}` to implement `Iterator`"),
            ));
        }
        let next_key = format!("{type_name}::next");
        if !matches!(
            self.method_receiver_kind(&next_key),
            Some(ReceiverKind::Ptr { .. })
        ) {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!("`Iterator::next` for `{type_name}` must take `self: *Self`"),
            ));
        }
        let next_id = self.instantiate_method(&next_key, type_args, span)?;
        let next_signature = self.signature_of(next_id);
        let option_ty = next_signature.return_type.clone();
        let Type::Enum(option_id) = option_ty.clone() else {
            return Err(Diagnostic::at(
                self.source,
                span,
                "`Iterator::next` must return an `Option`",
            ));
        };
        if self.state.borrow().type_keys[option_id.0].name != "std.Option" {
            return Err(Diagnostic::at(
                self.source,
                span,
                "`Iterator::next` must return `std.Option`",
            ));
        }
        let variants = self.enum_variants(option_id);
        let Some(some_index) = variants.iter().position(|variant| variant.name == "Some") else {
            return Err(Diagnostic::plain(
                "internal error: `std.Option` is missing the `Some` variant",
            ));
        };
        let item_ty = variants[some_index].fields[0].clone();

        let it_local = self.store_temporary(iterator);
        let opt_local = self.new_local(option_ty.clone());
        let loop_head = self.new_block();
        let body = self.new_block();
        let exit = self.new_block();
        self.terminate(Terminator::Jump(loop_head));

        self.current = loop_head;
        let receiver_place = ir::Place {
            kind: ir::PlaceKind::Local(it_local),
            ty: iterator_ty.clone(),
            mutable: true,
        };
        let receiver = Expr {
            kind: ir::ExprKind::AddressOf {
                place: Box::new(receiver_place),
            },
            ty: Type::Ptr {
                pointee: Box::new(iterator_ty),
                mutable: true,
            },
        };
        self.emit(Instruction::SetLocal {
            local: opt_local,
            value: Expr {
                kind: ir::ExprKind::Call {
                    function: next_id,
                    arguments: vec![receiver],
                },
                ty: option_ty.clone(),
            },
        });
        self.terminate(Terminator::Branch {
            condition: Expr {
                kind: ir::ExprKind::EnumIsVariant {
                    value: Box::new(local_expr(opt_local, option_ty.clone())),
                    variant: some_index,
                },
                ty: Type::Bool,
            },
            then_block: body,
            else_block: exit,
        });

        self.current = body;
        let item_local = self.new_local(item_ty.clone());
        self.emit(Instruction::SetLocal {
            local: item_local,
            value: Expr {
                kind: ir::ExprKind::EnumPayload {
                    value: Box::new(local_expr(opt_local, option_ty)),
                    variant: some_index,
                    field: 0,
                },
                ty: item_ty.clone(),
            },
        });
        self.loops.push(LoopTargets {
            break_block: exit,
            continue_block: loop_head,
            cleanup_scope: self.scopes.len(),
        });
        let result = self.with_scope(|lowerer| {
            lowerer.scopes.last_mut().unwrap().bindings.insert(
                name.to_string(),
                Binding {
                    local: item_local,
                    ty: item_ty.clone(),
                    mutable: false,
                },
            );
            lowerer.lower_statements(statements)
        });
        self.loops.pop();
        result?;
        if !self.is_terminated(self.current) {
            self.terminate(Terminator::Jump(loop_head));
        }
        self.current = exit;
        Ok(())
    }

    fn lower_expr(&mut self, expression: &ast::Expr) -> Result<Expr, Diagnostic> {
        self.current_location = self.location_of(expression.span);
        match &expression.kind {
            ExprKind::Number(value) => lower_number(self.source, value, false, expression.span),
            ExprKind::Character(value) => Ok(Expr {
                kind: ir::ExprKind::Char(*value),
                ty: Type::Char,
            }),
            ExprKind::Boolean(value) => Ok(Expr {
                kind: ir::ExprKind::Bool(*value),
                ty: Type::Bool,
            }),
            ExprKind::String(value) => Ok(Expr {
                kind: ir::ExprKind::String(value.clone()),
                ty: Type::String,
            }),
            ExprKind::Array(values) => {
                if values.is_empty() {
                    return Err(Diagnostic::at(
                        self.source,
                        expression.span,
                        "cannot infer the type of an empty array",
                    ));
                }
                let mut lowered = Vec::with_capacity(values.len());
                for value in values {
                    lowered.push(self.lower_expr(value)?);
                }
                let element = lowered[0].ty.as_scalar().ok_or_else(|| {
                    Diagnostic::at(
                        self.source,
                        values[0].span,
                        "array elements must be scalar values",
                    )
                })?;
                for (value, source_value) in lowered.iter().zip(values) {
                    self.require_type(&value.ty, &element.as_type(), source_value.span)?;
                }
                Ok(Expr {
                    kind: ir::ExprKind::Array(lowered),
                    ty: Type::Array {
                        element,
                        length: values.len(),
                    },
                })
            }
            ExprKind::RepeatArray { value, length } => {
                if *length == 0 {
                    return Err(Diagnostic::at(
                        self.source,
                        expression.span,
                        "array length must be greater than zero",
                    ));
                }
                let value = self.lower_expr(value)?;
                let element = value.ty.as_scalar().ok_or_else(|| {
                    Diagnostic::at(
                        self.source,
                        expression.span,
                        "array elements must be scalar values",
                    )
                })?;
                if layout::array_byte_size(element, *length, layout::POINTER_BYTES)
                    > layout::MAX_AGGREGATE_BYTES
                {
                    return Err(Diagnostic::at(
                        self.source,
                        expression.span,
                        "array is too large for the target",
                    ));
                }
                Ok(Expr {
                    kind: ir::ExprKind::RepeatArray {
                        value: Box::new(value),
                        length: *length,
                    },
                    ty: Type::Array {
                        element,
                        length: *length,
                    },
                })
            }
            ExprKind::Index { array, index } => {
                let array_value = self.lower_expr(array)?;
                let (element_ty, index_ty) = match array_value.ty.clone() {
                    Type::Array { element, length } => {
                        check_constant_index(self.source, index, length)?;
                        (element.as_type(), Type::I32)
                    }
                    Type::Slice { element, .. } => ((*element).clone(), Type::Usize),
                    other => {
                        return Err(Diagnostic::at(
                            self.source,
                            array.span,
                            format!("cannot index value of type `{}`", self.display_type(&other)),
                        ));
                    }
                };
                let index_value = if index_ty == Type::Usize {
                    self.lower_usize_value(index, "slice index")?
                } else {
                    let value = self.lower_expr(index)?;
                    self.require_type(&value.ty, &index_ty, index.span)?;
                    value
                };
                Ok(Expr {
                    kind: ir::ExprKind::Index {
                        array: Box::new(array_value),
                        index: Box::new(index_value),
                    },
                    ty: element_ty,
                })
            }
            ExprKind::Name(name) => {
                if name == "null" {
                    // null 字面量：类型从上下文推断为指针类型；此处使用占位类型。
                    return Ok(Expr {
                        kind: ir::ExprKind::Null,
                        ty: Type::Null,
                    });
                }
                let binding = self.lookup(name, expression.span)?;
                Ok(Expr {
                    kind: ir::ExprKind::Local(binding.local),
                    ty: binding.ty.clone(),
                })
            }
            ExprKind::Call {
                callee,
                callee_span,
                type_arguments,
                arguments,
            } => self.lower_call(callee, *callee_span, type_arguments, arguments),
            ExprKind::Cast { value, ty } => {
                let value = self.lower_expr(value)?;
                let to = self.resolve(ty)?;
                if !is_castable(value.ty.clone()) || !is_castable(to.clone()) {
                    return Err(Diagnostic::at(
                        self.source,
                        expression.span,
                        format!("cannot cast `{}` to `{to}`", value.ty),
                    ));
                }
                Ok(Expr {
                    kind: ir::ExprKind::Cast {
                        value: Box::new(value),
                        to: to.clone(),
                    },
                    ty: to,
                })
            }
            ExprKind::Unary { operator, operand } => {
                if matches!(operator, UnaryOperator::Negate)
                    && let ExprKind::Number(value) = &operand.kind
                {
                    return lower_number(self.source, value, true, expression.span);
                }
                let operand = self.lower_expr(operand)?;
                let expected = match operator {
                    UnaryOperator::Negate
                        if operand.ty.is_signed_integer() || operand.ty.is_float() =>
                    {
                        operand.ty.clone()
                    }
                    UnaryOperator::Negate => {
                        return Err(Diagnostic::at(
                            self.source,
                            expression.span,
                            format!("cannot negate `{}`", self.display_type(&operand.ty)),
                        ));
                    }
                    UnaryOperator::Not => Type::Bool,
                };
                self.require_type(&operand.ty, &expected, expression.span)?;
                Ok(Expr {
                    kind: ir::ExprKind::Unary {
                        operator: *operator,
                        operand: Box::new(operand),
                    },
                    ty: expected,
                })
            }
            ExprKind::Binary {
                operator,
                left,
                right,
            } => self.lower_binary(*operator, left, right, expression.span),
            ExprKind::Field {
                base,
                field,
                field_span,
            } => self.lower_field(base, field, *field_span),
            ExprKind::AddressOf { operand } => self.lower_address_of(operand),
            ExprKind::Deref { operand } => self.lower_deref(operand, expression.span),
            ExprKind::Match { value, arms } => self.lower_match(value, arms),
        }
    }

    /// 带期望类型地降低表达式。
    ///
    /// 用于从上下文补齐类型实参：`val x: Option<i32> = Option.None;` 中
    /// `Option.None` 没有显式类型实参，由声明的枚举类型确定具体实例。
    fn lower_expr_with_expected(
        &mut self,
        expression: &ast::Expr,
        expected: Option<&Type>,
    ) -> Result<Expr, Diagnostic> {
        self.current_location = self.location_of(expression.span);
        if let Some(Type::Enum(id)) = expected {
            let variants = self.enum_variants(*id);
            // 枚举成员引用（字段形式）：`Enum.Variant`。
            if let Some(variant) = enum_member_name(expression)
                && variants.iter().any(|candidate| candidate.name == variant)
            {
                return self.lower_enum_init(*id, &variants, &variant, &[], expression.span);
            }
            // 枚举构造（调用形式，含零参）：`Enum.Variant(...)`。
            // 必须校验 callee 前缀，避免把同名的普通函数调用误判为枚举构造。
            if let ExprKind::Call {
                callee,
                type_arguments,
                arguments,
                ..
            } = &expression.kind
                && type_arguments.is_empty()
            {
                let enum_name = self.state.borrow().type_keys[id.0].name.clone();
                if let Some(variant) = call_variant_for_enum(callee, &enum_name)
                    && variants.iter().any(|candidate| candidate.name == variant)
                {
                    return self.lower_enum_init(
                        *id,
                        &variants,
                        &variant,
                        arguments,
                        expression.span,
                    );
                }
            }
        }
        self.lower_expr(expression)
    }

    /// 取址 `&place`：返回指向 place 的指针。
    fn lower_address_of(&mut self, operand: &ast::Expr) -> Result<Expr, Diagnostic> {
        let place = self.lower_place(operand)?;
        let pointee = place.ty.clone();
        let mutable = place.mutable;
        Ok(Expr {
            kind: ir::ExprKind::AddressOf {
                place: Box::new(place),
            },
            ty: Type::Ptr {
                pointee: Box::new(pointee),
                mutable,
            },
        })
    }

    /// 解引用 `*p`：读取指针指向的值。
    fn lower_deref(&mut self, operand: &ast::Expr, span: Span) -> Result<Expr, Diagnostic> {
        let pointer = self.lower_expr(operand)?;
        let pointee = match &pointer.ty {
            Type::Ptr { pointee, .. } => (**pointee).clone(),
            other => {
                return Err(Diagnostic::at(
                    self.source,
                    span,
                    format!(
                        "cannot dereference value of type `{}`",
                        self.display_type(other)
                    ),
                ));
            }
        };
        if pointee == Type::Unit {
            return Err(Diagnostic::at(
                self.source,
                span,
                "cannot dereference a `*Unit` pointer",
            ));
        }
        Ok(Expr {
            kind: ir::ExprKind::Deref {
                pointer: Box::new(pointer),
            },
            ty: pointee,
        })
    }

    /// 将表达式解释为可寻址位置（place），用于取址与字段/索引写入。
    fn lower_place(&mut self, expr: &ast::Expr) -> Result<ir::Place, Diagnostic> {
        match &expr.kind {
            ExprKind::Name(name) => {
                let binding = self.lookup(name, expr.span)?.clone();
                Ok(ir::Place {
                    kind: ir::PlaceKind::Local(binding.local),
                    ty: binding.ty.clone(),
                    mutable: binding.mutable,
                })
            }
            ExprKind::Field {
                base,
                field,
                field_span,
            } => {
                let base_place = self.lower_place(base)?;
                let base_mutable = base_place.mutable;
                let Type::Struct(id) = base_place.ty.clone() else {
                    return Err(Diagnostic::at(
                        self.source,
                        expr.span,
                        format!(
                            "cannot take address of field on value of type `{}`",
                            base_place.ty
                        ),
                    ));
                };
                let fields = self.struct_fields(id);
                let field_index = fields
                    .iter()
                    .position(|field_def| field_def.name == *field)
                    .ok_or_else(|| {
                        Diagnostic::at(
                            self.source,
                            *field_span,
                            format!("struct has no field named `{field}`"),
                        )
                    })?;
                self.check_field_access(id, field_index, *field_span)?;
                let field_ty = fields[field_index].ty.clone();
                Ok(ir::Place {
                    kind: ir::PlaceKind::Field {
                        base: Box::new(base_place),
                        field: field_index,
                    },
                    ty: field_ty,
                    mutable: base_mutable,
                })
            }
            ExprKind::Index { array, index } => {
                let base_place = self.lower_place(array)?;
                let base_mutable = base_place.mutable;
                let (element_ty, element_mutable, index_is_usize) = match &base_place.ty {
                    Type::Slice { element, mutable } => ((**element).clone(), *mutable, true),
                    Type::Array { element, length } => {
                        check_constant_index(self.source, index, *length)?;
                        (element.as_type(), base_mutable, false)
                    }
                    other => {
                        return Err(Diagnostic::at(
                            self.source,
                            expr.span,
                            format!("cannot index value of type `{}`", self.display_type(other)),
                        ));
                    }
                };
                let index = if index_is_usize {
                    self.lower_usize_value(index, "slice index")?
                } else {
                    let value = self.lower_expr(index)?;
                    self.require_type(&value.ty, &Type::I32, index.span)?;
                    value
                };
                Ok(ir::Place {
                    kind: ir::PlaceKind::Index {
                        base: Box::new(base_place),
                        index: Box::new(index),
                    },
                    ty: element_ty,
                    mutable: element_mutable,
                })
            }
            ExprKind::Deref { operand } => {
                let pointer = self.lower_expr(operand)?;
                let (pointee, mutable) = match &pointer.ty {
                    Type::Ptr { pointee, mutable } => ((**pointee).clone(), *mutable),
                    other => {
                        return Err(Diagnostic::at(
                            self.source,
                            expr.span,
                            format!(
                                "cannot dereference value of type `{}`",
                                self.display_type(other)
                            ),
                        ));
                    }
                };
                if pointee == Type::Unit {
                    return Err(Diagnostic::at(
                        self.source,
                        expr.span,
                        "cannot dereference a `*Unit` pointer",
                    ));
                }
                Ok(ir::Place {
                    kind: ir::PlaceKind::Deref {
                        pointer: Box::new(pointer),
                    },
                    ty: pointee,
                    mutable,
                })
            }
            _ => Err(Diagnostic::at(
                self.source,
                expr.span,
                "expression does not have an addressable location",
            )),
        }
    }

    fn lower_call(
        &mut self,
        callee: &str,
        callee_span: Span,
        type_arguments: &[ast::TypeRef],
        arguments: &[ast::Expr],
    ) -> Result<Expr, Diagnostic> {
        if let Some(name) = callee.strip_prefix("std.mem.") {
            return self.lower_mem_intrinsic(name, callee_span, type_arguments, arguments);
        }
        if callee == "length" {
            if arguments.len() != 1 {
                return Err(Diagnostic::at(
                    self.source,
                    callee_span,
                    "`length` expects one argument",
                ));
            }
            let value = self.lower_expr(&arguments[0])?;
            self.require_type(&value.ty, &Type::String, arguments[0].span)?;
            return Ok(Expr {
                kind: ir::ExprKind::StringLength(Box::new(value)),
                ty: Type::Usize,
            });
        }
        if matches!(callee, "print" | "println") {
            return Err(Diagnostic::at(
                self.source,
                callee_span,
                format!("`{callee}` can only be used as a statement"),
            ));
        }
        // 方法调用：`value.method(...)`，接收者为用户类型（结构体/枚举/指针）。
        let method_receiver = callee.rsplit_once('.').and_then(|(receiver, method)| {
            if receiver.contains('.') {
                return None;
            }
            let binding = self.lookup(receiver, callee_span).ok()?;
            if matches!(
                binding.ty,
                Type::Struct(_) | Type::Enum(_) | Type::Ptr { .. }
            ) {
                Some((receiver.to_string(), method.to_string(), binding.clone()))
            } else {
                None
            }
        });
        if let Some((receiver, method, binding)) = method_receiver {
            return self.lower_method_call(
                &receiver,
                &method,
                &binding,
                type_arguments,
                arguments,
                callee_span,
            );
        }
        // 字符串字节视图：`s.bytes()`（零分配、只读）。
        if let Some((base_name, "bytes")) = callee.rsplit_once('.')
            && !base_name.contains('.')
            && self.lookup(base_name, callee_span).is_ok()
        {
            if !arguments.is_empty() {
                return Err(Diagnostic::at(
                    self.source,
                    callee_span,
                    "`bytes` expects no arguments",
                ));
            }
            let base = self.lower_expr(&ast::Expr {
                kind: ExprKind::Name(base_name.to_string()),
                span: callee_span,
            })?;
            self.require_type(&base.ty, &Type::String, callee_span)?;
            return Ok(Expr {
                kind: ir::ExprKind::StringBytes {
                    base: Box::new(base),
                },
                ty: Type::Slice {
                    element: Box::new(Type::U8),
                    mutable: false,
                },
            });
        }
        // 字符串视图：`string.from_bytes(bytes)`，校验 UTF-8，失败退出 104。
        if callee == "string.from_bytes" {
            if arguments.len() != 1 {
                return Err(Diagnostic::at(
                    self.source,
                    callee_span,
                    "`string.from_bytes` expects one argument",
                ));
            }
            let bytes = self.lower_expr(&arguments[0])?;
            let Type::Slice { element, .. } = bytes.ty.clone() else {
                return Err(Diagnostic::at(
                    self.source,
                    arguments[0].span,
                    "`string.from_bytes` expects a `[]const u8` slice",
                ));
            };
            self.require_type(&element, &Type::U8, arguments[0].span)?;
            return Ok(Expr {
                kind: ir::ExprKind::StringFromBytes {
                    bytes: Box::new(bytes),
                },
                ty: Type::String,
            });
        }
        // 切片子视图：`slice.slice(start, end)`。
        if let Some((base_name, "slice")) = callee.rsplit_once('.')
            && !base_name.contains('.')
            && self.lookup(base_name, callee_span).is_ok()
        {
            if arguments.len() != 2 {
                return Err(Diagnostic::at(
                    self.source,
                    callee_span,
                    "`slice` expects two arguments (start, end)",
                ));
            }
            let base = self.lower_expr(&ast::Expr {
                kind: ExprKind::Name(base_name.to_string()),
                span: callee_span,
            })?;
            let Type::Slice { element, mutable } = base.ty.clone() else {
                return Err(Diagnostic::at(
                    self.source,
                    callee_span,
                    format!("cannot call `.slice` on value of type `{}`", base.ty),
                ));
            };
            let start = self.lower_usize_value(&arguments[0], "slice range start")?;
            let end = self.lower_usize_value(&arguments[1], "slice range end")?;
            return Ok(Expr {
                kind: ir::ExprKind::SliceRange {
                    base: Box::new(base),
                    start: Box::new(start),
                    end: Box::new(end),
                },
                ty: Type::Slice { element, mutable },
            });
        }
        // 切片适配：`slice.iter()` 降低为 `SliceIter<T>::init(slice)`（零分配）。
        if let Some((base_name, "iter")) = callee.rsplit_once('.')
            && !base_name.contains('.')
            && let Some(binding) = self.lookup(base_name, callee_span).ok().cloned()
            && let Type::Slice { element, .. } = binding.ty
        {
            if !arguments.is_empty() {
                return Err(Diagnostic::at(
                    self.source,
                    callee_span,
                    "`iter` expects no arguments",
                ));
            }
            let base = self.lower_expr(&ast::Expr {
                kind: ExprKind::Name(base_name.to_string()),
                span: callee_span,
            })?;
            return self.lower_slice_iter(base, *element, callee_span);
        }
        // 结构体构造：`TypeName(args)` / `TypeName<i32>(args)`。
        if let Some(template) = self
            .tables
            .structs
            .get(callee)
            .map(|template| (template.type_params.clone(), template.fields.clone()))
        {
            let (params, fields) = template;
            let field_refs: Vec<ast::TypeRef> =
                fields.iter().map(|field| field.ty.clone()).collect();
            let id = self.construct_type(
                callee,
                &params,
                &field_refs,
                type_arguments,
                arguments,
                callee_span,
            )?;
            return self.lower_struct_init(id, arguments, callee_span);
        }
        // 枚举项构造：`Enum.Variant(args)` / `Enum<i32>.Variant(args)`。
        if let Some((enum_name, variant_name)) = callee.rsplit_once('.')
            && let Some(template) = self
                .tables
                .enums
                .get(enum_name)
                .map(|template| (template.type_params.clone(), template.variants.clone()))
        {
            let (params, variants) = template;
            let fields = variants
                .iter()
                .find(|variant| variant.name == variant_name)
                .map(|variant| variant.fields.clone())
                .ok_or_else(|| {
                    Diagnostic::at(
                        self.source,
                        callee_span,
                        format!("unknown variant `{variant_name}`"),
                    )
                })?;
            let id = self.construct_type(
                enum_name,
                &params,
                &fields,
                type_arguments,
                arguments,
                callee_span,
            )?;
            let variants = self.enum_variants(id);
            return self.lower_enum_init(id, &variants, variant_name, arguments, callee_span);
        }
        // 关联函数：`Type::function(...)`。
        if let Some((type_part, function_name)) = callee.rsplit_once("::") {
            let (resolved, inferred_args) = match self.env.get(type_part) {
                Some(ty) => self.type_key_parts(&ty, callee_span)?,
                None => (type_part.to_string(), Vec::new()),
            };
            let explicit = if type_arguments.is_empty() {
                inferred_args
            } else {
                let mut resolved_args = Vec::with_capacity(type_arguments.len());
                for ty in type_arguments {
                    resolved_args.push(self.resolve(ty)?);
                }
                resolved_args
            };
            let key = format!("{resolved}::{function_name}");
            if self.tables.methods.contains_key(&key) {
                return self.lower_associated_call(&key, explicit, arguments, callee_span);
            }
            return Err(Diagnostic::at(
                self.source,
                callee_span,
                format!("unknown associated function `{resolved}::{function_name}`"),
            ));
        }
        // 普通函数或泛型函数实例。
        let Some(template) = self.tables.functions.get(callee) else {
            return Err(Diagnostic::at(
                self.source,
                callee_span,
                format!("unknown function `{callee}`"),
            ));
        };
        let param_names: Vec<String> = template
            .type_params()
            .iter()
            .map(|param| param.name.clone())
            .collect();
        let parameter_types: Vec<ast::TypeRef> = template
            .function
            .parameters
            .iter()
            .map(|parameter| parameter.ty.clone())
            .collect();
        let template_name = callee.to_string();
        let depth = self.depth;

        if !param_names.is_empty() && type_arguments.is_empty() {
            // 先降低实参，再由实参类型推断类型实参。
            let mut lowered = Vec::with_capacity(arguments.len());
            for argument in arguments {
                lowered.push(self.lower_expr(argument)?);
            }
            let actuals: Vec<Type> = lowered.iter().map(|value| value.ty.clone()).collect();
            let inferred = {
                let ctx = self.context();
                let template = &self.tables.functions[&template_name];
                self.state.borrow_mut().infer_type_args(
                    &ctx,
                    self.source,
                    template,
                    &actuals,
                    None,
                    callee_span,
                )?
            };
            let id = self.instantiate(&template_name, inferred, callee_span, depth)?;
            let signature = self.signature_of(id);
            if arguments.len() != signature.parameters.len() {
                return Err(Diagnostic::at(
                    self.source,
                    callee_span,
                    format!(
                        "function `{callee}` expects {} arguments but {} were provided",
                        signature.parameters.len(),
                        arguments.len()
                    ),
                ));
            }
            for (value, expected) in lowered.iter().zip(&signature.parameters) {
                self.require_type(&value.ty, expected, callee_span)?;
            }
            return Ok(Expr {
                kind: ir::ExprKind::Call {
                    function: id,
                    arguments: lowered,
                },
                ty: signature.return_type,
            });
        }

        let explicit = if type_arguments.is_empty() {
            Vec::new()
        } else {
            let mut resolved = Vec::with_capacity(type_arguments.len());
            for ty in type_arguments {
                resolved.push(self.resolve(ty)?);
            }
            resolved
        };
        if !param_names.is_empty() && explicit.len() != param_names.len() {
            return Err(Diagnostic::at(
                self.source,
                callee_span,
                format!(
                    "function `{callee}` expects {} type arguments but {} were provided",
                    param_names.len(),
                    explicit.len()
                ),
            ));
        }
        let id = self.instantiate(&template_name, explicit, callee_span, depth)?;
        let signature = self.signature_of(id);
        if arguments.len() != signature.parameters.len() {
            return Err(Diagnostic::at(
                self.source,
                callee_span,
                format!(
                    "function `{callee}` expects {} arguments but {} were provided",
                    signature.parameters.len(),
                    arguments.len()
                ),
            ));
        }
        let _ = parameter_types;
        let mut lowered = Vec::with_capacity(arguments.len());
        for (argument, expected) in arguments.iter().zip(&signature.parameters) {
            let value = self.lower_expr_with_expected(argument, Some(expected))?;
            self.require_type(&value.ty, expected, argument.span)?;
            lowered.push(value);
        }
        Ok(Expr {
            kind: ir::ExprKind::Call {
                function: id,
                arguments: lowered,
            },
            ty: signature.return_type,
        })
    }

    /// 计算结构体/枚举构造的类型实参（显式或由实参推断）并实例化。
    #[allow(clippy::too_many_arguments)]
    fn construct_type(
        &mut self,
        name: &str,
        params: &[String],
        field_refs: &[ast::TypeRef],
        explicit: &[ast::TypeRef],
        arguments: &[ast::Expr],
        span: Span,
    ) -> Result<TypeId, Diagnostic> {
        let args = if !explicit.is_empty() {
            let mut resolved = Vec::with_capacity(explicit.len());
            for ty in explicit {
                resolved.push(self.resolve(ty)?);
            }
            resolved
        } else if params.is_empty() {
            Vec::new()
        } else {
            let mut actuals = Vec::with_capacity(arguments.len());
            for argument in arguments {
                actuals.push(self.lower_expr(argument)?.ty);
            }
            let ctx = self.context();
            self.state.borrow_mut().infer_from_field_types(
                &ctx,
                self.source,
                params,
                field_refs,
                &actuals,
                span,
            )?
        };
        if !params.is_empty() && args.len() != params.len() {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "type `{name}` expects {} type arguments but {} were provided",
                    params.len(),
                    args.len()
                ),
            ));
        }
        let ctx = self.context();
        let ty = self.state.borrow_mut().instantiate_named(
            &ctx,
            self.source,
            name,
            params,
            args,
            span,
        )?;
        match ty {
            Type::Struct(id) | Type::Enum(id) => Ok(id),
            _ => unreachable!("instantiate_named returns an aggregate type"),
        }
    }

    /// 取具体聚合类型（或指针目标）的实例名与类型实参。
    fn type_key_parts(&self, ty: &Type, span: Span) -> Result<(String, Vec<Type>), Diagnostic> {
        match ty {
            Type::Struct(id) | Type::Enum(id) => {
                let state = self.state.borrow();
                Ok((
                    state.type_keys[id.0].name.clone(),
                    state.type_keys[id.0].args.clone(),
                ))
            }
            Type::Ptr { pointee, .. } => self.type_key_parts(pointee, span),
            other => Err(Diagnostic::at(
                self.source,
                span,
                format!("type `{}` has no methods", self.display_type(other)),
            )),
        }
    }

    fn method_receiver_kind(&self, key: &str) -> Option<ReceiverKind> {
        let template = self.tables.methods.get(key)?;
        let first = template.function.parameters.first()?;
        if !first.receiver {
            return None;
        }
        Some(match &first.ty.kind {
            TypeRefKind::Ptr { mutable, .. } => ReceiverKind::Ptr { mutable: *mutable },
            _ => ReceiverKind::Value,
        })
    }

    fn instantiate_method(
        &self,
        key: &str,
        args: Vec<Type>,
        span: Span,
    ) -> Result<FunctionId, Diagnostic> {
        let ctx = self.context();
        self.state
            .borrow_mut()
            .instantiate_method(&ctx, self.source, key, args, span, self.depth)
    }

    fn lower_method_call(
        &mut self,
        receiver_name: &str,
        method_name: &str,
        binding: &Binding,
        type_arguments: &[ast::TypeRef],
        arguments: &[ast::Expr],
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        if !type_arguments.is_empty() {
            return Err(Diagnostic::at(
                self.source,
                span,
                "explicit type arguments on method calls are not supported yet",
            ));
        }
        let receiver_ty = binding.ty.clone();
        let (type_name, type_args) = self.type_key_parts(&receiver_ty, span)?;
        let key = format!("{type_name}::{method_name}");
        let Some(receiver_kind) = self.method_receiver_kind(&key) else {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!("type `{type_name}` has no method `{method_name}`"),
            ));
        };
        let self_expr = match (receiver_kind, receiver_ty) {
            (ReceiverKind::Value, Type::Ptr { .. }) => {
                return Err(Diagnostic::at(
                    self.source,
                    span,
                    format!(
                        "method `{method_name}` takes `self` by value and cannot be called on a pointer"
                    ),
                ));
            }
            (ReceiverKind::Value, ty) => local_expr(binding.local, ty),
            (ReceiverKind::Ptr { .. }, Type::Ptr { mutable, .. }) => {
                if let ReceiverKind::Ptr {
                    mutable: needs_mutable,
                } = receiver_kind
                    && needs_mutable
                    && !mutable
                {
                    return Err(Diagnostic::at(
                        self.source,
                        span,
                        format!(
                            "method `{method_name}` needs a mutable receiver but the pointer is `*const`"
                        ),
                    ));
                }
                local_expr(binding.local, binding.ty.clone())
            }
            (ReceiverKind::Ptr { mutable }, _) => {
                let place = self.lower_place(&ast::Expr {
                    kind: ExprKind::Name(receiver_name.to_string()),
                    span,
                })?;
                if mutable && !place.mutable {
                    return Err(Diagnostic::at(
                        self.source,
                        span,
                        format!("cannot call `{method_name}` on an immutable receiver"),
                    ));
                }
                let pointee = place.ty.clone();
                Expr {
                    kind: ir::ExprKind::AddressOf {
                        place: Box::new(place),
                    },
                    ty: Type::Ptr {
                        pointee: Box::new(pointee),
                        mutable,
                    },
                }
            }
        };
        let id = self.instantiate_method(&key, type_args, span)?;
        let signature = self.signature_of(id);
        if signature.parameters.len() != arguments.len() + 1 {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "method `{method_name}` expects {} arguments but {} were provided",
                    signature.parameters.len() - 1,
                    arguments.len()
                ),
            ));
        }
        let mut lowered = Vec::with_capacity(arguments.len() + 1);
        lowered.push(self_expr);
        for (argument, expected) in arguments.iter().zip(signature.parameters.iter().skip(1)) {
            let value = self.lower_expr_with_expected(argument, Some(expected))?;
            self.require_type(&value.ty, expected, argument.span)?;
            lowered.push(value);
        }
        Ok(Expr {
            kind: ir::ExprKind::Call {
                function: id,
                arguments: lowered,
            },
            ty: signature.return_type,
        })
    }

    fn lower_associated_call(
        &mut self,
        key: &str,
        type_args: Vec<Type>,
        arguments: &[ast::Expr],
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let requires_receiver = self
            .tables
            .methods
            .get(key)
            .and_then(|template| template.function.parameters.first())
            .map(|parameter| parameter.receiver)
            .unwrap_or(false);
        if requires_receiver {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!("`{key}` is a method and requires a receiver"),
            ));
        }
        let id = self.instantiate_method(key, type_args, span)?;
        let signature = self.signature_of(id);
        if signature.parameters.len() != arguments.len() {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "associated function `{key}` expects {} arguments but {} were provided",
                    signature.parameters.len(),
                    arguments.len()
                ),
            ));
        }
        let mut lowered = Vec::with_capacity(arguments.len());
        for (argument, expected) in arguments.iter().zip(&signature.parameters) {
            let value = self.lower_expr_with_expected(argument, Some(expected))?;
            self.require_type(&value.ty, expected, argument.span)?;
            lowered.push(value);
        }
        Ok(Expr {
            kind: ir::ExprKind::Call {
                function: id,
                arguments: lowered,
            },
            ty: signature.return_type,
        })
    }

    /// 降低内建 `std.mem` intrinsic（M14-C）。
    ///
    /// intrinsic 的显式类型实参已经在 parser 中作为 AST 类型节点保存，这里解析为
    /// 具体类型并交给统一布局/分配路径。M14 不支持用户泛型，所以类型实参只用于
    /// 这些内建入口。
    fn lower_mem_intrinsic(
        &mut self,
        name: &str,
        span: Span,
        type_arguments: &[ast::TypeRef],
        arguments: &[ast::Expr],
    ) -> Result<Expr, Diagnostic> {
        match name {
            "size_of" | "align_of" => {
                self.require_argument_count(name, arguments.len(), 0, span)?;
                let element = self.required_type_argument(name, type_arguments, span)?;
                let layout = {
                    let state = self.state.borrow();
                    layout::layout_of(&element, &state.types, layout::POINTER_BYTES)
                };
                let value = if name == "size_of" {
                    layout.size
                } else {
                    layout.align
                };
                Ok(Expr {
                    kind: ir::ExprKind::Integer(u64::from(value)),
                    ty: Type::Usize,
                })
            }
            "alloc" => {
                self.require_argument_count(name, arguments.len(), 1, span)?;
                let element = self.required_type_argument(name, type_arguments, span)?;
                self.reject_zero_sized(name, &element, span)?;
                let count = self.lower_count(&arguments[0], name)?;
                Ok(Expr {
                    kind: ir::ExprKind::MemAlloc {
                        element: element.clone(),
                        count: Box::new(count),
                    },
                    ty: Type::Slice {
                        element: Box::new(element),
                        mutable: true,
                    },
                })
            }
            "free" => {
                self.require_argument_count(name, arguments.len(), 1, span)?;
                let buffer = self.lower_expr(&arguments[0])?;
                let Type::Slice {
                    element,
                    mutable: true,
                } = buffer.ty.clone()
                else {
                    return Err(Diagnostic::at(
                        self.source,
                        arguments[0].span,
                        "`mem.free` expects a writable slice returned by `mem.alloc`",
                    ));
                };
                let element = self.matching_type_argument(*element, name, type_arguments, span)?;
                Ok(Expr {
                    kind: ir::ExprKind::MemFree {
                        element,
                        buffer: Box::new(buffer),
                    },
                    ty: Type::Unit,
                })
            }
            "create" => {
                self.require_argument_count(name, arguments.len(), 1, span)?;
                let value = self.lower_expr(&arguments[0])?;
                let element = if type_arguments.is_empty() {
                    if is_null_ptr(&value.ty) {
                        return Err(Diagnostic::at(
                            self.source,
                            arguments[0].span,
                            "cannot infer the type of `mem.create`; provide an explicit type argument",
                        ));
                    }
                    value.ty.clone()
                } else {
                    self.required_type_argument(name, type_arguments, span)?
                };
                self.require_type(&value.ty, &element, arguments[0].span)?;
                self.reject_zero_sized(name, &element, span)?;
                Ok(Expr {
                    kind: ir::ExprKind::MemCreate {
                        element: element.clone(),
                        value: Box::new(value),
                    },
                    ty: Type::Ptr {
                        pointee: Box::new(element),
                        mutable: true,
                    },
                })
            }
            "destroy" => {
                self.require_argument_count(name, arguments.len(), 1, span)?;
                let pointer = self.lower_expr(&arguments[0])?;
                let element = if type_arguments.is_empty() {
                    let Type::Ptr {
                        pointee,
                        mutable: true,
                    } = pointer.ty.clone()
                    else {
                        return Err(Diagnostic::at(
                            self.source,
                            arguments[0].span,
                            "`mem.destroy` expects a writable pointer returned by `mem.create`",
                        ));
                    };
                    *pointee
                } else {
                    self.required_type_argument(name, type_arguments, span)?
                };
                self.require_type(
                    &pointer.ty,
                    &Type::Ptr {
                        pointee: Box::new(element.clone()),
                        mutable: true,
                    },
                    arguments[0].span,
                )?;
                Ok(Expr {
                    kind: ir::ExprKind::MemDestroy {
                        element,
                        pointer: Box::new(pointer),
                    },
                    ty: Type::Unit,
                })
            }
            "copy" => {
                self.require_argument_count(name, arguments.len(), 2, span)?;
                let dst = self.lower_expr(&arguments[0])?;
                let src = self.lower_expr(&arguments[1])?;
                let Type::Slice {
                    element,
                    mutable: true,
                } = dst.ty.clone()
                else {
                    return Err(Diagnostic::at(
                        self.source,
                        arguments[0].span,
                        "`mem.copy` expects a writable destination slice",
                    ));
                };
                let Type::Slice {
                    element: src_element,
                    ..
                } = src.ty.clone()
                else {
                    return Err(Diagnostic::at(
                        self.source,
                        arguments[1].span,
                        "`mem.copy` expects a source slice",
                    ));
                };
                let element = self.matching_type_argument(*element, name, type_arguments, span)?;
                self.require_type(&src_element, &element, arguments[1].span)?;
                Ok(Expr {
                    kind: ir::ExprKind::MemCopy {
                        element,
                        dst: Box::new(dst),
                        src: Box::new(src),
                    },
                    ty: Type::Unit,
                })
            }
            "is_valid_utf8" => {
                self.require_argument_count(name, arguments.len(), 1, span)?;
                if !type_arguments.is_empty() {
                    return Err(Diagnostic::at(
                        self.source,
                        span,
                        "`mem.is_valid_utf8` does not accept type arguments",
                    ));
                }
                let bytes = self.lower_expr(&arguments[0])?;
                let Type::Slice { element, .. } = bytes.ty.clone() else {
                    return Err(Diagnostic::at(
                        self.source,
                        arguments[0].span,
                        "`mem.is_valid_utf8` expects a `[]const u8` slice",
                    ));
                };
                self.require_type(&element, &Type::U8, arguments[0].span)?;
                Ok(Expr {
                    kind: ir::ExprKind::MemIsValidUtf8 {
                        bytes: Box::new(bytes),
                    },
                    ty: Type::Bool,
                })
            }
            "view" | "view_const" => {
                self.require_argument_count(name, arguments.len(), 2, span)?;
                let element = self.required_type_argument(name, type_arguments, span)?;
                let pointer = self.lower_expr(&arguments[0])?;
                let mutable = name == "view";
                self.require_type(
                    &pointer.ty,
                    &Type::Ptr {
                        pointee: Box::new(element.clone()),
                        mutable,
                    },
                    arguments[0].span,
                )?;
                let len = self.lower_count(&arguments[1], name)?;
                Ok(Expr {
                    kind: ir::ExprKind::MemView {
                        pointer: Box::new(pointer),
                        len: Box::new(len),
                    },
                    ty: Type::Slice {
                        element: Box::new(element),
                        mutable,
                    },
                })
            }
            "cast_ptr" | "cast_const_ptr" => {
                self.require_argument_count(name, arguments.len(), 1, span)?;
                let element = self.required_type_argument(name, type_arguments, span)?;
                let pointer = self.lower_expr(&arguments[0])?;
                let Type::Ptr { mutable, .. } = &pointer.ty else {
                    return Err(Diagnostic::at(
                        self.source,
                        arguments[0].span,
                        format!("`mem.{name}` expects a pointer"),
                    ));
                };
                // `cast_ptr` 不能移除 const；`cast_const_ptr` 允许保留或增加 const。
                if name == "cast_ptr" && !*mutable {
                    return Err(Diagnostic::at(
                        self.source,
                        arguments[0].span,
                        "`mem.cast_ptr` cannot remove `const`; use `mem.cast_const_ptr`",
                    ));
                }
                Ok(Expr {
                    kind: ir::ExprKind::MemCast {
                        pointer: Box::new(pointer),
                    },
                    ty: Type::Ptr {
                        pointee: Box::new(element),
                        mutable: name == "cast_ptr",
                    },
                })
            }
            _ => Err(Diagnostic::at(
                self.source,
                span,
                format!("unknown function `std.mem.{name}`"),
            )),
        }
    }

    fn require_argument_count(
        &self,
        name: &str,
        actual: usize,
        expected: usize,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if actual == expected {
            Ok(())
        } else {
            Err(Diagnostic::at(
                self.source,
                span,
                format!("`mem.{name}` expects {expected} argument(s) but {actual} were provided"),
            ))
        }
    }

    /// 解析 `mem.<name>` 的显式类型实参；缺失时给出可操作诊断。
    fn required_type_argument(
        &self,
        name: &str,
        type_arguments: &[ast::TypeRef],
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let Some(ty) = type_arguments.first() else {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "`mem.{name}` requires an explicit type argument, e.g. `mem.{name}<T>(...)`"
                ),
            ));
        };
        if type_arguments.len() > 1 {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!("`mem.{name}` accepts exactly one type argument"),
            ));
        }
        self.resolve(ty)
    }

    /// 若给出类型实参，则要求它与推断出的元素类型一致；否则使用推断结果。
    fn matching_type_argument(
        &self,
        inferred: Type,
        name: &str,
        type_arguments: &[ast::TypeRef],
        span: Span,
    ) -> Result<Type, Diagnostic> {
        if type_arguments.is_empty() {
            return Ok(inferred);
        }
        let declared = self.required_type_argument(name, type_arguments, span)?;
        if declared == inferred {
            Ok(declared)
        } else {
            Err(Diagnostic::at(
                self.source,
                span,
                format!("type argument `{declared}` does not match `{inferred}`"),
            ))
        }
    }

    fn reject_zero_sized(&self, name: &str, element: &Type, span: Span) -> Result<(), Diagnostic> {
        let layout = {
            let state = self.state.borrow();
            layout::layout_of(element, &state.types, layout::POINTER_BYTES)
        };
        if layout.size == 0 {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "`mem.{name}` cannot operate on zero-sized type `{}`",
                    self.display_type(element)
                ),
            ));
        }
        Ok(())
    }

    /// 分配/视图长度：接受 `usize` 或无符号整数字面量（按上下文推断为 `usize`）。
    fn lower_count(&mut self, value: &ast::Expr, name: &str) -> Result<Expr, Diagnostic> {
        self.lower_usize_value(value, &format!("`mem.{name}` count"))
    }

    /// 需要一个 `usize` 值：接受 `usize` 或无后缀/`usize` 后缀的整数字面量，
    /// 其余整数类型必须显式转换；负数字面量报错。
    fn lower_usize_value(
        &mut self,
        value: &ast::Expr,
        description: &str,
    ) -> Result<Expr, Diagnostic> {
        if let ExprKind::Unary {
            operator: UnaryOperator::Negate,
            ..
        } = &value.kind
        {
            return Err(Diagnostic::at(
                self.source,
                value.span,
                format!("{description} must not be negative"),
            ));
        }
        let lowered = self.lower_expr(value)?;
        if lowered.ty == Type::Usize {
            return Ok(lowered);
        }
        if lowered.ty.is_integer()
            && let ExprKind::Number(raw) = &value.kind
        {
            let suffix = raw.rsplit_once('_').map(|(_, suffix)| suffix).unwrap_or("");
            if suffix.is_empty() || suffix == "usize" {
                return Ok(Expr {
                    kind: lowered.kind,
                    ty: Type::Usize,
                });
            }
        }
        Err(Diagnostic::at(
            self.source,
            value.span,
            format!("{description} must be `usize`; convert explicitly"),
        ))
    }

    fn lower_struct_init(
        &mut self,
        id: TypeId,
        arguments: &[ast::Expr],
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let fields = self.struct_fields(id);
        if arguments.len() != fields.len() {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "struct constructor expects {} arguments but {} were provided",
                    fields.len(),
                    arguments.len()
                ),
            ));
        }
        for index in 0..fields.len() {
            self.check_field_access(id, index, span)?;
        }
        let mut lowered = Vec::with_capacity(arguments.len());
        for (argument, field) in arguments.iter().zip(&fields) {
            let value = self.lower_expr_with_expected(argument, Some(&field.ty))?;
            self.require_type(&value.ty, &field.ty, argument.span)?;
            lowered.push(value);
        }
        Ok(Expr {
            kind: ir::ExprKind::StructInit { fields: lowered },
            ty: Type::Struct(id),
        })
    }

    fn lower_enum_init(
        &mut self,
        id: TypeId,
        variants: &[EnumVariant],
        variant_name: &str,
        arguments: &[ast::Expr],
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let index = variants
            .iter()
            .position(|variant| variant.name == variant_name)
            .ok_or_else(|| {
                Diagnostic::at(
                    self.source,
                    span,
                    format!("unknown variant `{variant_name}`"),
                )
            })?;
        let variant = &variants[index];
        if arguments.len() != variant.fields.len() {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "variant `{variant_name}` expects {} arguments but {} were provided",
                    variant.fields.len(),
                    arguments.len()
                ),
            ));
        }
        let mut lowered = Vec::with_capacity(arguments.len());
        for (argument, expected) in arguments.iter().zip(&variant.fields) {
            let value = self.lower_expr_with_expected(argument, Some(expected))?;
            self.require_type(&value.ty, expected, argument.span)?;
            lowered.push(value);
        }
        Ok(Expr {
            kind: ir::ExprKind::EnumInit {
                variant: index,
                arguments: lowered,
            },
            ty: Type::Enum(id),
        })
    }

    fn lower_field(
        &mut self,
        base: &ast::Expr,
        field: &str,
        field_span: Span,
    ) -> Result<Expr, Diagnostic> {
        // 无参枚举项引用：`Enum.Variant`（无括号）等价于 `Enum.Variant()`。
        if let ExprKind::Name(name) = &base.kind {
            if let Some(id) = self.concrete_type_id(name) {
                if self.type_is_enum(id) {
                    let variants = self.enum_variants(id);
                    let index = variants
                        .iter()
                        .position(|variant| variant.name == field)
                        .ok_or_else(|| {
                            Diagnostic::at(
                                self.source,
                                field_span,
                                format!("unknown variant `{field}`"),
                            )
                        })?;
                    if !variants[index].fields.is_empty() {
                        return Err(Diagnostic::at(
                            self.source,
                            field_span,
                            format!("variant `{field}` requires arguments"),
                        ));
                    }
                    return Ok(Expr {
                        kind: ir::ExprKind::EnumInit {
                            variant: index,
                            arguments: Vec::new(),
                        },
                        ty: Type::Enum(id),
                    });
                }
            } else if self.tables.enums.contains_key(name) {
                return Err(Diagnostic::at(
                    self.source,
                    field_span,
                    format!(
                        "cannot infer the type arguments of `{name}` for variant `{field}`; annotate the expected type or write `{name}<...>.{field}`"
                    ),
                ));
            }
        }
        let base_value = self.lower_expr(base)?;
        // 切片只读字段：`.ptr`（地址）与 `.len`（元素个数）。
        if let Type::Slice { element, mutable } = base_value.ty.clone() {
            match field {
                "ptr" => {
                    return Ok(Expr {
                        kind: ir::ExprKind::SlicePtr {
                            base: Box::new(base_value),
                        },
                        ty: Type::Ptr {
                            pointee: element,
                            mutable,
                        },
                    });
                }
                "len" => {
                    return Ok(Expr {
                        kind: ir::ExprKind::SliceLen {
                            base: Box::new(base_value),
                        },
                        ty: Type::Usize,
                    });
                }
                _ => {
                    return Err(Diagnostic::at(
                        self.source,
                        field_span,
                        format!("slice has no field named `{field}`"),
                    ));
                }
            }
        }
        let Type::Struct(id) = base_value.ty else {
            return Err(Diagnostic::at(
                self.source,
                field_span,
                format!("cannot access field on value of type `{}`", base_value.ty),
            ));
        };
        let fields = self.struct_fields(id);
        let index = fields
            .iter()
            .position(|field_def| field_def.name == field)
            .ok_or_else(|| {
                Diagnostic::at(
                    self.source,
                    field_span,
                    format!("struct has no field named `{field}`"),
                )
            })?;
        self.check_field_access(id, index, field_span)?;
        Ok(Expr {
            kind: ir::ExprKind::Field {
                base: Box::new(base_value),
                field: index,
            },
            ty: fields[index].ty.clone(),
        })
    }

    fn lower_match(
        &mut self,
        value: &ast::Expr,
        arms: &[ast::MatchArm],
    ) -> Result<Expr, Diagnostic> {
        let value_span = value.span;
        let value = self.lower_expr(value)?;
        let Type::Enum(id) = value.ty else {
            return Err(Diagnostic::at(
                self.source,
                value_span,
                "`match` expects an enum value",
            ));
        };
        let variants = self.enum_variants(id);

        let mut lowered_arms = Vec::with_capacity(arms.len());
        let mut result_type: Option<Type> = None;
        let mut matched = vec![false; variants.len()];
        let mut has_wildcard = false;

        for arm in arms {
            let (variant_index, binding_names, pattern_span) = match &arm.pattern {
                ast::MatchPattern::Wildcard => {
                    has_wildcard = true;
                    (None, Vec::new(), value_span)
                }
                ast::MatchPattern::Enum {
                    name,
                    name_span,
                    type_arguments: _,
                    bindings,
                } => {
                    let variant_name = name.rsplit_once('.').map(|(_, v)| v).unwrap_or(name);
                    let index = variants
                        .iter()
                        .position(|variant| variant.name == variant_name)
                        .ok_or_else(|| {
                            Diagnostic::at(
                                self.source,
                                *name_span,
                                format!("unknown variant `{variant_name}`"),
                            )
                        })?;
                    if matched[index] {
                        return Err(Diagnostic::at(
                            self.source,
                            *name_span,
                            format!("variant `{variant_name}` is matched more than once"),
                        ));
                    }
                    matched[index] = true;
                    (Some(index), bindings.clone(), *name_span)
                }
            };

            // 为解构绑定创建局部变量：与 variant 字段一一对应。
            let mut binding_locals = Vec::new();
            let mut binding_types = Vec::new();
            if let Some(index) = variant_index {
                let variant = &variants[index];
                if binding_names.len() != variant.fields.len() {
                    return Err(Diagnostic::at(
                        self.source,
                        pattern_span,
                        format!(
                            "variant `{}` expects {} bindings but {} were provided",
                            variant.name,
                            variant.fields.len(),
                            binding_names.len()
                        ),
                    ));
                }
                for field in &variant.fields {
                    binding_locals.push(self.new_local(field.clone()));
                    binding_types.push(field.clone());
                }
            }

            // 在绑定作用域内 lowering 分支体。
            let body = self.with_scope(|lowerer| {
                let scope = lowerer.scopes.last_mut().unwrap();
                for ((name, local), ty) in binding_names
                    .iter()
                    .zip(binding_locals.iter())
                    .zip(binding_types.iter())
                {
                    // `_` 表示忽略该字段，不绑定到任何变量。
                    if name == "_" {
                        continue;
                    }
                    scope.bindings.insert(
                        name.clone(),
                        Binding {
                            local: *local,
                            ty: ty.clone(),
                            mutable: false,
                        },
                    );
                }
                lowerer.lower_expr(&arm.body)
            })?;

            match result_type {
                None => result_type = Some(body.ty.clone()),
                Some(ref expected) => self.require_type(&body.ty, expected, arm.body.span)?,
            }
            let pattern = match variant_index {
                Some(v) => ir::MatchPattern::Variant {
                    variant: v,
                    bindings: binding_locals,
                },
                None => ir::MatchPattern::Wildcard,
            };
            lowered_arms.push(ir::MatchArm { pattern, body });
        }

        // 穷尽性检查：所有 variant 都被匹配，或存在通配符。
        if !has_wildcard {
            for (index, is_matched) in matched.iter().enumerate() {
                if !is_matched {
                    return Err(Diagnostic::at(
                        self.source,
                        value_span,
                        format!("match is missing variant `{}`", variants[index].name),
                    ));
                }
            }
        }

        let ty = result_type.unwrap_or(Type::Unit);
        Ok(Expr {
            kind: ir::ExprKind::Match {
                value: Box::new(value),
                arms: lowered_arms,
            },
            ty,
        })
    }

    fn lower_binary(
        &mut self,
        operator: BinaryOperator,
        left: &ast::Expr,
        right: &ast::Expr,
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let left = self.lower_expr(left)?;
        let right = self.lower_expr(right)?;
        let ty = match operator {
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder => {
                if !(left.ty.is_integer() || left.ty.is_float())
                    || matches!(operator, BinaryOperator::Remainder) && left.ty.is_float()
                {
                    return Err(Diagnostic::at(
                        self.source,
                        span,
                        format!("operator is not supported for `{}`", left.ty),
                    ));
                }
                self.require_type(&right.ty, &left.ty, span)?;
                left.ty.clone()
            }
            BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => {
                if !(left.ty.is_integer() || left.ty.is_float() || left.ty == Type::Char) {
                    return Err(Diagnostic::at(
                        self.source,
                        span,
                        format!(
                            "values of type `{}` cannot be ordered",
                            self.display_type(&left.ty)
                        ),
                    ));
                }
                self.require_type(&right.ty, &left.ty, span)?;
                Type::Bool
            }
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                if !is_equality_comparable(&left.ty) {
                    return Err(Diagnostic::at(
                        self.source,
                        span,
                        format!(
                            "values of type `{}` cannot be compared",
                            self.display_type(&left.ty)
                        ),
                    ));
                }
                self.require_type(&right.ty, &left.ty, span)?;
                Type::Bool
            }
            BinaryOperator::And | BinaryOperator::Or => {
                self.require_type(&left.ty, &Type::Bool, span)?;
                self.require_type(&right.ty, &Type::Bool, span)?;
                Type::Bool
            }
        };
        Ok(Expr {
            kind: ir::ExprKind::Binary {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
        })
    }

    fn require_type(&self, actual: &Type, expected: &Type, span: Span) -> Result<(), Diagnostic> {
        // null 字面量（占位指针）可匹配任意指针类型（任一方向）。
        let null_matches = (is_null_ptr(actual) && expected.is_pointer())
            || (is_null_ptr(expected) && actual.is_pointer());
        // 同一层增加只读限定：`*T` -> `*const T`、`[]T` -> `[]const T`；反向是错误。
        let const_coercion = matches!(
            (actual, expected),
            (
                Type::Ptr {
                    pointee: a,
                    mutable: true
                },
                Type::Ptr {
                    pointee: b,
                    mutable: false
                }
            ) if a == b
        ) || matches!(
            (actual, expected),
            (
                Type::Slice {
                    element: a,
                    mutable: true
                },
                Type::Slice {
                    element: b,
                    mutable: false
                }
            ) if a == b
        );
        if actual == expected || null_matches || const_coercion {
            Ok(())
        } else {
            Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "expected `{}`, found `{}`",
                    self.display_type(expected),
                    self.display_type(actual)
                ),
            ))
        }
    }

    fn lookup(&self, name: &str, span: Span) -> Result<&Binding, Diagnostic> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.bindings.get(name))
            .ok_or_else(|| Diagnostic::at(self.source, span, format!("unknown variable `{name}`")))
    }

    fn new_local(&mut self, ty: Type) -> LocalId {
        let local = LocalId(self.locals.len());
        self.locals.push(ty);
        local
    }

    fn store_temporary(&mut self, value: Expr) -> LocalId {
        let local = self.new_local(value.ty.clone());
        self.emit(Instruction::SetLocal { local, value });
        local
    }

    fn with_scope<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        self.scopes.push(Scope::default());
        let result = operation(self);
        let result = result?;
        // 自然出块：若当前块尚未终止（未被 return/break/continue 结束），
        // 在此处逆序发出本作用域登记的 defer。
        if !self.is_terminated(self.current) {
            let index = self.scopes.len() - 1;
            self.emit_cleanups_from(index);
        }
        self.scopes.pop();
        Ok(result)
    }

    /// 从最内层到 `first_scope`（含）逆序发出各作用域的 defer 调用。
    ///
    /// 不 drain 元数据：同一条清理边可能被多个控制流路径复用，否则另一条分支
    /// 会漏清理。
    fn emit_cleanups_from(&mut self, first_scope: usize) {
        for index in (first_scope..self.scopes.len()).rev() {
            let defers = self.scopes[index].defers.clone();
            for call in defers.into_iter().rev() {
                self.emit(Instruction::Evaluate(call));
            }
        }
    }

    fn has_defers(&self) -> bool {
        self.scopes.iter().any(|scope| !scope.defers.is_empty())
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        let location = self.current_location;
        self.blocks.push(WorkingBlock {
            instructions: Vec::new(),
            terminator: None,
            reachable: false,
            location,
            locations: Vec::new(),
            terminator_location: location,
        });
        id
    }

    fn emit(&mut self, instruction: Instruction) {
        let location = self.current_location;
        let block = &mut self.blocks[self.current.0];
        block.instructions.push(instruction);
        block.locations.push(location);
    }

    fn terminate(&mut self, terminator: Terminator) {
        debug_assert!(self.blocks[self.current.0].terminator.is_none());
        if self.blocks[self.current.0].reachable {
            match &terminator {
                Terminator::Jump(target) => self.blocks[target.0].reachable = true,
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    self.blocks[then_block.0].reachable = true;
                    self.blocks[else_block.0].reachable = true;
                }
                Terminator::Return(_) => {}
            }
        }
        let location = self.current_location;
        let block = &mut self.blocks[self.current.0];
        block.terminator = Some(terminator);
        block.terminator_location = location;
    }

    fn is_terminated(&self, block: BlockId) -> bool {
        self.blocks[block.0].terminator.is_some()
    }

    /// 不可达块使用的零值，借用当前已实例化的类型定义。
    fn default_expr(&self, ty: Type) -> Expr {
        default_expr(ty, &self.state.borrow().types)
    }
}

enum FormatPart {
    Text(String),
    Placeholder,
}

fn parse_format(
    source: &SourceFile,
    format: &str,
    span: Span,
) -> Result<Vec<FormatPart>, Diagnostic> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut chars = format.chars().peekable();
    while let Some(character) = chars.next() {
        match (character, chars.peek().copied()) {
            ('{', Some('{')) => {
                chars.next();
                text.push('{');
            }
            ('}', Some('}')) => {
                chars.next();
                text.push('}');
            }
            ('{', Some('}')) => {
                chars.next();
                parts.push(FormatPart::Text(std::mem::take(&mut text)));
                parts.push(FormatPart::Placeholder);
            }
            ('{' | '}', _) => {
                return Err(Diagnostic::at(
                    source,
                    span,
                    "unmatched brace in format string",
                ));
            }
            _ => text.push(character),
        }
    }
    parts.push(FormatPart::Text(text));
    Ok(parts)
}

fn local_expr(local: LocalId, ty: Type) -> Expr {
    Expr {
        kind: ir::ExprKind::Local(local),
        ty,
    }
}

/// 不可达块中满足 IR 类型要求的零值。
///
/// 块永远不会执行，但后续代码生成仍要求返回值的分量数与返回类型一致，
/// 因此结构体/枚举/切片也必须构造出形状正确的零值。
fn default_expr(ty: Type, types: &[TypeDef]) -> Expr {
    match ty {
        Type::Unit => unreachable!("Unit returns do not need a value"),
        ty if ty.is_integer() => Expr {
            kind: ir::ExprKind::Integer(0),
            ty,
        },
        ty if ty.is_float() => Expr {
            kind: ir::ExprKind::Float(0.0),
            ty,
        },
        Type::Char => Expr {
            kind: ir::ExprKind::Char('\0'),
            ty,
        },
        Type::Bool => Expr {
            kind: ir::ExprKind::Bool(false),
            ty,
        },
        Type::String => Expr {
            kind: ir::ExprKind::String(String::new()),
            ty,
        },
        Type::Array { element, length } => Expr {
            kind: ir::ExprKind::RepeatArray {
                value: Box::new(default_expr(element.as_type(), types)),
                length,
            },
            ty,
        },
        Type::Struct(id) => {
            let fields = match &types[id.0] {
                TypeDef::Struct { fields, .. } => fields
                    .iter()
                    .map(|field| default_expr(field.ty.clone(), types))
                    .collect(),
                _ => unreachable!("struct type resolves a struct definition"),
            };
            Expr {
                kind: ir::ExprKind::StructInit { fields },
                ty,
            }
        }
        Type::Enum(id) => {
            let arguments = match &types[id.0] {
                TypeDef::Enum { variants } => variants
                    .first()
                    .map(|variant| {
                        variant
                            .fields
                            .iter()
                            .map(|field| default_expr(field.clone(), types))
                            .collect()
                    })
                    .unwrap_or_default(),
                _ => unreachable!("enum type resolves an enum definition"),
            };
            Expr {
                kind: ir::ExprKind::EnumInit {
                    variant: 0,
                    arguments,
                },
                ty,
            }
        }
        Type::Slice { ref element, .. } => Expr {
            kind: ir::ExprKind::MemAlloc {
                element: element.as_ref().clone(),
                count: Box::new(Expr::i32(0)),
            },
            ty,
        },
        Type::Ptr { .. } | Type::Null => Expr {
            kind: ir::ExprKind::Null,
            ty,
        },
        _ => unreachable!("all types have default zero values"),
    }
}

fn assignment_binary(operator: AssignmentOperator) -> BinaryOperator {
    match operator {
        AssignmentOperator::Add => BinaryOperator::Add,
        AssignmentOperator::Subtract => BinaryOperator::Subtract,
        AssignmentOperator::Multiply => BinaryOperator::Multiply,
        AssignmentOperator::Divide => BinaryOperator::Divide,
        AssignmentOperator::Remainder => BinaryOperator::Remainder,
        AssignmentOperator::Assign => unreachable!("plain assignment is handled separately"),
    }
}

fn lower_number(
    source: &SourceFile,
    raw: &str,
    negative: bool,
    span: Span,
) -> Result<Expr, Diagnostic> {
    let (digits, suffix) = raw.rsplit_once('_').unwrap_or((raw, ""));
    let float_literal = digits.contains(['.', 'e', 'E']);
    if float_literal || matches!(suffix, "f32" | "f64") {
        let ty = match suffix {
            "f32" => Type::F32,
            "" => Type::F64,
            "f64" => Type::F64,
            _ => {
                return Err(Diagnostic::at(
                    source,
                    span,
                    format!("invalid numeric suffix `{suffix}`"),
                ));
            }
        };
        let mut value = digits
            .parse::<f64>()
            .map_err(|_| Diagnostic::at(source, span, "invalid floating-point literal"))?;
        if negative {
            value = -value;
        }
        if !value.is_finite() || ty == Type::F32 && !(value as f32).is_finite() {
            return Err(Diagnostic::at(
                source,
                span,
                format!("literal does not fit in `{ty}`"),
            ));
        }
        return Ok(Expr {
            kind: ir::ExprKind::Float(value),
            ty,
        });
    }
    let ty = match suffix {
        "" | "i32" => Type::I32,
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i64" => Type::I64,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        "usize" => Type::Usize,
        "isize" => Type::Isize,
        _ => {
            return Err(Diagnostic::at(
                source,
                span,
                format!("invalid numeric suffix `{suffix}`"),
            ));
        }
    };
    if negative && !ty.is_signed_integer() {
        return Err(Diagnostic::at(
            source,
            span,
            format!("cannot negate `{ty}` literal"),
        ));
    }
    let magnitude = digits
        .parse::<u64>()
        .map_err(|_| Diagnostic::at(source, span, "integer literal is too large"))?;
    let bits = ty.bits().unwrap();
    let maximum = if ty.is_signed_integer() {
        (1u128 << (bits - 1)) - 1
    } else if bits == 64 {
        u64::MAX as u128
    } else {
        (1u128 << bits) - 1
    };
    let allowed = maximum + u128::from(negative);
    if u128::from(magnitude) > allowed {
        return Err(Diagnostic::at(
            source,
            span,
            format!("literal does not fit in `{ty}`"),
        ));
    }
    let value = if negative {
        0u64.wrapping_sub(magnitude)
    } else {
        magnitude
    };
    Ok(Expr {
        kind: ir::ExprKind::Integer(value),
        ty,
    })
}

fn is_castable(ty: Type) -> bool {
    ty.is_integer() || ty.is_float() || ty == Type::Char
}

/// 支持 `==` / `!=` 的类型：标量、字符串（按内容）与裸指针（按地址）。
/// 结构体、枚举、切片与数组需要逐分量比较，当前后端只比较首个分量，必须拒绝。
fn is_equality_comparable(ty: &Type) -> bool {
    ty.is_integer()
        || ty.is_float()
        || matches!(
            ty,
            Type::Bool | Type::Char | Type::String | Type::Ptr { .. } | Type::Null
        )
}

/// `println` / `print` 占位符支持的类型：标量与字符串。
fn is_printable(ty: &Type) -> bool {
    ty.is_integer() || ty.is_float() || matches!(ty, Type::Bool | Type::Char | Type::String)
}

/// 判断是否为 `null` 字面量的占位类型。
/// 若表达式是形如 `Enum.Variant` 的枚举成员引用，返回变体名。
/// 调用形式 `Enum.Variant(...)` 由 `call_variant_for_enum` 校验前缀后处理。
fn enum_member_name(expression: &ast::Expr) -> Option<String> {
    match &expression.kind {
        ExprKind::Field { base, field, .. } => match &base.kind {
            ExprKind::Name(_) => Some(field.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn is_null_ptr(ty: &Type) -> bool {
    matches!(ty, Type::Null)
}

/// 已知期望枚举时，判断调用形式 `Enum.Variant(...)` / `Variant(...)` 是否指向该枚举。
///
/// 带模块前缀的 callee 必须与枚举限定名一致，避免把同名普通函数误判为枚举构造。
fn call_variant_for_enum(callee: &str, enum_name: &str) -> Option<String> {
    match callee.rsplit_once('.') {
        Some((prefix, variant)) if prefix == enum_name => Some(variant.to_string()),
        Some(_) => None,
        None => Some(callee.to_string()),
    }
}

fn parse_unsuffixed_integer(value: &str) -> Option<i64> {
    (!value.contains('_') && !value.contains(['.', 'e', 'E']))
        .then(|| value.parse::<i64>().ok())
        .flatten()
}

fn check_constant_index(
    source: &SourceFile,
    index: &ast::Expr,
    length: usize,
) -> Result<(), Diagnostic> {
    let constant = match &index.kind {
        ExprKind::Number(value) => parse_unsuffixed_integer(value),
        ExprKind::Unary {
            operator: UnaryOperator::Negate,
            operand,
        } => match &operand.kind {
            ExprKind::Number(value) => parse_unsuffixed_integer(value).map(|value| -value),
            _ => None,
        },
        _ => None,
    };
    if constant.is_some_and(|value| value < 0 || value as usize >= length) {
        Err(Diagnostic::at(
            source,
            index.span,
            format!("array index is out of bounds for length {length}"),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use dolphin_source::lexer;
    use dolphin_syntax::parser;

    use super::*;

    fn lower_text(text: &str) -> Result<ir::Program, Diagnostic> {
        let source = SourceFile::new(PathBuf::from("main.do"), text.to_string());
        let tokens = lexer::lex(&source)?;
        let ast = parser::parse(&source, tokens)?;
        lower(&source, &ast)
    }

    #[test]
    fn lowers_forward_function_calls() {
        let program = lower_text(
            "fn main() { return add(2, 3); } fn add(a: i32, b: i32): i32 { return a + b; }",
        )
        .unwrap();
        // 单文件模式现在也注入源码标准库（M15-B），因此只断言用户函数存在。
        assert_eq!(
            program
                .functions
                .iter()
                .filter(|function| matches!(function.name.as_str(), "main" | "add"))
                .count(),
            2
        );
    }

    #[test]
    fn rejects_wrong_call_arguments() {
        let error = lower_text("fn add(a: i32): i32 { return a; } fn main() { return add(true); }")
            .unwrap_err();
        assert!(error.to_string().contains("expected `i32`, found `bool`"));
    }

    #[test]
    fn validates_print_placeholders() {
        lower_text("fn main() { println(\"{} = {}\", \"answer\", 42); }").unwrap();
        let error = lower_text("fn main() { println(\"{} {}\", 1); }").unwrap_err();
        assert!(error.to_string().contains("2 placeholders"));
    }

    #[test]
    fn rejects_duplicate_functions_and_missing_returns() {
        let duplicate = lower_text("fn main() {} fn main() {}").unwrap_err();
        assert!(duplicate.to_string().contains("already defined"));

        let missing =
            lower_text("fn value(flag: bool): i32 { if flag { return 1; } } fn main() {}")
                .unwrap_err();
        assert!(
            missing
                .to_string()
                .contains("may exit without returning `i32`")
        );
    }

    #[test]
    fn rejects_assignment_to_val() {
        let error = lower_text("fn main() { val answer = 42; answer = 0; }").unwrap_err();
        assert!(error.to_string().contains("immutable variable"));
    }

    #[test]
    fn collecting_lowering_reports_independent_declarations() {
        let source = SourceFile::new(
            PathBuf::from("main.do"),
            "impl Missing0 { }\nimpl Missing1 { }\nfn main() { return 0; }\n".to_string(),
        );
        let tokens = lexer::lex(&source).unwrap();
        let ast = parser::parse(&source, tokens).unwrap();
        let loaded = crate::modules::inject_stdlib(&source, &ast).unwrap();
        let errors = lower_sources_analysis_collecting(
            &loaded.sources,
            &loaded.program,
            &loaded.packages,
            true,
        )
        .unwrap_err();
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].message().contains("unknown type `Missing0`"));
        assert!(errors[1].message().contains("unknown type `Missing1`"));
    }

    #[test]
    fn analysis_side_table_aligns_with_type_and_function_ids() {
        let source = SourceFile::new(
            PathBuf::from("main.do"),
            "struct Pair<T> { first: T, second: T }\nfn id<T>(value: T): T { return value; }\nfn main() { val p = Pair<i32>(1, 2); return id<i32>(p.first); }\n"
                .to_string(),
        );
        let tokens = lexer::lex(&source).unwrap();
        let ast = parser::parse(&source, tokens).unwrap();
        let loaded = crate::modules::inject_stdlib(&source, &ast).unwrap();
        let lowered = lower_sources_analysis(&loaded.sources, &loaded.program, &loaded.packages)
            .expect("analysis lowering");

        // side table 下标与 IR 编号一一对应。
        assert_eq!(
            lowered.analysis.type_names.len(),
            lowered.program.types.len()
        );
        assert_eq!(
            lowered.analysis.function_instances.len(),
            lowered.program.functions.len()
        );
        for (index, key) in lowered.analysis.type_names.iter().enumerate() {
            let ty = &lowered.program.types[index];
            assert!(!key.name.is_empty());
            let _ = ty;
        }
        for (index, key) in lowered.analysis.function_instances.iter().enumerate() {
            let ir_name = &lowered.program.functions[index].name;
            assert!(
                key.name == *ir_name || key.name.ends_with(&format!("::{ir_name}")),
                "function_instances[{index}] `{}` 必须对应 IR 函数 `{ir_name}`",
                key.name
            );
        }

        // 定义表含类型、泛型函数与类型参数，且限定名可读。
        let definition = |qualified: &str| {
            lowered
                .analysis
                .definitions
                .iter()
                .find(|definition| definition.qualified == qualified)
                .unwrap_or_else(|| panic!("missing definition `{qualified}`"))
        };
        assert_eq!(definition("Pair").kind, DefinitionKind::Struct);
        assert_eq!(definition("Pair.T").kind, DefinitionKind::TypeParam);
        assert_eq!(definition("id").kind, DefinitionKind::Function);
        assert_eq!(definition("main").kind, DefinitionKind::Function);
        assert!(
            lowered
                .analysis
                .type_names
                .iter()
                .any(|key| key.name == "Pair" && key.args == vec![ir::Type::I32])
        );
    }

    #[test]
    fn collecting_lowering_keeps_first_body_error() {
        let source = SourceFile::new(
            PathBuf::from("main.do"),
            "fn main() { val a: i32 = true; val b: i32 = \"x\"; return; }\n".to_string(),
        );
        let tokens = lexer::lex(&source).unwrap();
        let ast = parser::parse(&source, tokens).unwrap();
        let loaded = crate::modules::inject_stdlib(&source, &ast).unwrap();
        let errors = lower_sources_analysis_collecting(
            &loaded.sources,
            &loaded.program,
            &loaded.packages,
            true,
        )
        .unwrap_err();
        assert_eq!(errors.len(), 1, "函数体 lowering 仍首错即停：{errors:?}");
        assert!(errors[0].message().contains("expected `i32`, found `bool`"));
    }

    #[test]
    fn requires_boolean_conditions() {
        let error = lower_text("fn main() { if 1 { return 1; } }").unwrap_err();
        assert!(error.to_string().contains("expected `bool`"));
    }

    #[test]
    fn rejects_loop_control_outside_loop() {
        let break_error = lower_text("fn main() { break; }").unwrap_err();
        assert!(break_error.to_string().contains("outside a loop"));
    }

    #[test]
    fn enforces_block_scope() {
        let error =
            lower_text("fn main() { if true { val scoped = 1; } return scoped; }").unwrap_err();
        assert!(error.to_string().contains("unknown variable `scoped`"));
    }

    #[test]
    fn validates_m6_array_types_and_mutability() {
        lower_text(
            "fn copy(value: [i32; 2]): [i32; 2] { return value; } fn main() { var a = copy([1, 2]); a[0] = 3; for x in a { println(\"{}\", x); } }",
        )
        .unwrap();

        let immutable =
            lower_text("fn main() { val values = [1, 2]; values[0] = 3; }").unwrap_err();
        assert!(immutable.to_string().contains("immutable array"));

        let mismatch =
            lower_text("fn take(value: [i32; 2]) {} fn main() { take([1, 2, 3]); }").unwrap_err();
        assert!(mismatch.to_string().contains("expected `[i32; 2]`"));

        let bounds =
            lower_text("fn main() { val values = [1, 2]; return values[-1]; }").unwrap_err();
        assert!(bounds.to_string().contains("out of bounds"));
    }

    #[test]
    fn m13_lowers_structs_and_enums() {
        lower_text(
            "struct Point { x: i32, y: i32 } enum Shape { Circle(f64), Rectangle(f64, f64), Empty } fn main() { var p = Point(1, 2); val c = Shape.Circle(2.0); val area = match c { Shape.Circle(r) => r, Shape.Rectangle(w, h) => w + h, Shape.Empty => 0.0 }; return p.x + area as i32; }",
        )
        .unwrap();
    }

    #[test]
    fn m14_accepts_user_types_in_function_signatures() {
        lower_text(
            "struct Point { x: i32 } fn area(p: Point): i32 { return p.x; } fn main() { return area(Point(1)); }",
        )
        .unwrap();

        lower_text(
            "struct Point { x: i32 } fn make(): Point { return Point(1); } fn main() { return make().x; }",
        )
        .unwrap();
    }

    #[test]
    fn m14_rejects_recursive_layout() {
        let error = lower_text("struct Node { value: i32, next: Node } fn main() { return 0; }")
            .unwrap_err();
        assert!(error.to_string().contains("recursive layout"));
    }

    #[test]
    fn m14_lowers_pointer_and_slice_signatures() {
        lower_text(
            "fn take(s: []u8, cs: []const u8, p: *i32, cp: *const i32) {} fn main() { return 0; }",
        )
        .unwrap();
    }

    #[test]
    fn m14_rejects_deref_non_pointer() {
        let error = lower_text("fn main() { val x = 5; return *x; }").unwrap_err();
        assert!(error.to_string().contains("cannot dereference"));
    }

    #[test]
    fn m13_rejects_unknown_fields_and_non_exhaustive_match() {
        let field =
            lower_text("struct Point { x: i32 } fn main() { var p = Point(1); return p.y; }")
                .unwrap_err();
        assert!(field.to_string().contains("no field named `y`"));

        let exhaustive = lower_text(
            "enum Shape { Circle(f64), Rectangle(f64, f64) } fn main() { val s = Shape.Circle(1.0); return match s { Shape.Circle(r) => 1 }; }",
        )
        .unwrap_err();
        assert!(exhaustive.to_string().contains("missing variant"));
    }

    #[test]
    fn m15_lowers_generic_functions_and_types() {
        lower_text(
            "fn identity<T>(value: T): T { return value; } fn main() { val a = identity<i32>(3); val b = identity(a); return a + b; }",
        )
        .unwrap();
        lower_text(
            "struct Pair<T> { first: T, second: T } fn main() { val p = Pair<i32>(1, 2); return p.first + p.second; }",
        )
        .unwrap();
        lower_text(
            "enum Option<T> { Some(T), None } fn main() { val x: Option<i32> = Option.None; return match x { Option.Some(v) => v, Option.None => 0 }; }",
        )
        .unwrap();
        // 从字段访问结果推断（GEN-01）。
        lower_text(
            "struct P { x: i32 } fn identity<T>(value: T): T { return value; } fn main() { val p = P(3); return identity(p.x); }",
        )
        .unwrap();
    }

    #[test]
    fn m15_dedups_instances_and_generates_distinct_symbols() {
        let program = lower_text(
            "fn identity<T>(value: T): T { return value; } fn main() { return identity<i32>(1) + identity<u8>(2_u8) as i32 + identity<u8>(3_u8) as i32; }",
        )
        .unwrap();
        let mut names: Vec<&str> = program
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .collect();
        names.sort_unstable();
        // 两个 identity 实例（i32、u8）各生成一次。
        assert_eq!(names.iter().filter(|name| **name == "identity").count(), 2);
    }

    #[test]
    fn m15_rejects_unbounded_generic_recursion() {
        let error = lower_text(
            "fn loop_on<T>(): i32 { return loop_on<Pair<T>>(); } struct Pair<T> { first: T } fn main() { return loop_on<i32>(); }",
        )
        .unwrap_err();
        assert!(error.to_string().contains("depth limit"));
    }

    #[test]
    fn m15_allows_pointer_recursive_generic_layout() {
        lower_text(
            "struct Node<T> { value: T, next: *Node<T> } fn main() { var n = Node<i32>(1, null); return n.value; }",
        )
        .unwrap();
    }

    #[test]
    fn m15_lowers_inherent_methods_and_receivers() {
        lower_text(
            "struct Point { x: i32, y: i32 } impl Point { fn origin(): Point { return Point(0, 0); } fn sum(self): i32 { return self.x + self.y; } fn set_x(self: *Self, value: i32) { self->x = value; } fn read_x(self: *const Self): i32 { return self->x; } } fn main() { var p = Point::origin(); p.set_x(3); val q = Point(1, 2); return p.read_x() + q.sum(); }",
        )
        .unwrap();
    }

    #[test]
    fn m15_lowers_generic_impl_and_trait() {
        lower_text(
            "struct Box<T> { value: T } impl<T> Box<T> { fn get(self: *const Self): T { return self->value; } fn set(self: *Self, value: T) { self->value = value; } } fn main() { var b = Box<i32>(1); b.set(2); return b.get(); }",
        )
        .unwrap();
        lower_text(
            "trait Measured { type Item; fn measure(self: *const Self): i32; } struct Square { side: i32 } impl Measured for Square { type Item = i32; fn measure(self: *const Self): i32 { return self->side; } } fn main() { val s = Square(5); return s.measure(); }",
        )
        .unwrap();
    }

    #[test]
    fn m15_reports_missing_trait_members_and_unknown_methods() {
        let missing = lower_text(
            "trait T { fn f(self: *const Self): i32; } struct S { x: i32 } impl T for S { } fn main() { return 0; }",
        )
        .unwrap_err();
        assert!(missing.to_string().contains("missing"));

        let unknown =
            lower_text("struct S { x: i32 } fn main() { val s = S(1); return s.nope(); }")
                .unwrap_err();
        assert!(unknown.to_string().contains("has no method"));

        let duplicate = lower_text(
            "struct S { x: i32 } impl S { fn f(self): i32 { return self.x; } } impl S { fn f(self): i32 { return self.x; } } fn main() { return 0; }",
        )
        .unwrap_err();
        assert!(duplicate.to_string().contains("already defined"));
    }

    #[test]
    fn m15_lowers_explicit_generic_enum_member() {
        lower_text(
            "enum Maybe<T> { Just(T), Nothing } fn main() { val x = Maybe<i32>.Just(3); return match x { Maybe.Just(v) => v, Maybe.Nothing => 0 }; }",
        )
        .unwrap();
    }

    #[test]
    fn m15_lowers_nested_templates_and_reports_unknown_expected_type() {
        // 模板调用模板，且嵌套泛型实例。
        lower_text(
            "struct Pair<T> { first: T, second: T } fn pair<T>(a: T, b: T): Pair<T> { return Pair<T>(a, b); } fn identity<T>(value: T): T { return value; } fn main() { val p = pair<i32>(1, 2); val q = identity<Pair<i32>>(p); return q.first + q.second; }",
        )
        .unwrap();

        // `Maybe.Nothing` 缺少可知的期望类型时给出诊断。
        let error = lower_text(
            "enum Maybe<T> { Just(T), Nothing } fn main() { val x = Maybe.Nothing; return 0; }",
        )
        .unwrap_err();
        assert!(error.to_string().contains("cannot infer"));
    }

    #[test]
    fn m15_resolves_bound_associated_types() {
        lower_text(
            "trait Has { type Item; } struct Box { v: i32 } impl Has for Box { type Item = i32; } fn take<C: Has>(value: C::Item): C::Item { return value; } fn main() { return take<Box>(7); }",
        )
        .unwrap();
    }

    #[test]
    fn m15_rejects_trait_signature_mismatch() {
        let mismatch = lower_text(
            "trait T { fn f(self: *const Self): i32; } struct S { x: i32 } impl T for S { fn f(self: *const Self): bool { return true; } } fn main() { return 0; }",
        )
        .unwrap_err();
        assert!(mismatch.to_string().contains("does not match"));

        // 关联类型绑定后签名一致：`Self::Out` 等价于 `i32`。
        lower_text(
            "trait Get { type Out; fn get(self: *const Self): Self::Out; } struct B { v: i32 } impl Get for B { type Out = i32; fn get(self: *const Self): i32 { return self->v; } } fn main() { val b = B(1); return b.get(); }",
        )
        .unwrap();
    }

    #[test]
    fn m15_rejects_missing_bound_impl() {
        let error = lower_text(
            "trait Has { type Item; } struct Box { v: i32 } fn take<C: Has>(value: C): i32 { return 0; } fn main() { val b = Box(0); return take(b); }",
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not implement"));
    }

    #[test]
    fn m15_rejects_by_value_recursive_generic_layout() {
        let error = lower_text(
            "struct Bad<T> { next: Bad<T> } fn consume(p: *Bad<i32>) {} fn main() { return 0; }",
        )
        .unwrap_err();
        assert!(error.to_string().contains("recursive layout"));
    }
}
