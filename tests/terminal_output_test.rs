use looper::output::terminal::{format_summary, format_terminal};
use looper::types::*;

fn sample_deps() -> Vec<ShadowDep> {
    vec![
        ShadowDep::new(
            Category::EnvVar,
            "DATABASE_URL",
            Confidence::High,
            Location::new("src/db.ts", 12, 5),
        ),
        ShadowDep::new(
            Category::EnvVar,
            "API_KEY",
            Confidence::Medium,
            Location::new("src/api.ts", 3, 1),
        ),
        ShadowDep::new(
            Category::Network,
            "api.stripe.com",
            Confidence::High,
            Location::new("src/pay.ts", 8, 10),
        ),
        ShadowDep::new(
            Category::Binary,
            "ffmpeg",
            Confidence::High,
            Location::new("src/media.ts", 15, 1),
        ),
        ShadowDep::new(
            Category::Filesystem,
            "./config/app.json",
            Confidence::Medium,
            Location::new("src/config.ts", 4, 1),
        ),
        ShadowDep::new(
            Category::Port,
            ":3000",
            Confidence::High,
            Location::new("src/server.ts", 20, 1),
        ),
        ShadowDep::new(
            Category::Platform,
            "process.platform == darwin",
            Confidence::Low,
            Location::new("src/os.ts", 7, 1),
        ),
    ]
}

// --- format_terminal ---

#[test]
fn terminal_header_includes_version_and_stats() {
    let output = format_terminal(&sample_deps(), 42, 123);
    assert!(output.contains("looper"), "should include tool name");
    assert!(output.contains("42"), "should include file count");
    assert!(output.contains("123"), "should include duration");
}

#[test]
fn terminal_groups_by_category_with_headers() {
    let output = format_terminal(&sample_deps(), 10, 5);

    assert!(output.contains("ENV VARS"), "should have env var category header");
    assert!(output.contains("NETWORK"), "should have network category header");
    assert!(output.contains("BINARIES"), "should have binary category header");
    assert!(output.contains("FILESYSTEM"), "should have filesystem category header");
    assert!(output.contains("PORTS"), "should have port category header");
    assert!(output.contains("PLATFORM"), "should have platform category header");
}

#[test]
fn terminal_shows_dep_names() {
    let output = format_terminal(&sample_deps(), 10, 5);

    assert!(output.contains("DATABASE_URL"));
    assert!(output.contains("API_KEY"));
    assert!(output.contains("api.stripe.com"));
    assert!(output.contains("ffmpeg"));
    assert!(output.contains("./config/app.json"));
    assert!(output.contains(":3000"));
}

#[test]
fn terminal_shows_locations() {
    let output = format_terminal(&sample_deps(), 10, 5);

    assert!(output.contains("src/db.ts:12"), "should show file:line for DATABASE_URL");
    assert!(output.contains("src/pay.ts:8"), "should show file:line for api.stripe.com");
}

#[test]
fn terminal_shows_category_counts() {
    let output = format_terminal(&sample_deps(), 10, 5);

    // EnvVar has 2 deps
    assert!(output.contains("2 found"), "should show count for env vars category");
}

#[test]
fn terminal_summary_line() {
    let output = format_terminal(&sample_deps(), 10, 5);

    assert!(output.contains("Summary:"), "should have summary label");
    assert!(output.contains("7 shadow deps"), "should count all deps");
    assert!(output.contains("6 categories"), "should count unique categories");
}

#[test]
fn terminal_singular_category() {
    let deps = vec![ShadowDep::new(
        Category::EnvVar,
        "DB_URL",
        Confidence::High,
        Location::new("a.ts", 1, 1),
    )];

    let output = format_terminal(&deps, 1, 1);
    assert!(output.contains("1 category"), "should use singular 'category'");
}

#[test]
fn terminal_empty_deps() {
    let output = format_terminal(&[], 10, 5);

    assert!(output.contains("No shadow dependencies found"), "should show empty message");
    assert!(!output.contains("Summary:"), "should not show summary for empty");
}

#[test]
fn terminal_confidence_strings_present() {
    let output = format_terminal(&sample_deps(), 10, 5);

    // The confidence values are colored but the underlying text should be present.
    // colored crate wraps text in ANSI escape codes, but the word itself is still in the string.
    assert!(output.contains("high"), "should contain 'high' confidence");
    assert!(output.contains("medium"), "should contain 'medium' confidence");
    assert!(output.contains("low"), "should contain 'low' confidence");
}

// --- format_summary ---

#[test]
fn summary_one_liner() {
    let output = format_summary(&sample_deps(), 42, 250);

    assert!(output.contains("looper v"), "should include tool name and version");
    assert!(output.contains("42 files"), "should include file count");
    assert!(output.contains("250ms"), "should include duration");
    assert!(output.contains("7 shadow deps"), "should include dep count");
    assert!(output.contains("6 categories"), "should include category count");
}

#[test]
fn summary_ends_with_newline() {
    let output = format_summary(&sample_deps(), 10, 5);
    assert!(output.ends_with('\n'), "summary should end with newline");
}

#[test]
fn summary_empty_deps() {
    let output = format_summary(&[], 10, 5);

    assert!(output.contains("0 shadow deps"), "should show zero deps");
    assert!(output.contains("0 categories"), "should show zero categories");
}

#[test]
fn summary_single_category() {
    let deps = vec![
        ShadowDep::new(Category::EnvVar, "A", Confidence::High, Location::new("a.ts", 1, 1)),
        ShadowDep::new(Category::EnvVar, "B", Confidence::High, Location::new("a.ts", 2, 1)),
    ];

    let output = format_summary(&deps, 5, 10);
    assert!(output.contains("2 shadow deps"), "should count both deps");
    assert!(output.contains("1 categories"), "should show 1 category");
}

// --- Multi-location display ---

#[test]
fn terminal_shows_more_count_for_multi_location() {
    let mut dep = ShadowDep::new(
        Category::EnvVar,
        "DB_URL",
        Confidence::High,
        Location::new("src/db.ts", 5, 1),
    );
    dep.locations.push(Location::new("src/config.ts", 10, 1));
    dep.locations.push(Location::new("src/server.ts", 20, 1));

    let output = format_terminal(&[dep], 3, 10);
    assert!(
        output.contains("(+2 more)"),
        "should show (+2 more) for 3 locations"
    );
    assert!(
        output.contains("src/db.ts:5"),
        "should show first location"
    );
}

#[test]
fn terminal_no_more_suffix_for_single_location() {
    let dep = ShadowDep::new(
        Category::EnvVar,
        "API_KEY",
        Confidence::High,
        Location::new("src/api.ts", 3, 1),
    );

    let output = format_terminal(&[dep], 1, 5);
    assert!(!output.contains("more"), "should not show (+N more) for single location");
    assert!(output.contains("src/api.ts:3"), "should show the location");
}
