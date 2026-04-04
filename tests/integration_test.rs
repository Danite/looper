use looper::detectors::{default_registry, DetectorContext};
use looper::parser::parse_file;
use looper::resolver::resolve_bindings;
use looper::types::{Category, Confidence, ShadowDep};
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
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

#[test]
fn full_pipeline_detects_fixture_env_vars() {
    let deps = scan_fixture_dir();

    let names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();

    assert!(names.contains(&"DATABASE_URL"));
    assert!(names.contains(&"STRIPE_SECRET_KEY"));
    assert!(names.contains(&"API_KEY"));
    assert!(names.contains(&"NODE_ENV"));
    assert!(names.contains(&"LOG_LEVEL"));
    assert!(names.contains(&"PORT"));
    assert!(names.contains(&"API_TIMEOUT"));
    assert!(names.contains(&"VITE_API_URL"));
    assert!(names.contains(&"OPTIONAL_VAR"));
}

#[test]
fn full_pipeline_all_findings_are_env_var_category() {
    let deps = scan_fixture_dir();
    for dep in &deps {
        assert_eq!(dep.category, Category::EnvVar);
    }
}

#[test]
fn full_pipeline_port_has_default() {
    let deps = scan_fixture_dir();
    let port = deps.iter().find(|d| d.name == "PORT").unwrap();
    assert_eq!(port.metadata.get_bool("required"), Some(false));
    assert_eq!(port.metadata.get_str("default_value"), Some("3000"));
}

#[test]
fn full_pipeline_database_url_is_required() {
    let deps = scan_fixture_dir();
    let db = deps.iter().find(|d| d.name == "DATABASE_URL").unwrap();
    assert_eq!(db.metadata.get_bool("required"), Some(true));
}

#[test]
fn full_pipeline_vite_api_url_is_import_meta_env() {
    let deps = scan_fixture_dir();
    let vite = deps.iter().find(|d| d.name == "VITE_API_URL").unwrap();
    assert_eq!(
        vite.metadata.get_str("source_pattern"),
        Some("import_meta_env")
    );
}

#[test]
fn full_pipeline_all_high_confidence() {
    let deps = scan_fixture_dir();
    for dep in &deps {
        assert_eq!(dep.confidence, Confidence::High);
    }
}

#[test]
fn full_pipeline_locations_have_file_paths() {
    let deps = scan_fixture_dir();
    for dep in &deps {
        for loc in &dep.locations {
            assert!(
                loc.file.contains("env_vars.ts"),
                "unexpected file: {}",
                loc.file
            );
            assert!(loc.line > 0);
            assert!(loc.col > 0);
        }
    }
}

#[test]
fn full_pipeline_no_files_returns_error() {
    let roots = vec![PathBuf::from("tests/fixtures/nonexistent")];
    let result = discover_files(&roots, &WalkOptions::default());
    assert!(result.is_err());
}
