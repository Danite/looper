use looper::detectors::{DetectorContext, DetectorRegistry};
use looper::detectors::network::NetworkDetector;
use looper::resolver::{resolve_bindings, BindingMap};
use looper::types::{Category, Confidence, ShadowDep};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn detect_net(source: &str) -> Vec<ShadowDep> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::mjs()).parse();
    let program = allocator.alloc(ret.program);
    let bindings = resolve_bindings(program);
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(NetworkDetector));
    let ctx = DetectorContext {
        program,
        source_text: source,
        file_path: "test.ts",
        bindings: &bindings,
    };
    registry.run_all(&ctx)
}

fn detect_net_jsx(source: &str) -> Vec<ShadowDep> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::tsx()).parse();
    let program = allocator.alloc(ret.program);
    let bindings = BindingMap::new();
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(NetworkDetector));
    let ctx = DetectorContext {
        program,
        source_text: source,
        file_path: "test.tsx",
        bindings: &bindings,
    };
    registry.run_all(&ctx)
}

fn names(deps: &[ShadowDep]) -> Vec<&str> {
    deps.iter().map(|d| d.name.as_str()).collect()
}

// --- fetch ---

#[test]
fn fetch_with_url() {
    let deps = detect_net(r#"fetch("https://api.stripe.com/v1/charges");"#);
    assert_eq!(names(&deps), vec!["api.stripe.com"]);
    assert_eq!(deps[0].confidence, Confidence::High);
    assert_eq!(deps[0].metadata.get_str("protocol"), Some("https"));
    assert_eq!(deps[0].metadata.get_str("path_pattern"), Some("/v1/charges"));
}

#[test]
fn fetch_with_http() {
    let deps = detect_net(r#"fetch("http://internal.service:8080/health");"#);
    assert_eq!(names(&deps), vec!["internal.service:8080"]);
    assert_eq!(deps[0].metadata.get_str("protocol"), Some("http"));
}

// --- axios ---

#[test]
fn axios_get() {
    let deps = detect_net(r#"axios.get("https://auth.service.internal/token");"#);
    assert_eq!(names(&deps), vec!["auth.service.internal"]);
    assert_eq!(deps[0].metadata.get_str("protocol"), Some("https"));
}

#[test]
fn axios_post() {
    let deps = detect_net(r#"axios.post("https://api.example.com/data", payload);"#);
    assert_eq!(names(&deps), vec!["api.example.com"]);
}

#[test]
fn axios_direct_call() {
    let deps = detect_net(r#"axios("https://api.example.com/items");"#);
    assert_eq!(names(&deps), vec!["api.example.com"]);
}

// --- http.request ---

#[test]
fn http_request_with_hostname() {
    let deps = detect_net(r#"
        import http from 'http';
        http.request({ hostname: "db.example.com", port: 3000 });
    "#);
    assert_eq!(names(&deps), vec!["db.example.com"]);
    assert_eq!(deps[0].metadata.get_str("protocol"), Some("http"));
}

#[test]
fn http_request_with_host() {
    let deps = detect_net(r#"
        import http from 'http';
        http.request({ host: "api.internal" });
    "#);
    assert_eq!(names(&deps), vec!["api.internal"]);
}

#[test]
fn http_request_without_binding_ignored() {
    let deps = detect_net(r#"
        const http = { request: () => {} };
        http.request({ hostname: "not-real.com" });
    "#);
    assert!(deps.is_empty());
}

#[test]
fn http_request_string_url() {
    let deps = detect_net(r#"
        import http from 'http';
        http.request("http://api.internal.service/health");
    "#);
    assert_eq!(names(&deps), vec!["api.internal.service"]);
}

#[test]
fn https_get_string_url() {
    let deps = detect_net(r#"
        import https from 'https';
        https.get("https://registry.npmjs.org/looper");
    "#);
    assert_eq!(names(&deps), vec!["registry.npmjs.org"]);
}

// --- new URL ---

#[test]
fn new_url_https() {
    let deps = detect_net(r#"new URL("https://cdn.example.com/assets");"#);
    assert_eq!(names(&deps), vec!["cdn.example.com"]);
    assert_eq!(deps[0].metadata.get_str("protocol"), Some("https"));
}

#[test]
fn new_url_wss() {
    let deps = detect_net(r#"new URL("wss://realtime.example.com/socket");"#);
    assert_eq!(names(&deps), vec!["realtime.example.com"]);
    assert_eq!(deps[0].metadata.get_str("protocol"), Some("wss"));
}

// --- new WebSocket ---

#[test]
fn new_websocket() {
    let deps = detect_net(r#"new WebSocket("ws://localhost:8080/ws");"#);
    assert_eq!(names(&deps), vec!["localhost:8080"]);
    assert_eq!(deps[0].metadata.get_str("protocol"), Some("ws"));
}

#[test]
fn new_websocket_wss() {
    let deps = detect_net(r#"new WebSocket("wss://stream.example.com");"#);
    assert_eq!(names(&deps), vec!["stream.example.com"]);
    assert_eq!(deps[0].metadata.get_str("protocol"), Some("wss"));
}

// --- JSX attributes ---

#[test]
fn jsx_script_src() {
    let deps = detect_net_jsx(r#"const x = <script src="https://analytics.example.com/track.js" />;"#);
    assert_eq!(names(&deps), vec!["analytics.example.com"]);
}

#[test]
fn jsx_img_src() {
    let deps = detect_net_jsx(r#"const x = <img src="https://cdn.example.com/logo.png" />;"#);
    assert_eq!(names(&deps), vec!["cdn.example.com"]);
}

#[test]
fn jsx_relative_url_ignored() {
    let deps = detect_net_jsx(r#"const x = <img src="/images/logo.png" />;"#);
    assert!(deps.is_empty());
}

// --- Edge cases ---

#[test]
fn empty_source() {
    let deps = detect_net("");
    assert!(deps.is_empty());
}

#[test]
fn relative_url_ignored() {
    let deps = detect_net(r#"fetch("/api/users");"#);
    assert!(deps.is_empty());
}

#[test]
fn data_uri_ignored() {
    let deps = detect_net(r#"fetch("data:text/plain;base64,SGVsbG8=");"#);
    assert!(deps.is_empty());
}

#[test]
fn dynamic_url_ignored() {
    let deps = detect_net(r#"fetch(apiUrl);"#);
    assert!(deps.is_empty());
}

#[test]
fn category_is_network() {
    let deps = detect_net(r#"fetch("https://example.com");"#);
    assert_eq!(deps[0].category, Category::Network);
}

#[test]
fn id_format() {
    let deps = detect_net(r#"fetch("https://api.stripe.com/v1");"#);
    assert_eq!(deps[0].id, "network::api.stripe.com");
}

#[test]
fn multiple_endpoints() {
    let deps = detect_net(r#"
        fetch("https://api.stripe.com/v1");
        fetch("https://sentry.io/api/store");
        new WebSocket("wss://realtime.app.com");
    "#);
    assert_eq!(deps.len(), 3);
    let n = names(&deps);
    assert!(n.contains(&"api.stripe.com"));
    assert!(n.contains(&"sentry.io"));
    assert!(n.contains(&"realtime.app.com"));
}

#[test]
fn url_with_port() {
    let deps = detect_net(r#"fetch("https://api.example.com:9090/data");"#);
    assert_eq!(names(&deps), vec!["api.example.com:9090"]);
}

#[test]
fn localhost_detected() {
    let deps = detect_net(r#"fetch("http://localhost:3000/api");"#);
    assert_eq!(names(&deps), vec!["localhost:3000"]);
}

// --- Third-party HTTP clients ---

#[test]
fn got_get() {
    let deps = detect_net(r#"
        import got from 'got';
        got.get("https://api.example.com/data");
    "#);
    assert_eq!(names(&deps), vec!["api.example.com"]);
}

#[test]
fn got_direct_call() {
    let deps = detect_net(r#"
        import got from 'got';
        got("https://hooks.slack.com/trigger");
    "#);
    assert_eq!(names(&deps), vec!["hooks.slack.com"]);
}

#[test]
fn node_fetch_call() {
    let deps = detect_net(r#"
        import fetch from 'node-fetch';
        fetch("https://api.github.com/repos");
    "#);
    assert_eq!(names(&deps), vec!["api.github.com"]);
}

#[test]
fn ky_post() {
    let deps = detect_net(r#"
        import ky from 'ky';
        ky.post("https://analytics.example.com/events");
    "#);
    assert_eq!(names(&deps), vec!["analytics.example.com"]);
}

#[test]
fn superagent_get() {
    let deps = detect_net(r#"
        import superagent from 'superagent';
        superagent.get("https://api.weather.com/forecast");
    "#);
    assert_eq!(names(&deps), vec!["api.weather.com"]);
}

#[test]
fn undici_request() {
    let deps = detect_net(r#"
        import { request } from 'undici';
        request("https://cdn.example.com/assets");
    "#);
    assert_eq!(names(&deps), vec!["cdn.example.com"]);
}
