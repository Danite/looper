use looper::output::build_manifest;
use looper::output::json::format_json;
use looper::types::*;
use serde_json::json;

fn sample_deps() -> Vec<ShadowDep> {
    let mut deps = Vec::new();

    let mut env_meta = Metadata::new();
    env_meta.insert("required", json!(true));
    env_meta.insert("source_pattern", json!("direct_access"));
    deps.push(
        ShadowDep::new(
            Category::EnvVar,
            "DATABASE_URL",
            Confidence::High,
            Location::new("src/db.ts", 12, 5),
        )
        .with_metadata(env_meta),
    );

    let mut port_meta = Metadata::new();
    port_meta.insert("required", json!(false));
    port_meta.insert("default_value", json!("3000"));
    port_meta.insert("source_pattern", json!("direct_access"));
    deps.push(
        ShadowDep::new(
            Category::EnvVar,
            "PORT",
            Confidence::High,
            Location::new("src/server.ts", 5, 3),
        )
        .with_metadata(port_meta),
    );

    let mut net_meta = Metadata::new();
    net_meta.insert("protocol", json!("https"));
    net_meta.insert("path_pattern", json!("/v1/charges"));
    deps.push(
        ShadowDep::new(
            Category::Network,
            "api.stripe.com",
            Confidence::High,
            Location::new("src/billing.ts", 22, 3),
        )
        .with_metadata(net_meta),
    );

    deps
}

#[test]
fn json_output_is_valid() {
    let manifest = build_manifest(".", 142, 48, sample_deps());
    let json_str = format_json(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(parsed["schema_version"], "1");
    assert_eq!(parsed["tool_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(parsed["root"], ".");
    assert_eq!(parsed["files_scanned"], 142);
    assert_eq!(parsed["scan_duration_ms"], 48);
    assert!(parsed["scanned_at"].is_string());
    assert_eq!(parsed["deps"].as_array().unwrap().len(), 3);
}

#[test]
fn json_output_matches_spec_schema() {
    let manifest = build_manifest(".", 142, 48, sample_deps());
    let json_str = format_json(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    let first_dep = &parsed["deps"][0];
    assert!(first_dep["id"].is_string());
    assert!(first_dep["category"].is_string());
    assert!(first_dep["name"].is_string());
    assert!(first_dep["confidence"].is_string());
    assert!(first_dep["locations"].is_array());
    assert!(first_dep["locations"][0]["file"].is_string());
    assert!(first_dep["locations"][0]["line"].is_number());
    assert!(first_dep["locations"][0]["col"].is_number());
}

#[test]
fn json_output_snapshot() {
    let manifest = build_manifest(".", 142, 48, sample_deps());
    let json_str = format_json(&manifest).unwrap();
    let mut parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    parsed["scanned_at"] = serde_json::Value::String("[timestamp]".into());
    parsed["tool_version"] = serde_json::Value::String("[version]".into());

    insta::assert_json_snapshot!("json_manifest", parsed);
}

#[test]
fn json_roundtrip() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let json_str = format_json(&manifest).unwrap();
    let roundtripped: ScanManifest = serde_json::from_str(&json_str).unwrap();

    assert_eq!(roundtripped.schema_version, "1");
    assert_eq!(roundtripped.files_scanned, 10);
    assert_eq!(roundtripped.deps.len(), 3);
    assert_eq!(roundtripped.deps[0].name, "DATABASE_URL");
}

#[test]
fn json_empty_deps() {
    let manifest = build_manifest(".", 0, 1, Vec::new());
    let json_str = format_json(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(parsed["deps"].as_array().unwrap().len(), 0);
}

#[test]
fn json_metadata_omitted_when_empty() {
    let dep = ShadowDep::new(
        Category::Binary,
        "ffmpeg",
        Confidence::High,
        Location::new("src/video.ts", 15, 5),
    );
    let manifest = build_manifest(".", 1, 1, vec![dep]);
    let json_str = format_json(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert!(parsed["deps"][0].get("metadata").is_none());
}
