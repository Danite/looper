use looper::detectors::{default_registry, Detector, DetectorContext, DetectorRegistry};
use looper::resolver::BindingMap;
use looper::types::*;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn make_context<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    file_path: &'a str,
    bindings: &'a BindingMap,
) -> DetectorContext<'a> {
    let parser_return = Parser::new(allocator, source, SourceType::ts()).parse();
    // Leak the program into the allocator's lifetime so DetectorContext can hold it
    let program = allocator.alloc(parser_return.program);
    DetectorContext {
        program,
        source_text: source,
        file_path,
        bindings,
    }
}

struct MockDetector {
    findings: Vec<ShadowDep>,
}

impl MockDetector {
    fn new(findings: Vec<ShadowDep>) -> Self {
        Self { findings }
    }
}

impl Detector for MockDetector {
    fn name(&self) -> &str {
        "mock"
    }

    fn detect(&self, _ctx: &DetectorContext<'_>) -> Vec<ShadowDep> {
        self.findings.clone()
    }
}

#[test]
fn registry_starts_empty() {
    let registry = DetectorRegistry::new();
    assert_eq!(registry.detector_count(), 0);
}

#[test]
fn registry_register_adds_detector() {
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(MockDetector::new(vec![])));
    assert_eq!(registry.detector_count(), 1);
}

#[test]
fn registry_run_all_collects_findings() {
    let mut registry = DetectorRegistry::new();

    registry.register(Box::new(MockDetector::new(vec![ShadowDep::new(
        Category::EnvVar,
        "DATABASE_URL",
        Confidence::High,
        Location::new("test.ts", 1, 1),
    )])));

    registry.register(Box::new(MockDetector::new(vec![ShadowDep::new(
        Category::Network,
        "api.stripe.com",
        Confidence::High,
        Location::new("test.ts", 5, 1),
    )])));

    let allocator = Allocator::new();
    let bindings = BindingMap::new();
    let ctx = make_context(&allocator, "const x = 1;", "test.ts", &bindings);

    let results = registry.run_all(&ctx);

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].category, Category::EnvVar);
    assert_eq!(results[1].category, Category::Network);
}

#[test]
fn registry_run_all_empty_when_no_detectors() {
    let registry = DetectorRegistry::new();
    let allocator = Allocator::new();
    let bindings = BindingMap::new();
    let ctx = make_context(&allocator, "const x = 1;", "test.ts", &bindings);

    let results = registry.run_all(&ctx);
    assert!(results.is_empty());
}

#[test]
fn registry_detector_names() {
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(MockDetector::new(vec![])));
    assert_eq!(registry.detector_names(), vec!["mock"]);
}

#[test]
fn default_registry_has_env_vars() {
    let registry = default_registry();
    assert!(registry.detector_count() >= 1);
    assert!(registry.detector_names().contains(&"env_vars"));
}

#[test]
fn detector_context_holds_file_metadata() {
    let allocator = Allocator::new();
    let bindings = BindingMap::new();
    let ctx = make_context(
        &allocator,
        "const x = process.env.FOO;",
        "src/config.ts",
        &bindings,
    );

    assert_eq!(ctx.file_path, "src/config.ts");
    assert_eq!(ctx.source_text, "const x = process.env.FOO;");
    assert!(ctx.bindings.is_empty());
}

#[test]
fn detector_trait_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Box<dyn Detector>>();
}

#[test]
fn multiple_detectors_same_category() {
    let mut registry = DetectorRegistry::new();

    registry.register(Box::new(MockDetector::new(vec![
        ShadowDep::new(
            Category::EnvVar,
            "FOO",
            Confidence::High,
            Location::new("a.ts", 1, 1),
        ),
        ShadowDep::new(
            Category::EnvVar,
            "BAR",
            Confidence::Medium,
            Location::new("a.ts", 2, 1),
        ),
    ])));

    let allocator = Allocator::new();
    let bindings = BindingMap::new();
    let ctx = make_context(&allocator, "const x = 1;", "a.ts", &bindings);

    let results = registry.run_all(&ctx);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].name, "FOO");
    assert_eq!(results[1].name, "BAR");
}
