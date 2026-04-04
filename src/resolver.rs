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

/// Placeholder: full implementation in looper-82u.
pub fn resolve_bindings(_program: &oxc_ast::ast::Program<'_>) -> BindingMap {
    BindingMap::new()
}
