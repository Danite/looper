use looper::check::{check_against_manifest, load_manifest};
use looper::detectors::{default_registry, DetectorContext};
use looper::diff::diff_manifests;
use looper::output::{build_manifest, json::format_json};
use looper::parser::parse_file;
use looper::resolver::resolve_bindings;
use looper::types::*;
use looper::walker::{discover_files, WalkOptions};
use oxc_allocator::Allocator;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

fn scan_paths(paths: &[PathBuf]) -> Vec<ShadowDep> {
    let walk_result = discover_files(paths, &WalkOptions::default()).unwrap();
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

// --- Full CI workflow simulation ---

#[test]
fn ci_workflow_generate_check_update() {
    let dir = TempDir::new().unwrap();

    // Step 1: Create a JS file
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("app.ts"),
        "const url = process.env.DATABASE_URL;\nconst port = process.env.PORT || 3000;\n",
    )
    .unwrap();

    // Step 2: Generate manifest
    let deps = scan_paths(std::slice::from_ref(&src_dir));
    let manifest = build_manifest("src", deps.len(), 10, deps);
    let json = format_json(&manifest).unwrap();
    let manifest_path = dir.path().join("shadow-deps.json");
    std::fs::write(&manifest_path, &json).unwrap();

    // Step 3: Check should pass (no changes)
    let loaded = load_manifest(&manifest_path).unwrap();
    let current = scan_paths(std::slice::from_ref(&src_dir));
    let result = check_against_manifest(&current, &loaded);
    assert!(result.is_clean(), "check should pass with identical scan");

    // Step 4: Add new env var
    std::fs::write(
        src_dir.join("new-feature.ts"),
        "const key = process.env.NEW_API_KEY;\n",
    )
    .unwrap();

    // Step 5: Check should fail (new dep)
    let current = scan_paths(std::slice::from_ref(&src_dir));
    let result = check_against_manifest(&current, &loaded);
    assert!(!result.is_clean(), "check should fail after adding new dep");
    assert_eq!(result.added.len(), 1);
    assert_eq!(result.added[0].name, "NEW_API_KEY");

    // Step 6: Update manifest
    let updated_manifest = build_manifest("src", current.len(), 10, current.clone());
    let updated_json = format_json(&updated_manifest).unwrap();
    std::fs::write(&manifest_path, &updated_json).unwrap();

    // Step 7: Check should pass again
    let loaded = load_manifest(&manifest_path).unwrap();
    let result = check_against_manifest(&current, &loaded);
    assert!(result.is_clean(), "check should pass after update");
}

// --- Diff workflow ---

#[test]
fn diff_workflow_two_versions() {
    let dir = TempDir::new().unwrap();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    // Version 1: two env vars
    std::fs::write(
        src_dir.join("app.ts"),
        "const a = process.env.VAR_A;\nconst b = process.env.VAR_B;\n",
    )
    .unwrap();

    let deps_v1 = scan_paths(std::slice::from_ref(&src_dir));
    let manifest_v1 = build_manifest("src", deps_v1.len(), 5, deps_v1);
    let json_v1 = format_json(&manifest_v1).unwrap();
    let path_v1 = dir.path().join("v1.json");
    std::fs::write(&path_v1, &json_v1).unwrap();

    // Version 2: remove VAR_B, add VAR_C
    std::fs::write(
        src_dir.join("app.ts"),
        "const a = process.env.VAR_A;\nconst c = process.env.VAR_C;\n",
    )
    .unwrap();

    let deps_v2 = scan_paths(std::slice::from_ref(&src_dir));
    let manifest_v2 = build_manifest("src", deps_v2.len(), 5, deps_v2);
    let json_v2 = format_json(&manifest_v2).unwrap();
    let path_v2 = dir.path().join("v2.json");
    std::fs::write(&path_v2, &json_v2).unwrap();

    // Diff
    let loaded_v1 = load_manifest(&path_v1).unwrap();
    let loaded_v2 = load_manifest(&path_v2).unwrap();
    let diff = diff_manifests(&loaded_v1, &loaded_v2);

    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.added[0].name, "VAR_C");
    assert_eq!(diff.removed.len(), 1);
    assert_eq!(diff.removed[0].name, "VAR_B");
}

// --- Error recovery ---

#[test]
fn broken_file_still_scans_valid_files() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("good.ts"),
        "const x = process.env.GOOD_VAR;\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("broken.ts"),
        "const y = process.env.BROKEN_VAR;\nimport { foo } from ;\n",
    )
    .unwrap();

    let deps = scan_paths(&[dir.path().to_path_buf()]);
    let names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();

    assert!(names.contains(&"GOOD_VAR"));
}

// --- All fixture categories in check workflow ---

#[test]
fn check_all_categories_against_manifest() {
    let deps = scan_paths(&[PathBuf::from("tests/fixtures")]);

    let categories: std::collections::HashSet<Category> =
        deps.iter().map(|d| d.category).collect();

    assert!(categories.contains(&Category::EnvVar));
    assert!(categories.contains(&Category::Filesystem));
    assert!(categories.contains(&Category::Network));
    assert!(categories.contains(&Category::Binary));
    assert!(categories.contains(&Category::Port));
    assert!(categories.contains(&Category::Platform));

    let manifest = build_manifest("tests/fixtures", deps.len(), 10, deps.clone());
    let result = check_against_manifest(&deps, &manifest);
    assert!(result.is_clean());
}

// --- Manifest roundtrip ---

#[test]
fn manifest_json_roundtrip_preserves_all_fields() {
    let deps = scan_paths(&[PathBuf::from("tests/fixtures/env_vars.ts")]);
    let manifest = build_manifest(".", deps.len(), 5, deps);
    let json = format_json(&manifest).unwrap();
    let loaded: ScanManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.schema_version, "1");
    assert_eq!(loaded.tool_version, manifest.tool_version);
    assert_eq!(loaded.deps.len(), manifest.deps.len());

    for (orig, loaded) in manifest.deps.iter().zip(loaded.deps.iter()) {
        assert_eq!(orig.id, loaded.id);
        assert_eq!(orig.category, loaded.category);
        assert_eq!(orig.name, loaded.name);
        assert_eq!(orig.confidence, loaded.confidence);
    }
}
