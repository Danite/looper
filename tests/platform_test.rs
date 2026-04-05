use looper::detectors::platform::PlatformDetector;
use looper::detectors::{DetectorContext, DetectorRegistry};
use looper::resolver::resolve_bindings;
use looper::types::{Category, Confidence, ShadowDep};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn detect_platform(source: &str) -> Vec<ShadowDep> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::mjs()).parse();
    let program = allocator.alloc(ret.program);
    let bindings = resolve_bindings(program);
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(PlatformDetector));
    let ctx = DetectorContext {
        program,
        source_text: source,
        file_path: "test.ts",
        bindings: &bindings,
    };
    registry.run_all(&ctx)
}

// --- Unix-only paths ---

#[test]
fn unix_dev_null() {
    let deps = detect_platform(r#"fs.openSync("/dev/null");"#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].name, "/dev/null");
    assert_eq!(deps[0].metadata.get_str("assumed_platform"), Some("unix"));
    assert_eq!(deps[0].confidence, Confidence::High);
}

#[test]
fn unix_etc_path() {
    let deps = detect_platform(r#"fs.readFileSync("/etc/ssl/certs/ca.pem");"#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.starts_with("/etc/"));
}

#[test]
fn unix_tmp_path() {
    let deps = detect_platform(r#"fs.writeFileSync("/tmp/cache.json", data);"#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.starts_with("/tmp/"));
}

#[test]
fn unix_usr_path() {
    let deps = detect_platform(r#"exec("/usr/bin/env node");"#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.starts_with("/usr/"));
}

// --- Windows-only commands ---

#[test]
fn windows_cmd() {
    let deps = detect_platform(r#"exec("cmd /c dir");"#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].metadata.get_str("assumed_platform"), Some("windows"));
    assert_eq!(deps[0].confidence, Confidence::High);
}

#[test]
fn windows_powershell() {
    let deps = detect_platform(r#"exec("powershell Get-Process");"#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].metadata.get_str("assumed_platform"), Some("windows"));
}

// --- path.sep checks ---

#[test]
fn path_sep_unix() {
    let deps = detect_platform(r#"if (path.sep === "/") {}"#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("path.sep"));
    assert_eq!(deps[0].metadata.get_str("assumed_platform"), Some("unix"));
}

#[test]
fn path_sep_windows() {
    let deps = detect_platform(r#"if (path.sep === "\\") {}"#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].metadata.get_str("assumed_platform"), Some("windows"));
}

// --- process.platform checks ---

#[test]
fn process_platform_darwin() {
    let deps = detect_platform(r#"if (process.platform === "darwin") {}"#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("darwin"));
    assert_eq!(deps[0].metadata.get_str("assumed_platform"), Some("unix"));
    assert_eq!(deps[0].metadata.get_bool("guarded"), Some(true));
}

#[test]
fn process_platform_linux() {
    let deps = detect_platform(r#"if (process.platform === "linux") {}"#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("linux"));
    assert_eq!(deps[0].metadata.get_str("assumed_platform"), Some("unix"));
    assert_eq!(deps[0].metadata.get_bool("guarded"), Some(true));
}

#[test]
fn process_platform_win32() {
    let deps = detect_platform(r#"if (process.platform === "win32") {}"#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("win32"));
    assert_eq!(deps[0].metadata.get_str("assumed_platform"), Some("windows"));
    assert_eq!(deps[0].metadata.get_bool("guarded"), Some(true));
}

#[test]
fn process_platform_strict_inequality() {
    let deps = detect_platform(r#"if (process.platform !== "win32") {}"#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].metadata.get_bool("guarded"), Some(true));
}

// --- os module calls ---

#[test]
fn os_homedir() {
    let deps = detect_platform(r#"
        import os from 'os';
        const home = os.homedir();
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].name, "os.homedir()");
    assert_eq!(deps[0].confidence, Confidence::Medium);
    assert_eq!(deps[0].metadata.get_str("assumed_platform"), Some("any"));
    assert_eq!(deps[0].metadata.get_bool("guarded"), Some(false));
}

#[test]
fn os_tmpdir() {
    let deps = detect_platform(r#"
        import os from 'os';
        const tmp = os.tmpdir();
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].name, "os.tmpdir()");
}

#[test]
fn os_hostname() {
    let deps = detect_platform(r#"
        import os from 'os';
        const host = os.hostname();
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].name, "os.hostname()");
}

#[test]
fn os_with_node_prefix() {
    let deps = detect_platform(r#"
        import os from 'node:os';
        os.homedir();
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].name, "os.homedir()");
}

#[test]
fn os_named_import() {
    let deps = detect_platform(r#"
        import { homedir } from 'os';
        const h = homedir();
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].name, "os.homedir()");
}

#[test]
fn os_without_binding_ignored() {
    let deps = detect_platform(r#"
        const os = { homedir: () => "/fake" };
        os.homedir();
    "#);
    assert!(deps.is_empty());
}

// --- Edge cases ---

#[test]
fn empty_source() {
    let deps = detect_platform("");
    assert!(deps.is_empty());
}

#[test]
fn category_is_platform() {
    let deps = detect_platform(r#"if (process.platform === "darwin") {}"#);
    assert_eq!(deps[0].category, Category::Platform);
}

#[test]
fn regular_path_not_detected() {
    let deps = detect_platform(r#"fs.readFileSync("./config.json");"#);
    assert!(deps.is_empty());
}

#[test]
fn multiple_platform_deps() {
    let deps = detect_platform(r#"
        import os from 'os';
        fs.openSync("/dev/null");
        if (process.platform === "darwin") {}
        os.homedir();
    "#);
    assert_eq!(deps.len(), 3);
}

#[test]
fn unix_path_not_guarded() {
    let deps = detect_platform(r#"fs.openSync("/dev/null");"#);
    assert_eq!(deps[0].metadata.get_bool("guarded"), Some(false));
}

#[test]
fn os_platform_method() {
    let deps = detect_platform(r#"
        import os from 'os';
        const p = os.platform();
    "#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("os.platform()"));
}

#[test]
fn os_type_method() {
    let deps = detect_platform(r#"
        import os from 'os';
        os.type();
    "#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("os.type()"));
}

#[test]
fn os_arch_method() {
    let deps = detect_platform(r#"
        import os from 'os';
        os.arch();
    "#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("os.arch()"));
}

#[test]
fn process_arch_check() {
    let deps = detect_platform(r#"
        if (process.arch === "arm64") { }
    "#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("process.arch"));
    assert!(deps[0].name.contains("arm64"));
    assert_eq!(deps[0].metadata.get_bool("guarded"), Some(true));
}

#[test]
fn process_arch_x64() {
    let deps = detect_platform(r#"
        const isX64 = process.arch === "x64";
    "#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("x64"));
}
