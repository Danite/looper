pub mod config_access;
pub mod env_vars;
pub mod filesystem;
pub mod network;
pub mod platform;
pub mod ports;
pub mod shell;

use crate::resolver::BindingMap;
use crate::types::ShadowDep;
use oxc_ast::ast::{Argument, Expression, Program, TemplateLiteral};

pub struct StringOrTemplate {
    pub value: String,
    pub has_expressions: bool,
}

pub fn extract_string_or_template_arg(arg: &Argument<'_>) -> Option<StringOrTemplate> {
    match arg {
        Argument::StringLiteral(lit) => Some(StringOrTemplate {
            value: lit.value.to_string(),
            has_expressions: false,
        }),
        _ => {
            let expr = arg.as_expression()?;
            extract_string_or_template_expr(expr)
        }
    }
}

pub fn extract_string_or_template_expr(expr: &Expression<'_>) -> Option<StringOrTemplate> {
    match expr {
        Expression::StringLiteral(lit) => Some(StringOrTemplate {
            value: lit.value.to_string(),
            has_expressions: false,
        }),
        Expression::TemplateLiteral(tpl) => template_to_pattern(tpl),
        _ => None,
    }
}

fn template_to_pattern(tpl: &TemplateLiteral<'_>) -> Option<StringOrTemplate> {
    let quasis: Vec<&str> = tpl.quasis.iter().map(|q| q.value.raw.as_str()).collect();
    let joined = quasis.join("*");
    if joined.is_empty() || joined == "*" {
        return None;
    }
    Some(StringOrTemplate {
        value: joined,
        has_expressions: !tpl.expressions.is_empty(),
    })
}

/// Check if an expression is `require("module_name")`.
pub(crate) fn is_require_of(expr: &Expression<'_>, module_name: &str) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::Identifier(id) = &call.callee else {
        return false;
    };
    if id.name != "require" {
        return false;
    }
    matches!(call.arguments.first(), Some(Argument::StringLiteral(lit)) if lit.value.as_str() == module_name)
}

/// Check if an identifier expression is bound to the given module via bindings.
pub(crate) fn is_bound_to_module(
    expr: &Expression<'_>,
    bindings: &BindingMap,
    module_name: &str,
) -> bool {
    let Expression::Identifier(id) = expr else {
        return false;
    };
    bindings
        .get(id.name.as_str())
        .is_some_and(|b| b.module_source == module_name)
}

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
    registry.register(Box::new(filesystem::FilesystemDetector));
    registry.register(Box::new(network::NetworkDetector));
    registry.register(Box::new(shell::ShellDetector));
    registry.register(Box::new(ports::PortsDetector));
    registry.register(Box::new(platform::PlatformDetector));
    registry.register(Box::new(config_access::ConfigAccessDetector));
    registry
}
