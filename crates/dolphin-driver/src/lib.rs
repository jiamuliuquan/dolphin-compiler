use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use dolphin_backend::CodegenBackend;
use dolphin_hir::{lower, modules};
use dolphin_ir::ir;
use dolphin_linker::linker;
use dolphin_package::{lockfile, manifest, package_archive, registry, resolver};
use dolphin_platform::platform;
use dolphin_source::lexer;
use dolphin_source::source::SourceFile;
use dolphin_syntax::parser;

pub use dolphin_linker::linker::LinkerChoice;
pub use dolphin_package::cache::Cache;
pub use dolphin_package::lockfile::Lockfile;
pub use dolphin_package::manifest::{
    BinTarget, BuildConfig, Dependency, DependencyKind, LibTarget, Manifest, NativeSpec, Package,
    parse_coordinate,
};
pub use dolphin_package::package::{PackageGraph, PackageId, PackageInfo, PackageSource};
pub use dolphin_package::package_archive::{PackageMetadata, extract, read_metadata, sha256_hex};
pub use dolphin_package::registry::Registry;
pub use dolphin_package::resolver::{AcquiredPackage, RemoteSource, ResolveOptions};
pub use dolphin_platform::platform::{Abi, NativeInputs, TargetPlatform};
pub use dolphin_source::diagnostic::Diagnostic;

/// 代码生成后端选择（M16）。
///
/// 同一份类型化 IR 可运行于多个后端；默认 Cranelift（快速编译），LLVM 需在
/// 构建 `dc` 时启用 `llvm` feature，并用 `--backend llvm` 或环境变量
/// `DOLPHIN_BACKEND=llvm` 选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendChoice {
    Cranelift,
    Llvm,
}

impl BackendChoice {
    pub fn name(self) -> &'static str {
        match self {
            Self::Cranelift => "cranelift",
            Self::Llvm => "llvm",
        }
    }

    /// 解析后端名（CLI/环境变量共用）；未知名称返回诊断。
    pub fn parse(value: &str) -> Result<Self, Diagnostic> {
        match value.to_ascii_lowercase().as_str() {
            "cranelift" => Ok(Self::Cranelift),
            "llvm" => Ok(Self::Llvm),
            other => Err(Diagnostic::plain(format!(
                "unknown codegen backend `{other}` (expected `cranelift` or `llvm`)"
            ))),
        }
    }

    /// 从 `DOLPHIN_BACKEND` 读取默认后端；未设置时为 Cranelift。
    ///
    /// 非法取值会打印警告并回退到 Cranelift（`Default` / `BuildSettings` 的构造
    /// 无法返回诊断，`--backend` 路径仍会严格报错）。
    pub fn from_env() -> Self {
        match std::env::var("DOLPHIN_BACKEND") {
            Ok(value) => match Self::parse(&value) {
                Ok(choice) => choice,
                Err(error) => {
                    eprintln!("warning: {error}; falling back to `cranelift`");
                    Self::Cranelift
                }
            },
            Err(_) => Self::Cranelift,
        }
    }
}

impl Default for BackendChoice {
    fn default() -> Self {
        Self::from_env()
    }
}

/// 按选择返回编译进当前二进制的后端实现；未编译该后端时给出诊断。
fn select_backend(choice: BackendChoice) -> Result<&'static dyn CodegenBackend, Diagnostic> {
    match choice {
        BackendChoice::Cranelift => {
            #[cfg(feature = "cranelift")]
            {
                static BACKEND: dolphin_codegen_cranelift::CraneliftBackend =
                    dolphin_codegen_cranelift::CraneliftBackend;
                Ok(&BACKEND)
            }
            #[cfg(not(feature = "cranelift"))]
            {
                Err(Diagnostic::plain(
                    "this build of dc does not include the Cranelift backend; rebuild with the `cranelift` feature",
                ))
            }
        }
        BackendChoice::Llvm => {
            #[cfg(feature = "llvm")]
            {
                static BACKEND: dolphin_codegen_llvm::LlvmBackend =
                    dolphin_codegen_llvm::LlvmBackend;
                Ok(&BACKEND)
            }
            #[cfg(not(feature = "llvm"))]
            {
                Err(Diagnostic::plain(
                    "this build of dc does not include the LLVM backend; rebuild with the `llvm` feature",
                ))
            }
        }
    }
}

#[cfg(not(any(feature = "cranelift", feature = "llvm")))]
compile_error!("dolphin-driver requires the `cranelift` or `llvm` feature");

/// 宿主目标平台（M10 起宿主即目标）。
pub fn host_platform() -> Result<Box<dyn TargetPlatform>, Diagnostic> {
    platform::host()
}

/// 读取并解析项目根目录下的 `dolphin.toml`。
pub fn load_manifest(root: &Path) -> Result<Manifest, Diagnostic> {
    manifest::load(root)
}

#[derive(Debug)]
pub struct BuildOptions {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
}

/// 构建时使用的链接器与代码生成后端选择（M12/M16）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuildSettings {
    pub linker: LinkerChoice,
    pub backend: BackendChoice,
}

impl BuildSettings {
    pub fn with_linker(linker: LinkerChoice) -> Self {
        Self {
            linker,
            backend: BackendChoice::default(),
        }
    }

    pub fn with_backend(backend: BackendChoice) -> Self {
        Self {
            linker: LinkerChoice::default(),
            backend,
        }
    }
}

#[derive(Debug)]
pub struct BuildArtifact {
    pub executable: PathBuf,
    pub object: PathBuf,
}

/// 库构建产物（M15-C）：验证目标文件；M15-D 起附带生成的 `.dlib`。
#[derive(Debug)]
pub struct LibraryArtifact {
    pub object: PathBuf,
    pub package: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildProfile {
    Debug,
    Release,
}

impl BuildProfile {
    /// 根清单的默认 profile：`[build] optimization = "release"` 时为 Release，
    /// 否则 Debug。
    pub fn from_manifest(manifest: &Manifest) -> Self {
        if manifest.build.optimization == "release" {
            Self::Release
        } else {
            Self::Debug
        }
    }

    /// 冻结优先级：显式选择 > 根清单 `build.optimization` > Debug。
    ///
    /// `explicit` 为 `None` 表示调用方没有传 `--debug`/`--release`；`Some` 表示
    /// 显式选择。CLI 必须在命令开始时解析一次，并把同一结果传给库与 bin。
    pub fn resolve(manifest: &Manifest, explicit: Option<Self>) -> Self {
        explicit.unwrap_or_else(|| Self::from_manifest(manifest))
    }
}

pub fn build(options: BuildOptions) -> Result<BuildArtifact, Diagnostic> {
    build_with_profile(options, BuildProfile::Debug, BuildSettings::default())
}

pub fn check(input: &Path) -> Result<(), Diagnostic> {
    compile_frontend(input).map(|_| ())
}

/// 依赖解析结果：包图与最终锁文件。
pub struct ResolvedProject {
    pub graph: PackageGraph,
    pub lock: Lockfile,
}

/// 解析整个依赖闭包，并按锁文件模式读写 `dolphin.lock`（§8.3）。
///
/// - 无锁：解析并写锁；清单图变化则更新；
/// - `--locked`：必须有与清单、仓库映射、编译器版本一致的锁，禁止改写；
/// - `--offline`：不访问 HTTP(S)，只使用缓存/路径/file 仓库。
pub fn resolve_project(
    manifest: &Manifest,
    options: ResolveOptions,
) -> Result<ResolvedProject, Diagnostic> {
    resolve_project_with_cache(manifest, options, Cache::from_env())
}

/// 与 `resolve_project` 相同，但显式指定缓存根（测试隔离）。
pub fn resolve_project_with_cache(
    manifest: &Manifest,
    options: ResolveOptions,
    cache: Cache,
) -> Result<ResolvedProject, Diagnostic> {
    let existing = lockfile::load(&manifest.root)?;
    if options.locked {
        let Some(lock) = &existing else {
            return Err(Diagnostic::plain(format!(
                "`--locked` requires an existing `{}` in `{}`",
                lockfile::LOCKFILE_NAME,
                manifest.root.display()
            )));
        };
        if lock.compiler_version != package_archive::COMPILER_VERSION {
            return Err(Diagnostic::plain(format!(
                "`{}` was written by dc {} but this is dc {}; regenerate the lockfile",
                lockfile::LOCKFILE_NAME,
                lock.compiler_version,
                package_archive::COMPILER_VERSION
            )));
        }
    }

    let mut registry = Registry::new(&manifest.repositories, options, cache, existing.as_ref());
    let graph = resolver::resolve(manifest, options, &mut registry)?;
    let new_lock = Lockfile::from_graph(&graph, &manifest.root);

    if options.locked {
        let lock = existing.expect("checked above");
        if !lock.matches(&graph, &manifest.root) {
            return Err(Diagnostic::plain(format!(
                "`{}` is out of date with `dolphin.toml`; run without `--locked` to update it",
                lockfile::LOCKFILE_NAME
            )));
        }
        return Ok(ResolvedProject { graph, lock });
    }

    if existing.as_ref() != Some(&new_lock) {
        lockfile::write(&manifest.root, &new_lock)?;
    }
    Ok(ResolvedProject {
        graph,
        lock: new_lock,
    })
}

/// 构建清单中声明的可执行目标（仅路径依赖）。
///
/// `bin` 为 `None` 时构建全部目标；为 `Some(name)` 时只构建指定目标。
pub fn build_manifest(
    manifest: &Manifest,
    bin: Option<&str>,
    profile: BuildProfile,
    settings: BuildSettings,
) -> Result<Vec<BuildArtifact>, Diagnostic> {
    let graph = resolver::resolve_paths_only(manifest)?;
    build_manifest_with_graph(manifest, bin, profile, settings, &graph)
}

/// 用已解析的包图构建清单中的可执行目标（M15-E 由 CLI 先解析后调用）。
///
/// `profile` 是调用方已解析的最终选择（优先级：显式 CLI > 根清单
/// `build.optimization` > Debug，见 [`BuildProfile::resolve`]）；依赖包的清单不参与
/// profile 决策。库与 bin 必须用同一最终 profile。
pub fn build_manifest_with_graph(
    manifest: &Manifest,
    bin: Option<&str>,
    profile: BuildProfile,
    settings: BuildSettings,
    graph: &PackageGraph,
) -> Result<Vec<BuildArtifact>, Diagnostic> {
    let targets = select_targets(manifest, bin)?;
    let mut artifacts = Vec::with_capacity(targets.len());
    for target in targets {
        artifacts.push(build_bin(manifest, target, profile, settings, graph)?);
    }
    Ok(artifacts)
}

/// 检查清单中声明的全部可执行目标与库目标（仅路径依赖）。
pub fn check_manifest(manifest: &Manifest) -> Result<(), Diagnostic> {
    let graph = resolver::resolve_paths_only(manifest)?;
    check_manifest_with_graph(manifest, &graph)
}

/// 检查已解析包图中清单声明的全部可执行目标与库目标。
pub fn check_manifest_with_graph(
    manifest: &Manifest,
    graph: &PackageGraph,
) -> Result<(), Diagnostic> {
    let platform = platform::host()?;
    let native = package_native_inputs(graph, platform.as_ref())?;
    validate_native_paths(platform.as_ref(), &native)?;
    if manifest.lib.is_some() {
        compile_library(manifest, graph)?;
    }
    let targets = select_targets(manifest, None)?;
    for target in targets {
        compile_bin(manifest, target, graph)?;
    }
    Ok(())
}

/// 只检查库目标（`dc package` 的验证入口）。
pub fn check_library(manifest: &Manifest) -> Result<(), Diagnostic> {
    let graph = resolver::resolve_paths_only(manifest)?;
    check_library_with_graph(manifest, &graph)
}

/// 检查已解析包图中的库目标。
pub fn check_library_with_graph(
    manifest: &Manifest,
    graph: &PackageGraph,
) -> Result<(), Diagnostic> {
    let platform = platform::host()?;
    let native = package_native_inputs(graph, platform.as_ref())?;
    validate_native_paths(platform.as_ref(), &native)?;
    compile_library(manifest, graph).map(|_| ())
}

/// `dc check` 只校验声明、清单与路径：原生输入必须存在。
fn validate_native_paths(
    platform: &dyn TargetPlatform,
    native: &NativeInputs,
) -> Result<(), Diagnostic> {
    let triple = platform.triple();
    let check = |paths: &[PathBuf], kind: &str| -> Result<(), Diagnostic> {
        for path in paths {
            if !path.is_file() {
                return Err(Diagnostic::plain(format!(
                    "native {kind} `{}` for target `{triple}` does not exist or is not a file",
                    path.display()
                )));
            }
        }
        Ok(())
    };
    check(&native.objects, "object")?;
    check(&native.static_libs, "static library")?;
    check(&native.shared_libs, "shared library")?;
    check(&native.runtime_files, "runtime file")?;
    Ok(())
}

fn select_targets<'a>(
    manifest: &'a Manifest,
    bin: Option<&str>,
) -> Result<Vec<&'a BinTarget>, Diagnostic> {
    if let Some(name) = bin {
        manifest
            .bins
            .iter()
            .find(|target| target.name == name)
            .map(|target| vec![target])
            .ok_or_else(|| {
                Diagnostic::plain(format!(
                    "unknown binary target `{name}` (available: {})",
                    manifest
                        .bins
                        .iter()
                        .map(|target| target.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
    } else {
        Ok(manifest.bins.iter().collect())
    }
}

fn build_bin(
    manifest: &Manifest,
    target: &BinTarget,
    profile: BuildProfile,
    settings: BuildSettings,
    graph: &PackageGraph,
) -> Result<BuildArtifact, Diagnostic> {
    let platform = platform::host()?;
    let base = manifest.build.output.join(&target.name);
    let output = with_executable_suffix(&base, platform.as_ref());
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            Diagnostic::plain(format!(
                "could not create output directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    let program = compile_bin(manifest, target, graph)?;
    let object = object_path(&base, platform.object_suffix());
    select_backend(settings.backend)?.emit_program(
        &program,
        &object,
        profile == BuildProfile::Release,
        platform.as_ref(),
    )?;
    let native = package_native_inputs(graph, platform.as_ref())?;
    linker::link(
        platform.as_ref(),
        &object,
        &output,
        settings.linker,
        profile == BuildProfile::Debug,
        &native,
    )?;
    Ok(BuildArtifact {
        executable: output,
        object,
    })
}

/// 生成库验证目标文件（M15-C，仅路径依赖）。
///
/// 不链接可执行文件，也不生成启动入口；只对当前目标生成本机目标代码。
pub fn build_library(
    manifest: &Manifest,
    profile: BuildProfile,
    settings: BuildSettings,
) -> Result<LibraryArtifact, Diagnostic> {
    let graph = resolver::resolve_paths_only(manifest)?;
    build_library_with_graph(manifest, profile, settings, &graph)
}

/// 用已解析的包图构建库并产出 `.dlib`（M15-D/E）。
pub fn build_library_with_graph(
    manifest: &Manifest,
    profile: BuildProfile,
    settings: BuildSettings,
    graph: &PackageGraph,
) -> Result<LibraryArtifact, Diagnostic> {
    if manifest.lib.is_none() {
        return Err(Diagnostic::plain(format!(
            "`{}` does not declare a `[lib]` target",
            manifest.path.display()
        )));
    }
    let platform = platform::host()?;
    let program = compile_library(manifest, graph)?;
    let directory = manifest.build.output.join("lib");
    fs::create_dir_all(&directory).map_err(|error| {
        Diagnostic::plain(format!(
            "could not create output directory `{}`: {error}",
            directory.display()
        ))
    })?;
    let mut name = manifest.package.name.clone();
    name.push('.');
    name.push_str(platform.object_suffix());
    let object = directory.join(name);
    select_backend(settings.backend)?.emit_program(
        &program,
        &object,
        profile == BuildProfile::Release,
        platform.as_ref(),
    )?;

    // M15-D：`build --lib` 同时产出确定性 `.dlib` 与摘要文件。
    let archive = package_archive::build(manifest, graph)?;
    let package_dir = manifest.build.output.join("package");
    fs::create_dir_all(&package_dir).map_err(|error| {
        Diagnostic::plain(format!(
            "could not create output directory `{}`: {error}",
            package_dir.display()
        ))
    })?;
    let dlib = package_dir.join(format!(
        "{}-{}.dlib",
        manifest.package.name, manifest.package.version
    ));
    dolphin_package::cache::write_atomic(&dlib, &archive)?;
    let digest = package_archive::sha256_hex(&archive);
    let mut checksum_name = dlib.file_name().unwrap_or_default().to_os_string();
    checksum_name.push(".sha256");
    let checksum = dlib.with_file_name(checksum_name);
    dolphin_package::cache::write_atomic(&checksum, format!("{digest}\n").as_bytes())?;

    Ok(LibraryArtifact {
        object,
        package: Some(dlib),
    })
}

/// 用户测试函数（M19/H19-05b）：定义在包根 `tests/` 直接子文件中的 `test_*`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFunction {
    /// 全限定名；测试都编译为包根模块文件，因此就是函数名。
    pub name: String,
    /// 定义所在文件（诊断与调试）。
    pub path: PathBuf,
}

/// 测试目标：发现到的测试与构建产物（H19-05c 的 runner 消费同一列表）。
#[derive(Debug)]
pub struct TestTarget {
    pub tests: Vec<TestFunction>,
    pub artifact: BuildArtifact,
}

/// 发现包根 `tests/` 的**直接子文件** `*.do`（不递归）中的 `test_*` 函数，按全限定名排序。
///
/// 冻结校验（规格 §9.1）：测试文件不得声明 `pkg`、不得定义 `main`；`test_*` 必须无参数、
/// 无类型参数、返回 Unit（无返回类型标注）且必须是带函数体的普通函数。其他函数允许作为
/// helper。目录不存在或没有 `*.do` 时视为 0 测试；同名测试给出明确诊断（它们同属根模块）。
pub fn discover_tests(manifest: &Manifest) -> Result<Vec<TestFunction>, Diagnostic> {
    let paths = test_source_paths(manifest)?;
    let mut tests = Vec::new();
    for path in &paths {
        let text = fs::read_to_string(path).map_err(|error| {
            Diagnostic::plain(format!("could not read `{}`: {error}", path.display()))
        })?;
        let source = SourceFile::new(path.clone(), text);
        let tokens = lexer::lex(&source)?;
        let program = parser::parse(&source, tokens)?;
        if let Some(package) = &program.package {
            return Err(Diagnostic::at(
                &source,
                package.span,
                "test files belong to the root module and must omit `pkg`",
            ));
        }
        for function in &program.functions {
            if function.name == "main" {
                return Err(Diagnostic::at(
                    &source,
                    function.name_span,
                    "test files must not define `main`; `dc test` generates the entry",
                ));
            }
            if !function.name.starts_with("test_") {
                continue;
            }
            if function.extern_c {
                return Err(Diagnostic::at(
                    &source,
                    function.name_span,
                    format!("test `{}` must be a function with a body", function.name),
                ));
            }
            if !function.type_params.is_empty() {
                return Err(Diagnostic::at(
                    &source,
                    function.name_span,
                    format!("test `{}` must not declare type parameters", function.name),
                ));
            }
            if !function.parameters.is_empty() {
                return Err(Diagnostic::at(
                    &source,
                    function.name_span,
                    format!("test `{}` must not take parameters", function.name),
                ));
            }
            if function.return_type.is_some() {
                return Err(Diagnostic::at(
                    &source,
                    function.name_span,
                    format!(
                        "test `{}` must return Unit (omit the return type)",
                        function.name
                    ),
                ));
            }
            tests.push(TestFunction {
                name: function.name.clone(),
                path: path.clone(),
            });
        }
    }
    tests.sort_by(|left, right| left.name.cmp(&right.name));
    for pair in tests.windows(2) {
        if pair[0].name == pair[1].name {
            return Err(Diagnostic::plain(format!(
                "duplicate test `{}` defined in `{}` and `{}`",
                pair[0].name,
                pair[0].path.display(),
                pair[1].path.display()
            )));
        }
    }
    Ok(tests)
}

/// 包根 `tests/` 的直接子文件 `*.do`（不递归），按路径排序；目录不存在视为空。
///
/// 只含 helper 的测试文件也要加载（与含测试的文件同属根模块），因此注入时用完整列表。
fn test_source_paths(manifest: &Manifest) -> Result<Vec<PathBuf>, Diagnostic> {
    let directory = manifest.root.join("tests");
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(&directory).map_err(|error| {
        Diagnostic::plain(format!(
            "could not read test directory `{}`: {error}",
            directory.display()
        ))
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| Diagnostic::plain(format!("could not read test entry: {error}")))?;
        let file_type = entry.file_type().map_err(|error| {
            Diagnostic::plain(format!(
                "could not inspect `{}`: {error}",
                entry.path().display()
            ))
        })?;
        let path = entry.path();
        if file_type.is_file() && path.extension().is_some_and(|extension| extension == "do") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// 生成测试入口（M19/H19-05b）：按内部参数 `--dolphin-test <名称>` 分发到发现的测试。
///
/// 无测试时生成不接收参数、成功退出的占位入口（`dc test` 仍会构建测试目标）。
/// 未知名称返回 3、缺少/错误参数返回 2，均由 runner（H19-05c）识别；该接口不对外承诺。
pub fn generate_test_harness(tests: &[TestFunction]) -> String {
    if tests.is_empty() {
        return "fn main(): i32 {\n    return 0;\n}\n".to_string();
    }
    let mut source = String::new();
    source.push_str("// Generated by `dc test` (M19/H19-05b). Do not edit.\n");
    source.push_str("use std.process.arg;\nuse std.process.arg_count;\n\n");
    source.push_str("fn main(): i32 {\n");
    source.push_str("    if arg_count() != 3_usize {\n        return 2;\n    }\n");
    source.push_str("    val flag = arg(1_usize);\n");
    source.push_str("    val name = arg(2_usize);\n");
    source.push_str("    if flag.is_err() || name.is_err() {\n        return 2;\n    }\n");
    source.push_str(
        "    val flag_text = match flag {\n        Result.Ok(value) => value,\n        Result.Err(error) => \"\",\n    };\n",
    );
    source.push_str(
        "    val name_text = match name {\n        Result.Ok(value) => value,\n        Result.Err(error) => \"\",\n    };\n",
    );
    source.push_str("    if flag_text != \"--dolphin-test\" {\n        return 2;\n    }\n");
    for test in tests {
        source.push_str(&format!(
            "    if name_text == \"{}\" {{\n        {}();\n        return 0;\n    }}\n",
            test.name, test.name
        ));
    }
    source.push_str("    return 3;\n}\n");
    source
}

/// 发现并构建测试目标（M19/H19-05b，仅路径依赖）。
pub fn build_test_target(
    manifest: &Manifest,
    profile: BuildProfile,
    settings: BuildSettings,
) -> Result<TestTarget, Diagnostic> {
    let graph = resolver::resolve_paths_only(manifest)?;
    build_test_target_with_graph(manifest, profile, settings, &graph)
}

/// 用已解析的包图发现并构建测试目标（CLI 先解析完整包图后调用）。
///
/// 测试文件与生成入口都作为根模块源码注入：测试可访问根模块私有项与子模块 `pub` 项。
pub fn build_test_target_with_graph(
    manifest: &Manifest,
    profile: BuildProfile,
    settings: BuildSettings,
    graph: &PackageGraph,
) -> Result<TestTarget, Diagnostic> {
    let tests = discover_tests(manifest)?;
    let entry_source = generate_test_harness(&tests);
    let entry = write_test_entry(manifest, &entry_source)?;
    let mut extra = vec![modules::ExtraSource {
        path: entry,
        text: entry_source,
    }];
    // 注入 `tests/` 的全部直接子文件（含只定义 helper 的文件），按路径排序保持确定性。
    for path in test_source_paths(manifest)? {
        let text = fs::read_to_string(&path).map_err(|error| {
            Diagnostic::plain(format!("could not read `{}`: {error}", path.display()))
        })?;
        extra.push(modules::ExtraSource { path, text });
    }
    let artifact = build_test_sources_with_graph(manifest, profile, settings, graph, extra)?;
    Ok(TestTarget { tests, artifact })
}

/// 把生成的测试入口写入 `target/test/`（不写入用户 `src/`），返回其路径。
fn write_test_entry(manifest: &Manifest, entry_source: &str) -> Result<PathBuf, Diagnostic> {
    let directory = manifest.build.output.join("test");
    fs::create_dir_all(&directory).map_err(|error| {
        Diagnostic::plain(format!(
            "could not create output directory `{}`: {error}",
            directory.display()
        ))
    })?;
    let entry = directory.join(format!("{}-tests.entry.do", manifest.package.name));
    fs::write(&entry, entry_source).map_err(|error| {
        Diagnostic::plain(format!(
            "could not write generated test entry `{}`: {error}",
            entry.display()
        ))
    })?;
    Ok(entry)
}

/// 构建测试目标（M19/H19-05a，仅路径依赖；CLI 先解析完整包图后调用 `_with_graph`）。
pub fn build_tests(
    manifest: &Manifest,
    profile: BuildProfile,
    settings: BuildSettings,
    entry_source: &str,
) -> Result<BuildArtifact, Diagnostic> {
    let graph = resolver::resolve_paths_only(manifest)?;
    build_tests_with_graph(manifest, profile, settings, &graph, entry_source)
}

/// 构建测试目标（M19/H19-05a）：库源码 + 生成的根模块入口 -> `target/test/<包名>-tests`。
///
/// 这是**开发/测试目标入口**，与 `build --lib` 不同：不产出 `.dlib`、不要求可发布性，
/// 因此可以解析 path 依赖（D1：不放宽打包限制）。根包排除全部 `[[bin]]` 入口，
/// 生成入口作为根模块源码注入（不写入用户 `src/`）；依赖包只贡献库源码。
/// 需要 `[lib]` 目标；`main` 由生成的入口提供。
pub fn build_tests_with_graph(
    manifest: &Manifest,
    profile: BuildProfile,
    settings: BuildSettings,
    graph: &PackageGraph,
    entry_source: &str,
) -> Result<BuildArtifact, Diagnostic> {
    let entry = write_test_entry(manifest, entry_source)?;
    build_test_sources_with_graph(
        manifest,
        profile,
        settings,
        graph,
        vec![modules::ExtraSource {
            path: entry,
            text: entry_source.to_string(),
        }],
    )
}

/// 测试目标构建的公共实现：注入的根模块源码 + 库源码 -> `target/test/<包名>-tests`。
fn build_test_sources_with_graph(
    manifest: &Manifest,
    profile: BuildProfile,
    settings: BuildSettings,
    graph: &PackageGraph,
    extra: Vec<modules::ExtraSource>,
) -> Result<BuildArtifact, Diagnostic> {
    if manifest.lib.is_none() {
        return Err(Diagnostic::plain(format!(
            "`{}` does not declare a `[lib]` target; `dc test` requires a library target",
            manifest.path.display()
        )));
    }
    let platform = platform::host()?;
    let directory = manifest.build.output.join("test");
    let base = directory.join(format!("{}-tests", manifest.package.name));
    let output = with_executable_suffix(&base, platform.as_ref());
    let exclude: HashSet<PathBuf> = manifest.bins.iter().map(|bin| bin.path.clone()).collect();
    let loaded = load_graph_sources_with_extra(graph, exclude, extra)?;
    let program = lower::lower_sources(&loaded.sources, &loaded.program, &loaded.packages)?;
    let object = object_path(&base, platform.object_suffix());
    select_backend(settings.backend)?.emit_program(
        &program,
        &object,
        profile == BuildProfile::Release,
        platform.as_ref(),
    )?;
    let native = package_native_inputs(graph, platform.as_ref())?;
    linker::link(
        platform.as_ref(),
        &object,
        &output,
        settings.linker,
        profile == BuildProfile::Debug,
        &native,
    )?;
    Ok(BuildArtifact {
        executable: output,
        object,
    })
}

/// 汇总依赖图中当前目标三元组的原生链接输入。
///
/// 顺序为反向拓扑（使用者先于提供者），共享依赖排在全部使用者之后；同包沿用
/// 声明顺序；文件按规范化路径去重。同名不同内容的 runtime 文件报链接冲突。
fn package_native_inputs(
    graph: &PackageGraph,
    platform: &dyn TargetPlatform,
) -> Result<NativeInputs, Diagnostic> {
    let triple = platform.triple().to_string();
    let mut native = NativeInputs::default();
    let mut seen_objects: HashSet<PathBuf> = HashSet::new();
    let mut seen_static: HashSet<PathBuf> = HashSet::new();
    let mut seen_shared: HashSet<PathBuf> = HashSet::new();
    let mut runtime_by_name: BTreeMap<String, PathBuf> = BTreeMap::new();
    for package in graph.order_iter() {
        let Some(spec) = package.manifest.native_for(&triple) else {
            if !package.manifest.native.is_empty() {
                return Err(Diagnostic::plain(format!(
                    "package `{}` ships native files but not for target `{triple}`; supported targets: {}",
                    package.coordinate(),
                    package
                        .manifest
                        .native
                        .iter()
                        .map(|(key, _)| key.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            continue;
        };
        for path in &spec.objects {
            if seen_objects.insert(path.clone()) {
                native.objects.push(path.clone());
            }
        }
        for path in &spec.static_libs {
            if seen_static.insert(path.clone()) {
                native.static_libs.push(path.clone());
            }
        }
        for path in &spec.shared_libs {
            if seen_shared.insert(path.clone()) {
                native.shared_libs.push(path.clone());
            }
        }
        for path in &spec.runtime_files {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    Diagnostic::plain(format!("invalid runtime file `{}`", path.display()))
                })?
                .to_string();
            match runtime_by_name.get(&name) {
                Some(existing) if existing != path => {
                    if !same_file_contents(existing, path) {
                        return Err(Diagnostic::plain(format!(
                            "runtime file conflict: `{}` and `{}` share the name `{name}` but differ in content",
                            existing.display(),
                            path.display()
                        )));
                    }
                }
                Some(_) => {}
                None => {
                    runtime_by_name.insert(name, path.clone());
                    native.runtime_files.push(path.clone());
                }
            }
        }
    }
    Ok(native)
}

fn same_file_contents(left: &Path, right: &Path) -> bool {
    match (fs::read(left), fs::read(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// 编译单个可执行目标：从包图源码根加载模块，排除其他目标的入口文件。
fn compile_bin(
    manifest: &Manifest,
    target: &BinTarget,
    graph: &PackageGraph,
) -> Result<ir::Program, Diagnostic> {
    if !target.path.is_file() {
        return Err(Diagnostic::plain(format!(
            "binary target `{}` entry `{}` does not exist",
            target.name,
            target.path.display()
        )));
    }
    let exclude: HashSet<PathBuf> = manifest
        .bins
        .iter()
        .filter(|other| other.name != target.name)
        .map(|other| other.path.clone())
        .collect();
    let loaded = load_graph_sources(graph, exclude)?;
    lower::lower_sources(&loaded.sources, &loaded.program, &loaded.packages)
}

/// 编译库目标：排除全部 bin 入口，不要求 `main`。
fn compile_library(manifest: &Manifest, graph: &PackageGraph) -> Result<ir::Program, Diagnostic> {
    let lib = manifest.lib.as_ref().ok_or_else(|| {
        Diagnostic::plain(format!(
            "`{}` does not declare a `[lib]` target",
            manifest.path.display()
        ))
    })?;
    if !lib.path.is_file() {
        return Err(Diagnostic::plain(format!(
            "library entry `{}` does not exist",
            lib.path.display()
        )));
    }
    let exclude: HashSet<PathBuf> = manifest.bins.iter().map(|bin| bin.path.clone()).collect();
    let loaded = load_graph_sources(graph, exclude)?;
    lower::lower_library(&loaded.sources, &loaded.program, &loaded.packages)
}

/// 把包图中的各包源码根转为模块加载配置；根包排除指定入口文件。
///
/// 依赖包只贡献库源码：它自己的 `[[bin]]` 入口永远排除，因此同时声明 lib 与多个
/// bin 的 path 依赖不会把 bin `main` 带入使用者，行为与消费发布后的 `.dlib`
/// （归档已排除 bin）一致；库与 bin 共享的非入口 helper 文件仍然加载。
fn load_graph_sources(
    graph: &PackageGraph,
    root_exclude: HashSet<PathBuf>,
) -> Result<modules::LoadedProgram, Diagnostic> {
    load_graph_sources_with_extra(graph, root_exclude, Vec::new())
}

/// 与 [`load_graph_sources`] 相同，但给根包注入额外根模块源码（`dc test` 的
/// 生成入口；后续 `tests/*.do` 也经此进入），不参与磁盘发现。
fn load_graph_sources_with_extra(
    graph: &PackageGraph,
    root_exclude: HashSet<PathBuf>,
    root_extra: Vec<modules::ExtraSource>,
) -> Result<modules::LoadedProgram, Diagnostic> {
    let packages: Vec<modules::PackageSources> = graph
        .packages
        .iter()
        .map(|package| modules::PackageSources {
            id: package.id,
            prefix: package.prefix.clone(),
            aliases: package
                .aliases
                .iter()
                .map(|(alias, target)| (alias.clone(), graph.get(*target).prefix.clone()))
                .collect(),
            source_root: package.source_root.clone(),
            exclude: if package.id == graph.root {
                root_exclude.clone()
            } else {
                package
                    .manifest
                    .bins
                    .iter()
                    .map(|bin| bin.path.clone())
                    .collect()
            },
            extra: if package.id == graph.root {
                root_extra.clone()
            } else {
                Vec::new()
            },
        })
        .collect();
    modules::load_packages(&packages)
}

pub fn build_with_profile(
    options: BuildOptions,
    profile: BuildProfile,
    settings: BuildSettings,
) -> Result<BuildArtifact, Diagnostic> {
    let platform = platform::host()?;
    let default_output = default_output(&options.input)?;
    let base = options.output.unwrap_or(default_output);
    let output = with_executable_suffix(&base, platform.as_ref());
    let program = compile_frontend(&options.input)?;

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            Diagnostic::plain(format!(
                "could not create output directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }

    let object = object_path(&base, platform.object_suffix());
    select_backend(settings.backend)?.emit_program(
        &program,
        &object,
        profile == BuildProfile::Release,
        platform.as_ref(),
    )?;
    linker::link(
        platform.as_ref(),
        &object,
        &output,
        settings.linker,
        profile == BuildProfile::Debug,
        &NativeInputs::default(),
    )?;

    Ok(BuildArtifact {
        executable: output,
        object,
    })
}

/// 从 `start` 目录（或其任意祖先目录）查找 `dolphin.toml`。
///
/// 返回清单；未找到时返回 `None`，调用方回退到旧行为。
pub fn discover_manifest(start: &Path) -> Result<Option<Manifest>, Diagnostic> {
    let start = std::path::absolute(start).unwrap_or_else(|_| start.to_path_buf());
    let mut current = if start.is_dir() {
        start
    } else {
        start
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };
    loop {
        let candidate = current.join("dolphin.toml");
        if candidate.is_file() {
            return manifest::load(&current).map(Some);
        }
        if !current.pop() {
            return Ok(None);
        }
    }
}

/// 发布当前库包到仓库（M15-F，§9.2）。
///
/// v1 只发布当前库，不递归上传依赖；返回发布结果描述。
pub fn publish_library(
    manifest: &Manifest,
    _graph: &PackageGraph,
    repository: &str,
    package_path: &Path,
    options: ResolveOptions,
) -> Result<String, Diagnostic> {
    let bytes = fs::read(package_path).map_err(|error| {
        Diagnostic::plain(format!(
            "could not read `{}`: {error}",
            package_path.display()
        ))
    })?;
    let registry = Registry::new(&manifest.repositories, options, Cache::from_env(), None);
    let outcome = registry.publish(&manifest.package.coordinate(), repository, &bytes)?;
    Ok(match outcome {
        registry::PublishOutcome::Uploaded => "uploaded".to_string(),
        registry::PublishOutcome::AlreadyPresent => "already present (idempotent)".to_string(),
        registry::PublishOutcome::CompletedChecksum => {
            "checksum completed for an existing package".to_string()
        }
    })
}

fn compile_frontend(input: &Path) -> Result<ir::Program, Diagnostic> {
    if input.is_dir() {
        let loaded = modules::load_project(input)?;
        lower::lower_sources(&loaded.sources, &loaded.program, &loaded.packages)
    } else {
        let source_text = fs::read_to_string(input).map_err(|error| {
            Diagnostic::plain(format!("could not read `{}`: {error}", input.display()))
        })?;
        let source = SourceFile::new(input.to_path_buf(), source_text);
        let tokens = lexer::lex(&source)?;
        let program = parser::parse(&source, tokens)?;
        if let Some(package) = &program.package {
            return Err(Diagnostic::at(
                &source,
                package.span,
                "`pkg` requires compiling a project directory",
            ));
        }
        if let Some(import) = program.uses.first() {
            return Err(Diagnostic::at(
                &source,
                import.span,
                "`use` requires compiling a project directory",
            ));
        }
        lower::lower(&source, &program)
    }
}

fn default_output(input: &Path) -> Result<PathBuf, Diagnostic> {
    if input.is_dir() {
        let project_name = input
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("main");
        let output = input.join("target").join(project_name);
        Ok(output)
    } else if input.is_file() {
        let stem = input
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("main");
        let parent = input.parent().unwrap_or_else(|| Path::new("."));
        Ok(parent.join("target").join(stem))
    } else {
        Err(Diagnostic::plain(format!(
            "input `{}` does not exist",
            input.display()
        )))
    }
}

fn object_path(output: &Path, suffix: &str) -> PathBuf {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    output.with_file_name(name)
}

/// 按平台规则补全可执行文件后缀（Windows 为 `.exe`；已带后缀时保持原样）。
fn with_executable_suffix(base: &Path, platform: &dyn TargetPlatform) -> PathBuf {
    let suffix = platform.executable_suffix();
    if suffix.is_empty() {
        return base.to_path_buf();
    }
    let expected = format!(".{suffix}");
    let name = base.file_name().unwrap_or_default().to_os_string();
    if name.to_string_lossy().ends_with(&expected) {
        return base.to_path_buf();
    }
    let mut with_suffix = name;
    with_suffix.push(&expected);
    base.with_file_name(with_suffix)
}
