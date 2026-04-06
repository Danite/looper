use super::{Detector, DetectorContext};
use crate::resolver::BindingMap;
use crate::types::{Category, Confidence, Location, Metadata, ShadowDep};
use oxc_ast::ast::{Argument, CallExpression, Expression, NewExpression};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use regex::Regex;
use serde_json::json;
use std::sync::LazyLock;

const NET_MODULES: &[&str] = &["net", "node:net"];
const HTTP_MODULES: &[&str] = &[
    "http",
    "https",
    "node:http",
    "node:https",
    "http2",
    "node:http2",
];

static CONN_STRING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<proto>[a-z][a-z0-9+\-.]*?)://(?:[^@/]*@)?(?P<host>[^/:]+?)(?::(?P<port>\d+))?(?:/|$)")
        .unwrap()
});

const KNOWN_PROTOCOL_PORTS: &[(&str, &str, u16)] = &[
    ("redis", "redis", 6379),
    ("rediss", "redis", 6379),
    ("mongodb", "mongodb", 27017),
    ("mongodb+srv", "mongodb", 27017),
    ("postgres", "postgres", 5432),
    ("postgresql", "postgres", 5432),
    ("mysql", "mysql", 3306),
    ("amqp", "amqp", 5672),
    ("amqps", "amqp", 5672),
    ("nats", "nats", 4222),
];

pub struct PortsDetector;

impl Detector for PortsDetector {
    fn name(&self) -> &str {
        "ports"
    }

    fn detect(&self, ctx: &DetectorContext<'_>) -> Vec<ShadowDep> {
        let mut collector = PortCollector {
            file_path: ctx.file_path,
            source_text: ctx.source_text,
            bindings: ctx.bindings,
            findings: Vec::new(),
        };
        collector.visit_program(ctx.program);
        collector.findings
    }
}

struct PortCollector<'a> {
    file_path: &'a str,
    source_text: &'a str,
    bindings: &'a BindingMap,
    findings: Vec<ShadowDep>,
}

impl PortCollector<'_> {
    fn line_col(&self, offset: u32) -> (u32, u32) {
        crate::types::line_col(self.source_text, offset)
    }

    fn record_port(&mut self, port: u16, offset: u32, direction: &str, protocol_hint: &str) {
        let (line, col) = self.line_col(offset);
        let name = format!(":{port}");
        let id = format!("port::{port}::{direction}");
        let mut meta = Metadata::new();
        meta.insert("direction", json!(direction));
        if !protocol_hint.is_empty() {
            meta.insert("protocol_hint", json!(protocol_hint));
        }

        self.findings.push(ShadowDep {
            id,
            category: Category::Port,
            name,
            confidence: Confidence::High,
            metadata: meta,
            locations: vec![Location::new(self.file_path, line, col)],
        });
    }

    fn record_network_from_conn_string(&mut self, host: &str, offset: u32, protocol: &str) {
        let (line, col) = self.line_col(offset);
        let mut meta = Metadata::new();
        meta.insert("protocol", json!(protocol));

        self.findings.push(
            ShadowDep::new(
                Category::Network,
                host,
                Confidence::High,
                Location::new(self.file_path, line, col),
            )
            .with_metadata(meta),
        );
    }

    fn is_bound_to_any(&self, expr: &Expression<'_>, modules: &[&str]) -> bool {
        if let Expression::Identifier(id) = expr {
            if let Some(binding) = self.bindings.get(id.name.as_str()) {
                if binding.imported_name == "*" {
                    return modules.iter().any(|m| binding.module_source == *m);
                }
            }
        }
        false
    }

    fn is_bound_to_net(&self, expr: &Expression<'_>) -> bool {
        self.is_bound_to_any(expr, NET_MODULES)
    }

    fn resolve_listen_protocol(&self, obj: &Expression<'_>) -> &'static str {
        if self.is_bound_to_any(obj, NET_MODULES) {
            return "tcp";
        }
        if self.is_bound_to_any(obj, HTTP_MODULES) {
            return "http";
        }
        // Handle chained calls like net.createServer().listen(port)
        // where obj is the CallExpression `net.createServer()`
        if let Expression::CallExpression(call) = obj {
            if let Expression::StaticMemberExpression(m) = &call.callee {
                if self.is_bound_to_any(&m.object, NET_MODULES) {
                    return "tcp";
                }
                if self.is_bound_to_any(&m.object, HTTP_MODULES) {
                    return "http";
                }
            }
        }
        "http"
    }

    fn handle_listen_call(&mut self, call: &CallExpression<'_>) {
        let Expression::StaticMemberExpression(m) = &call.callee else {
            return;
        };
        if m.property.name != "listen" {
            return;
        }

        if let Some(port) = extract_port_from_first_arg(call) {
            let protocol = self.resolve_listen_protocol(&m.object);
            self.record_port(port, call.span().start, "listen", protocol);
        }
    }

    fn handle_net_create_connection(&mut self, call: &CallExpression<'_>) {
        let Expression::StaticMemberExpression(m) = &call.callee else {
            return;
        };
        if m.property.name != "createConnection" && m.property.name != "connect" {
            return;
        }
        if !self.is_bound_to_net(&m.object) {
            return;
        }

        if let Some(port) = extract_port_from_options_arg(call) {
            self.record_port(port, call.span().start, "connect", "tcp");
        }
    }

    fn handle_bun_serve(&mut self, call: &CallExpression<'_>) {
        let Expression::StaticMemberExpression(m) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &m.object else {
            return;
        };
        if obj.name != "Bun" || m.property.name != "serve" {
            return;
        }

        if let Some(port) = extract_port_from_options_arg(call) {
            self.record_port(port, call.span().start, "listen", "http");
        }
    }

    fn handle_connection_string_args(
        &mut self,
        args: &oxc_allocator::Vec<'_, Argument<'_>>,
        offset: u32,
    ) {
        let Some(Argument::StringLiteral(lit)) = args.first() else {
            return;
        };

        let s = lit.value.as_str();
        let Some(caps) = CONN_STRING_RE.captures(s) else {
            return;
        };

        let proto = caps.name("proto").map(|m| m.as_str()).unwrap_or("");
        let host = caps.name("host").map(|m| m.as_str()).unwrap_or("");

        let (protocol_hint, default_port) = KNOWN_PROTOCOL_PORTS
            .iter()
            .find(|(p, _, _)| *p == proto)
            .map(|(_, hint, dp)| (*hint, Some(*dp)))
            .unwrap_or(("", None));

        if protocol_hint.is_empty() {
            return;
        }

        let port: Option<u16> = caps
            .name("port")
            .and_then(|m| m.as_str().parse().ok())
            .or(default_port);

        if let Some(port) = port {
            self.record_port(port, offset, "connect", protocol_hint);
        }

        if !host.is_empty() {
            self.record_network_from_conn_string(host, offset, proto);
        }
    }
}

fn f64_to_port(v: f64) -> Option<u16> {
    if (1.0..=65535.0).contains(&v) && v.fract() == 0.0 {
        Some(v as u16)
    } else {
        None
    }
}

fn extract_port_from_first_arg(call: &CallExpression<'_>) -> Option<u16> {
    let arg = call.arguments.first()?;
    match arg {
        Argument::NumericLiteral(lit) => f64_to_port(lit.value),
        _ => {
            let expr = arg.as_expression()?;
            extract_port_from_logical_expr(expr)
        }
    }
}

fn extract_port_from_logical_expr(expr: &Expression<'_>) -> Option<u16> {
    if let Expression::LogicalExpression(logical) = expr {
        if let Expression::NumericLiteral(lit) = &logical.right {
            return f64_to_port(lit.value);
        }
        if let Expression::NumericLiteral(lit) = &logical.left {
            return f64_to_port(lit.value);
        }
    }
    if let Expression::NumericLiteral(lit) = expr {
        return f64_to_port(lit.value);
    }
    None
}

fn extract_port_from_options_arg(call: &CallExpression<'_>) -> Option<u16> {
    let arg = call.arguments.first()?;
    let expr = arg.as_expression()?;
    let Expression::ObjectExpression(obj) = expr else {
        return None;
    };

    for prop in &obj.properties {
        let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let Some(key_name) = p.key.name() else {
            continue;
        };
        if key_name == "port" {
            if let Expression::NumericLiteral(lit) = &p.value {
                return f64_to_port(lit.value);
            }
        }
    }
    None
}

impl<'a> Visit<'a> for PortCollector<'a> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        self.handle_listen_call(call);
        self.handle_net_create_connection(call);
        self.handle_bun_serve(call);
        self.handle_connection_string_args(&call.arguments, call.span().start);

        walk::walk_call_expression(self, call);
    }

    fn visit_new_expression(&mut self, new_expr: &NewExpression<'a>) {
        self.handle_connection_string_args(&new_expr.arguments, new_expr.span().start);

        walk::walk_new_expression(self, new_expr);
    }
}
