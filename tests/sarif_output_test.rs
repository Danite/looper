use looper::output::build_manifest;
use looper::output::sarif::format_sarif;
use looper::types::*;
use serde_json::json;

fn sample_deps() -> Vec<ShadowDep> {
    let mut env_meta = Metadata::new();
    env_meta.insert("required", json!(true));

    vec![
        ShadowDep::new(
            Category::EnvVar,
            "DATABASE_URL",
            Confidence::High,
            Location::new("src/db.ts", 12, 5),
        )
        .with_metadata(env_meta),
        ShadowDep::new(
            Category::Network,
            "api.stripe.com",
            Confidence::High,
            Location::new("src/billing.ts", 22, 3),
        ),
        ShadowDep::new(
            Category::Binary,
            "ffmpeg",
            Confidence::Medium,
            Location::new("src/video.ts", 15, 5),
        ),
    ]
}

#[test]
fn sarif_is_valid_json() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let sarif_str = format_sarif(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sarif_str).unwrap();
    assert!(parsed.is_object());
}

#[test]
fn sarif_has_correct_version() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let sarif_str = format_sarif(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sarif_str).unwrap();

    assert_eq!(parsed["version"], "2.1.0");
    assert!(parsed["$schema"].as_str().unwrap().contains("sarif"));
}

#[test]
fn sarif_has_tool_info() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let sarif_str = format_sarif(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sarif_str).unwrap();

    let driver = &parsed["runs"][0]["tool"]["driver"];
    assert_eq!(driver["name"], "looper");
    assert_eq!(driver["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn sarif_has_six_rules() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let sarif_str = format_sarif(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sarif_str).unwrap();

    let rules = parsed["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .unwrap();
    assert_eq!(rules.len(), 6);

    let rule_ids: Vec<&str> = rules.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert!(rule_ids.contains(&"env_var"));
    assert!(rule_ids.contains(&"filesystem"));
    assert!(rule_ids.contains(&"network"));
    assert!(rule_ids.contains(&"binary"));
    assert!(rule_ids.contains(&"port"));
    assert!(rule_ids.contains(&"platform"));
}

#[test]
fn sarif_results_count() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let sarif_str = format_sarif(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sarif_str).unwrap();

    let results = parsed["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn sarif_result_has_location() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let sarif_str = format_sarif(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sarif_str).unwrap();

    let result = &parsed["runs"][0]["results"][0];
    let loc = &result["locations"][0]["physicalLocation"];

    assert!(loc["artifactLocation"]["uri"].is_string());
    assert!(loc["region"]["startLine"].is_number());
    assert!(loc["region"]["startColumn"].is_number());
}

#[test]
fn sarif_confidence_mapped_to_level() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let sarif_str = format_sarif(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sarif_str).unwrap();

    let results = parsed["runs"][0]["results"].as_array().unwrap();

    // High confidence -> error (highest SARIF severity)
    let high = results.iter().find(|r| r["ruleId"] == "env_var").unwrap();
    assert_eq!(high["level"], "error");

    // Medium confidence -> warning
    let medium = results.iter().find(|r| r["ruleId"] == "binary").unwrap();
    assert_eq!(medium["level"], "warning");
}

#[test]
fn sarif_properties_include_metadata() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let sarif_str = format_sarif(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sarif_str).unwrap();

    let result = &parsed["runs"][0]["results"][0];
    assert_eq!(result["properties"]["confidence"], "high");
    assert!(result["properties"]["shadowDepId"].is_string());
}

#[test]
fn sarif_snapshot() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let sarif_str = format_sarif(&manifest).unwrap();
    let mut parsed: serde_json::Value = serde_json::from_str(&sarif_str).unwrap();

    parsed["runs"][0]["tool"]["driver"]["version"] = serde_json::Value::String("[version]".into());
    insta::assert_json_snapshot!("sarif_output", parsed);
}

#[test]
fn sarif_empty_deps() {
    let manifest = build_manifest(".", 0, 1, Vec::new());
    let sarif_str = format_sarif(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sarif_str).unwrap();

    let results = parsed["runs"][0]["results"].as_array().unwrap();
    assert!(results.is_empty());
}
