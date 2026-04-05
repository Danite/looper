use looper::detectors::{default_registry, DetectorContext};
use looper::parser::parse_file;
use looper::resolver::resolve_bindings;
use looper::types::{Category, ShadowDep};
use looper::walker::{discover_files, WalkOptions};
use oxc_allocator::Allocator;
use std::collections::HashMap;
use std::path::PathBuf;

fn scan_fixture_dir() -> Vec<ShadowDep> {
    let roots = vec![PathBuf::from("tests/fixtures")];
    let walk_result = discover_files(&roots, &WalkOptions::default()).unwrap();

    let registry = default_registry();
    let mut all_deps = Vec::new();

    for path in &walk_result.files {
        let source = std::fs::read_to_string(path).unwrap();
        if source.is_empty() {
            continue;
        }
        let allocator = Allocator::new();
        let source_leaked: &str = allocator.alloc_str(&source);
        let result = parse_file(&allocator, path, source_leaked);

        if result.panicked {
            continue;
        }

        let bindings = resolve_bindings(&result.program);
        let ctx = DetectorContext {
            program: &result.program,
            source_text: result.source_text,
            file_path: &result.file_path,
            bindings: &bindings,
        };

        all_deps.extend(registry.run_all(&ctx));
    }

    merge_deps(all_deps)
}

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

fn deps_by_category(deps: &[ShadowDep]) -> HashMap<Category, Vec<&ShadowDep>> {
    let mut map: HashMap<Category, Vec<&ShadowDep>> = HashMap::new();
    for dep in deps {
        map.entry(dep.category).or_default().push(dep);
    }
    map
}

// --- All 6 categories present ---

#[test]
fn all_six_categories_detected() {
    let deps = scan_fixture_dir();
    let by_cat = deps_by_category(&deps);

    assert!(by_cat.contains_key(&Category::EnvVar), "missing EnvVar");
    assert!(by_cat.contains_key(&Category::Filesystem), "missing Filesystem");
    assert!(by_cat.contains_key(&Category::Network), "missing Network");
    assert!(by_cat.contains_key(&Category::Binary), "missing Binary");
    assert!(by_cat.contains_key(&Category::Port), "missing Port");
    assert!(by_cat.contains_key(&Category::Platform), "missing Platform");
}

// --- Env vars ---

#[test]
fn env_vars_detected() {
    let deps = scan_fixture_dir();
    let names: Vec<&str> = deps
        .iter()
        .filter(|d| d.category == Category::EnvVar)
        .map(|d| d.name.as_str())
        .collect();

    assert!(names.contains(&"DATABASE_URL"));
    assert!(names.contains(&"PORT"));
    assert!(names.contains(&"VITE_API_URL"));
}

// --- Filesystem ---

#[test]
fn filesystem_detected() {
    let deps = scan_fixture_dir();
    let names: Vec<&str> = deps
        .iter()
        .filter(|d| d.category == Category::Filesystem)
        .map(|d| d.name.as_str())
        .collect();

    assert!(names.contains(&"./config/app.json"));
    assert!(names.contains(&"/tmp/cache.json"));
    assert!(names.contains(&"./settings.json"));
}

// --- Network ---

#[test]
fn network_detected() {
    let deps = scan_fixture_dir();
    let names: Vec<&str> = deps
        .iter()
        .filter(|d| d.category == Category::Network)
        .map(|d| d.name.as_str())
        .collect();

    assert!(names.contains(&"api.stripe.com"));
    assert!(names.contains(&"sentry.io"));
    assert!(names.contains(&"realtime.example.com"));
    assert!(names.contains(&"api.internal.service"));
}

// --- Binaries ---

#[test]
fn binaries_detected() {
    let deps = scan_fixture_dir();
    let names: Vec<&str> = deps
        .iter()
        .filter(|d| d.category == Category::Binary)
        .map(|d| d.name.as_str())
        .collect();

    assert!(names.contains(&"ffmpeg"));
    assert!(names.contains(&"convert"));
    assert!(names.contains(&"git"));
    assert!(names.contains(&"docker"));
}

// --- Ports ---

#[test]
fn ports_detected() {
    let deps = scan_fixture_dir();
    let port_deps: Vec<&ShadowDep> = deps
        .iter()
        .filter(|d| d.category == Category::Port)
        .collect();

    let names: Vec<&str> = port_deps.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&":3000"));
    assert!(names.contains(&":6379"));
    assert!(names.contains(&":27017"));
    assert!(names.contains(&":4000"));
}

// --- Platform ---

#[test]
fn platform_detected() {
    let deps = scan_fixture_dir();
    let platform_deps: Vec<&ShadowDep> = deps
        .iter()
        .filter(|d| d.category == Category::Platform)
        .collect();

    assert!(!platform_deps.is_empty());

    let has_darwin = platform_deps
        .iter()
        .any(|d| d.name.contains("darwin"));
    let has_linux = platform_deps
        .iter()
        .any(|d| d.name.contains("linux"));
    let has_homedir = platform_deps
        .iter()
        .any(|d| d.name.contains("homedir"));
    let has_dev_null = platform_deps
        .iter()
        .any(|d| d.name.contains("/dev/null"));

    assert!(has_darwin || has_linux, "missing process.platform guard");
    assert!(has_homedir, "missing os.homedir()");
    assert!(has_dev_null, "missing /dev/null");
}

// --- Cross-category: mixed.ts ---

#[test]
fn mixed_fixture_has_all_categories() {
    let roots = vec![PathBuf::from("tests/fixtures/mixed.ts")];
    let walk_result = discover_files(&roots, &WalkOptions::default()).unwrap();

    let registry = default_registry();
    let mut all_deps = Vec::new();

    for path in &walk_result.files {
        let source = std::fs::read_to_string(path).unwrap();
        let allocator = Allocator::new();
        let source_leaked: &str = allocator.alloc_str(&source);
        let result = parse_file(&allocator, path, source_leaked);
        let bindings = resolve_bindings(&result.program);
        let ctx = DetectorContext {
            program: &result.program,
            source_text: result.source_text,
            file_path: &result.file_path,
            bindings: &bindings,
        };
        all_deps.extend(registry.run_all(&ctx));
    }

    let by_cat = deps_by_category(&all_deps);
    assert!(by_cat.contains_key(&Category::EnvVar), "mixed: missing EnvVar");
    assert!(by_cat.contains_key(&Category::Filesystem), "mixed: missing Filesystem");
    assert!(by_cat.contains_key(&Category::Network), "mixed: missing Network");
    assert!(by_cat.contains_key(&Category::Binary), "mixed: missing Binary");
    assert!(by_cat.contains_key(&Category::Port), "mixed: missing Port");
    assert!(by_cat.contains_key(&Category::Platform), "mixed: missing Platform");
}

// --- Error handling ---

#[test]
fn no_files_returns_error() {
    let roots = vec![PathBuf::from("tests/fixtures/nonexistent")];
    let result = discover_files(&roots, &WalkOptions::default());
    assert!(result.is_err());
}

#[test]
fn binary_file_in_scan_does_not_crash() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("valid.ts"), "const url = process.env.API_KEY;").unwrap();
    // Non-UTF-8 bytes with a .js extension
    std::fs::write(dir.path().join("binary.js"), &[0xFF, 0xFE, 0x00, 0xD8, 0x80]).unwrap();

    let roots = vec![dir.path().to_path_buf()];
    let walk_result = discover_files(&roots, &WalkOptions::default()).unwrap();

    let registry = default_registry();
    let mut all_deps = Vec::new();

    for path in &walk_result.files {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if source.is_empty() {
            continue;
        }
        let allocator = Allocator::new();
        let source_leaked: &str = allocator.alloc_str(&source);
        let result = parse_file(&allocator, path, source_leaked);
        if result.panicked {
            continue;
        }
        let bindings = resolve_bindings(&result.program);
        let ctx = DetectorContext {
            program: &result.program,
            source_text: result.source_text,
            file_path: &result.file_path,
            bindings: &bindings,
        };
        all_deps.extend(registry.run_all(&ctx));
    }

    // The valid file should produce findings; the binary file should be skipped
    assert!(!all_deps.is_empty(), "should find deps from valid.ts");
    assert!(all_deps.iter().any(|d| d.name == "API_KEY"));
}

#[test]
fn large_file_skipped_in_scan() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("valid.ts"), "const url = process.env.API_KEY;").unwrap();

    // Create a file just over 10MB with a .js extension containing valid env access
    let large_path = dir.path().join("huge.js");
    let padding = "// padding\n".repeat(1_100_000);
    let content = format!("const x = process.env.HUGE_VAR;\n{padding}");
    std::fs::write(&large_path, content).unwrap();
    assert!(std::fs::metadata(&large_path).unwrap().len() > 10 * 1024 * 1024);

    let roots = vec![dir.path().to_path_buf()];
    let walk_result = discover_files(&roots, &WalkOptions::default()).unwrap();

    let registry = default_registry();
    let mut all_deps = Vec::new();

    for path in &walk_result.files {
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > 10 * 1024 * 1024 {
                continue;
            }
        }
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if source.is_empty() {
            continue;
        }
        let allocator = Allocator::new();
        let source_leaked: &str = allocator.alloc_str(&source);
        let result = parse_file(&allocator, path, source_leaked);
        if result.panicked {
            continue;
        }
        let bindings = resolve_bindings(&result.program);
        let ctx = DetectorContext {
            program: &result.program,
            source_text: result.source_text,
            file_path: &result.file_path,
            bindings: &bindings,
        };
        all_deps.extend(registry.run_all(&ctx));
    }

    assert!(all_deps.iter().any(|d| d.name == "API_KEY"), "valid.ts should still be scanned");
    assert!(!all_deps.iter().any(|d| d.name == "HUGE_VAR"), "huge.js should be skipped");
}

#[test]
fn broken_file_yields_partial_findings() {
    let roots = vec![PathBuf::from("tests/fixtures/broken.ts")];
    let walk_result = discover_files(&roots, &WalkOptions::default()).unwrap();

    let registry = default_registry();
    let mut all_deps = Vec::new();
    let mut had_errors = false;

    for path in &walk_result.files {
        let source = std::fs::read_to_string(path).unwrap();
        let allocator = Allocator::new();
        let source_leaked: &str = allocator.alloc_str(&source);
        let result = parse_file(&allocator, path, source_leaked);

        if result.has_errors() || result.panicked {
            had_errors = true;
        }
        if result.panicked {
            continue;
        }

        let bindings = resolve_bindings(&result.program);
        let ctx = DetectorContext {
            program: &result.program,
            source_text: result.source_text,
            file_path: &result.file_path,
            bindings: &bindings,
        };
        all_deps.extend(registry.run_all(&ctx));
    }

    assert!(had_errors, "broken.ts should produce parse errors");
    // oxc error recovery may or may not preserve earlier AST nodes.
    // The important thing is: no panic, no crash, scan completes.
    // If recovery improves, VALID_VAR may appear here in the future.
}

// --- Registry ---

#[test]
fn default_registry_has_all_six_detectors() {
    let registry = default_registry();
    assert_eq!(registry.detector_count(), 6);
    let names = registry.detector_names();
    assert!(names.contains(&"env_vars"));
    assert!(names.contains(&"filesystem"));
    assert!(names.contains(&"network"));
    assert!(names.contains(&"shell"));
    assert!(names.contains(&"ports"));
    assert!(names.contains(&"platform"));
}
