use super::{extract_string_or_template_arg, Detector, DetectorContext, StringOrTemplate};
use crate::resolver::BindingMap;
use crate::types::{Category, Confidence, Location, Metadata, ShadowDep};
use oxc_ast::ast::{
    CallExpression, Expression, JSXAttribute, JSXAttributeValue, JSXOpeningElement, NewExpression,
};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use serde_json::json;
use url::Url;

const HTTP_MODULES: &[&str] = &[
    "http",
    "https",
    "node:http",
    "node:https",
    "http2",
    "node:http2",
];
const AXIOS_METHODS: &[&str] = &[
    "get", "post", "put", "delete", "patch", "head", "options", "request",
];
const URL_SCHEMES: &[&str] = &["http", "https", "ws", "wss"];
const HTTP_CLIENT_MODULES: &[&str] = &[
    "got",
    "node-fetch",
    "ky",
    "superagent",
    "ofetch",
    "undici",
    "make-fetch-happen",
    "urllib",
    "phin",
];

pub struct NetworkDetector;

impl Detector for NetworkDetector {
    fn name(&self) -> &str {
        "network"
    }

    fn detect(&self, ctx: &DetectorContext<'_>) -> Vec<ShadowDep> {
        let mut collector = NetCollector {
            file_path: ctx.file_path,
            source_text: ctx.source_text,
            bindings: ctx.bindings,
            findings: Vec::new(),
        };
        collector.visit_program(ctx.program);
        collector.findings
    }
}

struct NetCollector<'a> {
    file_path: &'a str,
    source_text: &'a str,
    bindings: &'a BindingMap,
    findings: Vec<ShadowDep>,
}

impl NetCollector<'_> {
    fn line_col(&self, offset: u32) -> (u32, u32) {
        crate::types::line_col(self.source_text, offset)
    }

    fn record_url(&mut self, url_str: &str, offset: u32, confidence: Confidence) {
        let Ok(parsed) = Url::parse(url_str) else {
            return;
        };

        let scheme = parsed.scheme();
        if !URL_SCHEMES.contains(&scheme) {
            return;
        }

        let host = match parsed.host_str() {
            Some(h) => h.to_string(),
            None => return,
        };

        let name = if let Some(port) = parsed.port() {
            format!("{host}:{port}")
        } else {
            host.clone()
        };

        let (line, col) = self.line_col(offset);
        let mut meta = Metadata::new();
        meta.insert("protocol", json!(scheme));
        if !parsed.path().is_empty() && parsed.path() != "/" {
            meta.insert("path_pattern", json!(parsed.path()));
        }

        self.findings.push(
            ShadowDep::new(
                Category::Network,
                &name,
                confidence,
                Location::new(self.file_path, line, col),
            )
            .with_metadata(meta),
        );
    }

    fn record_url_or_template(&mut self, result: &StringOrTemplate, offset: u32) {
        if result.has_expressions {
            let static_prefix = result.value.split('*').next().unwrap_or(&result.value);
            self.record_url(static_prefix, offset, Confidence::Medium);
        } else {
            self.record_url(&result.value, offset, Confidence::High);
        }
    }

    fn is_bound_to_http(&self, expr: &Expression<'_>) -> bool {
        if let Expression::Identifier(id) = expr {
            if let Some(binding) = self.bindings.get(id.name.as_str()) {
                if binding.imported_name == "*" {
                    return HTTP_MODULES.iter().any(|m| binding.module_source == *m);
                }
            }
        }
        false
    }

    fn handle_fetch(&mut self, call: &CallExpression<'_>) {
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name != "fetch" {
            return;
        }
        if let Some(result) = call
            .arguments
            .first()
            .and_then(extract_string_or_template_arg)
        {
            self.record_url_or_template(&result, call.span().start);
        }
    }

    fn handle_axios(&mut self, call: &CallExpression<'_>) {
        let is_axios_call = match &call.callee {
            Expression::StaticMemberExpression(m) => {
                matches!(&m.object, Expression::Identifier(obj) if obj.name == "axios")
                    && AXIOS_METHODS.contains(&m.property.name.as_str())
            }
            Expression::Identifier(id) => id.name == "axios",
            _ => false,
        };
        if !is_axios_call {
            return;
        }
        if let Some(result) = call
            .arguments
            .first()
            .and_then(extract_string_or_template_arg)
        {
            self.record_url_or_template(&result, call.span().start);
        }
    }

    fn is_bound_to_http_client(&self, name: &str) -> bool {
        if let Some(binding) = self.bindings.get(name) {
            return HTTP_CLIENT_MODULES
                .iter()
                .any(|m| binding.module_source == *m);
        }
        false
    }

    fn handle_http_client_call(&mut self, call: &CallExpression<'_>) {
        let is_client_call = match &call.callee {
            Expression::StaticMemberExpression(m) => {
                matches!(&m.object, Expression::Identifier(obj) if self.is_bound_to_http_client(obj.name.as_str()))
                    && AXIOS_METHODS.contains(&m.property.name.as_str())
            }
            Expression::Identifier(id) => {
                id.name != "fetch"
                    && id.name != "axios"
                    && self.is_bound_to_http_client(id.name.as_str())
            }
            _ => false,
        };
        if !is_client_call {
            return;
        }
        if let Some(result) = call
            .arguments
            .first()
            .and_then(extract_string_or_template_arg)
        {
            self.record_url_or_template(&result, call.span().start);
        }
    }

    fn handle_http_request(&mut self, call: &CallExpression<'_>) {
        // http.request({ hostname: 'example.com' })
        let Expression::StaticMemberExpression(m) = &call.callee else {
            return;
        };
        if m.property.name != "request" && m.property.name != "get" {
            return;
        }
        if !self.is_bound_to_http(&m.object) {
            return;
        }

        let Some(first) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first.as_expression() else {
            return;
        };

        if let Expression::StringLiteral(lit) = expr {
            self.record_url(lit.value.as_str(), call.span().start, Confidence::High);
            return;
        }

        let Expression::ObjectExpression(obj) = expr else {
            return;
        };

        for prop in &obj.properties {
            let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue;
            };
            let Some(key_name) = p.key.name() else {
                continue;
            };
            if key_name == "hostname" || key_name == "host" {
                if let Expression::StringLiteral(lit) = &p.value {
                    let (line, col) = self.line_col(call.span().start);
                    let mut meta = Metadata::new();
                    meta.insert("protocol", json!("http"));

                    self.findings.push(
                        ShadowDep::new(
                            Category::Network,
                            lit.value.as_str(),
                            Confidence::High,
                            Location::new(self.file_path, line, col),
                        )
                        .with_metadata(meta),
                    );
                }
            }
        }
    }

    fn handle_new_url(&mut self, new_expr: &NewExpression<'_>) {
        let Expression::Identifier(id) = &new_expr.callee else {
            return;
        };
        if id.name != "URL" {
            return;
        }
        if let Some(result) = new_expr
            .arguments
            .first()
            .and_then(extract_string_or_template_arg)
        {
            self.record_url_or_template(&result, new_expr.span().start);
        }
    }

    fn handle_new_websocket(&mut self, new_expr: &NewExpression<'_>) {
        let Expression::Identifier(id) = &new_expr.callee else {
            return;
        };
        if id.name != "WebSocket" {
            return;
        }
        if let Some(result) = new_expr
            .arguments
            .first()
            .and_then(extract_string_or_template_arg)
        {
            self.record_url_or_template(&result, new_expr.span().start);
        }
    }
}

fn is_url_like(s: &str) -> bool {
    URL_SCHEMES
        .iter()
        .any(|scheme| s.starts_with(&format!("{scheme}://")))
}

impl<'a> Visit<'a> for NetCollector<'a> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        self.handle_fetch(call);
        self.handle_axios(call);
        self.handle_http_client_call(call);
        self.handle_http_request(call);

        walk::walk_call_expression(self, call);
    }

    fn visit_new_expression(&mut self, new_expr: &NewExpression<'a>) {
        self.handle_new_url(new_expr);
        self.handle_new_websocket(new_expr);

        walk::walk_new_expression(self, new_expr);
    }

    fn visit_jsx_opening_element(&mut self, elem: &JSXOpeningElement<'a>) {
        for attr in &elem.attributes {
            let oxc_ast::ast::JSXAttributeItem::Attribute(attr) = attr else {
                continue;
            };
            self.check_jsx_attr(attr);
        }
        walk::walk_jsx_opening_element(self, elem);
    }
}

impl NetCollector<'_> {
    fn check_jsx_attr(&mut self, attr: &JSXAttribute<'_>) {
        let name = attr.name.as_identifier().map(|id| id.name.as_str());
        if !matches!(name, Some("src" | "href" | "action")) {
            return;
        }

        let Some(value) = &attr.value else {
            return;
        };

        if let JSXAttributeValue::StringLiteral(lit) = value {
            if is_url_like(lit.value.as_str()) {
                self.record_url(lit.value.as_str(), attr.span().start, Confidence::High);
            }
        }
    }
}
