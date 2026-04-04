use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

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

fn cmd_scan(_args: ScanArgs) -> i32 {
    eprintln!("looper: scan not yet implemented");
    0
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
