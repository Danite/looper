use clap::{Parser, Subcommand};
use colored::Colorize;
use looper::detectors::{default_registry, DetectorContext};
use looper::parser::parse_file;
use looper::resolver::resolve_bindings;
use looper::types::ShadowDep;
use looper::walker::{discover_files, WalkError, WalkOptions};
use oxc_allocator::Allocator;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::sync::Mutex;
use std::time::Instant;

const SUBCOMMANDS: &[&str] = &["scan", "check", "init", "diff", "drift", "help"];

#[derive(Parser)]
#[command(
    name = "looper",
    version,
    about = "Static analysis tool that discovers implicit shadow dependencies in JS/TS codebases"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Scan for shadow dependencies (default when no subcommand given)
    Scan(ScanArgs),

    /// Compare scan results against a declared manifest
    Check(CheckArgs),

    /// Generate config file and initial manifest
    Init,

    /// Compare two manifest files
    Diff(DiffArgs),

    /// Compare code needs vs infrastructure provides
    Drift(DriftArgs),
}

#[derive(Parser)]
struct ScanArgs {
    /// Paths to scan (defaults to current directory)
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,

    /// Output format: terminal, json, sarif, html
    #[arg(short, long, default_value = "terminal")]
    format: String,

    /// Write output to file instead of stdout
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Only scan specific categories (comma-separated: env,fs,network,shell,ports,platform)
    #[arg(long, value_delimiter = ',')]
    only: Option<Vec<String>>,

    /// Exclude specific categories (comma-separated)
    #[arg(long, value_delimiter = ',')]
    exclude: Option<Vec<String>>,

    /// Minimum confidence level: low, medium, high
    #[arg(long, default_value = "low")]
    min_confidence: String,

    /// Only show findings not in the baseline manifest
    #[arg(long)]
    baseline: Option<PathBuf>,

    /// Additional glob patterns to ignore
    #[arg(long)]
    ignore: Option<Vec<String>>,

    /// Don't respect .gitignore
    #[arg(long)]
    no_gitignore: bool,

    /// Only output summary line
    #[arg(short, long)]
    quiet: bool,

    /// Show per-file scanning progress
    #[arg(short, long)]
    verbose: bool,

    /// Number of parallel threads (default: auto)
    #[arg(short = 'j', long)]
    threads: Option<usize>,

    /// Path to config file (default: auto-detect)
    #[arg(long)]
    config: Option<PathBuf>,
}

impl Default for ScanArgs {
    fn default() -> Self {
        Self {
            paths: vec![PathBuf::from(".")],
            format: "terminal".into(),
            output: None,
            only: None,
            exclude: None,
            min_confidence: "low".into(),
            baseline: None,
            ignore: None,
            no_gitignore: false,
            quiet: false,
            verbose: false,
            threads: None,
            config: None,
        }
    }
}

#[derive(Parser)]
struct CheckArgs {
    /// Path to declared manifest
    #[arg(short, long, default_value = "shadow-deps.json")]
    manifest: PathBuf,

    /// Exit 1 if new undeclared deps found (default)
    #[arg(long, default_value_t = true)]
    fail_on_new: bool,

    /// Also exit 1 if declared deps are no longer found
    #[arg(long)]
    fail_on_removed: bool,

    /// Update the manifest file with current scan results
    #[arg(long)]
    update: bool,

    /// Show per-file scanning progress
    #[arg(short, long)]
    verbose: bool,

    /// Path to config file (default: auto-detect)
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct DiffArgs {
    /// Path to old manifest
    old: PathBuf,

    /// Path to new manifest
    new: PathBuf,

    /// Output format: terminal, json
    #[arg(short, long, default_value = "terminal")]
    format: String,
}

#[derive(Parser)]
struct DriftArgs {
    /// Path to docker-compose.yml (default: auto-detect)
    #[arg(long)]
    compose: Option<PathBuf>,

    /// Path to .env file (default: auto-detect)
    #[arg(long)]
    env: Option<PathBuf>,

    /// Path to Dockerfile (default: auto-detect)
    #[arg(long)]
    dockerfile: Option<PathBuf>,

    /// Exit 1 if code needs something infra doesn't provide
    #[arg(long)]
    fail_on_missing: bool,

    /// Output format: terminal, json
    #[arg(short, long, default_value = "terminal")]
    format: String,

    /// Write output to file
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Override scan paths (default: from config or '.')
    #[arg(long, value_delimiter = ',')]
    scan_paths: Option<Vec<PathBuf>>,

    /// Show per-file scanning progress
    #[arg(short, long)]
    verbose: bool,

    /// Path to config file
    #[arg(long)]
    config: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse_from(implicit_scan_subcommand());

    let exit_code = match cli.command {
        Some(Command::Scan(args)) => cmd_scan(args),
        Some(Command::Check(args)) => cmd_check(args),
        Some(Command::Init) => cmd_init(),
        Some(Command::Diff(args)) => cmd_diff(args),
        Some(Command::Drift(args)) => cmd_drift(args),
        None => cmd_scan(ScanArgs::default()),
    };

    process::exit(exit_code);
}

/// When the first positional arg isn't a known subcommand, insert "scan"
/// so that `looper .` and `looper src/ lib/` work as shorthand.
fn implicit_scan_subcommand() -> Vec<String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        return args;
    }

    let first_positional = args[1..].iter().find(|a| !a.starts_with('-'));

    if let Some(arg) = first_positional {
        if !SUBCOMMANDS.contains(&arg.as_str()) {
            let mut new_args = vec![args[0].clone(), "scan".into()];
            new_args.extend_from_slice(&args[1..]);
            return new_args;
        }
    }

    args
}

fn cmd_scan(args: ScanArgs) -> i32 {
    if let Some(n) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .ok();
    }

    let config = looper::config::load_config(args.config.as_deref());

    let mut extra_ignores = args.ignore.unwrap_or_default();
    if let Some(ref cfg) = config {
        extra_ignores.extend(cfg.exclude.iter().cloned());
    }

    let opts = WalkOptions {
        no_gitignore: args.no_gitignore,
        extra_ignore_patterns: extra_ignores,
        follow_symlinks: true,
    };

    let start = Instant::now();

    let walk_result = match discover_files(&args.paths, &opts) {
        Ok(r) => r,
        Err(WalkError::NoFilesFound) => {
            eprintln!("{}", "error: no JavaScript/TypeScript files found".red());
            return 2;
        }
        Err(WalkError::Config(msg)) => {
            eprintln!("{} {msg}", "error:".red());
            return 2;
        }
    };

    let file_count = walk_result.files.len();
    let registry = default_registry();
    let all_deps: Mutex<Vec<ShadowDep>> = Mutex::new(Vec::new());
    let parse_errors: Mutex<Vec<String>> = Mutex::new(Vec::new());

    walk_result.files.par_iter().for_each(|path| {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                if args.verbose {
                    eprintln!("  {} read error: {e}", path.display());
                }
                return;
            }
        };

        if source.is_empty() {
            return;
        }

        let allocator = Allocator::new();
        let source_leaked: &str = allocator.alloc_str(&source);

        let result = parse_file(&allocator, path, source_leaked);

        if args.verbose {
            eprintln!("  {} {}", "scan".dimmed(), path.display());
        }

        if result.panicked {
            parse_errors
                .lock()
                .unwrap()
                .push(format!("{}: parser panicked", path.display()));
            return;
        }

        if result.has_errors() {
            for err in &result.errors {
                parse_errors
                    .lock()
                    .unwrap()
                    .push(format!("{}: {err}", path.display()));
            }
        }

        let bindings = resolve_bindings(&result.program);

        let ctx = DetectorContext {
            program: &result.program,
            source_text: result.source_text,
            file_path: &result.file_path,
            bindings: &bindings,
        };

        let deps = registry.run_all(&ctx);
        if !deps.is_empty() {
            all_deps.lock().unwrap().extend(deps);
        }
    });

    let duration = start.elapsed();
    let duration_ms = duration.as_millis() as u64;

    let raw_deps = all_deps.into_inner().unwrap();
    let errors = parse_errors.into_inner().unwrap();

    let mut merged = merge_deps(raw_deps);

    // Apply --only category filter
    if let Some(ref only) = args.only {
        let allowed: Vec<looper::types::Category> = only
            .iter()
            .filter_map(|s| looper::types::Category::from_filter_name(s))
            .collect();
        if !allowed.is_empty() {
            merged.retain(|d| allowed.contains(&d.category));
        }
    }

    // Apply --exclude category filter
    if let Some(ref exclude) = args.exclude {
        let excluded: Vec<looper::types::Category> = exclude
            .iter()
            .filter_map(|s| looper::types::Category::from_filter_name(s))
            .collect();
        merged.retain(|d| !excluded.contains(&d.category));
    }

    // Apply --min-confidence filter
    if let Some(min) = looper::types::Confidence::parse(&args.min_confidence) {
        merged.retain(|d| d.confidence >= min);
    }

    // Apply --baseline filter
    if let Some(ref baseline_path) = args.baseline {
        if let Ok(content) = std::fs::read_to_string(baseline_path) {
            if let Ok(baseline) = serde_json::from_str::<looper::types::ScanManifest>(&content) {
                let baseline_ids: std::collections::HashSet<String> =
                    baseline.deps.iter().map(|d| d.id.clone()).collect();
                merged.retain(|d| !baseline_ids.contains(&d.id));
            }
        }
    }

    let has_errors = !errors.is_empty();

    if args.verbose && has_errors {
        eprintln!();
        for err in &errors {
            eprintln!("  {} {err}", "warning:".yellow());
        }
    }

    let root = args
        .paths
        .first()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".into());

    if let Err(e) = looper::output::format_and_write(
        &args.format,
        args.quiet,
        args.output.as_deref(),
        &root,
        file_count,
        duration_ms,
        merged,
    ) {
        eprintln!("{} {e}", "error:".red());
        return 1;
    }

    if has_errors { 1 } else { 0 }
}

/// Deduplicate deps by id, merging locations.
fn merge_deps(deps: Vec<ShadowDep>) -> Vec<ShadowDep> {
    let mut map: HashMap<String, ShadowDep> = HashMap::new();
    for dep in deps {
        map.entry(dep.id.clone())
            .and_modify(|existing| existing.merge_locations(&dep))
            .or_insert(dep);
    }
    let mut result: Vec<ShadowDep> = map.into_values().collect();
    result.sort_by(|a, b| a.category.label().cmp(b.category.label()).then(a.name.cmp(&b.name)));
    result
}

struct ScanResult {
    deps: Vec<ShadowDep>,
    file_count: usize,
    duration_ms: u64,
    root: String,
}

fn run_scan(
    paths: &[PathBuf],
    no_gitignore: bool,
    extra_ignores: Vec<String>,
    verbose: bool,
    config_path: Option<&std::path::Path>,
) -> Result<ScanResult, i32> {
    let config = looper::config::load_config(config_path);

    let mut ignores = extra_ignores;
    if let Some(ref cfg) = config {
        ignores.extend(cfg.exclude.iter().cloned());
    }

    let opts = WalkOptions {
        no_gitignore,
        extra_ignore_patterns: ignores,
        follow_symlinks: true,
    };

    let start = Instant::now();

    let walk_result = match discover_files(paths, &opts) {
        Ok(r) => r,
        Err(WalkError::NoFilesFound) => {
            eprintln!("{}", "error: no JavaScript/TypeScript files found".red());
            return Err(2);
        }
        Err(WalkError::Config(msg)) => {
            eprintln!("{} {msg}", "error:".red());
            return Err(2);
        }
    };

    let file_count = walk_result.files.len();
    let registry = default_registry();
    let all_deps: Mutex<Vec<ShadowDep>> = Mutex::new(Vec::new());

    walk_result.files.par_iter().for_each(|path| {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                if verbose {
                    eprintln!("  {} read error: {e}", path.display());
                }
                return;
            }
        };

        if source.is_empty() {
            return;
        }

        let allocator = Allocator::new();
        let source_leaked: &str = allocator.alloc_str(&source);
        let result = parse_file(&allocator, path, source_leaked);

        if verbose {
            eprintln!("  {} {}", "scan".dimmed(), path.display());
        }

        if result.panicked {
            return;
        }

        let bindings = resolve_bindings(&result.program);
        let ctx = DetectorContext {
            program: &result.program,
            source_text: result.source_text,
            file_path: &result.file_path,
            bindings: &bindings,
        };

        let deps = registry.run_all(&ctx);
        if !deps.is_empty() {
            all_deps.lock().unwrap().extend(deps);
        }
    });

    let duration_ms = start.elapsed().as_millis() as u64;
    let raw_deps = all_deps.into_inner().unwrap();
    let deps = merge_deps(raw_deps);

    let root = paths
        .first()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".into());

    Ok(ScanResult {
        deps,
        file_count,
        duration_ms,
        root,
    })
}

fn cmd_check(args: CheckArgs) -> i32 {
    let manifest = match looper::check::load_manifest(&args.manifest) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {e}", "error:".red());
            return 2;
        }
    };

    let scan = match run_scan(
        &[PathBuf::from(".")],
        false,
        Vec::new(),
        args.verbose,
        args.config.as_deref(),
    ) {
        Ok(s) => s,
        Err(code) => return code,
    };

    if args.update {
        let new_manifest =
            looper::output::build_manifest(&scan.root, scan.file_count, scan.duration_ms, scan.deps);
        match looper::output::json::format_json(&new_manifest) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&args.manifest, &json) {
                    eprintln!("{} failed to write manifest: {e}", "error:".red());
                    return 1;
                }
                eprintln!(
                    "  {} {} ({} deps)",
                    "updated".green(),
                    args.manifest.display(),
                    new_manifest.deps.len()
                );
            }
            Err(e) => {
                eprintln!("{} JSON serialization failed: {e}", "error:".red());
                return 1;
            }
        }
        return 0;
    }

    let result = looper::check::check_against_manifest(&scan.deps, &manifest);
    looper::check::print_check_report(&result);

    if !result.added.is_empty() && args.fail_on_new {
        return 1;
    }
    if !result.removed.is_empty() && args.fail_on_removed {
        return 1;
    }

    0
}

fn cmd_init() -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{} could not determine current directory: {e}", "error:".red());
            return 1;
        }
    };

    let config_path = cwd.join("looper.config.json");
    if config_path.exists() {
        eprintln!(
            "{} looper.config.json already exists",
            "warning:".yellow()
        );
    } else {
        match looper::config::write_default_config(&cwd) {
            Ok(path) => {
                eprintln!("  {} {}", "created".green(), path.display());
            }
            Err(e) => {
                eprintln!("{} failed to write config: {e}", "error:".red());
                return 1;
            }
        }
    }

    0
}

fn cmd_drift(args: DriftArgs) -> i32 {
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    if !looper::infra::has_infra_files(&project_root)
        && args.compose.is_none()
        && args.env.is_none()
        && args.dockerfile.is_none()
    {
        eprintln!(
            "{} no infrastructure files found (docker-compose.yml, .env, Dockerfile)",
            "error:".red()
        );
        return 2;
    }

    let scan_paths = args
        .scan_paths
        .unwrap_or_else(|| vec![PathBuf::from(".")]);

    let scan = match run_scan(&scan_paths, false, Vec::new(), args.verbose, args.config.as_deref()) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let mut infra = looper::infra::build_infra_manifest(&project_root);

    // Override with explicit paths if provided
    if let Some(ref compose_path) = args.compose {
        if let Ok(result) = looper::infra::compose::parse_compose_file(compose_path) {
            for svc in &result.services {
                for pm in &svc.ports {
                    infra.exposed_ports.push(looper::infra::InfraPort {
                        port: pm.host,
                        source: format!("{} service {}", compose_path.display(), svc.name),
                    });
                }
                for env_name in &svc.environment {
                    infra.env_vars.entry(env_name.clone()).or_insert_with(|| {
                        format!("{} service {}", compose_path.display(), svc.name)
                    });
                }
            }
        }
    }
    if let Some(ref env_path) = args.env {
        if let Ok(result) = looper::infra::envfile::parse_env_file(env_path) {
            for (name, entry) in &result.entries {
                infra.env_vars.entry(name.clone()).or_insert_with(|| {
                    format!("{} line {}", entry.source_file, entry.line)
                });
            }
        }
    }
    if let Some(ref df_path) = args.dockerfile {
        if let Ok(result) = looper::infra::dockerfile::parse_dockerfile(df_path) {
            infra.binaries.extend(result.binaries);
            for port in result.exposed_ports {
                infra.exposed_ports.push(looper::infra::InfraPort {
                    port,
                    source: format!("{} EXPOSE", df_path.display()),
                });
            }
        }
    }
    let report = looper::drift::compare(&scan.deps, &infra);

    match args.format.as_str() {
        "json" => {
            let json = match serde_json::to_string_pretty(&report) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("{} JSON serialization failed: {e}", "error:".red());
                    return 1;
                }
            };
            if let Err(e) = looper::output::write_output(&json, args.output.as_deref()) {
                eprintln!("{} {e}", "error:".red());
                return 1;
            }
        }
        _ => {
            let output = format_drift_terminal(&report);
            print!("{output}");
        }
    }

    if args.fail_on_missing && report.has_missing() {
        return 1;
    }

    0
}

fn format_drift_terminal(report: &looper::drift::DriftReport) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    writeln!(out, "{}", "looper drift — comparing code vs infrastructure".bold()).unwrap();

    if !report.missing.is_empty() {
        writeln!(
            out,
            "\n {} {} ({}):",
            "MISSING FROM INFRASTRUCTURE".red().bold(),
            "(code needs, infra doesn't provide)".dimmed(),
            report.missing.len()
        )
        .unwrap();
        writeln!(out, " {}", "──────────────────────────────────────────────────".dimmed()).unwrap();
        for item in &report.missing {
            let src = item.code_source.as_deref().unwrap_or("");
            writeln!(out, "  {:<10} {:<25} {}", item.category, item.name, src.dimmed()).unwrap();
        }
    }

    if !report.external.is_empty() {
        writeln!(
            out,
            "\n {} ({}):",
            "EXTERNAL".dimmed().bold(),
            report.external.len()
        )
        .unwrap();
        writeln!(out, " {}", "──────────────────────────────────────────────────".dimmed()).unwrap();
        for item in &report.external {
            writeln!(out, "  {:<10} {:<25} {}", item.category, item.name, "external API".dimmed()).unwrap();
        }
    }

    if !report.unused.is_empty() {
        writeln!(
            out,
            "\n {} {} ({}):",
            "UNUSED IN INFRASTRUCTURE".yellow().bold(),
            "(infra provides, code doesn't use)".dimmed(),
            report.unused.len()
        )
        .unwrap();
        writeln!(out, " {}", "──────────────────────────────────────────────────".dimmed()).unwrap();
        for item in &report.unused {
            let src = item.infra_source.as_deref().unwrap_or("");
            writeln!(out, "  {:<10} {:<25} {}", item.category, item.name, src.dimmed()).unwrap();
        }
    }

    if !report.covered.is_empty() {
        writeln!(
            out,
            "\n {} ({}):",
            "COVERED".green().bold(),
            report.covered.len()
        )
        .unwrap();
        writeln!(out, " {}", "──────────────────────────────────────────────────".dimmed()).unwrap();
        for item in &report.covered {
            let src = item.infra_source.as_deref().unwrap_or("");
            writeln!(out, "  {:<10} {:<25} {}", item.category, item.name, src.dimmed()).unwrap();
        }
    }

    writeln!(
        out,
        "\n {} {} missing, {} external, {} unused, {} covered",
        "Summary:".bold(),
        report.missing.len(),
        report.external.len(),
        report.unused.len(),
        report.covered.len(),
    )
    .unwrap();

    out
}

fn cmd_diff(args: DiffArgs) -> i32 {
    let old_manifest = match looper::check::load_manifest(&args.old) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {e}", "error:".red());
            return 1;
        }
    };

    let new_manifest = match looper::check::load_manifest(&args.new) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {e}", "error:".red());
            return 1;
        }
    };

    let diff = looper::diff::diff_manifests(&old_manifest, &new_manifest);

    match args.format.as_str() {
        "json" => match looper::diff::format_diff_json(&diff) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("{} JSON serialization failed: {e}", "error:".red());
                return 1;
            }
        },
        _ => {
            print!("{}", looper::diff::format_diff_terminal(&diff));
        }
    }

    0
}
