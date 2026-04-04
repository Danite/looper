use looper::types::*;

// --- Category::from_filter_name ---

#[test]
fn category_from_filter_name() {
    assert_eq!(Category::from_filter_name("env"), Some(Category::EnvVar));
    assert_eq!(Category::from_filter_name("fs"), Some(Category::Filesystem));
    assert_eq!(Category::from_filter_name("network"), Some(Category::Network));
    assert_eq!(Category::from_filter_name("shell"), Some(Category::Binary));
    assert_eq!(Category::from_filter_name("ports"), Some(Category::Port));
    assert_eq!(Category::from_filter_name("platform"), Some(Category::Platform));
    assert_eq!(Category::from_filter_name("unknown"), None);
}

// --- Confidence::parse ---

#[test]
fn confidence_parse() {
    assert_eq!(Confidence::parse("low"), Some(Confidence::Low));
    assert_eq!(Confidence::parse("medium"), Some(Confidence::Medium));
    assert_eq!(Confidence::parse("high"), Some(Confidence::High));
    assert_eq!(Confidence::parse("invalid"), None);
}

// --- Category filtering ---

fn sample_deps() -> Vec<ShadowDep> {
    vec![
        ShadowDep::new(Category::EnvVar, "DB_URL", Confidence::High, Location::new("a.ts", 1, 1)),
        ShadowDep::new(Category::Network, "api.com", Confidence::High, Location::new("a.ts", 2, 1)),
        ShadowDep::new(Category::Binary, "git", Confidence::High, Location::new("a.ts", 3, 1)),
        ShadowDep::new(Category::Filesystem, "./config", Confidence::Medium, Location::new("a.ts", 4, 1)),
        ShadowDep::new(Category::Port, ":3000", Confidence::High, Location::new("a.ts", 5, 1)),
        ShadowDep::new(Category::Platform, "/dev/null", Confidence::Low, Location::new("a.ts", 6, 1)),
    ]
}

#[test]
fn only_filter() {
    let mut deps = sample_deps();
    let allowed = [Category::EnvVar, Category::Network];
    deps.retain(|d| allowed.contains(&d.category));
    assert_eq!(deps.len(), 2);
    assert_eq!(deps[0].category, Category::EnvVar);
    assert_eq!(deps[1].category, Category::Network);
}

#[test]
fn exclude_filter() {
    let mut deps = sample_deps();
    let excluded = [Category::Platform, Category::Port];
    deps.retain(|d| !excluded.contains(&d.category));
    assert_eq!(deps.len(), 4);
    assert!(deps.iter().all(|d| d.category != Category::Platform && d.category != Category::Port));
}

#[test]
fn min_confidence_high() {
    let mut deps = sample_deps();
    let min = Confidence::High;
    deps.retain(|d| d.confidence >= min);
    assert_eq!(deps.len(), 4);
    assert!(deps.iter().all(|d| d.confidence == Confidence::High));
}

#[test]
fn min_confidence_medium() {
    let mut deps = sample_deps();
    let min = Confidence::Medium;
    deps.retain(|d| d.confidence >= min);
    assert_eq!(deps.len(), 5);
    assert!(deps.iter().all(|d| d.confidence >= Confidence::Medium));
}

#[test]
fn min_confidence_low_keeps_all() {
    let mut deps = sample_deps();
    let min = Confidence::Low;
    deps.retain(|d| d.confidence >= min);
    assert_eq!(deps.len(), 6);
}

// --- Baseline filtering ---

#[test]
fn baseline_filter() {
    let deps = sample_deps();
    let baseline_ids: std::collections::HashSet<String> = deps.iter().map(|d| d.id.clone()).collect();

    let mut new_deps = sample_deps();
    new_deps.push(ShadowDep::new(
        Category::EnvVar,
        "NEW_VAR",
        Confidence::High,
        Location::new("b.ts", 1, 1),
    ));

    new_deps.retain(|d| !baseline_ids.contains(&d.id));
    assert_eq!(new_deps.len(), 1);
    assert_eq!(new_deps[0].name, "NEW_VAR");
}

#[test]
fn baseline_filter_empty_baseline() {
    let mut deps = sample_deps();
    let baseline_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    deps.retain(|d| !baseline_ids.contains(&d.id));
    assert_eq!(deps.len(), 6);
}

// --- Baseline via ScanManifest deserialization ---

#[test]
fn baseline_from_json_manifest() {
    let manifest = ScanManifest::new(".", 1, 1);
    let json = serde_json::to_string(&manifest).unwrap();
    let parsed: ScanManifest = serde_json::from_str(&json).unwrap();
    assert!(parsed.deps.is_empty());
}

// --- Combined filters ---

#[test]
fn only_and_min_confidence_combined() {
    let mut deps = sample_deps();
    let allowed = [Category::EnvVar, Category::Filesystem, Category::Platform];
    deps.retain(|d| allowed.contains(&d.category));
    let min = Confidence::Medium;
    deps.retain(|d| d.confidence >= min);
    assert_eq!(deps.len(), 2);
    let names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"DB_URL"));
    assert!(names.contains(&"./config"));
}
