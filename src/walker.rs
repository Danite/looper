use ignore::overrides::OverrideBuilder;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts",
];

/// Directories excluded by default even without .gitignore or config.
const DEFAULT_EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "coverage",
    ".next",
    ".nuxt",
    ".output",
    ".svelte-kit",
    "__pycache__",
    ".tox",
    "vendor",
    ".cache",
    ".parcel-cache",
    "tmp",
    ".turbo",
];

#[derive(Debug)]
pub struct WalkOptions {
    pub no_gitignore: bool,
    pub no_default_excludes: bool,
    pub extra_ignore_patterns: Vec<String>,
    pub follow_symlinks: bool,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            no_gitignore: false,
            no_default_excludes: false,
            extra_ignore_patterns: Vec::new(),
            follow_symlinks: true,
        }
    }
}

#[derive(Debug)]
pub struct WalkResult {
    pub files: Vec<PathBuf>,
    pub errors: Vec<String>,
}

/// Discover JS/TS files under the given root paths.
///
/// Uses the `ignore` crate for .gitignore-aware, parallel walking.
/// Returns an error if no matching files are found.
pub fn discover_files(roots: &[PathBuf], opts: &WalkOptions) -> Result<WalkResult, WalkError> {
    if roots.is_empty() {
        return Err(WalkError::NoFilesFound);
    }

    let first = &roots[0];
    let mut builder = WalkBuilder::new(first);

    for root in &roots[1..] {
        builder.add(root);
    }

    builder
        .git_ignore(!opts.no_gitignore)
        .git_global(!opts.no_gitignore)
        .git_exclude(!opts.no_gitignore)
        .follow_links(opts.follow_symlinks)
        .hidden(true);

    if !opts.extra_ignore_patterns.is_empty() {
        let mut overrides = OverrideBuilder::new(first);
        for pattern in &opts.extra_ignore_patterns {
            // Negate the pattern so it acts as an ignore (exclude) rule
            let negated = format!("!{pattern}");
            overrides
                .add(&negated)
                .map_err(|e| WalkError::Config(e.to_string()))?;
        }
        let built = overrides
            .build()
            .map_err(|e| WalkError::Config(e.to_string()))?;
        builder.overrides(built);
    }

    let files = Mutex::new(Vec::new());
    let errors = Mutex::new(Vec::new());

    let skip_defaults = !opts.no_default_excludes;

    builder.build_parallel().run(|| {
        Box::new(|entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    errors.lock().unwrap_or_else(|e| e.into_inner()).push(e.to_string());
                    return ignore::WalkState::Continue;
                }
            };

            // Skip default-excluded directories (prunes entire subtree)
            if skip_defaults && entry.file_type().is_some_and(|ft| ft.is_dir()) {
                if let Some(name) = entry.path().file_name().and_then(|n| n.to_str()) {
                    if DEFAULT_EXCLUDED_DIRS.contains(&name) {
                        return ignore::WalkState::Skip;
                    }
                }
                return ignore::WalkState::Continue;
            }

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                return ignore::WalkState::Continue;
            }

            let path = entry.path();

            if !has_supported_extension(path) {
                return ignore::WalkState::Continue;
            }

            files.lock().unwrap_or_else(|e| e.into_inner()).push(path.to_path_buf());
            ignore::WalkState::Continue
        })
    });

    let mut result = WalkResult {
        files: files.into_inner().unwrap_or_else(|e| e.into_inner()),
        errors: errors.into_inner().unwrap_or_else(|e| e.into_inner()),
    };

    result.files.sort();

    if result.files.is_empty() {
        return Err(WalkError::NoFilesFound);
    }

    Ok(result)
}

fn has_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| SUPPORTED_EXTENSIONS.contains(&ext))
}

#[derive(Debug)]
pub enum WalkError {
    NoFilesFound,
    Config(String),
}

impl std::fmt::Display for WalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoFilesFound => write!(f, "no JavaScript/TypeScript files found"),
            Self::Config(msg) => write!(f, "walker configuration error: {msg}"),
        }
    }
}

impl std::error::Error for WalkError {}
