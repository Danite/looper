use super::{is_bound_to_module, is_require_of, Detector, DetectorContext};
use crate::resolver::BindingMap;
use crate::types::{Category, Confidence, Location, Metadata, ShadowDep};
use oxc_ast::ast::{
    Argument, CallExpression, ComputedMemberExpression, Expression, LogicalExpression,
    LogicalOperator, Statement, StaticMemberExpression, VariableDeclarator,
};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use serde_json::json;

pub struct EnvVarsDetector;

impl Detector for EnvVarsDetector {
    fn name(&self) -> &str {
        "env_vars"
    }

    fn detect(&self, ctx: &DetectorContext<'_>) -> Vec<ShadowDep> {
        let mut collector = EnvVarCollector {
            file_path: ctx.file_path,
            source_text: ctx.source_text,
            bindings: ctx.bindings,
            findings: Vec::new(),
        };

        for stmt in &ctx.program.body {
            if let Statement::ImportDeclaration(import) = stmt {
                let src = import.source.value.as_str();
                if src == "dotenv/config" || src == "dotenv" {
                    let specifiers_empty = import
                        .specifiers
                        .as_ref()
                        .is_none_or(|s| s.is_empty());
                    if specifiers_empty {
                        collector.record(
                            "<dotenv>",
                            import.span.start,
                            Confidence::Medium,
                            "dotenv_config",
                        );
                    }
                }
            }
        }

        collector.visit_program(ctx.program);
        collector.findings
    }
}

struct EnvVarCollector<'a> {
    file_path: &'a str,
    source_text: &'a str,
    bindings: &'a BindingMap,
    findings: Vec<ShadowDep>,
}

impl EnvVarCollector<'_> {
    fn line_col(&self, offset: u32) -> (u32, u32) {
        crate::types::line_col(self.source_text, offset)
    }

    fn record(&mut self, name: &str, offset: u32, confidence: Confidence, pattern: &str) {
        let (line, col) = self.line_col(offset);
        let mut meta = Metadata::new();
        meta.insert("required", json!(true));
        meta.insert("source_pattern", json!(pattern));

        self.findings.push(
            ShadowDep::new(
                Category::EnvVar,
                name,
                confidence,
                Location::new(self.file_path, line, col),
            )
            .with_metadata(meta),
        );
    }

    fn record_with_default(
        &mut self,
        name: &str,
        offset: u32,
        confidence: Confidence,
        pattern: &str,
        default_value: &str,
    ) {
        let (line, col) = self.line_col(offset);
        let mut meta = Metadata::new();
        meta.insert("required", json!(false));
        meta.insert("default_value", json!(default_value));
        meta.insert("source_pattern", json!(pattern));

        self.findings.push(
            ShadowDep::new(
                Category::EnvVar,
                name,
                confidence,
                Location::new(self.file_path, line, col),
            )
            .with_metadata(meta),
        );
    }

    fn record_dynamic(&mut self, offset: u32, pattern: &str) {
        let (line, col) = self.line_col(offset);
        let mut meta = Metadata::new();
        meta.insert("required", json!(true));
        meta.insert("source_pattern", json!(pattern));

        self.findings.push(
            ShadowDep::new(
                Category::EnvVar,
                "<dynamic>",
                Confidence::Low,
                Location::new(self.file_path, line, col),
            )
            .with_metadata(meta),
        );
    }

    fn handle_envalid_schema(&mut self, call: &CallExpression<'_>) {
        let Some(second) = call.arguments.get(1) else {
            return;
        };
        let Some(Expression::ObjectExpression(obj)) = second.as_expression() else {
            return;
        };

        for prop in &obj.properties {
            let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue;
            };
            let Some(key_name) = p.key.name() else {
                continue;
            };

            if let Some(default_val) = extract_envalid_default(&p.value) {
                self.record_with_default(
                    key_name.as_ref(),
                    p.span().start,
                    Confidence::High,
                    "envalid",
                    &default_val,
                );
            } else {
                self.record(key_name.as_ref(), p.span().start, Confidence::High, "envalid");
            }
        }
    }
}

/// Check if an expression is `process.env`.
fn is_process_env(expr: &Expression<'_>) -> bool {
    if let Expression::StaticMemberExpression(m) = expr {
        if m.property.name == "env" {
            if let Expression::Identifier(id) = &m.object {
                return id.name == "process";
            }
        }
    }
    false
}

/// Check if an expression is `import.meta.env`.
fn is_import_meta_env(expr: &Expression<'_>) -> bool {
    if let Expression::StaticMemberExpression(m) = expr {
        if m.property.name == "env" {
            if let Expression::MetaProperty(meta) = &m.object {
                return meta.meta.name == "import" && meta.property.name == "meta";
            }
        }
    }
    false
}

/// Check if an expression is `Bun.env`.
fn is_bun_env(expr: &Expression<'_>) -> bool {
    if let Expression::StaticMemberExpression(m) = expr {
        if m.property.name == "env" {
            if let Expression::Identifier(id) = &m.object {
                return id.name == "Bun";
            }
        }
    }
    false
}

/// Check if an expression is `Deno.env`.
fn is_deno_env(expr: &Expression<'_>) -> bool {
    if let Expression::StaticMemberExpression(m) = expr {
        if m.property.name == "env" {
            if let Expression::Identifier(id) = &m.object {
                return id.name == "Deno";
            }
        }
    }
    false
}

/// Try to extract a string representation of a default value from an expression.
fn extract_default_value(expr: &Expression<'_>) -> Option<String> {
    match expr {
        Expression::StringLiteral(lit) => Some(lit.value.to_string()),
        Expression::NumericLiteral(lit) => Some(lit.raw.as_ref().map(|r| r.to_string()).unwrap_or_else(|| lit.value.to_string())),
        Expression::BooleanLiteral(lit) => Some(lit.value.to_string()),
        Expression::NullLiteral(_) => Some("null".to_string()),
        _ => None,
    }
}

/// Try to extract the env var name from a process.env access expression.
/// Returns (name, source_pattern) if it's a known pattern.
fn extract_env_access(expr: &Expression<'_>) -> Option<EnvAccess> {
    match expr {
        Expression::StaticMemberExpression(m) => {
            let name = m.property.name.as_str();
            if is_process_env(&m.object) {
                return Some(EnvAccess {
                    name: name.to_string(),
                    confidence: Confidence::High,
                    pattern: "direct_access",
                    offset: m.span().start,
                });
            }
            if is_import_meta_env(&m.object) {
                return Some(EnvAccess {
                    name: name.to_string(),
                    confidence: Confidence::High,
                    pattern: "import_meta_env",
                    offset: m.span().start,
                });
            }
            if is_bun_env(&m.object) {
                return Some(EnvAccess {
                    name: name.to_string(),
                    confidence: Confidence::High,
                    pattern: "bun_env",
                    offset: m.span().start,
                });
            }
            None
        }
        Expression::ComputedMemberExpression(m) => {
            if is_process_env(&m.object) {
                if let Expression::StringLiteral(lit) = &m.expression {
                    return Some(EnvAccess {
                        name: lit.value.to_string(),
                        confidence: Confidence::High,
                        pattern: "bracket_access",
                        offset: m.span().start,
                    });
                }
            }
            None
        }
        _ => None,
    }
}

/// Extract the default value from an envalid validator call, e.g.
/// `envalid.port({ default: 3000 })` returns `Some("3000")`.
fn extract_envalid_default(expr: &Expression<'_>) -> Option<String> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    let arg = call.arguments.first()?;
    let Expression::ObjectExpression(obj) = arg.as_expression()? else {
        return None;
    };
    for prop in &obj.properties {
        let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        if p.key.name().is_some_and(|k| k.as_ref() == "default") {
            return extract_default_value(&p.value);
        }
    }
    None
}

struct EnvAccess {
    name: String,
    confidence: Confidence,
    pattern: &'static str,
    offset: u32,
}

impl<'a> Visit<'a> for EnvVarCollector<'a> {
    fn visit_logical_expression(&mut self, expr: &LogicalExpression<'a>) {
        // process.env.X || default  /  process.env.X ?? default
        if matches!(
            expr.operator,
            LogicalOperator::Or | LogicalOperator::Coalesce
        ) {
            if let Some(access) = extract_env_access(&expr.left) {
                let default_str = extract_default_value(&expr.right)
                    .unwrap_or_default();
                if default_str.is_empty() {
                    self.record(&access.name, access.offset, access.confidence, access.pattern);
                } else {
                    self.record_with_default(
                        &access.name,
                        access.offset,
                        access.confidence,
                        access.pattern,
                        &default_str,
                    );
                }
                self.visit_expression(&expr.right);
                return;
            }
        }
        walk::walk_logical_expression(self, expr);
    }

    fn visit_static_member_expression(&mut self, expr: &StaticMemberExpression<'a>) {
        let name = expr.property.name.as_str();

        if is_process_env(&expr.object) {
            self.record(name, expr.span().start, Confidence::High, "direct_access");
        } else if is_import_meta_env(&expr.object) {
            self.record(name, expr.span().start, Confidence::High, "import_meta_env");
        } else if is_bun_env(&expr.object) {
            self.record(name, expr.span().start, Confidence::High, "bun_env");
        }

        walk::walk_static_member_expression(self, expr);
    }

    fn visit_computed_member_expression(&mut self, expr: &ComputedMemberExpression<'a>) {
        if is_process_env(&expr.object) {
            match &expr.expression {
                Expression::StringLiteral(lit) => {
                    self.record(
                        lit.value.as_str(),
                        expr.span().start,
                        Confidence::High,
                        "bracket_access",
                    );
                }
                _ => {
                    self.record_dynamic(expr.span().start, "dynamic_access");
                }
            }
        }

        walk::walk_computed_member_expression(self, expr);
    }

    fn visit_variable_declarator(&mut self, decl: &VariableDeclarator<'a>) {
        // const { NODE_ENV, PORT } = process.env
        if let Some(init) = &decl.init {
            if is_process_env(init) {
                if let oxc_ast::ast::BindingPattern::ObjectPattern(obj) = &decl.id {
                    for prop in &obj.properties {
                        if let Some(name) = prop.value.get_identifier_name() {
                            let key_name = prop
                                .key
                                .name()
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| name.as_str().to_string());
                            self.record(
                                &key_name,
                                prop.span().start,
                                Confidence::High,
                                "destructuring",
                            );
                        }
                    }
                }
            }
        }
        walk::walk_variable_declarator(self, decl);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Expression::StaticMemberExpression(m) = &call.callee {
            // Deno.env.get("VAR")
            if m.property.name == "get" && is_deno_env(&m.object) {
                if let Some(Argument::StringLiteral(lit)) = call.arguments.first() {
                    self.record(
                        lit.value.as_str(),
                        call.span().start,
                        Confidence::High,
                        "deno_env",
                    );
                }
            }

            // require("dotenv").config() or dotenv.config()
            if m.property.name == "config"
                && (is_require_of(&m.object, "dotenv")
                    || is_bound_to_module(&m.object, self.bindings, "dotenv"))
            {
                self.record(
                    "<dotenv>",
                    call.span().start,
                    Confidence::Medium,
                    "dotenv_config",
                );
            }

            // envalid.cleanEnv(process.env, { KEY: envalid.str(), ... })
            if m.property.name == "cleanEnv"
                && (is_bound_to_module(&m.object, self.bindings, "envalid")
                    || matches!(&m.object, Expression::Identifier(id) if id.name == "envalid"))
            {
                self.handle_envalid_schema(call);
            }
        }
        walk::walk_call_expression(self, call);
    }
}
