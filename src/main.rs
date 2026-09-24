use std::ffi::OsStr;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant};

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use dolphin_compiler::{
    BackendChoice, BuildArtifact, BuildOptions, BuildProfile, BuildSettings, Cache, DependencyKind,
    Diagnostic, LibraryArtifact, LinkerChoice, Manifest, ResolveOptions, build_library_with_graph,
    build_manifest_with_graph, build_test_target_with_graph, build_with_profile,
    check_library_with_graph, check_manifest_collecting, check_source_collecting,
    discover_manifest, host_platform, publish_library, resolve_project,
};

#[derive(Parser)]
#[command(
    name = "dc",
    version,
    about = "Compile and run Dolphin programs",
    long_about = "Dolphin native compiler\n\nChecks, builds, and runs single-file programs or projects whose sources live under src/, optionally driven by a dolphin.toml manifest."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Control colored diagnostic output
    #[arg(long, value_enum, default_value_t = ColorMode::Auto, global = true)]
    color: ColorMode,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse and type-check without producing build artifacts
    Check(CheckArgs),

    /// Compile to a native executable
    Build(BuildArgs),

    /// Compile and execute the program
    Run(RunArgs),

    /// Discover and run user tests in tests/ (M19/H19-05)
    Test(TestArgs),

    /// Produce a `.dlib` library package from the current library target
    Package(ProjectArgs),

    /// Resolve and download the dependency closure, writing dolphin.lock
    Fetch(ProjectArgs),

    /// Publish the current library to a repository
    Publish(PublishArgs),

    /// Show the package coordinate and build configuration of a project
    Info {
        /// Dolphin project directory (defaults to the current directory)
        input: Option<PathBuf>,
    },

    /// Show the host platform, target triple, and selected linker
    Env,

    /// Format Dolphin source files
    Fmt(FmtArgs),

    /// Run the language server over stdin/stdout
    Lsp {
        /// Dolphin project directory (defaults to the current directory)
        project: Option<PathBuf>,
    },
}

#[derive(Args)]
struct CheckArgs {
    /// Dolphin project directory or a standalone .do file (defaults to the current directory)
    input: Option<PathBuf>,

    /// Require an up-to-date dolphin.lock and do not rewrite it
    #[arg(long)]
    locked: bool,

    /// Do not access HTTP(S) repositories
    #[arg(long)]
    offline: bool,
}

#[derive(Args)]
struct FmtArgs {
    /// Files or directories to format (defaults to `[package].source`, else `src`)
    paths: Vec<PathBuf>,

    /// Check formatting without writing files (non-zero exit if changes are needed)
    #[arg(long)]
    check: bool,
}

/// 依赖解析开关，供 check/build/run/package/fetch/publish 复用。
#[derive(Args, Clone, Copy)]
struct DependencyArgs {
    /// Require an up-to-date dolphin.lock and do not rewrite it
    #[arg(long)]
    locked: bool,

    /// Do not access HTTP(S) repositories
    #[arg(long)]
    offline: bool,
}

impl DependencyArgs {
    fn options(self) -> ResolveOptions {
        ResolveOptions {
            offline: self.offline,
            locked: self.locked,
        }
    }
}

#[derive(Args)]
struct ProjectArgs {
    /// Dolphin project directory (defaults to the current directory)
    input: Option<PathBuf>,

    #[command(flatten)]
    dependency: DependencyArgs,
}

#[derive(Args)]
struct PublishArgs {
    /// Dolphin project directory (defaults to the current directory)
    input: Option<PathBuf>,

    /// Repository id from `[repositories]` (defaults to `default`)
    #[arg(long)]
    repository: Option<String>,

    #[command(flatten)]
    dependency: DependencyArgs,
}

#[derive(Args)]
struct BuildArgs {
    /// Dolphin project directory or a standalone .do file (defaults to the current directory)
    input: Option<PathBuf>,

    /// Select which binary target to build or run (for dolphin.toml projects)
    #[arg(long)]
    bin: Option<String>,

    /// Build the library target instead of binary targets
    #[arg(long, conflicts_with = "bin")]
    lib: bool,

    /// Write the executable to this path (single-file mode only)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Disable optimizations for faster compilation
    #[arg(long, conflicts_with = "release")]
    debug: bool,

    /// Enable speed optimizations
    #[arg(long, conflicts_with = "debug")]
    release: bool,

    /// Fall back to the system linker (cc/link) instead of the bundled rust-lld
    #[arg(long)]
    system_linker: bool,

    /// Select the code generation backend (defaults to $DOLPHIN_BACKEND or cranelift)
    #[arg(long, value_enum)]
    backend: Option<BackendArg>,

    /// Require an up-to-date dolphin.lock and do not rewrite it
    #[arg(long)]
    locked: bool,

    /// Do not access HTTP(S) repositories
    #[arg(long)]
    offline: bool,
}

/// `dc run` 的参数：编译选项 + `--` 之后原样转发给应用（H19-01）。
#[derive(Args)]
struct RunArgs {
    #[command(flatten)]
    build: BuildArgs,

    /// Application arguments after `--`, passed through unchanged
    #[arg(last = true)]
    app_args: Vec<std::ffi::OsString>,
}

/// `dc test` 的编译选项（M19/H19-05a/b）与执行选项（H19-05c）。
#[derive(Args)]
struct TestArgs {
    /// Dolphin project directory (defaults to the current directory)
    input: Option<PathBuf>,

    /// Only run tests whose fully qualified name contains this substring
    #[arg(long)]
    filter: Option<String>,

    /// Disable optimizations for faster compilation
    #[arg(long, conflicts_with = "release")]
    debug: bool,

    /// Enable speed optimizations
    #[arg(long, conflicts_with = "debug")]
    release: bool,

    /// Fall back to the system linker (cc/link) instead of the bundled rust-lld
    #[arg(long)]
    system_linker: bool,

    /// Select the code generation backend (defaults to $DOLPHIN_BACKEND or cranelift)
    #[arg(long, value_enum)]
    backend: Option<BackendArg>,

    /// Require an up-to-date dolphin.lock and do not rewrite it
    #[arg(long)]
    locked: bool,

    /// Do not access HTTP(S) repositories
    #[arg(long)]
    offline: bool,
}

/// 显式 profile：`--release`/`--debug` 才算显式；两者都没传返回 `None`，
/// 由命令在发现清单后按 `显式 > 清单 > Debug` 解析（`BuildProfile::resolve`）。
fn explicit_profile(release: bool, debug: bool) -> Option<BuildProfile> {
    if release {
        Some(BuildProfile::Release)
    } else if debug {
        Some(BuildProfile::Debug)
    } else {
        None
    }
}

fn compile_settings(system_linker: bool, backend: Option<BackendArg>) -> BuildSettings {
    let linker = if system_linker {
        LinkerChoice::System
    } else {
        LinkerChoice::default()
    };
    let backend = match backend {
        Some(BackendArg::Cranelift) => BackendChoice::Cranelift,
        Some(BackendArg::Llvm) => BackendChoice::Llvm,
        None => BackendChoice::default(),
    };
    BuildSettings { linker, backend }
}

fn resolve_options(locked: bool, offline: bool) -> ResolveOptions {
    ResolveOptions { offline, locked }
}

impl BuildArgs {
    fn explicit_profile(&self) -> Option<BuildProfile> {
        explicit_profile(self.release, self.debug)
    }

    fn settings(&self) -> BuildSettings {
        compile_settings(self.system_linker, self.backend)
    }

    fn resolve_options(&self) -> ResolveOptions {
        resolve_options(self.locked, self.offline)
    }
}

impl TestArgs {
    fn explicit_profile(&self) -> Option<BuildProfile> {
        explicit_profile(self.release, self.debug)
    }

    fn settings(&self) -> BuildSettings {
        compile_settings(self.system_linker, self.backend)
    }

    fn resolve_options(&self) -> ResolveOptions {
        resolve_options(self.locked, self.offline)
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

/// `--backend` 的取值（与 `BackendChoice` 对应，避免让 driver 依赖 clap）。
#[derive(Clone, Copy, ValueEnum)]
enum BackendArg {
    Cranelift,
    Llvm,
}

fn main() -> ExitCode {
    let mut command = Cli::command();
    if let Some(binary_name) = option_env!("CARGO_BIN_NAME") {
        // 显式固定显示名，避免 clap 从 argv[0] 取到带 `.exe` 后缀的文件名。
        command = command.name(binary_name).bin_name(binary_name);
    }
    let cli = Cli::from_arg_matches(&command.get_matches())
        .expect("Clap matches must satisfy the derived CLI schema");
    match execute(cli) {
        Ok(code) => code,
        Err((message, color)) => {
            eprintln!("{}", render_error(message, color));
            ExitCode::FAILURE
        }
    }
}

fn execute(cli: Cli) -> Result<ExitCode, (String, ColorMode)> {
    let color = cli.color;
    match cli.command {
        Commands::Check(args) => {
            let input = args.input.unwrap_or_else(|| PathBuf::from("."));
            match discover_manifest(&input).map_err(|error| (error.to_string(), color))? {
                Some(manifest) => {
                    let options = ResolveOptions {
                        offline: args.offline,
                        locked: args.locked,
                    };
                    let resolved = resolve_project(&manifest, options)
                        .map_err(|error| (error.to_string(), color))?;
                    let diagnostics = check_manifest_collecting(&manifest, &resolved.graph);
                    if !diagnostics.is_empty() {
                        print_diagnostics(&diagnostics);
                        return Ok(ExitCode::FAILURE);
                    }
                    println!("Checked {}", manifest.package.coordinate());
                }
                None => {
                    if args.locked || args.offline {
                        return Err((
                            "`--locked`/`--offline` require a project with a `dolphin.toml` manifest"
                                .to_string(),
                            color,
                        ));
                    }
                    let diagnostics = check_source_collecting(&input);
                    if !diagnostics.is_empty() {
                        print_diagnostics(&diagnostics);
                        return Ok(ExitCode::FAILURE);
                    }
                    println!("Checked {}", input.display());
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Commands::Build(args) => {
            let explicit = args.explicit_profile();
            let settings = args.settings();
            let input = args.input.clone().unwrap_or_else(|| PathBuf::from("."));
            let Some(manifest) =
                discover_manifest(&input).map_err(|error| (error.to_string(), color))?
            else {
                if args.lib || args.bin.is_some() {
                    return Err((
                        "`--lib`/`--bin` require a project with a `dolphin.toml` manifest"
                            .to_string(),
                        color,
                    ));
                }
                if args.locked || args.offline {
                    return Err((
                        "`--locked`/`--offline` require a project with a `dolphin.toml` manifest"
                            .to_string(),
                        color,
                    ));
                }
                // 单文件模式没有清单，默认 profile 为 Debug。
                let diagnostics = check_source_collecting(&input);
                if !diagnostics.is_empty() {
                    print_diagnostics(&diagnostics);
                    return Ok(ExitCode::FAILURE);
                }
                let artifact = build_with_profile(
                    BuildOptions {
                        input,
                        output: args.output.clone(),
                    },
                    explicit.unwrap_or(BuildProfile::Debug),
                    settings,
                )
                .map_err(|error| (error.to_string(), color))?;
                println!("Built {}", artifact.executable.display());
                return Ok(ExitCode::SUCCESS);
            };

            let profile = BuildProfile::resolve(&manifest, explicit);
            let resolved = resolve_project(&manifest, args.resolve_options())
                .map_err(|error| (error.to_string(), color))?;
            let diagnostics = check_manifest_collecting(&manifest, &resolved.graph);
            if !diagnostics.is_empty() {
                print_diagnostics(&diagnostics);
                return Ok(ExitCode::FAILURE);
            }
            if args.lib {
                let artifact =
                    build_library_with_graph(&manifest, profile, settings, &resolved.graph)
                        .map_err(|error| (error.to_string(), color))?;
                println!("Built {}", library_output(&artifact).display());
                return Ok(ExitCode::SUCCESS);
            }
            // 清单声明了 `[lib]` 且未指定单个 bin 时，同时构建库与全部 bin。
            if args.bin.is_none() && manifest.lib.is_some() {
                let artifact =
                    build_library_with_graph(&manifest, profile, settings, &resolved.graph)
                        .map_err(|error| (error.to_string(), color))?;
                println!("Built {}", library_output(&artifact).display());
            }
            let artifacts = build_manifest_with_graph(
                &manifest,
                args.bin.as_deref(),
                profile,
                settings,
                &resolved.graph,
            )
            .map_err(|error| (error.to_string(), color))?;
            for artifact in artifacts {
                println!("Built {}", artifact.executable.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Commands::Run(args) => {
            let app_args = args.app_args;
            let args = args.build;
            if args.lib {
                return Err((
                    "`--lib` builds a library; use `dc run --bin <name>` for an executable"
                        .to_string(),
                    color,
                ));
            }
            let explicit = args.explicit_profile();
            let settings = args.settings();
            let input = args.input.clone().unwrap_or_else(|| PathBuf::from("."));
            let Some(manifest) =
                discover_manifest(&input).map_err(|error| (error.to_string(), color))?
            else {
                if args.bin.is_some() || args.locked || args.offline {
                    return Err((
                        "`--bin`/`--locked`/`--offline` require a project with a `dolphin.toml` manifest"
                            .to_string(),
                        color,
                    ));
                }
                let diagnostics = check_source_collecting(&input);
                if !diagnostics.is_empty() {
                    print_diagnostics(&diagnostics);
                    return Ok(ExitCode::FAILURE);
                }
                let artifact = build_with_profile(
                    BuildOptions {
                        input,
                        output: args.output.clone(),
                    },
                    explicit.unwrap_or(BuildProfile::Debug),
                    settings,
                )
                .map_err(|error| (error.to_string(), color))?;
                return run_executable(&artifact, &app_args);
            };
            let profile = BuildProfile::resolve(&manifest, explicit);
            let resolved = resolve_project(&manifest, args.resolve_options())
                .map_err(|error| (error.to_string(), color))?;
            let diagnostics = check_manifest_collecting(&manifest, &resolved.graph);
            if !diagnostics.is_empty() {
                print_diagnostics(&diagnostics);
                return Ok(ExitCode::FAILURE);
            }
            let artifacts = build_manifest_with_graph(
                &manifest,
                args.bin.as_deref(),
                profile,
                settings,
                &resolved.graph,
            )
            .map_err(|error| (error.to_string(), color))?;
            if artifacts.is_empty() {
                return Err((
                    "this project only declares a library and cannot be run; add a `[[bin]]` target or use `dc build --lib`".to_string(),
                    color,
                ));
            }
            let artifact = select_run_target(&artifacts, color)?;
            run_executable(artifact, &app_args)
        }
        Commands::Test(args) => {
            let input = args.input.clone().unwrap_or_else(|| PathBuf::from("."));
            let manifest = require_manifest(&input, color)?;
            let profile = BuildProfile::resolve(&manifest, args.explicit_profile());
            let resolved = resolve_project(&manifest, args.resolve_options())
                .map_err(|error| (error.to_string(), color))?;
            let diagnostics = check_manifest_collecting(&manifest, &resolved.graph);
            if !diagnostics.is_empty() {
                print_diagnostics(&diagnostics);
                return Ok(ExitCode::FAILURE);
            }
            // H19-05c：发现 + 生成 harness + 构建，然后逐个子进程运行选中的测试。
            // 0 测试与过滤无匹配按冻结规则只输出固定消息并以 1 退出。
            let target =
                build_test_target_with_graph(&manifest, profile, args.settings(), &resolved.graph)
                    .map_err(|error| (error.to_string(), color))?;
            if target.tests.is_empty() {
                println!("no tests found");
                return Ok(ExitCode::FAILURE);
            }
            let selected: Vec<_> = match &args.filter {
                Some(filter) => target
                    .tests
                    .iter()
                    .filter(|test| test.name.contains(filter.as_str()))
                    .collect(),
                None => target.tests.iter().collect(),
            };
            if selected.is_empty() {
                println!("no tests matched filter");
                return Ok(ExitCode::FAILURE);
            }
            let filtered_out = target.tests.len() - selected.len();
            let mut passed = 0_usize;
            let mut failed = 0_usize;
            for test in selected {
                match run_test_case(&target.artifact.executable, &test.name)? {
                    TestOutcome::Passed => {
                        println!("test {} ... ok", test.name);
                        passed += 1;
                    }
                    TestOutcome::Failed(reason) => {
                        println!("test {} ... FAILED ({reason})", test.name);
                        failed += 1;
                    }
                }
            }
            println!("{passed} passed; {failed} failed; {filtered_out} filtered out");
            if failed == 0 {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::FAILURE)
            }
        }
        Commands::Package(args) => {
            let input = args.input.unwrap_or_else(|| PathBuf::from("."));
            let manifest = require_manifest(&input, color)?;
            let resolved = resolve_project(&manifest, args.dependency.options())
                .map_err(|error| (error.to_string(), color))?;
            check_library_with_graph(&manifest, &resolved.graph)
                .map_err(|error| (error.to_string(), color))?;
            let artifact = build_library_with_graph(
                &manifest,
                BuildProfile::Debug,
                BuildSettings::default(),
                &resolved.graph,
            )
            .map_err(|error| (error.to_string(), color))?;
            println!("Packaged {}", library_output(&artifact).display());
            Ok(ExitCode::SUCCESS)
        }
        Commands::Fetch(args) => {
            let input = args.input.unwrap_or_else(|| PathBuf::from("."));
            let manifest = require_manifest(&input, color)?;
            let resolved = resolve_project(&manifest, args.dependency.options())
                .map_err(|error| (error.to_string(), color))?;
            println!(
                "Resolved {} package(s) for {}",
                resolved.graph.packages.len(),
                manifest.package.coordinate()
            );
            println!("Wrote {}", manifest.root.join("dolphin.lock").display());
            Ok(ExitCode::SUCCESS)
        }
        Commands::Publish(args) => {
            let input = args.input.unwrap_or_else(|| PathBuf::from("."));
            let manifest = require_manifest(&input, color)?;
            let resolved = resolve_project(&manifest, args.dependency.options())
                .map_err(|error| (error.to_string(), color))?;
            check_library_with_graph(&manifest, &resolved.graph)
                .map_err(|error| (error.to_string(), color))?;
            let artifact = build_library_with_graph(
                &manifest,
                BuildProfile::Debug,
                BuildSettings::default(),
                &resolved.graph,
            )
            .map_err(|error| (error.to_string(), color))?;
            let package = artifact
                .package
                .ok_or_else(|| ("publish requires a `[lib]` target".to_string(), color))?;
            let repository = args.repository.unwrap_or_else(|| "default".to_string());
            let published = publish_library(
                &manifest,
                &resolved.graph,
                &repository,
                &package,
                args.dependency.options(),
            )
            .map_err(|error| (error.to_string(), color))?;
            println!(
                "Published {} to repository `{repository}` ({published})",
                manifest.package.coordinate()
            );
            Ok(ExitCode::SUCCESS)
        }
        Commands::Info { input } => {
            let input = input.unwrap_or_else(|| PathBuf::from("."));
            match discover_manifest(&input).map_err(|error| (error.to_string(), color))? {
                Some(manifest) => {
                    print_info(&manifest);
                    let lock_path = manifest.root.join("dolphin.lock");
                    if lock_path.is_file() {
                        println!("lock: {} (present)", lock_path.display());
                    } else {
                        println!("lock: absent (run `dc fetch` to create it)");
                    }
                }
                None => {
                    return Err((
                        format!("no `dolphin.toml` found from `{}` upward", input.display()),
                        color,
                    ));
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Commands::Env => {
            let platform = host_platform().map_err(|error| (error.to_string(), color))?;
            println!("host: {}", platform.triple());
            println!("target: {}", platform.triple());
            println!("abi: {}", platform.abi().name());
            println!("linker: {}", platform.linker_name());
            println!(
                "system linker: {} (--system-linker fallback)",
                platform.system_linker_name()
            );
            println!(
                "backend: {} (default; --backend overrides)",
                BackendChoice::default().name()
            );
            println!("cache: {}", Cache::from_env().home().display());
            Ok(ExitCode::SUCCESS)
        }
        Commands::Fmt(args) => run_fmt(args, color),
        Commands::Lsp { project } => {
            let code = dolphin_lsp::serve(project)
                .map_err(|error| (format!("language server error: {error}"), color))?;
            Ok(code)
        }
    }
}

/// `dc fmt`：按 D-M20-4 冻结规则选择文件并格式化。
///
/// 无路径参数时从 cwd 向上发现 `dolphin.toml`：找到则根为 `[package].source`，
/// 否则为 `src`。递归排除项目 `build.output`（默认 `target`）与 `.git`，不跟随目录
/// 符号链接；显式文件精确生效。先全部读入内存格式化，任一失败则本次不写任何文件。
fn run_fmt(args: FmtArgs, color: ColorMode) -> Result<ExitCode, (String, ColorMode)> {
    let FmtArgs { paths, check } = args;
    let cwd = std::env::current_dir().map_err(|error| {
        (
            format!("could not determine current directory: {error}"),
            color,
        )
    })?;
    let manifest = discover_manifest(&cwd).map_err(|error| (error.to_string(), color))?;

    let roots = if paths.is_empty() {
        match &manifest {
            Some(manifest) => vec![manifest.package.source.clone()],
            None => vec![PathBuf::from("src")],
        }
    } else {
        paths
    };

    let mut excluded = Vec::new();
    if let Some(manifest) = &manifest {
        excluded.push(lexical_normalize(&manifest.build.output));
    }

    let mut files = Vec::new();
    for root in &roots {
        if root.is_file() {
            // 显式文件精确生效，即使位于构建输出目录或扩展名不是 `.do`。
            files.push(root.clone());
        } else {
            collect_sources(root, &mut excluded, &mut files).map_err(|message| (message, color))?;
        }
    }
    files.sort();
    files.dedup();

    // 全有或全无：先读入并格式化全部文件，任一失败时不写任何文件（§8.2）。
    let mut planned = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).map_err(|error| {
            (
                format!("could not read `{}`: {error}", file.display()),
                color,
            )
        })?;
        let formatted =
            dolphin_format::format_source(&text).map_err(|error| (error.to_string(), color))?;
        if formatted != text {
            planned.push((file.clone(), formatted));
        }
    }

    if check {
        for (file, _) in &planned {
            println!("would reformat {}", file.display());
        }
        if !planned.is_empty() {
            return Err(("some files are not formatted".to_string(), color));
        }
        return Ok(ExitCode::SUCCESS);
    }

    for (file, formatted) in &planned {
        fs::write(file, formatted).map_err(|error| {
            (
                format!("could not write `{}`: {error}", file.display()),
                color,
            )
        })?;
        println!("formatted {}", file.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// 递归收集目录下的 `.do` 源文件；跳过 `.git` 与被排除的构建输出目录，
/// 不跟随目录符号链接（防环）；目录内自带清单时其构建输出同样排除。
fn collect_sources(
    path: &Path,
    excluded: &mut Vec<PathBuf>,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if path.is_dir() {
        if path.file_name() == Some(OsStr::new(".git")) {
            return Ok(());
        }
        let normalized = lexical_normalize(path);
        if excluded.contains(&normalized) {
            return Ok(());
        }
        if path.join("dolphin.toml").is_file()
            && let Ok(Some(manifest)) = discover_manifest(path)
        {
            let output = lexical_normalize(&manifest.build.output);
            if !excluded.contains(&output) {
                excluded.push(output);
            }
        }
        let entries = fs::read_dir(path)
            .map_err(|error| format!("could not read directory `{}`: {error}", path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("could not read source entry: {error}"))?;
            let child = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| format!("could not read `{}`: {error}", child.display()))?;
            if file_type.is_symlink() && child.is_dir() {
                continue;
            }
            collect_sources(&child, excluded, out)?;
        }
        Ok(())
    } else if path.is_file() {
        if path.extension().is_some_and(|ext| ext == "do") {
            out.push(path.to_path_buf());
        }
        Ok(())
    } else {
        Err(format!("`{}` does not exist", path.display()))
    }
}

/// 纯词法规范化：折叠 `.`/`..`，不解析符号链接，也不要求路径存在。
fn lexical_normalize(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// 读取并解析项目清单；未找到时报错。
fn require_manifest(
    input: &std::path::Path,
    color: ColorMode,
) -> Result<Manifest, (String, ColorMode)> {
    discover_manifest(input)
        .map_err(|error| (error.to_string(), color))?
        .ok_or_else(|| {
            (
                format!("no `dolphin.toml` found from `{}` upward", input.display()),
                color,
            )
        })
}

/// 打印 `dc info` 的项目摘要。
fn print_info(manifest: &Manifest) {
    println!("{}", manifest.package.coordinate());
    println!("source: {}", manifest.package.source.display());
    if let Some(lib) = &manifest.lib {
        println!("lib: {} -> {}", manifest.package.name, lib.path.display());
    }
    println!("bins:");
    for bin in &manifest.bins {
        println!("  {} -> {}", bin.name, bin.path.display());
    }
    if !manifest.dependencies.is_empty() {
        println!("dependencies:");
        for dependency in &manifest.dependencies {
            match &dependency.kind {
                DependencyKind::Coordinate {
                    coordinate,
                    repository,
                } => println!(
                    "  {} = {coordinate} (repository `{repository}`)",
                    dependency.alias
                ),
                DependencyKind::Path(path) => {
                    println!("  {} = path {}", dependency.alias, path.display());
                }
            }
        }
    }
    if !manifest.repositories.is_empty() {
        println!("repositories:");
        for (id, base) in &manifest.repositories {
            println!("  {id} = {base}");
        }
    }
    println!("optimization: {}", manifest.build.optimization);
}

/// 运行一个已构建的可执行文件并映射退出码；`--` 之后的参数原样转发。
fn run_executable(
    artifact: &BuildArtifact,
    app_args: &[std::ffi::OsString],
) -> Result<ExitCode, (String, ColorMode)> {
    let status = Command::new(&artifact.executable)
        .args(app_args)
        .status()
        .map_err(|error| {
            (
                format!("could not run `{}`: {error}", artifact.executable.display()),
                ColorMode::Auto,
            )
        })?;
    // 正常退出码为 0..=255；Windows 异常终止会返回 0xC0000xxx 之类的
    // 负值编码，统一映射为失败码 1，而不是被截断成 0。
    let code = status
        .code()
        .filter(|code| (0..=255).contains(code))
        .unwrap_or(1);
    Ok(ExitCode::from(code as u8))
}

/// 库构建对外展示的产物路径：优先 `.dlib`，否则验证目标文件。
fn library_output(artifact: &LibraryArtifact) -> &PathBuf {
    artifact.package.as_ref().unwrap_or(&artifact.object)
}

/// `run` 需要唯一可执行目标：多目标时必须显式 `--bin`。
fn select_run_target(
    artifacts: &[BuildArtifact],
    color: ColorMode,
) -> Result<&BuildArtifact, (String, ColorMode)> {
    if artifacts.len() == 1 {
        Ok(&artifacts[0])
    } else {
        Err((
            "multiple binary targets were built; use `--bin <name>` to select which one to run"
                .to_string(),
            color,
        ))
    }
}

/// `dc test` 的单测试固定超时（规格 §9.1：30 秒，超时 kill 并继续）。
const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// 单个测试子进程的结果分类。
enum TestOutcome {
    Passed,
    Failed(String),
}

/// 在独立子进程中运行一个测试；stdout/stderr 直接继承，超时 kill 并回收。
///
/// 退出码分类：0 通过；106 断言失败；其余（含信号终止统一按 1）为 `trap exit N`。
fn run_test_case(executable: &Path, name: &str) -> Result<TestOutcome, (String, ColorMode)> {
    let mut child = Command::new(executable)
        .arg("--dolphin-test")
        .arg(name)
        .spawn()
        .map_err(|error| {
            (
                format!("could not run `{}`: {error}", executable.display()),
                ColorMode::Auto,
            )
        })?;
    let deadline = Instant::now() + TEST_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Windows 异常终止会得到 0xC0000xxx 之类的负值编码，统一按 1 归类，
                // 保持在冻结的输出格式内。
                let code = status
                    .code()
                    .filter(|code| (0..=255).contains(code))
                    .unwrap_or(1);
                if code == 0 {
                    return Ok(TestOutcome::Passed);
                }
                if code == 106 {
                    return Ok(TestOutcome::Failed("assertion".to_string()));
                }
                return Ok(TestOutcome::Failed(format!("trap exit {code}")));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    child.kill().ok();
                    child.wait().ok();
                    return Ok(TestOutcome::Failed("timeout after 30s".to_string()));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                return Err((
                    format!("could not wait for `{}`: {error}", executable.display()),
                    ColorMode::Auto,
                ));
            }
        }
    }
}

/// 按收集顺序把全部诊断打印到 stderr（M20/H20-01，不添加分隔行）。
fn print_diagnostics(diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        eprintln!("{diagnostic}");
    }
}

fn render_error(message: String, color: ColorMode) -> String {
    let enabled = matches!(color, ColorMode::Always)
        || matches!(color, ColorMode::Auto) && io::stderr().is_terminal();
    if enabled {
        format!("\x1b[31m{message}\x1b[0m")
    } else {
        message
    }
}
