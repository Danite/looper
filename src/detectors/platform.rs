use super::{Detector, DetectorContext};
use crate::resolver::BindingMap;
use crate::types::{Category, Confidence, Location, Metadata, ShadowDep};
use oxc_ast::ast::{BinaryExpression, CallExpression, Expression};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use serde_json::json;

const OS_MODULES: &[&str] = &["os", "node:os"];
const UNIX_PATH_PREFIXES: &[&str] = &[
    "/dev/", "/etc/", "/usr/", "/var/", "/tmp/", "/opt/", "/proc/", "/sys/",
];
const WINDOWS_BINARIES: &[&str] = &["cmd", "cmd.exe", "powershell", "powershell.exe"];
const OS_PLATFORM_METHODS: &[&str] = &[
    "homedir",
    "tmpdir",
    "hostname",
    "userInfo",
    "networkInterfaces",
    "platform",
    "type",
    "release",
    "arch",
];
const PLATFORM_VALUES: &[&str] = &[
    "darwin", "linux", "win32", "freebsd", "openbsd", "sunos", "aix",
];
const ARCH_VALUES: &[&str] = &[
    "arm", "arm64", "ia32", "loong64", "mips", "mipsel", "ppc", "ppc64", "riscv64", "s390",
    "s390x", "x64",
];

pub struct PlatformDetector;

impl Detector for PlatformDetector {
    fn name(&self) -> &str {
        "platform"
    }

    fn detect(&self, ctx: &DetectorContext<'_>) -> Vec<ShadowDep> {
        let mut collector = PlatformCollector {
            file_path: ctx.file_path,
            source_text: ctx.source_text,
            bindings: ctx.bindings,
            findings: Vec::new(),
        };
        collector.visit_program(ctx.program);
        collector.findings
    }
}

struct PlatformCollector<'a> {
    file_path: &'a str,
    source_text: &'a str,
    bindings: &'a BindingMap,
    findings: Vec<ShadowDep>,
}

impl PlatformCollector<'_> {
    fn line_col(&self, offset: u32) -> (u32, u32) {
        crate::types::line_col(self.source_text, offset)
    }

    fn record(
        &mut self,
        name: &str,
        offset: u32,
        confidence: Confidence,
        assumed_platform: &str,
        guarded: bool,
    ) {
        let (line, col) = self.line_col(offset);
        let mut meta = Metadata::new();
        meta.insert("assumed_platform", json!(assumed_platform));
        meta.insert("guarded", json!(guarded));

        self.findings.push(
            ShadowDep::new(
                Category::Platform,
                name,
                confidence,
                Location::new(self.file_path, line, col),
            )
            .with_metadata(meta),
        );
    }

    fn is_bound_to_os(&self, expr: &Expression<'_>) -> bool {
        if let Expression::Identifier(id) = expr {
            if let Some(binding) = self.bindings.get(id.name.as_str()) {
                if binding.imported_name == "*" {
                    return OS_MODULES.iter().any(|m| binding.module_source == *m);
                }
            }
        }
        false
    }

    fn is_os_method(&self, name: &str) -> bool {
        if let Some(binding) = self.bindings.get(name) {
            return OS_MODULES.iter().any(|m| binding.module_source == *m);
        }
        false
    }

    /// Check string literals for Unix-only paths.
    fn check_unix_path(&mut self, value: &str, offset: u32) {
        for prefix in UNIX_PATH_PREFIXES {
            if value.starts_with(prefix) {
                self.record(value, offset, Confidence::High, "unix", false);
                return;
            }
        }
    }

    /// Check string literals for Windows-only commands.
    fn check_windows_command(&mut self, value: &str, offset: u32) {
        let first_word = value.split_whitespace().next().unwrap_or("");
        if WINDOWS_BINARIES.contains(&first_word) {
            self.record(value, offset, Confidence::High, "windows", false);
        }
    }

    /// process.platform === 'darwin' / 'linux' / 'win32'
    fn handle_platform_check(&mut self, expr: &BinaryExpression<'_>) {
        if !matches!(
            expr.operator,
            oxc_ast::ast::BinaryOperator::StrictEquality
                | oxc_ast::ast::BinaryOperator::Equality
                | oxc_ast::ast::BinaryOperator::StrictInequality
                | oxc_ast::ast::BinaryOperator::Inequality
        ) {
            return;
        }

        let (platform_expr, value_expr) = if is_process_platform(&expr.left) {
            (&expr.left, &expr.right)
        } else if is_process_platform(&expr.right) {
            (&expr.right, &expr.left)
        } else {
            return;
        };
        let _ = platform_expr;

        let Expression::StringLiteral(lit) = value_expr else {
            return;
        };

        let platform_name = lit.value.as_str();
        if !PLATFORM_VALUES.contains(&platform_name) {
            return;
        }

        let assumed = match platform_name {
            "win32" => "windows",
            "darwin" | "linux" | "freebsd" | "openbsd" | "sunos" | "aix" => "unix",
            _ => "any",
        };

        self.record(
            &format!("process.platform === \"{platform_name}\""),
            expr.span().start,
            Confidence::High,
            assumed,
            true,
        );
    }

    /// process.arch === 'x64' / 'arm64' / etc.
    fn handle_arch_check(&mut self, expr: &BinaryExpression<'_>) {
        if !matches!(
            expr.operator,
            oxc_ast::ast::BinaryOperator::StrictEquality
                | oxc_ast::ast::BinaryOperator::Equality
                | oxc_ast::ast::BinaryOperator::StrictInequality
                | oxc_ast::ast::BinaryOperator::Inequality
        ) {
            return;
        }

        let (_, value_expr) = if is_process_arch(&expr.left) {
            (&expr.left, &expr.right)
        } else if is_process_arch(&expr.right) {
            (&expr.right, &expr.left)
        } else {
            return;
        };

        let Expression::StringLiteral(lit) = value_expr else {
            return;
        };

        let arch_name = lit.value.as_str();
        if !ARCH_VALUES.contains(&arch_name) {
            return;
        }

        self.record(
            &format!("process.arch === \"{arch_name}\""),
            expr.span().start,
            Confidence::High,
            "any",
            true,
        );
    }

    /// path.sep === '/' or path.sep === '\\'
    fn handle_path_sep_check(&mut self, expr: &BinaryExpression<'_>) {
        if !matches!(
            expr.operator,
            oxc_ast::ast::BinaryOperator::StrictEquality | oxc_ast::ast::BinaryOperator::Equality
        ) {
            return;
        }

        let is_path_sep = is_path_sep(&expr.left) || is_path_sep(&expr.right);
        if !is_path_sep {
            return;
        }

        let value = if let Expression::StringLiteral(lit) = &expr.right {
            lit.value.as_str()
        } else if let Expression::StringLiteral(lit) = &expr.left {
            lit.value.as_str()
        } else {
            return;
        };

        let assumed = match value {
            "/" => "unix",
            "\\" => "windows",
            _ => return,
        };

        self.record(
            &format!("path.sep === \"{value}\""),
            expr.span().start,
            Confidence::High,
            assumed,
            false,
        );
    }

    /// os.homedir(), os.tmpdir(), os.hostname(), etc.
    fn handle_os_call(&mut self, call: &CallExpression<'_>) {
        let method_name = match &call.callee {
            Expression::StaticMemberExpression(m) => {
                if !self.is_bound_to_os(&m.object) {
                    return;
                }
                m.property.name.as_str()
            }
            Expression::Identifier(id) => {
                if !self.is_os_method(id.name.as_str()) {
                    return;
                }
                id.name.as_str()
            }
            _ => return,
        };

        if !OS_PLATFORM_METHODS.contains(&method_name) {
            return;
        }

        self.record(
            &format!("os.{method_name}()"),
            call.span().start,
            Confidence::Medium,
            "any",
            false,
        );
    }
}

fn is_process_platform(expr: &Expression<'_>) -> bool {
    if let Expression::StaticMemberExpression(m) = expr {
        if m.property.name == "platform" {
            if let Expression::Identifier(id) = &m.object {
                return id.name == "process";
            }
        }
    }
    false
}

fn is_process_arch(expr: &Expression<'_>) -> bool {
    if let Expression::StaticMemberExpression(m) = expr {
        if m.property.name == "arch" {
            if let Expression::Identifier(id) = &m.object {
                return id.name == "process";
            }
        }
    }
    false
}

fn is_path_sep(expr: &Expression<'_>) -> bool {
    if let Expression::StaticMemberExpression(m) = expr {
        if m.property.name == "sep" {
            if let Expression::Identifier(id) = &m.object {
                return id.name == "path";
            }
        }
    }
    false
}

impl<'a> Visit<'a> for PlatformCollector<'a> {
    fn visit_binary_expression(&mut self, expr: &BinaryExpression<'a>) {
        self.handle_platform_check(expr);
        self.handle_arch_check(expr);
        self.handle_path_sep_check(expr);

        walk::walk_binary_expression(self, expr);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        self.handle_os_call(call);

        // Check string literal arguments for Unix paths and Windows commands
        for arg in &call.arguments {
            if let oxc_ast::ast::Argument::StringLiteral(lit) = arg {
                self.check_unix_path(lit.value.as_str(), lit.span().start);
                self.check_windows_command(lit.value.as_str(), lit.span().start);
            }
        }

        walk::walk_call_expression(self, call);
    }
}
