use oxc_ast::ast::{
    Argument, BindingPattern, CallExpression, Expression, ImportDeclarationSpecifier, Program,
    Statement, VariableDeclarator,
};
use std::collections::HashMap;

/// Describes what a local name binds to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleBinding {
    /// The module source (e.g. "fs", "child_process", "fs/promises").
    pub module_source: String,
    /// The imported name (e.g. "readFileSync", or "*" for namespace/default).
    pub imported_name: String,
}

/// Map from local variable name to its module binding.
/// Built by walking import/require statements at the top of a file.
pub type BindingMap = HashMap<String, ModuleBinding>;

/// Normalize `node:fs` -> `fs`, `node:child_process` -> `child_process`, etc.
fn normalize_module_source(source: &str) -> String {
    source.strip_prefix("node:").unwrap_or(source).to_string()
}

/// Build a BindingMap from top-level import/require statements in the AST.
pub fn resolve_bindings(program: &Program<'_>) -> BindingMap {
    let mut map = BindingMap::new();

    for stmt in &program.body {
        match stmt {
            Statement::ImportDeclaration(import) => {
                let source = normalize_module_source(import.source.value.as_str());

                if let Some(specifiers) = &import.specifiers {
                    for spec in specifiers {
                        match spec {
                            ImportDeclarationSpecifier::ImportSpecifier(s) => {
                                let local = s.local.name.as_str().to_string();
                                let imported = s.imported.name().as_str().to_string();
                                map.insert(
                                    local,
                                    ModuleBinding {
                                        module_source: source.clone(),
                                        imported_name: imported,
                                    },
                                );
                            }
                            ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                                let local = s.local.name.as_str().to_string();
                                map.insert(
                                    local,
                                    ModuleBinding {
                                        module_source: source.clone(),
                                        imported_name: "*".to_string(),
                                    },
                                );
                            }
                            ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                                let local = s.local.name.as_str().to_string();
                                map.insert(
                                    local,
                                    ModuleBinding {
                                        module_source: source.clone(),
                                        imported_name: "*".to_string(),
                                    },
                                );
                            }
                        }
                    }
                }
            }
            Statement::VariableDeclaration(var_decl) => {
                for declarator in &var_decl.declarations {
                    resolve_require(declarator, &mut map);
                }
            }
            _ => {}
        }
    }

    map
}

/// Extract bindings from `const x = require('mod')` and `const { a, b } = require('mod')`.
fn resolve_require(declarator: &VariableDeclarator<'_>, map: &mut BindingMap) {
    let Some(init) = &declarator.init else {
        return;
    };

    let source = match extract_require_source(init) {
        Some(s) => normalize_module_source(&s),
        None => return,
    };

    match &declarator.id {
        // const fs = require('fs')
        BindingPattern::BindingIdentifier(ident) => {
            let local = ident.name.as_str().to_string();
            map.insert(
                local,
                ModuleBinding {
                    module_source: source,
                    imported_name: "*".to_string(),
                },
            );
        }
        // const { readFile, writeFile } = require('fs')
        BindingPattern::ObjectPattern(obj) => {
            for prop in &obj.properties {
                let local = match prop.value.get_identifier_name() {
                    Some(name) => name.as_str().to_string(),
                    None => continue,
                };
                let imported = prop
                    .key
                    .name()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| local.clone());

                map.insert(
                    local,
                    ModuleBinding {
                        module_source: source.clone(),
                        imported_name: imported,
                    },
                );
            }
        }
        _ => {}
    }
}

/// If the expression is `require('literal')`, return the module source string.
fn extract_require_source(expr: &Expression<'_>) -> Option<String> {
    let call = match expr {
        Expression::CallExpression(call) => call,
        _ => return None,
    };

    if !is_require_call(call) {
        return None;
    }

    let arg = call.arguments.first()?;
    match arg {
        Argument::StringLiteral(lit) => Some(lit.value.as_str().to_string()),
        _ => None,
    }
}

/// Check if a call expression is `require(...)`.
fn is_require_call(call: &CallExpression<'_>) -> bool {
    matches!(&call.callee, Expression::Identifier(id) if id.name == "require")
}
