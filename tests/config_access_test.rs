use looper::detectors::config_access::ConfigAccessDetector;
use looper::detectors::{DetectorContext, DetectorRegistry};
use looper::resolver::resolve_bindings;
use looper::types::{Category, Confidence, ShadowDep};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn detect_config(source: &str) -> Vec<ShadowDep> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::mjs()).parse();
    let program = allocator.alloc(ret.program);
    let bindings = resolve_bindings(program);
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(ConfigAccessDetector));
    let ctx = DetectorContext {
        program,
        source_text: source,
        file_path: "test.ts",
        bindings: &bindings,
    };
    registry.run_all(&ctx)
}

fn names(deps: &[ShadowDep]) -> Vec<&str> {
    deps.iter().map(|d| d.name.as_str()).collect()
}

// --- node-config (Direct) ---

#[test]
fn node_config_get() {
    let deps = detect_config(
        r#"
        import config from 'config';
        config.get('db.host');
    "#,
    );
    assert_eq!(names(&deps), vec!["db.host"]);
    assert_eq!(deps[0].confidence, Confidence::Medium);
    assert_eq!(
        deps[0].metadata.get_str("source_pattern"),
        Some("config_library")
    );
    assert_eq!(deps[0].metadata.get_bool("config_key"), Some(true));
}

#[test]
fn node_config_has() {
    let deps = detect_config(
        r#"
        import config from 'config';
        config.has('feature.enabled');
    "#,
    );
    assert_eq!(names(&deps), vec!["feature.enabled"]);
}

#[test]
fn node_config_require() {
    let deps = detect_config(
        r#"
        const config = require('config');
        config.get('redis.url');
    "#,
    );
    assert_eq!(names(&deps), vec!["redis.url"]);
}

#[test]
fn node_config_chained_require() {
    let deps = detect_config(r#"require('config').get('db.port');"#);
    assert_eq!(names(&deps), vec!["db.port"]);
}

// --- env-var (Direct, is_env_var) ---

#[test]
fn env_var_library_get() {
    let deps = detect_config(
        r#"
        import env from 'env-var';
        env.get('PORT');
    "#,
    );
    assert_eq!(names(&deps), vec!["PORT"]);
    assert_eq!(deps[0].confidence, Confidence::High);
    assert_eq!(
        deps[0].metadata.get_str("source_pattern"),
        Some("env_var_library")
    );
    assert!(deps[0].metadata.get_bool("config_key").is_none());
}

#[test]
fn env_var_library_require() {
    let deps = detect_config(
        r#"
        const env = require('env-var');
        env.get('DATABASE_URL');
    "#,
    );
    assert_eq!(names(&deps), vec!["DATABASE_URL"]);
    assert_eq!(deps[0].confidence, Confidence::High);
}

// --- NestJS ConfigService (Contextual) ---

#[test]
fn nestjs_config_service_get() {
    let deps = detect_config(
        r#"
        import { ConfigService } from '@nestjs/config';
        this.configService.get('PORT');
    "#,
    );
    assert_eq!(names(&deps), vec!["PORT"]);
    assert_eq!(deps[0].confidence, Confidence::Medium);
    assert_eq!(
        deps[0].metadata.get_str("source_pattern"),
        Some("nestjs_config")
    );
    assert_eq!(deps[0].metadata.get_bool("config_key"), Some(true));
}

#[test]
fn nestjs_config_service_get_or_throw() {
    let deps = detect_config(
        r#"
        import { ConfigService } from '@nestjs/config';
        this.configService.getOrThrow('SECRET_KEY');
    "#,
    );
    assert_eq!(names(&deps), vec!["SECRET_KEY"]);
}

#[test]
fn nestjs_identifier_config_service() {
    let deps = detect_config(
        r#"
        import { ConfigService } from '@nestjs/config';
        configService.get('API_KEY');
    "#,
    );
    assert_eq!(names(&deps), vec!["API_KEY"]);
}

#[test]
fn nestjs_without_import_not_detected() {
    let deps = detect_config(r#"this.configService.get('PORT');"#);
    assert!(deps.is_empty());
}

// --- Negative cases ---

#[test]
fn unbound_get_not_detected() {
    let deps = detect_config(r#"someLib.get('key');"#);
    assert!(deps.is_empty());
}

#[test]
fn wrong_method_not_detected() {
    let deps = detect_config(
        r#"
        import config from 'config';
        config.set('key', 'value');
    "#,
    );
    assert!(deps.is_empty());
}

#[test]
fn get_without_string_arg_not_detected() {
    let deps = detect_config(
        r#"
        import config from 'config';
        config.get(dynamicKey);
    "#,
    );
    assert!(deps.is_empty());
}

#[test]
fn empty_source_no_findings() {
    let deps = detect_config("");
    assert!(deps.is_empty());
}

#[test]
fn non_config_receiver_with_nestjs_import() {
    let deps = detect_config(
        r#"
        import { ConfigService } from '@nestjs/config';
        someMap.get('key');
    "#,
    );
    assert!(deps.is_empty());
}

#[test]
fn category_is_env_var() {
    let deps = detect_config(
        r#"
        import config from 'config';
        config.get('db.host');
    "#,
    );
    assert_eq!(deps[0].category, Category::EnvVar);
}

#[test]
fn id_format() {
    let deps = detect_config(
        r#"
        import config from 'config';
        config.get('db.host');
    "#,
    );
    assert_eq!(deps[0].id, "env_var::db.host");
}
