use looper::generate::env::generate_env;
use looper::types::{Category, Confidence, Location, Metadata, ShadowDep};
use serde_json::json;

fn env_dep(name: &str, required: bool, default_value: Option<&str>) -> ShadowDep {
    let mut meta = Metadata::new();
    meta.insert("required", json!(required));
    meta.insert("source_pattern", json!("direct_access"));
    if let Some(dv) = default_value {
        meta.insert("default_value", json!(dv));
    }
    ShadowDep::new(
        Category::EnvVar,
        name,
        Confidence::High,
        Location::new("src/config.ts", 10, 1),
    )
    .with_metadata(meta)
}

#[test]
fn empty_deps_produces_header() {
    let result = generate_env(&[]);
    assert!(result.contains("No environment variables"));
}

#[test]
fn required_var_uses_placeholder() {
    let deps = vec![env_dep("DATABASE_URL", true, None)];
    let result = generate_env(&deps);
    assert!(result.contains("DATABASE_URL=https://"));
    assert!(result.contains("Required"));
}

#[test]
fn optional_var_uses_default() {
    let deps = vec![env_dep("PORT", false, Some("3000"))];
    let result = generate_env(&deps);
    assert!(result.contains("PORT=3000"));
    assert!(result.contains("Optional"));
}

#[test]
fn groups_required_before_optional() {
    let deps = vec![
        env_dep("PORT", false, Some("3000")),
        env_dep("API_KEY", true, None),
    ];
    let result = generate_env(&deps);
    let req_pos = result.find("Required").unwrap();
    let opt_pos = result.find("Optional").unwrap();
    assert!(req_pos < opt_pos);
}

#[test]
fn includes_location_comment() {
    let deps = vec![env_dep("FOO", true, None)];
    let result = generate_env(&deps);
    assert!(result.contains("src/config.ts:10"));
}

#[test]
fn placeholder_for_secret() {
    let deps = vec![env_dep("STRIPE_SECRET_KEY", true, None)];
    let result = generate_env(&deps);
    assert!(result.contains("STRIPE_SECRET_KEY=changeme"));
}

#[test]
fn placeholder_for_host() {
    let deps = vec![env_dep("REDIS_HOST", true, None)];
    let result = generate_env(&deps);
    assert!(result.contains("REDIS_HOST=localhost"));
}

#[test]
fn placeholder_for_url() {
    let deps = vec![env_dep("DATABASE_URL", true, None)];
    let result = generate_env(&deps);
    assert!(result.contains("DATABASE_URL=https://"));
}

#[test]
fn placeholder_for_port() {
    let deps = vec![env_dep("REDIS_PORT", true, None), env_dep("PORT", true, None)];
    let result = generate_env(&deps);
    assert!(result.contains("REDIS_PORT=0"));
    assert!(result.contains("PORT=3000"));
}

#[test]
fn non_env_deps_are_ignored() {
    let binary_dep = ShadowDep::new(
        Category::Binary,
        "ffmpeg",
        Confidence::High,
        Location::new("src/video.ts", 5, 1),
    );
    let deps = vec![binary_dep, env_dep("API_KEY", true, None)];
    let result = generate_env(&deps);
    assert!(!result.contains("ffmpeg"));
    assert!(result.contains("API_KEY"));
}
