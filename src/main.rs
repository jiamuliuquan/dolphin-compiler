use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use dolphin_compiler::{
    BuildArtifact, BuildOptions, BuildProfile, BuildSettings, LinkerChoice, build_manifest,
    build_with_profile, check, check_manifest, discover_manifest, host_platform,
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
    Check {
        /// Dolphin project directory or a standalone .dc file (defaults to the current directory)
        input: Option<PathBuf>,
    },

    /// Compile to a native executable
    Build(BuildArgs),

    /// Compile and execute the program
    Run(BuildArgs),

    /// Show the package coordinate and build configuration of a project
    Info {
        /// Dolphin project directory (defaults to the current directory)
        input: Option<PathBuf>,
    },

    /// Show the host platform, target triple, and selected linker
    Env,
}

#[derive(Args)]
struct BuildArgs {
    /// Dolphin project directory or a standalone .dc file (defaults to the current directory)
    input: Option<PathBuf>,

    /// Select which binary target to build or run (for dolphin.toml projects)
    #[arg(long)]
    bin: Option<String>,

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
}

impl BuildArgs {
    fn profile(&self) -> BuildProfile {
        if self.release {
            BuildProfile::Release
        } else {
            BuildProfile::Debug
        }
    }

    fn settings(&self) -> BuildSettings {
        if self.system_linker {
            BuildSettings::with_linker(LinkerChoice::System)
        } else {
            BuildSettings::default()
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum ColorMode {
    Auto,
    Always,
    Never,
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
        Commands::Check { input } => {
            let input = input.unwrap_or_else(|| PathBuf::from("."));
            match discover_manifest(&input).map_err(|error| (error.to_string(), color))? {
                Some(manifest) => {
                    check_manifest(&manifest).map_err(|error| (error.to_string(), color))?;
                    println!("Checked {}", manifest.package.coordinate());
                }
                None => {
                    check(&input).map_err(|error| (error.to_string(), color))?;
                    println!("Checked {}", input.display());
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Commands::Build(args) => {
            let profile = args.profile();
            let artifacts = build_for_command(&args, profile, color)?;
            for artifact in artifacts {
                println!("Built {}", artifact.executable.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Commands::Run(args) => {
            let profile = args.profile();
            let artifacts = build_for_command(&args, profile, color)?;
            let artifact = select_run_target(&artifacts, color)?;
            let status = Command::new(&artifact.executable)
                .status()
                .map_err(|error| {
                    (
                        format!("could not run `{}`: {error}", artifact.executable.display()),
                        color,
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
        Commands::Info { input } => {
            let input = input.unwrap_or_else(|| PathBuf::from("."));
            match discover_manifest(&input).map_err(|error| (error.to_string(), color))? {
                Some(manifest) => {
                    println!("{}", manifest.package.coordinate());
                    println!("source: {}", manifest.package.source.display());
                    println!("bins:");
                    for bin in &manifest.bins {
                        println!("  {} -> {}", bin.name, bin.path.display());
                    }
                    println!("optimization: {}", manifest.build.optimization);
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
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// 根据是否发现清单，走清单构建或旧版单文件/目录构建。
fn build_for_command(
    args: &BuildArgs,
    profile: BuildProfile,
    color: ColorMode,
) -> Result<Vec<BuildArtifact>, (String, ColorMode)> {
    let settings = args.settings();
    let input = args.input.clone().unwrap_or_else(|| PathBuf::from("."));
    match discover_manifest(&input).map_err(|error| (error.to_string(), color))? {
        Some(manifest) => build_manifest(&manifest, args.bin.as_deref(), profile, settings)
            .map_err(|error| (error.to_string(), color)),
        None => {
            if args.bin.is_some() {
                return Err((
                    "`--bin` requires a project with a `dolphin.toml` manifest".to_string(),
                    color,
                ));
            }
            build_with_profile(
                BuildOptions {
                    input,
                    output: args.output.clone(),
                },
                profile,
                settings,
            )
            .map(|artifact| vec![artifact])
            .map_err(|error| (error.to_string(), color))
        }
    }
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

fn render_error(message: String, color: ColorMode) -> String {
    let enabled = matches!(color, ColorMode::Always)
        || matches!(color, ColorMode::Auto) && io::stderr().is_terminal();
    if enabled {
        format!("\x1b[31m{message}\x1b[0m")
    } else {
        message
    }
}
