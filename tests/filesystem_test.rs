use looper::detectors::{DetectorContext, DetectorRegistry};
use looper::detectors::filesystem::FilesystemDetector;
use looper::resolver::resolve_bindings;
use looper::types::{Category, Confidence, ShadowDep};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn detect_fs(source: &str) -> Vec<ShadowDep> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::mjs()).parse();
    let program = allocator.alloc(ret.program);
    let bindings = resolve_bindings(program);
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(FilesystemDetector));
    let ctx = DetectorContext {
        program,
        source_text: source,
        file_path: "test.ts",
        bindings: &bindings,
    };
    registry.run_all(&ctx)
}

fn detect_fs_cjs(source: &str) -> Vec<ShadowDep> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::cjs()).parse();
    let program = allocator.alloc(ret.program);
    let bindings = resolve_bindings(program);
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(FilesystemDetector));
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

// --- fs.readFileSync ---

#[test]
fn fs_read_file_sync_esm() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        const data = fs.readFileSync('./config.json');
    "#);
    assert_eq!(names(&deps), vec!["./config.json"]);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("read"));
    assert_eq!(deps[0].metadata.get_bool("is_absolute"), Some(false));
    assert_eq!(deps[0].confidence, Confidence::High);
}

#[test]
fn fs_read_file_sync_cjs() {
    let deps = detect_fs_cjs(r#"
        const fs = require('fs');
        const data = fs.readFileSync('/etc/ssl/certs/ca.pem');
    "#);
    assert_eq!(names(&deps), vec!["/etc/ssl/certs/ca.pem"]);
    assert_eq!(deps[0].metadata.get_bool("is_absolute"), Some(true));
}

// --- fs.promises.readFile ---

#[test]
fn fs_promises_read_file() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        const data = await fs.promises.readFile('./data.json');
    "#);
    assert_eq!(names(&deps), vec!["./data.json"]);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("read"));
}

// --- fs.writeFileSync ---

#[test]
fn fs_write_file_sync() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.writeFileSync('/tmp/cache.json', data);
    "#);
    assert_eq!(names(&deps), vec!["/tmp/cache.json"]);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("write"));
    assert_eq!(deps[0].metadata.get_bool("is_absolute"), Some(true));
}

// --- fs.existsSync ---

#[test]
fn fs_exists_sync() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        if (fs.existsSync('./config')) {}
    "#);
    assert_eq!(names(&deps), vec!["./config"]);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("exists"));
}

// --- fs.mkdirSync ---

#[test]
fn fs_mkdir_sync() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.mkdirSync('./uploads');
    "#);
    assert_eq!(names(&deps), vec!["./uploads"]);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("mkdir"));
}

// --- Named imports ---

#[test]
fn named_import_read_file_sync() {
    let deps = detect_fs(r#"
        import { readFileSync } from 'fs';
        const data = readFileSync('./settings.yaml');
    "#);
    assert_eq!(names(&deps), vec!["./settings.yaml"]);
}

// --- path.join ---

#[test]
fn path_join_with_literals() {
    let deps = detect_fs(r#"
        import path from 'path';
        const p = path.join('src', 'config', 'app.json');
    "#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("src"));
    assert!(deps[0].name.contains("app.json"));
    assert_eq!(deps[0].confidence, Confidence::High);
}

#[test]
fn path_join_with_dynamic_part() {
    let deps = detect_fs(r#"
        import path from 'path';
        const p = path.join('locales', lang, 'messages.json');
    "#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("locales"));
    assert!(deps[0].name.contains("*"));
    assert_eq!(deps[0].confidence, Confidence::Medium);
}

// --- path.resolve ---

#[test]
fn path_resolve() {
    let deps = detect_fs(r#"
        import path from 'path';
        const p = path.resolve('./data');
    "#);
    assert_eq!(deps.len(), 1);
    assert!(deps[0].name.contains("data"));
}

// --- Deno.readTextFile ---

#[test]
fn deno_read_text_file() {
    let deps = detect_fs(r#"
        const data = Deno.readTextFile('./config.json');
    "#);
    assert_eq!(names(&deps), vec!["./config.json"]);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("read"));
}

// --- Bun.file ---

#[test]
fn bun_file() {
    let deps = detect_fs(r#"
        const file = Bun.file('./data.json');
    "#);
    assert_eq!(names(&deps), vec!["./data.json"]);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("read"));
}

// --- node: prefix ---

#[test]
fn node_prefix_fs() {
    let deps = detect_fs(r#"
        import fs from 'node:fs';
        fs.readFileSync('./app.config');
    "#);
    assert_eq!(names(&deps), vec!["./app.config"]);
}

// --- Edge cases ---

#[test]
fn no_findings_without_fs_import() {
    let deps = detect_fs(r#"
        const fs = { readFileSync: () => {} };
        fs.readFileSync('./config.json');
    "#);
    assert!(deps.is_empty());
}

#[test]
fn no_findings_for_dynamic_path() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.readFileSync(dynamicVariable);
    "#);
    assert!(deps.is_empty());
}

#[test]
fn empty_source() {
    let deps = detect_fs("");
    assert!(deps.is_empty());
}

#[test]
fn category_is_filesystem() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.readFileSync('./test');
    "#);
    assert_eq!(deps[0].category, Category::Filesystem);
}

#[test]
fn id_format() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.readFileSync('./config.json');
    "#);
    assert_eq!(deps[0].id, "filesystem::./config.json");
}

#[test]
fn multiple_fs_calls() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.readFileSync('./a.json');
        fs.writeFileSync('./b.json', data);
        fs.existsSync('./c');
    "#);
    assert_eq!(deps.len(), 3);
    assert_eq!(names(&deps), vec!["./a.json", "./b.json", "./c"]);
}

#[test]
fn fs_extra_detected() {
    let deps = detect_fs(r#"
        import fs from 'fs-extra';
        fs.readFileSync('./config.json');
        fs.copySync('./src', './dist');
    "#);
    assert!(!deps.is_empty(), "fs-extra should be detected as filesystem module");
    assert!(deps.iter().any(|d| d.name == "./config.json"));
}

#[test]
fn graceful_fs_detected() {
    let deps = detect_fs(r#"
        import fs from 'graceful-fs';
        fs.readFileSync('./settings.yaml');
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].name, "./settings.yaml");
}

#[test]
fn copy_file_detected() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.copyFileSync('./src.txt', './dest.txt');
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].name, "./src.txt");
    assert_eq!(deps[0].metadata.get_str("operation"), Some("write"));
}

#[test]
fn rename_detected() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.renameSync('./old.json', './new.json');
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("write"));
}

#[test]
fn unlink_detected() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.unlinkSync('./temp.lock');
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].name, "./temp.lock");
    assert_eq!(deps[0].metadata.get_str("operation"), Some("delete"));
}

#[test]
fn rm_detected() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.rmSync('./cache');
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("delete"));
}

#[test]
fn watch_detected() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.watch('./src');
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].name, "./src");
    assert_eq!(deps[0].metadata.get_str("operation"), Some("watch"));
}

#[test]
fn chmod_detected() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.chmodSync('./script.sh', 0o755);
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("write"));
}

#[test]
fn realpath_detected() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.realpathSync('./link');
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("read"));
}

#[test]
fn lstat_detected() {
    let deps = detect_fs(r#"
        import fs from 'fs';
        fs.lstatSync('./maybe-symlink');
    "#);
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].metadata.get_str("operation"), Some("exists"));
}
