use colored::Colorize;
use looper::detectors::{default_registry, DetectorContext};
use looper::parser::parse_file;
use looper::resolver::resolve_bindings;
use looper::types::ShadowDep;
use looper::walker::{discover_files, WalkError, WalkOptions};
use oxc_allocator::Allocator;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use crate::cli::ScanArgs;

pub(crate) const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB

pub(crate) struct ScanResult {
    pub deps: Vec<ShadowDep>,
    pub file_count: usize,
    pub duration_ms: u64,
    pub root: String,
    pub errors: Vec<String>,
}

pub(crate) fn log_scan_errors(errors: &[String], verbose: bool) {
    if verbose && !errors.is_empty() {
        eprintln!();
        for err in errors {
            eprintln!("  {} {err}", "warning:".yellow());
        }
    }
}

/// Deduplicate deps by id, merging locations.
pub(crate) fn merge_deps(deps: Vec<ShadowDep>) -> Vec<ShadowDep> {
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

pub(crate) fn run_scan(
    paths: &[PathBuf],
    no_gitignore: bool,
    extra_ignores: Vec<String>,
    verbose: bool,
    config_path: Option<&Path>,
) -> Result<ScanResult, i32> {
    let config = looper::config::load_config(config_path);

    let mut ignores = extra_ignores;
    if let Some(ref cfg) = config {
        ignores.extend(cfg.exclude.iter().cloned());
    }

    let opts = WalkOptions {
        no_gitignore,
        no_default_excludes: false,
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
    let mut walk_errs: Vec<String> = walk_result
        .errors
        .iter()
        .map(|e| format!("walk: {e}"))
        .collect();
    let parse_errors: Mutex<Vec<String>> = Mutex::new(Vec::new());

    walk_result.files.par_iter().for_each(|path| {
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > MAX_FILE_SIZE {
                return;
            }
        }

        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                parse_errors
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(format!("{}: read error: {e}", path.display()));
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
            parse_errors
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("{}: parser panicked", path.display()));
            return;
        }

        if result.has_errors() {
            for err in &result.errors {
                parse_errors
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
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
            all_deps.lock().unwrap_or_else(|e| e.into_inner()).extend(deps);
        }
    });

    let duration_ms = start.elapsed().as_millis() as u64;
    let raw_deps = all_deps.into_inner().unwrap_or_else(|e| e.into_inner());
    let mut errors = parse_errors.into_inner().unwrap_or_else(|e| e.into_inner());
    errors.append(&mut walk_errs);
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
        errors,
    })
}

pub(crate) fn cmd_scan(args: ScanArgs) -> i32 {
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

    let scan_paths = if args.paths.len() == 1 && args.paths[0] == Path::new(".") {
        if let Some(ref cfg) = config {
            if !cfg.include.is_empty() {
                cfg.include
                    .iter()
                    .map(|pattern| {
                        let root = pattern.split("/**").next().unwrap_or(pattern);
                        PathBuf::from(root)
                    })
                    .filter(|p| p.exists())
                    .collect::<Vec<_>>()
            } else {
                args.paths.clone()
            }
        } else {
            args.paths.clone()
        }
    } else {
        args.paths.clone()
    };

    let scan_paths = if scan_paths.is_empty() {
        args.paths.clone()
    } else {
        scan_paths
    };

    let opts = WalkOptions {
        no_gitignore: args.no_gitignore,
        no_default_excludes: args.no_default_excludes,
        extra_ignore_patterns: extra_ignores,
        follow_symlinks: true,
    };

    let start = Instant::now();

    let walk_result = match discover_files(&scan_paths, &opts) {
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

    if args.verbose && !walk_result.errors.is_empty() {
        for err in &walk_result.errors {
            eprintln!("  {} walk: {err}", "warning:".yellow());
        }
    }

    let file_count = walk_result.files.len();
    let registry = default_registry();
    let all_deps: Mutex<Vec<ShadowDep>> = Mutex::new(Vec::new());
    let parse_errors: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let skipped_large = std::sync::atomic::AtomicUsize::new(0);
    let skipped_read = std::sync::atomic::AtomicUsize::new(0);

    walk_result.files.par_iter().for_each(|path| {
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > MAX_FILE_SIZE {
                skipped_large.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if args.verbose {
                    eprintln!("  {} skipping large file: {} ({}MB)", "warning:".yellow(), path.display(), meta.len() / 1_000_000);
                }
                return;
            }
        }

        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                skipped_read.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if args.verbose {
                    eprintln!("  {} {}: {e}", "warning:".yellow(), path.display());
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
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("{}: parser panicked", path.display()));
            return;
        }

        if result.has_errors() {
            for err in &result.errors {
                parse_errors
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
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
            all_deps.lock().unwrap_or_else(|e| e.into_inner()).extend(deps);
        }
    });

    let duration = start.elapsed();
    let duration_ms = duration.as_millis() as u64;

    let raw_deps = all_deps.into_inner().unwrap_or_else(|e| e.into_inner());
    let errors = parse_errors.into_inner().unwrap_or_else(|e| e.into_inner());

    let mut merged = merge_deps(raw_deps);

    if let Some(ref only) = args.only {
        let mut allowed = Vec::new();
        for s in only {
            match looper::types::Category::from_filter_name(s) {
                Some(cat) => allowed.push(cat),
                None => eprintln!("  {} unknown category '{}'. Valid: env, fs, network, shell, ports, platform", "warning:".yellow(), s),
            }
        }
        if allowed.is_empty() {
            eprintln!("{} --only contained no valid categories", "error:".red());
            return 1;
        }
        merged.retain(|d| allowed.contains(&d.category));
    }

    if let Some(ref exclude) = args.exclude {
        let mut excluded = Vec::new();
        for s in exclude {
            match looper::types::Category::from_filter_name(s) {
                Some(cat) => excluded.push(cat),
                None => eprintln!("  {} unknown category '{}'. Valid: env, fs, network, shell, ports, platform", "warning:".yellow(), s),
            }
        }
        merged.retain(|d| !excluded.contains(&d.category));
    }

    if let Some(min) = looper::types::Confidence::parse(&args.min_confidence) {
        merged.retain(|d| d.confidence >= min);
    }

    if let Some(ref baseline_path) = args.baseline {
        if let Ok(content) = std::fs::read_to_string(baseline_path) {
            if let Ok(baseline) = serde_json::from_str::<looper::types::ScanManifest>(&content) {
                let baseline_ids: std::collections::HashSet<String> =
                    baseline.deps.iter().map(|d| d.id.clone()).collect();
                merged.retain(|d| !baseline_ids.contains(&d.id));
            }
        }
    }

    if let Some(ref cfg) = config {
        merged.retain(|dep| {
            let cat_name = dep.category.to_string();
            let ignored = cfg.ignored_names(&cat_name);
            !ignored.contains(&dep.name)
        });
    }

    let has_errors = !errors.is_empty();
    let n_large = skipped_large.load(std::sync::atomic::Ordering::Relaxed);
    let n_read = skipped_read.load(std::sync::atomic::Ordering::Relaxed);

    if n_large > 0 || n_read > 0 {
        let mut parts = Vec::new();
        if n_large > 0 {
            parts.push(format!("{n_large} too large"));
        }
        if n_read > 0 {
            parts.push(format!("{n_read} unreadable"));
        }
        eprintln!("  {} {} files skipped ({})", "warning:".yellow(), n_large + n_read, parts.join(", "));
    }

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
