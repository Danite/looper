pub mod env_vars;

use crate::resolver::BindingMap;
use crate::types::ShadowDep;
use oxc_ast::ast::Program;

/// Context passed to each detector for a single file.
pub struct DetectorContext<'a> {
    pub program: &'a Program<'a>,
    pub source_text: &'a str,
    pub file_path: &'a str,
    pub bindings: &'a BindingMap,
}

/// Trait that all shadow dependency detectors implement.
///
/// The output is language-agnostic (`Vec<ShadowDep>`) so future language
/// backends (Python, Go) can implement the same trait with different AST types.
pub trait Detector: Send + Sync {
    fn name(&self) -> &str;
    fn detect(&self, ctx: &DetectorContext<'_>) -> Vec<ShadowDep>;
}

/// Registry that holds all enabled detectors and dispatches them.
pub struct DetectorRegistry {
    detectors: Vec<Box<dyn Detector>>,
}

impl DetectorRegistry {
    pub fn new() -> Self {
        Self {
            detectors: Vec::new(),
        }
    }

    pub fn register(&mut self, detector: Box<dyn Detector>) {
        self.detectors.push(detector);
    }

    /// Run all registered detectors on a file context, collecting results.
    pub fn run_all(&self, ctx: &DetectorContext<'_>) -> Vec<ShadowDep> {
        let mut results = Vec::new();
        for detector in &self.detectors {
            results.extend(detector.detect(ctx));
        }
        results
    }

    pub fn detector_count(&self) -> usize {
        self.detectors.len()
    }

    pub fn detector_names(&self) -> Vec<&str> {
        self.detectors.iter().map(|d| d.name()).collect()
    }
}

impl Default for DetectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a registry with all built-in detectors.
pub fn default_registry() -> DetectorRegistry {
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(env_vars::EnvVarsDetector));
    registry
}
