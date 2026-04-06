use clap::{Parser, Subcommand};
use std::path::PathBuf;

pub(crate) const SUBCOMMANDS: &[&str] = &[
    "scan", "check", "init", "diff", "doctor", "drift", "generate", "help",
];

#[derive(Parser)]
#[command(
    name = "looper",
    version,
    about = "Static analysis tool that discovers implicit shadow dependencies in JS/TS codebases"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Scan for shadow dependencies (default when no subcommand given)
    Scan(ScanArgs),

    /// Compare scan results against a declared manifest
    Check(CheckArgs),

    /// Generate config file and initial manifest
    Init(InitArgs),

    /// Compare two manifest files
    Diff(DiffArgs),

    /// Runtime health check — verify shadow deps are satisfied
    Doctor(DoctorArgs),

    /// Compare code needs vs infrastructure provides
    Drift(DriftArgs),

    /// Auto-generate infrastructure files from scan results
    Generate(GenerateArgs),
}

#[derive(Parser)]
pub(crate) struct ScanArgs {
    /// Paths to scan (defaults to current directory)
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

    /// Output format: terminal, json, sarif, html
    #[arg(short, long, default_value = "terminal")]
    pub format: String,

    /// Write output to file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Only scan specific categories (comma-separated: env,fs,network,shell,ports,platform)
    #[arg(long, value_delimiter = ',')]
    pub only: Option<Vec<String>>,

    /// Exclude specific categories (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub exclude: Option<Vec<String>>,

    /// Minimum confidence level: low, medium, high
    #[arg(long, default_value = "low")]
    pub min_confidence: String,

    /// Only show findings not in the baseline manifest
    #[arg(long)]
    pub baseline: Option<PathBuf>,

    /// Additional glob patterns to ignore
    #[arg(long)]
    pub ignore: Option<Vec<String>>,

    /// Don't respect .gitignore
    #[arg(long)]
    pub no_gitignore: bool,

    /// Don't exclude default directories (node_modules, dist, .git, etc.)
    #[arg(long)]
    pub no_default_excludes: bool,

    /// Only output summary line
    #[arg(short, long)]
    pub quiet: bool,

    /// Show per-file scanning progress
    #[arg(short, long)]
    pub verbose: bool,

    /// Number of parallel threads (default: auto)
    #[arg(short = 'j', long)]
    pub threads: Option<usize>,

    /// Path to config file (default: auto-detect)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Disable the file-hash cache (force full rescan)
    #[arg(long)]
    pub no_cache: bool,
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
            no_default_excludes: false,
            quiet: false,
            verbose: false,
            threads: None,
            config: None,
            no_cache: false,
        }
    }
}

#[derive(Parser)]
pub(crate) struct CheckArgs {
    /// Path to declared manifest
    #[arg(short, long, default_value = "shadow-deps.json")]
    pub manifest: PathBuf,

    /// Exit 1 if new undeclared deps found (default)
    #[arg(long, default_value_t = true)]
    pub fail_on_new: bool,

    /// Also exit 1 if declared deps are no longer found
    #[arg(long)]
    pub fail_on_removed: bool,

    /// Update the manifest file with current scan results
    #[arg(long)]
    pub update: bool,

    /// Show per-file scanning progress
    #[arg(short, long)]
    pub verbose: bool,

    /// Path to config file (default: auto-detect)
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Parser)]
pub(crate) struct DiffArgs {
    /// Path to old manifest
    pub old: PathBuf,

    /// Path to new manifest
    pub new: PathBuf,

    /// Output format: terminal, json
    #[arg(short, long, default_value = "terminal")]
    pub format: String,
}

#[derive(Parser)]
pub(crate) struct DriftArgs {
    /// Path to docker-compose.yml (default: auto-detect)
    #[arg(long)]
    pub compose: Option<PathBuf>,

    /// Path to .env file (default: auto-detect)
    #[arg(long)]
    pub env: Option<PathBuf>,

    /// Path to Dockerfile (default: auto-detect)
    #[arg(long)]
    pub dockerfile: Option<PathBuf>,

    /// Exit 1 if code needs something infra doesn't provide
    #[arg(long)]
    pub fail_on_missing: bool,

    /// Output format: terminal, json
    #[arg(short, long, default_value = "terminal")]
    pub format: String,

    /// Write output to file
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Override scan paths (default: from config or '.')
    #[arg(long, value_delimiter = ',')]
    pub scan_paths: Option<Vec<PathBuf>>,

    /// Show per-file scanning progress
    #[arg(short, long)]
    pub verbose: bool,

    /// Path to config file
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Parser)]
pub(crate) struct DoctorArgs {
    /// Use pre-scanned manifest instead of live scan
    #[arg(long)]
    pub manifest: Option<PathBuf>,

    /// Timeout for network/port checks in milliseconds
    #[arg(long, default_value_t = 2000)]
    pub timeout: u64,

    /// Exit 1 if any check fails
    #[arg(long)]
    pub fail_on_error: bool,

    /// Output format: terminal, json
    #[arg(short, long, default_value = "terminal")]
    pub format: String,

    /// Write output to file
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Override scan paths
    #[arg(long, value_delimiter = ',')]
    pub scan_paths: Option<Vec<PathBuf>>,

    /// Show detailed check output
    #[arg(short, long)]
    pub verbose: bool,

    /// Path to config file
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Parser)]
pub(crate) struct GenerateArgs {
    /// What to generate: env, compose, dockerfile, readme, all
    pub target: String,

    /// Write output to file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Overwrite existing files without warning
    #[arg(long)]
    pub force: bool,

    /// Minimum confidence level for included deps (default: medium)
    #[arg(long, default_value = "medium")]
    pub min_confidence: String,

    /// Override scan paths (default: from config or '.')
    #[arg(long, value_delimiter = ',')]
    pub scan_paths: Option<Vec<PathBuf>>,

    /// Show per-file scanning progress
    #[arg(short, long)]
    pub verbose: bool,

    /// Path to config file
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Parser)]
pub(crate) struct InitArgs {
    /// Also generate .env.example from scanned shadow deps
    #[arg(long)]
    pub with_env: bool,
}

/// When the first positional arg isn't a known subcommand, insert "scan"
/// so that `looper .` and `looper src/ lib/` work as shorthand.
pub(crate) fn implicit_scan_subcommand() -> Vec<String> {
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
