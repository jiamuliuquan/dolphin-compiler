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
        sources.push(source);
        units.push(Unit {
            source_id,
            module,
            program,
        });
    }

    resolve_modules(&sources, &mut units)?;
    let mut functions = Vec::new();
    for unit in &mut units {
        functions.append(&mut unit.program.functions);
    }
    Ok(LoadedProgram {
        sources,
        program: ast::Program {
            package: None,
            uses: Vec::new(),
            functions,
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
        for function in &mut unit.program.functions {
            resolve_block(
                source,
                &unit.module,
                &local_names,
                &bindings,
                &functions,
                &mut function.body,
            )?;
            function.name = qualify(&unit.module, &function.name);
        }
    }
    Ok(())
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
    statements: &mut [ast::Statement],
) -> Result<(), Diagnostic> {
    for statement in statements {
        match &mut statement.kind {
            StatementKind::Variable { initializer, .. }
            | StatementKind::Assignment {
                value: initializer, ..
            } => resolve_expr(source, module, locals, imports, functions, initializer)?,
            StatementKind::IndexAssignment { index, value, .. } => {
                resolve_expr(source, module, locals, imports, functions, index)?;
                resolve_expr(source, module, locals, imports, functions, value)?;
            }
            StatementKind::Expression(expression) => {
                resolve_expr(source, module, locals, imports, functions, expression)?
            }
            StatementKind::If {
                condition,
                then_block,
                else_block,
            } => {
                resolve_expr(source, module, locals, imports, functions, condition)?;
                resolve_block(source, module, locals, imports, functions, then_block)?;
                if let Some(block) = else_block {
                    resolve_block(source, module, locals, imports, functions, block)?;
                }
            }
            StatementKind::Loop(block) => {
                resolve_block(source, module, locals, imports, functions, block)?
            }
            StatementKind::While { condition, body } => {
                resolve_expr(source, module, locals, imports, functions, condition)?;
                resolve_block(source, module, locals, imports, functions, body)?;
            }
            StatementKind::For { iterable, body, .. } => {
                match iterable {
                    ast::ForIterable::Range { start, end, .. } => {
                        resolve_expr(source, module, locals, imports, functions, start)?;
                        resolve_expr(source, module, locals, imports, functions, end)?;
                    }
                    ast::ForIterable::Array(array) => {
                        resolve_expr(source, module, locals, imports, functions, array)?
                    }
                }
                resolve_block(source, module, locals, imports, functions, body)?;
            }
            StatementKind::Return(Some(value)) => {
                resolve_expr(source, module, locals, imports, functions, value)?
            }
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
    expression: &mut ast::Expr,
) -> Result<(), Diagnostic> {
    match &mut expression.kind {
        ExprKind::Array(values) => {
            for value in values {
                resolve_expr(source, module, locals, imports, functions, value)?;
            }
        }
        ExprKind::RepeatArray { value, .. } | ExprKind::Unary { operand: value, .. } => {
            resolve_expr(source, module, locals, imports, functions, value)?
        }
        ExprKind::Cast { value, .. } => {
            resolve_expr(source, module, locals, imports, functions, value)?
        }
        ExprKind::Index { array, index }
        | ExprKind::Binary {
            left: array,
            right: index,
            ..
        } => {
            resolve_expr(source, module, locals, imports, functions, array)?;
            resolve_expr(source, module, locals, imports, functions, index)?;
        }
        ExprKind::Call {
            callee,
            callee_span,
            arguments,
        } => {
            for argument in arguments {
                resolve_expr(source, module, locals, imports, functions, argument)?;
            }
            if matches!(callee.as_str(), "print" | "println" | "length") {
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
        ExprKind::Number(_)
        | ExprKind::Character(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Name(_) => {}
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
