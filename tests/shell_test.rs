use looper::detectors::shell::ShellDetector;
use looper::detectors::{DetectorContext, DetectorRegistry};
use looper::resolver::resolve_bindings;
use looper::types::{Category, Confidence, ShadowDep};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn detect_shell(source: &str) -> Vec<ShadowDep> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::mjs()).parse();
    let program = allocator.alloc(ret.program);
    let bindings = resolve_bindings(program);
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(ShellDetector));
    let ctx = DetectorContext {
        program,
        source_text: source,
        file_path: "test.ts",
        bindings: &bindings,
    };
    registry.run_all(&ctx)
}

fn detect_shell_cjs(source: &str) -> Vec<ShadowDep> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::cjs()).parse();
    let program = allocator.alloc(ret.program);
    let bindings = resolve_bindings(program);
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(ShellDetector));
    let ctx = DetectorContext {
        program,
        source_text: source,
        file_path: "test.cjs",
        bindings: &bindings,
    };
    registry.run_all(&ctx)
}

fn names(deps: &[ShadowDep]) -> Vec<&str> {
    deps.iter().map(|d| d.name.as_str()).collect()
}

// --- exec / execSync ---

#[test]
fn exec_detects_binary() {
    let deps = detect_shell(r#"
        import { exec } from 'child_process';
        exec("ffmpeg -i input.mp4 output.webm");
    "#);
    assert_eq!(names(&deps), vec!["ffmpeg"]);
    assert_eq!(deps[0].confidence, Confidence::High);
    assert_eq!(deps[0].metadata.get_str("args_pattern"), Some("-i input.mp4 output.webm"));
}

#[test]
fn exec_sync_detects_binary() {
    let deps = detect_shell(r#"
        import { execSync } from 'child_process';
        execSync("git rev-parse HEAD");
    "#);
    assert_eq!(names(&deps), vec!["git"]);
    assert_eq!(deps[0].metadata.get_str("args_pattern"), Some("rev-parse HEAD"));
}

#[test]
fn exec_via_namespace_import() {
    let deps = detect_shell(r#"
        import * as cp from 'child_process';
        cp.exec("docker build .");
    "#);
    assert_eq!(names(&deps), vec!["docker"]);
    assert_eq!(deps[0].metadata.get_str("args_pattern"), Some("build ."));
}

#[test]
fn exec_via_cjs_require() {
    let deps = detect_shell_cjs(r#"
        const { exec } = require('child_process');
        exec("npm install");
    "#);
    assert_eq!(names(&deps), vec!["npm"]);
}

#[test]
fn exec_via_cjs_namespace() {
    let deps = detect_shell_cjs(r#"
        const cp = require('child_process');
        cp.execSync("ls -la");
    "#);
    assert_eq!(names(&deps), vec!["ls"]);
}

// --- spawn / spawnSync ---

#[test]
fn spawn_detects_binary() {
    let deps = detect_shell(r#"
        import { spawn } from 'child_process';
        spawn("convert", ["-resize", "100x100"]);
    "#);
    assert_eq!(names(&deps), vec!["convert"]);
    assert_eq!(deps[0].metadata.get_str("args_pattern"), Some("-resize 100x100"));
}

#[test]
fn spawn_sync() {
    let deps = detect_shell(r#"
        import { spawnSync } from 'child_process';
        spawnSync("python3", ["script.py", "--verbose"]);
    "#);
    assert_eq!(names(&deps), vec!["python3"]);
    assert_eq!(deps[0].metadata.get_str("args_pattern"), Some("script.py --verbose"));
}

#[test]
fn spawn_without_args_array() {
    let deps = detect_shell(r#"
        import { spawn } from 'child_process';
        spawn("node");
    "#);
    assert_eq!(names(&deps), vec!["node"]);
    assert!(deps[0].metadata.get_str("args_pattern").is_none());
}

// --- execFile ---

#[test]
fn exec_file() {
    let deps = detect_shell(r#"
        import { execFile } from 'child_process';
        execFile("ffprobe", ["-v", "quiet", "input.mp4"]);
    "#);
    assert_eq!(names(&deps), vec!["ffprobe"]);
}

// --- execa ---

#[test]
fn execa_detects_binary() {
    let deps = detect_shell(r#"
        import { execa } from 'execa';
        execa("docker", ["build", "."]);
    "#);
    assert_eq!(names(&deps), vec!["docker"]);
    assert_eq!(deps[0].metadata.get_str("args_pattern"), Some("build ."));
}

#[test]
fn execa_default_import() {
    let deps = detect_shell(r#"
        import execa from 'execa';
        execa("git", ["status"]);
    "#);
    assert_eq!(names(&deps), vec!["git"]);
}

// --- zx tagged template ---

#[test]
fn zx_tagged_template() {
    let deps = detect_shell(r#"
        const result = $`grep -r "TODO" src/`;
    "#);
    assert_eq!(names(&deps), vec!["grep"]);
    assert_eq!(deps[0].confidence, Confidence::High);
}

#[test]
fn zx_tagged_template_with_interpolation() {
    let deps = detect_shell(r#"
        const result = $`docker build -t ${tag} .`;
    "#);
    assert_eq!(names(&deps), vec!["docker"]);
    assert_eq!(deps[0].confidence, Confidence::Medium);
}

// --- node: prefix ---

#[test]
fn node_prefix_child_process() {
    let deps = detect_shell(r#"
        import { exec } from 'node:child_process';
        exec("curl https://example.com");
    "#);
    assert_eq!(names(&deps), vec!["curl"]);
}

// --- Edge cases ---

#[test]
fn no_findings_without_import() {
    let deps = detect_shell(r#"
        const exec = (cmd) => console.log(cmd);
        exec("not-a-binary");
    "#);
    assert!(deps.is_empty());
}

#[test]
fn dynamic_command_ignored() {
    let deps = detect_shell(r#"
        import { exec } from 'child_process';
        exec(dynamicCommand);
    "#);
    assert!(deps.is_empty());
}

#[test]
fn empty_source() {
    let deps = detect_shell("");
    assert!(deps.is_empty());
}

#[test]
fn category_is_binary() {
    let deps = detect_shell(r#"
        import { exec } from 'child_process';
        exec("ls");
    "#);
    assert_eq!(deps[0].category, Category::Binary);
}

#[test]
fn id_format() {
    let deps = detect_shell(r#"
        import { exec } from 'child_process';
        exec("ffmpeg -i x");
    "#);
    assert_eq!(deps[0].id, "binary::ffmpeg");
}

#[test]
fn multiple_binaries() {
    let deps = detect_shell(r#"
        import { exec, spawn } from 'child_process';
        exec("git status");
        spawn("docker", ["ps"]);
        exec("npm run build");
    "#);
    assert_eq!(deps.len(), 3);
    let n = names(&deps);
    assert!(n.contains(&"git"));
    assert!(n.contains(&"docker"));
    assert!(n.contains(&"npm"));
}
