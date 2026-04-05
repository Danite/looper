use super::{Detector, DetectorContext};
use crate::resolver::BindingMap;
use crate::types::{Category, Confidence, Location, Metadata, ShadowDep};
use oxc_ast::ast::{Argument, CallExpression, Expression, TaggedTemplateExpression};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use serde_json::json;

const CP_MODULES: &[&str] = &[
    "child_process",
    "node:child_process",
];
const EXEC_METHODS: &[&str] = &["exec", "execSync"];
const SPAWN_METHODS: &[&str] = &["spawn", "spawnSync", "execFile", "execFileSync"];
const EXECA_MODULES: &[&str] = &["execa"];

pub struct ShellDetector;

impl Detector for ShellDetector {
    fn name(&self) -> &str {
        "shell"
    }

    fn detect(&self, ctx: &DetectorContext<'_>) -> Vec<ShadowDep> {
        let mut collector = ShellCollector {
            file_path: ctx.file_path,
            source_text: ctx.source_text,
            bindings: ctx.bindings,
            findings: Vec::new(),
        };
        collector.visit_program(ctx.program);
        collector.findings
    }
}

struct ShellCollector<'a> {
    file_path: &'a str,
    source_text: &'a str,
    bindings: &'a BindingMap,
    findings: Vec<ShadowDep>,
}

impl ShellCollector<'_> {
    fn line_col(&self, offset: u32) -> (u32, u32) {
        crate::types::line_col(self.source_text, offset)
    }

    fn record(&mut self, binary: &str, offset: u32, confidence: Confidence, args_pattern: &str) {
        let (line, col) = self.line_col(offset);
        let mut meta = Metadata::new();
        if !args_pattern.is_empty() {
            meta.insert("args_pattern", json!(args_pattern));
        }

        self.findings.push(
            ShadowDep::new(
                Category::Binary,
                binary,
                confidence,
                Location::new(self.file_path, line, col),
            )
            .with_metadata(meta),
        );
    }

    fn is_bound_to_cp(&self, expr: &Expression<'_>) -> bool {
        if let Expression::Identifier(id) = expr {
            if let Some(binding) = self.bindings.get(id.name.as_str()) {
                if binding.imported_name == "*" {
                    return CP_MODULES.iter().any(|m| binding.module_source == *m);
                }
            }
        }
        false
    }

    fn is_bound_to_cp_method(&self, name: &str) -> bool {
        if let Some(binding) = self.bindings.get(name) {
            return CP_MODULES.iter().any(|m| binding.module_source == *m);
        }
        false
    }

    fn is_bound_to_execa(&self, name: &str) -> bool {
        if let Some(binding) = self.bindings.get(name) {
            return EXECA_MODULES.iter().any(|m| binding.module_source == *m);
        }
        false
    }

    fn handle_exec_call(&mut self, call: &CallExpression<'_>) {
        let method_name = match &call.callee {
            Expression::StaticMemberExpression(m) => {
                if !self.is_bound_to_cp(&m.object) {
                    return;
                }
                m.property.name.as_str()
            }
            Expression::Identifier(id) => {
                if !self.is_bound_to_cp_method(id.name.as_str()) {
                    return;
                }
                id.name.as_str()
            }
            _ => return,
        };

        if EXEC_METHODS.contains(&method_name) {
            self.handle_exec_string(call);
        } else if SPAWN_METHODS.contains(&method_name) {
            self.handle_spawn_args(call);
        }
    }

    fn handle_exec_string(&mut self, call: &CallExpression<'_>) {
        let Some(Argument::StringLiteral(lit)) = call.arguments.first() else {
            return;
        };

        let cmd = lit.value.as_str().trim();
        if cmd.is_empty() {
            return;
        }

        let mut parts = cmd.split_whitespace();
        let binary = parts.next().unwrap();
        let args: String = parts.collect::<Vec<_>>().join(" ");

        self.record(binary, call.span().start, Confidence::High, &args);
    }

    fn handle_spawn_args(&mut self, call: &CallExpression<'_>) {
        let Some(Argument::StringLiteral(lit)) = call.arguments.first() else {
            return;
        };

        let binary = lit.value.as_str();
        if binary.is_empty() {
            return;
        }

        let args_pattern = extract_array_args(call);
        self.record(binary, call.span().start, Confidence::High, &args_pattern);
    }

    fn handle_execa_call(&mut self, call: &CallExpression<'_>) {
        let is_execa = match &call.callee {
            Expression::Identifier(id) => self.is_bound_to_execa(id.name.as_str()),
            Expression::StaticMemberExpression(m) => {
                if let Expression::Identifier(obj) = &m.object {
                    self.is_bound_to_execa(obj.name.as_str())
                } else {
                    false
                }
            }
            _ => false,
        };

        if !is_execa {
            return;
        }

        let Some(Argument::StringLiteral(lit)) = call.arguments.first() else {
            return;
        };

        let binary = lit.value.as_str();
        if binary.is_empty() {
            return;
        }

        let args_pattern = extract_array_args(call);
        self.record(binary, call.span().start, Confidence::High, &args_pattern);
    }

    fn handle_zx_tagged_template(&mut self, expr: &TaggedTemplateExpression<'_>) {
        let Expression::Identifier(tag) = &expr.tag else {
            return;
        };
        if tag.name != "$" {
            return;
        }

        let quasis: Vec<&str> = expr
            .quasi
            .quasis
            .iter()
            .map(|q| q.value.raw.as_str())
            .collect();

        let full_cmd = quasis.join("*").trim().to_string();
        if full_cmd.is_empty() {
            return;
        }

        let mut parts = full_cmd.split_whitespace();
        let binary = match parts.next() {
            Some(b) => b.to_string(),
            None => return,
        };
        let args: String = parts.collect::<Vec<_>>().join(" ");

        let confidence = if quasis.len() > 1 {
            Confidence::Medium
        } else {
            Confidence::High
        };

        self.record(&binary, expr.span().start, confidence, &args);
    }
}

fn extract_array_args(call: &CallExpression<'_>) -> String {
    let Some(second) = call.arguments.get(1) else {
        return String::new();
    };
    let Some(Expression::ArrayExpression(arr)) = second.as_expression() else {
        return String::new();
    };

    let parts: Vec<String> = arr
        .elements
        .iter()
        .map(|elem| {
            if let oxc_ast::ast::ArrayExpressionElement::StringLiteral(lit) = elem {
                lit.value.to_string()
            } else {
                "*".to_string()
            }
        })
        .collect();

    parts.join(" ")
}

impl<'a> Visit<'a> for ShellCollector<'a> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        self.handle_exec_call(call);
        self.handle_execa_call(call);
        walk::walk_call_expression(self, call);
    }

    fn visit_tagged_template_expression(&mut self, expr: &TaggedTemplateExpression<'a>) {
        self.handle_zx_tagged_template(expr);
        walk::walk_tagged_template_expression(self, expr);
    }
}
