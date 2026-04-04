use looper::types::*;
use serde_json::json;

#[test]
fn category_serializes_to_snake_case() {
    assert_eq!(serde_json::to_string(&Category::EnvVar).unwrap(), "\"env_var\"");
    assert_eq!(serde_json::to_string(&Category::Filesystem).unwrap(), "\"filesystem\"");
    assert_eq!(serde_json::to_string(&Category::Network).unwrap(), "\"network\"");
    assert_eq!(serde_json::to_string(&Category::Binary).unwrap(), "\"binary\"");
    assert_eq!(serde_json::to_string(&Category::Port).unwrap(), "\"port\"");
    assert_eq!(serde_json::to_string(&Category::Platform).unwrap(), "\"platform\"");
}

#[test]
fn category_deserializes_from_snake_case() {
    assert_eq!(
        serde_json::from_str::<Category>("\"env_var\"").unwrap(),
        Category::EnvVar
    );
    assert_eq!(
        serde_json::from_str::<Category>("\"platform\"").unwrap(),
        Category::Platform
    );
}

#[test]
fn confidence_serializes_to_lowercase() {
    assert_eq!(serde_json::to_string(&Confidence::High).unwrap(), "\"high\"");
    assert_eq!(serde_json::to_string(&Confidence::Medium).unwrap(), "\"medium\"");
    assert_eq!(serde_json::to_string(&Confidence::Low).unwrap(), "\"low\"");
}

#[test]
fn confidence_ordering() {
    assert!(Confidence::Low < Confidence::Medium);
    assert!(Confidence::Medium < Confidence::High);
}

#[test]
fn shadow_dep_id_generation() {
    let dep = ShadowDep::new(
        Category::EnvVar,
        "DATABASE_URL",
        Confidence::High,
        Location::new("src/db.ts", 12, 5),
    );
    assert_eq!(dep.id, "env_var::DATABASE_URL");
}

#[test]
fn shadow_dep_json_roundtrip() {
    let mut meta = Metadata::new();
    meta.insert("required", json!(true));
    meta.insert("source_pattern", json!("direct_access"));

    let dep = ShadowDep::new(
        Category::EnvVar,
        "DATABASE_URL",
        Confidence::High,
        Location::new("src/db.ts", 12, 5),
    )
    .with_metadata(meta);

    let json = serde_json::to_string_pretty(&dep).unwrap();
    let roundtripped: ShadowDep = serde_json::from_str(&json).unwrap();
    assert_eq!(dep, roundtripped);
}

#[test]
fn shadow_dep_json_matches_spec_schema() {
    let mut meta = Metadata::new();
    meta.insert("required", json!(true));
    meta.insert("source_pattern", json!("direct_access"));

    let dep = ShadowDep::new(
        Category::EnvVar,
        "DATABASE_URL",
        Confidence::High,
        Location::new("src/db.ts", 12, 5),
    )
    .with_metadata(meta);

    let value: serde_json::Value = serde_json::to_value(&dep).unwrap();

    assert_eq!(value["id"], "env_var::DATABASE_URL");
    assert_eq!(value["category"], "env_var");
    assert_eq!(value["name"], "DATABASE_URL");
    assert_eq!(value["confidence"], "high");
    assert_eq!(value["metadata"]["required"], true);
    assert_eq!(value["locations"][0]["file"], "src/db.ts");
    assert_eq!(value["locations"][0]["line"], 12);
    assert_eq!(value["locations"][0]["col"], 5);
}

#[test]
fn shadow_dep_without_metadata_omits_field() {
    let dep = ShadowDep::new(
        Category::Binary,
        "ffmpeg",
        Confidence::High,
        Location::new("src/video.ts", 15, 5),
    );

    let value: serde_json::Value = serde_json::to_value(&dep).unwrap();
    assert!(value.get("metadata").is_none());
}

#[test]
fn shadow_dep_merge_locations() {
    let mut dep1 = ShadowDep::new(
        Category::EnvVar,
        "PORT",
        Confidence::High,
        Location::new("src/server.ts", 5, 3),
    );
    let dep2 = ShadowDep::new(
        Category::EnvVar,
        "PORT",
        Confidence::High,
        Location::new("src/config.ts", 10, 1),
    );

    dep1.merge_locations(&dep2);
    assert_eq!(dep1.locations.len(), 2);
    assert_eq!(dep1.locations[1].file, "src/config.ts");
}

#[test]
fn shadow_dep_merge_locations_no_duplicates() {
    let mut dep1 = ShadowDep::new(
        Category::EnvVar,
        "PORT",
        Confidence::High,
        Location::new("src/server.ts", 5, 3),
    );
    let dep2 = ShadowDep::new(
        Category::EnvVar,
        "PORT",
        Confidence::High,
        Location::new("src/server.ts", 5, 3),
    );

    dep1.merge_locations(&dep2);
    assert_eq!(dep1.locations.len(), 1);
}

#[test]
fn port_dep_id_includes_direction() {
    let dep = ShadowDep {
        id: "port::3000::listen".into(),
        category: Category::Port,
        name: ":3000".into(),
        confidence: Confidence::High,
        metadata: Metadata::default(),
        locations: vec![Location::new("src/server.ts", 45, 1)],
    };

    let value: serde_json::Value = serde_json::to_value(&dep).unwrap();
    assert_eq!(value["id"], "port::3000::listen");
}

#[test]
fn scan_manifest_json_roundtrip() {
    let mut manifest = ScanManifest::new(".", 142, 48);
    manifest.deps.push(ShadowDep::new(
        Category::Network,
        "api.stripe.com",
        Confidence::High,
        Location::new("src/billing.ts", 22, 3),
    ));

    let json = serde_json::to_string_pretty(&manifest).unwrap();
    let roundtripped: ScanManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(roundtripped.schema_version, "1");
    assert_eq!(roundtripped.files_scanned, 142);
    assert_eq!(roundtripped.deps.len(), 1);
    assert_eq!(roundtripped.deps[0].name, "api.stripe.com");
}

#[test]
fn location_display() {
    let loc = Location::new("src/db.ts", 12, 5);
    assert_eq!(loc.to_string(), "src/db.ts:12:5");
}

#[test]
fn category_display_matches_serde() {
    for cat in Category::all() {
        let display = cat.to_string();
        let serde = serde_json::to_string(cat).unwrap();
        assert_eq!(format!("\"{display}\""), serde);
    }
}

#[test]
fn confidence_display_matches_serde() {
    for conf in [Confidence::Low, Confidence::Medium, Confidence::High] {
        let display = conf.to_string();
        let serde = serde_json::to_string(&conf).unwrap();
        assert_eq!(format!("\"{display}\""), serde);
    }
}

#[test]
fn category_all_has_six_entries() {
    assert_eq!(Category::all().len(), 6);
}

#[test]
fn metadata_accessors() {
    let mut meta = Metadata::new();
    meta.insert("required", json!(true));
    meta.insert("default_value", json!("3000"));

    assert_eq!(meta.get_bool("required"), Some(true));
    assert_eq!(meta.get_str("default_value"), Some("3000"));
    assert!(meta.get("nonexistent").is_none());
}
