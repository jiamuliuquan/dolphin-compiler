use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use dolphin_package::package::{PackageId, qualify_module};
use dolphin_source::diagnostic::Diagnostic;
use dolphin_source::lexer;
use dolphin_source::source::SourceFile;
use dolphin_syntax::ast::{self, ExprKind, StatementKind};
use dolphin_syntax::parser;

pub struct LoadedProgram {
    pub sources: Vec<SourceFile>,
    pub program: ast::Program,
    /// 与 `sources` 对齐的定义所在包身份（用户源码为 `ROOT`，内建标准库为 `STD`）。
    pub packages: Vec<PackageId>,
}

/// 一个待加载包的源码配置（M15-C）。
pub struct PackageSources {
    pub id: PackageId,
    /// 编译期模块前缀（根包为空，依赖包为 `@<id>`）。
    pub prefix: String,
    /// 该包自己的依赖别名环境：别名 -> 目标包前缀。
    pub aliases: BTreeMap<String, String>,
    pub source_root: PathBuf,
    /// 需要排除的入口文件（其他 bin、库构建时的全部 bin）。
    pub exclude: HashSet<PathBuf>,
}

struct Unit {
    source_id: usize,
    /// 全局限定模块名（已含包前缀）。
    module: String,
    program: ast::Program,
    /// 是否为编译器注入的内建标准库源码（允许占用保留的 `std` 命名空间）。
    builtin: bool,
    package: PackageId,
    /// 该单元所属包的模块前缀。
    prefix: String,
    /// 该包自己的依赖别名环境：别名 -> 目标包前缀。
    aliases: BTreeMap<String, String>,
}

/// 随编译器分发的 Dolphin 源码标准库模块。
///
/// 与内建 `std.mem`（无源码、由 lowering 直接实现）不同，这些模块是普通
/// Dolphin 源码，使用与用户包相同的解析、检查与单态化路径；它们只是不做磁盘
/// 发现、跳过 `pkg` 路径校验，并携带保留身份 `PackageId::STD`。
const STDLIB_SOURCES: &[(&str, &str, &str)] = dolphin_std::UNITS;

#[derive(Clone)]
struct FunctionInfo {
    module: String,
    public: bool,
}

#[derive(Clone)]
enum ImportBinding {
    Module(String),
    Function(String),
    /// `use mod.Trait;`：导入 trait 名（首版方法解析不依赖导入，但语法必须可用）。
    Trait(String),
    /// `use mod.Type;`：导入结构体/枚举类型名。
    Type(String),
}

pub fn load_project(project: &Path) -> Result<LoadedProgram, Diagnostic> {
    load_sources(&project.join("src"), &HashSet::new())
}

/// 从指定源码根加载单包项目，跳过 `exclude` 中的入口文件。
///
/// M9 多可执行目标下，每个 `[[bin]]` 拥有独立入口文件；编译其中一个
/// 目标时需要排除其余目标的入口，从而保证每个编译单元只有一个 `main`。
pub fn load_sources(
    source_root: &Path,
    exclude: &HashSet<PathBuf>,
) -> Result<LoadedProgram, Diagnostic> {
    load_packages(&[PackageSources {
        id: PackageId::ROOT,
        prefix: String::new(),
        aliases: BTreeMap::new(),
        source_root: source_root.to_path_buf(),
        exclude: exclude.clone(),
    }])
}

/// 从多个包的源码根加载并合并为单个编译单元（M15-C）。
pub fn load_packages(packages: &[PackageSources]) -> Result<LoadedProgram, Diagnostic> {
    let mut sources = Vec::new();
    let mut units = Vec::new();
    for package in packages {
        if !package.source_root.is_dir() {
            return Err(Diagnostic::plain(format!(
                "project does not contain a source directory `{}`",
                package.source_root.display()
            )));
        }
        let mut paths = Vec::new();
        discover_sources(&package.source_root, &mut paths)?;
        paths.retain(|path| !package.exclude.contains(path));
        paths.sort();
        if paths.is_empty() {
            return Err(Diagnostic::plain(format!(
                "source directory `{}` does not contain any `.do` files",
                package.source_root.display()
            )));
        }
        let mut top_level = HashSet::new();
        let mut parsed = Vec::with_capacity(paths.len());
        for path in &paths {
            let text = fs::read_to_string(path).map_err(|error| {
                Diagnostic::plain(format!("could not read `{}`: {error}", path.display()))
            })?;
            let source = SourceFile::new(path.clone(), text);
            let tokens = lexer::lex(&source)?;
            let mut program = parser::parse(&source, tokens)?;
            let relative = expected_module(&package.source_root, path)?;
            let expected_package = expected_package(&package.source_root, path)?;
            validate_package(&source, &program, &expected_package)?;
            if let Some(segment) = relative.split('.').next()
                && !segment.is_empty()
            {
                top_level.insert(segment.to_string());
            }
            let source_id = sources.len();
            assign_source_id(&mut program, source_id);
            sources.push(source);
            parsed.push((source_id, relative, program));
        }
        for (segment, target) in &package.aliases {
            if top_level.contains(segment) {
                return Err(Diagnostic::plain(format!(
                    "dependency alias `{segment}` conflicts with a top-level module of package `{}`",
                    package.id
                )));
            }
            let _ = target;
        }
        for (source_id, relative, program) in parsed {
            let module = qualify_module(&package.prefix, &relative);
            units.push(Unit {
                source_id,
                module,
                program,
                builtin: false,
                package: package.id,
                prefix: package.prefix.clone(),
                aliases: package.aliases.clone(),
            });
        }
    }

    append_stdlib_units(&mut sources, &mut units)?;
    resolve_modules(&sources, &mut units)?;
    Ok(flatten_units(sources, units))
}

/// 单文件模式：把给定程序视为根模块，注入内建标准库后完成模块解析。
pub(crate) fn inject_stdlib(
    source: &SourceFile,
    program: &ast::Program,
) -> Result<LoadedProgram, Diagnostic> {
    let source = SourceFile::new(source.path.clone(), source.text.clone());
    let mut program = program.clone();
    assign_source_id(&mut program, 0);
    let mut sources = vec![source];
    let mut units = vec![Unit {
        source_id: 0,
        module: String::new(),
        program,
        builtin: false,
        package: PackageId::ROOT,
        prefix: String::new(),
        aliases: BTreeMap::new(),
    }];
    append_stdlib_units(&mut sources, &mut units)?;
    resolve_modules(&sources, &mut units)?;
    Ok(flatten_units(sources, units))
}

/// 追加编译器内建标准库单元（模块名与包身份固定，跳过磁盘 `pkg` 校验）。
fn append_stdlib_units(
    sources: &mut Vec<SourceFile>,
    units: &mut Vec<Unit>,
) -> Result<(), Diagnostic> {
    for (module, path, text) in STDLIB_SOURCES {
        let source = SourceFile::new(PathBuf::from(path), (*text).to_string());
        let tokens = lexer::lex(&source)?;
        let mut program = parser::parse(&source, tokens)?;
        let source_id = sources.len();
        assign_source_id(&mut program, source_id);
        sources.push(source);
        units.push(Unit {
            source_id,
            module: (*module).to_string(),
            program,
            builtin: true,
            package: PackageId::STD,
            prefix: String::new(),
            aliases: BTreeMap::new(),
        });
    }
    Ok(())
}

/// 合并各单元为单个 `ast::Program`，并保留与 `sources` 对齐的包身份。
fn flatten_units(sources: Vec<SourceFile>, mut units: Vec<Unit>) -> LoadedProgram {
    let packages = units.iter().map(|unit| unit.package).collect();
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut enums = Vec::new();
    let mut traits = Vec::new();
    let mut impls = Vec::new();
    for unit in &mut units {
        functions.append(&mut unit.program.functions);
        structs.append(&mut unit.program.structs);
        enums.append(&mut unit.program.enums);
        traits.append(&mut unit.program.traits);
        impls.append(&mut unit.program.impls);
    }
    LoadedProgram {
        sources,
        program: ast::Program {
            package: None,
            uses: Vec::new(),
            functions,
            structs,
            enums,
            traits,
            impls,
        },
        packages,
    }
}

/// 为程序中的所有顶层定义打上源码文件编号。
fn assign_source_id(program: &mut ast::Program, source_id: usize) {
    for function in &mut program.functions {
        function.source_id = source_id;
    }
    for structure in &mut program.structs {
        structure.source_id = source_id;
    }
    for enumeration in &mut program.enums {
        enumeration.source_id = source_id;
    }
    for item in &mut program.traits {
        item.source_id = source_id;
        // trait/impl 方法也是定义在该源文件里的函数；漏设会让方法的
        // `source_id` 停留在解析器默认值 0，把 stdlib/其他文件的方法误归属到
        // 第一个源文件（诊断与 IR Location 错误，甚至切到多字节字符中间而 panic）。
        for method in &mut item.methods {
            method.source_id = source_id;
        }
    }
    for item in &mut program.impls {
        item.source_id = source_id;
        for method in &mut item.methods {
            method.source_id = source_id;
        }
    }
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

/// 文件所属包的声明路径：相对于源码根的父目录，分隔符换成 `.`；根目录文件为空。
///
/// 包只由目录决定，文件名不参与 `pkg`；文件自身的模块名仍是「包路径 + 文件名」。
fn expected_package(source_root: &Path, path: &Path) -> Result<String, Diagnostic> {
    let relative = path.strip_prefix(source_root).map_err(|_| {
        Diagnostic::plain(format!(
            "source `{}` is outside the source root",
            path.display()
        ))
    })?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let segments: Vec<String> = parent
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
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
                    format!("package path `{actual}` does not match directory `{expected}`"),
                ))
            }
        }
    }
}

fn resolve_modules(sources: &[SourceFile], units: &mut [Unit]) -> Result<(), Diagnostic> {
    let modules: HashSet<String> = units.iter().map(|unit| unit.module.clone()).collect();
    // `std` 命名空间保留给内建标准库；用户模块不得占用（内建 std 单元自身除外）。
    for unit in units.iter() {
        if !unit.builtin && unit.module.split('.').next() == Some("std") {
            return Err(Diagnostic::plain(format!(
                "module `{}` uses the reserved `std` namespace",
                unit.module
            )));
        }
    }
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

    // trait 与类型同属类型命名空间，收集可见性并按限定名索引。
    let mut trait_infos: HashMap<String, TypeInfo> = HashMap::new();
    for unit in units.iter() {
        let source = &sources[unit.source_id];
        for item in &unit.program.traits {
            let qualified = qualify(&unit.module, &item.name);
            if trait_infos
                .insert(
                    qualified.clone(),
                    TypeInfo {
                        module: unit.module.clone(),
                        public: item.public,
                    },
                )
                .is_some()
            {
                return Err(Diagnostic::at(
                    source,
                    item.name_span,
                    format!("trait `{qualified}` is already defined"),
                ));
            }
        }
    }
    for unit in units.iter_mut() {
        let source = &sources[unit.source_id];
        let bindings = resolve_imports(
            source,
            unit,
            &modules,
            &functions,
            &type_infos,
            &trait_infos,
        )?;
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
            resolve_bounds(
                source,
                &unit.module,
                &bindings,
                &trait_infos,
                &mut structure.type_params,
            )?;
            for field in &mut structure.fields {
                resolve_type_ref(source, &unit.module, &bindings, &type_infos, &mut field.ty)?;
            }
        }
        for enumeration in &mut unit.program.enums {
            enumeration.name = qualify(&unit.module, &enumeration.name);
            resolve_bounds(
                source,
                &unit.module,
                &bindings,
                &trait_infos,
                &mut enumeration.type_params,
            )?;
            for variant in &mut enumeration.variants {
                for field in &mut variant.fields {
                    resolve_type_ref(source, &unit.module, &bindings, &type_infos, field)?;
                }
            }
        }
        for function in &mut unit.program.functions {
            resolve_bounds(
                source,
                &unit.module,
                &bindings,
                &trait_infos,
                &mut function.type_params,
            )?;
            resolve_signature_types(source, &unit.module, &bindings, &type_infos, function)?;
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
        // trait 声明：qualify trait 名并解析方法签名中的类型。
        for item in &mut unit.program.traits {
            item.name = qualify(&unit.module, &item.name);
            resolve_bounds(
                source,
                &unit.module,
                &bindings,
                &trait_infos,
                &mut item.type_params,
            )?;
            for method in &mut item.methods {
                resolve_bounds(
                    source,
                    &unit.module,
                    &bindings,
                    &trait_infos,
                    &mut method.type_params,
                )?;
                resolve_signature_types(source, &unit.module, &bindings, &type_infos, method)?;
            }
        }
        // impl 块：qualify 目标类型/trait、关联类型绑定、方法签名与函数体。
        for item in &mut unit.program.impls {
            item.module = unit.module.clone();
            item.type_name =
                resolve_type_name(&unit.module, &bindings, &type_infos, &item.type_name);
            if let Some(trait_name) = &mut item.trait_name {
                *trait_name = resolve_type_name(&unit.module, &bindings, &trait_infos, trait_name);
            }
            for argument in &mut item.trait_arguments {
                resolve_type_ref(source, &unit.module, &bindings, &type_infos, argument)?;
            }
            for argument in &mut item.type_arguments {
                resolve_type_ref(source, &unit.module, &bindings, &type_infos, argument)?;
            }
            for binding in &mut item.associated_types {
                resolve_type_ref(
                    source,
                    &unit.module,
                    &bindings,
                    &type_infos,
                    &mut binding.ty,
                )?;
            }
            let mut method_locals = local_names.clone();
            method_locals.insert("self".to_string());
            for method in &mut item.methods {
                resolve_bounds(
                    source,
                    &unit.module,
                    &bindings,
                    &trait_infos,
                    &mut method.type_params,
                )?;
                resolve_signature_types(source, &unit.module, &bindings, &type_infos, method)?;
                resolve_block(
                    source,
                    &unit.module,
                    &method_locals,
                    &bindings,
                    &functions,
                    &type_infos,
                    &mut method.body,
                )?;
            }
        }
    }
    Ok(())
}

/// 解析类型参数的单 trait 约束为限定 trait 名。
fn resolve_bounds(
    source: &SourceFile,
    module: &str,
    imports: &HashMap<String, ImportBinding>,
    trait_infos: &HashMap<String, TypeInfo>,
    type_params: &mut [ast::TypeParamDecl],
) -> Result<(), Diagnostic> {
    for param in type_params {
        if let Some(bound) = &param.bound {
            let resolved = resolve_type_name(module, imports, trait_infos, bound);
            if !trait_infos.contains_key(&resolved) {
                return Err(Diagnostic::at(
                    source,
                    param.name_span,
                    format!("unknown trait `{bound}` in type parameter bound"),
                ));
            }
            param.bound = Some(resolved);
        }
    }
    Ok(())
}

/// 解析函数签名中的参数与返回类型（方法接收者的 `Self` 保持原名）。
fn resolve_signature_types(
    source: &SourceFile,
    module: &str,
    imports: &HashMap<String, ImportBinding>,
    type_infos: &HashMap<String, TypeInfo>,
    function: &mut ast::Function,
) -> Result<(), Diagnostic> {
    for parameter in &mut function.parameters {
        resolve_type_ref(source, module, imports, type_infos, &mut parameter.ty)?;
    }
    if let Some(return_type) = &mut function.return_type {
        resolve_type_ref(source, module, imports, type_infos, return_type)?;
    }
    Ok(())
}

/// 把一个类型名/trait 名解析为全限定名（未知名字保持原样交给 lower 报错）。
fn resolve_type_name(
    module: &str,
    imports: &HashMap<String, ImportBinding>,
    infos: &HashMap<String, TypeInfo>,
    name: &str,
) -> String {
    if let Some(ImportBinding::Trait(path)) | Some(ImportBinding::Type(path)) = imports.get(name)
        && infos.contains_key(path)
    {
        return path.clone();
    }
    // 本模块定义优先于其它模块的裸名（根模块类型以裸名存储，不能遮蔽子模块同名类型）。
    let qualified = qualify(module, name);
    if infos.contains_key(&qualified) {
        return qualified;
    }
    let resolved = resolve_imported(imports, name);
    if infos.contains_key(&resolved) {
        return resolved;
    }
    if infos.contains_key(name) {
        return name.to_string();
    }
    // 最小 prelude：未被本模块定义或导入遮蔽时，Option/Result/Iterator 指向 std。
    if let Some(prelude) = resolve_prelude_path(name)
        && infos.contains_key(&prelude)
    {
        return prelude;
    }
    resolved
}

/// 把 prelude 名字（`Option` / `Result` / `Iterator` 及其成员路径）映射到 std 定义。
///
/// 只在这三个名字未被显式定义或 `use` 导入遮蔽时使用。
fn resolve_prelude_path(name: &str) -> Option<String> {
    let (first, rest) = match name.split_once('.') {
        Some((first, rest)) => (first, Some(rest)),
        None => (name, None),
    };
    let qualified = match first {
        "Option" => "std.Option",
        "Result" => "std.Result",
        "Iterator" => "std.Iterator",
        _ => return None,
    };
    Some(match rest {
        Some(rest) => format!("{qualified}.{rest}"),
        None => qualified.to_string(),
    })
}

#[derive(Clone)]
struct TypeInfo {
    module: String,
    public: bool,
}

/// 把源码中的 `use` 路径解析为全局限定路径（M15-C）。
///
/// - 首段是当前包的依赖别名时，指向目标包的模块前缀；
/// - `std` 命名空间保持原样（标准库在全局命名空间中就叫 `std`）；
/// - 其余是包内相对模块名，补上当前包的模块前缀。
fn resolve_import_path(unit: &Unit, raw: &str) -> String {
    let (first, rest) = match raw.split_once('.') {
        Some((first, rest)) => (first, Some(rest)),
        None => (raw, None),
    };
    if let Some(target) = unit.aliases.get(first) {
        return match rest {
            Some(rest) => format!("{target}.{rest}"),
            None => target.clone(),
        };
    }
    if first == "std" {
        return raw.to_string();
    }
    qualify_module(&unit.prefix, raw)
}

/// 判断路径能否作为 `use` 目标：既是完整模块，或是某些模块的包前缀。
///
/// 后者支持 `use mathutil;` 这类包级导入，成员通过 `mathutil.math.min` 访问。
fn is_module_or_namespace(path: &str, modules: &HashSet<String>) -> bool {
    if modules.contains(path) {
        return true;
    }
    let prefix = format!("{path}.");
    modules.iter().any(|module| module.starts_with(&prefix))
}

fn resolve_imports(
    source: &SourceFile,
    unit: &Unit,
    modules: &HashSet<String>,
    functions: &HashMap<String, FunctionInfo>,
    types: &HashMap<String, TypeInfo>,
    traits: &HashMap<String, TypeInfo>,
) -> Result<HashMap<String, ImportBinding>, Diagnostic> {
    let mut bindings = HashMap::new();
    for import in &unit.program.uses {
        let raw = import.segments.join(".");
        let path = resolve_import_path(unit, &raw);
        let (name, binding) = if path == "std.mem" {
            // 内建 std.mem 命名空间：`use std.mem;` 无需磁盘上的 std.do。
            (
                "mem".to_string(),
                ImportBinding::Module("std.mem".to_string()),
            )
        } else if is_module_or_namespace(&path, modules) {
            (
                import.segments.last().unwrap().clone(),
                ImportBinding::Module(path),
            )
        } else if let Some((module, name)) = path.rsplit_once('.') {
            if let Some(info) = functions.get(&path) {
                if info.module != unit.module && !info.public {
                    return Err(Diagnostic::at(
                        source,
                        import.span,
                        format!("function `{path}` is private"),
                    ));
                }
                debug_assert_eq!(info.module, module);
                (name.to_string(), ImportBinding::Function(path))
            } else if let Some(info) = traits.get(&path) {
                if info.module != unit.module && !info.public {
                    return Err(Diagnostic::at(
                        source,
                        import.span,
                        format!("trait `{path}` is private"),
                    ));
                }
                (name.to_string(), ImportBinding::Trait(path))
            } else if let Some(info) = types.get(&path) {
                if info.module != unit.module && !info.public {
                    return Err(Diagnostic::at(
                        source,
                        import.span,
                        format!("type `{path}` is private"),
                    ));
                }
                (name.to_string(), ImportBinding::Type(path))
            } else {
                return Err(Diagnostic::at(
                    source,
                    import.span,
                    format!("unknown import `{path}`"),
                ));
            }
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
            StatementKind::Variable {
                initializer,
                type_name,
                ..
            } => {
                if let Some(ty) = type_name {
                    resolve_type_ref(source, module, imports, type_infos, ty)?;
                }
                resolve_expr(
                    source,
                    module,
                    locals,
                    imports,
                    functions,
                    type_infos,
                    initializer,
                )?;
            }
            StatementKind::Assignment {
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
            StatementKind::FieldAssignment { value, .. } => resolve_expr(
                source, module, locals, imports, functions, type_infos, value,
            )?,
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
            StatementKind::Defer(call) => {
                resolve_expr(source, module, locals, imports, functions, type_infos, call)?
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
        ExprKind::RepeatArray { value, .. }
        | ExprKind::Unary { operand: value, .. }
        | ExprKind::AddressOf { operand: value }
        | ExprKind::Deref { operand: value } => resolve_expr(
            source, module, locals, imports, functions, type_infos, value,
        )?,
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
            type_arguments,
            arguments,
        } => {
            for argument in arguments {
                resolve_expr(
                    source, module, locals, imports, functions, type_infos, argument,
                )?;
            }
            for ty in type_arguments.iter_mut() {
                resolve_type_ref(source, module, imports, type_infos, ty)?;
            }
            if matches!(callee.as_str(), "print" | "println" | "length") {
                if !type_arguments.is_empty() {
                    return Err(Diagnostic::at(
                        source,
                        *callee_span,
                        format!("`{callee}` does not accept explicit type arguments"),
                    ));
                }
                return Ok(());
            }
            // 内建 std.mem intrinsic：`mem.alloc<T>(...)` 等。
            if let Some(canonical) = resolve_std_mem_call(imports, callee) {
                let name = canonical
                    .strip_prefix("std.mem.")
                    .expect("std.mem canonical name has the prefix");
                if !STD_MEM_INTRINSICS.contains(&name) {
                    return Err(Diagnostic::at(
                        source,
                        *callee_span,
                        format!("unknown function `{callee}`"),
                    ));
                }
                *callee = canonical;
                return Ok(());
            }
            // 关联函数路径：`Type::function(...)`（M15/R11）。
            if let Some((type_part, method)) = callee.rsplit_once("::") {
                let resolved = resolve_associated_type(module, imports, type_infos, type_part);
                *callee = format!("{resolved}::{method}");
                return Ok(());
            }
            // 结构体/枚举构造的 callee 是类型名或枚举项名：qualify 后不按函数解析。
            if is_constructor_target(module, callee, imports, type_infos) {
                *callee = qualify_constructor(module, imports, type_infos, callee);
                return Ok(());
            }
            // 方法调用：`value.method(...)`。首段不是 `use` 导入的模块或函数时，
            // 视为方法调用，接收者交由 lower 按局部变量解析。
            if let Some((first, _)) = callee.split_once('.')
                && !imports.contains_key(first)
                && !functions.contains_key(callee)
            {
                return Ok(());
            }
            // 字符串内建调用形态：`string.from_bytes(bytes)`。
            if callee == "string.from_bytes" {
                if !type_arguments.is_empty() {
                    return Err(Diagnostic::at(
                        source,
                        *callee_span,
                        "`string.from_bytes` does not accept type arguments",
                    ));
                }
                return Ok(());
            }
            // 切片/字符串成员方法：`name.slice(...)`、`name.bytes()` 由 lower 直接降低，
            // 这里不按普通函数解析（`name` 是局部变量而非模块别名）。
            if let Some((base, method)) = callee.rsplit_once('.')
                && matches!(method, "slice" | "bytes")
                && !base.contains('.')
                && !imports.contains_key(base)
            {
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
        ExprKind::Field { base, .. } => {
            resolve_expr(source, module, locals, imports, functions, type_infos, base)?;
        }
        ExprKind::Match { value, arms } => {
            resolve_expr(
                source, module, locals, imports, functions, type_infos, value,
            )?;
            for arm in arms {
                if let ast::MatchPattern::Enum {
                    name,
                    type_arguments,
                    ..
                } = &mut arm.pattern
                {
                    for ty in type_arguments.iter_mut() {
                        resolve_type_ref(source, module, imports, type_infos, ty)?;
                    }
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
    // 结构体构造：`TypeName(...)`、`mod.TypeName(...)` 或本模块内的 `TypeName(...)`。
    if type_infos.contains_key(&resolved) || type_infos.contains_key(&qualify(module, callee)) {
        return true;
    }
    // 枚举项构造：`Enum.Variant(...)` 或 `mod.Enum.Variant(...)`，其中 Enum 是类型名。
    if let Some((prefix, _)) = resolved.rsplit_once('.')
        && type_infos.contains_key(prefix)
    {
        return true;
    }
    // 本模块内用裸名构造枚举项：`Enum.Variant(...)` 的 Enum 需按模块限定后查找
    // （类型表以 `模块.类型` 为键），否则子模块里的枚举构造会被误报为未知函数。
    if let Some((prefix, _)) = callee.rsplit_once('.')
        && type_infos.contains_key(&qualify(module, prefix))
    {
        return true;
    }
    // prelude 枚举：`Option.Some(...)` / `Result.Ok(...)`。
    if let Some(prelude) = resolve_prelude_path(callee) {
        if type_infos.contains_key(&prelude) {
            return true;
        }
        if let Some((prefix, _)) = prelude.rsplit_once('.') {
            return type_infos.contains_key(prefix);
        }
    }
    false
}

/// 内建 `std.mem` intrinsic 名称（M14-C）。这些符号没有磁盘源码，
/// 由编译器直接降低为布局常量、运行时分配/释放或视图构造。
pub(crate) const STD_MEM_INTRINSICS: &[&str] = &[
    "alloc",
    "free",
    "create",
    "destroy",
    "size_of",
    "align_of",
    "copy",
    "is_valid_utf8",
    "view",
    "view_const",
    "cast_ptr",
    "cast_const_ptr",
];

/// 把 `mem.<name>` / `std.mem.<name>` 调用解析为规范名 `std.mem.<name>`。
///
/// `use std.mem;` 会把 `mem` 绑定到内建模块 `std.mem`；也接受完整写法
/// `std.mem.alloc(...)`。其他情况返回 `None`。
fn resolve_std_mem_call(imports: &HashMap<String, ImportBinding>, callee: &str) -> Option<String> {
    let (first, rest) = callee.split_once('.')?;
    if first == "std" {
        let name = rest.strip_prefix("mem.")?;
        return Some(format!("std.mem.{name}"));
    }
    match imports.get(first) {
        Some(ImportBinding::Module(module)) if module == "std.mem" => {
            Some(format!("std.mem.{rest}"))
        }
        _ => None,
    }
}

/// 解析一个类型名到全限定名（用于结构体构造）。
/// 解析关联函数路径的接收类型名（`Vec<i32>::init` 中的 `Vec`）到全限定名。
fn resolve_associated_type(
    module: &str,
    imports: &HashMap<String, ImportBinding>,
    type_infos: &HashMap<String, TypeInfo>,
    name: &str,
) -> String {
    let qualified = qualify(module, name);
    if type_infos.contains_key(&qualified) {
        return qualified;
    }
    let resolved = resolve_imported(imports, name);
    if type_infos.contains_key(&resolved) {
        return resolved;
    }
    if let Some(prelude) = resolve_prelude_path(name) {
        return prelude;
    }
    resolved
}

/// 解析 `Enum.Variant` 到全限定枚举名 + variant（保留 `Enum.Variant` 结构）。
fn qualify_enum_variant(
    module: &str,
    imports: &HashMap<String, ImportBinding>,
    type_infos: &HashMap<String, TypeInfo>,
    name: &str,
) -> String {
    // 本模块定义的枚举优先于其它模块的裸名。
    let qualified = qualify(module, name);
    if let Some((prefix, variant)) = qualified.rsplit_once('.')
        && type_infos.contains_key(prefix)
    {
        return format!("{prefix}.{variant}");
    }
    let resolved = resolve_imported(imports, name);
    if let Some((prefix, variant)) = resolved.rsplit_once('.')
        && type_infos.contains_key(prefix)
    {
        return format!("{prefix}.{variant}");
    }
    if let Some(prelude) = resolve_prelude_path(name) {
        return prelude;
    }
    resolved
}

/// 通过 `use` 绑定解析可能被导入的名称。
fn resolve_imported(imports: &HashMap<String, ImportBinding>, name: &str) -> String {
    if let Some((first, rest)) = name.split_once('.') {
        match imports.get(first) {
            Some(ImportBinding::Module(imported)) | Some(ImportBinding::Type(imported)) => {
                return format!("{imported}.{rest}");
            }
            _ => {}
        }
        return name.to_string();
    }
    if let Some(ImportBinding::Type(imported)) = imports.get(name) {
        return imported.clone();
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
    // 本模块定义的类型优先于其它模块的裸名（根模块类型以裸名存储）。
    let qualified = qualify(module, name);
    if type_infos.contains_key(&qualified) {
        // 结构体构造：`TypeName` 或 `mod.TypeName`。
        return qualified;
    }
    if let Some((prefix, variant)) = qualified.rsplit_once('.')
        && type_infos.contains_key(prefix)
    {
        // 枚举项构造：`Enum.Variant` 或 `mod.Enum.Variant`。
        return format!("{prefix}.{variant}");
    }
    // 再解析 `use` 导入或模块别名。
    let resolved = resolve_imported(imports, name);
    if type_infos.contains_key(&resolved) {
        return resolved;
    }
    if let Some((prefix, variant)) = resolved.rsplit_once('.')
        && type_infos.contains_key(prefix)
    {
        return format!("{prefix}.{variant}");
    }
    if let Some(prelude) = resolve_prelude_path(name) {
        return prelude;
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
        ast::TypeRefKind::Name { name, arguments } => {
            for argument in arguments.iter_mut() {
                resolve_type_ref(source, module, imports, type_infos, argument)?;
            }
            // 类型参数名（如 `T`）不是用户类型，保持原样交由 lower 的实例化环境解析。
            // 本模块定义优先于其它模块的裸名（根模块类型以裸名存储）。
            let resolved = {
                let qualified = qualify(module, name);
                let imported = resolve_imported(imports, name);
                if type_infos.contains_key(&qualified) {
                    qualified
                } else if type_infos.contains_key(&imported) {
                    imported
                } else if type_infos.contains_key(name) {
                    name.to_string()
                } else if let Some(prelude) = resolve_prelude_path(name)
                    && type_infos.contains_key(&prelude)
                {
                    prelude
                } else {
                    // 基础类型、类型参数或未知类型：保持不变。
                    return Ok(());
                }
            };
            check_type_visibility(source, module, &resolved, type_infos, ty.span)?;
            *name = resolved;
            Ok(())
        }
        ast::TypeRefKind::Array { element, .. } => {
            resolve_type_ref(source, module, imports, type_infos, element)
        }
        ast::TypeRefKind::Ptr { pointee, .. } => {
            resolve_type_ref(source, module, imports, type_infos, pointee)
        }
        ast::TypeRefKind::Slice { element, .. } => {
            resolve_type_ref(source, module, imports, type_infos, element)
        }
    }
}

/// 检查类型是否对当前模块可见（私有类型不能被其他模块引用）。
fn check_type_visibility(
    source: &SourceFile,
    module: &str,
    type_name: &str,
    type_infos: &HashMap<String, TypeInfo>,
    span: dolphin_source::source::Span,
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
