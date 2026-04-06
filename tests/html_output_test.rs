use looper::output::build_manifest;
use looper::output::html::format_html;
use looper::types::*;
use serde_json::json;

fn sample_deps() -> Vec<ShadowDep> {
    let mut meta = Metadata::new();
    meta.insert("required", json!(true));

    vec![
        ShadowDep::new(
            Category::EnvVar,
            "DATABASE_URL",
            Confidence::High,
            Location::new("src/db.ts", 12, 5),
        )
        .with_metadata(meta),
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
fn html_is_valid_document() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let html = format_html(&manifest).unwrap();

    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<html"));
    assert!(html.contains("</html>"));
    assert!(html.contains("<head>"));
    assert!(html.contains("<body>"));
}

#[test]
fn html_is_self_contained() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let html = format_html(&manifest).unwrap();

    assert!(html.contains("<style>"));
    assert!(html.contains("<script>"));
    assert!(!html.contains("stylesheet\" href="));
    assert!(!html.contains("script src="));
}

#[test]
fn html_contains_data() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let html = format_html(&manifest).unwrap();

    assert!(html.contains("DATABASE_URL"));
    assert!(html.contains("api.stripe.com"));
    assert!(html.contains("ffmpeg"));
    assert!(html.contains("src/db.ts"));
}

#[test]
fn html_has_filterable_table() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let html = format_html(&manifest).unwrap();

    assert!(html.contains("<table>"));
    assert!(html.contains("filterCat"));
    assert!(html.contains("filterConf"));
    assert!(html.contains("filterSearch"));
}

#[test]
fn html_has_summary_stats() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let html = format_html(&manifest).unwrap();

    assert!(html.contains("stat-value"));
    assert!(html.contains("stat-label"));
}

#[test]
fn html_has_click_to_copy() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let html = format_html(&manifest).unwrap();

    assert!(html.contains("navigator.clipboard"));
    assert!(html.contains("clickable"));
    assert!(html.contains("Copied!"));
}

#[test]
fn html_has_sortable_columns() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let html = format_html(&manifest).unwrap();

    assert!(html.contains("data-col=\"name\""));
    assert!(html.contains("data-col=\"category\""));
    assert!(html.contains("data-col=\"confidence\""));
    assert!(html.contains("data-col=\"file\""));
    assert!(html.contains("data-col=\"line\""));
}

#[test]
fn html_empty_deps() {
    let manifest = build_manifest(".", 0, 1, Vec::new());
    let html = format_html(&manifest).unwrap();

    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("\"deps\":[]"));
}

#[test]
fn html_escapes_script_closing_tag() {
    let dep = ShadowDep::new(
        Category::Filesystem,
        "</script><img src=x onerror=alert(1)>",
        Confidence::High,
        Location::new("src/evil.ts", 1, 1),
    );
    let manifest = build_manifest(".", 1, 1, vec![dep]);
    let html = format_html(&manifest).unwrap();

    assert!(
        !html.contains("</script><img"),
        "raw </script> must not appear in embedded JSON"
    );
    assert!(
        html.contains("<\\/script>"),
        "closing tags must be escaped in JSON data"
    );
}

#[test]
fn html_has_confidence_colors() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let html = format_html(&manifest).unwrap();

    assert!(html.contains("conf-high"));
    assert!(html.contains("conf-medium"));
    assert!(html.contains("conf-low"));
}
