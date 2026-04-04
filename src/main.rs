use clap::{Parser, Subcommand};
use colored::Colorize;
use looper::detectors::{default_registry, DetectorContext};
use looper::parser::parse_file;
use looper::resolver::resolve_bindings;
use looper::types::{Category, ShadowDep};
use looper::walker::{discover_files, WalkError, WalkOptions};
use oxc_allocator::Allocator;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::sync::Mutex;
use std::time::Instant;

const SUBCOMMANDS: &[&str] = &["scan", "check", "init", "diff", "help"];

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

fn main() {
    let cli = Cli::parse_from(implicit_scan_subcommand());

    let exit_code = match cli.command {
        Some(Command::Scan(args)) => cmd_scan(args),
        Some(Command::Check(args)) => cmd_check(args),
        Some(Command::Init) => cmd_init(),
        Some(Command::Diff(args)) => cmd_diff(args),
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

    let opts = WalkOptions {
        no_gitignore: args.no_gitignore,
        extra_ignore_patterns: args.ignore.unwrap_or_default(),
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

    let merged = merge_deps(raw_deps);

    if args.verbose && !errors.is_empty() {
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

    match args.format.as_str() {
        "json" => {
            let manifest =
                looper::output::build_manifest(&root, file_count, duration_ms, merged);
            match looper::output::json::format_json(&manifest) {
                Ok(json_str) => {
                    if let Err(e) =
                        looper::output::write_output(&json_str, args.output.as_deref())
                    {
                        eprintln!("{} {e}", "error:".red());
                        return 1;
                    }
                }
                Err(e) => {
                    eprintln!("{} JSON serialization failed: {e}", "error:".red());
                    return 1;
                }
            }
        }
        _ => {
            if args.quiet {
                print_summary_only(&merged, file_count, duration_ms);
            } else {
                print_terminal(&merged, file_count, duration_ms);
            }
        }
    }

    0
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

fn print_terminal(deps: &[ShadowDep], file_count: usize, duration_ms: u64) {
    println!(
        "{} {} — scanned {} files in {}ms",
        "looper".bold(),
        format!("v{}", env!("CARGO_PKG_VERSION")).dimmed(),
        file_count,
        duration_ms,
    );

    if deps.is_empty() {
        println!("\n  {}", "No shadow dependencies found.".dimmed());
        return;
    }

    let mut by_category: HashMap<Category, Vec<&ShadowDep>> = HashMap::new();
    for dep in deps {
        by_category.entry(dep.category).or_default().push(dep);
    }

    let mut total = 0;
    let mut categories_with_findings = 0;

    for category in Category::all() {
        let Some(cat_deps) = by_category.get(category) else {
            continue;
        };
        categories_with_findings += 1;
        total += cat_deps.len();

        println!(
            "\n {} ({} found)",
            category.label().bold(),
            cat_deps.len()
        );
        println!(" {}", "──────────────────────────────────────────────────".dimmed());

        for dep in cat_deps {
            let loc = dep
                .locations
                .first()
                .map(|l| format!("{}:{}", l.file, l.line))
                .unwrap_or_default();
            let conf = match dep.confidence {
                looper::types::Confidence::High => "high".green(),
                looper::types::Confidence::Medium => "medium".yellow(),
                looper::types::Confidence::Low => "low".red(),
            };
            println!("  {:<20} {:<25} {}", dep.name, loc.dimmed(), conf);
        }
    }

    println!(
        "\n {} {} shadow deps across {} {}",
        "Summary:".bold(),
        total,
        categories_with_findings,
        if categories_with_findings == 1 {
            "category"
        } else {
            "categories"
        },
    );
}

fn print_summary_only(deps: &[ShadowDep], file_count: usize, duration_ms: u64) {
    let mut by_category: HashMap<Category, usize> = HashMap::new();
    for dep in deps {
        *by_category.entry(dep.category).or_default() += 1;
    }

    println!(
        "looper v{} — {} files, {}ms — {} shadow deps across {} categories",
        env!("CARGO_PKG_VERSION"),
        file_count,
        duration_ms,
        deps.len(),
        by_category.len(),
    );
}

fn cmd_check(_args: CheckArgs) -> i32 {
    eprintln!("looper: check not yet implemented");
    0
}

fn cmd_init() -> i32 {
    eprintln!("looper: init not yet implemented");
    0
}

fn cmd_diff(_args: DiffArgs) -> i32 {
    eprintln!("looper: diff not yet implemented");
    0
}
