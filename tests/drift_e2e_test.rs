use looper::detectors::{default_registry, DetectorContext};
use looper::drift;
use looper::infra;
use looper::parser::parse_file;
use looper::resolver::resolve_bindings;
use looper::types::{Category, ShadowDep};
use looper::walker::{discover_files, WalkOptions};
use oxc_allocator::Allocator;
use std::collections::HashMap;
use std::path::PathBuf;

fn scan_fixtures() -> Vec<ShadowDep> {
    let roots = vec![PathBuf::from("tests/fixtures")];
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

#[test]
fn full_drift_pipeline_produces_report() {
    let deps = scan_fixtures();
    let infra_root = PathBuf::from("tests/fixtures/infra");
    let manifest = infra::build_infra_manifest(&infra_root);
    let report = drift::compare(&deps, &manifest);

    assert!(!report.covered.is_empty(), "should have covered items");
    assert!(
        !report.missing.is_empty() || !report.external.is_empty(),
        "should have at least some missing or external items"
    );
}

#[test]
fn drift_covers_env_vars_from_compose_and_dotenv() {
    let deps = scan_fixtures();
    let manifest = infra::build_infra_manifest(&PathBuf::from("tests/fixtures/infra"));
    let report = drift::compare(&deps, &manifest);

    let covered_names: Vec<&str> = report.covered.iter().map(|i| i.name.as_str()).collect();

    // DATABASE_URL is in both .env.example and docker-compose app environment
    assert!(covered_names.contains(&"DATABASE_URL"), "DATABASE_URL should be covered: {covered_names:?}");
    // NODE_ENV is in .env.example and compose
    assert!(covered_names.contains(&"NODE_ENV"), "NODE_ENV should be covered: {covered_names:?}");
}

#[test]
fn drift_covers_ports_from_compose() {
    let deps = scan_fixtures();
    let manifest = infra::build_infra_manifest(&PathBuf::from("tests/fixtures/infra"));
    let report = drift::compare(&deps, &manifest);

    let covered_names: Vec<&str> = report.covered.iter().map(|i| i.name.as_str()).collect();

    // Port 3000 is exposed by the app service; ports.ts has app.listen(3000)
    assert!(covered_names.contains(&":3000"), ":3000 should be covered: {covered_names:?}");
    // Port 6379 is exposed by redis service; ports.ts has Redis connection
    assert!(covered_names.contains(&":6379"), ":6379 should be covered: {covered_names:?}");
}

#[test]
fn drift_covers_binaries_from_dockerfile() {
    let deps = scan_fixtures();
    let manifest = infra::build_infra_manifest(&PathBuf::from("tests/fixtures/infra"));
    let report = drift::compare(&deps, &manifest);

    let covered_names: Vec<&str> = report.covered.iter().map(|i| i.name.as_str()).collect();

    // ffmpeg, git are installed via apk in the Dockerfile; shell.ts uses them
    assert!(covered_names.contains(&"ffmpeg"), "ffmpeg should be covered: {covered_names:?}");
    assert!(covered_names.contains(&"git"), "git should be covered: {covered_names:?}");
}

#[test]
fn drift_external_hosts_are_classified() {
    let deps = scan_fixtures();
    let manifest = infra::build_infra_manifest(&PathBuf::from("tests/fixtures/infra"));
    let report = drift::compare(&deps, &manifest);

    let external_names: Vec<&str> = report.external.iter().map(|i| i.name.as_str()).collect();

    // network.ts has api.stripe.com, sentry.io — these are external APIs
    assert!(external_names.contains(&"api.stripe.com"), "api.stripe.com should be external: {external_names:?}");
    assert!(external_names.contains(&"sentry.io"), "sentry.io should be external: {external_names:?}");
}

#[test]
fn drift_report_categories_are_disjoint() {
    let deps = scan_fixtures();
    let manifest = infra::build_infra_manifest(&PathBuf::from("tests/fixtures/infra"));
    let report = drift::compare(&deps, &manifest);

    // No item should appear in more than one bucket
    let covered_ids: Vec<&str> = report.covered.iter().map(|i| i.name.as_str()).collect();
    let missing_ids: Vec<&str> = report.missing.iter().map(|i| i.name.as_str()).collect();
    let external_ids: Vec<&str> = report.external.iter().map(|i| i.name.as_str()).collect();

    for name in &covered_ids {
        assert!(!missing_ids.contains(name), "{name} is both covered and missing");
        assert!(!external_ids.contains(name), "{name} is both covered and external");
    }
    for name in &missing_ids {
        assert!(!external_ids.contains(name), "{name} is both missing and external");
    }
}

#[test]
fn drift_report_serializes_to_json() {
    let deps = scan_fixtures();
    let manifest = infra::build_infra_manifest(&PathBuf::from("tests/fixtures/infra"));
    let report = drift::compare(&deps, &manifest);

    let json = serde_json::to_string_pretty(&report).unwrap();
    assert!(json.contains("\"missing\""));
    assert!(json.contains("\"covered\""));
    assert!(json.contains("\"external\""));
    assert!(json.contains("\"unused\""));

    // Roundtrip: should parse back
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["covered"].is_array());
    assert!(parsed["missing"].is_array());
}

#[test]
fn drift_unused_infra_detected() {
    let deps = scan_fixtures();
    let manifest = infra::build_infra_manifest(&PathBuf::from("tests/fixtures/infra"));
    let report = drift::compare(&deps, &manifest);

    // docker-compose defines mongo on :27017 — but code may or may not use it.
    // The compose also defines POSTGRES_DB, POSTGRES_USER, POSTGRES_PASSWORD
    // which the JS code doesn't reference directly.
    let unused_names: Vec<&str> = report.unused.iter().map(|i| i.name.as_str()).collect();

    // At minimum, the Postgres service-level env vars should be unused by JS code
    let has_some_unused = !unused_names.is_empty();
    assert!(has_some_unused, "should detect some unused infra provisions");
}

#[test]
fn drift_all_code_deps_are_classified() {
    let deps = scan_fixtures();
    let manifest = infra::build_infra_manifest(&PathBuf::from("tests/fixtures/infra"));
    let report = drift::compare(&deps, &manifest);

    // Platform deps are skipped by drift, so filter them out
    let non_platform_deps: Vec<&ShadowDep> = deps
        .iter()
        .filter(|d| d.category != Category::Platform)
        .collect();

    let total_classified = report.covered.len() + report.missing.len() + report.external.len();

    // Every non-platform code dep should end up in one of the three buckets
    assert_eq!(
        total_classified,
        non_platform_deps.len(),
        "covered({}) + missing({}) + external({}) = {} but have {} non-platform deps",
        report.covered.len(),
        report.missing.len(),
        report.external.len(),
        total_classified,
        non_platform_deps.len(),
    );
}
