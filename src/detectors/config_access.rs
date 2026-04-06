use super::{
    extract_string_or_template_arg, is_bound_to_module, is_require_of, Detector, DetectorContext,
};
use crate::resolver::BindingMap;
use crate::types::{Category, Confidence, Location, Metadata, ShadowDep};
use oxc_ast::ast::{CallExpression, Expression, Statement};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use serde_json::json;
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub enum MatchStyle {
    /// The call receiver is the imported identifier itself (e.g. `config.get('key')`)
    Direct,
    /// The call receiver is a class instance; match by file-level import + receiver name
    /// heuristic (e.g. NestJS `this.configService.get('KEY')`)
    Contextual,
}

pub struct ConfigAccessRule {
    pub modules: &'static [&'static str],
    pub methods: &'static [&'static str],
    pub match_style: MatchStyle,
    pub confidence: Confidence,
    pub source_pattern: &'static str,
    pub is_env_var: bool,
}

pub fn default_config_rules() -> &'static [ConfigAccessRule] {
    &[
        ConfigAccessRule {
            modules: &["env-var"],
            methods: &["get"],
            match_style: MatchStyle::Direct,
            confidence: Confidence::High,
            source_pattern: "env_var_library",
            is_env_var: true,
        },
        ConfigAccessRule {
            modules: &["config"],
            methods: &["get", "has"],
            match_style: MatchStyle::Direct,
            confidence: Confidence::Medium,
            source_pattern: "config_library",
            is_env_var: false,
        },
        // Convict limitation: only matches when the identifier is directly bound
        // to the convict module (e.g. `import config from 'convict'`). The common
        // `const config = convict(schema)` pattern creates an instance that is NOT
        // a module binding — detecting that requires data-flow analysis.
        ConfigAccessRule {
            modules: &["convict"],
            methods: &["get"],
            match_style: MatchStyle::Direct,
            confidence: Confidence::Medium,
            source_pattern: "convict",
            is_env_var: false,
        },
        ConfigAccessRule {
            modules: &["@nestjs/config"],
            methods: &["get", "getOrThrow"],
            match_style: MatchStyle::Contextual,
            confidence: Confidence::Medium,
            source_pattern: "nestjs_config",
            is_env_var: false,
        },
    ]
}

pub struct ConfigAccessDetector;

impl Detector for ConfigAccessDetector {
    fn name(&self) -> &str {
        "config_access"
    }

    fn detect(&self, ctx: &DetectorContext<'_>) -> Vec<ShadowDep> {
        let rules = default_config_rules();

        let mut contextual_modules: HashSet<String> = HashSet::new();
        for rule in rules {
            if matches!(rule.match_style, MatchStyle::Contextual) {
                for stmt in &ctx.program.body {
                    if let Statement::ImportDeclaration(import) = stmt {
                        let src = import.source.value.as_str();
                        if rule.modules.contains(&src) {
                            contextual_modules.insert(src.to_string());
                        }
                    }
                }
            }
        }

        let mut collector = ConfigAccessCollector {
            file_path: ctx.file_path,
            source_text: ctx.source_text,
            bindings: ctx.bindings,
            rules,
            contextual_modules,
            findings: Vec::new(),
        };
        collector.visit_program(ctx.program);
        collector.findings
    }
}

struct ConfigAccessCollector<'a> {
    file_path: &'a str,
    source_text: &'a str,
    bindings: &'a BindingMap,
    rules: &'a [ConfigAccessRule],
    contextual_modules: HashSet<String>,
    findings: Vec<ShadowDep>,
}

impl ConfigAccessCollector<'_> {
    fn line_col(&self, offset: u32) -> (u32, u32) {
        crate::types::line_col(self.source_text, offset)
    }

    fn try_match_call(&mut self, call: &CallExpression<'_>) {
        let Expression::StaticMemberExpression(m) = &call.callee else {
            return;
        };
        let method = m.property.name.as_str();

        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(result) = extract_string_or_template_arg(first_arg) else {
            return;
        };

        for rule in self.rules {
            if !rule.methods.contains(&method) {
                continue;
            }

            let matched = match rule.match_style {
                MatchStyle::Direct => self.matches_direct(&m.object, rule),
                MatchStyle::Contextual => self.matches_contextual(&m.object, rule),
            };

            if matched {
                let (line, col) = self.line_col(call.span().start);
                let confidence = if result.has_expressions {
                    Confidence::Medium
                } else {
                    rule.confidence
                };

                let mut meta = Metadata::new();
                meta.insert("source_pattern", json!(rule.source_pattern));
                if !rule.is_env_var {
                    meta.insert("config_key", json!(true));
                }

                self.findings.push(
                    ShadowDep::new(
                        Category::EnvVar,
                        &result.value,
                        confidence,
                        Location::new(self.file_path, line, col),
                    )
                    .with_metadata(meta),
                );
                return;
            }
        }
    }

    fn matches_direct(&self, object: &Expression<'_>, rule: &ConfigAccessRule) -> bool {
        rule.modules
            .iter()
            .any(|m| is_bound_to_module(object, self.bindings, m) || is_require_of(object, m))
    }

    fn matches_contextual(&self, object: &Expression<'_>, rule: &ConfigAccessRule) -> bool {
        let has_import = rule
            .modules
            .iter()
            .any(|m| self.contextual_modules.contains(*m));
        has_import && receiver_name_contains_config(object)
    }
}

fn receiver_name_contains_config(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::Identifier(id) => id.name.as_str().to_ascii_lowercase().contains("config"),
        Expression::StaticMemberExpression(m) => m
            .property
            .name
            .as_str()
            .to_ascii_lowercase()
            .contains("config"),
        _ => false,
    }
}

impl<'a> Visit<'a> for ConfigAccessCollector<'a> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        self.try_match_call(call);
        walk::walk_call_expression(self, call);
    }
}
