use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::{self, ExprKind, StatementKind};
use crate::diagnostic::Diagnostic;
use crate::source::SourceFile;
use crate::{lexer, parser};

pub struct LoadedProgram {
    pub sources: Vec<SourceFile>,
    pub program: ast::Program,
}

struct Unit {
    source_id: usize,
    module: String,
    program: ast::Program,
}

#[derive(Clone)]
struct FunctionInfo {
    module: String,
    public: bool,
}

#[derive(Clone)]
enum ImportBinding {
    Module(String),
    Function(String),
}

pub fn load_project(project: &Path) -> Result<LoadedProgram, Diagnostic> {
    load_sources(&project.join("src"), &HashSet::new())
}

/// 从指定源码根加载项目，跳过 `exclude` 中的入口文件。
///
/// M9 多可执行目标下，每个 `[[bin]]` 拥有独立入口文件；编译其中一个
/// 目标时需要排除其余目标的入口，从而保证每个编译单元只有一个 `main`。
pub fn load_sources(
    source_root: &Path,
    exclude: &HashSet<PathBuf>,
) -> Result<LoadedProgram, Diagnostic> {
    if !source_root.is_dir() {
        return Err(Diagnostic::plain(format!(
            "project does not contain a source directory `{}`",
            source_root.display()
        )));
    }
    let mut paths = Vec::new();
    discover_sources(source_root, &mut paths)?;
    paths.retain(|path| !exclude.contains(path));
    paths.sort();
    if paths.is_empty() {
        return Err(Diagnostic::plain(format!(
            "source directory `{}` does not contain any `.do` files",
            source_root.display()
        )));
    }

    let mut sources = Vec::with_capacity(paths.len());
    let mut units = Vec::with_capacity(paths.len());
    for path in paths {
        let text = fs::read_to_string(&path).map_err(|error| {
            Diagnostic::plain(format!("could not read `{}`: {error}", path.display()))
        })?;
        let source = SourceFile::new(path.clone(), text);
        let tokens = lexer::lex(&source)?;
        let mut program = parser::parse(&source, tokens)?;
        let module = expected_module(source_root, &path)?;
        validate_package(&source, &program, &module)?;
        let source_id = sources.len();
        for function in &mut program.functions {
            function.source_id = source_id;
        }
        for structure in &mut program.structs {
            structure.source_id = source_id;
        }
        for enumeration in &mut program.enums {
            enumeration.source_id = source_id;
        }
        sources.push(source);
        units.push(Unit {
            source_id,
            module,
            program,
        });
    }

    resolve_modules(&sources, &mut units)?;
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut enums = Vec::new();
    for unit in &mut units {
        functions.append(&mut unit.program.functions);
        structs.append(&mut unit.program.structs);
        enums.append(&mut unit.program.enums);
    }
    Ok(LoadedProgram {
        sources,
        program: ast::Program {
            package: None,
            uses: Vec::new(),
            functions,
            structs,
            enums,
        },
    })
}

fn discover_sources(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), Diagnostic> {
    let entries = fs::read_dir(directory).map_err(|error| {
        Diagnostic::plain(format!(
            "could not read directory `{}`: {error}",
            directory.display()
        ))
    })?;
    for entry in entries {
        let entry = entry
            .map_err(|error| Diagnostic::plain(format!("could not read source entry: {error}")))?;
        let file_type = entry.file_type().map_err(|error| {
            Diagnostic::plain(format!(
                "could not inspect `{}`: {error}",
                entry.path().display()
            ))
        })?;
        if file_type.is_dir() {
            discover_sources(&entry.path(), paths)?;
        } else if file_type.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "do")
        {
            paths.push(entry.path());
        }
    }
    Ok(())
}

fn expected_module(source_root: &Path, path: &Path) -> Result<String, Diagnostic> {
    let relative = path.strip_prefix(source_root).map_err(|_| {
        Diagnostic::plain(format!(
            "source `{}` is outside the source root",
            path.display()
        ))
    })?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    if parent.as_os_str().is_empty() {
        return Ok(String::new());
    }
    let mut segments: Vec<String> = parent
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    segments.push(
        relative
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| Diagnostic::plain("module file name must be valid UTF-8"))?
            .to_string(),
    );
    Ok(segments.join("."))
}

fn validate_package(
    source: &SourceFile,
    program: &ast::Program,
    expected: &str,
) -> Result<(), Diagnostic> {
    match (&program.package, expected.is_empty()) {
        (None, true) => Ok(()),
        (Some(package), true) => Err(Diagnostic::at(
            source,
            package.span,
            "files directly under `src` belong to the root module and must omit `pkg`",
        )),
        (None, false) => Err(Diagnostic::plain(format!(
            "source `{}` must declare `pkg {expected};`",
            source.path.display()
        ))),
        (Some(package), false) => {
            let actual = package.segments.join(".");
            if actual == expected {
                Ok(())
            } else {
                Err(Diagnostic::at(
                    source,
                    package.span,
                    format!("package path `{actual}` does not match file module `{expected}`"),
                ))
            }
        }
    }
}

fn resolve_modules(sources: &[SourceFile], units: &mut [Unit]) -> Result<(), Diagnostic> {
    let modules: HashSet<String> = units.iter().map(|unit| unit.module.clone()).collect();
    let mut functions = HashMap::new();
    for unit in units.iter() {
        let source = &sources[unit.source_id];
        for function in &unit.program.functions {
            if matches!(function.name.as_str(), "print" | "println" | "length") {
                return Err(Diagnostic::at(
                    source,
                    function.name_span,
                    format!("`{}` is a reserved built-in function", function.name),
                ));
            }
            let qualified = qualify(&unit.module, &function.name);
            if functions
                .insert(
                    qualified.clone(),
                    FunctionInfo {
                        module: unit.module.clone(),
                        public: function.public,
                    },
                )
                .is_some()
            {
                return Err(Diagnostic::at(
                    source,
                    function.name_span,
                    format!("function `{qualified}` is already defined"),
                ));
            }
        }
    }

    // 收集并校验用户自定义类型（struct/enum），构建全限定名到可见性信息的映射。
    let mut type_infos = HashMap::new();
    for unit in units.iter() {
        let source = &sources[unit.source_id];
        for structure in &unit.program.structs {
            let qualified = qualify(&unit.module, &structure.name);
            if type_infos
                .insert(
                    qualified.clone(),
                    TypeInfo {
                        module: unit.module.clone(),
                        public: structure.public,
                    },
                )
                .is_some()
            {
                return Err(Diagnostic::at(
                    source,
                    structure.name_span,
                    format!("type `{qualified}` is already defined"),
                ));
            }
        }
        for enumeration in &unit.program.enums {
            let qualified = qualify(&unit.module, &enumeration.name);
            if type_infos
                .insert(
                    qualified.clone(),
                    TypeInfo {
                        module: unit.module.clone(),
                        public: enumeration.public,
                    },
                )
                .is_some()
            {
                return Err(Diagnostic::at(
                    source,
                    enumeration.name_span,
                    format!("type `{qualified}` is already defined"),
                ));
            }
        }
    }
    for unit in units.iter_mut() {
        let source = &sources[unit.source_id];
        let bindings = resolve_imports(source, unit, &modules, &functions)?;
        let local_names: HashSet<String> = unit
            .program
            .functions
            .iter()
            .map(|function| function.name.clone())
            .collect();
        for binding in bindings.keys() {
            if local_names.contains(binding) {
                return Err(Diagnostic::plain(format!(
                    "import `{binding}` conflicts with a function in module `{}`",
                    display_module(&unit.module)
                )));
            }
        }
        // 先 qualify 类型声明自身的名字与其字段类型。
        for structure in &mut unit.program.structs {
            structure.name = qualify(&unit.module, &structure.name);
            for field in &mut structure.fields {
                resolve_type_ref(source, &unit.module, &bindings, &type_infos, &mut field.ty)?;
            }
        }
        for enumeration in &mut unit.program.enums {
            enumeration.name = qualify(&unit.module, &enumeration.name);
            for variant in &mut enumeration.variants {
                for field in &mut variant.fields {
                    resolve_type_ref(source, &unit.module, &bindings, &type_infos, field)?;
                }
            }
        }
        for function in &mut unit.program.functions {
            // M14 起结构体/枚举可作为参数与返回类型，需 qualify 签名中的类型引用。
            for parameter in &mut function.parameters {
                resolve_type_ref(
                    source,
                    &unit.module,
                    &bindings,
                    &type_infos,
                    &mut parameter.ty,
                )?;
            }
            if let Some(return_type) = &mut function.return_type {
                resolve_type_ref(source, &unit.module, &bindings, &type_infos, return_type)?;
            }
            resolve_block(
                source,
                &unit.module,
                &local_names,
                &bindings,
                &functions,
                &type_infos,
                &mut function.body,
            )?;
            function.name = qualify(&unit.module, &function.name);
        }
    }
    Ok(())
}

#[derive(Clone)]
struct TypeInfo {
    module: String,
    public: bool,
}

fn resolve_imports(
    source: &SourceFile,
    unit: &Unit,
    modules: &HashSet<String>,
    functions: &HashMap<String, FunctionInfo>,
) -> Result<HashMap<String, ImportBinding>, Diagnostic> {
    let mut bindings = HashMap::new();
    for import in &unit.program.uses {
        let path = import.segments.join(".");
        let (name, binding) = if modules.contains(&path) {
            (
                import.segments.last().unwrap().clone(),
                ImportBinding::Module(path),
            )
        } else if let Some((module, name)) = path.rsplit_once('.') {
            let info = functions.get(&path).ok_or_else(|| {
                Diagnostic::at(source, import.span, format!("unknown import `{path}`"))
            })?;
            if info.module != unit.module && !info.public {
                return Err(Diagnostic::at(
                    source,
                    import.span,
                    format!("function `{path}` is private"),
                ));
            }
            debug_assert_eq!(info.module, module);
            (name.to_string(), ImportBinding::Function(path))
        } else {
            return Err(Diagnostic::at(
                source,
                import.span,
                format!("unknown import `{path}`"),
            ));
        };
        if bindings.insert(name.clone(), binding).is_some() {
            return Err(Diagnostic::at(
                source,
                import.span,
                format!("import name `{name}` is already defined"),
            ));
        }
    }
    Ok(bindings)
}

fn resolve_block(
    source: &SourceFile,
    module: &str,
    locals: &HashSet<String>,
    imports: &HashMap<String, ImportBinding>,
    functions: &HashMap<String, FunctionInfo>,
    type_infos: &HashMap<String, TypeInfo>,
    statements: &mut [ast::Statement],
) -> Result<(), Diagnostic> {
    for statement in statements {
        match &mut statement.kind {
            StatementKind::Variable { initializer, .. }
            | StatementKind::Assignment {
                value: initializer, ..
            } => resolve_expr(
                source,
                module,
                locals,
                imports,
                functions,
                type_infos,
                initializer,
            )?,
            StatementKind::IndexAssignment { index, value, .. } => {
                resolve_expr(
                    source, module, locals, imports, functions, type_infos, index,
                )?;
                resolve_expr(
                    source, module, locals, imports, functions, type_infos, value,
                )?;
            }
            StatementKind::DerefAssignment { target, value, .. } => {
                resolve_expr(
                    source, module, locals, imports, functions, type_infos, target,
                )?;
                resolve_expr(
                    source, module, locals, imports, functions, type_infos, value,
                )?;
            }
            StatementKind::PtrFieldAssignment { base, value, .. } => {
                resolve_expr(source, module, locals, imports, functions, type_infos, base)?;
                resolve_expr(
                    source, module, locals, imports, functions, type_infos, value,
                )?;
            }
            StatementKind::Defer { value } => resolve_expr(
                source, module, locals, imports, functions, type_infos, value,
            )?,
            StatementKind::Try { resources, body } => {
                for resource in resources {
                    resolve_expr(
                        source,
                        module,
                        locals,
                        imports,
                        functions,
                        type_infos,
                        &mut resource.initializer,
                    )?;
                }
                resolve_block(source, module, locals, imports, functions, type_infos, body)?;
            }
            StatementKind::Expression(expression) => resolve_expr(
                source, module, locals, imports, functions, type_infos, expression,
            )?,
            StatementKind::If {
                condition,
                then_block,
                else_block,
            } => {
                resolve_expr(
                    source, module, locals, imports, functions, type_infos, condition,
                )?;
                resolve_block(
                    source, module, locals, imports, functions, type_infos, then_block,
                )?;
                if let Some(block) = else_block {
                    resolve_block(
                        source, module, locals, imports, functions, type_infos, block,
                    )?;
                }
            }
            StatementKind::Loop(block) => resolve_block(
                source, module, locals, imports, functions, type_infos, block,
            )?,
            StatementKind::While { condition, body } => {
                resolve_expr(
                    source, module, locals, imports, functions, type_infos, condition,
                )?;
                resolve_block(source, module, locals, imports, functions, type_infos, body)?;
            }
            StatementKind::For { iterable, body, .. } => {
                match iterable {
                    ast::ForIterable::Range { start, end, .. } => {
                        resolve_expr(
                            source, module, locals, imports, functions, type_infos, start,
                        )?;
                        resolve_expr(source, module, locals, imports, functions, type_infos, end)?;
                    }
                    ast::ForIterable::Array(array) => resolve_expr(
                        source, module, locals, imports, functions, type_infos, array,
                    )?,
                }
                resolve_block(source, module, locals, imports, functions, type_infos, body)?;
            }
            StatementKind::Return(Some(value)) => resolve_expr(
                source, module, locals, imports, functions, type_infos, value,
            )?,
            StatementKind::Break | StatementKind::Continue | StatementKind::Return(None) => {}
        }
    }
    Ok(())
}

fn resolve_expr(
    source: &SourceFile,
    module: &str,
    locals: &HashSet<String>,
    imports: &HashMap<String, ImportBinding>,
    functions: &HashMap<String, FunctionInfo>,
    type_infos: &HashMap<String, TypeInfo>,
    expression: &mut ast::Expr,
) -> Result<(), Diagnostic> {
    match &mut expression.kind {
        ExprKind::Array(values) => {
            for value in values {
                resolve_expr(
                    source, module, locals, imports, functions, type_infos, value,
                )?;
            }
        }
        ExprKind::RepeatArray { value, .. } | ExprKind::Unary { operand: value, .. } => {
            resolve_expr(
                source, module, locals, imports, functions, type_infos, value,
            )?
        }
        ExprKind::Cast { value, ty } => {
            resolve_expr(
                source, module, locals, imports, functions, type_infos, value,
            )?;
            resolve_type_ref(source, module, imports, type_infos, ty)?;
        }
        ExprKind::Index { array, index }
        | ExprKind::Binary {
            left: array,
            right: index,
            ..
        } => {
            resolve_expr(
                source, module, locals, imports, functions, type_infos, array,
            )?;
            resolve_expr(
                source, module, locals, imports, functions, type_infos, index,
            )?;
        }
        ExprKind::Call {
            callee,
            callee_span,
            arguments,
        } => {
            for argument in arguments {
                resolve_expr(
                    source, module, locals, imports, functions, type_infos, argument,
                )?;
            }
            if matches!(
                callee.as_str(),
                "print" | "println" | "length" | "allocate" | "free"
            ) {
                return Ok(());
            }
            // 结构体/枚举构造的 callee 是类型名或枚举项名：qualify 后不按函数解析。
            if is_constructor_target(module, callee, imports, type_infos) {
                *callee = qualify_constructor(module, imports, type_infos, callee);
                return Ok(());
            }
            let resolved = resolve_callee(module, locals, imports, callee);
            let info = functions.get(&resolved).ok_or_else(|| {
                Diagnostic::at(source, *callee_span, format!("unknown function `{callee}`"))
            })?;
            if info.module != module && !info.public {
                return Err(Diagnostic::at(
                    source,
                    *callee_span,
                    format!("function `{resolved}` is private"),
                ));
            }
            *callee = resolved;
        }
        ExprKind::Field { base, .. } | ExprKind::PtrField { base, .. } => {
            resolve_expr(source, module, locals, imports, functions, type_infos, base)?;
        }
        ExprKind::AddressOf { operand } | ExprKind::Deref { operand } => {
            resolve_expr(
                source, module, locals, imports, functions, type_infos, operand,
            )?;
        }
        ExprKind::Match { value, arms } => {
            resolve_expr(
                source, module, locals, imports, functions, type_infos, value,
            )?;
            for arm in arms {
                if let ast::MatchPattern::Enum { name, .. } = &mut arm.pattern {
                    *name = qualify_enum_variant(module, imports, type_infos, name);
                }
                resolve_expr(
                    source,
                    module,
                    locals,
                    imports,
                    functions,
                    type_infos,
                    &mut arm.body,
                )?;
            }
        }
        ExprKind::Number(_)
        | ExprKind::Character(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Name(_) => {}
    }
    Ok(())
}

/// 判断 callee 是否为结构体构造（类型名后跟 `(`）或枚举项构造。
fn is_constructor_target(
    module: &str,
    callee: &str,
    imports: &HashMap<String, ImportBinding>,
    type_infos: &HashMap<String, TypeInfo>,
) -> bool {
    // 先解析 callee 的第一个段（可能是 `use` 导入的模块别名）。
    let resolved = resolve_imported(imports, callee);
    // 结构体构造：`TypeName(...)` 或 `mod.TypeName(...)`。
    if type_infos.contains_key(&resolved) {
        return true;
    }
    // 本模块内构造：短名 `TypeName` 需 qualify 到全限定名后判定。
    if type_infos.contains_key(&qualify(module, callee)) {
        return true;
    }
    // 枚举项构造：`Enum.Variant(...)` 或 `mod.Enum.Variant(...)`，其中 Enum 是类型名。
    if let Some((prefix, _)) = resolved.rsplit_once('.') {
        return type_infos.contains_key(prefix);
    }
    false
}

/// 解析一个类型名到全限定名（用于结构体构造）。
/// 解析 `Enum.Variant` 到全限定枚举名 + variant（保留 `Enum.Variant` 结构）。
fn qualify_enum_variant(
    module: &str,
    imports: &HashMap<String, ImportBinding>,
    type_infos: &HashMap<String, TypeInfo>,
    name: &str,
) -> String {
    let resolved = resolve_imported(imports, name);
    if let Some((prefix, variant)) = resolved.rsplit_once('.')
        && type_infos.contains_key(prefix)
    {
        return format!("{prefix}.{variant}");
    }
    // 回退：用模块 qualify。
    let qualified = qualify(module, name);
    if let Some((prefix, variant)) = qualified.rsplit_once('.')
        && type_infos.contains_key(prefix)
    {
        return format!("{prefix}.{variant}");
    }
    resolved
}

/// 通过 `use` 绑定解析可能被导入的名称。
fn resolve_imported(imports: &HashMap<String, ImportBinding>, name: &str) -> String {
    if let Some((first, rest)) = name.split_once('.')
        && let Some(ImportBinding::Module(imported)) = imports.get(first)
    {
        return format!("{imported}.{rest}");
    }
    name.to_string()
}

/// 解析构造 callee（结构体名或枚举项名）到全限定名。
fn qualify_constructor(
    module: &str,
    imports: &HashMap<String, ImportBinding>,
    type_infos: &HashMap<String, TypeInfo>,
    name: &str,
) -> String {
    // 先解析第一个段（可能是 `use` 导入的模块别名）。
    let resolved = resolve_imported(imports, name);
    if type_infos.contains_key(&resolved) {
        // 结构体构造：`TypeName` 或 `mod.TypeName`。
        return resolved;
    }
    // 枚举项构造：`Enum.Variant` 或 `mod.Enum.Variant`。
    if let Some((prefix, variant)) = resolved.rsplit_once('.')
        && type_infos.contains_key(prefix)
    {
        return format!("{prefix}.{variant}");
    }
    // 回退：用模块 qualify 后再试一次（用于本模块内定义的类型）。
    let qualified = qualify(module, name);
    if type_infos.contains_key(&qualified) {
        return qualified;
    }
    resolved
}

/// 解析类型引用中的名字，把用户类型名 qualify 为全限定名。
fn resolve_type_ref(
    source: &SourceFile,
    module: &str,
    imports: &HashMap<String, ImportBinding>,
    type_infos: &HashMap<String, TypeInfo>,
    ty: &mut ast::TypeRef,
) -> Result<(), Diagnostic> {
    match &mut ty.kind {
        ast::TypeRefKind::Name(name) => {
            let resolved = if type_infos.contains_key(name) {
                name.clone()
            } else {
                let imported = resolve_imported(imports, name);
                if type_infos.contains_key(&imported) {
                    imported
                } else {
                    let qualified = qualify(module, name);
                    if type_infos.contains_key(&qualified) {
                        qualified
                    } else {
                        // 基础类型或未知类型：保持不变，交给 lower 的 resolve_type 处理。
                        return Ok(());
                    }
                }
            };
            check_type_visibility(source, module, &resolved, type_infos, ty.span)?;
            *name = resolved;
            Ok(())
        }
        ast::TypeRefKind::Array { element, .. } => {
            resolve_type_ref(source, module, imports, type_infos, element)
        }
        ast::TypeRefKind::Slice { element } => {
            resolve_type_ref(source, module, imports, type_infos, element)
        }
        ast::TypeRefKind::Pointer { inner } => {
            resolve_type_ref(source, module, imports, type_infos, inner)
        }
    }
}

/// 检查类型是否对当前模块可见（私有类型不能被其他模块引用）。
fn check_type_visibility(
    source: &SourceFile,
    module: &str,
    type_name: &str,
    type_infos: &HashMap<String, TypeInfo>,
    span: crate::source::Span,
) -> Result<(), Diagnostic> {
    let info = type_infos.get(type_name).expect("resolved type must exist");
    if info.module != module && !info.public {
        return Err(Diagnostic::at(
            source,
            span,
            format!("type `{type_name}` is private"),
        ));
    }
    Ok(())
}

fn resolve_callee(
    module: &str,
    locals: &HashSet<String>,
    imports: &HashMap<String, ImportBinding>,
    callee: &str,
) -> String {
    if let Some((first, rest)) = callee.split_once('.') {
        if let Some(ImportBinding::Module(imported)) = imports.get(first) {
            return format!("{imported}.{rest}");
        }
        return callee.to_string();
    }
    if locals.contains(callee) {
        return qualify(module, callee);
    }
    if let Some(ImportBinding::Function(function)) = imports.get(callee) {
        return function.clone();
    }
    qualify(module, callee)
}

fn qualify(module: &str, name: &str) -> String {
    if module.is_empty() {
        name.to_string()
    } else {
        format!("{module}.{name}")
    }
}

fn display_module(module: &str) -> &str {
    if module.is_empty() { "<root>" } else { module }
}
