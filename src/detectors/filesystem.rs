use super::{Detector, DetectorContext};
use crate::resolver::BindingMap;
use crate::types::{Category, Confidence, Location, Metadata, ShadowDep};
use oxc_ast::ast::{Argument, CallExpression, Expression};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use serde_json::json;

const FS_MODULES: &[&str] = &["fs", "fs/promises", "node:fs", "node:fs/promises", "fs-extra", "graceful-fs"];
const PATH_MODULES: &[&str] = &["path", "node:path", "path/posix", "path/win32"];

const FS_READ_METHODS: &[&str] = &[
    "readFileSync", "readFile", "readdir", "readdirSync",
    "readlink", "readlinkSync", "realpath", "realpathSync",
];
const FS_WRITE_METHODS: &[&str] = &[
    "writeFileSync", "writeFile", "appendFileSync", "appendFile",
    "copyFile", "copyFileSync", "rename", "renameSync",
    "chmod", "chmodSync", "chown", "chownSync",
    "symlink", "symlinkSync", "link", "linkSync",
];
const FS_EXISTS_METHODS: &[&str] = &[
    "existsSync", "accessSync", "access", "statSync", "stat",
    "lstatSync", "lstat",
];
const FS_MKDIR_METHODS: &[&str] = &["mkdirSync", "mkdir", "mkdtempSync", "mkdtemp"];
const FS_OPEN_METHODS: &[&str] = &["openSync", "open", "createReadStream", "createWriteStream"];
const FS_DELETE_METHODS: &[&str] = &[
    "unlink", "unlinkSync", "rm", "rmSync", "rmdir", "rmdirSync",
];
const FS_WATCH_METHODS: &[&str] = &["watch", "watchFile", "unwatchFile"];

pub struct FilesystemDetector;

impl Detector for FilesystemDetector {
    fn name(&self) -> &str {
        "filesystem"
    }

    fn detect(&self, ctx: &DetectorContext<'_>) -> Vec<ShadowDep> {
        let mut collector = FsCollector {
            file_path: ctx.file_path,
            source_text: ctx.source_text,
            bindings: ctx.bindings,
            findings: Vec::new(),
        };
        collector.visit_program(ctx.program);
        collector.findings
    }
}

struct FsCollector<'a> {
    file_path: &'a str,
    source_text: &'a str,
    bindings: &'a BindingMap,
    findings: Vec<ShadowDep>,
}

impl FsCollector<'_> {
    fn line_col(&self, offset: u32) -> (u32, u32) {
        crate::types::line_col(self.source_text, offset)
    }

    fn record(&mut self, path_str: &str, offset: u32, confidence: Confidence, operation: &str) {
        let (line, col) = self.line_col(offset);
        let is_absolute = path_str.starts_with('/');
        let mut meta = Metadata::new();
        meta.insert("operation", json!(operation));
        meta.insert("is_absolute", json!(is_absolute));

        self.findings.push(
            ShadowDep::new(
                Category::Filesystem,
                path_str,
                confidence,
                Location::new(self.file_path, line, col),
            )
            .with_metadata(meta),
        );
    }

    /// Check if an expression resolves to one of the given module names via bindings.
    fn is_bound_to_any(&self, expr: &Expression<'_>, modules: &[&str]) -> bool {
        // Direct identifier: `fs`, `path`, etc.
        if let Expression::Identifier(id) = expr {
            if let Some(binding) = self.bindings.get(id.name.as_str()) {
                if binding.imported_name == "*" {
                    return modules.iter().any(|m| binding.module_source == *m);
                }
            }
            // Also match well-known global names without binding
            let name = id.name.as_str();
            return name == "Deno" || name == "Bun";
        }
        // fs.promises.readFile -> check fs.promises
        if let Expression::StaticMemberExpression(m) = expr {
            if m.property.name == "promises" {
                return self.is_bound_to_any(&m.object, modules);
            }
        }
        false
    }

    fn classify_fs_method(&self, method: &str) -> Option<&'static str> {
        if FS_READ_METHODS.contains(&method) {
            return Some("read");
        }
        if FS_WRITE_METHODS.contains(&method) {
            return Some("write");
        }
        if FS_EXISTS_METHODS.contains(&method) {
            return Some("exists");
        }
        if FS_MKDIR_METHODS.contains(&method) {
            return Some("mkdir");
        }
        if FS_OPEN_METHODS.contains(&method) {
            return Some("open");
        }
        if FS_DELETE_METHODS.contains(&method) {
            return Some("delete");
        }
        if FS_WATCH_METHODS.contains(&method) {
            return Some("watch");
        }
        None
    }

    fn handle_fs_call(&mut self, call: &CallExpression<'_>) {
        let (method_name, callee_obj) = match &call.callee {
            Expression::StaticMemberExpression(m) => {
                (m.property.name.as_str(), Some(&m.object))
            }
            Expression::Identifier(id) => (id.name.as_str(), None),
            _ => return,
        };

        let operation = match self.classify_fs_method(method_name) {
            Some(op) => op,
            None => return,
        };

        // Verify the callee is bound to an fs module
        let is_fs = match callee_obj {
            Some(obj) => self.is_bound_to_any(obj, FS_MODULES),
            None => {
                if let Some(binding) = self.bindings.get(method_name) {
                    FS_MODULES.iter().any(|m| binding.module_source == *m)
                } else {
                    false
                }
            }
        };
        if !is_fs {
            return;
        }

        if let Some(path_str) = first_string_arg(call) {
            self.record(&path_str, call.span().start, Confidence::High, operation);
        }
    }

    fn handle_path_call(&mut self, call: &CallExpression<'_>) {
        let method_name = match &call.callee {
            Expression::StaticMemberExpression(m) => m.property.name.as_str(),
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };

        if method_name != "join" && method_name != "resolve" {
            return;
        }

        let is_path = match &call.callee {
            Expression::StaticMemberExpression(m) => {
                self.is_bound_to_any(&m.object, PATH_MODULES)
            }
            Expression::Identifier(id) => {
                self.bindings
                    .get(id.name.as_str())
                    .is_some_and(|b| PATH_MODULES.iter().any(|m| b.module_source == *m))
            }
            _ => false,
        };
        if !is_path {
            return;
        }

        let parts: Vec<String> = call
            .arguments
            .iter()
            .map(|arg| match arg {
                Argument::StringLiteral(lit) => lit.value.to_string(),
                _ => "*".to_string(),
            })
            .collect();

        if parts.iter().all(|p| p == "*") {
            return;
        }

        let path_pattern = parts.join("/");
        let confidence = if parts.iter().any(|p| p == "*") {
            Confidence::Medium
        } else {
            Confidence::High
        };

        self.record(&path_pattern, call.span().start, confidence, "read");
    }

    fn handle_deno_fs(&mut self, call: &CallExpression<'_>) {
        // Deno.readTextFile('./path'), Deno.readFile, Deno.writeTextFile, etc.
        let Expression::StaticMemberExpression(m) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &m.object else {
            return;
        };
        if obj.name != "Deno" {
            return;
        }

        let method = m.property.name.as_str();
        let operation = match method {
            "readTextFile" | "readTextFileSync" | "readFile" | "readFileSync" | "readDir"
            | "readDirSync" => "read",
            "writeTextFile" | "writeTextFileSync" | "writeFile" | "writeFileSync" => "write",
            "stat" | "statSync" | "lstat" | "lstatSync" => "exists",
            "mkdir" | "mkdirSync" => "mkdir",
            "open" | "openSync" => "open",
            _ => return,
        };

        if let Some(path_str) = first_string_arg(call) {
            self.record(&path_str, call.span().start, Confidence::High, operation);
        }
    }

    fn handle_bun_file(&mut self, call: &CallExpression<'_>) {
        // Bun.file('./path')
        let Expression::StaticMemberExpression(m) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &m.object else {
            return;
        };
        if obj.name != "Bun" || m.property.name != "file" {
            return;
        }

        if let Some(path_str) = first_string_arg(call) {
            self.record(&path_str, call.span().start, Confidence::High, "read");
        }
    }
}

fn first_string_arg(call: &CallExpression<'_>) -> Option<String> {
    let arg = call.arguments.first()?;
    match arg {
        Argument::StringLiteral(lit) => Some(lit.value.to_string()),
        _ => None,
    }
}

impl<'a> Visit<'a> for FsCollector<'a> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        self.handle_fs_call(call);
        self.handle_path_call(call);
        self.handle_deno_fs(call);
        self.handle_bun_file(call);

        walk::walk_call_expression(self, call);
    }
}
